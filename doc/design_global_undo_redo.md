# Global Undo/Redo Design

## Overview

This document designs a global undo/redo system for atomCAD using the command pattern. Commands are recorded at the `StructureDesigner` level, enabling undo/redo of all node network operations: adding/deleting nodes, connecting/disconnecting wires, changing node properties, copy/paste, and network-level operations.

**Out of scope:** The `atom_edit` node has large, complex state (atomic structure diffs) that requires specialized undo commands. Its undo/redo design will be covered in a separate document. Until then, `atom_edit` mutations are recorded as opaque `SetNodeData` snapshots (functional but memory-heavy for large structures).

**No-op:** The `edit_atom` node is deprecated and will be removed from the codebase. Global undo/redo will be a no-op for `edit_atom` mutations — its `SetNodeData` commands are simply not recorded.

## Design Principles

1. **Explicit do/undo** — Each command stores enough state to reverse itself. No replay-from-scratch at the global level (too expensive with many nodes).
2. **Generic node data handling** — Node property changes use the existing `node_data_saver`/`node_data_loader` serialization to capture before/after snapshots as `serde_json::Value`. Zero per-node-type code.
3. **Compound commands** — Multi-step operations (delete selection, cut, paste, text edit, factor) are grouped into a single undo step.
4. **Commands created in StructureDesigner** — Not in the API layer (see rationale below).
5. **Selection changes are not undoable** — Consistent with most CAD applications and significantly simpler.
6. **Move coalescing** — Drag operations produce many intermediate positions; these are coalesced into a single undo step.

## Where Commands Are Created: StructureDesigner

### Rationale

Commands should be created inside `StructureDesigner` methods, not in the API layer. Reasons:

1. **StructureDesigner owns the mutation logic.** The API layer is a thin wrapper that calls `with_mut_cad_instance`, delegates to a StructureDesigner method, then calls `refresh_structure_designer_auto`. The actual validation, state changes, dirty marking, and display policy happen in StructureDesigner.

2. **StructureDesigner has access to the before-state.** To create undo commands, we need to snapshot state *before* the mutation. StructureDesigner methods are the natural place for this — they already access the node network to perform the mutation.

3. **Avoids duplication.** Multiple API functions call the same StructureDesigner method (e.g., `cut_selection` calls `copy_selection` + `delete_selected`). If commands were created at the API level, compound operations would require coordination between API functions.

4. **Internal callers get undo for free.** Text format editing, factoring, and CLI operations all go through StructureDesigner methods. Commands are recorded regardless of entry point.

### Pattern

Each mutating StructureDesigner method follows this pattern:

```rust
pub fn some_mutation(&mut self, args...) {
    // 1. Capture before-state (for undo)
    let before = /* snapshot relevant state */;

    // 2. Perform the mutation (existing code, unchanged)
    /* ... existing mutation logic ... */

    // 3. Create and push the command
    self.push_command(SomeMutationCommand { before, after, ... });

    // 4. Existing post-mutation bookkeeping (unchanged)
    self.set_dirty(true);
    self.mark_node_data_changed(node_id);
}
```

## User Action to Command Mapping

A critical question: does every user action map 1:1 to a single StructureDesigner mutation method call? If not, we'd need command grouping/transactions. Here is the full analysis:

### Actions That Map 1:1 (No Grouping Needed)

| User Action | SD Method | Command |
|---|---|---|
| Add node | `add_node()` | `AddNode` |
| Delete selection (nodes) | `delete_selected()` | `DeleteNodes` |
| Delete selection (wires) | `delete_selected()` | `DeleteWires` |
| Connect wire (drag) | `connect_nodes()` | `ConnectWire` |
| Edit node property | `set_node_network_data()` | `SetNodeData` |
| Set return node | `set_return_node_id()` | `SetReturnNode` |
| Toggle node display | `set_node_display()` | `SetNodeDisplay` |
| Paste | `paste_at_position()` | `PasteNodes` |
| Duplicate node | `duplicate_node()` | `DuplicateNode` |
| Rename network | `rename_node_network()` | `RenameNetwork` |
| Delete network | `delete_node_network()` | `DeleteNetwork` |
| Factor selection | `factor_selection_into_subnetwork()` | `FactorSelection` |

### Actions That Involve Multiple SD Calls But Are Still One Command

| User Action | SD Calls | Mechanism |
|---|---|---|
| Cut (Ctrl+X) | `copy_selection()` + `delete_selected()` | See below |
| Auto-connect | `auto_connect_to_node()` → `connect_nodes()` | See below |
| Move nodes (drag) | `move_selected_nodes()` × many | See below |

**Cut:** `cut_selection()` calls `copy_selection()` then `delete_selected()`. `copy_selection()` only writes to the clipboard field on StructureDesigner — it does not mutate any node network, so it never pushes a command. `delete_selected()` pushes a `DeleteNodesCommand`. Result: exactly 1 command. No special mechanism needed.

**Auto-connect:** `auto_connect_to_node()` computes compatible pins, then calls `self.connect_nodes()`. `auto_connect_to_node()` itself does NOT push a command — only `connect_nodes()` does, producing exactly 1 `ConnectWireCommand`. This works because `auto_connect_to_node()` is a pure wrapper: it computes which pins to connect, then delegates to the single command-producing method.

**Move nodes (drag):** `move_selected_nodes()` is called many times per drag but it NEVER pushes commands on its own. Instead, the begin/end grouping mechanism (see MoveNodes section) handles this: `begin_move_nodes()` captures start positions, intermediate `move_selected_nodes()` calls just move the nodes without recording anything, and `end_move_nodes()` creates a single `MoveNodesCommand` from start to final positions.

### Undo Recording Suppression

The cases above work naturally today. However, some methods like `connect_nodes()` serve dual roles: they are both direct user actions (user draws a wire) and internal building blocks (called by `auto_connect_to_node()`). If a future method calls `connect_nodes()` multiple times as part of a single user action, each call would push a separate command.

To handle this, the `UndoStack` provides a suppression flag:

```rust
impl UndoStack {
    /// When true, `push()` calls are silently ignored.
    recording_suppressed: bool,

    pub fn suppress_recording(&mut self) { self.recording_suppressed = true; }
    pub fn resume_recording(&mut self) { self.recording_suppressed = false; }

    pub fn push(&mut self, command: Box<dyn UndoCommand>) {
        if self.recording_suppressed { return; }
        // ... normal push logic ...
    }
}
```

This is NOT needed for any current use case. It is a safety valve for future operations that compose multiple command-producing methods into a single user action. Such operations would suppress recording, perform the sub-mutations, then resume recording and push their own single command.

### Actions That Need Special Handling

**`add_node_network_with_name()`** — The API function calls two SD methods: `add_node_network()` and `set_active_node_network_name()`. However, `set_active_node_network_name()` is a **navigation action** (like selecting a node), not a data mutation. It must NOT push its own command. Instead, the `AddNetwork` command stores the previous active network name and restores it on undo:

```rust
struct AddNetworkCommand {
    network_name: String,
    previous_active_network: Option<String>,  // restored on undo
}
```

On undo: delete the network AND switch back to `previous_active_network`.
On redo: re-create the network AND switch to it.

**`apply_text_to_active_network()`** — Currently lives in the API layer with complex logic (remove network from registry, parse, edit, validate, re-insert). This should be refactored so the command creation happens inside it. It's a self-contained operation that pushes a single `TextEditNetwork` command (snapshot-based). No grouping needed.

**`ai_edit_network()`** — Same pattern as text edit. Single snapshot-based command.

### Methods That Must NOT Push Commands

These methods are either non-mutating, bookkeeping, or navigation:

| Method | Reason |
|---|---|
| `copy_selection()` | Only writes to clipboard, doesn't mutate network |
| `set_active_node_network_name()` | Navigation, not data mutation |
| `navigate_back()` / `navigate_forward()` | Navigation history traversal |
| `set_dirty()` | Internal bookkeeping |
| `mark_node_data_changed()` | Internal bookkeeping |
| `mark_full_refresh()` | Internal bookkeeping |
| `mark_lightweight_refresh()` | Internal bookkeeping |
| `apply_node_display_policy()` | Internal bookkeeping |
| `validate_active_network()` | Internal bookkeeping |
| `new_project()` | Resets everything; undo stack is cleared |

### Conclusion

No command grouping/transaction mechanism is needed. Every user action naturally maps to exactly one command being pushed, because:
- Compound methods like `cut_selection()` contain at most one mutating sub-call
- Wrapper methods like `auto_connect_to_node()` delegate to a single command-producing method
- Navigation/bookkeeping methods don't push commands
- Complex operations (text edit, factor) are self-contained and push their own specialized command

## Command Architecture

### UndoStack

```rust
/// Lives inside StructureDesigner
pub struct UndoStack {
    /// Command history. Index 0 is the oldest command.
    history: Vec<Box<dyn UndoCommand>>,
    /// Points to the next available slot. Commands at indices [0..cursor) have been executed.
    /// Undo decrements cursor, redo increments it.
    cursor: usize,
    /// Maximum number of commands to retain (oldest are dropped when exceeded).
    max_history: usize,
}
```

**Behavior:**
- `push(command)`: If `cursor < history.len()`, truncate the redo tail. Append command. If `history.len() > max_history`, drop the oldest command and adjust cursor.
- `undo()`: If `cursor > 0`, decrement cursor and call `history[cursor].undo(context)`.
- `redo()`: If `cursor < history.len()`, call `history[cursor].redo(context)` and increment cursor.
- `clear()`: Reset history and cursor. Called when loading a new file.

### UndoCommand Trait

```rust
pub trait UndoCommand: Debug {
    /// Human-readable description for UI display (e.g., "Add cuboid node")
    fn description(&self) -> &str;

    /// Reverse the command's effect
    fn undo(&self, ctx: &mut UndoContext);

    /// Re-apply the command's effect
    fn redo(&self, ctx: &mut UndoContext);

    /// What kind of refresh is needed after undo/redo
    fn refresh_mode(&self) -> UndoRefreshMode;
}
```

### UndoContext

The undo/redo methods need access to the node network and registry. Rather than passing all of `StructureDesigner` (which owns the `UndoStack`, creating borrow conflicts), we pass a focused context:

```rust
pub struct UndoContext<'a> {
    pub node_type_registry: &'a mut NodeTypeRegistry,
    /// Mutable so commands like AddNetwork/DeleteNetwork can switch the active network.
    pub active_network_name: &'a mut Option<String>,
}

impl<'a> UndoContext<'a> {
    /// Get mutable reference to a network by name.
    /// Commands use this with their stored network_name — NOT the active network,
    /// since undo/redo may fire while a different network is active.
    pub fn network_mut(&mut self, name: &str) -> Option<&mut NodeNetwork> {
        self.node_type_registry.node_networks.get_mut(name)
    }
}
```

**Why no `active_network_mut` helper:** Each command stores its target `network_name` and looks it up via `ctx.network_mut(&self.network_name)`. The `active_network_name` field exists only so that network-lifecycle commands (AddNetwork, DeleteNetwork) can switch the active network as part of undo/redo.

### UndoRefreshMode

```rust
pub enum UndoRefreshMode {
    /// Only UI needs updating (e.g., node moved)
    Lightweight,
    /// Re-evaluate specific nodes (e.g., node data changed)
    NodeDataChanged(Vec<u64>),
    /// Re-evaluate entire network (e.g., structural change)
    Full,
}
```

### Undo/Redo Execution in StructureDesigner

```rust
impl StructureDesigner {
    pub fn undo(&mut self) -> bool {
        // Temporarily take the undo stack to avoid borrow conflict
        let mut stack = std::mem::take(&mut self.undo_stack);
        let result = stack.undo(&mut UndoContext {
            node_type_registry: &mut self.node_type_registry,
            active_network_name: &mut self.active_node_network_name,
        });
        self.undo_stack = stack;

        if let Some(refresh_mode) = result {
            self.apply_refresh_mode(refresh_mode);
        }
        result.is_some()
    }

    pub fn redo(&mut self) -> bool {
        // Same pattern as undo
    }

    fn push_command(&mut self, command: impl UndoCommand + 'static) {
        self.undo_stack.push(Box::new(command));
    }
}
```

**Note:** `undo()` does not require an active network. Commands know their own target network name and look it up directly via `ctx.network_mut()`. The active network name is passed mutably so that network-lifecycle commands can update it.

## Command Types

### 1. AddNode

**Recorded by:** `StructureDesigner::add_node()`

```rust
struct AddNodeCommand {
    description: String,          // "Add cuboid"
    network_name: String,         // Which network this happened in
    node_id: u64,                 // ID assigned to the new node
    node_type_name: String,       // "cuboid"
    position: DVec2,              // Where it was placed
    node_data_json: Value,        // Serialized initial node data
    num_parameters: usize,        // Parameter count from NodeType
    // For parameter nodes:
    param_id: Option<u64>,        // Assigned param_id
    next_param_id_before: u64,    // To restore network.next_param_id on undo
}
```

- **redo:** Re-add the node with the same ID, type, position, and data. Restore param state if parameter node.
- **undo:** Remove the node from the network. If it was a parameter node, restore `next_param_id`.
- **refresh:** `Full` (structural change)

**Note:** `NodeNetwork::add_node` currently auto-assigns the next available ID. We need a variant or parameter that allows specifying the ID for redo to work correctly. This is a minor change: add an `add_node_with_id` method.

### 2. DeleteNodes

**Recorded by:** `StructureDesigner::delete_selected()`

```rust
struct DeleteNodesCommand {
    description: String,
    network_name: String,
    /// Full snapshot of each deleted node: type, position, data, custom_name, etc.
    deleted_nodes: Vec<NodeSnapshot>,
    /// All wires that were removed (both explicit wire deletions and wires
    /// connected to deleted nodes)
    deleted_wires: Vec<WireSnapshot>,
    /// Was the return node among the deleted? If so, store its ID.
    was_return_node: Option<u64>,
    /// Display state of deleted nodes
    display_states: Vec<(u64, NodeDisplayMode)>,
}

struct NodeSnapshot {
    node_id: u64,
    node_type_name: String,
    position: DVec2,
    custom_name: Option<String>,
    node_data_json: Value,
    /// All input arguments (connections into this node)
    arguments: Vec<ArgumentSnapshot>,
}

struct WireSnapshot {
    source_node_id: u64,
    source_output_pin_index: i32,
    dest_node_id: u64,
    dest_param_index: usize,
}
```

- **redo:** Re-delete all the same nodes and wires.
- **undo:** Re-add all deleted nodes with their original IDs, data, and positions. Re-establish all wires. Restore return node and display states.
- **refresh:** `Full`

### 3. DeleteWires

**Recorded by:** `StructureDesigner::delete_selected()` when only wires are selected.

```rust
struct DeleteWiresCommand {
    description: String,
    network_name: String,
    deleted_wires: Vec<WireSnapshot>,
}
```

- **redo:** Remove the wires.
- **undo:** Re-add the wires.
- **refresh:** `NodeDataChanged` for destination nodes.

### 4. ConnectWire

**Recorded by:** `StructureDesigner::connect_nodes()`

```rust
struct ConnectWireCommand {
    description: String,
    network_name: String,
    wire: WireSnapshot,
    /// If the destination pin was not multi-valued, connecting may have
    /// replaced an existing wire. Store the replaced wire for undo.
    replaced_wire: Option<WireSnapshot>,
}
```

- **redo:** Re-establish the wire (replacing if needed).
- **undo:** Remove the wire. If a wire was replaced, restore it.
- **refresh:** `NodeDataChanged(vec![wire.dest_node_id])`

### 5. SetNodeData

**Recorded by:** `StructureDesigner::set_node_network_data()`

This is the generic command that handles all `set_*_data` API calls.

```rust
struct SetNodeDataCommand {
    description: String,
    network_name: String,
    node_id: u64,
    node_type_name: String,
    old_data_json: Value,    // Serialized via node_data_saver before mutation
    new_data_json: Value,    // Serialized via node_data_saver after mutation
}
```

- **redo:** Deserialize `new_data_json` via `node_data_loader` and set on the node.
- **undo:** Deserialize `old_data_json` via `node_data_loader` and set on the node.
- **refresh:** `NodeDataChanged(vec![node_id])`

**Implementation in `set_node_network_data`:**

```rust
pub fn set_node_network_data(&mut self, node_id: u64, mut data: Box<dyn NodeData>) {
    let network_name = match &self.active_node_network_name {
        Some(name) => name.clone(),
        None => return,
    };

    // --- NEW: Capture before-state ---
    let old_data_json = self.snapshot_node_data(node_id);

    // ... existing mutation logic (unchanged) ...

    // --- NEW: Capture after-state and push command ---
    let new_data_json = self.snapshot_node_data(node_id);
    if let (Some(old_json), Some(new_json)) = (old_data_json, new_data_json) {
        self.push_command(SetNodeDataCommand {
            description: format!("Edit node"),
            network_name,
            node_id,
            node_type_name: /* from node */,
            old_data_json: old_json,
            new_data_json: new_json,
        });
    }
}
```

**Size concern:** For most nodes, the JSON is tiny (tens of bytes). For `atom_edit` nodes with large structures, this could be 50-100 KB per snapshot. This is acceptable for now; the dedicated atom_edit undo design will optimize this later.

### 6. MoveNodes

**Recorded by:** `StructureDesigner::move_node()` and `StructureDesigner::move_selected_nodes()`

```rust
struct MoveNodesCommand {
    description: String,
    network_name: String,
    /// (node_id, old_position, new_position)
    moves: Vec<(u64, DVec2, DVec2)>,
}
```

- **redo:** Set each node to `new_position`.
- **undo:** Set each node to `old_position`.
- **refresh:** `Lightweight`

**Coalescing via begin/end grouping:** During a drag operation, `move_selected_nodes` is called many times. We don't want each pixel of movement to be a separate undo step. The API layer calls `begin_move_nodes()` when the drag starts and `end_move_nodes()` when the drag ends. `begin_move_nodes()` captures the current positions of all selected nodes into a temporary `PendingMove` struct on StructureDesigner. Intermediate `move_selected_nodes` calls proceed normally without creating commands. `end_move_nodes()` reads the current positions, compares with the captured start positions, and creates a single `MoveNodesCommand`. If positions haven't changed (e.g., click without drag), no command is created.

```rust
/// Temporary state held during a drag operation
pub struct PendingMove {
    /// (node_id, position_at_drag_start)
    start_positions: Vec<(u64, DVec2)>,
}
```

### 7. SetReturnNode

**Recorded by:** `StructureDesigner::set_return_node_id()`

```rust
struct SetReturnNodeCommand {
    description: String,
    network_name: String,
    old_return_node_id: Option<u64>,
    new_return_node_id: Option<u64>,
}
```

- **redo:** Set return node to `new_return_node_id`.
- **undo:** Set return node to `old_return_node_id`.
- **refresh:** `Full`

### 8. SetNodeDisplay

**Recorded by:** `StructureDesigner::set_node_display()`

```rust
struct SetNodeDisplayCommand {
    description: String,
    network_name: String,
    node_id: u64,
    old_displayed: bool,
    new_displayed: bool,
}
```

- **redo:** Set display state to `new_displayed`.
- **undo:** Set display state to `old_displayed`.
- **refresh:** `Lightweight`

### 9. PasteNodes

**Recorded by:** `StructureDesigner::paste_at_position()`

```rust
struct PasteNodesCommand {
    description: String,
    network_name: String,
    /// IDs of the pasted nodes (needed for undo deletion)
    pasted_node_ids: Vec<u64>,
    /// Full snapshot of each pasted node (for redo re-creation)
    pasted_nodes: Vec<NodeSnapshot>,  // Reuse the same snapshot type
    /// Wires created between pasted nodes
    pasted_wires: Vec<WireSnapshot>,
}
```

- **redo:** Re-add all pasted nodes and wires with original IDs.
- **undo:** Delete all pasted nodes and their internal wires.
- **refresh:** `Full`

### 10. DuplicateNode

**Recorded by:** `StructureDesigner::duplicate_node()`

```rust
struct DuplicateNodeCommand {
    description: String,
    network_name: String,
    /// ID of the original node that was duplicated
    source_node_id: u64,
    /// ID assigned to the new duplicate node
    new_node_id: u64,
    /// Full snapshot of the duplicated node (for redo re-creation)
    node_snapshot: NodeSnapshot,
}
```

- **redo:** Re-add the duplicate node with the same ID and data.
- **undo:** Remove the duplicate node.
- **refresh:** `Full`

### 11. TextEditNetwork

**Recorded by:** `apply_text_to_active_network()` (API function that does the text edit)

Text edits can make arbitrary changes to the network. Rather than decomposing into fine-grained commands, we store before/after snapshots of the entire network.

```rust
struct TextEditNetworkCommand {
    description: String,
    network_name: String,
    /// Serialized network state before the text edit
    before_snapshot: Value,
    /// Serialized network state after the text edit
    after_snapshot: Value,
}
```

- **redo:** Deserialize `after_snapshot` and replace the network.
- **undo:** Deserialize `before_snapshot` and replace the network.
- **refresh:** `Full`

**Note:** This uses the existing `.cnnd` network serialization. A single network's JSON is typically a few KB to a few hundred KB. This is acceptable for undo history.

**Special handling:** `apply_text_to_active_network` is currently in the API layer and does its own network removal/reinsertion. It should be refactored to call a StructureDesigner method that handles the text edit and creates the undo command.

### 12. Network-Level Commands

#### AddNetwork

**Recorded by:** `StructureDesigner::add_new_node_network()` and `add_node_network_with_name()` API

```rust
struct AddNetworkCommand {
    description: String,
    network_name: String,
    /// The active network before this one was added (restored on undo)
    previous_active_network: Option<String>,
}
```

- **redo:** Re-add empty network with same name, switch active to it.
- **undo:** Remove the network, switch active back to `previous_active_network`.
- **refresh:** `Full`

**Note:** The API function `add_node_network_with_name()` calls both `add_node_network()` and `set_active_node_network_name()`. The `set_active_node_network_name()` call does NOT push a command (it's navigation). Instead, the `AddNetworkCommand` handles the active-network switch as part of its undo/redo logic.

#### DeleteNetwork

**Recorded by:** `StructureDesigner::delete_node_network()`

```rust
struct DeleteNetworkCommand {
    description: String,
    network_name: String,
    /// Full serialized network for restoration
    network_snapshot: Value,
    /// The active network after deletion (in case it changed)
    active_network_after: Option<String>,
    /// The active network before deletion (restored on redo→undo)
    active_network_before: Option<String>,
}
```

- **redo:** Delete the network, restore `active_network_after`.
- **undo:** Deserialize and re-add the network, restore `active_network_before`.
- **refresh:** `Full`

#### RenameNetwork

**Recorded by:** `StructureDesigner::rename_node_network()`

```rust
struct RenameNetworkCommand {
    description: String,
    old_name: String,
    new_name: String,
}
```

- **redo:** Rename old → new (calls same logic as original rename, including updating references).
- **undo:** Rename new → old.
- **refresh:** `Full`

### 13. FactorSelection

**Recorded by:** `StructureDesigner::factor_selection_into_subnetwork()`

This is a complex operation: it creates a new network, moves nodes into it, and replaces them with a custom node.

```rust
struct FactorSelectionCommand {
    description: String,
    source_network_name: String,
    subnetwork_name: String,
    /// Snapshot of the source network before factoring
    source_network_before: Value,
    /// The newly created subnetwork (for redo)
    subnetwork_snapshot: Value,
    /// Snapshot of the source network after factoring (for redo)
    source_network_after: Value,
}
```

- **redo:** Restore both networks to their after-factoring state.
- **undo:** Remove the subnetwork, restore the source network to its before-factoring state.
- **refresh:** `Full`

## Compound Commands

As shown in the "User Action to Command Mapping" analysis above, no compound command mechanism is currently needed. Every user action maps to exactly one command push. A `CompoundCommand` type can be added later if multi-step user actions are introduced.

## Per-Network vs Global Undo Stack

**Decision: Single global stack in StructureDesigner.**

Each command stores which `network_name` it applies to. When the user switches networks, the undo stack persists. If the user undoes while in a different network than the command targets, the undo still works — it modifies the target network via `ctx.network_mut(&self.network_name)`. This is the simplest approach and matches how most applications work.

**LIFO safety guarantee:** The strict LIFO ordering of the undo stack ensures that commands never target a non-existent network. For undo: if a `DeleteNetwork` is at position `j` and earlier commands target that network at positions `i < j`, the deletion is undone (restoring the network) before reaching position `i`. For redo: if an `AddNetwork` is at position `i` and later commands target it at positions `j > i`, the creation is redone before reaching `j`. The same argument applies to `RenameNetwork` — the rename is undone/redone before commands that use the old/new name are reached. No command validation or existence checks are needed as long as the stack remains strictly LIFO (no selective undo or per-network undo).

**Alternative considered:** Per-network undo stacks. This would be more intuitive if users frequently switch networks, but adds complexity (need to track which stack to use, handle cross-network operations like factoring). Deferred unless user feedback demands it.

## Helper Methods

```rust
impl StructureDesigner {
    /// Serialize a node's data to JSON using the registered node_data_saver.
    fn snapshot_node_data(&mut self, node_id: u64) -> Option<Value> {
        let network_name = self.active_node_network_name.as_ref()?;
        let node = self.node_type_registry
            .node_networks.get_mut(network_name)?
            .nodes.get_mut(&node_id)?;
        let node_type = self.node_type_registry
            .get_node_type(&node.node_type_name)?;
        (node_type.node_data_saver)(node.data.as_mut(), None).ok()
    }

    /// Serialize an entire network to JSON for snapshot-based undo.
    fn snapshot_network(&mut self, network_name: &str) -> Option<Value> {
        // Use existing cnnd serialization
    }

    /// Restore a network from a JSON snapshot.
    fn restore_network_from_snapshot(&mut self, network_name: &str, snapshot: &Value) -> bool {
        // Use existing cnnd deserialization
    }
}
```

**Borrow conflict note:** `snapshot_node_data` needs mutable access to `node.data` (because `node_data_saver` takes `&mut dyn NodeData`) and immutable access to the registry (to look up the saver function). This is the same split-borrow pattern already used elsewhere in StructureDesigner (e.g., `set_node_network_data` lines 1253-1266). The solution is to split the borrow:

```rust
let (built_in_types, node_networks) = (
    &self.node_type_registry.built_in_node_types,
    &mut self.node_type_registry.node_networks,
);
// Look up saver from built_in_types, look up node from node_networks
```

## API Layer Changes

The API layer needs two new functions:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn undo() -> bool { /* delegates to structure_designer.undo() + refresh */ }

#[flutter_rust_bridge::frb(sync)]
pub fn redo() -> bool { /* delegates to structure_designer.redo() + refresh */ }

#[flutter_rust_bridge::frb(sync)]
pub fn can_undo() -> bool { /* delegates to structure_designer.undo_stack.can_undo() */ }

#[flutter_rust_bridge::frb(sync)]
pub fn can_redo() -> bool { /* delegates to structure_designer.undo_stack.can_redo() */ }

/// Called by Flutter when a node drag begins
#[flutter_rust_bridge::frb(sync)]
pub fn begin_move_nodes() { /* captures current positions of selected nodes */ }

/// Called by Flutter when a node drag ends
#[flutter_rust_bridge::frb(sync)]
pub fn end_move_nodes() { /* creates MoveNodesCommand from begin positions to current */ }
```

## Flutter Integration

### Keyboard Shortcuts

- `Ctrl+Z` → call `undo()` API
- `Ctrl+Shift+Z` (or `Ctrl+Y`) → call `redo()` API

### UI Indicators

- Edit menu items "Undo" / "Redo" with descriptions from `UndoCommand::description()`
- Grayed out when `can_undo()` / `can_redo()` returns false

### Move Drag Integration

The Flutter node network widget already tracks pointer down/up for drag operations. Add calls to `begin_move_nodes()` on drag start and `end_move_nodes()` on drag end.

## Integration with Evaluation & Caching

The existing change tracking system handles undo/redo naturally:

| Command | After undo/redo call |
|---------|---------------------|
| SetNodeData | `mark_node_data_changed(node_id)` → partial eval of node + downstream |
| ConnectWire / DeleteWires | `mark_node_data_changed(dest_node_id)` → partial eval |
| MoveNodes | `mark_lightweight_refresh()` → UI only, no re-eval |
| AddNode, DeleteNodes, Paste, TextEdit, Factor | `mark_full_refresh()` → re-eval all |
| SetReturnNode | `mark_full_refresh()` → re-eval all |
| SetNodeDisplay | visibility change tracking → lightweight |

The CSG geometry cache (keyed by content hash) automatically provides cache hits for previously-computed geometry when undoing/redoing. No special integration needed.

## NodeNetwork Changes Required

### add_node_with_id

Currently `NodeNetwork::add_node` auto-assigns IDs. For redo to recreate a node with the same ID, we need:

```rust
impl NodeNetwork {
    /// Add a node with a specific ID (used by undo/redo system)
    pub fn add_node_with_id(
        &mut self,
        node_id: u64,
        node_type_name: &str,
        position: DVec2,
        num_parameters: usize,
        node_data: Box<dyn NodeData>,
    ) {
        // Same as add_node but uses the provided node_id
        // Also updates next_node_id if node_id >= next_node_id
    }
}
```

### Wire Capture Before Deletion

`NodeNetwork::delete_selected()` currently doesn't return information about what was deleted. We need it (or a pre-deletion query) to return the set of affected wires and nodes for the undo command. Options:

- **Option A:** Add a `collect_deletion_info()` method that returns what *would* be deleted without doing it. StructureDesigner calls this before `delete_selected()`.
- **Option B:** Change `delete_selected()` to return a `DeletionResult` struct with the deleted data.

Option A is cleaner because it separates query from mutation.

## Undo Stack Persistence

The undo stack is **not** persisted to `.cnnd` files. When a file is saved and reloaded, undo history starts fresh. This is standard for most applications and avoids serialization complexity for command objects.

The undo stack **is** cleared when:
- A new file is loaded
- A new file is created
- The file is closed

## Nodes Requiring Special Handling (Deferred)

The following nodes have complex state that may benefit from specialized undo commands instead of generic `SetNodeData` snapshots:

| Node | Reason | Status |
|------|--------|--------|
| `atom_edit` | Large `AtomicStructure` diff state; interactive sub-operations (add atom, add bond, guided placement, minimize) | Deferred to separate design document |
| `edit_atom` | Deprecated node, will be removed from codebase | No-op: undo commands are not recorded for this node |

All other nodes (~45 types) work with generic `SetNodeData` using `serde_json::Value` snapshots. Their serialized data is small (typically < 1 KB).

## Phased Implementation Plan

This plan is designed so that each phase produces a working, testable increment. Every phase ends with `cargo test` and `cargo clippy` passing. Commit after each phase.

### Key Source Files Reference

| File | Contains |
|------|----------|
| `rust/src/structure_designer/structure_designer.rs` | `StructureDesigner` struct (line 53), all core mutation methods |
| `rust/src/structure_designer/node_network.rs` | `NodeNetwork` struct (line 195), `Node` struct (line 101) |
| `rust/src/structure_designer/node_type_registry.rs` | `NodeTypeRegistry` (line 65) |
| `rust/src/structure_designer/mod.rs` | Module tree — add `pub mod undo;` here |
| `rust/src/structure_designer/serialization/node_networks_serialization.rs` | CNND save/load — reuse for snapshots |
| `rust/src/api/structure_designer/structure_designer_api.rs` | API layer — add `undo()`, `redo()`, etc. |
| `lib/structure_designer/` | Flutter UI — keyboard shortcuts |

---

### Phase 1: Core Infrastructure + Test Harness

**Goal:** `UndoStack`, `UndoCommand` trait, `UndoContext`, snapshot helpers, test infrastructure. No commands yet — just the skeleton that all later phases build on.

**Create files:**

1. **`rust/src/structure_designer/undo/mod.rs`** — Core types:
   - `UndoStack` struct with `history: Vec<Box<dyn UndoCommand>>`, `cursor: usize`, `max_history: usize`, `recording_suppressed: bool`
   - Methods: `push()`, `undo()`, `redo()`, `can_undo()`, `can_redo()`, `clear()`, `suppress_recording()`, `resume_recording()`
   - `push()` logic: if `cursor < history.len()`, truncate redo tail. Append. If over `max_history`, drop oldest and adjust cursor.
   - `undo()` returns `Option<UndoRefreshMode>` — decrements cursor, calls `command.undo(ctx)`, returns `command.refresh_mode()`
   - `redo()` same pattern but increments cursor
   - `UndoCommand` trait: `description()`, `undo(&self, ctx: &mut UndoContext)`, `redo(&self, ctx: &mut UndoContext)`, `refresh_mode(&self) -> UndoRefreshMode`
   - `UndoContext` struct: `node_type_registry: &'a mut NodeTypeRegistry`, `active_network_name: &'a mut Option<String>`. Method: `network_mut(name: &str) -> Option<&mut NodeNetwork>`
   - `UndoRefreshMode` enum: `Lightweight`, `NodeDataChanged(Vec<u64>)`, `Full`
   - Implement `Default` for `UndoStack` (empty history, cursor 0, `max_history = 100`)

2. **`rust/src/structure_designer/undo/snapshot.rs`** — Snapshot types:
   - `NodeSnapshot` struct: `node_id`, `node_type_name`, `position`, `custom_name`, `node_data_json: serde_json::Value`, `arguments: Vec<ArgumentSnapshot>`
   - `ArgumentSnapshot` struct: mirrors `Argument` — `argument_output_pins: HashMap<u64, i32>`
   - `WireSnapshot` struct: `source_node_id`, `source_output_pin_index`, `dest_node_id`, `dest_param_index`
   - `PendingMove` struct: `start_positions: Vec<(u64, DVec2)>`

3. **`rust/src/structure_designer/undo/commands/mod.rs`** — Empty, just declares sub-modules (added in later phases).

**Modify files:**

4. **`rust/src/structure_designer/mod.rs`** — Add `pub mod undo;`

5. **`rust/src/structure_designer/structure_designer.rs`**:
   - Add field: `pub undo_stack: UndoStack` (initialized to `UndoStack::default()` in `new()`)
   - Add field: `pub pending_move: Option<PendingMove>`
   - Add method `undo(&mut self) -> bool` — takes undo_stack via `std::mem::take`, creates `UndoContext` with `&mut self.node_type_registry` and `&mut self.active_node_network_name`, calls `stack.undo(ctx)`, puts stack back, calls `apply_refresh_mode()` if needed
   - Add method `redo(&mut self) -> bool` — same pattern
   - Add method `push_command(&mut self, command: impl UndoCommand + 'static)` — delegates to `self.undo_stack.push(Box::new(command))`
   - Add helper `snapshot_node_data(&mut self, node_id: u64) -> Option<Value>` — split-borrow `built_in_node_types` and `node_networks`, look up the node, call `node_data_saver`, return the JSON. Must handle the borrow split carefully: get the saver function from `built_in_node_types` (or from `node_networks` if it's a custom node type), get the node from `node_networks`, call the saver.
   - Add helper `snapshot_node(&mut self, network_name: &str, node_id: u64) -> Option<NodeSnapshot>` — captures full node state including arguments
   - Add method `apply_refresh_mode(&mut self, mode: UndoRefreshMode)` — matches on mode and calls the appropriate `mark_*` methods
   - Clear undo stack in `new_project()` and in the load-file path

6. **`rust/src/structure_designer/node_network.rs`**:
   - Add method `add_node_with_id(node_id, node_type_name, position, num_parameters, node_data)` — same as `add_node` but uses the provided ID, and updates `next_node_id = max(next_node_id, node_id + 1)`
   - Add method `collect_deletion_info(&self) -> DeletionInfo` — returns what `delete_selected()` *would* delete: the set of selected node IDs, all wires connected to those nodes, and selected wires. Does NOT mutate anything.

**Create test file:**

7. **`rust/tests/structure_designer/undo_test.rs`**:
   - Add `setup_designer_with_network(name) -> StructureDesigner` helper
   - Add `snapshot_all_networks(registry: &mut NodeTypeRegistry) -> Value` — use the existing CNND serialization to produce an in-memory JSON value. Look at `save_node_networks_to_file` in `node_networks_serialization.rs` — it builds a JSON structure internally before writing to disk. Extract or replicate that logic to return a `Value` instead.
   - Add `assert_undo_redo_roundtrip(designer, action)` helper (see Testing Strategy section)
   - Write UndoStack unit tests: push/undo/redo cursor behavior, redo tail truncation on push, max_history eviction, can_undo/can_redo, clear, suppression
   - Register in `rust/tests/structure_designer.rs`: add `mod undo_test;`

**Verification:** `cargo test --test structure_designer undo` passes. UndoStack works in isolation. No commands exist yet.

---

### Phase 2: SetNodeData Command

**Goal:** The most impactful single command — covers all node property edits (~45 node types).

**Create:**

1. **`rust/src/structure_designer/undo/commands/set_node_data.rs`**:
   - `SetNodeDataCommand` struct with fields from the design doc
   - Implement `UndoCommand`: `undo` deserializes `old_data_json` via `node_data_loader` (looked up from registry) and sets on the node. `redo` does the same with `new_data_json`.
   - `refresh_mode()` returns `NodeDataChanged(vec![node_id])`

**Modify:**

2. **`rust/src/structure_designer/structure_designer.rs`** — In `set_node_network_data()` (line ~1214):
   - Before the mutation: call `snapshot_node_data(node_id)` to capture `old_data_json`
   - After the mutation: call `snapshot_node_data(node_id)` to capture `new_data_json`
   - Push `SetNodeDataCommand` if both snapshots succeeded
   - **Suppression for `edit_atom`**: Check if the node's type name is `"edit_atom"`. If so, skip pushing the command (the deprecated node — see design doc "No-op" note).

3. **`rust/src/structure_designer/undo/commands/mod.rs`** — Add `pub mod set_node_data;`

**Test:**

4. In `undo_test.rs`:
   - `undo_set_node_data`: Create a float node, change its value via `set_node_network_data`, call `assert_undo_redo_roundtrip`
   - `undo_set_node_data_multiple_edits`: Edit the same node multiple times, undo all, verify original state
   - Test with different node types (float, vec3, sphere) to exercise generic `node_data_saver`/`node_data_loader`

**Verification:** `cargo test undo_set_node_data` passes.

---

### Phase 3: AddNode + DeleteNodes + DuplicateNode

**Goal:** Node structural mutations. These are tightly coupled (delete is the inverse of add), so implement together.

**Create:**

1. **`rust/src/structure_designer/undo/commands/add_node.rs`**:
   - `AddNodeCommand` with fields from design doc
   - `undo`: Remove the node from the network (delete by ID)
   - `redo`: Re-add with `add_node_with_id()`, restore `node_data_json`, restore `param_id` and `next_param_id` if parameter node

2. **`rust/src/structure_designer/undo/commands/delete_nodes.rs`**:
   - `DeleteNodesCommand` with `deleted_nodes: Vec<NodeSnapshot>`, `deleted_wires: Vec<WireSnapshot>`, `was_return_node`, `display_states`
   - `undo`: Re-add all nodes via `add_node_with_id()`, restore data/positions/custom_names, re-establish all wires, restore return node and display states
   - `redo`: Re-delete the same nodes and wires

3. **`rust/src/structure_designer/undo/commands/delete_wires.rs`**:
   - `DeleteWiresCommand` — for when only wires are selected (no nodes)
   - `undo`: Re-add wires. `redo`: Re-remove wires.

4. **`rust/src/structure_designer/undo/commands/duplicate_node.rs`**:
   - `DuplicateNodeCommand` with source_node_id, new_node_id, node_snapshot
   - `undo`: Remove the duplicate. `redo`: Re-add it.

**Modify:**

5. **`rust/src/structure_designer/structure_designer.rs`**:
   - In `add_node()` (line ~694): After adding the node, capture its state and push `AddNodeCommand`. For parameter nodes, also capture `param_id` and `next_param_id` before/after.
   - In `delete_selected()` (line ~2017): Before deletion, call `collect_deletion_info()` to get all nodes/wires that will be deleted. Snapshot each node via `snapshot_node()`. Capture return node state. Then perform the existing deletion. Push `DeleteNodesCommand` or `DeleteWiresCommand` depending on what was deleted.
   - In `duplicate_node()` (line ~799): After duplication, snapshot the new node and push `DuplicateNodeCommand`.

6. **`rust/src/structure_designer/undo/commands/mod.rs`** — Add the new modules.

**Test:**

7. In `undo_test.rs`:
   - `undo_add_node` — add node, undo, verify network is empty
   - `undo_delete_single_node` — add node, select, delete, undo, verify node restored
   - `undo_delete_connected_nodes` — build a small graph (3 nodes, 2 wires), delete a node, undo, verify wires restored
   - `undo_delete_wires_only` — select wires, delete, undo
   - `undo_delete_return_node` — set a return node, delete it, undo, verify return node ID restored
   - `undo_duplicate_node` — duplicate, undo, verify only original remains
   - `undo_sequence_add_delete` — add 3 nodes, delete 2, undo all, verify empty

**Verification:** `cargo test undo_add\|undo_delete\|undo_duplicate` passes.

---

### Phase 4: ConnectWire + MoveNodes + SetReturnNode + SetNodeDisplay

**Goal:** Wire operations, move coalescing, and simple state toggles.

**Create:**

1. **`rust/src/structure_designer/undo/commands/connect_wire.rs`**:
   - `ConnectWireCommand` with `wire: WireSnapshot`, `replaced_wire: Option<WireSnapshot>`
   - `undo`: Remove the wire. If `replaced_wire` is Some, re-establish it.
   - `redo`: Re-establish the wire (removing any existing wire on that pin first).

2. **`rust/src/structure_designer/undo/commands/move_nodes.rs`**:
   - `MoveNodesCommand` with `moves: Vec<(u64, DVec2, DVec2)>` (node_id, old_pos, new_pos)
   - `undo`: Set each node to old_pos. `redo`: Set each node to new_pos.
   - `refresh_mode()` returns `Lightweight`

3. **`rust/src/structure_designer/undo/commands/set_return_node.rs`**:
   - `SetReturnNodeCommand` with old/new return_node_id

4. **`rust/src/structure_designer/undo/commands/set_node_display.rs`**:
   - `SetNodeDisplayCommand` with old/new displayed state

**Modify:**

5. **`rust/src/structure_designer/structure_designer.rs`**:
   - In `connect_nodes()` (line ~979): Before connecting, check if an existing wire will be replaced on the destination pin (non-multi-valued pins). Capture it. After connecting, push `ConnectWireCommand`.
   - Add `begin_move_nodes(&mut self)`: Capture current positions of all selected nodes into `self.pending_move = Some(PendingMove { start_positions })`.
   - Add `end_move_nodes(&mut self)`: Compare `pending_move.start_positions` with current positions. If any changed, push `MoveNodesCommand`. Set `self.pending_move = None`. If no change (click without drag), push nothing.
   - `move_selected_nodes()` (line ~1797) is unchanged — it does NOT push commands. The begin/end wrapper handles it.
   - In `set_return_node_id()`: Capture old return node ID, perform set, push command.
   - In `set_node_display()`: Capture old display state, perform set, push command.

6. **`rust/src/api/structure_designer/structure_designer_api.rs`**:
   - Add `begin_move_nodes()` API function — calls `designer.begin_move_nodes()`
   - Add `end_move_nodes()` API function — calls `designer.end_move_nodes()`

**Test:**

7. In `undo_test.rs`:
   - `undo_connect_wire` — connect two nodes, undo, verify no wire
   - `undo_connect_wire_that_replaced_existing` — connect A→C, then B→C on same pin, undo, verify A→C restored
   - `undo_move_nodes` — begin_move, move 3 times, end_move, undo = single step, original positions
   - `move_without_actual_movement_creates_no_command` — begin/end with no move calls
   - `undo_set_return_node`, `undo_set_node_display` — straightforward roundtrips

**Verification:** `cargo test undo_connect\|undo_move\|undo_set_return\|undo_set_node_display` passes.

---

### Phase 5: PasteNodes + API Functions

**Goal:** Copy/paste undo, and expose undo/redo to Flutter.

**Create:**

1. **`rust/src/structure_designer/undo/commands/paste_nodes.rs`**:
   - `PasteNodesCommand` with `pasted_nodes: Vec<NodeSnapshot>`, `pasted_wires: Vec<WireSnapshot>`, `pasted_node_ids: Vec<u64>`
   - `undo`: Delete all pasted nodes. `redo`: Re-add them with original IDs.

**Modify:**

2. **`rust/src/structure_designer/structure_designer.rs`**:
   - In `paste_at_position()` (line ~877): After pasting, snapshot all newly created nodes (their IDs are known from the paste logic). Collect internal wires between pasted nodes. Push `PasteNodesCommand`.

3. **`rust/src/api/structure_designer/structure_designer_api.rs`**:
   - Add `undo() -> bool` — `with_mut_cad_instance`, call `designer.undo()`, call `refresh_structure_designer_auto`
   - Add `redo() -> bool` — same pattern
   - Add `can_undo() -> bool` — `with_cad_instance`, return `designer.undo_stack.can_undo()`
   - Add `can_redo() -> bool` — same
   - All marked `#[flutter_rust_bridge::frb(sync)]`

4. Run `flutter_rust_bridge_codegen generate` to update FFI bindings.

**Test:**

5. In `undo_test.rs`:
   - `undo_paste_nodes` — copy a selection, paste, undo, verify pasted nodes removed
   - `undo_cut_is_single_step` — cut = copy + delete → single undo step
   - `undo_redo_api_roundtrip` — test through the API functions (if possible without Flutter)

**Verification:** `cargo test undo_paste\|undo_cut` passes.

---

### Phase 6: Network-Level Commands

**Goal:** Add/delete/rename network undo. These are less frequent but important for correctness.

**Create:**

1. **`rust/src/structure_designer/undo/commands/add_network.rs`**:
   - `AddNetworkCommand` with `network_name`, `previous_active_network`
   - `undo`: Delete the network, set active to `previous_active_network`
   - `redo`: Re-add empty network, set active to it

2. **`rust/src/structure_designer/undo/commands/delete_network.rs`**:
   - `DeleteNetworkCommand` with `network_name`, `network_snapshot: Value`, `active_network_before`, `active_network_after`
   - `undo`: Deserialize and re-add the network, set active to `active_network_before`
   - `redo`: Delete, set active to `active_network_after`

3. **`rust/src/structure_designer/undo/commands/rename_network.rs`**:
   - `RenameNetworkCommand` with `old_name`, `new_name`
   - `undo`/`redo`: Rename in the appropriate direction, updating all references (same logic as the existing rename)

**Modify:**

4. **`rust/src/structure_designer/structure_designer.rs`**:
   - In `add_node_network()` / `add_new_node_network()`: Capture `previous_active_network`, push `AddNetworkCommand` after adding
   - In `delete_node_network()`: Snapshot the network to JSON before deletion, capture active network before/after, push `DeleteNetworkCommand`
   - In `rename_node_network()`: Push `RenameNetworkCommand`
   - Add `snapshot_network(name: &str) -> Option<Value>` helper — serialize a single network to JSON using CNND serialization logic
   - Add `restore_network_from_snapshot(name: &str, snapshot: &Value) -> bool` helper — deserialize and insert into registry

**Test:**

5. In `undo_test.rs`:
   - `undo_add_network` — add network, undo, verify removed and active network restored
   - `undo_delete_network_restores_contents` — build a network with nodes, delete it, undo, verify full contents restored
   - `undo_rename_network` — rename, undo, verify old name exists
   - `undo_rename_then_undo_earlier_commands` — the LIFO rename safety test
   - `undo_across_network_switch` — the cross-network undo test from Testing Strategy

**Verification:** `cargo test undo_add_network\|undo_delete_network\|undo_rename` passes.

---

### Phase 7: TextEditNetwork + FactorSelection

**Goal:** The two most complex commands — both use full network snapshots.

**Create:**

1. **`rust/src/structure_designer/undo/commands/text_edit_network.rs`**:
   - `TextEditNetworkCommand` with `network_name`, `before_snapshot: Value`, `after_snapshot: Value`
   - `undo`: Replace network with `before_snapshot`. `redo`: Replace with `after_snapshot`.

2. **`rust/src/structure_designer/undo/commands/factor_selection.rs`**:
   - `FactorSelectionCommand` with `source_network_name`, `subnetwork_name`, `source_network_before: Value`, `source_network_after: Value`, `subnetwork_snapshot: Value`
   - `undo`: Remove subnetwork, restore source from `source_network_before`
   - `redo`: Restore source from `source_network_after`, add subnetwork from `subnetwork_snapshot`

**Modify:**

3. **`rust/src/structure_designer/structure_designer.rs`** (or the API layer):
   - In `apply_text_to_active_network()`: Snapshot active network before text edit, perform the edit, snapshot after, push `TextEditNetworkCommand`. This function currently lives in the API layer — either refactor it into a StructureDesigner method, or have the API layer create the command and push it via `designer.push_command()`. The latter is simpler and acceptable as an exception.
   - In `factor_selection_into_subnetwork()` (line ~2950): Snapshot source network before factoring, perform the operation, snapshot source network after and the new subnetwork, push `FactorSelectionCommand`.

**Test:**

4. In `undo_test.rs`:
   - `undo_text_edit_network` — build a network, apply a text edit that changes it, undo, verify original state
   - `undo_factor_selection` — build a network, select some nodes, factor, undo, verify subnetwork removed and source network restored
   - Sequence test: factor, edit inside subnetwork, undo all — should restore to pre-factor state

**Verification:** `cargo test undo_text_edit\|undo_factor` passes.

---

### Phase 8: Integration Tests + Flutter Integration

**Goal:** End-to-end sequence tests. Connect undo/redo to the Flutter UI.

**Rust tests:**

1. In `undo_test.rs`, add comprehensive sequence tests:
   - `undo_full_workflow` — add nodes, connect, edit data, move, delete, paste, undo all → initial state
   - `undo_redo_interleaved` — do 5 ops, undo 3, do 2 new ops, undo all → initial state
   - `undo_max_history_eviction` — push more than `max_history` commands, verify oldest are dropped and remaining undo correctly
   - `redo_tail_truncation_after_new_command` — undo, push new, verify redo is gone
   - Test with real `.cnnd` sample files: load a sample, perform edits, undo all, compare with original

**Flutter integration:**

2. **`lib/structure_designer/`** — Add keyboard shortcut handling:
   - `Ctrl+Z` → call `undo()` API, then refresh UI
   - `Ctrl+Shift+Z` / `Ctrl+Y` → call `redo()` API, then refresh UI
   - Read the `lib/AGENTS.md` before modifying Flutter files

3. **UI indicators** (if edit menu exists):
   - "Undo" / "Redo" menu items, disabled when `can_undo()` / `can_redo()` returns false

4. **Move drag integration** — In the Flutter node network widget:
   - Call `begin_move_nodes()` when a node drag starts (pointer down on selected node)
   - Call `end_move_nodes()` when the drag ends (pointer up)

5. Run `flutter_rust_bridge_codegen generate` if any API signatures changed.

**Verification:** `cargo test undo` passes. `flutter analyze` clean. Manual test: open app, add nodes, Ctrl+Z undoes, Ctrl+Shift+Z redoes.

---

### Phase Summary

| Phase | Commands | Tests | Builds on |
|-------|----------|-------|-----------|
| 1 | *(none — infrastructure only)* | UndoStack unit tests, test harness | — |
| 2 | SetNodeData | 3 tests | Phase 1 |
| 3 | AddNode, DeleteNodes, DeleteWires, DuplicateNode | 7 tests | Phases 1-2 |
| 4 | ConnectWire, MoveNodes, SetReturnNode, SetNodeDisplay | 6 tests | Phases 1-3 |
| 5 | PasteNodes + API exposure | 3 tests | Phases 1-4 |
| 6 | AddNetwork, DeleteNetwork, RenameNetwork | 5 tests | Phases 1-5 |
| 7 | TextEditNetwork, FactorSelection | 3 tests | Phases 1-6 |
| 8 | *(none — integration + Flutter)* | 4+ sequence tests | All |

Phases 1-4 deliver a usable undo system for the most common operations. Phases 5-7 handle less frequent operations. Phase 8 connects everything to the UI and validates end-to-end.

## Testing Strategy

### Core Invariants

Every undo/redo test verifies one or both of these properties:

1. **do + undo = identity:** Performing a command then undoing it restores the exact original state.
2. **do + undo + redo = do:** Undoing then redoing restores the exact post-command state.

These hold for single commands, sequences of commands, and cross-network scenarios.

### State Snapshot Mechanism

Tests need to capture and compare the full state of all networks in the registry. The existing CNND serialization already serializes every network to JSON (nodes, wires, positions, node data, return node, display state). We add an in-memory variant for testing:

```rust
/// Serialize all networks to a comparable JSON Value.
/// Uses the same codepath as CNND file saving, but returns the Value
/// instead of writing to disk.
fn snapshot_all_networks(registry: &mut NodeTypeRegistry) -> Value {
    // Use the existing CNND serialization logic to produce a serde_json::Value
    // containing all networks, their nodes, wires, node data, etc.
}
```

Two snapshots are equal if their `Value`s are equal (`==`). This is a deep structural comparison that covers:
- Node count, IDs, types, positions, custom names
- Wire connections (arguments with source node/pin)
- Node data (serialized via `node_data_saver`)
- Return node IDs, display state, next_node_id
- Network-level metadata (name, parameters, output type)

**Why JSON equality works:** The CNND serialization is already roundtrip-tested (see `cnnd_roundtrip_test.rs`). If two registries produce the same JSON, they are equivalent for all observable purposes. Node data goes through `node_data_saver` which produces deterministic JSON for all built-in node types.

### Test Helper

```rust
/// Captures state, runs an action, then verifies undo/redo invariants.
fn assert_undo_redo_roundtrip(
    designer: &mut StructureDesigner,
    action: impl FnOnce(&mut StructureDesigner),
) {
    let before = snapshot_all_networks(&mut designer.node_type_registry);

    action(designer);

    let after = snapshot_all_networks(&mut designer.node_type_registry);

    // Property 1: do + undo = identity
    let undone = designer.undo();
    assert!(undone, "undo() should return true after a command");
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(before, after_undo, "State after undo should match state before action");

    // Property 2: do + undo + redo = do
    let redone = designer.redo();
    assert!(redone, "redo() should return true after an undo");
    let after_redo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(after, after_redo, "State after redo should match state after action");

    // Undo again to leave designer in original state for composability
    designer.undo();
}
```

This helper is the workhorse for most tests. It verifies both invariants in a single call and leaves the designer in the original state, so tests can compose multiple `assert_undo_redo_roundtrip` calls.

### Test Categories

#### 1. Single-Command Tests (one per command type)

Each command type gets a dedicated test that builds a network, performs the action, and calls `assert_undo_redo_roundtrip`:

```rust
#[test]
fn undo_add_node() {
    let mut designer = setup_designer_with_network("test");
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.add_node("sphere", DVec2::new(100.0, 50.0));
    });
}

#[test]
fn undo_delete_nodes() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    select_node(&mut designer, id);
    // Clear the undo stack so the add_node command isn't in the way
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.delete_selected();
    });
}

#[test]
fn undo_connect_wire() {
    let mut designer = setup_designer_with_network("test");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.connect_nodes(float_id, 0, sphere_id, 0);
    });
}

#[test]
fn undo_set_node_data() { /* ... */ }

#[test]
fn undo_move_nodes() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    select_node(&mut designer, id);
    designer.undo_stack.clear();

    // Move uses begin/end grouping
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.begin_move_nodes();
        d.move_selected_nodes(DVec2::new(10.0, 0.0));
        d.move_selected_nodes(DVec2::new(10.0, 0.0));
        d.move_selected_nodes(DVec2::new(10.0, 0.0));
        d.end_move_nodes();
    });
}

// ... one test per command type: SetReturnNode, SetNodeDisplay, DuplicateNode,
//     PasteNodes, TextEditNetwork, AddNetwork, DeleteNetwork, RenameNetwork,
//     FactorSelection, DeleteWires
```

#### 2. Sequence Tests (undo-all restores initial state)

These verify that a sequence of N commands, undone N times, returns to the original state:

```rust
#[test]
fn undo_sequence_restores_initial_state() {
    let mut designer = setup_designer_with_network("test");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Perform a sequence of varied operations
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);         // cmd 1
    let float_id = designer.add_node("float", DVec2::new(200.0, 0.0)); // cmd 2
    designer.connect_nodes(float_id, 0, sphere_id, 0);                 // cmd 3
    designer.set_return_node_id(Some(sphere_id));                       // cmd 4

    // Undo all 4
    for _ in 0..4 {
        assert!(designer.undo());
    }
    assert!(!designer.undo()); // Nothing left to undo

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_all);

    // Redo all 4
    for _ in 0..4 {
        assert!(designer.redo());
    }
    assert!(!designer.redo()); // Nothing left to redo

    // Undo all again to verify full cycle
    for _ in 0..4 {
        assert!(designer.undo());
    }
    let after_second_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_second_undo_all);
}
```

#### 3. Redo Tail Truncation Tests

Verify that doing new work after an undo discards the redo tail:

```rust
#[test]
fn new_command_after_undo_truncates_redo() {
    let mut designer = setup_designer_with_network("test");
    designer.add_node("sphere", DVec2::ZERO);    // cmd 1
    designer.add_node("cuboid", DVec2::ZERO);     // cmd 2

    designer.undo(); // undo cmd 2, now cmd 2 is in redo tail

    designer.add_node("cylinder", DVec2::ZERO);   // cmd 3 — truncates cmd 2

    assert!(!designer.redo()); // cmd 2 is gone, can't redo
    assert!(designer.undo());  // undo cmd 3
    assert!(designer.undo());  // undo cmd 1
    assert!(!designer.undo()); // nothing left
}
```

#### 4. Cross-Network Tests

Verify that undo works correctly when commands target different networks:

```rust
#[test]
fn undo_across_network_switch() {
    let mut designer = setup_designer_with_network("net_a");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Work in net_a
    designer.add_node("sphere", DVec2::ZERO);

    // Switch to net_b
    designer.add_node_network("net_b");
    designer.set_active_node_network_name(Some("net_b".to_string()));
    designer.add_node("cuboid", DVec2::ZERO);

    // Undo all (while active network is net_b)
    for _ in 0..3 { // add_node(cuboid), add_network(net_b), add_node(sphere)
        assert!(designer.undo());
    }

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_all);
}
```

#### 5. Network Lifecycle Tests

Verify the LIFO safety guarantee — undo/redo across network creation, deletion, and rename:

```rust
#[test]
fn undo_delete_network_restores_contents() {
    let mut designer = setup_designer_with_network("main");

    // Build a network with content
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));

    let before_delete = snapshot_all_networks(&mut designer.node_type_registry);

    // Delete it
    designer.set_active_node_network_name(Some("main".to_string()));
    designer.delete_node_network("helper");

    // Undo delete — network and all contents should be restored
    designer.undo();
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(before_delete, after_undo);
}

#[test]
fn undo_rename_network_then_undo_earlier_commands() {
    let mut designer = setup_designer_with_network("alpha");
    designer.add_node("sphere", DVec2::ZERO); // targets "alpha"
    designer.rename_node_network("alpha", "beta"); // rename

    // Undo rename — network is "alpha" again
    designer.undo();
    // Undo add_node — targets "alpha", which exists
    designer.undo();
    // Both should succeed without panicking
}
```

#### 6. Edge Case Tests

```rust
#[test]
fn undo_on_empty_stack_returns_false() {
    let mut designer = setup_designer_with_network("test");
    assert!(!designer.undo());
    assert!(!designer.redo());
}

#[test]
fn move_without_actual_movement_creates_no_command() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    select_node(&mut designer, id);
    designer.undo_stack.clear();

    designer.begin_move_nodes();
    // No move_selected_nodes calls — click without drag
    designer.end_move_nodes();

    assert!(!designer.undo()); // No command was created
}

#[test]
fn undo_connect_wire_that_replaced_existing() {
    let mut designer = setup_designer_with_network("test");
    let float1 = designer.add_node("float", DVec2::ZERO);
    let float2 = designer.add_node("float", DVec2::new(0.0, 100.0));
    let sphere = designer.add_node("sphere", DVec2::new(200.0, 0.0));

    // Connect float1 → sphere pin 0
    designer.connect_nodes(float1, 0, sphere, 0);
    designer.undo_stack.clear();

    // Connect float2 → sphere pin 0 (replaces float1's wire)
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.connect_nodes(float2, 0, sphere, 0);
    });
    // After undo: float1's wire should be restored
}

#[test]
fn max_history_drops_oldest_command() {
    let mut designer = setup_designer_with_network("test");
    designer.undo_stack.max_history = 3;

    designer.add_node("sphere", DVec2::ZERO);   // cmd 1
    designer.add_node("cuboid", DVec2::ZERO);    // cmd 2
    designer.add_node("cylinder", DVec2::ZERO);  // cmd 3
    designer.add_node("float", DVec2::ZERO);     // cmd 4 — drops cmd 1

    // Can only undo 3 times, not 4
    assert!(designer.undo()); // undo cmd 4
    assert!(designer.undo()); // undo cmd 3
    assert!(designer.undo()); // undo cmd 2
    assert!(!designer.undo()); // cmd 1 was dropped
}
```

#### 7. Compound Operation Tests

Verify that compound user actions produce exactly one undo step:

```rust
#[test]
fn cut_is_single_undo_step() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    select_node(&mut designer, id);
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.cut_selection();
    });
    // Only one undo() needed — copy doesn't push, only delete does
}
```

### Test File Organization

```
rust/tests/
├── structure_designer/
│   └── undo_test.rs           # All undo/redo tests
└── structure_designer.rs      # Add: mod undo_test;
```

All undo tests go in a single file since they share the same helpers (`snapshot_all_networks`, `assert_undo_redo_roundtrip`, `setup_designer_with_network`) and test a single subsystem. As the file grows, it can be split into `undo/single_command_test.rs`, `undo/sequence_test.rs`, etc.

### What Is NOT Tested Here

- **Evaluation correctness after undo/redo** — The evaluator and caching system are tested separately. Undo tests only verify structural state (nodes, wires, data). The refresh mode mechanism ensures the evaluator is triggered, but correctness of re-evaluation is out of scope.
- **Flutter UI integration** — Keyboard shortcuts, menu state, and visual refresh are tested manually or via Flutter integration tests.
- **atom_edit node data** — Deferred (see "Nodes Requiring Special Handling"). Generic `SetNodeData` snapshots are tested, but the future specialized atom_edit undo commands will need their own test suite.

## File Organization

```
rust/src/structure_designer/
├── undo/
│   ├── mod.rs              # UndoStack, UndoCommand trait, UndoContext, UndoRefreshMode
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── add_node.rs
│   │   ├── delete_nodes.rs
│   │   ├── delete_wires.rs
│   │   ├── connect_wire.rs
│   │   ├── set_node_data.rs
│   │   ├── move_nodes.rs
│   │   ├── set_return_node.rs
│   │   ├── set_node_display.rs
│   │   ├── duplicate_node.rs
│   │   ├── paste_nodes.rs
│   │   ├── text_edit_network.rs
│   │   ├── add_network.rs
│   │   ├── delete_network.rs
│   │   ├── rename_network.rs
│   │   └── factor_selection.rs
│   └── snapshot.rs         # NodeSnapshot, WireSnapshot, helper serialization
```
