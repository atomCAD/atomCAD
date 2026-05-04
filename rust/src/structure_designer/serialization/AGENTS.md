# Serialization - Agent Instructions

JSON-based persistence for `.cnnd` project files.

## Files

| File | Purpose |
|------|---------|
| `node_networks_serialization.rs` | Save/load entire projects (.cnnd files) |
| `atom_edit_data_serialization.rs` | Save/load atom_edit node diff data (inline flags + backward-compat migration) |
| `edit_atom_data_serialization.rs` | Save/load EditAtom node command history (legacy) |

## .cnnd File Format

JSON with versioned schema (`SERIALIZATION_VERSION = 2`):
- Top-level: array of `SerializableNodeNetwork` plus `record_type_defs` (record schemas)
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

## Record Type Defs

- **`record_type_defs`** (project root) — array of `{ name, fields: [{name, type}, ...] }`, fields preserved in **authored order**. Uses `#[serde(default)]`, so pre-record `.cnnd` files load with an empty registry — purely additive, no version bump, no migration code. On save, entries are emitted sorted by name for deterministic output despite `HashMap` iteration order.
- **`DataType::Record`** serializes as a `RecordType` enum: `{"Named": "Point"}` for registry references (no schema duplication — the schema lives in `record_type_defs`) and `{"Anonymous": [...fields...]}` for inline schemas (e.g. `expr` literal types).
- **`record_construct.schema` / `record_destructure.schema` / `product.target`** are bare-string node properties holding the def name, not embedded `RecordType` values.
- **On-load validation:** re-runs the cycle check on the registry and flags any `Named(N)` whose `N` is missing — defensive against hand-edited files.

## Multi-Output Pin Serialization

- **`SerializableNodeType.output_pins: Vec<SerializableOutputPin>`** — always written on save. Old `output_type: Option<String>` is read-only for migration (single type → `output_pins[0]`).
- **`SerializableNodeNetwork.displayed_output_pins: Vec<(u64, Vec<i32>)>`** — per-node pin display state. Omitted if empty (backward compat). On load, merged with `displayed_node_ids` into the unified `displayed_nodes: HashMap<u64, NodeDisplayState>`. Default is `{0}` (pin 0 only).
- **`displayed_node_ids`** is always written (backward compat with old readers). On save, split from `displayed_nodes`.
- **atom_edit `output_diff` migration:** On load, `output_diff: true` → `displayed_pins: {1}`. No longer written on save.

## atom_edit Data (`atom_edit_data_serialization.rs`)

Serializes `AtomEditData` for the `atom_edit` node (non-destructive diff-based editor):
- **`SerializableAtom`** includes `flags: u16` — per-atom metadata (frozen, hybridization, H passivation) stored inline. Selection bit stripped on save.
- **Inline flags** are the canonical format. Old external map fields (`frozen_base_atoms`, `frozen_diff_atoms`, `hybridization_override_base_atoms`, `hybridization_override_diff_atoms`) are kept on `SerializableAtomEditData` for backward-compat deserialization but are always written empty on save (skipped via `skip_serializing_if`).
- **Backward-compat migration:** On load, if old map fields are present, diff-provenance entries are applied to diff atom flags. Base-provenance entries are ignored (promotion requires the base structure, unavailable at load time).
- Tests: `rust/tests/integration/inline_metadata_migration_test.rs`

## EditAtom Data (Legacy)

`EditAtomData` has its own serialization for the command history:
- Commands serialized with type tag + JSON data
- Preserves undo/redo index for session continuity
- Command types: SelectCommand, AddAtomCommand, AddBondCommand, ReplaceCommand, TransformCommand, DeleteCommand
