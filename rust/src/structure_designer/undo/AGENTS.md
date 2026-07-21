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
    ├── composite.rs                # Bundles N child commands into one undo step
    ├── set_node_data.rs, set_return_node.rs, set_node_display.rs
    ├── duplicate_node.rs, paste_nodes.rs
    ├── add_network.rs, delete_network.rs, rename_network.rs
    ├── text_edit_network.rs, factor_selection.rs
    ├── inline_node.rs             # Inline a custom node (top-level; whole-network snapshot, like text_edit)
    ├── convert_to_closure.rs      # Network→Closure (top-level; before/after whole-network snapshot)
    ├── extract_closure_body.rs    # Closure→Network inside a zone body (body snapshot + add/remove of N)
    ├── edit_zone_body.rs           # Body-scoped structural edits (whole-body snapshot)
    ├── set_zone_size.rs            # HOF body resize (begin/end coalesced)
    ├── set_collapse_mode.rs        # HOF body collapse mode
    ├── add_record_type_def.rs, delete_record_type_def.rs,
    │   rename_record_type_def.rs, update_record_type_def.rs  # Record type def lifecycle
    ├── atom_edit_mutation.rs      # Incremental diff deltas (includes flag changes)
    └── atom_edit_toggle_flag.rs   # Boolean flag toggles
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

### Body-Scoped Undo (zones / closures)

Edits *inside* an HOF body (`Node.zone`) are addressed by a `scope_path: Vec<u64>` (chain of HOF node ids, `[parent.., hof_id]`). Commands that touch a body carry that path and resolve through `UndoContext::network_in_scope_mut(name, scope_path)` (walks `Node::zone_mut` down the chain). `SetNodeDataCommand`, `SetCollapseModeCommand`, `SetZoneSizeCommand`, and `MoveNodesCommand` all carry a `scope_path`.

All body-scoped **structural** edits (add / delete / duplicate, and connect of every wire shape: intra-body, capture, zone-input, body-return) funnel through a single `EditZoneBodyCommand`: it stores a before/after `ZoneBodySnapshot` (the body `SerializableNodeNetwork` + the HOF's `zone_output_arguments` wires) and restores it wholesale. Body networks are small, and this covers every wire shape and nested bodies uniformly without per-operation surgery. Helpers `StructureDesigner::snapshot_zone_body` / `push_zone_body_command` capture before/after and push only if the body actually changed. Restore re-runs `initialize_custom_node_types_for_network` on the deserialized body so body-node caches are repopulated.

Moves are the exception — they use the lighter scope-aware `MoveNodesCommand` via `begin_move_nodes_scoped` / `end_move_nodes` coalescing (one command per drag). Body resize uses `SetZoneSizeCommand` via `begin_zone_resize` / `end_zone_resize` coalescing (Flutter calls these on the resize handle's pan start/end).

### Composite Commands & Reflow Bundling

`CompositeCommand { commands: Vec<Box<dyn UndoCommand>>, description }` (`commands/composite.rs`) bundles N children into **one** undo step: `undo` runs them in reverse, `redo` forward (standard composite order; `MoveNodesCommand` sets absolute positions so order is immaterial in practice). Its `refresh_mode()` is `combine_refresh_modes` (in `mod.rs`) — strongest child wins: any `Full` ⇒ `Full`; else union all `NodeDataChanged` id-lists; else `Lightweight`. **Never construct a 1-child composite** — push the bare child instead.

This is the mechanism behind **reflow-on-footprint-change** (`doc/design_reflow_on_footprint_change.md`): when an in-place footprint growth (HOF expand on `f`-disconnect / `set_collapse_mode` / in-body add·paste·duplicate·connect) pushes neighbour nodes, `StructureDesigner::reflow_for_footprint_change` returns the moved `(id, old, new)` per scope and the trigger bundles a `MoveNodesCommand` per scope alongside its primary command. **Rule: bundle a move command only for scopes the primary command's snapshot does NOT already cover.** A body-scoped `EditZoneBodyCommand` takes a *fresh after-snapshot at push time*, so moves *within that body* ride along for free — only **ancestor**-scope moves (the cascade climbing past the edited body) need explicit bundling. Helpers: `capture_footprint_chain` / `capture_body_owner_footprint_chain` (snapshot pre-edit sizes BEFORE mutating) and `push_zone_body_command_with_ancestor_reflow` (Case C — reflows starting one scope up at the body-owning HOF). `combine_refresh_modes` must promote to the strongest child so a deletion's `Full` is not downgraded to a move's `Lightweight`.

### Selection Is Not Undoable

Consistent with most CAD applications. Simplifies the system significantly.

### Suppression

`UndoStack::suppress_recording` / `resume_recording` — prevents commands from being recorded at all (distinct from bundling several into one step, which is `CompositeCommand`'s job). `SetNodeData` is suppressed for `edit_atom` (deprecated) and `atom_edit` (has its own incremental commands).

## Known Pitfalls

- **Display state**: `add_node_with_id` always adds to `displayed_nodes`. If the original node wasn't displayed (e.g., from `duplicate_node`), explicitly remove after re-add on redo. Undo commands store `Vec<(u64, NodeDisplayType)>` and wrap in `NodeDisplayState::with_type()`.
- **Per-pin display**: `SetOutputPinDisplayCommand` stores full `Option<NodeDisplayState>` for old/new state (atomic undo — removing the last pin drops the node from `displayed_nodes` entirely). `displayed_output_pins` is included in `SerializableNodeNetwork` snapshots automatically. Like `SetNodeDisplayCommand` it carries a `scope_path` and resolves through `network_in_scope_mut`, so per-pin toggles inside a 0-ary closure body are undoable; a **scoped** display command reports `UndoRefreshMode::Full` (neither `Lightweight` nor the bare-`u64` `NodeDataChanged` can express adding/removing a *scoped* scene entry), top-level stays `Lightweight`. See `doc/design_zero_ary_closure_body_display.md` §5.
- **next_node_id / next_param_id**: Must be saved/restored on undo. JSON snapshot comparison includes these fields.
- **HashMap ordering**: `displayed_nodes` and `nodes` are HashMaps; test snapshot comparisons use `normalize_json()` to sort `displayed_node_ids`, `displayed_output_pins`, and `nodes` arrays.
- **validate_active_network**: Must be called after Full refresh undo/redo to update derived state like `output_type`.
