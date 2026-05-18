# Serialization - Agent Instructions

JSON-based persistence for `.cnnd` project files.

## Files

| File | Purpose |
|------|---------|
| `node_networks_serialization.rs` | Save/load entire projects (.cnnd files); chained version dispatch |
| `migrate_v2_to_v3.rs` | One-shot JSON pre-pass for v2 files (atom_fill split, etc.) |
| `migrate_v3_to_v4.rs` | One-shot JSON pre-pass for v3 files: insert `collect` between iterator producers (`range`/`map`/`filter`/`product` and transitively-iterator custom networks) and `Array[T]`-typed consumers |
| `atom_edit_data_serialization.rs` | Save/load atom_edit node diff data (inline flags + backward-compat migration) |
| `edit_atom_data_serialization.rs` | Save/load EditAtom node command history (legacy) |

## .cnnd File Format

JSON with versioned schema (`SERIALIZATION_VERSION = 4`):
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

## Version Migrations (chained dispatch)

`load_node_networks_from_file` runs a chained sequence of one-way JSON pre-passes against `serde_json::Value` *before* strict deserialization, then bumps the in-memory version to `SERIALIZATION_VERSION`:

```text
if version < 3 { migrate_v2_to_v3(&mut root_value)?; }
if version < 4 { migrate_v3_to_v4(&mut root_value)?; }
```

A v2 file chains both passes; a v3 file runs only v3→v4; a v4 file runs neither. Migrations are pre-deserialization because they synthesize new nodes (atom_fill split, `collect` insertion) — serde-level field defaults can't express that. Each migration is **frozen at its release version** (constants like `migrate_v3_to_v4::ITERATOR_PINS_V4` are hardcoded, not read from the live `NodeTypeRegistry`) so future registry changes don't retroactively alter how an old file gets up-converted. ID and position allocation is deterministic (read-only pre-pass + sorted mutation pass) for byte-identical re-runs and idempotent double-migration. Design doc: `doc/design_iterators.md` §"Backward compatibility" (and `doc/design_cnnd_migration_v2_to_v3.md` for the older pass).

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

## Zone (HOF body) Serialization

The four HOF node types (`map`, `filter`, `fold`, `foreach`) carry an inline body. Two sets of fields capture it:

- **`SerializableNodeType.zone_input_pins` / `zone_output_pins`** — empty on non-HOF node types; for HOFs, the inside-facing source/destination pin definitions. Frozen at SERIALIZATION_VERSION = 4.
- **`SerializableNode.zone: Option<SerializableNodeNetwork>`** — `Some(body)` for HOF nodes that have an inline body, `None` for non-HOF nodes. Uses `#[serde(default)]` so pre-zones `.cnnd` fixtures continue to deserialize (HOFs there have `zone: None` and validation_errors will flag the missing zone-output wire on load).
- **`SerializableNode.zone_output_arguments: Vec<Argument>`** — wires terminating at the HOF's zone-output (inside-right) pins, one `Argument` per declared zone-output pin. Always empty for non-HOF nodes. `#[serde(default)]`.
- **`SerializableNode.body_width` / `body_height: f64`** — stored body dimensions in logical pixels. Default 320×180 via `default_body_width`/`default_body_height`. Meaningful only when `zone.is_some()`; the renderer uses `max(stored, content_bbox + padding)` so this is the *minimum* size, never the rendered one.

Wire scope semantics (`IncomingWire.source_scope_depth`, `source_pin: SourcePin::NodeOutput | ZoneInput`) are part of the wire serialization shape — see `node_network.rs`. The `Argument` type used by `zone_output_arguments` is the same one used by `arguments`, so wires inside a body that terminate on its containing HOF's zone-output pins serialize identically to ordinary wires (just with a different storage list).

No version bump for zones: the new fields are all `#[serde(default)]`, and pre-zones networks load with `zone: None`, `zone_output_arguments: vec![]`, default body sizes. Validation flags the resulting all-HOFs-missing-bodies state on load — the user fixes individual HOFs interactively (or `.cnnd` migration deferred per `design_zones.md`).

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
