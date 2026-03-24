# Multi-Output Pins Design

## Status: Draft

## Motivation

Currently, each node has exactly one result output pin (index 0) and one implicit function pin (index -1). The `atom_edit` node works around this limitation via a boolean `output_diff` flag that switches its single output between the applied result and the raw diff. This is cumbersome: the user must toggle a flag and re-display to switch views, and cannot see both simultaneously.

Multi-output pins allow a single node evaluation to produce multiple named results, each independently connectable and displayable. The first consumer is `atom_edit`, which will have a "result" pin and a "diff" pin.

## Current System Summary

### Pin Indexing

| Index | Meaning |
|-------|---------|
| -1 | Function pin (implicit, returns `Closure`) |
| 0 | Result pin (the node's `output_type`) |
| ≥1 | Reserved but unused — `get_output_pin_type()` returns `DataType::None` |

### Key Data Structures

- **`NodeType.output_type: DataType`** — single output type
- **`Node.arguments: Vec<Argument>`** — each `Argument` has `argument_output_pins: HashMap<u64, i32>` mapping source node ID → output pin index
- **`Wire`** — `source_output_pin_index: i32` references the output pin
- **`NodeData::eval()` → `NetworkResult`** — returns a single value
- **`NodeView`** (API) — has `output_type: String` (singular)
- **`displayed_node_ids: HashMap<u64, NodeDisplayType>`** — per-node, not per-pin

### Evaluation Flow

1. `generate_scene()` iterates `displayed_node_ids`
2. For each node, calls `evaluate(node_id, output_pin_index=0, ...)`
3. `evaluate()` dispatches: pin -1 → `Closure`, pin 0 → `NodeData::eval()`
4. Result converted to `NodeOutput` based on `NodeType.output_type`
5. Custom network nodes evaluate their `return_node_id` at pin 0

## Design

### Design Principles

1. **Single evaluation, multiple results.** A node evaluates once and returns all its outputs. No separate evaluation per pin.
2. **Uniform representation.** All output pins (including the primary) live in one `Vec`. No special-casing of pin 0 in the data structure. Accessor functions provide convenient access to the primary (index 0) output for code that only cares about one output.
3. **Pin 0 remains the primary output.** This preserves the return node semantics, custom network output type, and all existing wiring. But this is a semantic convention, not a structural split.
4. **Display is per output pin, not per node.** Each output pin of a displayed node can independently be shown/hidden in the viewport.

### Rust Backend Changes

#### 1. NodeType: Unified Output Pin List

Replace `output_type: DataType` with a unified `output_pins: Vec<OutputPinDefinition>`:

```rust
// node_type.rs

/// Definition of an output pin.
#[derive(Clone, Debug)]
pub struct OutputPinDefinition {
    pub name: String,        // e.g. "result", "diff"
    pub data_type: DataType,
}

pub struct NodeType {
    pub name: String,
    pub description: String,
    pub summary: Option<String>,
    pub category: NodeTypeCategory,
    pub parameters: Vec<Parameter>,
    pub output_pins: Vec<OutputPinDefinition>,  // replaces `output_type: DataType`
    pub public: bool,
    pub node_data_creator: fn() -> Box<dyn NodeData>,
    // ... saver, loader ...
}
```

**Accessor functions** for backward-compatible access:

```rust
impl NodeType {
    /// The primary output type (pin 0). Panics if no output pins.
    pub fn output_type(&self) -> &DataType {
        &self.output_pins[0].data_type
    }

    /// Output type for any pin index.
    pub fn get_output_pin_type(&self, output_pin_index: i32) -> DataType {
        if output_pin_index == -1 {
            self.get_function_type()
        } else {
            self.output_pins
                .get(output_pin_index as usize)
                .map(|p| p.data_type.clone())
                .unwrap_or(DataType::None)
        }
    }

    /// Number of result output pins (excludes function pin).
    pub fn output_pin_count(&self) -> usize {
        self.output_pins.len()
    }

    /// Whether this node type has multiple output pins.
    pub fn has_multi_output(&self) -> bool {
        self.output_pins.len() > 1
    }
}
```

**Migration of node registrations.** Every node type currently written as:

```rust
NodeType {
    output_type: DataType::Atomic,
    ...
}
```

Changes to:

```rust
NodeType {
    output_pins: vec![OutputPinDefinition {
        name: "result".to_string(),
        data_type: DataType::Atomic,
    }],
    ...
}
```

This is mechanical (search-and-replace with a minor template). We can also provide a helper:

```rust
impl OutputPinDefinition {
    pub fn single(data_type: DataType) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition {
            name: "result".to_string(),
            data_type,
        }]
    }
}
```

So node registrations become `output_pins: OutputPinDefinition::single(DataType::Atomic)`.

**All code that currently reads `node_type.output_type`** migrates to `node_type.output_type()` (the accessor). This is also mechanical — the accessor returns `&DataType` just like the old field. In early phases, all such code continues to only use pin 0.

#### 2. NodeData::eval() Returns Multiple Results

```rust
// node_data.rs — new return type
pub struct EvalOutput {
    pub results: Vec<NetworkResult>,  // index 0 = pin 0, index 1 = pin 1, ...
}

impl EvalOutput {
    /// Convenience for single-output nodes.
    pub fn single(result: NetworkResult) -> Self {
        EvalOutput { results: vec![result] }
    }

    /// Multi-output constructor.
    pub fn multi(results: Vec<NetworkResult>) -> Self {
        EvalOutput { results }
    }

    /// Get result for a given output pin index.
    pub fn get(&self, output_pin_index: i32) -> NetworkResult {
        self.results
            .get(output_pin_index as usize)
            .cloned()
            .unwrap_or(NetworkResult::None)
    }

    /// Get the primary (pin 0) result.
    pub fn primary(&self) -> &NetworkResult {
        &self.results[0]
    }
}
```

**Migration strategy:** Change `NodeData::eval()` signature to return `EvalOutput`. All existing node implementations change their `return result;` to `return EvalOutput::single(result);` — a mechanical, one-line change per node.

**Alternative considered: keeping `NetworkResult` and adding `NetworkResult::Multi(Vec<NetworkResult>)`.** Rejected because it would require every consumer of `eval()` to pattern-match on `Multi` vs. a direct value, and it conflates the evaluation result structure with the value type system. `EvalOutput` is a clean wrapper that separates "how many pins" from "what type of value."

#### 3. Evaluator: Dispatch Per Pin from EvalOutput

The evaluator currently calls `eval()` and returns the `NetworkResult`. With multi-output, `evaluate()` calls `eval()` and extracts the requested pin:

```rust
// network_evaluator.rs — evaluate()
pub fn evaluate(..., output_pin_index: i32, ...) -> NetworkResult {
    if output_pin_index == -1 {
        // Function pin logic unchanged
        return NetworkResult::Function(Closure { ... });
    }

    // Evaluate node — returns all outputs
    let eval_output = node.data.eval(self, network_stack, node_id, registry, decorate, context);

    // Extract the requested pin
    eval_output.get(output_pin_index)
}
```

**No new eval caching.** The evaluator does not currently cache eval results, and redundant evaluation of fan-out nodes already exists today. Multi-output does not fundamentally change this: a node with two downstream consumers is already evaluated twice regardless of whether it has one or two output pins. Adding an eval cache would be a separate performance improvement orthogonal to this feature. We keep the existing evaluation semantics unchanged.

#### 4. generate_scene(): Display Per Output Pin

Currently `displayed_node_ids: HashMap<u64, NodeDisplayType>` tracks which nodes are visible. We need to know *which output pin* to display.

**Approach:** Keep `displayed_node_ids` for node-level display state (is this node visible at all?), add `displayed_output_pins: HashMap<u64, HashSet<i32>>` for per-pin control.

```rust
// node_network.rs
pub struct NodeNetwork {
    // ... existing ...
    pub displayed_node_ids: HashMap<u64, NodeDisplayType>,  // unchanged
    pub displayed_output_pins: HashMap<u64, HashSet<i32>>,  // NEW: node_id → set of displayed pin indices
    // ...
}
```

**Behavior:**
- A node is in `displayed_node_ids` = it is "visible" (unchanged semantics).
- `displayed_output_pins` says *which pins* of that visible node are rendered in the viewport.
- When `displayed_output_pins` has no entry for a displayed node, **all** output pins are displayed (backward compat default — for single-output nodes this means pin 0; for multi-output nodes this means all pins). Alternative: default to pin 0 only. **Decision: default to pin 0 only.** This is safer — showing everything by default could be surprising for multi-output nodes. The absence of an entry means "legacy node, show pin 0."
- Display policy resolver (Manual, Selected, Frontier) continues to operate at node level. Pin-level display is always explicit/manual.

**Scene generation:**

`generate_scene()` is called once per displayed node. It now evaluates the node (via cached `EvalOutput`) and produces `NodeSceneData` containing outputs for all displayed pins of that node.

```rust
pub struct NodeSceneData {
    pub outputs: Vec<DisplayedPinOutput>,  // one per displayed pin
    pub node_errors: HashMap<u64, String>,
    pub node_output_strings: HashMap<u64, String>,
    pub unit_cell: Option<UnitCellStruct>,
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
}

pub struct DisplayedPinOutput {
    pub pin_index: i32,
    pub output: NodeOutput,
    pub geo_tree: Option<GeoNode>,
}
```

**Phase 1 simplification:** In the early phases, `NodeSceneData` keeps its current single `output: NodeOutput` field. We always evaluate and display pin 0 only. The multi-output display logic is added in a later phase. This lets us land the `NodeType` and `EvalOutput` changes without touching the renderer/Flutter display pipeline.

#### 5. Custom Network Nodes (Return Node)

When a custom network is used as a node in another network, the evaluator currently calls:
```rust
evaluate(child_network_stack, child_network.return_node_id.unwrap(), 0, ...)
```

**For multi-output custom networks:** The return node's outputs become the custom node's outputs. The evaluator dispatches:
```rust
evaluate(child_network_stack, child_network.return_node_id.unwrap(), output_pin_index, ...)
```

The parent network's `NodeType.output_pins` should mirror the return node's output pins. `update_network_output_type()` in the validator already handles updating `output_type` from the return node — extend it to also update the full `output_pins` list.

This is a later phase. Initially, custom networks continue to expose only pin 0.

#### 6. Wire Connection Compatibility

**No changes needed to `Wire` or `Argument`.** The wire's `source_output_pin_index: i32` already supports positive indices. `connect_nodes()` works as-is. Type validation at connection time just needs to call `get_output_pin_type(pin_index)` which already dispatches correctly with the new `output_pins` vec.

#### 7. Serialization (.cnnd)

**NodeType serialization.** Currently serializes `output_type` as a single string. Change to serialize `output_pins` as an array of `{name, data_type}` objects.

**Backward compatibility on load:**
- If the JSON has old-style `"output_type": "Atomic"`, convert to `output_pins: [{ name: "result", data_type: "Atomic" }]`.
- If the JSON has new-style `"output_pins": [...]`, use directly.
- On save, always write new format. Old atomCAD versions won't load new files, but this is acceptable for a forward-moving format.

**`displayed_output_pins` serialization:**
- Omit from JSON if empty (all nodes default to pin 0 display).
- On load, if missing, default to empty HashMap.

### Impact on Existing Caches

The codebase has several caching mechanisms. Here's how each is affected by the multi-output change:

#### 1. CSG Conversion Cache (`CsgConversionCache` on `NetworkEvaluator`)

- **What:** Caches `GeoNode` → `CSGMesh`/`CSGSketch` conversions using BLAKE3 content hashes. Memory-bounded (200 MB mesh + 56 MB sketch).
- **Impact: None.** This cache operates on `GeoNode` trees, not on `NetworkResult` or eval output. The CSG conversion happens *after* evaluation, in `generate_explicit_mesh_output()`. The eval return type change doesn't affect it.

#### 2. Invisible Node Cache (`MemoryBoundedLruCache<u64, NodeSceneData>` in `StructureDesignerScene`)

- **What:** When a node becomes invisible (display toggled off), its `NodeSceneData` is moved to an LRU cache (256 MB). When it becomes visible again, the cached data is restored instantly without re-evaluation.
- **Impact: Affected in later phases.** Currently keyed by `node_id` and stores one `NodeSceneData` with one `output: NodeOutput`. When we extend `NodeSceneData` to hold multiple pin outputs (Phase 2), the cache automatically stores/restores all of them — no key change needed since it's still per node. The `estimate_memory_bytes()` method will need to account for additional outputs in the size estimate.
- **Phase 1: No change.** `NodeSceneData` keeps its single `output` field; the cache works as before.

#### 3. Selected Node Eval Cache (`NetworkEvaluationContext.selected_node_eval_cache`)

- **What:** A `Box<dyn Any>` slot populated by specific nodes (e.g., `atom_rot`, `add_hydrogen`) during eval to pass gadget metadata (pivot points, axes, messages) to the UI.
- **Impact: None.** This stores node-specific gadget data, not the eval result itself. It's populated inside `eval()` via `context.selected_node_eval_cache = Some(...)`. The eval return type change doesn't affect how nodes populate this.

#### 4. atom_edit `cached_input` (`Mutex<Option<AtomicStructure>>` on `AtomEditData`)

- **What:** Caches the upstream input structure to avoid re-evaluating the parent DAG during interactive atom editing (dragging, tool operations). Cleared on full refresh via `clear_input_cache()`.
- **Impact: None.** This caches the *input* to atom_edit (the upstream node's result), not atom_edit's own output. It's populated inside `eval()` by calling `evaluate_arg_required()`, which returns `NetworkResult` (not `EvalOutput`). The upstream node's eval produces `EvalOutput`, but `evaluate()` extracts the single `NetworkResult` for the requested pin before returning it — so `evaluate_arg_required()` still returns `NetworkResult`.

#### 5. Per-Evaluation Context (`node_errors`, `node_output_strings`)

- **What:** `HashMap<u64, String>` maps populated during DAG traversal. Error messages from `NetworkResult::Error(...)` and display strings from `result.to_display_string()`. Copied into `NodeSceneData` after evaluation.
- **Impact: Minor.** Currently populated from the single `NetworkResult` returned by `evaluate()`. With multi-output, `evaluate()` still returns a single `NetworkResult` (for the requested pin), so the error/string capture code in `evaluate()` works as-is. However, for multi-output nodes, errors from pin 1 evaluation need consideration:
  - Errors are accumulated inside `eval()` itself (not per-pin in the evaluator).
  - `to_display_string()` is called on the result of `evaluate()` (one specific pin). For multi-output, the display string reflects whichever pin was requested. This is fine — the display string shown on the node widget comes from the primary pin.
  - **No code change needed in Phase 1.** In later phases, we may want per-pin error/display strings, but this can be addressed when the UI supports it.

#### 6. Undo/Redo Snapshots

- **What:** `NodeSnapshot` and `SerializableNodeNetwork` store serialized node data (JSON) for undo/redo.
- **Impact: None on snapshot mechanism.** Snapshots store serialized `NodeData`, not eval output. The `output_pins` change in `NodeType` will be reflected in `SerializableNodeNetwork`'s `node_type` field when serialized, but this is handled by the serialization migration (section 7 above), not by undo-specific code. The `displayed_output_pins` field is new network state that must be included in `SerializableNodeNetwork` — covered in the undo section below.

### atom_edit Node Changes

The `atom_edit` node currently has:
- `output_type: DataType::Atomic` (single output)
- `output_diff: bool` flag to switch between result and diff views

With multi-output:

**Registration:**
```rust
NodeType {
    name: "atom_edit".to_string(),
    output_pins: vec![
        OutputPinDefinition {
            name: "result".to_string(),
            data_type: DataType::Atomic,  // pin 0: applied result
        },
        OutputPinDefinition {
            name: "diff".to_string(),
            data_type: DataType::Atomic,  // pin 1: raw diff
        },
    ],
    // ...
}
```

**eval() change:**
```rust
fn eval(&self, ...) -> EvalOutput {
    let input = evaluate_arg(node_id, 0);

    // Always compute the applied result (pin 0)
    let diff_result = apply_diff(&input, &self.diff, self.tolerance);
    let mut result = diff_result.result;
    // ... apply frozen flags, decorations, etc.

    // Always compute the diff output (pin 1)
    let mut diff_output = self.diff.clone();
    if self.include_base_bonds_in_diff {
        enrich_diff_with_base_bonds(&mut diff_output, &input, self.tolerance);
    }
    // ... apply diff decorations

    EvalOutput::multi(vec![
        NetworkResult::Atomic(result),
        NetworkResult::Atomic(diff_output),
    ])
}
```

**Deprecation of `output_diff` flag:** With multi-output, `output_diff` is no longer needed. The user controls visibility per pin. For migration:
- Keep `output_diff` in serialization for backward compatibility on load.
- On load, if `output_diff` is true, add pin 1 to `displayed_output_pins` for that node. If false, only pin 0.
- Remove `output_diff` from the properties panel.
- The `show_anchor_arrows` and `include_base_bonds_in_diff` flags remain — they affect *how* the diff output is computed, independent of which pin is displayed.

### Hit Testing and Interaction with Multiple Displayed Outputs

#### The Problem

Currently, `output_diff` serves two roles:
1. **Display:** which output is rendered in the viewport.
2. **Interaction:** which atom ID space tools use for hit testing. When `output_diff = false`, hit atoms are in result-space (provenance mapping needed to reach diff IDs). When `output_diff = true`, hit atoms are in diff-space (direct IDs).

With multi-output, both pin 0 (result) and pin 1 (diff) can be displayed simultaneously. Many atoms overlap at identical positions. If the raycaster tests all displayed atoms, it would arbitrarily pick from either output, and the tool wouldn't know which ID space the hit atom belongs to.

#### Solution: Interactive Pin

Hit testing is always performed against exactly **one** output pin per node — the **interactive pin**. Other displayed pins are visual-only (rendered but not hittable).

**Rule for determining the interactive pin:**
- If pin 0 (result) is displayed → interactive pin is 0. Tools use provenance mapping. (Same as current `output_diff = false`.)
- If only pin 1 (diff) is displayed → interactive pin is 1. Tools use diff-native IDs. (Same as current `output_diff = true`.)
- If both pin 0 and pin 1 are displayed → interactive pin is 0. Pin 1 is visual-only.

**Generalized:** The interactive pin is the **lowest-indexed displayed output pin**. This is automatic — no separate toggle needed.

**How this replaces `output_diff`:** Tools currently check `output_diff` to decide whether to use provenance mapping or direct diff IDs. With multi-output, they check `interactive_pin_index` instead:
- `interactive_pin_index == 0` → provenance mode (was `output_diff = false`)
- `interactive_pin_index == 1` → diff-native mode (was `output_diff = true`)

The tools' actual logic doesn't change — only the condition they check.

#### Scene-Level Hit Testing

`hit_test_all_atomic_structures_with_node_id()` currently iterates all entries in `node_data` looking for `NodeOutput::Atomic`. When `NodeSceneData` holds multiple outputs per node, this function must only test the interactive pin's output, not all of them.

In practice: `generate_scene()` can tag which output per node is interactive, or the hit test function can look up the interactive pin from the network's display state.

#### Gadget Interaction

The gadget (XYZ translation gizmo) operates on selected atoms, which are tracked in atom_edit's internal selection state (`selected_diff_atoms`, `selected_base_atoms`). The gadget position is derived from these selections, not from the rendered output. So the gadget is unaffected by which pins are displayed — it always works on the diff structure internally.

The gadget's own hit test (`gadget_hit_test`) is independent of atom hit testing and runs first (gadget has priority). No change needed.

#### Decorations

Currently, `decorate: bool` in `eval()` controls whether selection highlights, measurement marks, and tool-specific visual feedback are applied to the output. With multi-output:
- **Pin 0 (result):** Decorations applied as today (selection highlights via provenance mapping, measurement marks, etc.)
- **Pin 1 (diff):** Decorations applied in diff-space (anchor arrows, diff-specific coloring). No selection highlights from tools — it's visual-only when both are displayed.
- **General rule:** Only the interactive pin's output receives tool-driven decorations (selection, hover, measurement). Other pins receive their inherent decorations (anchor arrows, diff coloring) but not interactive feedback.

### Flutter UI Changes

#### 1. NodeView API Extension

```rust
pub struct NodeView {
    // ... existing fields ...
    pub output_pins: Vec<OutputPinView>,        // NEW: replaces output_type
    pub function_type: String,                  // pin -1 type (unchanged)
    pub displayed_output_pins: Vec<i32>,        // NEW: which pins are currently displayed
}

pub struct OutputPinView {
    pub name: String,
    pub data_type: String,
    pub index: i32,  // 0, 1, 2, ...
}
```

The old `output_type: String` field can be kept temporarily for compatibility and derived from `output_pins[0].data_type`.

#### 2. Node Widget Layout

Current layout (title bar):
```
[Node Type Name          ] [👁] [  ] [fn●]
```

**New layout (all nodes, single-output and multi-output alike):**

The eye icon moves from the title bar to beside the output pin, always. This is a uniform layout — no conditional behavior based on output pin count.

Single-output node:
```
┌─────────────────────────────────────┐
│ Node Type Name              [fn●]   │  ← title bar (no eye icon)
├─────────────────────────────────────┤
│                                     │
│  [●] param1              [👁]   [●] │  ← pin 0 with eye icon
│  [●] param2                         │
│                                     │
└─────────────────────────────────────┘
```

Multi-output node (e.g. atom_edit):
```
┌─────────────────────────────────────┐
│ Node Type Name              [fn●]   │  ← title bar (no eye icon)
├─────────────────────────────────────┤
│                                     │
│  [●] param1     [👁] result     [●] │  ← pin 0 with eye + name
│  [●] param2       [👁] diff     [●] │  ← pin 1 with eye + name
│                                     │
└─────────────────────────────────────┘
```

**Key layout decisions:**
- The eye icon is **always** next to the output pin it controls, for all nodes. One pattern to learn.
- The title bar no longer contains an eye icon. It has the node type name and the function pin.
- For **multi-output nodes**, output pin names are displayed next to each pin ("result", "diff"). Users need to distinguish pins at a glance.
- For **single-output nodes**, the pin name is **not** displayed (there's nothing to distinguish from). The eye icon alone is sufficient. The name is still available as a tooltip on hover.
- This is a slight visual change for existing single-output nodes (eye icon moves from title bar to output pin row), but it's a net win: consistent behavior, simpler widget code, and no jarring layout reorganization if a node type later gains additional outputs.

**Alternative considered: eye icon in title bar for single-output, per-pin for multi-output.** Rejected — inconsistent UX. Users learn two patterns. If a node gains an output pin, the eye icon suddenly jumps to a different location. Simpler to always have it in the same place.

**Alternative considered: show names on all nodes including single-output.** Unnecessary — a single output has nothing to distinguish from. The name "result" next to a lone pin adds visual noise without information. Names appear on hover via tooltip.

#### 3. Display Toggle Behavior

- Each output pin's eye icon toggles independently.
- Multiple pins can be displayed simultaneously (not radio-button exclusive).
- **Rationale:** Displaying both the result and the diff simultaneously is the core use case. Radio buttons would defeat the purpose. The viewport renders both as separate structures (which can overlap, but that's useful for visual comparison).
- When the node-level display is toggled off (e.g., by display policy), all pin displays are hidden. When toggled back on, the previously-displayed pins restore.

#### 4. Wire Attachment Points

For multi-output nodes, output pins stack vertically on the right side:
```
                    ● result  ← pin 0 (top)
                    ● diff    ← pin 1
```

Wire endpoint calculation accounts for the output pin index to compute the correct vertical offset. The existing `NODE_VERT_WIRE_OFFSET_PER_PARAM` pattern extends naturally.

**Input/output alignment:** Input pins are on the left, output pins are on the right. They're in separate columns. A node can have more inputs than outputs or vice versa. The node body height is `max(input_pin_count, output_pin_count) * PIN_SPACING`.

#### 5. NodeNetworkPainter

The painter draws wires to the correct output pin vertical position based on `source_output_pin_index`. Currently all result-pin wires go to the same point. With multiple result pins, each pin index maps to a different y-offset.

### Text Format Changes

The text format for wires currently uses:
```
output = atom_edit { base: input }
```

For multi-output, we need syntax to reference a specific output pin:
```
output = atom_edit { base: input }
diff_consumer = some_node { input: atom_edit.diff }
```

The `.pinname` suffix after a node reference selects the output pin. Unqualified references default to pin 0 (backward compatible). This is similar to how many node-based systems (Houdini, Blender) reference specific outputs.

### Undo System Impact

- **`displayed_output_pins`** is new state captured in undo snapshots. The `SerializableNodeNetwork` format needs to include it.
- **`SetNodeDisplay` command** needs extension or a new `SetOutputPinDisplay` command for per-pin toggling.
- **Snapshot comparison** in tests: `normalize_json()` may need to sort `displayed_output_pins`.
- **`output_diff` migration:** The `AtomEditToggleFlagCommand` for `output_diff` is replaced by `SetOutputPinDisplay` operations.

### Implementation Phases

#### Phase 1: Representation Change (Rust only, no behavior change)

The goal is to land the new data structures while all code continues to operate on pin 0 only.

- Replace `NodeType.output_type: DataType` with `output_pins: Vec<OutputPinDefinition>`
- Add `OutputPinDefinition` struct, `OutputPinDefinition::single()` helper
- Migrate all node type registrations to use `output_pins`
- Add `output_type()` accessor on `NodeType` (returns `&output_pins[0].data_type`)
- Migrate all code that reads `node_type.output_type` to use the accessor
- Change `NodeData::eval()` → `EvalOutput`, migrate all nodes to `EvalOutput::single()`
- Update evaluator to unwrap `EvalOutput` → primary result (just `.primary()`)
- Serialization: load old `output_type` format into `output_pins[0]`, save new format
- All tests should pass with zero behavior change

**Tests:**
- All existing tests must pass unchanged (this phase is a pure refactor)
- Add unit tests for `OutputPinDefinition::single()`, `NodeType::output_type()` accessor, `NodeType::get_output_pin_type()` for indices -1, 0, 1+, and `NodeType::has_multi_output()`
- Add unit tests for `EvalOutput::single()`, `EvalOutput::multi()`, `EvalOutput::get()` for valid and out-of-range indices, `EvalOutput::primary()`
- Add a .cnnd roundtrip test: save a network with the new `output_pins` format, reload, verify `output_pins` restored correctly
- Add a .cnnd backward-compat test: load a file with old `output_type` format, verify it migrates to `output_pins[0]` correctly

**Manual testing:** No user-visible changes. Run the app, open existing .cnnd files, verify everything works as before. Save and re-open a file to verify the new serialization format loads correctly.

#### Phase 2: Evaluator Multi-Output Support

- Update `evaluate()` to dispatch by pin index from `EvalOutput`
- Update `generate_scene()`: evaluate all displayed pins, build scene data
- Add `displayed_output_pins` to `NodeNetwork` with backward-compat defaults
- `NodeSceneData` gains multi-output support
- Renderer/display pipeline handles multiple outputs per node
- Implement interactive pin logic for hit testing (lowest-indexed displayed pin)

**Tests:**
- Test `evaluate()` with output_pin_index > 0: create a test node type with two output pins, wire pin 1 to a downstream node, verify the downstream node receives the correct value
- Test `displayed_output_pins` defaults: node in `displayed_node_ids` but not in `displayed_output_pins` → pin 0 displayed
- Test `displayed_output_pins` explicit: set pin 1 displayed, verify `generate_scene()` produces output for pin 1
- Test multi-output scene generation: display both pin 0 and pin 1 of a node, verify `NodeSceneData` contains both outputs
- Test interactive pin determination: both pins displayed → pin 0 is interactive; only pin 1 displayed → pin 1 is interactive
- Serialization roundtrip for `displayed_output_pins`

**Manual testing:** No user-visible changes yet (Flutter UI not updated). All existing behavior preserved. Can verify via Rust tests that multi-output evaluation works.

#### Phase 3: atom_edit Multi-Output

- Register `atom_edit` with two output pins: "result" (pin 0) and "diff" (pin 1)
- Refactor `atom_edit` eval to produce both results
- Deprecate `output_diff` flag with migration logic
- Update `atom_edit` gadgets/tools that check `output_diff` to use interactive pin index
- Update hit testing to only test against the interactive pin's output

**Tests:**
- Test atom_edit eval returns `EvalOutput` with two results: pin 0 is applied result, pin 1 is raw diff
- Test that pin 0 result matches the old `output_diff = false` output
- Test that pin 1 result matches the old `output_diff = true` output
- Test `output_diff` migration on load: old file with `output_diff: true` → pin 1 in `displayed_output_pins`; `output_diff: false` → pin 0 only
- Test interactive pin with atom_edit: both pins displayed → hit test uses pin 0 (provenance mode); only pin 1 displayed → hit test uses pin 1 (diff-native mode)
- Test decorations: only the interactive pin's output gets selection highlights; other pin gets inherent decorations only
- Test `include_base_bonds_in_diff` and `show_anchor_arrows` flags still work on pin 1 output

**Manual testing:** No user-visible changes yet (Flutter UI not updated). The Rust backend now produces multi-output for atom_edit but the Flutter UI still shows pin 0 only (old NodeView API). Existing behavior preserved.

#### Phase 4: Flutter UI

- Extend `NodeView` API with `output_pins` and `displayed_output_pins`
- Update node widget: eye icon next to each output pin, pin names for multi-output nodes
- Update wire rendering for multiple output pin positions
- Update wire dragging/connection for output pin selection
- Per-pin display toggle API functions

**Tests:**
- Test `NodeView` API: verify `output_pins` and `displayed_output_pins` populated correctly from Rust
- Test wire position calculation: wires to pin 0 and pin 1 have different vertical offsets
- Test node body height: `max(input_count, output_count) * PIN_SPACING`

**Manual testing:** This is the first phase with user-visible changes.
- Open the app. All single-output nodes should show the eye icon next to the output pin (moved from title bar).
- Open or create an atom_edit node. It should show two output pins: "result" and "diff", each with an eye icon.
- Toggle eye icons independently. Pin 0 and pin 1 can be displayed/hidden separately.
- Display both pins simultaneously — viewport shows both atomic structures overlapping.
- Display only pin 1 — viewport shows the diff view (same as old `output_diff = true`).
- Wire from pin 1 of atom_edit to another node's input — verify wire attaches to the correct output pin position.
- Drag a wire from a single-output node — verify wire comes from the correct (only) output pin.
- Hover over an output pin — verify tooltip shows pin name and data type.
- Interact with atoms (click, drag, add atom tool, add bond tool) while both pins are displayed — verify hit testing uses pin 0 (result) only. Pin 1 atoms should not be selectable.
- Display only pin 1, interact — verify hit testing works in diff-native mode.

#### Phase 5: Text Format

- Add `.pinname` output pin reference syntax to parser
- Update serializer to emit qualified references when needed

**Tests:**
- Parse test: `some_node { input: atom_edit.diff }` correctly references pin 1 of atom_edit
- Parse test: `some_node { input: atom_edit }` (unqualified) defaults to pin 0
- Parse test: `some_node { input: atom_edit.result }` explicitly references pin 0
- Parse test: `some_node { input: atom_edit.nonexistent }` produces a parse error or validation error
- Serialize test: network with a wire from pin 1 serializes as `node_name.pinname`
- Serialize test: wire from pin 0 serializes without qualifier (backward compatible)
- Roundtrip test: parse → serialize → parse produces identical network

**Manual testing:** Use the text editor to edit a network containing atom_edit with multi-output wires. Verify `.diff` syntax works. Verify unqualified references default to pin 0.

#### Phase 6: Undo Integration

- `SetOutputPinDisplay` command for per-pin display toggling
- Update `SerializableNodeNetwork` snapshot format to include `displayed_output_pins`
- Migrate `output_diff` toggle undo command to `SetOutputPinDisplay`
- Tests

**Tests:**
- Undo/redo `SetOutputPinDisplay`: toggle pin 1 display on → undo → pin 1 hidden → redo → pin 1 visible
- Snapshot roundtrip: `displayed_output_pins` preserved through snapshot → restore cycle
- `normalize_json()` handles `displayed_output_pins` sorting for deterministic comparison
- Full workflow test: create atom_edit node → display pin 1 → add atoms → undo all → verify initial state restored (including pin display state)
- History eviction: verify `displayed_output_pins` state is correctly captured in commands that get evicted

**Manual testing:**
- Toggle pin 1 display on atom_edit. Press Ctrl+Z — pin 1 should hide. Press Ctrl+Shift+Z — pin 1 should re-appear.
- Make atom edits while both pins are displayed. Undo/redo — verify both pin outputs update correctly.
- Test undo across the `output_diff` migration boundary: open an old file with `output_diff: true`, make changes, undo past the migration point.

#### Phase 7: Custom Network Multi-Output

- Update `update_network_output_type()` to propagate full `output_pins` from return node
- Update custom node evaluation to pass through multi-output
- UI for defining extra outputs in custom networks

**Tests:**
- Test custom network with multi-output return node: return node has 2 output pins → custom node type has 2 output pins
- Test `update_network_output_type()` updates full `output_pins` list when return node changes
- Test evaluation: wire from pin 1 of a custom node → evaluates return node's pin 1
- Test custom network with single-output return node: behaves as before (one output pin)
- Test return node change: switch return node from multi-output to single-output → custom node type's extra pins removed, existing wires to removed pins get disconnected

**Manual testing:**
- Create a custom network whose return node is an atom_edit. Use this custom network as a node in another network. Verify it exposes both "result" and "diff" output pins. Wire from pin 1 and verify the diff propagates correctly.

## Open Questions

1. **Should all output pins of a multi-output node share the same eval, or could some be lazy?**
   Current design: single eval, all outputs computed. If the diff computation is expensive and the user only displays pin 0, we'd still compute both. For `atom_edit`, both outputs derive from the same `apply_diff()` call so the marginal cost is low. For future nodes where extra outputs are expensive, we could add a `requested_pins: &HashSet<i32>` parameter to `eval()`. **Recommendation: defer this.** Add it when a real performance need arises.

2. **Display policy interaction.** The display policy resolver (Manual, Selected, Frontier) currently operates at node level — it decides *whether* a node is displayed, not *which pins*. When a policy causes a node to become displayed, pin 0 is shown automatically. Additional pins are only shown via explicit manual toggle by the user. Policies never auto-display pins beyond pin 0.

3. **What happens when you connect to pin 1 but the node only has pin 0?** `get_output_pin_type(1)` returns `DataType::None` for a single-output node, which cannot be connected to any input. The UI should not offer invalid pins during wire dragging.

4. **Node body height with asymmetric input/output counts.** A node with 5 inputs and 2 outputs: the body height is driven by `max(5, 2) = 5` rows. Output pins are top-aligned within that space. This matches how input pins work today (they don't stretch to fill the body if there's only one).
