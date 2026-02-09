# Serialization - Agent Instructions

JSON-based persistence for `.cnnd` project files.

## Files

| File | Purpose |
|------|---------|
| `node_networks_serialization.rs` | Save/load entire projects (.cnnd files) |
| `edit_atom_data_serialization.rs` | Save/load EditAtom node command history |

## .cnnd File Format

JSON with versioned schema (`SERIALIZATION_VERSION = 2`):
- Top-level: array of `SerializableNodeNetwork`
- Each network: name, node_type, nodes, return_node_id, camera_settings
- Each node: id, type_name, custom_name, position, arguments (wires), data
- Node data is polymorphic: `node_data_saver`/`node_data_loader` fns on `NodeType`

Key entry points:
- `save_node_networks_to_file(path, registry)` → writes .cnnd
- `load_node_networks_from_file(path)` → returns `HashMap<String, NodeNetwork>`

## Serialization Conventions

- `HashMap` → `Vec` conversion for deterministic JSON output
- `Node.custom_name` assigned during migration if missing (uses type name)
- Camera settings persisted per network (optional)
- Version field enables forward-compatible migrations

## EditAtom Data

`EditAtomData` has its own serialization for the command history:
- Commands serialized with type tag + JSON data
- Preserves undo/redo index for session continuity
- Command types: SelectCommand, AddAtomCommand, AddBondCommand, ReplaceCommand, TransformCommand, DeleteCommand
