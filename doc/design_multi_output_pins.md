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

**Eval cache.** We add a per-scene-generation cache (`HashMap<u64, EvalOutput>`) to the evaluator context. Currently the evaluator avoids redundant evaluation through the DAG traversal structure (each node evaluated at most once per downstream path). But with multi-output, the same node may be pulled from different output pins by different downstream consumers. The cache ensures one `eval()` call per node per scene generation. This is also a performance improvement for single-output nodes that fan out to multiple consumers.

The cache must be cleared at the start of each `generate_scene()` call.

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

**Single-output nodes** (no visual change):
```
[Node Type Name          ] [👁] [  ] [fn●]
```

For single-output nodes, the eye icon stays in the title bar — it controls pin 0 display. No change from current behavior.

**Multi-output nodes:**
```
┌─────────────────────────────────────┐
│ Node Type Name              [fn●]   │  ← title bar (no eye icon here)
├─────────────────────────────────────┤
│                                     │
│  [●] param1          [👁] result [●]│  ← pin 0 with eye + label
│  [●] param2            [👁] diff [●]│  ← pin 1 with eye + label
│                                     │
└─────────────────────────────────────┘
```

**Key layout decisions:**
- For multi-output nodes, eye icons move from the title bar to beside each output pin on the right side.
- Output pin **names are displayed** next to each pin (unlike the single-output case where there's nothing to distinguish). Space is tight but with short names like "result" and "diff" it fits. Names are right-aligned, before the pin circle.
- For single-output nodes, nothing changes. The eye icon stays in the title bar.
- On hover over an output pin, a tooltip shows the full pin name and data type.

**Alternative considered: always show eye icons in title bar, one per output.** Separates the icon from the pin it refers to — confusing when there are multiple.

**Alternative considered: show names only on hover.** With multiple pins, users need to see which is which at a glance. Short names are acceptable.

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
- Update evaluator to unwrap `EvalOutput` → primary result (no caching yet, just `.primary()`)
- Serialization: load old `output_type` format into `output_pins[0]`, save new format
- All tests should pass with zero behavior change

#### Phase 2: Evaluator Multi-Output Support

- Add per-evaluation `EvalOutput` cache to evaluator (clear per `generate_scene()`)
- Update `evaluate()` to cache and dispatch by pin index
- Update `generate_scene()`: evaluate all displayed pins, build scene data
- Add `displayed_output_pins` to `NodeNetwork` with backward-compat defaults
- `NodeSceneData` gains multi-output support
- Renderer/display pipeline handles multiple outputs per node

#### Phase 3: atom_edit Multi-Output

- Register `atom_edit` with two output pins: "result" (pin 0) and "diff" (pin 1)
- Refactor `atom_edit` eval to produce both results
- Deprecate `output_diff` flag with migration logic
- Update `atom_edit` gadgets/tools that check `output_diff`

#### Phase 4: Flutter UI

- Extend `NodeView` API with `output_pins` and `displayed_output_pins`
- Update node widget to show per-pin eye icons and labels for multi-output nodes
- Update wire rendering for multiple output pin positions
- Update wire dragging/connection for output pin selection

#### Phase 5: Text Format

- Add `.pinname` output pin reference syntax to parser
- Update serializer to emit qualified references when needed

#### Phase 6: Undo Integration

- `SetOutputPinDisplay` command
- Update snapshot format for `displayed_output_pins`
- Migrate `output_diff` toggle to display-based approach
- Tests

#### Phase 7: Custom Network Multi-Output

- Update `update_network_output_type()` to propagate full `output_pins` from return node
- Update custom node evaluation to pass through multi-output
- UI for defining extra outputs in custom networks

## Open Questions

1. **Should all output pins of a multi-output node share the same eval, or could some be lazy?**
   Current design: single eval, all outputs computed. If the diff computation is expensive and the user only displays pin 0, we'd still compute both. For `atom_edit`, both outputs derive from the same `apply_diff()` call so the marginal cost is low. For future nodes where extra outputs are expensive, we could add a `requested_pins: &HashSet<i32>` parameter to `eval()`. **Recommendation: defer this.** Add it when a real performance need arises.

2. **Display policy interaction.** The display policy resolver (Manual, Selected, Frontier) currently operates on nodes. With per-pin display, should policies control which pins are shown? **Recommendation:** Policies continue to toggle node-level display. Pin-level display is always manual/explicit.

3. **What happens when you connect to pin 1 but the node only has pin 0?** `get_output_pin_type(1)` returns `DataType::None` for a single-output node, which cannot be connected to any input. The UI should not offer invalid pins during wire dragging.

4. **atom_edit tool interaction with simultaneous display.** When both result and diff are displayed, and the user clicks an atom, which output's coordinate space is used for hit testing? Hit testing already uses the primary output (pin 0) in result mode and the diff in diff mode — tools know which space they're working in. Showing both outputs simultaneously doesn't change this; the active tool context determines which output is interactive.

5. **Node body height with asymmetric input/output counts.** A node with 5 inputs and 2 outputs: the body height is driven by `max(5, 2) = 5` rows. Output pins are top-aligned within that space. This matches how input pins work today (they don't stretch to fill the body if there's only one).
