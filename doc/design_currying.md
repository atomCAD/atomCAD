# Currying and Partial Application: Function Types Up To Currying

## Scope

This document designs **currying** — treating function types up to the
currying equivalence

```
(A, B, C) → D  ≡  (A) → ((B, C) → D)  ≡  (A, B) → ((C) → D)  ≡  (A) → (B) → (C) → D
```

— and adding **partial application** as the runtime operation that makes
this equivalence constructive. The motivating problem is that today's
closure substrate (`doc/design_closures.md`) treats `Function {
parameter_types: Vec<DataType>, output_type: DataType }` as a flat
*tuple-then-result* with a hard barrier between "where the argument ends
and where the result begins". As a consequence:

- `(Float, Float) → Int` cannot flow into a pin expecting `(Float) →
  ((Float) → Int)` even though the two are isomorphic.
- The `apply` node requires *every* argument pin to be wired; there is no
  way to call a 3-arg function on one or two args and get the remaining
  function back.
- The only authoring path for partial application is **manually nesting
  closure nodes** — for a 3-arg body, three frames deep, with capture
  wires threaded between each layer (see the screenshots in the
  originating discussion: a 3-arg `expr` body nested as `Float → (Float →
  (Float → Float))` is six nodes for what an uncurried `expr` writes in
  one).
- `(Float, Float) → Int` cannot flow into `map.f: (Float) → U` to produce
  `Iter[Function((Float,), Int)]`, because the type system rejects the
  arity mismatch and `map` has no way to absorb extra params into a
  partial result.

Concretely, this branch adds:

1. **Canonical (flat) `FunctionType` storage.** `FunctionType` is stored
   so that `output_type` is never itself a `Function`.
   `FunctionType::new` is the single construction site and absorbs nested
   `Function` returns into `parameter_types`. A one-shot recursive
   normalization pass on `.cnnd` load canonicalizes existing data. After
   Phase 1, `Function((A,), Function((B, C), D))` and `Function((A, B,
   C), D)` are byte-identical in memory — they were semantically equal
   types all along (the user's central thesis). Comparison reverts to
   today's structural same-arity rule with no flatten helper.
2. **`ZoneClosure.pre_supplied_args`.** A small additional
   `Arc<Vec<NetworkResult>>` field on the runtime closure value, holding
   arguments already bound. The shared `run_closure_once` prepends them
   to the caller-supplied frame; the body's `ZoneInput { pin_index }`
   resolution is unchanged because positions line up. **`param_types`
   is the body's actual frame size** — how many caller args one
   `run_closure_once` consumes. It is *decoupled* from the closure's
   declared (canonical, flat) function type, which can be wider when
   the body returns a `Function` value that absorbs more args
   downstream.
3. **Partial application on `apply`, with multi-step consumption.** The
   `apply` node derives its arg pins from the connected `f` source's
   canonical (flat) arity. The user wires a *contiguous prefix* of arg
   pins (`arg0…arg_{k-1}`) and leaves the rest unwired. At runtime,
   `apply.eval` consumes the underlying closure's *body* arity per
   step; if more wired args remain, it recursively applies them to the
   resulting `Function` value. This makes apply correct even when the
   wired closure's body arity is smaller than its declared flat arity
   (e.g. a 1-arg closure returning a 1-arg function, viewed as a 2-arg
   function under canonicalization).
4. **HOF auto-partialization on `map`.** `map.f` accepts any `Function`
   source whose parameter list *starts with* `[element_type]`. Excess
   parameters become the partial-application tail; `map.output_type`
   is **derived** from `f` rather than user-set whenever `f` is
   connected. A `(Float, Float) → Int` source flowing into `map.f`
   over an `Iter[Float]` produces `Iter[Function((Float,), Int)]`
   directly — no inline-body authoring required. `filter` / `fold` /
   `foreach` keep exact-arity `f` pins; their output type is
   constrained (Bool / Acc / Unit) so a partial result has nowhere to
   go. Only `map`'s output is unconstrained, so only `map`
   participates.
5. **0-arity `Custom` closures.** The `Custom` `ClosureKind` accepts an
   empty `param_names` list. The substrate already supports `args =
   vec![]`; this lifts the editor's "at least one param" assumption.
   The title bar renders `() → R`.
6. **Editor support.** When `f` is wired on `apply`, the shape panel
   becomes a read-only *derived display* of the wired source's flat
   function type, and the existing `ClosureShapeEditor` (kind picker)
   is hidden. When `f` is unwired, the kind picker returns as the
   intended-shape affordance. On `map`, the `output_type` editor field
   becomes a read-only derived display whenever `f` is connected.

In scope:
- Conceptual model and worked examples.
- Canonical `FunctionType` construction + `.cnnd` load-time normalization.
- `ZoneClosure` field addition and `run_closure_once` prepend.
- `apply` node rewrite (`calculate_custom_node_type`, `eval` with
  recursive consumption, validation).
- `map`'s `calculate_custom_node_type` derives `output_type` from `f`;
  `map.f` pin compatibility uses the "starts-with" rule.
- 0-arity `Custom` closure editor relaxation.
- Editor changes for the `apply` shape editor, `map` output-type
  display, and pin-derivation flow.
- Implementation phases, each ending in `cargo test` green plus (Phase
  5) `flutter run` working.

Out of scope:
- **Auto-partialization on other HOFs.** `filter` / `fold` / `foreach`
  have constrained output types (Bool / Acc / Unit) and cannot
  meaningfully absorb extra args. A future "result HOF" with an
  unconstrained output could adopt the `map` pattern verbatim.
- **Non-prefix wiring on `apply`** ("skip arg1, supply arg0 and
  arg2"). Requires permutation bookkeeping and breaks currying's
  positional clarity. See Open Question 1.
- **Smarter shape inference when `f` is disconnected on `apply`.** With
  nothing wired, `apply` still needs *some* default shape to render arg
  pins against. v1 keeps the existing `ClosureKind` picker for this
  case; anything cleverer is deferred. See Open Question 2.
- **Composition / `flip` combinators.** Deferred per
  `design_closures.md` open questions; this doc unblocks them by
  making partial application cheap, but does not deliver them.
- **`.cnnd` migration version bump.** The normalization is *lossless*
  (canonical and non-canonical forms are semantically equal types), so
  no version bump is required: old files load, the walker normalizes
  every stored `DataType` recursively, save writes canonical. The
  on-disk schema of `ApplyData` / `ClosureData` / `MapData` is
  unchanged; only the *content* of `DataType::Function` values is
  normalized in place.

## Scope of this branch — build/test contract

The verification gate is `cd rust && cargo test` green and `cargo
clippy` clean for every Rust phase (1–4), plus `flutter run`
launching a working editor at the end of Phase 5. Existing closure
behavior is preserved byte-identically by every phase (empty
`pre_supplied_args`; canonicalization is the identity on already-flat
types, and every `FunctionType` in the existing fixture set is already
flat).

| Must pass | When |
|---|---|
| `rust/` workspace (`cargo build`, `cargo clippy`, `cargo test`) | every phase |
| `flutter_rust_bridge_codegen generate` succeeds | every phase |
| Existing closure-/HOF-using tests, **byte-identical results** | Phase 1, 2, 3, 4 |
| Existing `.cnnd` fixtures load successfully (incl. canonicalization round-trip) | Phase 1 onward |
| `flutter run` launches; partial `apply` and `map` auto-partial work end-to-end | Phase 5 |
| Existing non-closure editing still works | every phase (regression) |

The branch starts after the zones / closures / function-pins /
migration docs have all landed. Baseline: the full Rust suite (3400+
tests) green, the four HOFs and `closure` / `apply` evaluating via
the substrate in `evaluator/zone_closure.rs`, `DataType::Function`
carrying a flat `parameter_types` + `output_type`.

## Motivation

The user's complaint, condensed: *"Float → Float → Float → Float,
(Float, Float) → (Float → Float), (Float, Float, Float) → Float —
these are all the same thing. Where the argument ends and where the
result begins depends on how many slots one fills when partially
evaluating. With curried format being the go-to default there is no
way to do partial evaluations. And setting up currying is bad UX
ATM."*

Symptoms in the editor today:

- **The non-nested representation is unreachable from existing entry
  points.** Authoring an N-arg body as a single `expr` inside one
  `closure` produces a `Function((P0, …, P_{N-1}), R)` value. There is
  no way to partially apply it: `apply` requires all N pins wired, and
  no HOF accepts a multi-arg function on its `(T) → U` / `(T) → Bool`
  / `(A, T) → A` / `(T) → Unit` `f` pin.
- **The nested representation is brutal.** To express the same 3-arg
  body with partial-application potential, the user has to draw three
  closure frames deep, each capturing one arg of the next. Visual
  clutter scales linearly with arity; the user reports it as "no user
  would do that".
- **The asymmetry between authoring and consumption is forced by the
  type system.** Internally the substrate is already arity-agnostic —
  frames are `Vec<NetworkResult>`, `run_closure_once` pushes whatever
  the caller passes, `ZoneInput { pin_index }` resolves positionally.
  The barrier between parameters and result lives in
  `DataType::Function`'s structural same-arity rule
  (`data_type.rs:391-414`) and in `apply`'s requirement that every
  declared arg pin be wired (`apply.rs:131-143`).

The fix: collapse the type-system barrier by storing `FunctionType`
in a canonical flat form, add a tiny runtime field to carry pre-bound
args, let `apply` partially apply on the wired prefix (with recursive
consumption for nested-body cases), and let `map` absorb excess
source arity into a partial output.

## Concept

### Canonical (flat) function types

`FunctionType` is stored so that `output_type` is never itself a
`Function`. The canonical form is produced by a single constructor:

```rust
impl FunctionType {
    pub fn new(parameter_types: Vec<DataType>, output_type: DataType) -> Self {
        let mut params = parameter_types;
        let mut output = output_type;
        // Absorb nested Function returns into the parameter list.
        while let DataType::Function(inner) = output {
            params.extend(inner.parameter_types);
            output = *inner.output_type;
        }
        Self { parameter_types: params, output_type: Box::new(output) }
    }
}
```

Every construction site routes through `FunctionType::new`. Serde
`Deserialize` is wired to call `new` post-deserialization (custom
`Deserialize` impl, or a `serde(from = "FunctionTypeRaw")` shim) so
on-disk forms are canonicalized as they enter memory.

A recursive `canonicalize_data_type` walker canonicalizes any
`DataType::Function` nodes embedded inside container types
(`Iter[Function(…)]`, `Array[Function(…)]`, `Option[Function(…)]`,
record fields containing function types, etc.). The walker runs on
`.cnnd` load before the loaded network is exposed to anything else,
ensuring no non-canonical form ever reaches the validator or
evaluator.

After Phase 1, the type-comparison rule is plain structural same-arity
(today's rule, unchanged in code, but now sound by construction):

```
(A, B, C) → D  ≡  (A) → ((B, C) → D)  ≡  (A, B) → ((C) → D)  ≡  (A) → (B) → (C) → D
```

All four are stored as `Function((A, B, C), D)`. Comparison is identity.

**Storage is canonical; runtime closure body arity is not.** A
`ZoneClosure` constructed from a 1-arg `closure` node whose return
type is `Function((B,), C)` still has `param_types = [A]` (the body's
actual frame size) and `return_type = Function((B,), C)`. Its
*declared function type*, used at the type-system level, is
canonicalized to `Function((A, B), C)`. The two views are reconciled
at runtime by `apply.eval`'s recursive consumption (see §"`apply`
semantics").

### `pre_supplied_args` and partial application

A `ZoneClosure` gains one field:

```rust
pub struct ZoneClosure {
    // ... existing fields ...
    pub pre_supplied_args: Arc<Vec<NetworkResult>>,  // default: Arc::new(Vec::new())
}
```

The semantics: *when a consumer pushes its iteration frame, the
actual frame becomes `[pre_supplied_args… ++ caller_args…]`.* The
body's `ZoneInput { pin_index }` resolution is unchanged — pins are
positional, the frame is just longer than the caller's `args` vector.
`param_types` continues to be "the types of the slots a caller must
fill in this body invocation" — the *remaining unbound* positions in
the body's frame. A freshly-built closure has `pre_supplied_args =
empty`, `param_types = full body arity`.

**Partial application within a single body** is the operation that,
given a closure `C` with body arity `n_body` and `k ≤ n_body` arg
values, produces a new closure `C'`:

```
C'.pre_supplied_args = Arc::new(C.pre_supplied_args ++ [a_0, …, a_{k-1}])
C'.param_types       = C.param_types[k..].to_vec()
C'.body, captures, zone_output_wires, owner_node_id, return_type — unchanged
```

`C'` is exactly the function `(P_k, …, P_{n_body-1}) → return_type`
that `C` *evaluated on its first k arguments* would have left behind.
When `k == n_body`, we push the full frame and resolve the body,
exactly as today's `run_closure_once`.

When the caller has *more* args than the body's arity (the canonical
flat declared type was wider than the body), `apply.eval` runs the
body to completion, expects the result to be
`NetworkResult::Function(C_next)`, and recurses on `C_next` with the
remaining args. See next section.

### `apply` semantics

`apply` chooses k — how many of the function's arguments are
supplied at this call site. Today it implicitly fixes `k = N` (every
arg pin required). After this design:

- `apply` derives its argument pin shape from the wired `f`'s
  **declared** (canonical, flat) function type. Number of arg pins =
  `f.declared_type.parameter_types.len()` (call it `N`).
- The user wires the **contiguous prefix** `arg0 … arg_{k-1}` and
  leaves `arg_k … arg_{N-1}` unwired. `k = number of wired arg pins`.
- Output pin type is **dynamic in k**:
  - `k == N` ⇒ `R` (full eval).
  - `k < N` ⇒ `Function(declared_params[k..], R)`, canonicalized.
- Wiring `arg_j` while `arg_i` (i < j) is unwired is a **validation
  error** attributed to `apply`. Prefix-only is the rule (Open
  Question 1).

At runtime, `apply.eval` walks the closure value step-by-step,
because the underlying body may have a smaller arity than the
declared flat type. Pseudocode:

```rust
let k = count_wired_arg_prefix(self, node);
let mut remaining: VecDeque<NetworkResult> = /* k resolved arg values */;

// Identity-partial guard: caller supplied zero args to a non-thunk.
// (Thunks — declared arity 0 — fall through and run their body below.)
if k == 0 && !f_current.function_type().parameter_types.is_empty() {
    return Function(f_current);
}

loop {
    let n_body = f_current.param_types.len();

    // Partial step: not enough args left to fill this body invocation.
    if remaining.len() < n_body {
        let drained: Vec<_> = remaining.drain(..).collect();
        let drained_len = drained.len();
        let mut extended = (*f_current.pre_supplied_args).clone();
        extended.extend(drained);
        return Function(ZoneClosure {
            body: Arc::clone(&f_current.body),
            captures: Arc::clone(&f_current.captures),
            zone_output_wires: Arc::clone(&f_current.zone_output_wires),
            owner_node_id: f_current.owner_node_id,
            param_types: f_current.param_types[drained_len..].to_vec(),
            return_type: f_current.return_type.clone(),
            pre_supplied_args: Arc::new(extended),
        });
    }

    // Full body step: consume n_body args, run the body.
    let step_args: Vec<_> = remaining.drain(..n_body).collect();
    let result = run_closure_once_in_inner_ctx(.., &f_current, step_args);

    if remaining.is_empty() {
        return result;
    }

    // More args to go — the result must be another Function value.
    match result {
        NetworkResult::Function(next) => f_current = next,
        NetworkResult::Error(_)       => return result,
        other => return Error("apply: expected Function for further application"),
    }
}
```

For the common case — `k == N == n_body`, declared arity matches body
arity — this collapses to a single iteration that calls
`run_closure_once` exactly as today's `apply`. The recursive branch
fires only when the wired closure's body returns another `Function`
value that further args must be applied to — which canonical storage
explicitly *permits* (a 1-arg closure returning a 1-arg function is a
valid runtime shape, even though its declared flat type is 2-arg).

The 0-arity case (`N == 0`, `k == 0`) is the *thunk-force*: the
identity-partial guard does not fire (declared params are empty), so
the loop runs once with `n_body = 0`, `step_args = vec![]`, calls
`run_closure_once` with no args, returns the result.

### HOF auto-partialization (`map`)

`map.f` accepts any `Function` source whose parameter list **starts
with** `[element_type]`. The excess parameters become the
partial-application tail; `map.output_type` is **derived** from `f`
whenever `f` is connected.

Compatibility rule on `map.f` (used by the validator and the drag-aware
add-node popup):

```
src is Function
  AND src.parameter_types.starts_with([element_type])
```

Derivation of `map.output_type` when `f` is connected:

```
let tail = &src.parameter_types[1..]; // params after the leading element_type
if tail.is_empty() {
    map.output_type = (*src.output_type).clone()
} else {
    map.output_type = DataType::Function(FunctionType::new(
        tail.to_vec(),
        (*src.output_type).clone(),
    ))
}
```

When `f` is disconnected, `map.output_type` falls back to its
user-configured stored value (today's behavior).

Filter / fold / foreach keep exact-arity `f` pins. Their output types
are constrained (Bool, Acc, Unit), so they cannot absorb a partial
result. The design does not lift this restriction.

### Worked examples

**1. The user's headline screenshot, fixed (via `map` auto-partial).**
A `closure` `g` of kind `Custom` with `param_names = ["x", "y"]`,
`type_args = [Float, Float, Int]`, body `expr` evaluating `x * y`.
Its declared canonical type is `Function((Float, Float), Int)`. The
user wants to turn this into `Iter[Function((Float,), Int)]` by
mapping over an `Iter[Float]`.

```
range(3)  --xs-->  map  --result-->  collect
                    ^
                f = g   (g connected directly to map.f)
```

`map`'s element type is `Float` (from `xs: Iter[Float]`). `g`'s
declared type starts with `[Float]`. The tail is `(Float,)`,
non-empty. `map.output_type` is derived as `Function((Float,), Int)`,
so `map`'s output is `Iter[Function((Float,), Int)]`. The walker
yields one partially-applied closure per `xs` element (each carrying
that element in `pre_supplied_args`). No inline body required, no
`apply` node, no nesting.

**2. Direct partial chain.** Top-level: `g: (Float, Float, Float) →
Float` with body `expr` evaluating `x + y + z`. Wire `apply(g,
float(2.0))` — `N=3`, `k=1`, output `Function((Float, Float),
Float)`. Wire *that* into a second `apply` with `arg0 = float(3.0)`
— output `Function((Float,), Float)`. Wire *that* into a third
`apply` with `arg0 = float(4.0)` — output `Float`, evaluating to
`9.0`.

The user's "right side" reference shape (the chain of three `apply`
nodes in the second screenshot) is the flat equivalent of their
nested left-side ladder.

**3. Existing fully-wired apply.** Unchanged. `N == k == n_body`,
`pre_supplied_args` empty before and after. `apply.eval`'s loop runs
exactly one iteration and returns the result.

**4. Body arity less than declared flat arity (recursive
consumption).** A `closure` `h` of kind `Custom` with `param_names =
["x"]`, `type_args = [Float, Function((Float,), Float)]`, body
returns an inner closure `inner_closure(y) = x + y` (`y` is the
inner's zone input, `x` is the captured outer zone input). The outer
closure's body arity is 1; its declared canonical type is
`Function((Float, Float), Float)`. An `apply(h, float(2.0),
float(3.0))` shows 2 arg pins (from the declared flat type), `k=2,
N=2`. At runtime, `apply.eval`'s loop runs twice: first consumes
`[2.0]` (body arity = 1) and gets back the inner closure with `x =
2.0` captured; second consumes `[3.0]` on the inner closure (its
body arity = 1, `remaining = 1`), full-evals to `5.0`.

**5. `fold`-shaped function `(A, T) → A` wired into `map.f: (T) →
U`.** Now *accepted* via `map` auto-partial: `map`'s element type is
`T`, starts-with-`[T]` matches, tail `(A,)`, `map.output_type`
derived as `Function((A,), A)`. (Whether the user intended a
list-of-folders is up to them; the type system accepts it.) The same
function wired into `fold.f` works exactly as today (exact arity).

**6. 0-arity thunk forced via `apply`.** A `closure` `g` of kind
`Custom` with `param_names = []`, `type_args = [Float]`, body `expr`
evaluating `42.0`. Declared type: `Function((), Float)`. `apply` with
`f = g` shows zero arg pins. The output pin type is `Float`. At
runtime, `apply.eval`'s identity-partial guard does not fire
(declared params are empty); the loop runs once with `n_body = 0`,
calls the body with no args, returns `42.0`.

## Data model

### `DataType::Function` — type unchanged, construction canonicalizes

The `FunctionType` struct stays as it is on the wire
(`data_type.rs:8-11`):

```rust
pub struct FunctionType {
    pub parameter_types: Vec<DataType>,
    pub output_type: Box<DataType>,
}
```

What changes is the **invariant**: in canonical form, `output_type`
is never itself a `Function`. The invariant is enforced by
`FunctionType::new` (see §"Canonical (flat) function types"). All
construction sites use `new`. Code review enforces this in new code;
existing struct-literal sites are audited and rewritten in Phase 1.
(Optionally, the fields could be made private and accessed via
getters; this is a larger refactor and is left as a Phase 1 follow-up
if it doesn't fit the budget.)

The on-disk schema is unchanged; the only thing that changes on-disk
is that the load-time canonicalization walker collapses any
non-canonical form to its canonical equivalent. Save then writes
canonical, so re-saving a loaded file yields a normalized file.

### `ZoneClosure` — one new field, body arity decoupled from declared arity

```rust
pub struct ZoneClosure {
    pub body: Arc<NodeNetwork>,
    pub captures: Arc<HashMap<CaptureKey, NetworkResult>>,
    pub zone_output_wires: Arc<Vec<IncomingWire>>,
    pub owner_node_id: u64,
    pub param_types: Vec<DataType>,
    pub return_type: DataType,
    /// Args already bound by partial application. Prepended to the
    /// caller-supplied frame inside `run_closure_once`. Default empty.
    /// `Arc`-shared so cloning a partially-applied closure (Walker
    /// Invariant 2) stays a refcount bump.
    pub pre_supplied_args: Arc<Vec<NetworkResult>>,
}
```

`param_types` is the **body's actual frame size** — how many caller
args one `run_closure_once` consumes. It is *decoupled* from the
closure's **declared (canonical, flat)** function type, which may be
wider when `return_type` is itself a `Function`. The declared type is
computed by a helper:

```rust
impl ZoneClosure {
    pub fn function_type(&self) -> FunctionType {
        FunctionType::new(self.param_types.clone(), self.return_type.clone())
    }
}
```

Since `FunctionType::new` canonicalizes, `function_type()` may return
a flatter form than `param_types.len()` would suggest. The declared
type is what `apply`'s pin layout and type-conversion checks see.
Runtime body arity (`param_types.len()`) is what `apply.eval` actually
consumes per loop iteration.

Cloning cost: an empty `Arc<Vec<_>>` shares one allocation across all
contexts. Partial-application clones bump the refcount of the extended
args vec; no deep copy.

### `ApplyData` — stored shape unchanged

```rust
pub struct ApplyData {
    pub kind: ClosureKind,
    pub type_args: Vec<DataType>,
    #[serde(default)]
    pub param_names: Vec<String>,
}
```

Unchanged on the data model side. The semantic role shifts:

- **`f` connected**: the wired source's declared (canonical, flat)
  function type drives `calculate_custom_node_type`'s arg-pin
  enumeration and the output-pin type. `ApplyData.kind` / `type_args`
  are ignored at evaluation and the editor de-emphasizes the kind
  picker.
- **`f` disconnected**: the kind picker still drives the default arg
  layout, exactly as today.

### `MapData` — stored shape unchanged; `output_type` becomes derived when `f` connected

```rust
pub struct MapData {
    pub element_type: DataType,    // derived from xs in calculate_custom_node_type
    pub output_type: DataType,     // user-set fallback; derived from f when connected
    // ... existing fields ...
}
```

The stored `output_type` is now a fallback for when `f` is
disconnected. When `f` is connected, `calculate_custom_node_type`
overrides with the derived type (see §"HOF auto-partialization
(`map`)"). On disconnect, the editor restores the stored value into
the displayed field.

### Editor view (`APIApplyView` + `APIMapView`, surfaced via `NodeView`)

```rust
pub struct APIApplyView {
    /// Declared (canonical, flat) arity of the wired source.
    pub arity: usize,
    /// Per-arg display name (from source's pin names or "arg0", "arg1", …).
    pub arg_names: Vec<String>,
    pub param_types: Vec<DataType>,
    pub return_type: DataType,
    /// True if the resolved shape came from a connected `f`; false
    /// if we fell back to the kind picker.
    pub from_wired_f: bool,
}

pub struct APIMapView {
    /// True if `output_type` was derived from a connected `f`;
    /// false if from the stored `MapData.output_type`.
    pub output_type_from_wired_f: bool,
    pub effective_output_type: DataType,
}
```

The "what shape am I actually showing" is computed Rust-side (needs
`NodeTypeRegistry::resolve_output_type` for the wired source).
Flutter reads these views and renders.

## Evaluator changes

### Reuse from today's substrate (unchanged)

| Existing piece | Reused? |
|---|---|
| `build_inline_closure`, `build_node_function_closure` | Yes — emit closures with empty `pre_supplied_args`; `param_types` is body's actual arity (not declared flat arity) |
| `obtain_closure` | Yes |
| `run_closure_once` | Yes, with one tiny prepend |
| `Walker::MapZone` / `FilterZone::next` | Yes — they call `run_closure_once`; partial-applied closures arriving via `map` auto-partial are evaluated per-element (the captured element becomes `pre_supplied_args`) |
| Eager `fold` / `foreach` drain loops | Yes |
| `evaluate_arg` / `evaluate_arg_required` | Yes |
| `CapturesGuard` and the `current_zone_input_values` scope-stack | Yes |
| Per-node `eval` implementations | Yes, except `apply` |

### `run_closure_once` — prepend `pre_supplied_args`

```rust
// Today:
//   context.push_zone_input_frame(closure.owner_node_id, args);
//
// After:
let mut frame = Vec::with_capacity(closure.pre_supplied_args.len() + args.len());
frame.extend(closure.pre_supplied_args.iter().cloned());
frame.extend(args);
context.push_zone_input_frame(closure.owner_node_id, frame);
```

For an empty `pre_supplied_args` (every existing call site), this is
one `Vec::with_capacity(args.len())` and two zero-length `extend`s
folded into the existing single `extend(args)` — nil cost.

### `apply.eval` — partial application + recursive consumption

```rust
fn eval(...) -> EvalOutput {
    // 1. Resolve f.
    let mut f_current: ZoneClosure = match evaluator.evaluate_arg(.., f_idx) {
        NetworkResult::Function(zc) => zc,
        NetworkResult::None => return EvalOutput::single(
            NetworkResult::Error("apply: f not connected".into())),
        e @ NetworkResult::Error(_) => return EvalOutput::single(e),
        other => return EvalOutput::single(
            NetworkResult::Error(format!("apply: f is not a function (got {})",
                                          other.to_display_string()))),
    };

    // 2. Resolve k wired arg pins (contiguous prefix).
    let k = count_wired_arg_prefix(self, node);
    let mut remaining: VecDeque<NetworkResult> = VecDeque::with_capacity(k);
    for i in 0..k {
        match evaluator.evaluate_arg_required(.., 1 + i) {
            v @ NetworkResult::Error(_) => return EvalOutput::single(v),
            v => remaining.push_back(v),
        }
    }

    // 3. Identity-partial guard. k=0 with a non-thunk f is just `f`.
    //    (Thunks — declared arity 0 — fall through to the loop and run.)
    if k == 0 && !f_current.function_type().parameter_types.is_empty() {
        return EvalOutput::single(NetworkResult::Function(f_current));
    }

    // 4. Walk the closure, consuming body-arity args per step.
    loop {
        let n_body = f_current.param_types.len();

        // Partial: not enough args remaining to fill this body invocation.
        if remaining.len() < n_body {
            let drained: Vec<NetworkResult> = remaining.drain(..).collect();
            let drained_len = drained.len();
            let mut extended = (*f_current.pre_supplied_args).clone();
            extended.extend(drained);
            let partial = ZoneClosure {
                body: Arc::clone(&f_current.body),
                captures: Arc::clone(&f_current.captures),
                zone_output_wires: Arc::clone(&f_current.zone_output_wires),
                owner_node_id: f_current.owner_node_id,
                param_types: f_current.param_types[drained_len..].to_vec(),
                return_type: f_current.return_type.clone(),
                pre_supplied_args: Arc::new(extended),
            };
            return EvalOutput::single(NetworkResult::Function(partial));
        }

        // Full body step: consume n_body args, run the body.
        let step_args: Vec<NetworkResult> = remaining.drain(..n_body).collect();
        let mut inner_ctx = context.fresh_inner_for_eager_body();
        let result = run_closure_once(.., &mut inner_ctx, &f_current, step_args);
        context.drain_inner_context(inner_ctx);

        if remaining.is_empty() {
            return EvalOutput::single(result);
        }

        // More args to go — the result must be another Function value.
        match result {
            NetworkResult::Function(next) => f_current = next,
            NetworkResult::Error(_) => return EvalOutput::single(result),
            other => return EvalOutput::single(NetworkResult::Error(format!(
                "apply: expected Function for further application, got {}",
                other.to_display_string()))),
        }
    }
}
```

For the common case (declared arity = body arity = `k`), the loop
runs once. The recursive branch fires only when the wired closure's
body returns another `Function` and there are still args to consume.

The body's `ZoneInput { pin_index }` references continue to use the
original pin indices — positional parameter passing through
`owner_node_id`-keyed scope frames is unchanged.

### `map.calculate_custom_node_type` — derive `output_type` from `f`

```rust
fn calculate_custom_node_type(.., wired_f_source_type: Option<&DataType>) {
    // ... existing element_type derivation from xs ...

    let derived_output = match wired_f_source_type {
        Some(DataType::Function(ft)) if ft.parameter_types.starts_with(std::slice::from_ref(&element_type)) => {
            let tail = &ft.parameter_types[1..];
            if tail.is_empty() {
                (*ft.output_type).clone()
            } else {
                DataType::Function(FunctionType::new(
                    tail.to_vec(),
                    (*ft.output_type).clone(),
                ))
            }
        }
        Some(_) => /* mismatch — leave incompatible; validator flags */,
        None => self.data.output_type.clone(),  // fallback to stored
    };

    // map's output pin type is Iter[derived_output].
    self.set_output_type(DataType::Iter(Box::new(derived_output)));
}
```

The `from_wired_f` flag in `APIMapView` is true whenever `f` is
connected with a compatible source.

### Edge cases

- **Nested partials via `apply` chaining.** `apply(apply(g, x), y)`
  where `g: (A, B, C) → R`. The inner `apply` sees `N=3`, `k=1`,
  returns `Function((B, C), R)`. The outer sees the result's declared
  arity = 2, `k=1`, returns `Function((C,), R)`. Each step extends
  the args vector; no body duplication, no capture re-evaluation.
- **Partials of a 1-arg function.** `g: (A) → R`, `apply(g, a)`.
  `N=1, k=1, n_body=1`, single iteration of the loop, full eval.
- **`k = 0` on a non-thunk.** Identity-partial guard returns `f`
  unchanged.
- **0-arity thunk forced via `apply`.** Identity guard does not fire
  (declared params empty); loop runs once with `n_body = 0`, calls
  body with `vec![]`, returns the result.
- **`fold`-shaped function `(A, T) → A` wired into `map.f: (T) →
  U`.** Accepted via auto-partial: tail is `(A,)`, `map.output_type`
  derives to `Function((A,), A)`. The same function wired into
  `fold.f` works as today (exact arity).
- **Pathological case: body arity 0 with result `Function` and more
  args.** A 0-arity body that returns a `Function` is forced
  immediately, and `f_current` advances to the returned closure on
  the next loop iteration. Each loop iteration must make progress
  either by draining args or by advancing `f_current`; the 0-arity
  case advances `f_current` exactly once per iteration. To bound the
  loop defensively, assert that an iteration with `n_body == 0` and
  `remaining` non-empty either advances `f_current` to a different
  `ZoneClosure` value or returns an error. (In practice, body arity
  >= 1 in real networks unless someone explicitly chains a thunk.)

## Type system

### Canonical storage + load-time migration

The canonical-storage invariant is enforced at three points:

1. **`FunctionType::new`** is the single in-code construction site.
2. **`serde Deserialize`** routes through `new` (custom impl or
   `serde(from = "FunctionTypeRaw")` shim).
3. **`.cnnd` load** runs `canonicalize_network` over every stored
   `DataType` in the deserialized network, before the network is
   handed to the validator or evaluator.

The walker:

```rust
fn canonicalize_data_type(t: &mut DataType) {
    match t {
        DataType::Function(ft) => {
            for p in &mut ft.parameter_types {
                canonicalize_data_type(p);
            }
            canonicalize_data_type(&mut ft.output_type);
            // Absorb nested Function return:
            loop {
                let replaced = std::mem::replace(
                    ft.output_type.as_mut(),
                    DataType::None,
                );
                match replaced {
                    DataType::Function(inner) => {
                        ft.parameter_types.extend(inner.parameter_types);
                        *ft.output_type = *inner.output_type;
                    }
                    other => {
                        *ft.output_type = other;
                        break;
                    }
                }
            }
        }
        DataType::Iter(inner) | DataType::Array(inner) | DataType::Option(inner) => {
            canonicalize_data_type(inner);
        }
        // Records reference type defs by name; canonicalization of
        // record-def field types is driven by the record-type-def
        // walker, not here.
        _ => {}
    }
}
```

Sites that store `DataType` values needing the walker, in v1:

- `ClosureData.type_args: Vec<DataType>` — closure node param/return types.
- `ApplyData.type_args: Vec<DataType>` — apply node fallback types.
- `MapData.{element_type, output_type}` — and equivalents on
  `FilterData`, `FoldData`, `ForeachData`.
- `RecordTypeDef.fields[i].field_type` — record field types.
- Custom-network return types if stored.
- Any other node-data field carrying a `DataType` (audit per-node-type
  in Phase 1).

A central `fn canonicalize_network(net: &mut SerializableNodeNetwork)`
walks every such field and recurses into nested bodies. The migration
is **lossless** (canonical and non-canonical forms are semantically
equal types), so no `.cnnd` version bump is required.

The comparison rule (`can_be_converted_to`'s `Function` arm) is then
plain structural same-arity — today's rule, unchanged in code:

```rust
if let (DataType::Function(src), DataType::Function(dst)) =
    (source_type, dest_type)
{
    if src.parameter_types.len() != dst.parameter_types.len() {
        return false;
    }
    if !DataType::can_be_converted_to(&src.output_type, &dst.output_type, registry) {
        return false;
    }
    for (s, d) in src.parameter_types.iter().zip(dst.parameter_types.iter()) {
        if !DataType::can_be_converted_to(s, d, registry) {
            return false;
        }
    }
    return true;
}
```

The `strict_no_broadcast` variant likewise stays structural
same-arity. No flatten helper is needed at compare time — the
invariant is established at construction.

For `map.f` (Phase 4) the compatibility check uses the *starts-with*
rule and is implemented at the HOF-pin-compatibility layer, not in
`can_be_converted_to`. The type rule stays simple.

### Effect on existing wires

- Every wire that exists today has source and destination types whose
  canonical forms are *identical to themselves* (no nested `Function`
  returns appear in any built-in pin signature). Canonicalization is
  the identity for the existing fixture set. **No existing wires
  change validity.** Phase 1 should add a regression test that this
  holds across the existing fixture set.
- New wires that exercise canonical equivalence (a `closure` of kind
  `Custom` whose stored `type_args[-1]` is a `Function`, or a partial
  `apply` emitting one) work from Phase 1 onward.

## Validation

Four small additions / changes to `network_validator.rs`:

1. **Prefix-only wiring on `apply`.** If `apply.arguments[1+j]` is
   wired while `apply.arguments[1+i]` for some `i < j` is unwired,
   attach an error to the `apply` node ("argument pins must be wired
   as a contiguous prefix").
2. **`apply`-arity-vs-`f` consistency.** When `f` is wired and
   resolved, `apply`'s declared arg-pin count equals the canonical
   (flat) arity of `f`'s source type. The drift case is repaired by
   `repair_node_network` following the existing "input type changes
   ⇒ disconnect now-incompatible wires" pattern.
3. **`apply` with `f` disconnected.** Still an error — `apply` always
   requires `f`.
4. **`map.f` starts-with rule.** When `f` is wired, the source's
   canonical function type must start with `[element_type]`. If not,
   error on the wire (incompatible).

The existing "every required input pin must be wired" rule is
**relaxed selectively for `apply`'s arg pins**: only pin 0 (`f`)
remains required. Arg pins are marked optional in
`get_parameter_metadata`.

## Editor (Flutter) changes

The Flutter changes are concentrated in three node panels and two new
view types:

1. **`apply` shape panel.** When `f` is connected, hide the kind
   dropdown and show a read-only summary: *"f: (A, B, C) → R — derived
   from the wired source."* Per-arg pin labels sourced from the wired
   `closure`'s authored `param_names` when available, else `arg0 …
   arg_{N-1}`. When `f` is disconnected, the existing
   `ClosureShapeEditor` returns as the affordance for declaring the
   intended shape.
2. **`map` output-type display.** When `f` is connected, the
   `output_type` editor field becomes a read-only display of the
   derived type with a "derived from f" hint. When `f` is
   disconnected, the field returns to user-editable, restored from
   `MapData.output_type`.
3. **0-arity Custom closures.** The `ClosureShapeEditor`'s Custom
   branch must accept an empty param-name list. "Add param" works as
   today; "Remove param" works down to zero rows. With zero params,
   the title bar renders `() → R`. The corresponding `apply` shape
   shows zero arg pins.
4. **`APIApplyView` + `APIMapView`** plumbed through `NodeView`,
   populated by `build_node_view`, FRB-regenerated for Flutter.
5. **Output pin typing** on `apply` and `map` reflects either the
   derived type or the fallback, recomputed when wiring changes —
   same lifecycle as `record_destructure`'s schema-driven pin types.
6. **Per-arg-pin "unconnected = partial" affordance.** Arg pins on
   `apply` render with no default-value placeholder (unwired =
   deferred, not defaulted). A small tooltip reads *"unwired ⇒ part
   of the resulting function's parameter list"*.
7. **No changes to wire creation or drag.** `Function` values are
   still normal typed wires. Canonical storage makes type comparison
   a one-line structural check.
8. **No changes to `closure`** beyond the 0-arity Custom relaxation.

## Reuse map (summary)

**Reused unchanged:**
- `Walker::MapZone` / `FilterZone` — partial-applied closures
  produced by `map` auto-partial are evaluated per-element by the
  standard `run_closure_once` (the captured element travels via
  `pre_supplied_args`).
- `obtain_closure`, `build_inline_closure`,
  `build_node_function_closure`.
- Every per-node `eval` implementation except `apply`.
- `CapturesGuard`, `current_zone_input_values` scope-stack,
  `eval_step`.
- The `closure` node and its `ClosureKind` (incl. `Custom`).
- `repair_node_network` machinery.

**Reused with small extensions:**
- `ZoneClosure` — `pre_supplied_args: Arc<Vec<NetworkResult>>` field.
- `run_closure_once` — prepend `pre_supplied_args`.
- `DataType::can_be_converted_to`'s `Function` arm — structural
  same-arity (today's rule, now sound by construction).
- `apply.eval` — partial branch with recursive consumption loop.
- `apply.calculate_custom_node_type` — derive arg-pin layout from
  wired `f`'s declared (canonical, flat) type.
- `apply.get_parameter_metadata` — arg pins become optional.
- `map.calculate_custom_node_type` — derive `output_type` from `f`
  via the starts-with rule when connected.
- `MapData.output_type` — semantic shift to "fallback when `f`
  disconnected."
- `ClosureShapeEditor` — accept empty `param_names` for Custom kind.
- Validator — four new/updated checks per §"Validation".

**New from scratch:**
- `FunctionType::new` canonicalizing constructor + custom
  `Deserialize` routing through it.
- `canonicalize_data_type` walker + `canonicalize_network` driver.
- `APIApplyView` + `APIMapView` API types + Flutter widget glue.
- Property-panel skins for `apply` (connected `f`) and `map`
  (connected `f`).

**Deleted / removed:**
- Nothing. `ApplyData.kind` is kept as a fallback for the
  disconnected-`f` case; `MapData.output_type` is kept as the
  fallback for disconnected `f`. Eventual deprecations are polish
  follow-ups, not part of this branch.

## Implementation phases

Each phase ends with `cd rust && cargo test` green plus `cargo clippy`
clean; Phase 5 additionally ends with `flutter run` launching a
working editor. Phases are strictly sequential.

### Phase 1: Canonical `FunctionType` storage + load-time migration

**Goal.** Establish canonical storage. Every `FunctionType` in memory
is flat. `.cnnd` files load and normalize. Type comparison stays as
today's structural same-arity. Every existing wire continues to
validate.

**Scope.**
- `data_type.rs`:
  - Add `FunctionType::new(parameter_types, output_type) -> Self`
    that canonicalizes by absorbing nested `Function` returns.
  - Custom `Deserialize` impl (or `serde(from = "FunctionTypeRaw")`)
    routes deserialization through `new`.
  - Audit existing struct-literal construction sites and rewrite to
    `FunctionType::new`.
  - `can_be_converted_to`'s `Function` arm: leave as today's
    structural same-arity (or revert if a flatten-on-compare patch
    has been merged in advance).
- New module `canonicalize.rs` (or alongside `data_type.rs`)
  containing:
  - `canonicalize_data_type(&mut DataType)` — recursive walker.
  - `canonicalize_network(&mut SerializableNodeNetwork)` — driver
    that walks every stored DataType across all node-data variants
    and nested bodies, plus record type defs.
- `.cnnd` load path: call `canonicalize_network` on the loaded
  network before validation.

**Tests.** New unit tests in `rust/tests/structure_designer/`:
- `FunctionType::new` golden cases: nested `Function` in return →
  flat; already-flat → identity; mixed-depth nested.
- `canonicalize_data_type` recursion: nested `Function` inside
  `Iter`, inside `Array`, inside `Option`.
- `canonicalize_network` round-trip: build a fixture with a
  non-canonical `ClosureData.type_args[-1] = Function(..)`, run,
  verify the stored value is now flat. Same for `MapData.output_type`
  and record field types.
- Existing-fixture regression: every fixture in `rust/tests/fixtures/`
  loads, canonicalizes (no-op since they're already flat), produces
  the same in-memory network as today.
- `.cnnd` migration round-trip: load → save → load → no changes.

**Gotchas.**
- `Box<DataType>` recursion in the absorb loop: use `std::mem::replace`
  to mutate in place without cloning subtrees.
- The strict-no-broadcast variant no longer needs special handling
  beyond the same structural rule.
- If a test directly constructs `FunctionType { ... }` via struct
  literal, switch to `FunctionType::new`. Code review enforces
  `new`-only construction in new code.
- The walker must also recurse into nested HOF bodies (closure /
  apply nodes inside `MapZone` etc. inside a custom-network
  serialization). `canonicalize_network` driver enumerates these via
  `walk_all_nodes_mut`.

### Phase 2: `ZoneClosure.pre_supplied_args` — substrate-only

**Goal.** Add the field, plumb through `run_closure_once`, ensure
every constructor emits an empty value. No node yet produces a
non-empty `pre_supplied_args`, so the existing closure / HOF suite
passes byte-identically.

**Scope.**
- `evaluator/zone_closure.rs`:
  - Add `pre_supplied_args: Arc<Vec<NetworkResult>>` to `ZoneClosure`.
  - Update `build_inline_closure` and `build_node_function_closure`
    to set `pre_supplied_args: Arc::new(Vec::new())`.
  - Update `run_closure_once` to prepend.
  - Add `ZoneClosure::function_type()` returning
    `FunctionType::new(param_types.clone(), return_type.clone())` —
    the canonical (flat) declared type used by `apply` in Phase 3.
- Update any `ZoneClosure { … }` struct literals in tests.

**Tests.** Hand-construct a `ZoneClosure` with non-empty
`pre_supplied_args` and verify `run_closure_once` prepends correctly
against a synthetic two-param body. Existing closure / HOF suite is
the regression check.

**Gotchas.**
- Walker `Clone` independence (Invariant 2): `pre_supplied_args` is
  `Arc<Vec<…>>`, refcount-bump only.
- Serialization: `pre_supplied_args` is runtime-only
  (`NetworkResult` is not `Serialize`). Confirm by grep that no
  `Serialize` impl exists for `ZoneClosure`; if one does, add
  `#[serde(skip)]`.

### Phase 3: `apply` partial application + recursive consumption

**Goal.** The minimum viable partial-`apply`: derived arg-pin shape
from connected `f`, dynamic output pin type, partial/full dispatch
with recursive consumption in `eval`. Closes the §2 nested-body
correctness gap and §"`apply` semantics"'s 0-arity thunk case.

**Scope.**
- `nodes/apply.rs`:
  - `calculate_custom_node_type` — when `f`'s source's function type
    is resolvable, derive `params` / `ret` from the source's
    declared (canonical) type. Fall back to the kind picker when `f`
    is disconnected.
  - `eval` — implement the loop in §"`apply.eval`" with the
    identity-partial guard and recursive consumption.
  - `get_parameter_metadata` — mark arg pins optional.
- `network_validator.rs` — checks 1–3 per §"Validation".
- `node_type_registry.rs::repair_node_network` — extend wire-retention
  filter to handle "`f` source's function type changed ⇒ refresh
  `apply`'s arg-pin layout, disconnect stale wires past the new
  declared arity".

**Tests.** New file `rust/tests/structure_designer/currying_test.rs`:
- **Full apply unchanged:** `N = k`, behavior matches pre-branch.
- **One-arg partial:** 3-arg `g`, partial with `k=1` yields a 2-arg
  function. Wire into a second `apply` with `k=2` ⇒ full result.
- **Recursive consumption (§2 case):** A 1-arg closure whose body
  returns a 1-arg closure (canonical declared type: 2-arg). `apply(g,
  a, b)` with both args wired evaluates correctly via two loop
  iterations.
- **Identity partial:** `apply(g, …)` with `k=0` and declared arity
  > 0 returns `g` unchanged.
- **0-arity thunk:** A 0-arity closure forced by `apply(g)` with no
  arg pins runs the body and emits `R`.
- **Currying-equivalent acceptance** (post-Phase 1): an `apply`
  declaring `(A, B, C) → D` accepts a source whose authored type was
  `(A) → ((B, C) → D)` and is stored canonical as `(A, B, C) → D`.
- **Validation:** prefix-only rule rejects "arg0 unwired, arg1
  wired"; `f` disconnected still errors; arity-drift after a kind
  change is repaired by `repair_node_network`.
- **Walker clone independence:** a partially-applied closure flowing
  through a `map → collect`-and-`map → collect` fanout produces
  independent walkers.

**Gotchas.**
- **Resolving `f`'s declared type at `calculate_custom_node_type`
  time.** Mirror `record_destructure`'s "derive pin layout from
  connected schema" pattern.
- **`ApplyData.kind` becomes vestigial when `f` is connected.** Phase
  5 handles the visual side; in Phase 3, the editor still shows the
  kind picker until that work lands. That's fine — it just becomes a
  redundant control.
- **Pathological 0-arity body returning a Function and consuming
  args.** Each loop iteration must advance `f_current` if `n_body ==
  0`. Defensive assert: if `n_body == 0 && remaining.len() > 0` and
  the result is not a `Function`, error. The user must explicitly
  chain thunks for this case to arise — vanishingly rare.

### Phase 4: HOF auto-partialization on `map`

**Goal.** `map.f` accepts higher-arity sources via the starts-with
rule; `map.output_type` derives from `f` when connected. Headline
screenshot scenario works.

**Scope.**
- `nodes/map.rs::calculate_custom_node_type`:
  - When `f`'s source type is a `Function` whose `parameter_types`
    starts with `[element_type]`, derive `output_type` per §"HOF
    auto-partialization (`map`)".
  - When `f` is disconnected, fall back to stored
    `MapData.output_type` (today's behavior).
  - When `f`'s source is a `Function` whose first param does not
    match `element_type`, leave incompatibility for the validator.
- `map.f`'s connection-compatibility check (in `network_validator.rs`
  and any connect-time helper): use the starts-with rule instead of
  structural same-arity.
- `repair_node_network` — when `f`'s source's function type changes
  shape (or `xs`'s element type changes), recompute derived
  `output_type` and propagate downstream.
- `APIMapView`: populate `output_type_from_wired_f` and
  `effective_output_type` based on the derivation.

**Tests.** New tests in `currying_test.rs`:
- **Headline screenshot scenario:** `(Float, Float) → Int` source
  flows into `map.f` over `Iter[Float]`. `map.output_type` derives to
  `Function((Float,), Int)`. Each element yields a partially-applied
  closure carrying that element. A second `map(apply(_, y), …)`
  pass folds each one with a fixed `y`; results match a hand-computed
  reference.
- **Exact arity:** `(Float) → Int` flows in normally; output is
  `Iter[Int]`.
- **Mismatch:** `(Int, Float) → Bool` rejected for `map` over
  `Iter[Float]` (doesn't start with `[Float]`).
- **`f` disconnect:** stored `MapData.output_type` is restored when
  `f` is unwired.
- **`filter` / `fold` / `foreach` exact-arity unchanged:** regressions
  on existing fixtures.

**Gotchas.**
- The starts-with check uses *canonical* (flat) source type,
  guaranteed by Phase 1.
- When `map.f`'s source emits a partially-applied closure (its
  declared type already absorbed some args), the canonical type
  reflects the *remaining* params — starts-with is checked against
  those.
- `MapData.output_type` is read only when `f` is disconnected.
  Connecting `f` does not overwrite the stored value, so disconnect
  restores it cleanly.

### Phase 5: Editor (Flutter) — partial `apply`, derived `map` output, 0-arity Custom

**Goal.** Author and use partial `apply` end-to-end. Author and use
`map` auto-partial end-to-end. Author 0-arity Custom closures.

**Scope.**
- `APIApplyView` + `APIMapView` API types, populated by
  `build_node_view`. FRB-regenerated for Flutter.
- Flutter `apply` widget — read `APIApplyView`, render the read-only
  derived summary when `from_wired_f`, render
  `ClosureShapeEditor` when not.
- Flutter `map` widget — render derived `output_type` as read-only
  when `output_type_from_wired_f`, user-editable field otherwise.
  Disconnect restores the stored value into the field.
- `ClosureShapeEditor` — 0-arity Custom support: allow empty
  `param_names`, render `() → R` in titles. Add / remove buttons
  work down to zero rows.
- Pin colors / tooltips — output pin shows dynamic type; arg pins
  on `apply` gain the "unwired ⇒ deferred" tooltip.
- `flutter analyze` clean.

**Tests.** Manual walkthrough:
1. Place a `closure` of kind Custom, `param_names = ["x","y"]`,
   `type_args = [Float, Float, Float]`, body `expr: x+y`. Place an
   `apply` and wire the closure into `f`. Confirm two arg pins, type
   `Float` each.
2. Wire only `arg0` to a `float` literal. Output pin re-types to
   `Function((Float,), Float)`.
3. Wire that output into another `apply` with one arg wired ⇒ full
   result.
4. Disconnect `f` on the original apply — kind picker returns,
   declaring the intended shape works as before.
5. **Headline scenario:** Place a `closure` Custom with `param_names
   = ["x","y"]`, `type_args = [Float, Float, Int]`, body `expr:
   x*y`. Place a `map`, wire `range(3)` to `xs`, wire the closure
   directly to `f`. Confirm `map.output_type` becomes
   `Function((Float,), Int)` (read-only display, "derived from f"
   hint). Wire into `collect`, then into a downstream `map(apply(_,
   y), …)` chain; results match hand-computed values.
6. **0-arity Custom:** Place a `closure` Custom, remove all params
   via the editor, set return type `Float`, body `expr: 42.0`.
   Title shows `() → Float`. Place `apply` with `f` wired, no arg
   pins — forces the thunk to `42.0`.
7. Regression: existing closure / HOF networks load and evaluate
   unchanged.

**Verification.** `cd rust && cargo test` green; `flutter run`
launches; manual walkthrough passes.

**Gotchas.**
- **Showing the wired closure's pin names.** Propagate `param_names`
  from the wired source. When the source is a function pin
  (`output_pin_index == -1`) or a subnetwork's `Function` output,
  fall back to the node-type's parameter names.
- **Re-render trigger on wiring change.**
  `calculate_custom_node_type` re-runs when input wires change;
  verify the `apply` and `map` widgets update without an explicit
  user click.
- **0-arity Custom + `apply` with zero arg pins.** The editor must
  allow placing an `apply` whose `f` is wired to a 0-arity closure
  with zero arg pins rendered — and the output pin still shows the
  return type. The "no args wired" branch of `apply.eval` (k=0,
  declared=0) forces the thunk.
- **`map.output_type` field state restore.** On `f` disconnect, the
  editor must refresh the field's editor from the stored value (not
  show whatever derived value was last displayed). Mirror the
  existing pattern used for any node where input changes invalidate
  derived output.

### Out of phase plan (deferred)

- **Auto-partialization on a hypothetical future HOF with
  unconstrained output.** Adopt the `map` pattern verbatim.
- **`ApplyData.kind` deprecation.** Once the editor exclusively
  renders the derived shape and the kind picker becomes a niche
  default-shape-hint, retire the field entirely. Needs a one-version
  serialization migration.
- **`MapData.output_type` deprecation.** Same trajectory once `map`'s
  `f` pin is always-connected in practice (or once a "shape hint"
  affordance ships for the disconnected case).
- **`compose` / `flip` and other combinators.** Per
  `design_closures.md`'s deferred list; partial application makes
  these much easier to express but doesn't deliver them.

## Open questions

1. **Non-prefix wiring on `apply`.** Allow wiring `arg0` and `arg2`
   while `arg1` is empty? The result would be `Function((P_1,), R)`
   with `P_0 = a_0`, `P_2 = a_2`, `P_1` left to the caller. Requires
   tracking a "supplied mask" rather than just `k`, and re-indexing
   `ZoneInput` references. v1 stays prefix-only; revisit if users
   actually want the freedom.
2. **Disconnected-`f` shape on `apply`.** The kind picker is the v1
   fallback, slightly awkward — "doesn't really matter once you wire
   `f`." Alternative: show no pins at all until `f` is wired. Decide
   during Phase 5 UX work.
3. **`pre_supplied_args` deep-clone cost.** For large payloads (a
   `Crystal` or `Molecule` value pre-bound into a closure), the
   per-iteration `NetworkResult::clone` could matter. The payloads
   are already `Arc`-backed (`CrystalData`'s `atoms`,
   `geo_tree_root`), so clones are refcount bumps in practice — but
   a `Cow`-style "share read-only" path inside `run_closure_once`
   would be a clean optimization if profiling ever shows it.
4. **Cycle detection.** `Function`-typed values can already flow
   into captures, into `pre_supplied_args`, and through `ZoneInput`
   reads to downstream `apply` consumers — and today's evaluator
   does no cycle detection across that flow. This design adds one
   new value path (`pre_supplied_args`) but does not introduce a
   fundamentally new cycle vector: a `NetworkResult::Function`
   carried as a pre-bound arg is structurally identical to one
   carried as a capture. If a future recursion / fixed-point story
   for closures lands, cycle detection should be designed there.
   Defer.

## Phasing summary

| Phase | Outcome |
|---|---|
| 1 | Canonical `FunctionType` storage + `.cnnd` load-time normalization (no node behavior change) |
| 2 | `ZoneClosure.pre_supplied_args` substrate (no node yet produces a non-empty value) |
| 3 | `apply` partial-application: derived pins + dynamic output type + partial/full eval with recursive consumption |
| 4 | `map` auto-partialization: starts-with `f` rule, derived `output_type` (headline screenshot works) |
| 5 | Editor surface: `APIApplyView` / `APIMapView`, derived-shape rendering, 0-arity Custom |

Each phase's exit gate is the same as elsewhere in the project: `cd
rust && cargo test` green plus (Phase 5) `flutter run` launching a
working editor. The user-visible payoff lands fully at Phase 4 (the
headline screenshot scenario evaluates correctly with a direct
closure-to-`map.f` wire); Phase 5 is the ergonomic polish that makes
it the obvious tool.
