# Drag-aware add-node popup

## Scope

When the user drags a wire from a pin and drops it on empty space, an "add node" popup opens with a list filtered to node types whose pins can connect to the dragged source. Today's filter only consults each candidate node type's **static default** pin types — it ignores the fact that several nodes (`map`, `filter`, `fold`, `collect`, `array_at`, `sequence`, …) have user-configurable type properties that determine their actual pin types at runtime. Concretely:

- Drag `Iter[Int]` from a `range` output → `map` is silently absent (its default `MapData::input_type` is `Float`, not `Int`), even though picking it and configuring its types to `Int` *would* work.
- If a type-parameterized node accidentally surfaces (say a destination expects the same type the node's defaults happen to declare), it's instantiated with default properties and the auto-connect step often fails to find a matching pin — the wire silently vanishes.

This document designs a single mechanism that fixes both halves: candidate nodes whose type properties *could* be configured to match the drag are surfaced in the popup, **and** when the user picks one, the new node is instantiated with type properties already set to make the connection work.

Out of scope:

- `expr`'s output type (driven by parsed expression text — no clean automatic adaptation).
- Custom networks (their parameters' types come from the network's own `parameter` nodes; those *do* get adapted via this mechanism, which means custom networks that re-export a single user parameter benefit transitively).
- `record_construct` / `record_destructure` / `product` schema selection — the property is a record-def *name* (not a `DataType`), so adapting requires a fuzzy lookup against `record_type_defs`. Possible follow-up; left out for v1.
- Auto-connect pin selection when multiple pins of the new node are compatible (already user-driven via the existing pin-picker dialog).

## Current state

| Concern | Location | What it does |
|---|---|---|
| Filter | `node_type_registry.rs:284-359` (`get_compatible_node_types`) | Iterates built-in + custom node types. For each, checks against **`node_type.parameters[i].data_type`** (static, from `&NodeType`) when dragging from output, and **`node_type.output_type()`** (static, pin 0) when dragging from input. Calls `DataType::can_be_converted_to`. **Never invokes `calculate_custom_node_type`.** |
| Create | `structure_designer.rs:1240-1380` (`add_node`) | Calls `(node_type.node_data_creator)()` to get a default `NodeData`, applies the `parameter`-specific bookkeeping (`param_id` / `param_name` / `sort_order`, lines 1264-1286), inserts into the network. No hook for drag-source-derived customization. |
| Auto-connect | `structure_designer_api.rs:647-663` (`get_compatible_pins_for_auto_connect`) | After the node exists, this *does* call `resolve_output_type_detailed` (which honors `calculate_custom_node_type`) — so once a node is correctly instantiated, the pin-level auto-connect step works. The gap is purely upstream: filter visibility and create-time property propagation. |
| Flutter popup | `lib/structure_designer/node_network/add_node_popup.dart:68-70` | Calls `getCompatibleNodeTypes(sourceTypeStr, draggingFromOutput)`. |
| Flutter create | `lib/structure_designer/node_network/node_network.dart:434-476` | `graphModel.createNode(name, position)` then `getCompatiblePinsForAutoConnect` + `autoConnectToNode` / `connectNodes`. |

The static-pin filter is fast and correct for nodes with statically-typed pins. The gap is exactly: nodes with at least one pin whose type comes from `calculate_custom_node_type`, against the stored `NodeData`, are filtered as if their stored data were always the `node_data_creator()` default.

## Design

### Trait hook

A new method on `NodeData`, with a no-op default:

```rust
// rust/src/structure_designer/node_data.rs (or a new sibling file)
pub enum DragDirection {
    /// User dragged from an *output* pin of `source_type`.
    /// Adapter must arrange for at least one of this node's *input* pins to accept it.
    FromOutput,
    /// User dragged from an *input* pin of `source_type`.
    /// Adapter must arrange for this node's *output* (pin 0) to satisfy it.
    FromInput,
}

pub trait NodeData {
    // ... existing methods ...

    /// Adapt this node's stored data so its pins line up with a dragged source pin.
    /// Returns the adapted data (typically a clone of `self` with type properties overwritten),
    /// or `None` if no adaptation can make the node compatible.
    ///
    /// Default: returns `None`. The drag filter falls back to a static-pin check for
    /// node types that don't override this — same path as today.
    fn adapt_for_drag_source(
        &self,
        _source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        None
    }
}
```

The adapter returns full owned `NodeData`, not a property delta. Returning a delta is more "minimal" but invents a new property-encoding type; returning a full `NodeData` reuses what the trait already passes around (`node_data_creator`, `clone_box`, etc.) and makes the per-node implementation a 5-line `clone_self_then_overwrite_a_field` pattern.

### Filter (`get_compatible_node_types`)

Two-step check per candidate node type:

```rust
for (name, node_type) in built_in_types.iter().chain(custom_types) {
    // 1. Static fast path. Covers every node with no type properties (sphere, cuboid, range, ...).
    if static_match(node_type, source_type, direction, self) {
        candidates.push((name, /* no override */));
        continue;
    }

    // 2. Adapter slow path. Only allocates / clones for type-parameterized nodes whose
    //    static defaults didn't match.
    let default_data = (node_type.node_data_creator)();
    let Some(adapted) = default_data.adapt_for_drag_source(source_type, direction, self) else {
        continue;
    };

    // The adapter said "yes I can adapt"; verify the *resolved* node type after adaptation.
    // calculate_custom_node_type returns Some(resolved) for type-parameterized nodes; for
    // statically-typed nodes it returns None (and we'd already have caught them in step 1).
    let resolved = adapted
        .calculate_custom_node_type(node_type)
        .unwrap_or_else(|| node_type.clone());
    if !static_match(&resolved, source_type, direction, self) {
        // Adapter overpromised. Skip rather than show a misleading entry.
        continue;
    }
    candidates.push((name, /* override */));
}
```

`static_match(...)` is exactly the predicate used today — pulled into a small helper so both paths use the same logic. The `calculate_custom_node_type` call is the same function the evaluator already runs every pass; calling it once per type-parameterized candidate during a popup open is negligible.

### Create-time API change

Two design options for getting the adapted data into the create call:

**Option A (chosen): stateless API, recompute at create time.**
Pass the source pin type + direction through to `add_node`. Rust recomputes the adapter using the same logic the filter used:

```rust
// rust/src/api/structure_designer/structure_designer_api.rs
pub fn add_node(
    node_type_name: &str,
    position: APIVec2,
    drag_source: Option<APIDragSource>,  // new, optional; existing callers pass None
) -> u64;

pub struct APIDragSource {
    pub source_pin_type: String,        // encoded via `DataType::Display` (see below)
    pub dragging_from_output: bool,
}
```

**Encoding of `source_pin_type`.** The string is produced by `DataType`'s `Display` impl (`data_type.rs:128`) and parsed server-side by `DataType::from_string` (`data_type.rs:505`) — exactly the same round-trip as today's `get_compatible_node_types(source_type_str, ...)`. The grammar covers all primitive types, `[T]` (Array), `Iter[T]`, `T -> U` / `(T,U) -> V` (Function), and `Record(Name)` (named records).

**Known limitation.** `DataType::from_string` does **not** parse anonymous-record syntax (`{x: Int, y: Int}`) — see the comment at `data_type.rs:580`. A drag from a pin whose type is `Record(Anonymous(_))` will fail to round-trip, and `add_node` should treat that as `drag_source = None` (silent fall-back to default data). This is harmless in v1 because none of the in-scope adapters target records — `record_construct` / `record_destructure` / `product` are already deferred under "Out of scope." If anonymous-record pins later need to drive adapters, the cleanest fix is to switch `APIDragSource::source_pin_type` from `String` to the existing structured `APIDataType` (already in the FFI; used by the schema editor) and update `get_compatible_node_types` in lockstep.

**Option B: server-side cache keyed by drag session.**
`get_compatible_node_types` stashes adapted data in a transient `StructureDesigner` slot; `add_node_for_drag(name, position)` consumes it. Cleaner-looking API but introduces session state ("when does the slot get cleared?", "what if the user opens two popups?"). Recomputing in option A is cheap (the adapter is pure pattern-matching) and avoids the state question entirely.

We pick option A.

### Instantiation order

Inside `structure_designer.rs::add_node`:

```rust
// 1. Default data.
let mut node_data = (node_type.node_data_creator)();

// 2. NEW: drag-source adapter, gated by the same static-match verification the filter runs.
//    Mirroring the filter's check at create time means the adapter contract is "fail-safe":
//    an over-promised adapter is silently dropped to default data instead of producing a
//    misconfigured node. This protects callers that bypass the popup (CLI/cli_runner.rs,
//    direct API/scripted invocations, stale popups after concurrent network mutations).
if let Some(drag) = drag_source {
    if let Some(adapted) = node_data.adapt_for_drag_source(
        &drag.source_type, drag.direction, &self.node_type_registry,
    ) {
        let resolved = adapted
            .calculate_custom_node_type(node_type)
            .unwrap_or_else(|| node_type.clone());
        if static_match(&resolved, &drag.source_type, drag.direction, &self.node_type_registry) {
            node_data = adapted;
        }
        // else: adapter overpromised — keep the default node_data.
    }
}

// 3. Existing parameter-node special case (lines 1266-1289 in current code): assigns
//    param_id, param_name, sort_order. These fields are orthogonal to type properties —
//    the parameter adapter only touches data_type, so the special case happily writes its
//    bookkeeping fields on top of the adapted data.

// 4. Insert into network (`network.add_node(node_type_name, position, num_parameters, node_data)`).

// 5. Existing call to `populate_custom_node_type_cache_with_types(..., node, true)`
//    (lines 1302-1322) — this recomputes the node's resolved pin types from its `NodeData`,
//    so it MUST run after the adapter has mutated `node_data`. It already does (the adapter
//    sits before insertion, the cache call sits after); spelling it out so nobody reorders
//    the steps in a future cleanup.
```

Adapter runs *before* the parameter special-case so the special-case retains authority over `param_id` / `param_name` / `sort_order` — those are network-level bookkeeping, not user-configurable types.

The static-match gate in step 2 lets per-node adapters be loose: e.g. `MapData::adapt_for_drag_source` reuses `drag_element_type_from_output` for both directions even though the `FromInput` case can't actually accept scalar broadcast (map's output is `Iter[T]`). The filter's verification step caught those at popup-open time; replicating it here means create-time is just as safe.

### Flutter integration

Single callsite in `node_network.dart` (around line 434). The popup widget already knows the drag source and direction (they're the two arguments it received as `filterByCompatibleType` and `draggingFromOutput`); pass them through to `createNode`:

```dart
// Before (current):
final newNodeId = widget.graphModel.createNode(selectedNodeType, logicalPosition);

// After:
final newNodeId = widget.graphModel.createNode(
  selectedNodeType,
  logicalPosition,
  dragSource: dragSourceType == null ? null : ApiDragSource(
    sourcePinType: dragSourceType,
    draggingFromOutput: draggingFromOutput,
  ),
);
```

`createNode` (in `StructureDesignerModel`) forwards to `sd_api.addNode` with the new argument. The auto-connect step (`getCompatiblePinsForAutoConnect` + `autoConnectToNode` / `connectNodes`) is unchanged — once the node is instantiated with the right type properties, those calls already see the resolved pin types.

The popup widget's filter call (`getCompatibleNodeTypes`) is unchanged in shape — the filter improvements are entirely Rust-side.

## Per-node adapters

| Node | Property/properties | `FromOutput` (drag from value pin) | `FromInput` (drag from consumer pin) | Notes |
|---|---|---|---|---|
| `map` | `input_type`, `output_type` | source `Iter[T]` / `Array[T]` / `T` → `input_type=T, output_type=T` | source `Iter[U]` / `Array[U]` → `input_type=U, output_type=U` | Tweak: `output_type=input_type` (and vice versa) for convenience — users typically tweak only one afterwards. |
| `filter` | `element_type` | source `Iter[T]` / `Array[T]` / `T` → `element_type=T` | source `Iter[T]` / `Array[T]` → `element_type=T` | Filter preserves type, both directions identical shape. |
| `fold` | `element_type`, `accumulator_type` | source `Iter[T]` / `Array[T]` / `T` → `element_type=T, accumulator_type=T` | source `T` → `element_type=T, accumulator_type=T` | Tweak: `Acc=T` by default. |
| `collect` | `element_type` | source `Iter[T]` / `Array[T]` → `element_type=T` (no scalar broadcast — `collect` is meant for streams) | source `Array[T]` → `element_type=T` | |
| `range` | (none) | static match only | static match only | Output is always `Iter[Int]`; no adapter needed. |
| `sequence` | `element_type` | source `T` → `element_type=T` (sequence's pins are all element-typed) | source `Array[T]` → `element_type=T` | |
| `array_at` | `element_type` | source `Array[T]` → `element_type=T` (matches the `array` pin) | source `T` → `element_type=T` (output is element type) | |
| `array_len` | `element_type` | source `Array[T]` → `element_type=T` | static — output is always `Int`, no adapter needed | |
| `array_concat` | `element_type` | source `Array[T]` → `element_type=T` | source `Array[T]` → `element_type=T` | |
| `array_append` | `element_type` | source `Array[T]` *or* `T` → `element_type=T` (two compatible pins; auto-connect picker handles disambiguation) | source `Array[T]` → `element_type=T` | |
| `parameter` | `data_type` | source `T` → `data_type=T` (the `default` input pin then accepts `T`) | source `T` → `data_type=T` (the output is `data_type`) | The `FromInput` case is the user-asked-for "drag from input pin → spawn parameter that feeds it." See §"Parameter node specifics". |

### Element-type extraction helper

Almost every adapter peels `Iter[T]` / `Array[T]` / `T` to find the element type. Factor a helper into `data_type.rs`:

```rust
impl DataType {
    /// For drag-from-output: extract the "element type" from a value-producing pin.
    /// Used by adapters that want to set a stored `element_type` to match the source.
    pub fn drag_element_type_from_output(&self) -> Option<DataType> {
        match self {
            DataType::Iterator(t) | DataType::Array(t) => Some((**t).clone()),
            DataType::Function(_) => None,                            // can't be an element
            t if t.is_abstract() => None,                              // require concrete
            t => Some(t.clone()),                                      // single-element broadcast
        }
    }

    /// For drag-from-input: same extraction, but rejecting scalar broadcast where the
    /// adapter's downstream connection wouldn't make sense (e.g. `collect`).
    pub fn drag_element_type_from_input_strict(&self) -> Option<DataType> {
        match self {
            DataType::Iterator(t) | DataType::Array(t) => Some((**t).clone()),
            _ => None,
        }
    }
}
```

Adapters call whichever helper fits and apply the result. Keeps each per-node `adapt_for_drag_source` to ~5-10 lines of straight match-and-clone.

### Worked example: `map`

```rust
impl NodeData for MapData {
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        let elem = match direction {
            DragDirection::FromOutput => source_type.drag_element_type_from_output()?,
            DragDirection::FromInput  => source_type.drag_element_type_from_output()?,  // same shape
        };
        Some(Box::new(MapData {
            input_type: elem.clone(),
            output_type: elem,
        }))
    }
}
```

### Parameter node specifics

The user's motivating case: drag from an input pin of type `T` and have `parameter` show up so picking it spawns a parameter feeding that pin. Implementation:

```rust
impl NodeData for ParameterData {
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // Reject types that don't make sense as parameter declarations.
        if source_type.is_abstract() || matches!(source_type, DataType::Function(_)) {
            return None;
        }
        let mut adapted = self.clone();
        adapted.data_type = source_type.clone();
        Some(Box::new(adapted))
        // Both directions adapt identically: a parameter's output and its `default` input
        // are both typed by `data_type`, so setting `data_type` covers both drag directions.
    }
}
```

The instantiation-order discussion in §"Instantiation order" is specifically motivated by this node — the existing parameter special-case in `add_node` (lines 1264-1286) overwrites `param_id` / `param_name` / `sort_order`, none of which the adapter sets, so the two passes compose cleanly.

## Backward compatibility

Purely additive:

- New trait method has a no-op default — existing `NodeData` impls compile unchanged.
- New API parameter is optional (`Option<APIDragSource>`); existing callsites that pass `None` (or, in dynamically-typed FFI: omit it) get today's behavior.
- No `.cnnd` file format change.
- No `SERIALIZATION_VERSION` bump.

## Tests

### Adapter unit tests (`tests/structure_designer/drag_adapter_test.rs`)

One file, one test per adapter × direction × representative source type. Roughly:

| Adapter | Drag direction | Source type | Expected adapted properties |
|---|---|---|---|
| `map` | FromOutput | `Iter[Int]` | `input_type=Int, output_type=Int` |
| `map` | FromOutput | `Array[Float]` | `input_type=Float, output_type=Float` |
| `map` | FromOutput | `Vec3` (broadcast) | `input_type=Vec3, output_type=Vec3` |
| `map` | FromOutput | `HasAtoms` (abstract) | `None` |
| `map` | FromInput | `Iter[Crystal]` | `input_type=Crystal, output_type=Crystal` |
| `filter` | FromOutput | `Iter[IVec3]` | `element_type=IVec3` |
| `fold` | FromOutput | `Iter[Float]` | `element_type=Float, accumulator_type=Float` |
| `fold` | FromInput | `Int` | `element_type=Int, accumulator_type=Int` |
| `collect` | FromOutput | `Iter[Int]` | `element_type=Int` |
| `collect` | FromOutput | `Int` (scalar) | `None` (no broadcast for collect) |
| `collect` | FromInput | `Array[Float]` | `element_type=Float` |
| `array_at` | FromOutput | `Array[IVec3]` | `element_type=IVec3` |
| `array_at` | FromInput | `Float` | `element_type=Float` |
| `array_len` | FromInput | `Int` | `None` (static match handles it; adapter not invoked) |
| `array_concat` | FromOutput | `Array[Bool]` | `element_type=Bool` |
| `array_append` | FromOutput | `Array[Int]` | `element_type=Int` (matches `array` pin) |
| `array_append` | FromOutput | `Int` | `element_type=Int` (matches `element` pin) |
| `sequence` | FromInput | `Array[Foo]` | `element_type=Foo` |
| `sequence` | FromOutput | `Foo` | `element_type=Foo` |
| `parameter` | FromInput | `Crystal` | `data_type=Crystal` |
| `parameter` | FromInput | `HasAtoms` | `None` |
| `parameter` | FromInput | `Function(_)` | `None` |
| `parameter` | FromOutput | `Int` | `data_type=Int` |

Each test constructs the node's default `NodeData`, calls `adapt_for_drag_source`, and asserts on the relevant properties of the returned data (downcasting via `as_any().downcast_ref::<MapData>()` etc.).

### Filter integration tests (`tests/structure_designer/get_compatible_node_types_test.rs`)

One test per shape:

| Source pin (type, direction) | Expected to surface |
|---|---|
| `Iter[Int]` from output | `range`-static is irrelevant (no static input pin matches); `map`, `filter`, `fold`, `collect` all surface via adapter; `array_at`/`len`/`concat`/`append` surface only if their static defaults happen to align (test their absence then re-test with adapter to confirm appearance). |
| `Iter[Crystal]` from output | `map`, `filter`, `fold`, `collect` surface; built-in scalar nodes do not. |
| `Array[Foo]` from output | `array_at`, `array_len`, `array_concat`, `array_append`, `collect`, plus iterator nodes via the implicit `Array[T] → Iter[T]` rule. |
| `Int` from input (dragged target) | `int`, `expr`, `value`, `parameter`, `array_at`, `array_len`, `fold` (Acc=Int), … all the producers of `Int`. |
| `Function(_)` from output | Restricted set; `parameter` rejects. |

### Create-time tests (`tests/structure_designer/structure_designer_test.rs` — extend existing)

| Scenario | Assertion |
|---|---|
| `add_node("map", _, drag_source=Iter[Int]/FromOutput)` | Created node's `MapData::input_type == Int && output_type == Int`. |
| `add_node("parameter", _, drag_source=Crystal/FromInput)` | Created node's `ParameterData::data_type == Crystal`, `param_id` assigned, `param_name == "param0"` (or whatever the next slot is). |
| `add_node("range", _, drag_source=Iter[Int]/FromInput)` | No adapter applies (returns `None`); created with default data; output still `Iter[Int]`. |
| `add_node("map", _, drag_source=None)` | Backward-compat: created with default `MapData`. |

### Flutter smoke (manual)

In a fresh project:

1. Drop a `range` node. Drag from its `Iter[Int]` output → drop on empty space. Popup should show `map`, `filter`, `fold`, `collect` (and array nodes via `[T] → Iter[T]`). Pick `map`. New `map` should have its `Input type` and `Output type` properties both set to `Int`. Auto-connect should wire `range.output → map.xs` without intervention.
2. Drop a `cuboid` node. Drag from one of its input pins (e.g. an `IVec3` pin) → drop on empty space. Popup should show `parameter`. Pick it. New `parameter` should have `data_type == IVec3`, default name `param0`, and auto-connect should wire `parameter.output → cuboid.<that pin>`.
3. Reverse direction: drop a `map` configured for `Float`. Drag from its `Iter[Float]` output. Popup shows `fold`. Pick it. New `fold` has `element_type=Float, accumulator_type=Float`.

## Implementation phases

Each phase is independently shippable and testable.

### Phase 1: Trait hook + filter wiring + iterator adapters + create-API plumbing

Phases 1 and 2 are merged: shipping the trait + filter refactor without any overrides leaves the slow path dead-coded and untested, so the iterator adapters (which exercise it) and the create-time API (which depends on the trait) all land together.

1. Add `DragDirection` enum and `NodeData::adapt_for_drag_source` trait method (default `None`).
2. Add the two `DataType::drag_element_type_*` helpers to `data_type.rs`.
3. Refactor `get_compatible_node_types` to use the static-match helper + adapter slow path.
4. Add `APIDragSource` to the API types layer (`api/structure_designer/structure_designer_api_types.rs`).
5. Extend `add_node` (Rust + API + Flutter `createNode` signature) to accept `Option<APIDragSource>`, including the create-time static-match verification described in §"Instantiation order". Behavior on `None`: identical to today.
6. Implement `adapt_for_drag_source` for `range` (no-op), `map`, `filter`, `fold`, `collect`.
7. Wire `node_network.dart`'s `createNode` callsite to pass the popup's known `dragSourceType` + `draggingFromOutput`.
8. **Test scope:**
   - Unit tests on the `drag_element_type_*` helpers.
   - Adapter unit tests for each iterator node (both directions).
   - Filter-level test for at least one type-parameterized adapter (e.g. `map` with `Iter[Int]` from output) so the slow path is actually exercised — the static-match-only fast path is covered by the existing `node_type_registry_test.rs` cases.
   - Create-time tests that exercise `add_node(..., drag_source=...)` for at least `map` (FromOutput) and `collect` (FromOutput), plus one create-time fall-back test that passes a `drag_source` an adapter would over-promise on (e.g. `map` with scalar `Int` / `FromInput`) and asserts the node is created with default data, not the adapter's output.
9. **Manual smoke:** the iterator scenarios from §"Flutter smoke (manual)" §1 and §3.

### Phase 2: Array-node adapters

1. Implement `adapt_for_drag_source` for `array_at`, `array_len`, `array_concat`, `array_append`, `sequence`.
2. **Test scope:** adapter unit tests for each.
3. **Manual smoke:** drag from an `Array[Foo]` source — verify `array_at` / `array_len` / `array_concat` / `array_append` / `sequence` / `collect` all surface; pick each in turn and confirm `element_type=Foo` and the connection works.

### Phase 3: Parameter-node adapter

1. Implement `adapt_for_drag_source` for `ParameterData` (both directions).
2. Verify the instantiation order in `add_node` — adapter runs before the parameter special-case bookkeeping; confirm `data_type` survives, `param_id` / `param_name` / `sort_order` get assigned by the existing special-case.
3. **Test scope:** adapter unit tests + a create-time test that exercises `add_node("parameter", _, drag_source=Crystal/FromInput)` and asserts both the adapter-set `data_type` and the special-case-assigned `param_id`.
4. **Manual smoke:** scenario §2 from §"Flutter smoke (manual)".

## Open questions / left for follow-up

1. **`record_construct` / `record_destructure` / `product`** — these have a `schema` / `target` `String` property naming a record def, not a `DataType`. Adapter would need to find a record def whose schema matches the drag source's element type, which is more involved (multiple defs may match, none may match, the user might want a *new* def). Defer until the v1 mechanism is in place; the per-node adapter slot is ready for them when the lookup logic is decided.
2. **`expr` adapter** — `expr`'s output type comes from parsing the user-authored expression; there's no clean way to adapt without writing a default expression. Possibility: adapt to `expr` with a single parameter typed `T` and a body that is just the parameter name (so the output is `T`). Useful but borderline; punt.
3. **Custom-network adapters** — a custom network whose return-node passes through a single parameter's type *could* surface in the drag popup with that parameter's type set to the source. Doable via a default `adapt_for_drag_source` impl on `CustomNodeData` that walks the network's signature and adapts a single parameter. Significantly more complex than the per-node adapters; defer to a follow-up.
4. **`is_abstract` rejection consistency** — every adapter currently rejects abstract source types (`HasAtoms`, `HasStructure`, `HasFreeLinOps`) and `Function(_)`. Worth lifting that check into the filter so adapters don't have to repeat it. Cosmetic; do it once two or three adapters duplicate the boilerplate.
5. **Multiple adapters per node** — could a node have two valid adaptations for the same source (e.g. `array_append` accepting either `Array[T]` or `T` from a `FromOutput` drag of type `T`)? The current design returns at most one adapted `NodeData`; the auto-connect step's existing pin-picker handles which pin gets wired. This is fine because both pins use the same `element_type`, so one adaptation covers both. If a future node has two type properties whose *separate* settings would each yield a valid match, the design would need to surface multiple popup entries — not currently anticipated.
6. **Adapter timing in custom networks** — `add_node` calls `adapt_for_drag_source` against the *built-in* registry; should a custom-network instance node receive the same treatment? In v1, custom-network nodes can't adapt (open question 3 above), so this doesn't bite. Worth revisiting when (3) is tackled.
