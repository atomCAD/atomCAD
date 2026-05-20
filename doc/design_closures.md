# Closures: Reusable Zone Bodies as First-Class Function Values

## Scope

This document designs **closures** — a generalization of the zones feature
(`doc/design_zones.md`) that re-introduces a *function value* into the
user-visible programming model, implemented entirely on top of the zone
evaluation substrate that already exists. The goal is to give the
functional-programming-minded user the generality they expect (pass a function
around, reuse one body across several call sites, eventually build combinators)
**without reviving the old `FunctionEvaluator` / temp-network machinery** and
without adding a second, parallel evaluation model. v1 ships the substrate plus
reuse, function factories, and single-value application; combinators are deferred
(see §"Applying a function value" and §"Out of phase plan").

Concretely, this branch adds:

1. A new **`closure` node** that owns a zone body (like the HOF nodes) but,
   instead of consuming it inline, exposes it as a `Function`-typed output pin.
   Its zone interface (number/types of zone-input and zone-output pins) is
   driven by UI type parameters, so it is generic.
2. An optional **`f: Function` input pin** on each HOF node (`map`, `filter`,
   `fold`, `foreach`). Wiring a `closure` output into this pin makes the HOF
   evaluate that closure instead of its own inline body.
3. A new **`apply` node** that calls a function value once on a single argument
   set (`f: Function` plus one pin per parameter → the function's return value).
   This is what makes a `Function` a *callable* value rather than only something
   an HOF iterates with — e.g. applying a closure produced by a function-factory
   subnetwork to a single value (see §"Applying a function value: the `apply`
   node"). Combinators (`compose`/`flip`) are a natural extension but are
   **deferred** — they need capabilities v1 lacks; see that section.

The load-bearing insight is that a function value in the zone world **already
exists in the code** — it is exactly the bundle a `Walker::MapZone` carries
(`iterator_walker.rs:67`): `{ body, captures, zone_output_wires, hof_node_id }`.
A `closure` node is that bundle detached from its consumer and handed around as
a value; an HOF with an `f` pin is the consumer detached from the body. The two
together are just the *unfused* form of an inline HOF, evaluated by the same
substrate.

In scope:
- Conceptual model: a function value as a detached zone closure.
- Data-model changes (`ZoneClosure`, the repurposed `NetworkResult::Function`).
- Evaluator changes — what reuses today's zone machinery and what's new.
- The `closure` node type, the `f` input pin on the four HOFs, and the `apply`
  node (single-value function application — what makes `Function` callable).
- Capture-freeze timing semantics (the one genuinely new mental-model item).
- Dead weight finally removed (`FunctionEvaluator`, the `-1` pin convention).
- The (small) Flutter editor work — folded in here rather than a separate doc,
  because zone-body rendering is already generic so almost nothing UI-specific
  is needed. See §"Editor (Flutter) changes".
- Implementation phases: Rust phases each ending in `cargo test` green, plus a
  final editor phase ending in `flutter run` working.

Out of scope:
- `.cnnd` migration from main's function-pin/closure world. This branch breaks
  compatibility freely; migration is a later doc once both Rust and UI land.
- **Implicit auto-wrap**: letting the user wire an arbitrary computation node's
  function pin straight into an HOF's `f` pin (synthesizing a closure body in
  the background). Buildable but a UX decision; see Open Questions.
- Text-format syntax for closures. Deferred until the data model stabilizes.
- Multiple zone-output pins on a single closure (multi-result functions).
  v1 closures have exactly one zone-output pin; see Open Questions.

## Scope of this branch — build/test contract

The Rust phases (1–5) are gated on `cd rust && cargo test` green and
`cargo clippy` clean. The final editor phase (6) is additionally gated on
`flutter run` launching a working editor. Flutter consumer code may go red
during the Rust phases (the API surface changes — the `closure` node, the `f`
parameter, reviving `DataType::Function` exposure) and is brought back online in
Phase 6.

| Must pass | When |
|---|---|
| `rust/` workspace (`cargo build`, `cargo clippy`, `cargo test`) | every phase |
| `flutter_rust_bridge_codegen generate` succeeds | every phase |
| Rust integration/roundtrip tests under `rust/tests/` | every phase |
| `flutter run` launches; closure authoring works end-to-end | Phase 6 |
| Existing non-closure editing still works | Phase 6 (regression) |

Insta snapshot tests will fail when serialization shapes change; accept the new
snapshots with `cargo insta review` as part of the relevant phase.

The branch starts after zones Rust phase 6 has landed. Baseline: the full Rust
suite (3000+ tests) green, the four HOFs evaluating via inline zone bodies,
`FunctionEvaluator` / `Closure` / `DataType::Function` present but **dead**
(no node produces or consumes a `Closure` value after the zones rewrite).

### Relationship to the zones dead-weight-cleanup plan

`design_zones.md` (§"Out of phase plan") scheduled the eventual deletion of
`FunctionEvaluator`, `Closure`, `DataType::Function`, the
`output_pin_index == -1` branch, and Flutter function-pin rendering. **This
design supersedes part of that plan:**

- `FunctionEvaluator` and the `output_pin_index == -1` branch are **deleted**
  (this design makes them unnecessary, as planned).
- `DataType::Function` and `NetworkResult::Function` are **kept and
  repurposed** — they become the carrier of the new `ZoneClosure`. The old
  `Closure` struct's *fields* are replaced; the variant survives.
- Flutter function-pin rendering is **kept and repurposed** for real
  `Function`-typed pins (the `closure` output, the HOF `f` inputs) rather than
  the legacy `-1` convention. Detailed in §"Editor (Flutter) changes".

## Motivation

Inline zones (the existing feature) made authoring `map`/`filter`/`fold`/
`foreach` bodies dramatically more pleasant: draw the body inside the node, drag
capture wires across the boundary. For the common case this is the right
ergonomics and stays the default.

But inline zones fuse the body to a single call site. They can't express:

- **Reuse.** One body applied by three different HOFs requires drawing (and
  maintaining) the same body three times.
- **Function factories.** A subnetwork can't compute and hand back "a function"
  configured by its inputs, because there is no function value to return.
- **Single-value application.** There is no way to call a per-element computation
  *once*, on one value, outside an iteration.

These are the things a function value buys you, and v1 delivers them: this design
surfaces the value the zones substrate already carries internally as a `closure`
node + a `Function` pin, and makes it *callable* with an `apply` node, reusing the
substrate wholesale. Reuse comes from passing the value into an HOF's `f` pin;
factories come from returning a `closure` as a subnetwork's `Function` output;
single-value application comes from `apply` (see §"Applying a function value: the
`apply` node").

Two further uses — **abstraction over behavior** (a subnetwork that takes a
function *parameter* and applies it) and **combinators** (`compose`/`flip`) — are
the natural continuation but are **deferred**: they need `Function`-typed
parameter authoring and/or configurable arity, neither of which v1 ships. See
§"Applying a function value" and §"Out of phase plan".

## Concept

### A function value is a detached zone closure

Recall the zone substrate (`design_zones.md`, and confirmed in code):

- An HOF owns a body `NodeNetwork` on `Node.zone: Option<Arc<NodeNetwork>>`.
- At HOF `eval`, the body's **captures** (wires with `source_scope_depth > 0`)
  are pre-evaluated **once** into an `Arc<HashMap<CaptureKey, NetworkResult>>`
  and frozen (`map.rs:218` `build_captures`).
- Per element, the consumer pushes an iteration frame onto
  `current_zone_input_values[hof_id]`, evaluates the **zone-output wire(s)**,
  and pops (`iterator_walker.rs:329`, `fold.rs:163`).

The bundle that fully describes "the per-element computation, ready to run" is:

```rust
pub struct ZoneClosure {
    /// The body network. CoW-shared, cheap to clone (Arc bump).
    pub body: Arc<NodeNetwork>,
    /// Captured environment, pre-evaluated and frozen at definition time.
    pub captures: Arc<HashMap<CaptureKey, NetworkResult>>,
    /// One wire per zone-output pin, delivering the body's result(s).
    pub zone_output_wires: Arc<Vec<IncomingWire>>,
    /// Scope-stack key for iteration frames: the id of the node that *owns*
    /// the body (the HOF node for an inline body, the `closure` node for a
    /// closure value). Determines which `current_zone_input_values` entry the
    /// consumer pushes frames onto. This key is **not unique** across networks;
    /// see §"`owner_node_id`: the model's one conceptual debt" for why that is
    /// nonetheless safe.
    pub owner_node_id: u64,
    /// Arity/types, mirrored from the owner's zone pins. Carried so a consumer
    /// can sanity-check shape and so the value's `DataType::Function` can be
    /// inferred. (Zone-input pin types = parameter types; the single
    /// zone-output pin type = the function's return type.)
    pub param_types: Vec<DataType>,
    pub return_type: DataType,
}
```

This is **identical** to the four flattened fields `Walker::MapZone` carries
today (`source` aside), plus the type metadata needed to make it a typed value.
It is *not* the old `Closure { node_network_name, node_id,
captured_argument_values }` — that referenced a registry node and required
`FunctionEvaluator` to rebuild a temp network. The new closure carries the real
body and the real frozen environment; nothing is rebuilt.

### The three forms are one substrate

| Form | Body source | Captures frozen at | Consumed by |
|---|---|---|---|
| Inline HOF (today) | the HOF's own `node.zone` | the HOF's `eval` | the HOF itself |
| `closure` node (new) | the closure's own `node.zone` | the `closure`'s `eval` | wherever the value flows |
| HOF with `f` pin (new) | the wired-in `ZoneClosure`'s body | (already frozen by the producer) | the HOF |

All three feed the **same** `Walker::MapZone`/`FilterZone` (lazy) or eager drain
loop (`fold`/`foreach`). The only differences are *where the body comes from*
and *when the captures were frozen*. A function value is also consumed *outside*
any iteration — by the `apply` node, the single-element degenerate of the eager
loop — which is what makes it a callable value and not merely HOF fuel (next).

### Applying a function value: the `apply` node

The four HOFs consume a function by running it *across a stream*. The minimal
consumer runs it *once*, on a single argument set — and that is the operation
that makes a `Function` a genuinely callable value rather than only fuel for
iteration. That node is `apply`:

```text
apply(f: Function, a0: P0, a1: P1, …) -> R
```

where `(P0, P1, …) -> R` is `f`'s function type. `apply` owns no body and has no
inline zone; it always reads its function from the `f` pin. Its `eval` is
exactly **one step of the eager drain loop with the iterator removed**: obtain
the `ZoneClosure` from `f`, swap in its frozen captures, push one frame carrying
`(a0, a1, …)` onto `current_zone_input_values[closure.owner_node_id]`, resolve
`closure.zone_output_wires`, pop. It reuses the same per-step resolver the eager
HOFs use (`run_closure_once`, factored out in Phase 1) — no new evaluation
machinery.

The v1 payoff is **single-value application**. The only `Function` values v1 can
construct are the four HOF-shaped closure kinds, and the most useful place an
`apply` consumes one is the output of a **function-factory** subnetwork — e.g. a
`(k: Int) -> Function` network whose return is a `closure` capturing `k` and
adding it. `apply(make_adder(5), 10)` then yields `15`. This crosses a network
boundary (the closure is built inside the subnetwork, returned as a `Function`
output, and called in the parent) and needs no `Function`-typed *input* pin.
Without `apply`, "call this function on this single value" had to be faked by
wrapping the value in a one-element iterator and `map`/`collect`-ing it back out;
`apply` removes that detour.

**Combinators are deferred, not delivered.** Building a function from other
functions — `compose (f, g)`, `flip (f)` — is the natural next step, and `apply`
is the substrate it would stand on (a closure body calling a captured function on
its own `ZoneInput` parameter). Two prerequisites are missing in v1, so this doc
records the shape but does not ship it:

- `compose`/`flip` are subnetworks that take `Function`-typed **parameters**,
  which v1 cannot author — there is no `Function` entry in the type picker /
  `APIDataTypeBase`, and §"Editor (Flutter) changes" deliberately adds only
  *derived* `Function` pins (the `closure` output, the HOF/`apply` `f` inputs).
- `flip f` for a binary `f: (A, T) -> A` returns `(T, A) -> A`, whose return type
  differs from its first parameter — a shape **no v1 closure kind can express**
  (the only binary kind is `(A, T) -> A`). It needs the configurable arity of
  Open Question 3.

Both land together with those prerequisites in a later increment (tracked in
§"Out of phase plan"). `Function`-typed values *can* already be captured (only
`Iter[T]` is barred — §"Type system"), so the substrate is ready when they do.

**Shape (v1).** Like the `closure` node, `apply` picks its arity/types from a
fixed set of "kinds" (`(T)->U`, `(T)->Bool`, `(A,T)->A`, `(T)->Unit`), which
drives both its argument pins and its `f` pin's `Function` type. Deriving the
argument pins automatically from the wired `f` source's declared function type is
the natural ergonomic follow-up (parallel to Open Question 3); deferred so v1
needs no "pin count depends on connected input" mechanism.

**Effectful application is free.** When `f` returns `Unit` (a `(T)->Unit`
function), `apply`'s single output pin resolves to `Unit`, so the existing
central skip rule (`evaluator/AGENTS.md`) already gates it to Execute passes —
applying an effectful function is itself an effect, with no apply-specific code.

## Data model

### `ZoneClosure` and the repurposed `NetworkResult::Function`

`NetworkResult::Function(Closure)` (`network_result.rs:255`) is repurposed to
carry a `ZoneClosure`:

```rust
// network_result.rs — the variant survives; its payload changes.
Function(ZoneClosure),
```

The old `Closure` struct (`network_result.rs:216`) is **deleted**. `ZoneClosure`
(shown above) replaces it. All fields are `Arc`-backed or plain `Copy`/small, so
`Clone` is cheap (Invariant 2 for `Walker` continues to hold — cloning a walker
that embeds a `ZoneClosure` is refcount bumps only).

### `Walker::MapZone` / `FilterZone` carry a `ZoneClosure`

The four flattened fields collapse into one:

```rust
// iterator_walker.rs
MapZone   { source: Box<Walker>, closure: ZoneClosure },
FilterZone{ source: Box<Walker>, closure: ZoneClosure },
```

`next()` does exactly what it does today, reading `closure.body`,
`closure.captures`, `closure.zone_output_wires`, `closure.owner_node_id` instead
of the loose fields. No behavioral change.

The legacy FE-driven `Walker::Map` / `Walker::Filter` variants
(`iterator_walker.rs:79,89`) are **deleted** — they are already dead weight kept
only until `FunctionEvaluator` retires, which happens here.

### `NodeType` zone pins — already sufficient

The `closure` node is zone-bearing: it declares `zone_input_pins`
(parameters, inside-left) and `zone_output_pins` (results, inside-right) just
like the HOFs (`node_type.rs`, `MapData::calculate_custom_node_type` at
`map.rs:35`). Because `NodeType::has_zone()` (`node_type.rs:255`) is true for
it, `ensure_zone_init` (`node_network.rs:546`) automatically gives a freshly
added `closure` node an empty body and the right number of
`zone_output_arguments`. **No new lifecycle code** — the zones machinery already
handles owned-body creation, CoW cloning, copy/paste, undo, and recursive
walking (`walk_all_nodes`) for any zone-bearing node.

### Body rendering is free (no per-node UI work)

Zone-body rendering is **generic**, driven entirely by the presence of a zone
plus the node type's declared zone pins — it is *not* implemented per HOF. The
`closure` node, being zone-bearing, therefore renders and edits its body with
the existing code, with zero rendering work specific to it:

- **Rust API** (`structure_designer_api.rs::build_zone_view`): gated on
  `node_type.has_zone()` and populated from `node_type.zone_input_pins` /
  `zone_output_pins`. No node-name special-casing — any `has_zone()` type emits
  a `ZoneView`.
- **Flutter** (`node_widget.dart`): the body region, inner zone pins, recursive
  body-node rendering, the `ZoneBodyLayer` wire painter, the resize handle, the
  collapsed placeholder, hit-testing, and per-body selection are all keyed off
  `node.zone != null` and iterate `ZoneView.zoneInputPins` / `zoneOutputPins`.

This is why the editor work for closures is small (see §Scope "Out of scope"):
the body *renders and authors for free*; only the `f`-pin/inline-zone toggle,
the closure-shape property editor, and function-wire plumbing are new.

### The `f` input pin on HOFs

Each HOF gains one optional external input pin, `f`, of type
`DataType::Function(FunctionType { parameter_types, output_type })`. The
function type is derived in `calculate_custom_node_type` from the HOF's existing
type properties:

| HOF | `f` pin type |
|---|---|
| `map` | `(input_type) -> output_type` |
| `filter` | `(element_type) -> Bool` |
| `fold` | `(accumulator_type, element_type) -> accumulator_type` |
| `foreach` | `(element_type) -> Unit` |

`f` is stored as an ordinary `Argument` with an ordinary `IncomingWire`
(`SourcePin::NodeOutput`, `source_scope_depth = 0`, `ArgumentKind::External`).
**No new wire shapes** — a closure flowing into `f` is just a normal value wire
carrying a `Function` value. Serialization, undo, and copy/paste need no new
cases. The `apply` node carries the same `f` pin — here *required*, since it has
no inline body to fall back on — plus one ordinary `External` input pin per
function parameter; still no new wire shapes.

## Evaluator changes

### Reuse from today's zone machinery (unchanged)

| Existing piece | Reused for closures? |
|---|---|
| `resolve_incoming_wire` (`network_evaluator.rs:1228`) | Yes — capture-cache → `NodeOutput` → `ZoneInput`, untouched |
| `build_captures` (`map.rs:218`, `fold.rs:261`) | Yes — closure `eval` calls it identically |
| `CaptureKey` / `captured_source_values` / `CapturesGuard` | Yes, unchanged |
| `current_zone_input_values` scope-stack + helpers | Yes, unchanged |
| `eval_step` (`iterator_walker.rs:555`) — body-only stack resolve | Yes — the single per-step resolver |
| `Walker::MapZone`/`FilterZone::next` discipline | Yes — push/pop frame + captures swap |
| Per-node `eval` implementations | Yes — fully unchanged |

### What's new (small and contained)

**1. One shared accessor: `obtain_closure`.** A free function (or method on
`NetworkEvaluator`) that yields the `ZoneClosure` an HOF should run:

```rust
fn obtain_closure(
    evaluator, network_stack, node_id, registry, context,
    f_param_index: usize,   // index of the `f` pin on this HOF
) -> Result<ZoneClosure, NetworkResult /* Error */> {
    // If `f` is wired, evaluate it and take the carried closure.
    match evaluator.evaluate_arg(network_stack, node_id, registry, context, f_param_index) {
        NetworkResult::Function(zc) => return Ok(zc),
        e @ NetworkResult::Error(_) => return Err(e),
        NetworkResult::None => { /* not connected — fall through to inline */ }
        _ => return Err(NetworkResult::Error("f is not a function".into())),
    }
    // Otherwise build a closure from this node's own inline zone — exactly
    // steps (b) + (c) of today's map.eval: grab body, pre-evaluate captures.
    build_inline_closure(evaluator, network_stack, node_id, registry, context)
}
```

`build_inline_closure` is the existing inline-body logic (`map.rs:86`–`128`)
factored out so both the inline path and the `closure` node share it.

**2. The four HOFs call `obtain_closure`, then proceed exactly as today.**
`map.eval` becomes:

```rust
let xs_walker = /* resolve xs as today */;
let closure = match obtain_closure(.., f_index, ..) { Ok(c) => c, Err(e) => return EvalOutput::single(e) };
EvalOutput::single(NetworkResult::Iterator(Walker::map_zone(xs_walker, closure)))
```

`filter` is the same with `Walker::filter_zone`. `fold`/`foreach` resolve their
external inputs, obtain the closure, then run the existing eager drain loop —
with one unification (next item).

**3. Eager path switches to carried-wires resolution.** Today `fold`/`foreach`
read zone-output wires off the HOF node via `evaluate_zone_output`
(`network_evaluator.rs:1370`), which reaches `network_stack[len-2]` for the HOF.
For a closure-from-`f`, the zone-output wires live on the *closure* node, not
the HOF. So the eager loop switches to the same carried-wires resolver the lazy
walker already uses (`eval_step` style): push the closure's body, push the
iteration frame on `closure.owner_node_id`, resolve `closure.zone_output_wires`,
pop. This makes inline and closure cases uniform and lets `evaluate_zone_output`
be deleted along with the rest of the dead weight (its only callers were the
eager HOFs).

> ⚠️ **"Same resolver as the lazy walker" does *not* mean "body-only stack."**
> `run_closure_once(evaluator, network_stack, …)` pushes the closure body onto
> a **base `network_stack` that the caller supplies**, then resolves the
> zone-output wire against `network_stack + body`. The lazy walkers
> (`MapZone`/`FilterZone::next`) have no access to the outer network stack, so
> they pass `&[]` (body-only) — sound for them because their bodies' deep
> captures were pre-frozen at the producing HOF's `eval`. The **eager** path is
> different: `fold`/`foreach` *do* hold their real `network_stack` and **must
> pass it**, because a *nested* HOF inside the body freezes its own captures at
> its `eval`, which runs *during* the outer drain — and a capture reaching past
> the immediate body (e.g. a grandparent constant at `source_scope_depth ≥ 2`)
> can only resolve if the full ancestor stack is present at that moment. Passing
> a body-only stack here truncates the ancestors and the nested HOF's deep
> capture fails to resolve (regression `nested_fold_inner_captures_outer_constant`,
> "expected Int(36), got Error"). **The same applies to every future eager
> consumer — the `apply` node and the `f`-driven HOFs must pass the consuming
> node's real `network_stack` to `run_closure_once`.**

**4. The `closure` node's `eval`** is the first half of an HOF eval, wrapped as a
value:

```rust
fn eval(..) -> EvalOutput {
    let closure = match build_inline_closure(.., node_id, ..) {
        Ok(c) => c,
        Err(e) => return EvalOutput::single(e),
    };
    EvalOutput::single(NetworkResult::Function(closure))
}
```

That's the entire new evaluation logic. Everything downstream — the walker, the
scope-stack, the capture cache, `eval_step` — is reused verbatim.

**5. The `apply` node's `eval`** consumes a function value on a single argument
set — the dual of the `closure` node (which *produces* a function value), and
the degenerate one-element, no-iterator case of an eager HOF. It requires `f`
and so never falls back to an inline body:

```rust
fn eval(..) -> EvalOutput {
    let closure = match evaluator.evaluate_arg(network_stack, node_id, registry, context, f_index) {
        NetworkResult::Function(zc) => zc,
        NetworkResult::None         => return EvalOutput::single(NetworkResult::Error("apply: f not connected".into())),
        e @ NetworkResult::Error(_) => return EvalOutput::single(e),
        _                           => return EvalOutput::single(NetworkResult::Error("apply: f is not a function".into())),
    };
    // Resolve the argument pins, then run one body step via `run_closure_once`:
    // push the body, swap in `closure.captures`, push one frame on
    // `closure.owner_node_id`, resolve `closure.zone_output_wires`, pop.
    EvalOutput::single(run_closure_once(.., &closure, args))
}
```

`run_closure_once` is the eager loop's per-element step, factored out in Phase 1
and shared by `fold`/`foreach`, `apply`, and the lazy walker. `apply` is that
loop body without the loop.

### Capture-freeze timing — the one new semantic

This is the only genuinely new thing to reason about, and it is worth stating
precisely because it differs (correctly) from inline zones.

- **Inline zone:** captures are frozen when the *consuming HOF* runs `eval`
  (body entry). If that HOF sits inside an outer `fold` body, its captures are
  re-frozen once per outer iteration.
- **`closure` node:** captures are frozen when the *`closure` node* runs `eval`
  (its definition site). The resulting value carries that frozen environment to
  wherever it is consumed, however many times.

This is standard "capture at definition site" closure semantics, and the
existing nested-HOF machinery already produces the right behavior in both
directions:

- A `closure` node placed **outside** a `fold` is evaluated once; its captures
  freeze once; all `fold` iterations share them. (Reuse without recomputation —
  the whole point.)
- A `closure` node placed **inside** a `fold` body is evaluated once per outer
  iteration (it lives in the body), so each iteration produces a fresh closure
  whose captures snapshot that iteration's values — including `ZoneInput`
  captures of the enclosing `fold`'s `acc`/`element`, which `build_captures`
  already resolves via the live `current_zone_input` lookup (`map.rs:288`).

Worked example (mirrors `design_zones.md` §"Worked example"): outer `fold` over
`[1,2,3]`; a `closure` defined **inside** the body capturing a parent constant
`K` and the fold's `acc`; the closure is consumed by an inner `map`. The closure
node's `eval` runs three times (once per outer iteration). Each run freezes `K`
once and the *current* `acc` once. Correct, and identical to what an equivalent
inline inner `map` would do.

### `owner_node_id`: the model's one conceptual debt

This is the part of the model that is *not* a clean closure, and it deserves to
be named rather than tucked away. A function value does not bind its parameters
structurally (by position, into the body's `ZoneInput` pins). It binds them
indirectly: it carries `owner_node_id`, an opaque numeric key, and the consumer
passes arguments by pushing a frame onto a process-global mutable side-table,
`current_zone_input_values[owner_node_id]`, which the body's `ZoneInput`
resolution then reads back. Parameter passing is therefore *by key into mutable
global state* — the one place the design trades conceptual cleanliness for
wholesale substrate reuse.

The hazard is that `owner_node_id` is **not unique**, and closures make that
worse than inline zones did:

- For **inline zones** the collision is *incidental*: body and consumer share
  one network's id space (`next_node_id` restarts at 1 per body), so a clash
  only arises through nesting, and only sometimes.
- For **closures** the collision is *structural*: the body comes from one
  network's id space and the consumer from another, both starting at 1, so an
  `owner_node_id` coinciding with some id live at the consumption site is the
  normal case, not an edge case.

It is nonetheless **safe** — but *not* for the natural-sounding reason that "the
drain loop is strictly nested," which is in fact **false**: the lazy consumers
are not a nested drain loop. `Walker::MapZone`/`FilterZone::next` push one frame,
resolve a *single* element, and pop (`iterator_walker.rs`); they do not fully
drain an inner walker per frame, and a resolved element can itself be a lazy
`Iterator` that escapes the frame entirely. Safety rests instead on two
properties the substrate already guarantees:

1. **Enclosing reads are frozen, never live.** The only frame a body reads
   *live* is the one for its immediately-enclosing scope, keyed by its own
   `owner_node_id`. Every reference to a *more distant* ancestor's `ZoneInput`
   sits at `source_scope_depth ≥ 2` and is therefore a capture (`is_capture`,
   `map.rs`), frozen into `ZoneClosure.captures` at the producing HOF's `eval`
   and read from there. So no consumer ever reaches *past* the top of a
   colliding id's stack into a lower frame.
2. **Each live read is bracketed by its own push/pop.** A consumer pushes its
   one frame, resolves the zone-output wire(s) to a concrete value, and pops —
   all synchronously within a single `next()` / `run_closure_once`. The frame
   never survives a yield, and an escaping lazy `Iterator` carries no dependence
   on the popped frame, because everything it needed from an enclosing scope was
   already frozen by (1).

Given those, the scope-**stack** (`Vec<Vec<NetworkResult>>` per id; reads consult
`last()`) trivially handles the remaining case — a colliding inner consumer
running to completion *nested* inside an outer frame. Pushes and pops stay
balanced, `last()` always returns the innermost active invocation's frame, and
the outer frame is restored on pop; and because the upstream element/arguments
are resolved *before* the consumer pushes (`source.next()` precedes
`push_zone_input_frame` in `MapZone::next`), a same-id source never even
transiently overlaps the consumer's frame. The correctness argument is thus
"enclosing reads are frozen, and each live frame is read only within its own
synchronous push/pop" — not "ids happen not to clash" and not "the drain
discipline is nested."

**This is a first-class correctness requirement, not a nicety.** The forced-
collision regression test must exercise the *genuinely subtle* shape, not merely
an eagerly-nested `fold`: a **lazy** consumer (`map`/`filter`) whose closure body
returns an `Iterator` that **escapes** the producing step and is drained only
*after* that step has popped its frame. Construct an outer HOF and a consumed
closure whose `owner_node_id` is deliberately made equal to the outer HOF's id,
have the inner closure emit such an escaping stream, and assert correct results.
The test is load-bearing precisely because the bug it guards is silent: were
property (1) above to regress — an enclosing read resolved from a live frame
instead of from frozen captures — the colliding inner frame would be read in
place of the outer one, with no error. (An eagerly-nested `fold` variant is worth
keeping too, but it is the *easy* case: its drains genuinely are nested, so it
would pass even under the weaker, incorrect justification.) Treat this test as
gating Phase 4, as `design_zones.md` Phase 5 mandated the analogous test for
inline nested HOFs.

**The clean future direction**, recorded here so the debt is explicit: bind a
closure's parameters *positionally* at the call site instead of through a global
id-keyed stack — the consumer hands the argument frame straight to the body
evaluation, and `ZoneInput { pin_index }` resolves against the current
invocation's frame without consulting a node-id key at all. That removes
`owner_node_id` and the non-uniqueness with it. The cost is reworking how
captures of *enclosing* zones' `ZoneInput`s are resolved — today that path
consults the id-keyed slot (via `source_scope_depth` + `owner_node_id`) to pick
the right enclosing frame. Out of scope for v1; the scope-stack plus the
forced-collision test is the sanctioned interim, tracked in §"Out of phase
plan".

## Type system

`DataType::Function(FunctionType { parameter_types, output_type })` already
exists (`data_type.rs:8,49`) and is revived rather than deleted.

- **`closure` output pin type:** `Function((zone-input pin types) ->
  zone-output pin type)`, computed in `calculate_custom_node_type`.
- **HOF `f` pin types:** the table in §"The `f` input pin on HOFs".
- **`apply` pin types:** for the selected kind `(P0, …) -> R`, the `f` pin is
  `Function((P0, …) -> R)`, the argument pins are `P0, …`, and the single output
  pin is `R` — computed in `calculate_custom_node_type` from the kind, the same
  way the `closure` node computes its interface.
- **Compatibility:** the `can_be_converted_to` Function arm
  (`data_type.rs:383`) is **simplified** from the partial-application "prefix"
  rule to a structural match: same arity, each parameter and the return type
  pairwise convertible (keeping the usual leaf conversions like `Int → Float`).
  Partial application is now expressed by *captures*, not by the type rule, so
  the prefix rule has no remaining consumer.
- **Existing guards stay:** `Function` cannot be an array element, record field,
  or `Iter[T]` element (already enforced at `array_at.rs:156`,
  `parameter.rs:199`, `sequence.rs:138`, etc.). `Iter[T]` still cannot be
  captured into a closure (walker aliasing). No new guards needed; we simply
  stop treating `Function` as universally rejected.

## Validation

Three checks, layered onto the existing zone validation
(`network_validator.rs::validate_zones_recursive`):

1. **`f` xor inline body.** When an HOF's `f` pin is connected, its inline zone
   is ignored at eval time, so the existing rule "every zone-output pin has an
   incoming wire" is **suspended** for that HOF (an empty inline body is fine
   when `f` drives it). When `f` is *not* connected, the existing rule applies
   unchanged.
2. **Closure body is complete.** A `closure` node is validated by the existing
   zone-body rules: every zone-output pin needs ≥ 1 incoming wire, captures
   resolve, `ZoneInput` references are valid. A closure that doesn't deliver its
   result is invalid (attributed to the `closure` node).
3. **`f` source is function-typed and shape-compatible.** Falls out of the
   normal wire type-compatibility check against the `f` pin's declared
   `Function` type — no special-case code, just the revived `Function` arm of
   `can_be_converted_to`.
4. **`apply` requires `f`.** Unlike an HOF (whose disconnected `f` falls back to
   the inline body), `apply` has no body, so a disconnected `f` is a validation
   error attributed to the `apply` node. The `f`-source type/shape check is the
   same revived `Function` arm of `can_be_converted_to` as (3).

## Editor (Flutter) changes

The editor work is **small** because zone-body rendering is already generic
(§"Body rendering is free") — the `closure` node's body region, inner zone pins,
recursive body nodes/wires, resize handle, hit-testing, per-body selection,
nested rendering, and zoom collapse are all inherited unchanged from
`design_zones_ui.md`. Only four narrow pieces are new, and they land together in
Phase 6:

1. **Inline-zone ⇄ `f`-pin toggle.** When an HOF's `f` pin is connected, the
   editor hides (or visibly disables) that HOF's own inline body so there is one
   obvious source of truth; when `f` is disconnected, the inline body returns.
   This mirrors the existing "input pin overrides stored data" affordance used
   by `atom_replace.rules` and `collect.limit`.
2. **Closure/`apply` shape property editor.** A small `node_data_widget` for the
   `closure` node — reused unchanged for the `apply` node — directly analogous to
   `map`'s existing `input_type`/`output_type` editor, with a *kind* selector in
   front of the type pickers.

   A **kind** is a shape *template*: it fixes the arity and decides, per pin,
   whether the type is **free** (user picks a `DataType` via the existing
   `DataTypeInput`) or **fixed/derived** (the system supplies it). The four v1
   kinds are exactly the four HOF shapes, so a closure of a given kind drops into
   the matching HOF's `f` pin by construction:

   | Kind | arity | free slots (user picks) | result pin |
   |---|---|---|---|
   | `(T) -> U` *(map-like)* | 1 | `T`, `U` | free `U` |
   | `(T) -> Bool` *(filter-like)* | 1 | `T` | fixed `Bool` |
   | `(A, T) -> A` *(fold-like)* | 2 | `A`, `T` | derived `= A` |
   | `(T) -> Unit` *(foreach-like)* | 1 | `T` | fixed `Unit` |

   The editor is a `DropdownButton<ClosureKind>` (signature glyphs) above 1–2
   `DataTypeInput` rows that appear by kind; the result line is a `DataTypeInput`
   only for `(T)->U` and a read-only label otherwise. `onChanged` calls
   `model.setClosureData(nodeId, …)` / `setApplyData`, the
   `setMapData`/`setFoldData` pattern.

   **One widget, two expansions.** Both nodes store the same data —
   `{ kind, type_args: Vec<DataType> }` (1 or 2 args by kind) — and differ only
   in `calculate_custom_node_type`: the `closure` node expands the kind *inward*
   (zone-input pins for the params, one zone-output pin, and a `Function((params)
   -> result)` *output* pin), while `apply` expands it *outward* (a `Function(…)`
   *input* pin `f`, one ordinary arg pin per param, and a value output). The kind
   is the single source of truth, which is why one editor serves both.

   **Changing a kind** is a structural pin change and routes through existing
   repair: `repair_zone_body` for the `closure` node (the same path as flipping
   `map.input_type`), ordinary `repair_node_network` wire-retention for `apply`.
   Captures are *not* part of the shape — they are ordinary capture wires drawn
   into the body — so the shape editor only ever describes parameters + result.
3. **Add Node popup.** Add the `closure` and `apply` nodes to the type list and
   ensure the four HOFs remain addable. No popup mechanics change — only the
   list.
4. **Function wiring + FRB regen.** Real `Function` pins are ordinary typed
   pins, so dragging a closure output into an `f` input flows through the
   existing pin-to-pin path; the compatibility check already lives in Rust's
   `can_be_converted_to`. FRB regen surfaces the new API shapes (the `closure`
   node, the `f` parameter, the `Function` data type). Distinct
   function-typed-wire styling is optional polish.

## Reuse map (summary)

**Reused unchanged:**
- The entire capture pipeline: `build_captures`, `CaptureKey`,
  `captured_source_values`, `CapturesGuard`.
- The scope-stack: `current_zone_input_values` + `push/pop/current/write` helpers.
- `eval_step` and the per-step push/pop + captures-swap discipline.
- `resolve_incoming_wire` and its three arms.
- Owned-body lifecycle: `ensure_zone_init`, `zone_mut`, copy/paste, undo,
  `walk_all_nodes` recursion.
- Every per-node `eval`.

**Reused with small changes:**
- `NetworkResult::Function` — payload changes from `Closure` to `ZoneClosure`.
- `DataType::Function` — kept; `can_be_converted_to` arm simplified to structural.
- `Walker::MapZone`/`FilterZone` — carry a `ZoneClosure` instead of four loose fields.
- `map`/`filter`/`fold`/`foreach` — gain an `f` pin and call `obtain_closure`;
  eager HOFs switch to carried-wires resolution.

**New from scratch:**
- `ZoneClosure` struct.
- `obtain_closure` / `build_inline_closure` shared accessors, plus
  `run_closure_once` (the eager loop's per-step body — push body, swap captures,
  push frame, resolve zone-output wires, pop — factored out in Phase 1 and shared
  by `fold`/`foreach`, `apply`, and the lazy walker).
- The `closure` node type (`nodes/closure.rs`).
- The `apply` node type (`nodes/apply.rs`) — the minimal function-value consumer
  that makes `Function` callable.
- A small amount of editor glue (Phase 6): the inline-zone/`f` toggle, the shared
  closure/`apply` shape property editor, the Add Node popup entries. Body
  rendering is reused, not new.

**Deleted (net simplification):**
- `FunctionEvaluator` (`function_evaluator.rs`) — entirely.
- The old `Closure { node_network_name, node_id, captured_argument_values }`.
- The `output_pin_index == -1` Closure-construction branch
  (`network_evaluator.rs:1612`) and the "-1 is special" output convention.
- The legacy FE-driven `Walker::Map`/`Walker::Filter` variants.
- `evaluate_zone_output` (its eager-HOF callers move to carried-wires resolution).
- The partial-application prefix rule in `data_type.rs`.

## Implementation phases

Each phase is self-contained and ends with `cd rust && cargo test` green and
`cargo clippy` clean; the final editor phase (6) additionally ends with
`flutter run` launching a working editor. Phases are strictly sequential.

### Phase 1: Extract `ZoneClosure` — internal refactor, no new surface

**Goal.** Introduce `ZoneClosure` and route the four existing HOFs through it,
with no user-visible change. Pure refactor.

**Scope.**
- Define `ZoneClosure` (in `network_result.rs` or a small new module). It is
  not yet a `NetworkResult` value — just an internal bundle.
- Factor `build_inline_closure` out of `map.rs` (and the parallel logic in
  `filter`/`fold`/`foreach`): grab `node.zone`, pre-evaluate captures via the
  existing `build_captures`, collect `zone_output_arguments` wires, fill
  `owner_node_id = node_id` and the type metadata.
- Change `Walker::MapZone`/`FilterZone` to carry `ZoneClosure`; update
  `map_zone`/`filter_zone` constructors and `next_inner`.
- Switch `fold`/`foreach`'s eager loop to carried-wires resolution (the
  `eval_step` style) so it reads `ZoneClosure.zone_output_wires` rather than
  `evaluate_zone_output`, factoring the per-step push-frame / swap-captures /
  resolve / pop into a shared `run_closure_once` helper so later closure
  consumers (`apply`, the `f`-driven HOFs) reuse it verbatim. Keep
  `evaluate_zone_output` for now if any other caller remains; otherwise mark it
  for deletion in Phase 2.

**Tests.** No new tests; the existing zone tests are the regression check. All
HOF behavior must be byte-identical.

**Gotchas.** `owner_node_id` must be the body-owning node's id (the HOF itself
here). Keep the push/pop balance audited by the existing debug invariants in
`push/pop_zone_input_frame`.

> ⚠️ **`run_closure_once` must take a base `network_stack` — do not hard-code a
> body-only stack.** This was the one non-obvious trap when implementing Phase 1
> (and is restated under §"What's new" point 3). The naïve reading of "switch
> the eager loop to the `eval_step` style the lazy walker uses" is to resolve
> the zone-output wire against a body-only stack (`vec![{body, owner_id}]`).
> That is correct for the **lazy** walkers (which lack the outer stack and pass
> `&[]`), but it **breaks nested eager HOFs**: when an inner `fold` runs *during*
> the outer drain, its `build_inline_closure` freezes the inner body's captures,
> and a capture reaching past the immediate body (a grandparent constant at
> `source_scope_depth ≥ 2`) only resolves if the full ancestor network stack is
> present. So the signature is
> `run_closure_once(evaluator, network_stack, registry, context, closure, args)`
> and it builds `body_stack = network_stack + body`. **Lazy walkers pass `&[]`;
> eager `fold`/`foreach` pass their real `network_stack`.** The load-bearing
> regression is `nested_fold_inner_captures_outer_constant` (inner fold captures
> grandparent `K` at depth 2; must yield `Int(36)`, not `Error`).

### Phase 2: Make `Function` carry `ZoneClosure`; delete the FE world

**Goal.** Promote `ZoneClosure` to a first-class value and remove the legacy
machinery. No node *produces* a `Function` yet, so the value is valid-but-
unconstructed; tests stay green.

**Scope.**
- Change `NetworkResult::Function(Closure)` → `Function(ZoneClosure)`; delete
  the old `Closure` struct.
- Delete `FunctionEvaluator`, the `output_pin_index == -1` branch in
  `evaluate`, the legacy `Walker::Map`/`Filter` variants, and (now unused)
  `evaluate_zone_output`.
- Keep `DataType::Function`; simplify the `can_be_converted_to` Function arm to
  structural compatibility (drop partial application). Keep `convert_to`'s
  `Function → Function` identity passthrough (`network_result.rs:585`).
- Update `infer_data_type` / `to_display_string` for the new payload.
- Regenerate any insta snapshots touched by the serialization/display changes.

**Tests.** Existing suite green. Optionally a unit test asserting a hand-built
`NetworkResult::Function(ZoneClosure)` round-trips through `convert_to`/
`infer_data_type`.

**Gotchas.** Grep for every reference to the old `Closure` fields and the `-1`
pin convention (validator, serialization, text-format introspection,
`promote_to_parameter.rs`, drag-aware filters). The zones doc already lists most
of these as the planned cleanup sites.

### Phase 3: The `closure` node, the `apply` node, and `f` pin on `map`

**Goal.** First end-to-end function value, produced once and consumed two ways:
a `closure` evaluated by `map` via its new `f` pin, and the same `closure` called
once by an `apply` node. This is the phase where `Function` becomes a real,
callable value.

**Scope.**
- `nodes/closure.rs`: a zone-bearing node with configurable zone pins (v1: one
  zone-input parameter + one zone-output result, types from UI properties,
  following `MapData`'s `calculate_custom_node_type` pattern), a `Function`
  output pin, and the `eval` that returns `Function(build_inline_closure(...))`.
  Register in `nodes/mod.rs` + `node_type_registry.rs`.
- Add the optional `f: Function` input pin to `map`; `map.eval` calls
  `obtain_closure`. When `f` is connected, the inline zone is ignored.
- `nodes/apply.rs`: a bodyless node with a *required* `f: Function` pin, one
  argument pin per parameter (shape from a v1 "kind"), and a single output pin =
  the function's return type. `eval` obtains the `ZoneClosure` from `f` and runs
  it once via `run_closure_once`. Register in `nodes/mod.rs` +
  `node_type_registry.rs`.
- API helpers as needed for tests to construct a `closure` body and wire it
  into `map.f`.

**Tests.** New `rust/tests/structure_designer/closures_test.rs`:
- `range(3) → map(f: closure(element + 1)) → collect` yields `[1,2,3]`.
- Closure reuse: one `closure` wired into two `map`s; both evaluate correctly
  and independently (walker clone independence).
- Capture through a closure: closure body captures a parent `k = int(5)`;
  result reflects the frozen value.
- `f` connected ⇒ inline zone ignored (populate a different inline body, assert
  `f` wins).
- Direct call: `apply(f: closure(element + 1), 10)` yields `11` — no iterator
  involved.
- `apply` honoring a capture: `apply` a closure that captures `k = int(5)` and
  adds it; the result reflects the frozen capture.

### Phase 4: `f` pin on `filter`, `fold`, `foreach`

**Goal.** Roll the pattern out to the remaining HOFs.

**Scope.** Same shape as Phase 3 for each: derive the `f` pin's `Function` type,
call `obtain_closure`, run the existing lazy/eager path.

**Tests.** Extend `closures_test.rs`:
- `filter(f: closure(element % 2 == 0))`.
- `fold(f: closure((acc, element) -> acc + element))` with and without a captured offset.
- `foreach(f: closure(... -> Unit))`; verify Execute gating still works.
- **Capture-freeze timing:** a `closure` defined outside a `fold` (frozen once)
  vs. inside the `fold` body (re-frozen per iteration) — assert both behaviors.
- **`owner_node_id` collision regression:** force a consumed closure's owner id
  to collide with an enclosing HOF's id, consuming the closure *lazily*
  (`map`/`filter`) so its body returns an `Iterator` that escapes the producing
  step and is drained after that step pops; assert correct results. This is the
  load-bearing frozen-capture / scope-stack regression — see §"`owner_node_id`:
  the model's one conceptual debt" for why the lazy/escaping shape (not an
  eagerly-nested `fold`) is the one that actually exercises the invariant.
- **Function-factory smoke test:** build a `(k: Int) -> Function` subnetwork
  whose return is a `closure` capturing `k` and adding it; in the parent,
  `apply(factory(5), 10)` and assert `15`. Proves a function value crosses a
  network boundary *and* is callable via `apply`, using only authorable v1
  surface (no `Function`-typed parameter). The combinator case (`compose`/`flip`)
  is deferred with its prerequisites; see §"Applying a function value".

### Phase 5: Validation

**Goal.** Surface closure/`f`-pin errors at validation time.

**Scope.** In `network_validator.rs`: suspend the "zone-output wire required"
rule for an HOF whose `f` is connected; validate `closure` nodes by the existing
zone-body rules; flag an `apply` node whose required `f` pin is disconnected;
rely on wire type-compat for all `f`-source function-type checks. Ensure
validation and `repair_node_network` descend into `closure` bodies (they already
do via `has_zone()` / `walk_all_nodes`).

**Tests.** Extend `closures_test.rs`: each rule's invalid case asserts the
expected `ValidationError`; a wrong-arity closure wired into `f` is rejected; a
type-incompatible closure is rejected; an `apply` with a disconnected `f` is
rejected.

### Phase 6: Editor (Flutter) — bring the UI online for closures

**Goal.** Author and use closures end-to-end in the editor: place a `closure`,
edit its body (free, via generic zone rendering), pick its shape, and wire its
output into an HOF's `f` pin.

**Scope.** The four items in §"Editor (Flutter) changes": the inline-zone/`f`
toggle, the shared closure/`apply` shape property editor, the Add Node popup
entries (`closure` and `apply`), and function-wiring + FRB regen. No
body-rendering work — that is inherited from the zones UI.

**Tests.** Manual walkthrough — the editor surface is thin and the evaluation it
exercises is already covered by the Rust tests in Phases 3–5, so this phase is
verified by hand rather than a new automated test. Steps: place a `closure`,
set its shape to `(T)->U`, author body `element + 1`, wire its output into a
`map`'s `f` pin, feed `range(3)`, display a downstream `collect`, and confirm
`[1, 2, 3]`. Then place an `apply` node, wire the same `closure` into its `f`
pin, feed a single `int`, and confirm the one-shot result. Also confirm: wiring
`f` hides/disables the HOF's own inline body; disconnecting `f` restores it;
existing non-closure editing is unchanged.

**Verification.** `flutter run` launches a working editor that authors and runs
closure-bearing networks, **and** `cd rust && cargo test` is still green.

### Out of phase plan (deferred)

- **Migration:** `.cnnd` migrator from main's function-pin/closure world to
  `closure` nodes + `f` pins. The preserved `f`-pin topology is what makes this
  localized (rewire a `-1` source to a `closure` node's pin-0) rather than a
  body-duplicating graph surgery.
- **Implicit auto-wrap** (see Open Questions).
- **Combinators + `Function`-typed parameters:** abstraction over behavior (a
  subnetwork taking a `Function` parameter) and `compose`/`flip`. Requires a
  `Function` entry in the type picker / `APIDataTypeBase` (so a parameter can be
  declared `Function`) and, for `flip` and other non-HOF-shaped functions, the
  configurable closure/`apply` arity of Open Question 3. The `apply` node and the
  capturability of `Function` values ship in v1, so this is additive.
- **Structural parameter binding:** replace the id-keyed scope-stack with
  positional argument frames supplied directly at the call site, removing
  `owner_node_id` and its non-uniqueness (see §"`owner_node_id`: the model's one
  conceptual debt").

## Open questions

1. **Implicit closures (auto-wrap).** Could a user wire an arbitrary
   computation node's function pin straight into an HOF's `f` pin and have the
   editor synthesize a one-node closure body in the background? **Mechanically
   yes** — synthesize a `ZoneClosure` whose body wraps the source node, mapping
   its "argument" input to a `ZoneInput` pin and everything else to captures
   (this is what `FunctionEvaluator` did, re-pointed at the zone substrate). The
   consumer side needs no change. The real snag is *which input is the
   argument* — well-defined for an `expr` with a free variable (the old
   convention), ambiguous for a general node, so it needs a UI/convention
   decision. Treat as optional sugar layered on top of the explicit `closure`
   node, not a replacement. Defer.

2. **Multi-result closures.** A closure with more than one zone-output pin is a
   multi-result function. The substrate supports it (`zone_output_wires` is a
   `Vec`), but the `Function` return type would need to be a tuple/record and
   the consumers extended. v1 restricts closures to one zone-output pin. Defer.

3. **Configurable closure arity in the UI.** The substrate is arity-agnostic
   (frames are `Vec<NetworkResult>`), but the editor needs a way to declare "N
   parameters of types ...". v1 can ship fixed shapes matching the HOFs
   (`(T)->U`, `(T)->Bool`, `(A,T)->A`, `(T)->Unit`) selected by a closure
   "kind", with free-form arity as future work.

4. **Promote inline zone ↔ closure node.** An editor gesture to extract an HOF's
   inline body into a standalone `closure` node (and inline a closure back). High
   value, not blocking. Defer to a later iteration.

5. **Recursion.** Closures cannot reference themselves (no self-binding), so no
   recursion is expressible. This is fine for v1; revisit only if a use case
   appears.

6. **Should inline zones remain?** Yes. Inline zones are the ergonomic default
   for the common single-use case; `closure` + `f` is the general/reusable form.
   They share one substrate, so keeping both costs only the `obtain_closure`
   branch.
