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

#### 3. Evaluator: Two Methods — `evaluate()` and `evaluate_all_outputs()`

The evaluator gains a new method `evaluate_all_outputs()` that returns the full `EvalOutput` (all pins) from a single `eval()` call. The existing `evaluate()` delegates to it and extracts the requested pin:

```rust
// network_evaluator.rs

/// Evaluate a node and return all output pin results.
/// Used by generate_scene() to avoid redundant evaluation when
/// displaying multiple output pins of the same node.
fn evaluate_all_outputs(
    &self,
    network_stack: &[NetworkStackElement<'_>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    decorate: bool,
    context: &mut NetworkEvaluationContext,
) -> EvalOutput {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);

    let eval_output = if registry.built_in_node_types.contains_key(&node.node_type_name) {
        node.data.eval(self, network_stack, node_id, registry, decorate, context)
    } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
        // custom node — evaluate return node, get all outputs
        // (details: validity check, child network stack, etc. — same as current evaluate())
        ...
    } else {
        EvalOutput::single(NetworkResult::Error(format!("Unknown node type: {}", node.node_type_name)))
    };

    // Record error/display string from primary (pin 0) result.
    // This preserves current behavior: node_errors and node_output_strings
    // are keyed by node_id and reflect the primary output.
    let primary = eval_output.primary();
    if let NetworkResult::Error(error_message) = primary {
        context.node_errors.insert(node_id, error_message.clone());
    }
    context.node_output_strings.insert(node_id, primary.to_display_string());

    eval_output
}

/// Evaluate a node and return the result for a specific output pin.
/// Used by wiring/argument resolution (evaluate_arg) and custom network evaluation.
pub fn evaluate(
    &self,
    network_stack: &[NetworkStackElement<'_>],
    node_id: u64,
    output_pin_index: i32,
    registry: &NodeTypeRegistry,
    decorate: bool,
    context: &mut NetworkEvaluationContext,
) -> NetworkResult {
    if output_pin_index == -1 {
        // Function pin logic unchanged
        return NetworkResult::Function(Closure { ... });
    }

    self.evaluate_all_outputs(network_stack, node_id, registry, decorate, context)
        .get(output_pin_index)
}
```

**No new eval caching.** The evaluator does not add a general eval cache. Redundant evaluation of fan-out nodes already exists today and is a separate concern. However, `generate_scene()` uses `evaluate_all_outputs()` to call `eval()` once per displayed node and extract all displayed pins from the single `EvalOutput` — avoiding the guaranteed double-evaluation that would occur if it called `evaluate()` separately for each displayed pin. This is not a cache; it's just holding onto the return value long enough to use all of it.

#### 4. generate_scene(): Display Per Output Pin

Currently `displayed_node_ids: HashMap<u64, NodeDisplayType>` tracks which nodes are visible. We need to know *which output pin* to display.

**Approach:** Replace `displayed_node_ids: HashMap<u64, NodeDisplayType>` with a unified `displayed_nodes: HashMap<u64, NodeDisplayState>` that bundles the display type and the set of displayed output pins into a single entry.

```rust
// node_network.rs

/// Display state for a single node. Bundles node-level visibility (Normal/Ghost)
/// with per-output-pin display control.
#[derive(Clone, Debug)]
pub struct NodeDisplayState {
    pub display_type: NodeDisplayType,   // Normal or Ghost
    pub displayed_pins: HashSet<i32>,    // which output pins are rendered in the viewport
}

impl NodeDisplayState {
    /// Default display state: Normal visibility, pin 0 only.
    pub fn normal() -> Self {
        Self {
            display_type: NodeDisplayType::Normal,
            displayed_pins: HashSet::from([0]),
        }
    }

    pub fn with_type(display_type: NodeDisplayType) -> Self {
        Self {
            display_type,
            displayed_pins: HashSet::from([0]),
        }
    }
}

pub struct NodeNetwork {
    // ... existing ...
    pub displayed_nodes: HashMap<u64, NodeDisplayState>,  // replaces displayed_node_ids
    // ...
}
```

**Why a single map instead of two.** An earlier version of this design kept `displayed_node_ids` unchanged and added a separate `displayed_output_pins: HashMap<u64, HashSet<i32>>` map. This creates an invariant that must be manually maintained at every mutation site: undo commands, display policy resolver, serialization load, node deletion, selection factoring, text format editor, paste/duplicate — all must keep two maps in sync. A node in one map but not the other is either a latent bug or an ambiguous "empty means default" convention. Bundling the state into one struct eliminates this entire class of bugs. If a node is displayed, all its display state is present. If it's not displayed, there's no entry at all.

**Backward-compatible accessors.** The existing methods on `NodeNetwork` are updated internally but keep their signatures, so callers are unaffected:

```rust
impl NodeNetwork {
    pub fn is_node_displayed(&self, node_id: u64) -> bool {
        self.displayed_nodes.contains_key(&node_id)
    }

    pub fn get_node_display_type(&self, node_id: u64) -> Option<NodeDisplayType> {
        self.displayed_nodes.get(&node_id).map(|s| s.display_type)
    }

    pub fn set_node_display(&mut self, node_id: u64, is_displayed: bool) {
        if self.nodes.contains_key(&node_id) {
            if is_displayed {
                // Preserve existing pin state if re-displaying, otherwise default
                self.displayed_nodes
                    .entry(node_id)
                    .or_insert_with(NodeDisplayState::normal);
            } else {
                self.displayed_nodes.remove(&node_id);
            }
        }
    }

    pub fn set_node_display_type(&mut self, node_id: u64, display_type: Option<NodeDisplayType>) {
        if self.nodes.contains_key(&node_id) {
            match display_type {
                Some(dt) => {
                    self.displayed_nodes
                        .entry(node_id)
                        .and_modify(|s| s.display_type = dt)
                        .or_insert_with(|| NodeDisplayState::with_type(dt));
                }
                None => {
                    self.displayed_nodes.remove(&node_id);
                }
            }
        }
    }

    /// Get the set of displayed output pins for a node.
    /// Returns None if the node is not displayed.
    pub fn get_displayed_pins(&self, node_id: u64) -> Option<&HashSet<i32>> {
        self.displayed_nodes.get(&node_id).map(|s| &s.displayed_pins)
    }

    /// Toggle a specific output pin's display state for an already-displayed node.
    /// If the last pin is removed, the node is auto-removed from `displayed_nodes`
    /// (a displayed node with no visible pins is wasteful — it would be evaluated
    /// for nothing by generate_scene()).
    pub fn set_pin_displayed(&mut self, node_id: u64, pin_index: i32, displayed: bool) {
        if let Some(state) = self.displayed_nodes.get_mut(&node_id) {
            if displayed {
                state.displayed_pins.insert(pin_index);
            } else {
                state.displayed_pins.remove(&pin_index);
                if state.displayed_pins.is_empty() {
                    self.displayed_nodes.remove(&node_id);
                }
            }
        }
    }
}
```

**Behavior:**
- A node is in `displayed_nodes` = it is "visible" (same semantics as the old `displayed_node_ids`).
- `displayed_pins` within each entry says *which pins* are rendered in the viewport.
- When a node becomes displayed (by any path: user toggle, display policy, undo/redo), `displayed_pins` defaults to `{0}` (pin 0 only). This is safe — the user explicitly adds more pins. No "empty means default" ambiguity.
- Display policy resolver (Manual, Selected, Frontier) continues to operate at node level. Pin-level display is always explicit/manual. The resolver calls `set_node_display_type()` which preserves existing `displayed_pins` via the `entry().and_modify()` pattern.

**Scene generation:**

`generate_scene()` is called once per displayed node. It calls `evaluate_all_outputs()` once to get the full `EvalOutput`, then extracts each displayed pin's result from it. This avoids redundant evaluation when multiple pins are displayed. It produces `NodeSceneData` containing outputs for all displayed pins of that node.

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

##### How .cnnd serialization currently works

Understanding the current architecture is critical for backward compatibility:

**Built-in node types** (atom_edit, sphere, cuboid, etc.) are **NOT serialized** in .cnnd files. Their `NodeType` definitions (including `output_type`) are reconstructed from the built-in registry at startup. Only the node *instances* are serialized: `node_type_name`, position, arguments, and node-specific `data` (via `node_data_saver`).

**Custom network node types** are serialized as entire `SerializableNodeNetwork` entries. Each network has a `SerializableNodeType` containing `output_type: String` — this is the network's output signature when used as a node in another network. However, this `output_type` is **recomputed on load** by `update_network_output_type()` from the return node's type, so the serialized value serves mainly as a saved snapshot.

**`displayed_node_ids`** is serialized as `Vec<(u64, NodeDisplayType)>` — a list of `[node_id, "Normal"|"Ghost"]` pairs. No per-pin information.

**atom_edit's `output_diff`** flag is serialized inside the node's `data` JSON object via `SerializableAtomEditData`, with `#[serde(default)]` (defaults to `false` on old files that lack it).

##### What changes

**1. `SerializableNodeType` — `output_type` → `output_pins`**

Current:
```rust
pub struct SerializableNodeType {
    pub name: String,
    pub parameters: Vec<SerializableParameter>,
    pub output_type: String,  // e.g. "Geometry", "Atomic"
    // ...
}
```

New:
```rust
pub struct SerializableNodeType {
    pub name: String,
    pub parameters: Vec<SerializableParameter>,

    // New field: always written on save
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_pins: Vec<SerializableOutputPin>,

    // Old field: only read for migration, never written
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<String>,

    // ...
}

#[derive(Serialize, Deserialize)]
pub struct SerializableOutputPin {
    pub name: String,
    pub data_type: String,
}
```

**Loading logic (in `serializable_to_node_type()`):**
```rust
let output_pins = if !serializable.output_pins.is_empty() {
    // New format: use output_pins directly
    serializable.output_pins.iter().map(|p| OutputPinDefinition {
        name: p.name.clone(),
        data_type: DataType::from_string(&p.data_type),
    }).collect()
} else if let Some(ref output_type_str) = serializable.output_type {
    // Old format: migrate single output_type to output_pins[0]
    vec![OutputPinDefinition {
        name: "result".to_string(),
        data_type: DataType::from_string(output_type_str),
    }]
} else {
    // Fallback: no output
    vec![OutputPinDefinition {
        name: "result".to_string(),
        data_type: DataType::None,
    }]
};
```

**Saving logic (in `node_type_to_serializable()`):** Always write `output_pins`. Never write `output_type`. Old atomCAD versions that expect `output_type` will fail to find it, but this is acceptable for a forward-moving format.

**Important:** This only affects **custom network node types**. Built-in node types get their `output_pins` from the registry, not from serialization. So old .cnnd files with built-in nodes (the vast majority) are completely unaffected. The migration only matters for .cnnd files that define custom user networks.

**Additional safety:** Even for custom networks, `update_network_output_type()` recomputes the output type from the return node after loading. So even if the serialized `output_pins` is stale or migrated incorrectly, it gets corrected by the validator. The serialized value is a snapshot, not the source of truth.

**2. `SerializableNodeNetwork` — add `displayed_output_pins` (serialization only)**

The serialization format keeps `displayed_node_ids` and adds a separate `displayed_output_pins` for backward compatibility. These two fields are **merged into the unified `displayed_nodes: HashMap<u64, NodeDisplayState>` on load** and **split back out on save**.

```rust
pub struct SerializableNodeNetwork {
    pub next_node_id: u64,
    pub node_type: SerializableNodeType,
    pub nodes: Vec<SerializableNode>,
    pub return_node_id: Option<u64>,
    pub displayed_node_ids: Vec<(u64, NodeDisplayType)>,  // always written (backward compat)

    // NEW: per-node pin display state. Omitted from JSON if empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub displayed_output_pins: Vec<(u64, Vec<i32>)>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_settings: Option<SerializableCameraSettings>,
}
```

**Loading logic (merge into `displayed_nodes`):**
```rust
// Build displayed_nodes from the two serialized fields
let mut displayed_nodes = HashMap::new();
for (node_id, display_type) in &serializable.displayed_node_ids {
    displayed_nodes.insert(*node_id, NodeDisplayState {
        display_type: *display_type,
        displayed_pins: HashSet::from([0]),  // default: pin 0 only
    });
}
// Overlay explicit pin display state where present
for (node_id, pins) in &serializable.displayed_output_pins {
    if let Some(state) = displayed_nodes.get_mut(node_id) {
        state.displayed_pins = pins.iter().copied().collect();
    }
}
network.displayed_nodes = displayed_nodes;
```

- Old files: `displayed_output_pins` field absent → serde defaults to empty vec → all displayed nodes get `displayed_pins: {0}` (backward compat)
- New files: field present → merged in, overriding the default `{0}`

**Saving logic (split from `displayed_nodes`):**
```rust
let displayed_node_ids: Vec<(u64, NodeDisplayType)> = network
    .displayed_nodes.iter()
    .map(|(&id, state)| (id, state.display_type))
    .collect();

// Only write displayed_output_pins for nodes with non-default pin state
let displayed_output_pins: Vec<(u64, Vec<i32>)> = network
    .displayed_nodes.iter()
    .filter(|(_, state)| state.displayed_pins != HashSet::from([0]))
    .map(|(&id, state)| (id, state.displayed_pins.iter().copied().collect()))
    .collect();
```

This keeps the JSON format identical for single-output-only networks (no `displayed_output_pins` field emitted). Old atomCAD versions can still read `displayed_node_ids` and ignore the unknown `displayed_output_pins` field.

**3. atom_edit `output_diff` migration**

`SerializableAtomEditData` keeps the `output_diff` field for reading:
```rust
pub struct SerializableAtomEditData {
    #[serde(default)]
    pub output_diff: bool,  // kept for migration, eventually stop writing
    // ...
}
```

Migration happens at load time, after the atom_edit node data is deserialized and the node is inserted into the network, and after `displayed_nodes` has been built from the two serialized fields. The loader checks: if a node is an `atom_edit` with `output_diff: true` AND the node's `displayed_pins` has not already been set from `displayed_output_pins`, then set `displayed_pins` to `{1}` (show diff pin only). This logic runs once during deserialization and is idempotent.

On save, `output_diff` is no longer written (or always written as `false`). The display state is captured in the node's `NodeDisplayState.displayed_pins` (which is serialized via `displayed_output_pins`).

##### Summary of backward compatibility guarantees

| Scenario | Behavior |
|----------|----------|
| Old .cnnd with built-in nodes only | Loads perfectly. Built-in types get `output_pins` from registry. No `displayed_output_pins` → all nodes get `displayed_pins: {0}`. |
| Old .cnnd with custom networks | Loads correctly. `output_type` string migrated to `output_pins[0]`. Validator recomputes from return node anyway. |
| Old .cnnd with atom_edit `output_diff: true` | `output_diff` flag read, migrated to `displayed_pins: {1}` in that node's `NodeDisplayState`. |
| Old .cnnd with `displayed_node_ids` only | No `displayed_output_pins` → all entries in `displayed_nodes` get `displayed_pins: {0}`. |
| New .cnnd loaded by old atomCAD | Will fail on custom networks (missing `output_type` field). Built-in-only files may work if `output_type` absence is tolerated by old code. Not a supported scenario. |

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
- **Impact: Minor.** With multi-output, `evaluate_all_outputs()` records error/display string from the **primary (pin 0) result**, preserving current behavior. Both `evaluate()` (single-pin path, used by wiring) and `evaluate_all_outputs()` (all-pins path, used by `generate_scene()`) share this recording logic. For multi-output nodes, errors from non-primary pins are accumulated inside `eval()` itself, not per-pin in the evaluator.
  - **No code change needed in Phase 1.** In later phases, we may want per-pin error/display strings, but this can be addressed when the UI supports it.

#### 6. Undo/Redo Snapshots

- **What:** `NodeSnapshot` and `SerializableNodeNetwork` store serialized node data (JSON) for undo/redo.
- **Impact: None on snapshot mechanism.** Snapshots store serialized `NodeData`, not eval output. The `output_pins` change in `NodeType` will be reflected in `SerializableNodeNetwork`'s `node_type` field when serialized, but this is handled by the serialization migration (section 7 above), not by undo-specific code. The `displayed_output_pins` field in `SerializableNodeNetwork` captures the per-pin display state from `displayed_nodes` — this is part of the network snapshot automatically.

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
- On load, if `output_diff` is true, set `displayed_pins` to `{1}` in that node's `NodeDisplayState`. If false, default `{0}` applies.
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
    pub displayed_pins: Vec<i32>,               // NEW: which pins are currently displayed
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

- **`displayed_pins`** is part of `NodeDisplayState` in the unified `displayed_nodes` map, so it is automatically captured in `SerializableNodeNetwork` snapshots (serialized via `displayed_output_pins`). No separate snapshot mechanism needed.
- **`SetNodeDisplay` command** needs extension or a new `SetOutputPinDisplay` command for per-pin toggling. `SetNodeDisplayCommand` currently stores `old_display_type: Option<NodeDisplayType>` / `new_display_type` — extend to store `old_display_state: Option<NodeDisplayState>` / `new_display_state` so pin display state is captured atomically with the display type.
- **Snapshot comparison** in tests: `normalize_json()` may need to sort `displayed_output_pins` arrays.
- **`output_diff` migration:** The `AtomEditToggleFlagCommand` for `output_diff` is replaced by `SetOutputPinDisplay` operations.

### Implementation Phases

#### Phase 0: Create Backward-Compatibility Fixtures (before any code changes)

Create frozen .cnnd fixture files and their tests while the code is still unchanged. This guarantees fixtures use the real old serialization format.

- Create `rust/tests/fixtures/multi_output_migration/` directory
- Create fixture files (either by saving from the app or hand-crafting minimal JSON):
  1. `old_builtin_only.cnnd` — network with built-in nodes, old `output_type` field
  2. `old_custom_network.cnnd` — custom network node type with old `output_type: "Geometry"`
  3. `old_atom_edit_output_diff_true.cnnd` — atom_edit with `output_diff: true`
  4. `old_atom_edit_output_diff_false.cnnd` — atom_edit with `output_diff: false`
- Write tests that load each fixture and verify current behavior (all pass against unchanged code)
- The atom_edit `output_diff` migration tests are `#[ignore]` until Phase 3

**Tests:** All fixture load tests pass against the current (pre-change) code. This establishes the baseline.

**Manual testing:** N/A.

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
- **Backward-compat fixture tests** (see below)

**Backward-compatibility fixture tests:**

Create frozen .cnnd fixture files **before implementation** that capture the old serialization format. These files are never modified — they permanently represent the old format. After Phase 1, the same tests verify migration works.

Fixture files in `rust/tests/fixtures/multi_output_migration/`:

1. **`old_builtin_only.cnnd`** — A simple network with built-in nodes (e.g. sphere → union). Uses old `output_type` field on the network's `node_type`. Has `displayed_node_ids` but no `displayed_output_pins`.
   - **Test:** Load → verify all nodes resolve their types from the registry with correct `output_pins`. Verify `displayed_nodes` entries have `displayed_pins: {0}` (default). Verify evaluation produces same results as before.

2. **`old_custom_network.cnnd`** — Two networks: one defines a custom node type (with old `"output_type": "Geometry"` on its `node_type`), the other uses it as a node.
   - **Test:** Load → verify custom network's `output_pins[0]` is migrated from `output_type` string. Verify `output_type()` accessor returns the correct `DataType`. Verify the custom node instance in the second network evaluates correctly.

3. **`old_atom_edit_output_diff_true.cnnd`** — A network with an atom_edit node whose data has `"output_diff": true`.
   - **Test (Phase 3):** Load → verify `output_diff` flag is migrated to `displayed_pins: {1}` in that node's `NodeDisplayState`. (This test is written in Phase 1 but the migration logic is implemented in Phase 3, so it starts as a `#[ignore]` test and is un-ignored in Phase 3.)

4. **`old_atom_edit_output_diff_false.cnnd`** — Same but with `"output_diff": false` (or absent).
   - **Test (Phase 3):** Load → verify node's `displayed_pins` is `{0}` (default).

**How to create the fixtures:** Run the current (pre-change) code to save representative networks, or hand-craft minimal JSON. The key is that they use the current serialization format and are frozen before any code changes.

**Manual testing:** No user-visible changes. Run the app, open existing .cnnd files, verify everything works as before. Save and re-open a file to verify the new serialization format loads correctly.

#### Phase 2: Evaluator Multi-Output Support

- Update `evaluate()` to dispatch by pin index from `EvalOutput`
- Add `evaluate_all_outputs()` method, have `generate_scene()` use it
- Replace `displayed_node_ids: HashMap<u64, NodeDisplayType>` with `displayed_nodes: HashMap<u64, NodeDisplayState>` on `NodeNetwork`
- Update all code that reads/writes `displayed_node_ids` to use the new accessors
- Update serialization to split/merge `displayed_nodes` ↔ `displayed_node_ids` + `displayed_output_pins`
- `NodeSceneData` gains multi-output support
- Renderer/display pipeline handles multiple outputs per node
- Implement interactive pin logic for hit testing (lowest-indexed displayed pin)

**Tests:**
- Test `evaluate()` with output_pin_index > 0: create a test node type with two output pins, wire pin 1 to a downstream node, verify the downstream node receives the correct value
- Test `NodeDisplayState` defaults: new displayed node gets `displayed_pins: {0}`
- Test `displayed_pins` explicit: set pin 1 displayed, verify `generate_scene()` produces output for pin 1
- Test multi-output scene generation: display both pin 0 and pin 1 of a node, verify `NodeSceneData` contains both outputs
- Test interactive pin determination: both pins displayed → pin 0 is interactive; only pin 1 displayed → pin 1 is interactive
- Serialization roundtrip: `displayed_nodes` with non-default `displayed_pins` survives save/load via the split `displayed_node_ids` + `displayed_output_pins` format

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
- Test `output_diff` migration on load: old file with `output_diff: true` → `displayed_pins: {1}`; `output_diff: false` → `displayed_pins: {0}`
- Test interactive pin with atom_edit: both pins displayed → hit test uses pin 0 (provenance mode); only pin 1 displayed → hit test uses pin 1 (diff-native mode)
- Test decorations: only the interactive pin's output gets selection highlights; other pin gets inherent decorations only
- Test `include_base_bonds_in_diff` and `show_anchor_arrows` flags still work on pin 1 output

**Manual testing:** No user-visible changes yet (Flutter UI not updated). The Rust backend now produces multi-output for atom_edit but the Flutter UI still shows pin 0 only (old NodeView API). Existing behavior preserved.

#### Phase 4: Flutter UI + Undo for Pin Display

- Extend `NodeView` API with `output_pins` and `displayed_pins`
- Update node widget: eye icon next to each output pin, pin names for multi-output nodes
- Update wire rendering for multiple output pin positions
- Update wire dragging/connection for output pin selection
- Per-pin display toggle API functions
- `SetOutputPinDisplay` undo command for per-pin display toggling (stores `old_display_state` / `new_display_state` as `Option<NodeDisplayState>`)
- Migrate `output_diff` toggle undo command to `SetOutputPinDisplay`

**Why undo is included here:** Per-pin display toggling is the first user-visible change. Shipping it without undo would be a regression — the current `output_diff` toggle already has undo support. The undo command itself is straightforward (same pattern as `SetNodeDisplayCommand`), and the `SerializableNodeNetwork` snapshot format already includes `displayed_output_pins` from Phase 2, so snapshots automatically capture per-pin state.

**Tests:**
- Test `NodeView` API: verify `output_pins` and `displayed_pins` populated correctly from Rust
- Test wire position calculation: wires to pin 0 and pin 1 have different vertical offsets
- Test node body height: `max(input_count, output_count) * PIN_SPACING`
- Undo/redo `SetOutputPinDisplay`: toggle pin 1 display on → undo → pin 1 hidden → redo → pin 1 visible
- Snapshot roundtrip: `displayed_nodes` with non-default `displayed_pins` preserved through snapshot → restore cycle
- `normalize_json()` handles `displayed_output_pins` sorting for deterministic comparison
- Full workflow test: create atom_edit node → display pin 1 → add atoms → undo all → verify initial state restored (including pin display state)
- History eviction: verify pin display state is correctly captured in commands that get evicted

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
- Toggle pin 1 display on atom_edit. Press Ctrl+Z — pin 1 should hide. Press Ctrl+Shift+Z — pin 1 should re-appear.
- Make atom edits while both pins are displayed. Undo/redo — verify both pin outputs update correctly.
- Test undo across the `output_diff` migration boundary: open an old file with `output_diff: true`, make changes, undo past the migration point.

#### Phase 5: Text Format (DONE)

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

#### Phase 6: Custom Network Multi-Output

- Update `update_network_output_type()` to propagate full `output_pins` from return node
- Update custom node evaluation to pass through multi-output

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
