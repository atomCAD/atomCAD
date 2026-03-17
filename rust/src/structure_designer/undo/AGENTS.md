# Undo/Redo System - Agent Instructions

Global undo/redo using the command pattern. Design docs: `doc/design_global_undo_redo.md` (global) and `doc/design_atom_edit_undo.md` (atom_edit node).

## Module Structure

```
undo/
├── mod.rs              # UndoStack, UndoCommand trait, UndoContext, UndoRefreshMode
├── snapshot.rs         # NodeSnapshot, WireSnapshot, ArgumentSnapshot, PendingMove
└── commands/
    ├── mod.rs
    ├── add_node.rs, delete_nodes.rs, delete_wires.rs
    ├── connect_wire.rs, move_nodes.rs
    ├── set_node_data.rs, set_return_node.rs, set_node_display.rs
    ├── duplicate_node.rs, paste_nodes.rs
    ├── add_network.rs, delete_network.rs, rename_network.rs
    ├── text_edit_network.rs, factor_selection.rs
    ├── atom_edit_mutation.rs      # Incremental diff deltas
    ├── atom_edit_toggle_flag.rs   # Boolean flag toggles
    └── atom_edit_frozen_change.rs # Freeze/unfreeze operations
```

## Architecture

- **UndoStack** lives on `StructureDesigner`. Single global stack (not per-network).
- **UndoCommand** trait: `description()`, `undo(&self, ctx)`, `redo(&self, ctx)`, `refresh_mode()`.
- **UndoContext** provides `&mut NodeTypeRegistry` + `&mut Option<String>` (active network name) to avoid borrow conflicts with `StructureDesigner` which owns the `UndoStack`.
- Commands store their target `network_name` and look up the network via `ctx.network_mut(name)`.
- `StructureDesigner::undo()`/`redo()` use `std::mem::take` to temporarily move the stack, avoiding simultaneous borrow of stack and context.

## Adding a New Command

1. Create `commands/my_command.rs` implementing `UndoCommand`
2. Add `pub mod my_command;` in `commands/mod.rs`
3. In the StructureDesigner mutation method:
   - Capture before-state
   - Perform the mutation (existing code)
   - Push the command via `self.push_command(MyCommand { ... })`
4. Add tests in `rust/tests/structure_designer/undo_test.rs`

## Key Patterns

### Command Creation in StructureDesigner

Commands are created inside `StructureDesigner` methods (not the API layer), because StructureDesigner owns the mutation logic and has access to the before-state. Exception: `TextEditNetworkCommand` is created in the API layer because text editing logic lives there.

### Refresh After Undo/Redo

`UndoRefreshMode` controls post-undo/redo evaluation:
- `Lightweight` — UI-only (e.g., move nodes)
- `NodeDataChanged(Vec<u64>)` — re-evaluate specific nodes
- `Full` — re-evaluate entire network. Also calls `apply_node_display_policy(None)` and `validate_active_network()` to update derived state.

### Move Coalescing

Node drags use `begin_move_nodes()`/`end_move_nodes()` to coalesce many `move_selected_nodes()` calls into a single `MoveNodesCommand`. The `PendingMove` struct captures start positions.

### Node ID Stability

`NodeNetwork::add_node_with_id()` allows redo to recreate nodes with the same ID. Commands that add nodes must save/restore `next_node_id` on undo.

### Selection Is Not Undoable

Consistent with most CAD applications. Simplifies the system significantly.

### Suppression

`UndoStack::suppress_recording` / `resume_recording` — safety valve for future compound operations. `SetNodeData` is suppressed for `edit_atom` (deprecated) and `atom_edit` (has its own incremental commands).

## Known Pitfalls

- **Display state**: `add_node_with_id` always adds to `displayed_node_ids`. If the original node wasn't displayed (e.g., from `duplicate_node`), explicitly remove after re-add on redo.
- **next_node_id / next_param_id**: Must be saved/restored on undo. JSON snapshot comparison includes these fields.
- **HashMap ordering**: `displayed_node_ids` and `nodes` are HashMaps; test snapshot comparisons use `normalize_json()` to sort arrays.
- **validate_active_network**: Must be called after Full refresh undo/redo to update derived state like `output_type`.
