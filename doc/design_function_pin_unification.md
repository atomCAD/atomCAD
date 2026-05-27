# Function Pin Unification: `AnyFunction` + Phase 5

## Context

This doc builds on `doc/design_currying.md` (Phases 1-4 shipped). It
retrofits `apply.f` and `map.f` to a single first-class concept and
then delivers the editor work that was originally scoped as Phase 5 of
`design_currying.md` — now built on a cleaner foundation.

The motivating observation: after Phases 1-4 landed, both `apply.f`
and `map.f` had accumulated enough special-case handling that they
were no longer "typed pins" in any meaningful sense. Each individual
exception was justified locally; the sum looked like a concept the
type system wasn't modeling.

### Accumulated specialness as of Phase 4

`apply.f`:
- Validation post-pass (`update_apply_pin_layouts_for_network`) rewrites
  the pin's declared `Function(...)` type from the wired source on
  every validate pass.
- Connect-time check passes only because the declared type was already
  rewritten by a previous post-pass run.
- Output pin type is `k`-dependent (partial vs full).
- Required-pin rule; contiguous-prefix rule.
- Eval has a recursive consumption loop for body-arity-vs-declared-arity
  mismatch.
- Hover/tooltip needs to mean "any function shape" once Phase D drops
  the kind picker UI.

`map.f`:
- Validation post-pass (`update_map_pin_layouts_for_network`) overrides
  declared pin type from wired source when the source's params start
  with `[element_type]`.
- Connect-time has a name-matched exception
  (`if dest_node.node_type_name == "map" && dest_param_index == 1`)
  implementing the starts-with rule.
- Output pin type derives from the wired source.

Two pins, two sets of overlapping exceptions, both reachable only by
name-matching the node type. The unifying concept: a pin whose
declared type is "a function value, optionally constrained by a
required prefix on its parameter list."

## Concept

### `DataType::AnyFunction { leading_params }`

A single new `DataType` variant:

```rust
pub enum DataType {
    // ... existing variants ...
    /// A pin that accepts any `Function(_)` value whose parameter list
    /// starts with `leading_params`. An empty `leading_params` accepts
    /// any function regardless of shape (used by `apply.f`); a
    /// non-empty `leading_params` enforces a prefix constraint (used
    /// by `map.f` to require the first param matches `element_type`).
    ///
    /// `AnyFunction` is an INPUT-PIN-ONLY type. Sources never resolve
    /// to `AnyFunction` — every concrete `Function` value carries a
    /// fully-specified `FunctionType`. The validator rejects
    /// `AnyFunction` appearing as a source output type.
    AnyFunction {
        leading_params: Vec<DataType>,
    },
}
```

`apply.f`'s declared type becomes `AnyFunction { leading_params: vec![] }`.
`map.f`'s declared type becomes `AnyFunction { leading_params: vec![element_type] }`.

### Compatibility rule

In `DataType::can_be_converted_to`:

```rust
if let (DataType::Function(src_ft), DataType::AnyFunction { leading_params })
    = (source_type, dest_type)
{
    if src_ft.parameter_types.len() < leading_params.len() {
        return false;
    }
    for (src_param, dest_param) in src_ft
        .parameter_types
        .iter()
        .zip(leading_params.iter())
    {
        if !DataType::can_be_converted_to(src_param, dest_param, registry) {
            return false;
        }
    }
    return true;
}
```

Direction is one-way: concrete `Function(_)` flows INTO `AnyFunction`,
not the reverse. The strict-no-broadcast variant uses the same rule
(no broadcast involved in the function-into-AnyFunction case).

Note: `leading_params` empty makes the parameter-prefix loop trivially
pass, so `apply.f`'s "any function" check falls out of the same code
path as `map.f`'s starts-with — no separate arm needed.

### What `AnyFunction` replaces

**Post-pass type rewriting on `apply.f` and `map.f`** (in
`update_apply_pin_layouts_for_network` and
`update_map_pin_layouts_for_network`): both currently rewrite the
f-pin's declared `Function(...)` type from the wired source on every
validate pass. After this branch, the f-pin's declared type is
*permanent*:

| Pin | Declared type (always) |
|---|---|
| `apply.f` | `AnyFunction { leading_params: vec![] }` |
| `map.f` | `AnyFunction { leading_params: vec![element_type] }` |

The post-passes still compute the arg-pin LAYOUT on apply (count,
names, types from the wired source) and the OUTPUT pin type on both
nodes (partial vs full on apply; `Iter[derived]` on map) — those are
node-structure / wire-state inferences that the type system cannot
express. They no longer touch the f-pin's declared type.

**Name-matched starts-with exception in
`node_network.rs::can_connect_nodes`** (`if dest_node.node_type_name
== "map" && dest_param_index == 1`): removed. The starts-with rule is
now expressed structurally by `map.f`'s declared type
(`AnyFunction { leading_params: vec![element_type] }`) and reaches
connect-time via the standard `can_be_converted_to` rule.

### What still needs a post-pass

The post-passes do not go away — they compute things that the type
system cannot express:

- **Apply's arg-pin layout** (count of arg pins, their names, their
  types) comes from the wired `f` source. This is *node-structure*
  inference, not type inference. The post-pass stays for this.
- **Apply's output pin type** is `k`-dependent (full eval vs partial).
  Type-level expressions cannot encode "depends on how many other
  pins are wired."
- **Map's output pin type** derives `Iter[derived]` from the wired
  source's tail. Same shape — depends on wire state, not just types.

After this refactor, the post-passes do *less*: they handle arg-pin
layout and output-pin-type derivation, but not the f-pin's declared
type. That stays static.

## Scope of this branch

In scope:
1. **`DataType::AnyFunction` plumbing** (Phase A) — type variant +
   compatibility rule + strict variant + display + parser +
   canonicalization recursion + record-name walker + serde +
   `NetworkResult::convert_to` identity passthrough.
2. **`apply.f` retrofit** (Phase B) — declared type becomes
   `AnyFunction { leading_params: vec![] }`. Post-pass simplified to
   only compute arg-pin layout and output-pin type. Apply's kind
   picker UI is removed in Phase D (resolves `design_currying.md`
   Open Question 2 in favor of "no pins until wired"); the
   `ApplyData.kind` / `type_args` data fields stay for `.cnnd`
   back-compat, deprecation deferred.
3. **`map.f` retrofit** (Phase C) — declared type becomes
   `AnyFunction { leading_params: vec![element_type] }`. Post-pass
   simplified to only compute map's output-pin type. Name-matched
   exception in `can_connect_nodes` removed.
4. **Editor work** (Phase D; corresponds to Phase 5 of
   `design_currying.md`) — unified `APIDerivedShapeView` plumbed to
   Flutter; apply renders "f pin only" until wired; map renders
   derived output as read-only when wired; 0-arity Custom closure
   support; pin colors / tooltips for `AnyFunction`.
5. **Test updates** — Phase 3/4 tests that asserted "post-pass
   rewrites f-pin declared type" need to be re-grounded against the
   new model (pin type stays put, layout updates).

Out of scope:
- **Polymorphic function sources.** `AnyFunction` is input-only;
  sources always resolve to concrete `Function(_)`. A future
  `SameAsInput`-style polymorphic Function output would need its own
  treatment.
- **Composable function-shape constraints beyond starts-with.** If a
  future HOF needs "tail must match" or "any-position constraint,"
  that's a new variant or a richer constraint enum. Not now.
- **`.cnnd` migration.** `AnyFunction` is a new variant that did not
  exist when previous files were saved. No file in the wild carries
  it, so load is unaffected. The variant appears only on built-in
  node declarations (post-load), so saving doesn't write it either —
  `apply.f` and `map.f` types live on built-in `NodeType` structures
  the loader sets up afresh, not on serialized per-node data. No
  version bump.

## Build/test contract

Per existing project convention (and `doc/design_currying.md`'s
contract): `cd rust && cargo test` green and `cargo clippy` clean for
every Rust phase. `flutter_rust_bridge_codegen generate` succeeds.
Existing closure/HOF/apply/map tests pass after their assertions are
re-grounded against the new model.

`flutter run` launches at the end of Phase D with the manual
walkthrough (see Phase D §Tests) passing.

## Implementation phases

### Phase A: `DataType::AnyFunction` plumbing

**Goal.** Add the type variant. Compatibility rule works in
isolation. No node uses it yet — existing behavior unchanged.

**Scope.**
- `data_type.rs`:
  - Add `DataType::AnyFunction { leading_params: Vec<DataType> }`.
  - `can_be_converted_to`: add the `(Function, AnyFunction)` arm
    (concrete Function source flows into AnyFunction destination
    when `src.parameter_types` starts with `dest.leading_params`,
    pairwise convertible). Reject AnyFunction as a *source* type
    (return false).
  - `can_be_converted_to_strict_no_broadcast`: mirror.
  - `Display` impl: render `AnyFunction { vec![] }` as `Function*`
    and `AnyFunction { vec![T1, T2] }` as `Function(T1, T2, *)`. The
    `*` is the "any tail allowed" marker.
  - Parser: add `*` as a new token. Accept both forms, route
    construction through canonicalizing constructor for nested-Function
    `leading_params`.
  - `canonicalize_data_type`: new arm; recurse into `leading_params`
    (entries may carry nested `Function` variants needing
    canonicalization).
  - `walk_data_type_record_names_mut`: new arm; recurse into
    `leading_params`.
  - `is_abstract`: returns false (it's not an abstract phase type;
    it's a structural acceptance constraint on input).
  - `is_array`: returns false.
  - serde: standard derive works.
- `node_type_registry.rs`:
  - `collect_named_record_refs_in_type`: new arm; recurse into
    `leading_params` so a Named record reference embedded in an
    AnyFunction's prefix is tracked for cycle/dangling-ref checks.
- `network_validator.rs`:
  - `contains_abstract`: new arm; returns false (or recurses into
    `leading_params` if any embedded type is abstract — TBD whether
    a parameter type carrying an abstract subtype should flag the
    declared pin; for now, return false uniformly to match the
    `is_abstract` policy).
  - New rule: an output pin's resolved type must never be
    `AnyFunction`. Catches accidental author-declared AnyFunction
    outputs. (Built-in nodes will not declare it; this is defensive.)
- `evaluator/network_result.rs`:
  - `NetworkResult::convert_to`: add identity-passthrough arm for
    `Function → AnyFunction`. A concrete `Function(ZoneClosure)`
    flowing into an `AnyFunction`-typed slot is the same runtime
    value, unchanged. (Without this arm, the conversion would hit a
    no-match path and surface as a runtime error.)
- Registry-build-time assertion (debug-only):
  - In `NodeTypeRegistry::add_node_type`, assert that no pin in
    `node_type.output_pins` declares `AnyFunction` as its `Fixed`
    type. Catches author error eagerly.

**Tests.** New tests in `currying_test.rs` (or a new
`function_pin_unification_test.rs`):
- Compatibility: `Function([Int], Bool) → AnyFunction { vec![] }` ✓.
- Compatibility: `Function([Int, Bool], String) → AnyFunction { vec![Int] }` ✓.
- Compatibility: `Function([Bool], String) → AnyFunction { vec![Int] }` ✗ (first param mismatch).
- Compatibility: `Function([], Int) → AnyFunction { vec![Int] }` ✗ (too short).
- Reverse direction rejected: `AnyFunction → Function(_)` ✗.
- Canonicalization through `leading_params`: nested function returns
  inside `leading_params[i]` get flattened on construction.
- Parser round-trip: `Function*`, `Function(Int, *)`, `Function((A) -> B, *)` etc.
- Display matches parser input (round-trip).
- `NetworkResult::Function(zc).convert_to(&AnyFunction { … })`
  returns the same Function value unchanged.

**Gotchas.**
- The `*` token must not collide with anything in the existing
  grammar. Today's parser uses `(`, `)`, `[`, `]`, `,`, `->`, `=>`,
  identifiers. `*` is free.
- `leading_params` is a `Vec<DataType>`, not a `FunctionType`. The
  prefix doesn't have an output type — it's just a list of leading
  parameter types. Don't confuse it with `FunctionType`.
- AnyFunction is INPUT-ONLY. Be explicit at every site (compatibility,
  inference, registry assertion) that AnyFunction-as-source is invalid.

### Phase B: `apply.f` retrofit

**Goal.** `apply.f`'s declared type is `AnyFunction { vec![] }`. Post-pass
no longer touches the pin type — only arg-pin layout and output type.
All existing `apply` tests pass with updated assertions.

**Scope.**
- `nodes/apply.rs::calculate_custom_node_type`:
  - `f`-pin type set to `DataType::AnyFunction { leading_params: vec![] }`.
  - Default ApplyData-driven arg-pin layout still computed for the
    disconnected-`f` case (until Phase D removes it — see below).
- `node_type_registry.rs`:
  - `compute_apply_custom_type_from_wired_f`: stop rewriting the
    `f`-pin type. Continue computing arg-pin layout (count, names,
    types) + output pin type. The function returns the same kind of
    `NodeType` override, just without the `f` pin's `data_type` field
    being rewritten.
  - `update_apply_pin_layouts_for_network`: behavior unchanged at the
    driver level; the work it delegates to is now narrower.
- Validation: the connect-time check `can_connect_nodes` for apply.f
  now resolves through standard `can_be_converted_to(src, AnyFunction)`.
  No special case needed.
- Phase 3 tests:
  - Tests that grep for "apply.f's declared type after wiring" need
    to be updated. With the new model, apply.f's declared type STAYS
    `AnyFunction`; only the arg-pin layout reflects the source's shape.
  - Tests that exercise partial application, full eval, identity
    partial, prefix-only validation, 0-arity thunk, recursive
    consumption — all should still pass with the same assertions on
    arg-pin count, output type, and eval results.

**Tests.** Existing Phase 3 tests in `currying_test.rs` are
re-grounded. New tests:
- Assert `apply.f`'s declared type is `AnyFunction { vec![] }`
  regardless of wiring state.
- Assert connect-time gate uses standard `can_be_converted_to` (no
  name-matched exception).

**Gotchas.**
- Phase 3's post-pass currently propagates `param_types` from the
  source's flat type onto `apply.parameters[1].data_type`. After this
  refactor, that field stays `AnyFunction`. The arg-pin types
  (parameters 2..N) still come from the source. Make sure no caller
  is incorrectly reading `parameters[1].data_type` expecting the
  source's function type.

### Phase C: `map.f` retrofit

**Goal.** `map.f`'s declared type is
`AnyFunction { leading_params: vec![element_type] }`. The starts-with
exception in `can_connect_nodes` is removed. Map's output-type
derivation post-pass continues to work; type rewriting on `map.f` is
gone.

**Scope.**
- `nodes/map.rs::calculate_custom_node_type`:
  - `f`-pin type set to `DataType::AnyFunction { leading_params: vec![input_type.clone()] }`.
- `node_type_registry.rs`:
  - `compute_map_custom_type_from_wired_f`: stop rewriting `f`-pin
    type. Continue computing the derived output type. Continue
    falling back to MapData defaults when no compatible source.
- `node_network.rs::can_connect_nodes`:
  - Remove the name-matched `if dest_node.node_type_name == "map" &&
    dest_param_index == 1` block (Phase 4 hack). The starts-with rule
    is now expressed by `map.f`'s declared type.
- Phase 4 tests: same re-grounding as Phase B. Higher-arity sources
  flowing into map.f now connect via the AnyFunction compatibility
  rule, not the name-matched exception.

**Tests.** Existing Phase 4 tests in `currying_test.rs` re-grounded.
New tests:
- Assert `map.f`'s declared type is
  `AnyFunction { leading_params: vec![element_type] }` for various
  MapData configurations.
- Assert removal of the name-matched exception: a unit test that
  walks `can_connect_nodes` source verifying no `node_type_name ==
  "map"` branch survives. (Optional — easy to skip if too tedious.)

**Gotchas.**
- `MapData.input_type` still drives `leading_params`. When the user
  changes `input_type`, `calculate_custom_node_type` re-runs and
  updates the pin's `leading_params`. Existing repair-on-input-type-
  change flows continue to work.
- The walker change from Phase 4 (`MapZone::next` branching on body
  arity to build partial closures) is unaffected by this refactor —
  it operates on the closure value at runtime, not on declared types.

### Phase D: editor (corresponds to Phase 5 of `design_currying.md`)

**Goal.** Apply with no pins-until-wired UX. Map with derived output
type displayed as read-only when `f` wired. 0-arity Custom closure
support. `flutter run` works.

**Scope.**
- **Apply node UX** (resolves `design_currying.md` Open Question 2):
  - Disconnected `f`: render only the `f` pin. No kind picker UI.
    No arg pins. A placard/hint reads "wire a function value to
    materialize arg pins."
  - Wired `f`: arg pins materialize per the post-pass; output pin
    shows derived type.
  - `ApplyData.kind` / `type_args` data fields stay (loaded from
    `.cnnd`, default `ClosureKind::Map` / `[Float, Float]`) but are
    structurally irrelevant: the disconnected-`f` UX no longer
    consults them to render pins, and the wired-`f` UX is driven
    by the source. Deprecation deferred — see "Out of phase plan."
- **Map node UX**:
  - Wired `f`: `output_type` editor field becomes read-only display
    of derived type. Tooltip: "derived from f."
  - Disconnected `f`: `output_type` field editable, drives the
    fallback used when `f` is later disconnected.
- **0-arity Custom closure**: `ClosureShapeEditor` accepts empty
  `param_names`. Title bar renders `() → R`. The corresponding apply
  with a 0-arity source shows zero arg pins; output pin type =
  return type; eval forces the thunk.
- **API**:
  - Unified `APIDerivedShapeView` (single struct) exposes the
    abstract "is this node's layout/types derived from a wired
    input?" bit:
    ```rust
    pub struct APIDerivedShapeView {
        /// `Some(pin_name)` when the wired source on `pin_name`
        /// drives the derived layout/output type; `None` when no
        /// derivation is in play (node renders its static layout).
        pub derived_from_input_pin: Option<String>,
    }
    ```
    Apply populates `Some("f")` when `f` is wired, else `None`. Map
    populates `Some("f")` when `f` is wired with a compatible source,
    else `None`. Per-pin info continues to flow through the existing
    `NodeView` machinery — the view holds only the derivation status.
  - Populated by `build_node_view`. FRB-regenerated for Flutter.
- **Pin colors / tooltips**:
  - `AnyFunction` pins render with the Function pin color (same as
    concrete `Function` pins).
  - Type-level tooltip:
    - `AnyFunction { vec![] }` → "any function value".
    - `AnyFunction { vec![T] }` → "function whose first parameter is
      `<T>` (extra parameters allowed)".
  - Node-specific secondary line appended by the node's pin tooltip
    builder: apply → "apply will call it on the wired arguments";
    map → "applied per element of the stream". Final wording finalized
    during this phase.

**Tests.** Manual walkthrough (per `design_currying.md` Phase 5):
1. Place an `apply`. Confirm only `f` pin renders, no kind picker.
2. Place a 2-arg `closure` Custom `(x, y) → x*y`. Wire to apply.f.
   Confirm arg pins materialize as `x`, `y` (or `arg0`, `arg1`).
3. Wire only `arg0`; confirm output pin retypes to `Function((Int,), Int)`.
4. Disconnect `f`; confirm apply collapses back to single-pin view.
5. Place a `map`, configure `input_type=Int`. Wire `range(3)` to `xs`.
   Wire a `(Int, Int) → Int` closure to `f`. Confirm map's output_type
   field shows `Function((Int,), Int)` read-only.
6. Disconnect `f`; confirm field becomes editable again at whatever
   value was stored.
7. Place a 0-arity Custom closure (remove all params). Confirm
   `() → R` title. Wire into an apply with zero arg pins. Eval forces
   the thunk.
8. Regression: existing closure/HOF networks load and evaluate
   unchanged.

**Gotchas.**
- **Pin-name policy on apply** (carried over from Phase 3): the
  post-pass preserves OLD pin names at overlapping indices to
  protect wire-by-name preservation. With this refactor's
  disconnected-`f` UX showing no arg pins, the "OLD names" carry over
  from prior wired sources only. This is fine for the wire-once path.
  For the wire-disconnect-rewire path, OLD names are the previous
  source's pin names — which may or may not match the new source.
  Acceptable in v1; revisit if a user complains.
- **Source-authored pin names in label overlay**: a Phase 5 polish
  item from `design_currying.md`. The wired closure's `param_names`
  could appear next to the apply's arg pins as a hint (e.g.
  "arg0 (x)"). Optional — flagged for future work if not done here.

### Out of phase plan (deferred)

- **`ApplyData.kind` deprecation.** Now structurally irrelevant when
  `f` is wired; with disconnected-`f` UX showing only the `f` pin,
  it's structurally irrelevant always. Retire the field after one
  serialization-migration cycle. Not this branch.
- **`MapData.output_type` deprecation.** Same trajectory. Probably
  longer horizon — the disconnected-`f` UX on map still uses it.
- **`SameAsInput`-style polymorphic Function outputs.** If a future
  node needs to emit `Function(_)` whose shape mirrors an input, it
  needs its own treatment. Out of scope.
- **Composable function constraints beyond starts-with.** Future
  HOFs may need other constraints (tail-match, arity-bound). Add
  variants to `AnyFunction`'s constraint or split into multiple
  variants when the third use case arrives.

## Notation reference

Display / parser syntax for `AnyFunction`:

| Type | Renders as |
|---|---|
| `AnyFunction { leading_params: vec![] }` | `Function*` |
| `AnyFunction { leading_params: vec![T] }` | `Function(T, *)` |
| `AnyFunction { leading_params: vec![T1, T2] }` | `Function(T1, T2, *)` |

The `*` is the "any tail allowed" marker. The parser adds `*` as a
new token; no existing grammar uses it.

## Phasing summary

| Phase | Outcome |
|---|---|
| A | `DataType::AnyFunction` type variant + compatibility rule + plumbing (no node uses it yet) |
| B | `apply.f` retrofit: declared type is `AnyFunction { vec![] }`; post-pass simplified |
| C | `map.f` retrofit: declared type is `AnyFunction { vec![element_type] }`; `can_connect_nodes` exception removed |
| D | Editor: pins-until-wired on apply, derived output read-only on map, 0-arity Custom, FRB views |

Each phase's exit gate: `cd rust && cargo test` green plus `cargo
clippy` clean. Phase D additionally requires `flutter analyze` clean
and `flutter run` launching with the manual walkthrough passing.
