# `zip_with` node — n-ary element-wise map (issue #382)

## Motivation

Issue [#382](https://github.com/atomCAD/atomCAD/issues/382) (mechadense): combining N
streams element-wise with an N-argument function is a fundamental operation
(`map` == zipWith1, `zipWith` == zipWith2, `zipWith3`, …). Today the only way to
express it is to pack the parallel arrays into records (via `product` — wrong
semantics — or manual record plumbing) and no-op-thread the record through the
closure, a workaround the reporter has needed three times. Since the node
network has no parametric polymorphism, a custom network cannot be defined for
this either.

We add **one variadic node**, not a `zip_with2` / `zip_with3` family. Several
existing nodes already support a user-configurable number of typed input pins
(`expr`, `product`, `apply`), and every layer `zip_with` needs is already N-ary:

- `zone_closure::run_closure_once` takes `args: Vec<NetworkResult>` — `fold`
  already pushes a 2-element frame `[acc, element]`. An N-element frame needs
  zero changes to the closure substrate.
- `NodeType.zone_input_pins` is a `Vec` — `fold` declares two inside-facing
  source pins; N is just data. Body rendering, capture freezing, the three zone
  validation rules, `repair_zone_body`, copy/paste, and undo are all generic
  over the pin list.
- `expr` derives its input pins from a user-edited parameter list whose entries
  carry a **stable id** so wires survive list edits. `zip_with` borrows the
  stable-id part of that pattern (but not user-chosen names — see "Lane pin
  naming" below).
- `ClosureKind::Custom` already produces N-ary function values, and `map.f`'s
  `AnyFunction { leading_params: [element] }` starts-with rule generalizes
  directly to `leading_params: [T_1 .. T_N]`.
- The walker layer needs exactly one new variant (`ZipZone`), structurally
  simpler than the existing `Product` odometer.

## Naming

The node is named **`zip_with`**.

- It is the established FP name for exactly this operation (Haskell `zipWith`,
  cited in the issue), so it is what users search for.
- It follows the codebase's snake_case multi-word convention (`array_at`,
  `record_construct`, `plane_tiling_vectors`).
- Plain `zip` would be wrong: in every mainstream language `zip` means tupling
  *without* a function, and this node has a body. `multimap` names a data
  structure (multi-valued map) and would mislead. `map_n` reads as an arity
  suffix — the thing we're avoiding.
- The node's `description` / `summary` mention "multimap", "zipWith", and
  "n-ary map" so registry search finds it under any of those names.

## Lane pin naming: fixed positional names, hidden stable ids

The N input lanes use **fixed, position-derived pin names**: the external
stream pins are `xs1 .. xsN` and the inside-facing zone-input pins are
`element1 .. elementN`. Lane identity is carried by a **hidden stable id** per
lane (stamped onto `Parameter.id`), never by the name.

A user-named-lanes alternative (one user-chosen name per lane, used for both
pin faces) was considered and rejected:

- **Renameable pins are this codebase's most recurrent bug family** (#377
  record-field rename dropping wires, parameter wire stability). User-named
  lanes would open one more renameable-pin surface and then spend a large part
  of the implementation defending it (id-preserving merge logic, rename/reorder
  wire-survival tests).
- **Name collisions.** A lane named `f` would collide with the function pin,
  and in the text format a node's `{ key: value }` block mixes property keys
  and pin names — a lane named `output_type` or `lane_types` would be
  unparseable. Fixed names need no reserved-name or duplicate-name validation.
- **The readability benefit is smaller than it looks.** The typical `zip_with`
  body is a single `expr` node, whose parameters the user *does* name — so
  `element1 → width`, `element2 → height` wiring puts the semantic names
  exactly where they are consumed. Naming the lanes as well would duplicate
  that one level up.
- Fixed names also extend `map`/`fold`'s existing `xs` / `element` vocabulary,
  keep the property panel to a plain type list, and make "reorder" a
  non-operation (with no names, reordering lanes is just retyping them), so
  lane editing collapses to **add / remove / retype**.

Numbering is **1-based** (`xs1` = "first array"), matching human counting and
the zipWith2/zipWith3 convention in the issue. This is a conscious mild
inconsistency with `apply`'s derived `arg0, arg1, …` pins, whose 0-based
numbering comes from the function-signature domain.

## Out of scope (deferred)

- **`zip` / `zipRecord`** (tupling without a function). A `zip_with` body
  containing a `record_construct` (or an `expr` building a vector) covers it
  with no generated record types.
- **Changing `map` to N inputs.** Considered and rejected: it would put
  serialization migration, `f`-pin derivation churn, and drag-adapter risk on
  the single most-used HOF for zero semantic gain. A separate node whose
  1-input degenerate case equals `map` is the same mild redundancy Haskell
  lives with (`map` vs `zipWith`), and is far safer.
- **Strict length-mismatch variant** (error instead of truncating at the
  shortest input). Could later be a `strict: bool` property; not in this drop.
- **Expression-language `zip_with(...)`.** Node-graph only, like
  `filter`/`fold`.

## Recap: the machinery this node composes (file refs)

- `rust/src/structure_designer/nodes/map.rs` — the single-input template:
  `calculate_custom_node_type` writes pin types from stored data, `eval`
  resolves `xs`, calls `obtain_closure`, and wraps a lazy walker.
- `rust/src/structure_designer/nodes/fold.rs` — the two-zone-input-pin
  precedent (`acc`, `element`) and the "f pin index after the data pins"
  convention (`obtain_closure(…, 2, "fold")`).
- `rust/src/structure_designer/nodes/expr.rs` — the stable-id precedent:
  `ExprParameter.id` propagated to `Parameter.id` so wires survive parameter
  list edits.
- `rust/src/structure_designer/evaluator/zone_closure.rs` —
  `build_inline_closure` mirrors the owner's `zone_input_pins` into
  `param_types` generically; `run_closure_once(…, args: Vec<NetworkResult>)`
  is already N-ary.
- `rust/src/structure_designer/evaluator/iterator_walker.rs` — `WalkerKind`
  variants; Invariant 2 (clone independence); `MapZone`'s auto-partialization
  branch (currying Phase 4) that `ZipZone` mirrors.
- `rust/src/structure_designer/node_type_registry.rs` —
  `update_map_pin_layouts_for_network` / `compute_map_custom_type`: the
  post-pass that derives a map's output type from a wired `f`; recomputes
  *every* map node each validate so `f`-disconnect restores the stored layout;
  runs **after** the apply post-pass. `zip_with` gets the analogous pass.

## Design decisions

| Question | Decision |
|---|---|
| Node name | `zip_with` (see Naming above). Category `MathAndProgramming`. |
| Arity | User-configurable list of lanes, default **2** (both `Float`). Minimum **1** is legal (degenerate = `map`); **zero lanes is rejected** — the editor disables deleting the last lane, and the setter / `set_text_properties` return an error on an empty lane list. The property panel's Add/Delete buttons are the arity UI. |
| Pin naming | Fixed positional: `xs1 .. xsN` external, `element1 .. elementN` inside, 1-based (see "Lane pin naming" above). No user-editable names, hence no rename and no reorder — lane editing is add / remove / retype. |
| Lane identity | Each lane carries a **hidden stable `id: Option<u64>`** stamped onto `Parameter.id`, so external wires follow their lane across removal of an earlier lane (the label renumbers, the wire stays). This must not reproduce the wire-drop bug family (#377, identity-vs-naming). |
| Termination | **Shortest input** ends the stream (Haskell `zipWith` convention). Empty lane → empty output. |
| Laziness | Fully lazy: output is `Iter[output_type]`, carried by a new `WalkerKind::ZipZone`. The intermediate sequence is never materialized. |
| `f` pin | Trailing optional pin, declared `AnyFunction { leading_params: [T_1 .. T_N] }` — any function whose parameter list starts with the lane types flows in; excess parameters become the partial-application tail (elements then map to partially-applied `Function` values), mirroring `map` exactly (`doc/design_function_pin_unification.md`). |
| Output type | Stored `output_type: DataType` when `f` is disconnected (editable in the panel); **derived from the wired `f`'s tail** by a post-pass sibling of `update_map_pin_layouts_for_network` when connected (read-only display in the panel). |
| Zone pins | One zone-input pin per lane (`element1 .. elementN`, typed `T_i`); one zone-output pin `result: output_type`. |
| ClosureKind | **No new preset.** `Custom` covers N-ary bodies for the `f`-pin path; the inline body is the primary authoring path anyway. |
| Scalar broadcast | The implicit `S → Iter[T]` single-element broadcast means a scalar-fed lane ends the whole zip after one element. **Silently allowed** — it is the ordinary conversion rule, the result is well-defined, and such cases are easy to debug. Users wanting "each element combined with a constant" use a **capture** into the body, not a lane; the node's `description` documents this. |
| Text format | `name = zip_with { lane_types: [Int, Float], output_type: Int, xs1: src1, xs2: src2, f: @g }` — lane types as a plain array; pin names are derived, so no name array and no key-collision hazard. |
| Serialization | `generic_node_data_saver`/`loader`. New node type ⇒ **no `.cnnd` migration** and no `produces_iter` migration arm (no legacy file contains it). |
| Display | Output pin is `Iter[..]` ⇒ no viewport output; users wire `collect` to inspect (existing `doc/design_iter_display_via_collect.md` behavior, nothing new). |
| Undo | Lane-list edits are **not** pure node-data edits: removal and retype also drop or remap wires (the removed lane's external wire, body wires incl. nested bodies, retype-incompatible body wires), and `snapshot_node_data` captures only the `node_data_saver` blob — no `arguments`, no zone body. Lane-list mutations therefore push a **`ZipWithLaneEditCommand`** holding whole-network before/after snapshots (`SerializableNodeNetwork`, the `TextEditNetworkCommand`/`DeleteNetworkCommand` pattern) of the owning **top-level** network — bodies travel inside their HOF nodes, so body-internal zip nodes are covered. Pushed only when the snapshots differ; text-path edits stay covered by the existing `TextEditNetworkCommand` (no double push). Inline body edits/undo remain generic zone machinery. |

## Data model and pin derivation

New file `rust/src/structure_designer/nodes/zip_with.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipWithLane {
    #[serde(default)]
    pub id: Option<u64>,      // hidden stable identity; wires survive lane removal
    pub data_type: DataType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipWithData {
    pub lanes: Vec<ZipWithLane>,     // default: two Float lanes (ids 1, 2)
    pub output_type: DataType,       // default: Float
    /// Monotonic id source for new lanes. Persisted — deriving it as
    /// max(existing)+1 would recycle the id of a just-removed highest lane,
    /// the same hazard class as the `next_param_id` wire-stability regression
    /// (`doc/design_parameter_wire_stability.md`). `#[serde(default)]`; healed
    /// on load to max(lane ids)+1 when missing/zero. The loader also mints
    /// ids from the healed counter for any lane loaded with `id: None`
    /// (hand-authored files) — an id-less lane silently degrades to
    /// name-based (i.e. positional) wire matching in `set_custom_node_type`,
    /// exactly the fragility the ids exist to prevent.
    #[serde(default)]
    pub next_lane_id: u64,           // default: 3
}
```

`calculate_custom_node_type` builds the parameter list from scratch (like
`product`/`expr`, not by indexing base parameters — the count varies):

```rust
fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
    let mut custom = base_node_type.clone();

    // External pins: xs1..xsN (Iter[T_i]), then the trailing optional `f`.
    custom.parameters = self.lanes.iter().enumerate()
        .map(|(i, lane)| Parameter {
            id: lane.id,
            name: format!("xs{}", i + 1),
            data_type: DataType::Iterator(Box::new(lane.data_type.clone())),
        })
        .collect();
    custom.parameters.push(Parameter {
        id: None,
        name: "f".to_string(),
        data_type: DataType::AnyFunction {
            leading_params: self.lanes.iter().map(|l| l.data_type.clone()).collect(),
        },
    });

    custom.output_pins = OutputPinDefinition::single(
        DataType::Iterator(Box::new(self.output_type.clone())));

    // Inside-facing pins: element1..elementN, one result destination.
    custom.zone_input_pins = self.lanes.iter().enumerate()
        .map(|(i, lane)| OutputPinDefinition::fixed(
            &format!("element{}", i + 1), lane.data_type.clone()))
        .collect();
    custom.zone_output_pins = vec![Parameter {
        id: None, name: "result".to_string(), data_type: self.output_type.clone(),
    }];

    Some(custom)
}
```

The base `NodeType` registration declares the 2-lane Float default (so
`NodeType::has_zone()` is true from registration, before any custom-type cache
runs) and `get_parameter_metadata` marks every lane required and `f` optional.
`get_subtitle` renders the shape, e.g. `(Int, Float) → Vec3`.

**Identity notes.** Ids live only on the **external** `Parameter`s — that is
what by-name/by-id argument rebuilds consult (verified:
`node_network.rs::set_custom_node_type` matches parameters by id when both
sides carry one, with name-based fallback — the way `expr`/record nodes rely
on it). New lane ids come from the persisted `next_lane_id` counter, never
from max+1 recomputation (see the field comment above).

The **body-internal** `ZoneInput { pin_index }` wires are index-based; lane
*removal* therefore shifts the meaning of later indices. The remap
(disconnect wires to the removed index, decrement wires to later indices)
**must be performed by the remove-lane operation itself at mutation time** —
a `StructureDesigner`-level op, since it walks the zip node's body, which
the `ZipWithData` struct cannot reach. A validate-time repair pass only sees
the post-edit pin list and cannot know *which* index was removed, and a
shifted-but-still-in-range wire between same-typed lanes passes every type
check, so leaving this to repair produces silently wrong values, not a red
badge. The remap must also recurse into **nested HOF bodies** inside the zip
body: a body-internal `map`'s own body can reference the zip's lanes at
`source_scope_depth ≥ 2`, and the existing `repair_zone_body`
(`node_type_registry.rs`) explicitly skips depth ≥ 2 `ZoneInput` wires — so
this recursion is new machinery, not an extension of it. Matching must
require all three of `SourcePin::ZoneInput`,
`source_node_id == zip_node_id`, **and** `source_scope_depth` equal to the
wire's nesting distance from the zip body (1 for a node directly in the zip
body, 2 for a node inside a nested HOF's body, …) — node ids collide across
scopes (per-body `next_node_id`), so matching on id alone would corrupt an
inner HOF's own element wires. Validation rule 3 and `repair_zone_body`
remain only as a backstop for hand-authored files. The same recursive walk,
in disconnect-only form (no decrement), also cleans body wires to
tail-dropped indices when the positional whole-list merge shrinks the lane
list — see Phase 3. Since there is no rename and no reorder, no other
identity machinery is needed.

## Runtime semantics

### `eval` (mirrors `map.eval`, looped)

1. For each lane `i` in `0..N`: `evaluate_arg_required(…, i)`; accept
   `NetworkResult::Iterator(w)` (the wire-level `Array→Iter` / lazy
   `Iter[S]→Iter[T]` conversions have already run), belt-and-braces accept
   `Array` and wrap via `Walker::from_array`, propagate `Error`, reject other
   variants with `zip_with: input 'xs{i+1}' is not an iterator (got …)` (pin
   names are 1-based; the loop index is 0-based).
2. `obtain_closure(…, f_param_index = self.lanes.len(), "zip_with")` — wired
   `f` wins, otherwise the inline zone body via `build_inline_closure` (which
   mirrors the N zone-input pins into `param_types` generically). Construction
   errors return `EvalOutput::single(Error)` — never a degenerate walker
   (errors would multiply per element).
3. `EvalOutput::single(NetworkResult::Iterator(Walker::zip_zone(sources, closure)))`.

### `WalkerKind::ZipZone`

```rust
ZipZone {
    sources: Vec<Walker>,
    closure: ZoneClosure,
}
```

`next()` algorithm:

1. Pull one element from each source **in lane order**. If any source returns
   `None`, the zip ends (`return None`) — elements already pulled this step
   from earlier lanes are discarded (documented; determinism note: sources are
   always pulled in lane order, so `print`-node side effects inside upstream
   walkers fire deterministically). If any source yields `Some(Error(_))`,
   yield it (the outer fuse then terminates the stream).
2. Run the closure on the pulled frame, mirroring `MapZone`'s branch including
   the currying auto-partialization: if `closure.param_types.len() > N`, the N
   elements fill the leading slots and the result is a partially-applied
   `Function` value; if `== N`, `run_closure_once(&[], …, args)` (body-only
   stack, same as `MapZone` — deep captures were pre-frozen at `eval`); if
   `< N` — unreachable through type-checking (`AnyFunction` requires arity
   ≥ N and inline bodies have exactly N params) but reachable via a
   hand-authored file — yield a `NetworkResult::Error` rather than panicking
   or silently truncating the frame.
3. `reset()` resets every source. Clone independence (Invariant 2) holds:
   per-source state is owned via the `Vec<Walker>`, and `ZoneClosure` is fully
   `Arc`-backed.

Factor the "run closure on frame + auto-partialization" step out of the
`MapZone` arm into a shared helper so `MapZone` and `ZipZone` cannot drift.

### Validation and repair

- Zone rules 1–3 (`validate_zones_recursive`) apply generically. Rule 1
  ("zone-output pin needs a wire") stays **non-blocking** and is suspended when
  `f` is connected — `function_input_pin_connected` (`node_network.rs`)
  already locates the `f` pin by name + function shape rather than a hardcoded
  index (verified), so the trailing pin works unchanged; pinning test in
  Phase 2.
- Lane **retype** → the existing `repair_zone_body` disconnects now-incompatible
  body wires (keyed off zone-input pin type change), same as `map.input_type`.
  Depth ≥ 2 wires reading a retyped lane are beyond `repair_zone_body`'s reach
  — exactly as they are for `map.input_type` retype today; a shared,
  pre-existing limitation, not new `zip_with` scope.
- Lane **removal** → external wires follow their lane ids (a wire on the old
  `xs3` survives as the new `xs2` when lane 2 is removed); body wires
  referencing the removed `ZoneInput { pin_index }` are disconnected and wires
  to later indices are decremented **by `remove_lane` at mutation time**, not
  by the repair pass — repair has no removal diff, and a shifted-but-in-range
  wire between same-typed lanes is silently wrong rather than invalid (see
  "Identity notes"). The remap recurses into nested bodies with the
  depth+id matching described there. Rule 3 and `repair_zone_body` stay as a
  backstop for hand-authored files only. This is Phase 3's core deliverable.
- Scalar-fed lanes need **no validation rule**: the implicit `S → Iter[T]`
  broadcast produces a well-defined 1-element stream and the zip truncates
  accordingly. Documented in the node's `description` (constant-per-element =
  capture, not lane), not policed.

### The `f`-derivation post-pass

`update_zip_with_pin_layouts_for_network` (+ `_preserving_args` variant),
mirroring the map pass in `node_type_registry.rs`:

- Recomputes **every** `zip_with` node's custom type each validate, so
  disconnecting `f` cleanly restores the `ZipWithData`-driven layout
  (idempotent; the data-driven default equals the cache-populated one).
- Derives the output pin type from the wired source's canonical-flat signature
  tail: exact arity ⇒ `Iter[R]`; higher arity ⇒ `Iter[Function(<tail> → R)]`.
- Runs **after** the apply post-pass (same ordering constraint and reason as
  map's), recurses into zone bodies with the ancestor chain threaded.
- Extract the shared skeleton from `compute_map_custom_type` /
  `update_map_pin_layouts_scoped` where practical instead of copy-pasting a
  third sibling (apply, map, zip_with) — at minimum share the
  "read wired `f` source's canonical signature across scopes" resolution
  helper.
- Populates the same derived-shape view (`APIDerivedShapeView.derived_from_input_pin
  = Some("f")`) that drives the Flutter read-only output-type display.

---

## Phase 1 — Core node + `ZipZone` walker (inline body path)

**Deliverables**

- `nodes/zip_with.rs`: `ZipWithData` / `ZipWithLane`, `NodeData` impl
  (`calculate_custom_node_type`, `eval`, `get_parameter_metadata`,
  `get_subtitle` — e.g. `(Int, Float) → Vec3` — and stub text properties),
  registration in `nodes/mod.rs` +
  `node_type_registry.rs::create_built_in_node_types()`.
- `WalkerKind::ZipZone` + `Walker::zip_zone(…)` in `iterator_walker.rs`,
  including the shared run-frame helper factored out of the `MapZone` arm and
  `reset()` support.
- `f` pin declared and `obtain_closure(f_param_index = N)` wired in `eval`
  (the wired-`f` **evaluation** path is nearly free via `obtain_closure`; the
  layout/output-type **derivation** post-pass is Phase 2).
- Update any registry-wide assertions (node-type counts/lists in existing
  tests — closures Phase 4 had to touch 4 such files) and re-bless node-type
  snapshots (`cargo insta review`, `node_snapshot_test.rs`).

**Automated tests** — new `rust/tests/structure_designer/zip_with_test.rs`
(registered in `tests/structure_designer.rs`), plus additions to
`iterator_walker_test.rs`:

1. Walker-level (`iterator_walker_test.rs`): `zip_zone` over two `FromArray`
   sources — element-wise results in order; shortest-input truncation (3⊗5 → 3);
   one empty source → empty; error mid-stream in a source fuses the zip;
   clone-independence (clone the walker mid-stream, advance both, results
   independent); `reset()` rewinds all sources.
2. Network-level, 2 lanes (`zip_with_test.rs`, building networks
   programmatically like `zones_test.rs`): `range` + `range` into `zip_with`
   with an inline body summing the two elements (an `expr` in the body with
   `element1`/`element2` wired to its parameters), `collect`, assert the
   array. Mind the per-body `next_node_id` / `num_params` gotchas documented
   in the zones test patterns.
3. 3 lanes: `element1 * element2 + element3` body — proves variadic pins
   end-to-end.
4. 1 lane degenerate: behaves exactly like `map`.
5. Mixed lane types (`Iter[Int]` ⊗ `Iter[Vec3]` → `Vec3`): per-lane types flow
   to the correct zone-input pins.
6. Captures: body references an outer constant (depth-1 capture) — frozen once,
   correct per element; nested case: `zip_with` inside a `map` body with a
   deep capture (the eager-vs-lazy stack discipline is inherited, but assert
   it).
7. Wire conversions: an `Array[Int]` wired to an `Iter[Float]` lane converts;
   `Iter[Int] → Iter[Float]` lane wraps in `Convert` lazily.
8. Errors: unwired lane → `input is missing`; malformed body (no zone-output
   wire) → single construction-time error, not per-element.
9. Wired `f` (evaluation only): a `closure` node (`Custom`, 2 params) wired
   into `f` drives the zip; inline body ignored. Auto-partialization: a 3-param
   closure on a 2-lane zip yields `Function` elements (assert via `apply` on a
   collected element or the display string).
10. Laziness: `zip_with` feeding `fold` with a `print` in the body under a
    non-execute pass — pulls exactly `min(len)` times (reuse the counting
    patterns from `iter_type_test.rs` / `fold_test.rs`).

**Gate:** `cd rust && cargo test && cargo clippy && cargo fmt`.

## Phase 2 — `f`-derivation post-pass + validation polish

**Deliverables**

- `update_zip_with_pin_layouts_for_network` (+ `_preserving_args`) in
  `node_type_registry.rs`, sequenced after the apply post-pass in
  `validate_network`; shared-helper extraction from the map pass.
- Derived output type: exact-arity ⇒ `Iter[R]`; excess-arity ⇒
  `Iter[Function(tail → R)]`; disconnect restores stored `output_type`.
- `derived_shape` view populated for `zip_with` (drives the Phase 5 UI).
- `function_input_pin_connected` (`node_network.rs`) already locates the `f`
  pin by name + function shape rather than a hardcoded index, so zone rule-1
  suspension covers the trailing pin unchanged — add a pinning test only.
- `drag_hint_for_input_pin` for `f`: expose the concrete
  `(T_1 .. T_N) → output_type` so a `closure` dragged off the pin lands
  pre-shaped (mirrors `map.rs`).

**Automated tests** — `zip_with_test.rs` + `function_pin_unification_test.rs`
additions:

1. Wire a `(Int, Int) → Vec3` closure into a 2×Int-lane zip: output pin
   resolves to `Iter[Vec3]`; stored `output_type` untouched; disconnect
   restores `Iter[stored]`.
2. Starts-with acceptance matrix: `(Int, Int, Float) → R` accepted on
   `[Int, Int]` lanes (output `Iter[Function((Float) → R)]`); `(Float, Int)`
   source on `[Int, Int]` lanes follows the same pairwise-convertibility rules
   as `map.f` (assert whichever way `can_be_converted_to` resolves, as a
   pinning test); `(Bool, Int)` rejected at validation.
3. `f` wired + empty inline body: network stays valid (rule-1 suspension), and
   evaluation uses the wired closure.
4. Post-pass idempotence: two consecutive `validate_network` calls leave the
   custom type byte-identical (guards the recompute-every-node discipline).
5. Body-internal `zip_with` whose `f` is a cross-scope capture derives its
   layout (the scoped-recursion path).
6. Load-order: a `.cnnd` where the `f`-source lives in a later-loaded network —
   positional argument preservation must hold (same bug class as `apply`'s
   under-derived layout; reuse the `apply_function_pin_iter_test.rs` fixture
   pattern).

**Gate:** full `cargo test`; no new clippy warnings.

## Phase 3 — Lane editing: add / remove / retype + repair

**Deliverables**

- Lane-list mutation semantics, in two forms:
  - **Positional id merge** (the whole-list path: `set_text_properties` now,
    the API setter in Phase 5) — lane at position `i` keeps the old
    position-`i` id (retype preserves identity), grow mints fresh ids from
    `next_lane_id` (never reusing a consumed id), shrink drops the tail
    **and disconnects body wires referencing the dropped tail indices** (the
    same recursive depth-aware walk as removal, in disconnect-only form — no
    decrement needed for a tail drop; without it, nested depth ≥ 2 wires to
    dropped pins are only flagged red by rule 3, never cleaned), and an
    empty lane list is rejected with an error. There is no rename and no
    reorder, so this is the whole merge.
  - **Id-accurate removal of a specific lane** — a core operation implemented
    and tested **in this phase**: `ZipWithData::remove_lane(index)` **plus the
    mutation-time recursive body-wire remap described below**; Phase 5 merely
    exposes it over the API for the delete button. Surviving lanes keep their
    ids, so external wires follow them while the `xs{i}` labels renumber.
    Removing the last remaining lane is rejected.
- External wire preservation across removal via `Parameter.id` (verified:
  `node_network.rs::set_custom_node_type` rebuilds arguments by id-matching
  with name fallback, the mechanism `expr`/record nodes rely on).
- Body-wire remap on lane changes: retype → `repair_zone_body` disconnects
  incompatible wires (existing machinery); removal → body wires with the
  removed `ZoneInput { pin_index }` are disconnected and wires to later
  indices **decremented by `remove_lane` at mutation time** — a repair pass
  cannot do this (no removal diff; a shifted-but-in-range wire between
  same-typed lanes is silently wrong, not invalid). The remap walks the zip
  body **recursively into nested HOF bodies**, touching only wires matching
  `SourcePin::ZoneInput` + `source_node_id == zip_node_id` +
  `source_scope_depth == nesting distance from the zip body` (id alone is
  ambiguous — node ids collide across scopes via per-body `next_node_id`).
  The existing `repair_zone_body` deliberately skips depth ≥ 2 wires, so this
  recursion is new code, not an extension of it; rule 3 / repair remain a
  backstop for hand-authored files.
- Undo: **`ZipWithLaneEditCommand`** — whole-network before/after snapshots
  (`SerializableNodeNetwork`, the `TextEditNetworkCommand` /
  `DeleteNetworkCommand` pattern) of the owning **top-level** network, so it
  captures the full wire fallout (dropped external wire, body remap incl.
  nested bodies, retype-repair drops) that the generic node-data snapshot
  cannot — `snapshot_node_data` serializes only the `node_data_saver` blob,
  not `arguments` or the zone body. The `StructureDesigner`-level lane
  mutation ops — `set_zip_with_lanes(scope_path, node_id, lane_types)` for
  the positional merge and `remove_zip_with_lane(scope_path, node_id, index)`
  for id-accurate removal; used directly by tests here, wrapped by the API in
  Phase 5 — snapshot before the mutation, run validation (retype wire-drops
  happen there), snapshot after, and push the command only if the two differ.
  The text path does **not** push it — text edits are already covered by
  `TextEditNetworkCommand` (no double push).
- `default_display_all_output_pins` stays default-false (single `Iter` pin,
  nothing to show); `adapt_for_drag_source`: peel the drag source's element
  type into **lane 1 and `output_type`** (mirrors `map.rs`'s
  identity-shaped default), leaving lane 2 at its `Float` default — the
  popup's static-match verification only needs one connectable pin.

**Automated tests** — `zip_with_test.rs` (+ an invariants suite touch if the
identity-vs-naming property checker enumerates id-carrying pin providers):

1. Remove middle lane of 3 (id-accurate path): its external wire is dropped,
   the later lane's wire survives on the renumbered pin (old `xs3` → new
   `xs2`, same id); body wires to the removed `element2` are disconnected and
   wires to `element3` are remapped to index 2 — assert the **evaluation
   result** is unchanged for the surviving lanes, not just the structure; the
   network re-validates without manual fixes.
2. Nested-body remap: a `map` inside the zip body whose own body reads the
   zip's `element1` and `element3` at `source_scope_depth = 2`; remove lane 2
   → the depth-2 wire to `element3` is decremented to index 1, the wire to
   `element1` is untouched, and the map's **own** depth-1 `element` wires are
   untouched. Arrange the map node's id inside the zip body to numerically
   equal the zip node's id in the outer network, so the test also pins the
   depth+id matching (a match on `source_node_id` alone would corrupt the
   inner map's wires). Assert by evaluation result, not just structure.
3. Remove the last lane: trivial case — no renumbering, only the dropped
   lane's wires disappear.
4. Retype lane `Int → Crystal`: incompatible body wires disconnected
   (`repair_zone_body`), compatible ones kept; the external wire follows the
   usual wire-type revalidation.
5. Add lane: new `xs{N+1}` / `element{N+1}` pins appear unwired; existing
   wires and body untouched; fresh id ≠ any prior id — specifically, remove
   the **highest-id** lane then add a new one and assert the removed id is not
   recycled (the `next_lane_id` counter, not max+1; the recycling variant is
   exactly the `next_param_id` regression shape).
6. Positional text merge: shrinking / growing / retyping `lane_types` through
   `set_text_properties` preserves position-stable ids (retype keeps id, grow
   mints from `next_lane_id`, shrink drops tail). A shrink with a nested
   depth-2 wire to a dropped tail index **disconnects** that wire (not just
   flags it red).
7. Minimum arity enforced: `remove_lane` on a 1-lane node and an empty
   `lane_types` array through `set_text_properties` both return an error and
   leave the node unchanged.
8. Scalar broadcast (pinning test): bare `float` node wired to a lane → no
   validation error or warning, and the zip evaluates to exactly one element
   regardless of the other lanes' lengths.
9. Property-suite/invariants run stays green (`invariants.rs` checker via
   `validate_network` on all fixtures).
10. Undo/redo: lane add/remove/retype through `ZipWithLaneEditCommand`
    restores exact whole-network JSON state — including `next_lane_id` (the
    `next_node_id`/`next_param_id` comparison pitfall), the removed lane's
    **external wire**, and the disconnected/remapped **body wires** (exactly
    the state a node-data-only snapshot cannot restore; pattern from
    `undo_test.rs` / `normalize_json` helpers). Cover a remove with wires
    attached in both the immediate and a nested body.

**Gate:** full `cargo test`.

## Phase 4 — Text format + serialization round-trips

**Deliverables**

- `get_text_properties` / `set_text_properties` finalized: `lane_types` as
  `TextValue::Array` of `TextValue::DataType`, `output_type` as
  `TextValue::DataType`. Pin names (`xs1 …`) are derived, so lane edits through
  the text path use the positional id merge from Phase 3 (document that a
  middle-lane removal expressed in text rebinds positionally — the text format
  has no way to say "remove lane 2 specifically"; the id-accurate path is
  Phase 3's `remove_lane`, exposed to the UI in Phase 5).
- Serializer emits lane connections by derived pin name
  (`xs1: src1, xs2: src2, f: @g`); parser/editor resolve names → param indices
  via the custom node type (existing machinery; verify the dynamic-pin path
  used by `expr` covers zone-bearing nodes).
- `.cnnd` round-trip via `generic_node_data_saver`/`loader` (`id` and
  `next_lane_id` serialize with `#[serde(default)]`; the loader heals a
  missing/zero `next_lane_id` to max(lane ids)+1 **and mints ids from the
  healed counter for any lane loaded with `id: None`** — an id-less lane
  silently degrades to positional wire matching, defeating the ids).

**Automated tests** — `text_format_test.rs` module additions +
`cnnd_roundtrip_test.rs` + `node_snapshot_test.rs`:

1. Parse: `z = zip_with { lane_types: [Int, Int], output_type: Int, xs1: a,
   xs2: b, f: @g }` creates the node with correct lanes, wires, and `f`.
2. Serialize: a programmatically built 3-lane zip with body round-trips to the
   same canonical text (serialize → edit-replace → serialize, byte-equal).
3. Incremental edit: changing only `lane_types` through `edit_network`
   preserves position-stable ids/wires (exercises the Phase 3 merge through
   the text path).
4. `.cnnd` round-trip: save/load a network containing `zip_with` with a
   populated body, captures, and a wired `f`; `normalize_json` comparison is
   exact; a body-internal `zip_with` (nested in a `map` zone) also survives.
5. Snapshot: the registered node type's insta snapshot (pins, zone pins,
   description) blessed and stable.
6. Healing: a hand-authored `.cnnd` with lanes missing `id` and/or a
   missing/zero `next_lane_id` loads with fresh distinct lane ids and a
   consistent counter; a subsequent lane edit on the healed node preserves
   wires (proves the healed ids actually participate in preservation).

**Gate:** full `cargo test`, including `cargo test cnnd_roundtrip` and
`cargo test node_snapshots`.

## Phase 5 — API + Flutter UI

**Deliverables**

- API types in `rust/src/api/structure_designer/`: `APIZipWithData
  { lane_types: Vec<APIDataType>, output_type: APIDataType }`;
  `get_zip_with_data` / `set_zip_with_data`, both `#[frb(sync)]` and both
  taking **`scope_path`** (hard rule in `rust/AGENTS.md` — body-internal nodes
  must be addressable). An explicit
  `remove_zip_with_lane(scope_path, node_id, lane_index)` exposes Phase 3's
  id-accurate removal operation for the UI's delete button — a bare
  lane-type-list setter can only express the positional merge. Ids are
  managed Rust-side and never cross the API. Both mutators call the Phase 3
  `StructureDesigner`-level lane ops, so the `ZipWithLaneEditCommand` undo
  capture is shared — the API layer adds no undo logic of its own.
- `flutter_rust_bridge_codegen generate`.
- Model methods `getZipWithData` / `setZipWithData` / `removeZipWithLane`
  forwarding `propertyEditorScopeChain`, then `refreshFromKernel()` +
  `notifyListeners()`.
- `lib/structure_designer/node_data/zip_with_editor.dart`, registered in
  `node_data_widget.dart`: one row per lane — a fixed `xs{i}` label +
  `DataTypeInput` + delete button — with an "Add Input" button (no name
  fields), plus the Output Type field that swaps to the read-only "derived
  from f" display when `node.derivedShape?.derivedFromInputPin == 'f'`
  (reuse/extract `_DerivedOutputTypeDisplay` from `map_editor.dart` into a
  shared widget rather than copying).
- Node widget / zones UI: nothing new — zone-input pins, body rendering,
  collapse, and per-pin layout are generic; `estimate_node_height` /
  `getNodeSize()` already scale with `max(inputs, outputs)`.
- Docs: add `zip_with` to `nodes/AGENTS.md` (Iterators bullet),
  `doc/atomCAD_reference_guide.md` if node tables exist there, and the atomcad
  skill's node list if it enumerates types.

**Automated tests / checks**

1. Rust-side: the API layer is thin (per testing policy, wrappers may skip
   dedicated tests), but add one `zip_with_test.rs` case driving the same
   underlying setter/removal the API calls, against a **body-internal** node
   via `scope_path`, asserting the scoped resolution (the
   property-panel-wrong-node bug class).
2. `flutter analyze` clean (no new issues beyond the pre-existing baseline).
3. `dart format lib/`; `flutter test integration_test/` smoke stays green.
4. Manual smoke walkthrough (recorded in `node_data/AGENTS.md` per house
   convention):
   - Add `zip_with`; pins `xs1`, `xs2` render, body renders with two
     inside-facing pins `element1`, `element2`.
   - Add a third lane in the panel → `xs3` + `element3` appear; neighbours
     reflow like other in-body growth cases.
   - Build a sum in the body via `expr` (its params named by the user), wire
     two `range`s, `collect`, display → correct array; hover readout capped
     per `ITER_DISPLAY_CAP` rules.
   - Wire a `closure` into `f` → Output Type flips to the read-only derived
     display; disconnect → editable field restores.
   - Delete the middle of three lanes with wires attached → the last lane's
     wire survives on the renumbered `xs2`, the removed lane's wire drops,
     body wires remap in the same step, no red residue; Ctrl+Z restores all
     of it at once.
   - Wire a bare `float` to a lane → no error badge; the collected output has
     exactly one element (scalar 1-element broadcast, by design).

**Gate:** `cd rust && cargo fmt && cargo clippy && cargo test`,
`flutter analyze`, `dart format lib/`, integration smoke.

---

## Resolved questions

- **Scalar-broadcast handling** — resolved: **silently allow**. A scalar-fed
  lane is the ordinary `S → Iter[T]` 1-element broadcast and the zip truncates
  to one element; this is easy to debug when unintended, so no warning or
  rejection rule. The node's `description` points users at captures for the
  constant-per-element case. (A non-blocking warning was considered and
  dropped as noise.)

## Open questions

1. **Strict length variant.** A future `strict: bool` property could error on
   length mismatch instead of truncating. Deferred; shortest-input is the
   established default and lazy streams make "length" a non-local property.
2. **Post-pass code sharing.** How far to unify the apply/map/zip_with
   post-passes is an implementation-time judgment; the minimum bar is sharing
   the cross-scope `f`-source signature resolution. A third copy-paste of the
   full scoped-recursion skeleton is the ceiling of acceptable.
