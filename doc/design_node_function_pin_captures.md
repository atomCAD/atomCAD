# Node Function Pin From Free Args: Capture the Connected Pins

## Context

This doc builds on the zones → closures → currying → function-pin-unification
arc. It changes one thing about how a node's **function pin** (the upper-right
`-1` output) behaves, and removes the `.cnnd` migration that the change makes
redundant.

Today, on the `zones` branch, a node's `-1` pin is synthesized by
`build_node_function_closure` (`evaluator/zone_closure.rs:226`) as a function of
**all** the node's declared input pins, regardless of their wiring. A
companion **function-mode mutual-exclusion** rule enforces that a node whose
`-1` pin is consumed must have **every** input pin unwired
(`node_network.rs::can_connect_nodes` gate + a `network_validator` rule):
"every input is a parameter, so a wired input would be a dead wire the
synthesizer never reads."

That rule is the reason a `main`-branch file cannot load directly. On `main`
(the old `FunctionEvaluator` world), wiring a node's `-1` pin into an HOF's `f`
pin with **some inputs wired** was the normal idiom — the wired inputs were
*captures*, baked in at evaluation. The convention was **parameters first
(unwired), captures last (wired)**: the source's first `K` (the HOF's arity)
pins were the per-call parameters; the trailing pins were captures. The
`migrate_v4_to_v5` pass (`serialization/migrate_v4_to_v5.rs`) exists purely to
rewrite that topology into a synthesized `closure` node so the zones branch can
swallow it.

This design makes the `-1` pin reflect the node's **actual wiring** instead of
just its declaration:

> **A node's function pin produces a `Function` whose parameters are the node's
> _unconnected_ input pins, with the _connected_ input pins frozen as
> captures.** The output pin's type is the node's return type.

This is precisely main's old semantics, generalized (captures may sit at any
pin position, not only the trailing ones) and made **visible** — captures are
just the node's ordinary, named, wired input pins. It also makes a
custom-network instance node and a `closure` node two surfaces of one idea (a
function with some positions bound), so they are non-disruptively
interconvertible. And because main's "parameters-first/unwired,
captures-last/wired" files satisfy the new rule by construction, they load and
evaluate **directly** — so `migrate_v4_to_v5` is deleted.

### What this design deliberately does NOT do

- It does **not** change how `closure` nodes capture values. A `closure`
  node's captures remain **boundary-crossing wires** (`source_scope_depth ≥ 1`),
  exactly as zones/closures shipped them — they are **not** turned into named
  input pins on the `closure` node. Mark's call request ("even closure captures
  displayed as named input pins") is **not** implemented here. The
  captures-as-named-pins effect appears *only* on ordinary nodes' `-1` pin, and
  there only because a normal node's wired inputs already *are* named pins — no
  new rendering. The "unification" framing below is **conceptual** (a
  node-with-wired-inputs and a `closure` node are the same idea — a function
  with some positions bound); this design does not act on that equivalence. The
  promote/demote-to-inline gesture and named closure-capture pins stay deferred,
  as in `design_closures.md`.
- It does **not** make `apply`'s arg pins non-prefix ("any-capture on
  `apply`"). That is a separate, speculatively-motivated change requiring a new
  positional hole-mask carrier; deferred. (`apply` keeps its contiguous-prefix
  rule and `pre_supplied_args` exactly as currying shipped them.)
- It does **not** touch `map`/`filter`/`fold`/`foreach` `eval`, `obtain_closure`,
  `run_closure_once`, the walker, the scope-stack, or `apply.eval`. The touched
  surface is: how the `-1` pin's type and closure are synthesized
  (`build_node_function_closure` — gaining a `context` param — and
  `resolve_output_type`'s `-1` arm); the removal of two connection gates and one
  validator rule; new `function_pin_consumed`-keyed revalidation triggers in
  `connect_nodes` / `delete_selected_scoped`; and the editor's
  `NodeView.function_type` computation. See the §Reuse map for the authoritative
  touch-point list.

## Build/test contract

Per project convention: `cd rust && cargo test` green and `cargo clippy` clean
for every Rust phase. `flutter_rust_bridge_codegen generate` succeeds. The
editor phase additionally requires `flutter analyze` clean and `flutter run`
launching.

| Must pass | When |
|---|---|
| `rust/` workspace (`cargo build`, `cargo clippy`, `cargo test`) | every phase |
| Existing zones/closures/currying/HOF tests, re-grounded where they asserted the old mutual-exclusion rule | Phase 1 |
| A representative `main`-branch v4 function-pin-with-captures fixture **loads and evaluates** correctly | Phase 2 |
| `flutter run` launches; a node with wired captures feeding `map.f` authors and evaluates end-to-end | Phase 3 |

The branch baseline: the full Rust suite (3400+ tests) green; the `-1` pin
synthesizing an all-params closure; `migrate_v4_to_v5` active at
`SERIALIZATION_VERSION = 5`.

## Motivation

Two concrete payoffs:

1. **Main-branch files just work.** A `sphere(radius, smoothness) → Geometry`
   with `smoothness` wired to a constant and its `-1` pin feeding `map.f` over
   `Iter[Float]` evaluates to `Iter[Geometry]` directly — `radius` is the
   per-element parameter, `smoothness` is the frozen capture. This is what the
   `FunctionEvaluator` did; the user ("Mark") asked to keep it.

2. **It closes the silent-convention UX hole** that zones was built to fix.
   The old hot-vs-captured distinction was invisible. Now the captured values
   are the node's ordinary wired input pins, drawn and named like any other
   wire; the parameters are the unwired pins. There is nothing hidden.

A third, structural payoff: a custom-network instance with some inputs wired,
and a `closure` node, become the same concept ("a function with some positions
bound"), which makes a future promote/demote-to-inline gesture a representation
swap rather than graph surgery.

## Concept

### The rule

For a node `N` of type `(P0, P1, …, P_{n-1}) → R`:

| `N`'s input pin `i` | role under the `-1` pin |
|---|---|
| **wired** (has incoming wire) | **capture** — pre-evaluated once at the `-1` pin's eval, frozen |
| **unwired** | **parameter** — a zone-input of the synthesized closure, in pin order |

- The `-1` pin's **declared/resolved type** is
  `Function((types of the unwired pins, in pin order), R)`.
- The synthesized `ZoneClosure`'s `param_types` = unwired pin types;
  `captures` = the wired pins' upstream values, pre-evaluated.
- If **all** inputs are wired, the function is a nullary thunk `() → R` (valid;
  forceable by `apply`, the same shape as currying's 0-arity `Custom` closure).
- If **no** inputs are wired, the result is identical to today (function of all
  params) — so this change is conservative on existing zones-branch networks,
  where the mutual-exclusion rule guaranteed exactly that case.

### Worked example (the headline)

`sphere(radius: Float, smoothness: Float) → Geometry`, `smoothness ← float(0.1)`,
`sphere.-1 → map.f`, `xs ← range(3): Iter[Float]`:

- *Contrast case* — if `smoothness` were left **unwired** → `-1` resolves to
  `(Float, Float) → Geometry`; `map` derives `Iter[Function((Float,), Geometry)]`
  (a stream of partials — **today's behavior, unchanged**).
- The setup above (`smoothness` **wired** to `float(0.1)`) → `-1` resolves to
  `(Float) → Geometry` (smoothness captured); `map.f` is
  `AnyFunction { leading_params: [Float] }`, tail empty → `map` output
  `Iter[Geometry]`. **No change to `map` code** — the narrower type flows out of
  the `-1` pin and `map`'s existing derive-from-`f` post-pass picks it up.

The change sits entirely **upstream** of every consumer, which is why no
consumer (`map`, `filter`, `fold`, `foreach`, `apply`) needs editing.

### Why this subsumes the migration

`migrate_v4_to_v5` reads a main file's source node, partitions its pins into
parameters (first `K`, unwired) and captures (trailing, wired), and synthesizes
a `closure` node whose body clones the source with parameters wired to
`ZoneInput` pins and captures forwarded at `source_scope_depth = 1`. That is
**exactly** what the new `build_node_function_closure` does — at runtime, from
the live node, instead of as a load-time JSON transform. The migration was
doing statically what the synthesizer now does dynamically. So:

- Main's `NoOp` shape (source has exactly `K` unwired inputs, no captures) — was
  already handled by today's synthesizer; still handled.
- Main's `ClosureWrap` shape (parameters-first/unwired, captures-last/wired) —
  now handled directly by the partition; **no closure synthesis needed**.
- Main's `Skip` shapes (e.g. "wired pin after an unwired pin") — under the new
  model these are simply **valid** (non-prefix captures) and load with sensible
  semantics, or surface as an ordinary `AnyFunction` type mismatch.

The independent `argument_output_pins → incoming_wires` format conversion lives
in the custom `Argument` deserializer (`node_network.rs:290-334`), **not** in
the migration, so it is unaffected by the migration's removal.

## Data model

No data-model changes. `ZoneClosure` (`evaluator/zone_closure.rs:42`),
`IncomingWire` / `SourcePin` / `CaptureKey`, `DataType::Function` /
`AnyFunction`, and the serialized `.cnnd` shape are all unchanged. The change is
in how existing structures are *populated* from a node.

## Evaluator changes

### `build_node_function_closure` — partition wired vs unwired

Today (`zone_closure.rs:226-301`) it maps **all** `node_type.parameters` to
`ZoneInput` pins and sets `captures = empty`. The rewrite:

1. Read `N`'s `arguments`. Partition pin indices into `wired` (non-empty
   `incoming_wires`) and `unwired`.
2. **Parameters** = `unwired`, in ascending pin order. `param_types[j]` =
   `node_type.parameters[unwired[j]].data_type`. The synthesized body node's
   `arguments[unwired[j]]` reads `ZoneInput { pin_index: j }` at
   `source_scope_depth: 1` (note: the zone-input index `j` is the *parameter*
   index, not the original pin index — parameters are renumbered densely).
3. **Captures** = `wired`. The body node's `arguments[i]` (for wired `i`)
   forwards `N`'s original incoming wire(s) for pin `i`, rebased to
   `source_scope_depth + 1` so they resolve against the parent scope from inside
   the synthesized body. These are ordinary capture wires.
4. Build the one-node body (clone of `N`, body-local id `1`) exactly as today,
   then **pre-evaluate the captures** by reusing the existing capture pipeline
   (`build_captures`, `zone_closure.rs:427`) against the body pushed onto the
   current `network_stack` — mirroring `build_inline_closure`. (Today
   `build_node_function_closure` skips this because it has no captures; now it
   needs it.)
5. `return_type = node_type.output_type()`. Drop the current
   `param_types.is_empty()` → error: a fully-captured node is a legal `() → R`
   thunk.

The result is structurally `build_inline_closure` specialized to a synthesized
one-node body whose parameters are the unwired pins and whose captures are the
wired pins. Reuse `build_captures` / `run_closure_once` / the walker verbatim —
no new evaluation machinery.

The `output_pin_index == -1` dispatch branch (`network_evaluator.rs:1560-1564`)
keeps the same shape — it calls `build_node_function_closure` and wraps the
result as `NetworkResult::Function`. The one mechanical change: because the
synthesizer must now pre-evaluate captures (step 4), `build_node_function_closure`
gains a `context: &mut NetworkEvaluationContext` parameter (the current
signature takes only `_evaluator`, which it ignores), and the dispatch branch
passes the in-scope `context` through. This mirrors `build_inline_closure`'s
signature.

### Capture-freeze timing

Captures freeze when the `-1` pin is evaluated — i.e. when the consuming HOF/
`apply` resolves its `f` arg via `obtain_closure`. This is the same
once-per-HOF-invocation timing as an inline body's captures, so a `-1`-sourced
function nested inside an outer `fold` body re-freezes per outer iteration,
identical to an equivalent inline body. No new semantics.

## Type system

### Wiring-aware `-1` pin type

`NodeType::get_function_type` (`node_type.rs:217`) builds a function type from
**all** `parameters`. The `-1` pin's type must instead be built from the
**unwired** parameters of the *specific node instance*. Since a `NodeType`
doesn't know an instance's wiring, the wiring-aware computation moves to
`NodeTypeRegistry::resolve_output_type`, which already receives
`(source_node, network, output_pin_index)` and is the single point consulted by
both `can_connect_nodes` and `validate_wires`:

```text
resolve_output_type(node, network, -1):
    let unwired = node.arguments indices with empty incoming_wires
    let params  = unwired.map(|i| resolved type of node's param i)
    Function(FunctionType::new(params, node's resolved output_type()))
```

`FunctionType::new` canonicalizes as usual. Because both the connect-time gate
and `validate_wires` go through `resolve_output_type`, the wiring-aware type is
consistent everywhere with no separate post-pass *to compute the `-1` pin's own
type* (the `-1` type is derived on demand, unlike the `map`/`apply` consumer
types, which are cached in `custom_node_type` and rewritten by their post-passes
— see the propagation note below). (Param-type resolution for an unwired pin
uses the node's declared param type; polymorphic-input source nodes are an edge
case — see Open Questions.)

This makes the `-1` pin's type **wire-state-dependent**: wiring/unwiring an
input on `N` changes the function it exposes, which must re-propagate to the
consumer (re-derive `map`/`apply` output type, re-validate `map.f`/`apply.f`).
The re-derivation machinery itself is reused unchanged — the `map`/`apply`
post-passes (`update_map_pin_layouts_for_network` /
`update_apply_pin_layouts_for_network`) already resolve the `f`-source type via
`resolve_output_type` and rewrite the consumer's `custom_node_type`, and they
re-run on every `validate_active_network` pass. But **the trigger that decides
whether to validate must be extended** — see §Validation & connection gating
("Revalidation triggers"). The existing triggers fire on the *consumer* side
(dest is an `f`/`apply` pin) or a `-1` *source*; the new edit is on `N`'s
*ordinary input pin*, so without a new trigger the partial refresh runs without
validating and the consumer's derived type goes stale.

Note this makes the `-1` arm wiring-aware for the two **backend** consumers that
route through `resolve_output_type` — `can_connect_nodes` and `validate_wires`
— but **not** the editor's *displayed* pin type, which is precomputed separately
into `NodeView.function_type` via `get_function_type()`. That display path must
be redirected too; see §Editor changes ("Required: make the displayed `-1` pin
type wiring-aware").

### Compatibility

No change to `can_be_converted_to`. A concrete `Function((unwired…), R)` flows
into `map.f`/`apply.f` (`AnyFunction { … }`) through the existing Phase-A rule.
For a main file with parameters-first, the first unwired pin's type is the
element type, so `map.f`'s `leading_params: [element_type]` starts-with check
passes exactly as it did via the migration's synthesized closure.

## Validation & connection gating

Three removals (all in service of "wired inputs on a function-consumed node are
now legal captures, not dead wires"):

1. **`can_connect_nodes`** (`node_network.rs:1190-1256`): remove the gate
   `if source_output_pin_index == -1 && source_node.has_any_wired_input_pin() { return false; }`.
   Wiring a `-1` pin no longer requires all inputs unwired.
2. **`can_connect_nodes`**: remove the dual gate
   `if self.function_pin_consumed(dest_node_id) { return false; }`. Wiring an
   input pin on a node whose `-1` is consumed is now legal — it adds a capture
   (removes a parameter), changing the function's arity, which re-derives on the
   next validate pass.
3. **`network_validator`** (the function-mode mutual-exclusion rule,
   ~`network_validator.rs:791-799`): remove the
   "all input pins must be left disconnected" error.

**Kept unchanged:**

- `function_pin_consumed` (`node_network.rs:1178`) — still the derived
  "function mode" predicate. Still drives the `generate_scene` skip
  (`network_evaluator.rs:518`, a node consumed purely as a function isn't
  rendered) and the Flutter eye-disable. A node feeding its `-1` pin normally
  has unwired parameters, so its pin-0 value isn't independently valid anyway
  (missing required input); the skip stays correct. (A fully-captured `() → R`
  thunk whose pin-0 is also determined would be skipped from the scene — an
  acceptable edge.)
- `repair_output_pin_wires`'s `-1` guard (`network_validator.rs:298-336`) —
  still preserves `-1` wires across validation.
- `validate_wires`' type-check of a `-1` source — now type-checks against the
  wiring-aware function type via `resolve_output_type` (free, no code change).

After removal, audit `Node::has_any_wired_input_pin` for remaining callers; if
the removed gate/rule were its only users, delete it.

### Revalidation triggers (new — the wire-state propagation gap)

Because the `-1` pin's type is now wire-state-dependent, editing an **ordinary
input pin of a function-consumed node `N`** must re-derive the downstream
consumer's type. The re-derivation runs inside `validate_active_network` (the
`map`/`apply` post-passes), but `refresh_partial`/`refresh_full` deliberately
do **not** validate — a mutator must opt in. The existing opt-in flags
(`connect_nodes`' `revalidate`, `delete_selected_scoped`'s `should_validate`)
key on the *consumer* (`dest_is_function_pin` / `dest_is_apply`) or a `-1`
*source*. None of those match when the edited wire's source is an ordinary
output pin (`≥ 0`) and its destination is `N`'s value pin — so today's flags
leave the consumer's derived type (and the now-wiring-aware displayed
`function_type`) **stale**. This is the direct analog of the earlier
"apply wire-delete refresh" fix, but keyed on the *source node being edited*
rather than the consumer.

Add one trigger per top-level path, keyed on `function_pin_consumed`:

- `connect_nodes` (`structure_designer.rs:~2186`): extend `revalidate` with
  `|| network.function_pin_consumed(dest_node_id)` — wiring an input on a node
  whose `-1` is consumed changes the exposed arity.
- `delete_selected_scoped`, wire-selection branch (`~4533`): set
  `should_validate` when `network.function_pin_consumed(wire.destination_node_id)`
  — deleting an input wire on such a node restores a parameter.
- `delete_selected_scoped`, node-selection branch: set `should_validate` when
  any node losing an input to the deletion has `function_pin_consumed` (deleting
  a capture *source*, e.g. the `float` feeding `N.smoothness`, also retypes
  `N.-1`). The connected/dirty set already enumerated there is the natural place
  to test this.

**No gap** on the scoped/body paths (`connect_nodes_scoped` /
`delete_selected_scoped` with a non-empty `scope_path` already validate
unconditionally) or on `.cnnd` / text-format loads (full validate). **Verify**
that undo/redo of these wire edits (`ConnectWireCommand` and the delete
commands) use a refresh mode that revalidates — otherwise undo reintroduces the
same staleness.

No caching change is needed: `ValidationContext.resolved_outputs` is built fresh
per validate pass, and the consumer's `custom_node_type` is rewritten by the
post-pass on every validate.

## Migration changes

### Delete `migrate_v4_to_v5`

The closure-synthesis migration is now redundant (the synthesizer does it at
runtime) and undesirable (it would rewrite the node-with-function-pin topology
we now want to preserve as the canonical form). Remove:

- `serialization/migrate_v4_to_v5.rs` (the module).
- Its dispatch line in `node_networks_serialization.rs` (the
  `if version < 5 { … migrate_v4_to_v5 … }` arm, ~line 786-829).
- The `migrate_v4_to_v5` test module and the migration-specific fixtures under
  `rust/tests/fixtures/zones_migration/` that assert closure synthesis /
  orphan-cleanup / skip behavior (`fanout_creates_two_closures`,
  `source_cleanup_*`, `bad_wired_after_unwired`, `bad_too_few_inputs`, etc.).
- Its entry in `serialization/AGENTS.md` and the AGENTS migration table.

### Version handling — recommended: keep `SERIALIZATION_VERSION = 5`

Keep the constant at 5 and let the existing
`if version < SERIALIZATION_VERSION { bump version field to 5 }` step handle v4
files: they get their in-memory version bumped to 5 with **no transform pass**,
the custom `Argument` deserializer converts the wire format, and the new
synthesizer handles the function-pin semantics. v5 files (any stray dev files)
still load (no transform). This keeps the version monotonic and avoids the
"Unsupported version" rejection that dropping to 4 would inflict on any existing
v5 file.

> *Alternative considered:* drop `SERIALIZATION_VERSION` back to 4 and delete
> the v4→v5 dispatch entirely. Cleaner conceptually (no phantom version), but
> any v5 file produced during zones-branch development would then fail the
> future-version guard (`version > SERIALIZATION_VERSION` → error). Since the
> user confirms no user `.cnnd` files were saved on the zones branch, this is
> safe in principle, but the monotonic option above is lower-risk. Pick during
> implementation.

### Repurpose the good fixtures as load-and-evaluate regressions

The capture fixtures that represent **real main-branch files**
(`simple_map_with_capture`, `simple_filter_with_capture`,
`simple_fold_with_capture`, `simple_foreach_with_capture`, and the
custom-network / HOF-source variants that are well-formed) are exactly the
regression coverage we want for "main files still work." Convert their tests
from "assert a closure node was synthesized" to "load the v4 fixture, evaluate,
assert the correct result" — proving the new synthesizer reproduces main's
semantics end-to-end.

## Editor (Flutter) changes

Captures themselves render for free — on a normal node they are its ordinary
wired input pins, already drawn and named. But the **type shown on the `-1` pin
is a separate, required change** (see below); without it the pin would still
advertise the full all-parameters signature, hiding the very capture/parameter
split this design exists to surface.

### Required: make the displayed `-1` pin type wiring-aware

The `-1` pin's type is **not** read from `resolve_output_type` on the Flutter
side. It is precomputed into the `NodeView.function_type` field
(`structure_designer_api.rs:606`) as `node_type.get_function_type()` — the
**all-parameters** type, blind to wiring — and that string is what flows to the
function-pin `PinWidget` (`node_widget.dart`, `dataType: node.functionType`) and
the `PinKind.functionPin` arm in `scope_resolver.dart`, i.e. it is exactly what
the **hover tooltip** displays.

So the backend's wiring-aware `resolve_output_type(-1)` arm (Phase 1) is *not*
enough to fix the displayed/hover type — it never reaches this field. The fix is
to compute `function_type` through the same wiring-aware path the validator and
connect-gate use:

```rust
// structure_designer_api.rs, replacing `node_type.get_function_type()`
let function_type = cad_instance
    .structure_designer
    .node_type_registry
    .resolve_output_type(node, node_network, -1)
    .unwrap_or_else(|| node_type.get_function_type()); // fallback if unresolved
```

`node`, `node_network`, and the registry are already in scope at that call site
(the per-result-pin loop just below uses `resolve_output_type_detailed(node,
node_network, i)`). With this, hovering a `-1` pin shows
`Function((unwired pin types), R)` — the captured (wired) pins drop out of the
signature, matching what the node actually produces. **No new API types**
(`function_type` already exists on `NodeView`); only its computation changes.

### Verification (no code change)

- A node whose `-1` pin is consumed (`NodeView.function_pin_consumed`) is
  skipped from scene generation but is still drawn in the editor with its input
  wires; confirm its wired (capture) input pins and unwired (parameter) input
  pins both render, and the eye stays disabled. This is existing behavior; just
  verify it under the new "wired inputs allowed" state.

### Optional polish

- A `-1`-pin tooltip line reading "function of the unwired inputs; wired inputs
  are captured," and/or a subtle visual marker distinguishing parameter pins
  (unwired) from capture pins (wired). Not required for correctness.

## Reuse map (summary)

**Reused unchanged:** `obtain_closure`, `build_inline_closure`,
`run_closure_once`, `build_captures`, `CaptureKey` / `IncomingWire` /
`SourcePin`, the scope-stack, the walker, `apply.eval`, all four HOF `eval`s,
`map`/`apply` post-passes, the custom `Argument` deserializer, the
`output_pin_index == -1` dispatch branch, `function_pin_consumed`, the
`generate_scene` skip, `repair_output_pin_wires`.

**Reused with small changes:** `build_node_function_closure` (partition
wired→captures / unwired→params; pre-evaluate captures; allow 0 params; gains a
`context` param for capture pre-eval); `resolve_output_type`'s `-1` arm
(wiring-aware function type); `NodeView.function_type` computation
(`structure_designer_api.rs:606`) — routed through `resolve_output_type(-1)` so
the **displayed/hover** type is wiring-aware too; `connect_nodes`' `revalidate`
flag and `delete_selected_scoped`'s `should_validate` flag — new
`function_pin_consumed`-keyed triggers (see §Validation & connection gating,
"Revalidation triggers").

**Removed:** the two `can_connect_nodes` function-mode gates; the
`network_validator` mutual-exclusion rule; `migrate_v4_to_v5` (module, dispatch
arm, tests, synthesis-specific fixtures); possibly `has_any_wired_input_pin` if
now unused.

**New from scratch:** nothing structural — only re-grounded tests and
repurposed fixtures.

## Implementation phases

Each phase ends with `cd rust && cargo test` green and `cargo clippy` clean;
Phase 3 additionally with `flutter run` launching.

### Phase 1: The model — wiring-aware `-1` pin (type + closure + gating)

These three are interdependent (you cannot allow the connection without the
type being right, and the closure must match), so they land together.

**Scope.**
- `evaluator/zone_closure.rs::build_node_function_closure` — partition; route
  unwired→`ZoneInput` (densely renumbered), wired→capture wires
  (`source_scope_depth + 1`); pre-evaluate captures via `build_captures`; allow
  empty params (thunk); keep the polymorphic-output error.
- `node_type_registry.rs::resolve_output_type` — `-1` arm builds
  `Function((unwired param types), output_type())` from the node instance.
- `node_network.rs::can_connect_nodes` — remove both function-mode gates.
- `network_validator.rs` — remove the mutual-exclusion rule.
- `structure_designer.rs::connect_nodes` / `delete_selected_scoped` — add the
  `function_pin_consumed`-keyed revalidation triggers (see §Validation &
  connection gating, "Revalidation triggers"). Without these the consumer's
  derived type does not re-propagate when an input pin of a function-consumed
  node is wired/unwired.

**Tests.** New cases in the zones/closures test files:
- A synthesized source with one wired (capture) and one unwired (param) input,
  `-1` → `map.f` over a stream, evaluates with the capture frozen.
- Capture-freeze timing: the captured upstream value reflects its
  value-at-`-1`-eval; nested under a `fold`, re-freezes per outer iteration.
- All-wired source → `() → R` thunk forced via `apply` returns `R`.
- No-wired source → unchanged from today (function of all params; `map`
  produces the partial-stream result).
- **Propagation:** with `sphere.-1 → map.f`, wire `sphere.smoothness` and assert
  `map`'s resolved output type flips `Iter[Function((Float,),Geometry)]` →
  `Iter[Geometry]` *without* any edit at `map`; then delete that input wire and
  assert it flips back. (Guards the new revalidation triggers — this fails today
  because `connect_nodes`/`delete` don't validate on an ordinary-input edit.)
- Re-ground any existing test that asserted "wiring a `-1` source with wired
  inputs is rejected" — that connection is now valid.
- Walker clone independence for a `-1`-sourced closure fanned into two consumers.

**Gotchas.**
- Parameter renumbering: zone-input pin index is the dense parameter index
  (0..#unwired), not the original pin index. The body node's wired-pin arguments
  use `ZoneInput { pin_index: param_index }` for unwired pins and capture wires
  for wired pins — keep the two index spaces straight.
- Capture wires must be rebased `+1` in scope depth (parent-relative from inside
  the synthesized body), then pre-evaluated against `network_stack + body` —
  mirror `build_inline_closure` exactly.
- `resolve_output_type` and `validate_wires`/`can_connect_nodes` must agree;
  routing the `-1` type through `resolve_output_type` guarantees this.

### Phase 2: Remove `migrate_v4_to_v5`; repurpose fixtures

**Scope.**
- Delete the module, the dispatch arm, and the synthesis-specific tests/
  fixtures. Keep `SERIALIZATION_VERSION = 5` (recommended) with no v4→v5
  transform.
- Convert the well-formed capture fixtures into load-and-evaluate regression
  tests asserting correct results (not closure synthesis).
- Update `serialization/AGENTS.md` and the migration table.

**Tests.** The repurposed fixtures load + evaluate to hand-computed reference
values. A v4 fixture with a non-prefix capture (previously a `Skip` case) now
loads and evaluates correctly (or surfaces a clean `AnyFunction` type error if
genuinely ill-typed). Version dispatch: a v4 file loads with no transform; a v5
file loads unchanged.

**Gotchas.**
- Confirm no other code references `migrate_v4_to_v5::HOF_F_PINS_V5` or the
  module's helpers before deleting.
- `migrate_v3_to_v4` and `migrate_v2_to_v3` are untouched — a v2/v3 file still
  chains up to v5 (its function-pin wires, if any, are now handled by the
  synthesizer rather than rewritten).

### Phase 3: Editor verification + polish

**Scope.**
- **Required:** redirect `NodeView.function_type`
  (`structure_designer_api.rs:606`) from `node_type.get_function_type()` to
  `resolve_output_type(node, node_network, -1)` (fallback to
  `get_function_type()` if unresolved), so the displayed/hover `-1` pin type is
  wiring-aware. This is the deliverable that makes hovering a `-1` pin show
  `Function((unwired…), R)` with captures excluded. FRB regen (no new API
  types — `function_type` already exists).
- Verify a function-consumed node renders its wired (capture) and unwired
  (parameter) input pins and keeps the eye disabled, now that wired inputs are
  permitted.
- Optional `-1`-pin tooltip / parameter-vs-capture visual marker.

**Tests.** Manual walkthrough: place `sphere`, wire `smoothness ← float(0.1)`,
drag `sphere.-1 → map.f`, feed `range`, display a downstream `collect`, confirm
`Iter[Geometry]`. **Hover `sphere`'s `-1` pin and confirm the tooltip reads
`(Float) → Geometry`** — the captured `smoothness` is gone from the signature,
not `(Float, Float) → Geometry`. Disconnect `smoothness`; confirm the `-1`
tooltip reverts to `(Float, Float) → Geometry` and `map` reverts to the
partial-stream output. Load a main-branch `.cnnd` with a captured function pin;
confirm it renders and evaluates.

## Open questions

1. **Polymorphic-input source nodes.** If `N` has an abstract/polymorphic input
   pin left unwired, its parameter type may be abstract and the `-1` function
   type may not resolve concretely. v1: build from the declared param type and
   let the existing `resolve_output_type`-returns-`None` path reject the
   connection until resolvable, matching today's treatment of unresolved
   polymorphic outputs. Revisit if a real source needs it.

2. **Pin-position hot-slot for `map`.** The first *unwired* pin is the element
   slot. This is visible (the user sees which pins are wired) but still
   positional. For main files (parameters-first) it coincides with pin 0.
   A future affordance to choose the hot slot is out of scope.

3. **Dropping vs keeping version 5.** Recommended: keep at 5 with no transform.
   Decide during Phase 2 (see §Migration changes).

4. **Any-capture on `apply`.** Deliberately deferred (see §Context). Designed
   but not built; would unify `apply`'s partial mechanism with this one via a
   positional hole-mask carrier. Gate on a concrete use case.
