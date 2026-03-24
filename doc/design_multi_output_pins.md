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
2. **Backward compatible.** Nodes with a single output pin work exactly as before with zero changes. The multi-output mechanism is opt-in.
3. **Pin 0 remains the primary output.** This preserves the return node semantics, custom network output type, and all existing wiring.
4. **Display is per output pin, not per node.** Each output pin of a displayed node can independently be shown/hidden in the viewport.

### Rust Backend Changes

#### 1. NodeType: Multiple Output Types

```rust
// node_type.rs
pub struct NodeType {
    // ... existing fields ...
    pub output_type: DataType,                       // pin 0 (unchanged)
    pub extra_output_pins: Vec<OutputPinDefinition>,  // pins 1, 2, ... (NEW)
    // ...
}

/// Definition of an additional output pin (index 1+).
#[derive(Clone, Debug)]
pub struct OutputPinDefinition {
    pub name: String,        // e.g. "diff"
    pub data_type: DataType, // e.g. DataType::Atomic
}
```

**Why `extra_output_pins` instead of replacing `output_type`?** Backward compatibility. The vast majority of nodes have a single output. Replacing `output_type` with a `Vec` would require changing every node registration and every place that reads `output_type`. With `extra_output_pins`, single-output nodes don't change at all.

**`get_output_pin_type()` update:**
```rust
pub fn get_output_pin_type(&self, output_pin_index: i32) -> DataType {
    if output_pin_index == -1 {
        self.get_function_type()
    } else if output_pin_index == 0 {
        self.output_type.clone()
    } else {
        let extra_index = (output_pin_index - 1) as usize;
        self.extra_output_pins
            .get(extra_index)
            .map(|p| p.data_type.clone())
            .unwrap_or(DataType::None)
    }
}
```

#### 2. NodeData::eval() Returns Multiple Results

```rust
// node_data.rs — new return type
pub struct EvalOutput {
    pub primary: NetworkResult,                  // pin 0 result
    pub extra: Vec<NetworkResult>,               // pin 1, 2, ... results
}

impl EvalOutput {
    /// Convenience for single-output nodes.
    pub fn single(result: NetworkResult) -> Self {
        EvalOutput { primary: result, extra: vec![] }
    }

    /// Get result for a given output pin index (0-based for result pins).
    pub fn get(&self, output_pin_index: i32) -> NetworkResult {
        if output_pin_index == 0 {
            self.primary.clone()
        } else {
            let idx = (output_pin_index - 1) as usize;
            self.extra.get(idx).cloned().unwrap_or(NetworkResult::None)
        }
    }
}
```

**Migration strategy:** Change `NodeData::eval()` signature to return `EvalOutput`. All existing node implementations change their `return result;` to `return EvalOutput::single(result);` — a mechanical, one-line change per node.

**Alternative considered: keeping `NetworkResult` and adding `NetworkResult::Multi(Vec<NetworkResult>)`.** Rejected because it would require every consumer of `eval()` to pattern-match on `Multi` vs. a direct value, and it conflates the evaluation result structure with the value type system. `EvalOutput` is a clean wrapper that separates "how many pins" from "what type of value."

#### 3. Evaluator: Cache Full EvalOutput, Dispatch Per Pin

The evaluator currently calls `eval()` and returns the result for the requested pin. With multi-output:

```rust
// network_evaluator.rs — evaluate()
pub fn evaluate(..., output_pin_index: i32, ...) -> NetworkResult {
    if output_pin_index == -1 {
        // Function pin logic unchanged
        return NetworkResult::Function(Closure { ... });
    }

    // Check cache for full EvalOutput
    let eval_output = if let Some(cached) = self.eval_cache.get(&node_id) {
        cached.clone()
    } else {
        // Evaluate node once, cache full output
        let output = node.data.eval(self, network_stack, node_id, registry, decorate, context);
        self.eval_cache.insert(node_id, output.clone());
        output
    };

    eval_output.get(output_pin_index)
}
```

**Critical detail:** The evaluator already has evaluation caching infrastructure. The change ensures a node with multiple outputs is only evaluated once even when multiple downstream nodes pull from different pins.

**Note on `eval_cache`:** Currently the evaluator doesn't cache eval results between `evaluate()` calls within a single `generate_scene()` invocation (each displayed node triggers fresh recursive evaluation). We should add a per-scene-generation cache (`HashMap<u64, EvalOutput>`) to the evaluator. This is a performance improvement even for single-output nodes and becomes essential for multi-output to avoid redundant evaluations. **Update:** Looking more closely at the code, the evaluator does already avoid redundant evaluation through the DAG traversal structure (each node evaluated at most once per downstream path). But with multi-output, the same node may be pulled from different output pins by different downstream consumers. The cache ensures one eval per node per scene generation.

#### 4. generate_scene(): Display Per Output Pin

Currently `displayed_node_ids: HashMap<u64, NodeDisplayType>` tracks which nodes are visible. We need to know *which output pin* to display.

**New structure:**

```rust
/// Identifies a specific output of a specific node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayedOutput {
    pub node_id: u64,
    pub output_pin_index: i32,  // 0, 1, 2, ... (not -1; function pins are never displayed)
    pub display_type: NodeDisplayType,
}
```

**Option A: Replace `displayed_node_ids` with `displayed_outputs: HashMap<(u64, i32), NodeDisplayType>`**

This is the clean approach. A node with two outputs showing would have two entries. The scene generation loop iterates displayed outputs, evaluates each (with caching), and produces a `NodeSceneData` per displayed output.

**Option B: Keep `displayed_node_ids` as-is and add `displayed_output_pins: HashMap<u64, HashSet<i32>>`**

More backward compatible for serialization. `displayed_node_ids` retains its role (the node is "displayed"), and `displayed_output_pins` says which pins. If a node is in `displayed_node_ids` but not in `displayed_output_pins`, it defaults to pin 0 only (backward compatibility with old files).

**Recommendation: Option B.** This preserves backward compatibility with existing `.cnnd` files and minimizes changes to the display policy resolver and undo system. Single-output nodes work exactly as before.

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
- When `displayed_output_pins` has no entry for a node, pin 0 is displayed (backward compat).
- When toggling display for a multi-output node, the initial toggle shows pin 0. Per-pin toggles are separate.
- `generate_scene()` generates a `NodeSceneData` per (node_id, pin_index) pair, keyed differently in the scene.

**Scene data key change:**

```rust
// structure_designer_scene.rs
pub struct StructureDesignerScene {
    // Change from HashMap<u64, NodeSceneData> to:
    pub node_data: HashMap<(u64, i32), NodeSceneData>,  // (node_id, output_pin_index) → scene data
    // ... rest unchanged ...
}
```

**However**, this key change has widespread impact (renderer, Flutter, etc.). A more pragmatic initial approach:

For the first version, keep `node_data: HashMap<u64, NodeSceneData>` and have `NodeSceneData` contain results for all displayed pins of that node:

```rust
pub struct NodeSceneData {
    pub outputs: Vec<(i32, NodeOutput)>,  // (pin_index, output) for each displayed pin
    pub geo_tree: Option<GeoNode>,        // from primary output
    pub node_errors: HashMap<u64, String>,
    pub node_output_strings: HashMap<u64, String>,
    pub unit_cell: Option<UnitCellStruct>,
    pub selected_node_eval_cache: Option<Box<dyn Any>>,
}
```

Or even simpler for v1: keep `output: NodeOutput` as pin 0's output, add `extra_outputs: Vec<(i32, NodeOutput)>` for additional displayed pins. The renderer handles all of them.

#### 5. Custom Network Nodes (Return Node)

When a custom network is used as a node in another network, the evaluator currently calls:
```rust
evaluate(child_network_stack, child_network.return_node_id.unwrap(), 0, ...)
```

This evaluates the return node's pin 0 and uses it as the custom node's pin 0 output.

**For multi-output custom networks:** The return node's multi-outputs become the custom node's multi-outputs. The evaluator dispatches:
```rust
evaluate(child_network_stack, child_network.return_node_id.unwrap(), output_pin_index, ...)
```

The child network's return node must have matching extra output pins. The parent network's `NodeType.extra_output_pins` should mirror the return node's extra output pins. `update_network_output_type()` in the validator already handles updating `output_type` from the return node — extend it to also update `extra_output_pins`.

#### 6. Wire Connection Compatibility

**No changes needed to `Wire` or `Argument`.** The wire's `source_output_pin_index: i32` already supports positive indices. `connect_nodes()` works as-is. Type validation at connection time just needs to call `get_output_pin_type(pin_index)` which already dispatches correctly with the NodeType change.

#### 7. Serialization (.cnnd)

`NodeType` serialization needs to include `extra_output_pins`. For backward compatibility:
- If `extra_output_pins` is empty, omit it from JSON (old format).
- On load, if the field is missing, default to empty vec.

`displayed_output_pins` similarly: omit if empty, default to empty on load.

### atom_edit Node Changes

The `atom_edit` node currently has:
- `output_type: DataType::Atomic` (single output)
- `output_diff: bool` flag to switch between result and diff views

With multi-output:

**Registration:**
```rust
NodeType {
    name: "atom_edit".to_string(),
    output_type: DataType::Atomic,  // pin 0: applied result
    extra_output_pins: vec![
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

    EvalOutput {
        primary: NetworkResult::Atomic(result),
        extra: vec![NetworkResult::Atomic(diff_output)],
    }
}
```

**Deprecation of `output_diff` flag:** With multi-output, `output_diff` is no longer needed. The user controls visibility per pin. For migration:
- Keep `output_diff` in serialization for backward compatibility.
- On load, if `output_diff` is true, add pin 1 to `displayed_output_pins`. If false, only pin 0.
- Remove `output_diff` from the properties panel.

### Flutter UI Changes

#### 1. NodeView API Extension

```rust
pub struct NodeView {
    // ... existing fields ...
    pub output_type: String,       // pin 0 type (unchanged)
    pub function_type: String,     // pin -1 type (unchanged)
    pub extra_output_pins: Vec<OutputPinView>,  // NEW
    pub displayed_output_pins: Vec<i32>,        // NEW: which pins are displayed
}

pub struct OutputPinView {
    pub name: String,
    pub data_type: String,
    pub index: i32,  // 1, 2, ...
}
```

#### 2. Node Widget Layout

Current layout (title bar):
```
[Node Type Name          ] [👁] [  ] [fn●]
```

Proposed layout for multi-output nodes:
```
[Node Type Name                ] [fn●]
                            [👁] [●]    ← pin 0 (primary result)
  [param1 ●]                [👁] [●]    ← pin 1 ("diff")
  [param2 ●]
```

**Key changes:**
- The eye icon moves from the title bar to beside each output pin.
- Each output pin gets its own row on the right side of the node body.
- Output pin names are shown as tooltips on hover (not displayed permanently, to save space).
- For single-output nodes, the eye icon stays in the title bar (no visual change).

**Alternative considered: eye icon in title bar with a dropdown.** Rejected — too many clicks, doesn't show state at a glance.

**Alternative considered: eye icons only in title bar, one per output.** Could work for 2 outputs but doesn't scale. Also separates the eye icon from the pin it refers to, which is confusing.

#### 3. Display Toggle Behavior

- Each output pin's eye icon toggles independently.
- Multiple pins can be displayed simultaneously (not radio-button exclusive).
- **Rationale:** Displaying both the result and the diff simultaneously is the core use case. Radio buttons would defeat the purpose. The viewport renders both as separate atomic structures (which can overlap, but that's useful for visual comparison).
- When the node-level display is toggled off (e.g., by deselection or display policy), all pin displays are hidden. When toggled back on, the previously-displayed pins restore.

#### 4. Wire Attachment Points

For multi-output nodes, output pins stack vertically on the right side:
```
                    ●  ← pin 0 (top)
                    ●  ← pin 1
                    ●  ← pin 2 (if any)
```

Wire endpoint calculation needs to account for the output pin index to compute the correct vertical offset. The existing `NODE_VERT_WIRE_OFFSET_PER_PARAM` pattern extends naturally — we add a similar offset per extra output pin.

#### 5. NodeNetworkPainter

The painter needs to:
- Draw wires to the correct output pin vertical position based on `source_output_pin_index`.
- Currently all result-pin wires go to the same point. With multiple result pins, each pin index maps to a different y-offset.

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

- **`displayed_output_pins`** is new state that must be captured in undo snapshots. The `SerializableNodeNetwork` format needs to include it.
- **`SetNodeDisplay` command** needs extension or a new `SetOutputPinDisplay` command.
- **Snapshot comparison** in tests: `normalize_json()` may need to sort `displayed_output_pins`.
- **`output_diff` migration:** If we remove `output_diff`, the `AtomEditToggleFlagCommand` for it should migrate to `SetOutputPinDisplay` operations.

### Implementation Phases

#### Phase 1: Core Multi-Output Infrastructure (Rust)
- Add `OutputPinDefinition` and `extra_output_pins` to `NodeType`
- Change `NodeData::eval()` → `EvalOutput` (mechanical migration of all nodes to `EvalOutput::single()`)
- Update `get_output_pin_type()` for positive indices
- Add eval caching in evaluator
- Update `evaluate()` to dispatch from cached `EvalOutput`
- Add `displayed_output_pins` to `NodeNetwork`
- Update `generate_scene()` to handle multi-output display
- Serialization backward compatibility

#### Phase 2: atom_edit Multi-Output
- Register `atom_edit` with extra "diff" output pin
- Refactor `atom_edit` eval to produce both result and diff
- Deprecate `output_diff` flag with migration logic
- Update `atom_edit` gadgets/tools that check `output_diff`

#### Phase 3: Flutter UI
- Extend `NodeView` API with extra output pins and display state
- Update node widget to show per-pin eye icons for multi-output nodes
- Update wire rendering for multiple output pin positions
- Update wire dragging/connection for output pin selection

#### Phase 4: Text Format
- Add `.pinname` output pin reference syntax to parser
- Update serializer to emit qualified references when needed

#### Phase 5: Undo Integration
- `SetOutputPinDisplay` command
- Update snapshot format
- Migrate `output_diff` toggle to display-based approach
- Tests

#### Phase 6: Custom Network Multi-Output
- Update `update_network_output_type()` to propagate extra output pins from return node
- Update custom node evaluation to pass through multi-output
- UI for setting extra outputs on parameter/return nodes in custom networks

## Open Questions

1. **Should all output pins of a multi-output node share the same eval, or could some be lazy?**
   Current design: single eval, all outputs computed. If the diff computation is expensive and the user only displays pin 0, we'd still compute both. For `atom_edit`, both outputs derive from the same `apply_diff()` call so the marginal cost is low. For future nodes where extra outputs are expensive, we could add a `requested_pins: HashSet<i32>` parameter to `eval()`.

2. **Display policy interaction.** The display policy resolver (Manual, Selected, Frontier) currently operates on nodes. With per-pin display, should policies control which pins are shown? Proposal: policies still toggle node-level display. Pin-level display is always manual/explicit.

3. **What happens when you connect to pin 1 but the node only has pin 0?** Currently returns `DataType::None` which prevents connection in the UI. The type checker at wire-creation time should validate. This already works: `get_output_pin_type(1)` returns `DataType::None` for a single-output node, and `DataType::None` cannot be connected to any input.

4. **atom_edit tool interaction with simultaneous display.** When both result and diff are displayed, and the user clicks an atom, which output's coordinate space is used for hit testing? Currently hit testing operates on the active/selected node's output. Proposal: hit testing uses the primary output (pin 0) when in result mode, or the diff output (pin 1) when the user's active tool context indicates diff editing. This is already the behavior — tools know whether they're working with diff-space or result-space coordinates.
