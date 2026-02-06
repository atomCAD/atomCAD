# Copy & Paste Design for atomCAD Node Networks

## Overview

Implement copy-paste for selected nodes in the atomCAD node network editor.
The clipboard is represented as a `NodeNetwork`, and both copy and paste are
the same underlying operation: copying an island of nodes from one network
into another.

## Core Concept: Clipboard as NodeNetwork

The clipboard is stored as an `Option<NodeNetwork>` on `StructureDesigner`.
When the user copies a selection, the selected nodes are extracted into a
fresh `NodeNetwork` (the clipboard). When the user pastes, nodes are copied
from the clipboard network into the active network. Both directions use the
same method.

**Key simplification — the "island" rule**: external connections (wires to/from
nodes outside the selection) are always dropped on copy. The clipboard is a
self-contained, isolated network. This means:

- No dangling references to nodes that don't exist
- Cross-network paste works cleanly (copy in network A, paste in network B)
- No need to track original node IDs beyond the copy operation

## Data Structure Changes

### StructureDesigner (`rust/src/structure_designer/structure_designer.rs`)

Add one field:

```rust
pub struct StructureDesigner {
    // ... existing fields ...
    pub clipboard: Option<NodeNetwork>,
}
```

Initialize to `None` in `new()`.

### NodeNetwork — new constructor

`NodeNetwork::new()` requires a `NodeType` (with function pointers). For the
clipboard this is meaningless. Add a minimal constructor:

```rust
impl NodeNetwork {
    /// Creates an empty NodeNetwork with a placeholder node type.
    /// Used for clipboard and other transient networks.
    pub fn new_empty() -> Self {
        use crate::structure_designer::node_type::{NodeType, no_data_saver, no_data_loader};
        use crate::structure_designer::node_data::NoData;
        use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
        use crate::structure_designer::data_type::DataType;

        let placeholder_type = NodeType {
            name: String::new(),
            description: String::new(),
            summary: None,
            category: NodeTypeCategory::General,
            parameters: vec![],
            output_type: DataType::None,
            public: false,
            node_data_creator: || Box::new(NoData {}),
            node_data_saver: no_data_saver,
            node_data_loader: no_data_loader,
        };
        Self::new(placeholder_type)
    }
}
```

## The Single Core Operation

Add one method to `NodeNetwork` in `rust/src/structure_designer/node_network.rs`:

```rust
/// Copies nodes from another network into this network.
///
/// Internal connections between copied nodes are preserved (with remapped IDs).
/// External connections (to nodes not in source_node_ids) are dropped.
/// Each pasted node gets a fresh ID, a unique display name, and is set to displayed.
///
/// Returns the list of newly created node IDs.
pub fn copy_nodes_from(
    &mut self,
    source: &NodeNetwork,
    source_node_ids: &HashSet<u64>,
    position_offset: DVec2,
) -> Vec<u64>
```

### Implementation of `copy_nodes_from`

This method has two internal steps (both inside the same function).
Step 2 must run after step 1 because we need the complete `old_id -> new_id`
mapping before we can remap wires.

**Step 1 — Create all nodes:**

For each node ID in `source_node_ids`:
1. Look up the node in `source.nodes`. Skip if not found.
2. Allocate a new ID from `self.next_node_id` (increment it).
3. Record the mapping `old_id -> new_id` in a local `HashMap<u64, u64>`.
4. Clone node data via `node.data.clone_box()`.
5. Clone `arguments`, `custom_node_type`, `node_type_name`.
6. Compute position: `source_node.position + position_offset`.
7. Generate a display name: `self.generate_unique_display_name(&node_type_name)`.
8. Insert the new `Node` into `self.nodes`.
9. Call `self.set_node_display(new_id, true)`.
10. Collect the new ID into the result vec.

**Step 2 — Remap arguments (runs after all nodes are created):**

For each newly created node:
- For each `Argument` in `node.arguments`:
  - Build a new `argument_output_pins` HashMap:
    - If the source node ID is in the `old_to_new` map (internal wire) ->
      replace with the new ID, keep the pin index.
    - Otherwise (external wire) -> drop it.
  - Replace `arg.argument_output_pins` with the remapped version.

Return the vec of new node IDs.

## Copy Flow

When the user triggers copy (Ctrl+C or context menu):

1. Get the active network. Get its `selected_node_ids`. If empty, return false.
2. Compute the **centroid** of selected nodes' positions:
   `centroid = sum(positions) / count`.
3. Create a fresh clipboard: `NodeNetwork::new_empty()`.
4. Call `clipboard.copy_nodes_from(active_network, &selected_ids, -centroid)`.
   This centers the clipboard nodes around (0, 0).
5. Store the clipboard: `self.clipboard = Some(clipboard)`.
6. Return true.

## Paste Flow

When the user triggers paste (Ctrl+V or context menu):

1. Check `self.clipboard` is `Some`. If `None`, return empty vec.
2. Get the active network (mutable).
3. Collect all node IDs from the clipboard: `clipboard.nodes.keys().collect()`.
4. Call `active_network.copy_nodes_from(&clipboard, &all_clipboard_ids, paste_position)`.
   Since clipboard nodes are centered at (0, 0), this places them at the cursor.
5. Select the pasted nodes: call `active_network.select_nodes(new_ids)`.
6. Trigger refresh (mark dirty, re-evaluate, update display).
7. Return the new node IDs.

## Cut Flow

Cut = Copy + Delete:
1. Call `copy_selection()`.
2. If successful, call `delete_selected()` on the active network.

## Clipboard Invalidation

The clipboard must stay consistent when custom node types change. Strategy:
**clear the clipboard when it becomes invalid** (simple, minimal code).

### On network rename (`rename_node_network` in `structure_designer.rs`)

Update `node_type_name` for clipboard nodes, same as for all other networks.
Add to the existing loop that updates node_type_names:

```rust
// After the existing loop over node_type_registry.node_networks:
if let Some(ref mut clipboard) = self.clipboard {
    for (_, node) in &mut clipboard.nodes {
        if node.node_type_name == old_name {
            node.node_type_name = new_name.to_string();
        }
    }
}
```

The clipboard survives renames.

### On network delete (`delete_node_network` in `structure_designer.rs`)

Do NOT block deletion because of the clipboard. Instead, after successful
deletion, check if the clipboard references the deleted type and clear it:

```rust
// After removing the network from the registry:
if let Some(ref clipboard) = self.clipboard {
    if clipboard.nodes.values().any(|n| n.node_type_name == network_name) {
        self.clipboard = None;
    }
}
```

### On parameter interface change (`validate_active_network_with_initial_errors` in `structure_designer.rs`)

When `interface_changed == true` for a network, check if the clipboard
contains nodes of that type. If so, clear the clipboard:

```rust
// After the validation cascade, when interface_changed is true for network_name:
if let Some(ref clipboard) = self.clipboard {
    if clipboard.nodes.values().any(|n| n.node_type_name == changed_network_name) {
        self.clipboard = None;
    }
}
```

## API Layer

Add to `rust/src/api/structure_designer/structure_designer_api.rs`:

### `copy_selection() -> bool`

Sync FFI function. Calls the copy logic on StructureDesigner.
Returns true if something was copied, false if selection was empty.

### `paste_at_position(x: f64, y: f64) -> Vec<u64>`

Sync FFI function. Calls the paste logic on StructureDesigner.
Returns the list of new node IDs (empty if clipboard was empty).
Calls `refresh_structure_designer_auto()` after pasting.

### `cut_selection() -> bool`

Sync FFI function. Calls copy, then delete_selected if copy succeeded.
Returns true if something was cut.
Calls `refresh_structure_designer_auto()` after cutting.

### `has_clipboard_content() -> bool`

Sync FFI function. Returns `self.clipboard.is_some()`.
Used by Flutter to decide whether to show the "Paste" option in menus.

## Flutter UI Changes

### Keyboard shortcuts (`lib/structure_designer/node_network/node_network.dart`)

Add alongside the existing Ctrl+D handler:

- **Ctrl+C**: Call `structureDesignerApi.copySelection()`.
- **Ctrl+V**: Call `structureDesignerApi.pasteAtPosition(cursorX, cursorY)`
  where cursor position is the current mouse position in network coordinates.
  After paste, call `refreshFromKernel()`.
- **Ctrl+X**: Call `structureDesignerApi.cutSelection()`.
  After cut, call `refreshFromKernel()`.

### Context menu — node right-click (`lib/structure_designer/node_network/node_widget.dart`)

In the existing `_handleContextMenu()`, add a **"Copy"** menu item alongside
"Duplicate". It should be available whenever any nodes are selected. The copy
action calls `structureDesignerApi.copySelection()`.

### Context menu — canvas right-click

Currently, right-clicking on empty canvas directly opens the **Add Node dialog**.
This behavior needs to change when the clipboard has content:

- If `structureDesignerApi.hasClipboardContent()` is **false**: open the
  Add Node dialog directly (unchanged behavior).
- If `structureDesignerApi.hasClipboardContent()` is **true**: show an
  intermediary context menu with two options:
  - **"Add Node"** — opens the Add Node dialog (same as before).
  - **"Paste"** — pastes clipboard content at the click position.

### Paste position

The paste position should be in **network coordinates** (accounting for pan
and zoom). The Flutter side needs to convert the screen-space mouse position
to network coordinates before passing to `paste_at_position()`.

## Edge Cases

| Case | Behavior |
|------|----------|
| Return node in selection | Pasted nodes are NOT set as return node |
| Empty selection | Copy returns false, no clipboard change |
| Empty clipboard | Paste returns empty vec, no-op |
| Repeated paste | Each paste clones from clipboard with fresh IDs/names |
| Paste into different network | Works. Internal wires preserved, no external wires |
| Custom node type in selection | `custom_node_type` is cloned via `NodeType::clone()` |
| Selected wires only (no nodes) | Copy ignores wire-only selection |
| Node data state | Cloned via `clone_box()` at copy time, frozen in clipboard |

## Files to Modify

### Rust

| File | Changes |
|------|---------|
| `rust/src/structure_designer/node_network.rs` | Add `NodeNetwork::new_empty()`, add `NodeNetwork::copy_nodes_from()` |
| `rust/src/structure_designer/structure_designer.rs` | Add `clipboard: Option<NodeNetwork>` field, add `copy_selection()` / `paste_at_position()` / `cut_selection()` methods, add clipboard invalidation in `rename_node_network()`, `delete_node_network()`, `validate_active_network_with_initial_errors()` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `copy_selection()`, `paste_at_position()`, `cut_selection()`, `has_clipboard_content()` API functions |

### Dart / Flutter

| File | Changes |
|------|---------|
| `lib/structure_designer/node_network/node_network.dart` | Add Ctrl+C, Ctrl+V, Ctrl+X keyboard handlers |
| `lib/structure_designer/node_network/node_widget.dart` | Add "Copy" to node context menu |
| `lib/structure_designer/structure_designer_model.dart` | Add `copySelection()`, `pasteAtPosition()`, `cutSelection()`, `hasClipboardContent()` model methods |
| Add-node panel (wherever the add-node popup is implemented) | Add "Paste" option when clipboard has content |

After adding the Rust API functions, run `flutter_rust_bridge_codegen generate`
to regenerate FFI bindings.

## Implementation Order

### Phase 1 — Rust backend

1. `NodeNetwork::new_empty()` and `NodeNetwork::copy_nodes_from()` — the core logic
2. `StructureDesigner` clipboard field + `copy_selection()` / `paste_at_position()` / `cut_selection()`
3. Clipboard invalidation in `rename_node_network()`, `delete_node_network()`, `validate_active_network_with_initial_errors()`
4. API layer functions: `copy_selection()`, `paste_at_position()`, `cut_selection()`, `has_clipboard_content()`
5. Run `flutter_rust_bridge_codegen generate`

### Phase 2 — Flutter frontend

6. Flutter model methods in `structure_designer_model.dart`
7. Keyboard shortcuts (Ctrl+C / Ctrl+V / Ctrl+X) in `node_network.dart`
8. Context menu items (Copy on node right-click, Paste on canvas right-click)

### Testing

9. Test: copy single node, paste, verify data and position
10. Test: copy multiple connected nodes, paste, verify internal wires preserved
11. Test: copy, rename used type, paste — verify clipboard survives
12. Test: copy, delete used type — verify clipboard cleared
13. Test: cross-network paste
