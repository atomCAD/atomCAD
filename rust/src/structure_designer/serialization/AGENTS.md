# Serialization - Agent Instructions

JSON-based persistence for `.cnnd` project files.

## Files

| File | Purpose |
|------|---------|
| `node_networks_serialization.rs` | Save/load entire projects (.cnnd files); chained version dispatch |
| `migrate_v2_to_v3.rs` | One-shot JSON pre-pass for v2 files (atom_fill split, etc.) |
| `migrate_v3_to_v4.rs` | One-shot JSON pre-pass for v3 files: insert `collect` between iterator producers (`range`/`map`/`filter`/`product` and transitively-iterator custom networks) and `Array[T]`-typed consumers |
| `migrate_v5_to_v6.rs` | One-shot JSON pre-pass for v5 files: `free_rot` angle radians → degrees (issue #384). Renames `data.angle` → `data.angle_degrees` (value `to_degrees`) and splices a synthesised `degrees(x)` expr node onto any wired angle pin. Recurses into every zone body (keyed on the `zone` field, not HOF names). No v4→v5 pass exists |
| `atom_edit_data_serialization.rs` | Save/load atom_edit node diff data (inline flags + backward-compat migration) |
| `edit_atom_data_serialization.rs` | Save/load EditAtom node command history (legacy) |

## .cnnd File Format

JSON with versioned schema (`SERIALIZATION_VERSION = 6`):
- Top-level: array of `SerializableNodeNetwork` plus `record_type_defs` (record schemas)
- Each network: name, node_type, nodes, return_node_id, camera_settings
- Each node: id, type_name, custom_name, position, arguments (wires), data
- Node data is polymorphic: `node_data_saver`/`node_data_loader` fns on `NodeType`

Key entry points:
- `save_node_networks_to_file(path, registry)` → writes .cnnd
- `load_node_networks_from_file(path)` → returns `HashMap<String, NodeNetwork>`

## Load pipeline & derived state (read before touching the load path)

Loading runs in **two stages with different network orderings**, and the gap between them is a recurring source of "wires silently disappear on load" bugs.

1. **`load_node_networks_from_file` — per network, in FILE order.** Deserialize (the custom `Argument` deserializer rebuilds each node's `arguments` straight from JSON) → `canonicalize_network` → `initialize_custom_node_types_for_network` → `repair_node_network` → insert into the registry.
2. **`StructureDesigner::load_node_networks` — all networks, in DEPENDENCY order** (`get_networks_in_dependency_order`, dependencies first), calling `validate_network` on each.

**The core asymmetry.** A node's *wires* are authoritative serialized data (positional, in `arguments`). A node's *pin layout* (`custom_node_type`) is **not** serialized — for most nodes it's reconstructed from per-node data (`calculate_custom_node_type`), but for **`apply` the layout is derived from the type of whatever is wired into `f`** (the source's canonical-flat function arity ⇒ `[f, arg0, …, argN-1]`). That source frequently lives in **another network**, which — because stage 1 runs in file order — may not be loaded yet. So `apply`'s real shape often **cannot** be derived during stage 1; only the dependency-ordered stage 2 can complete it.

**The invariant:** *no operation that runs before the layout is derived may destroy the positional wire data.* The two ways a wire gets dropped are both layout-shape rebuilds against an under-derived (`[f]`) layout:
- a **by-name `arguments` rebuild** (`set_custom_node_type(.., refresh_args = true)`) — has no name for the `arg0` slot the wire sits at, so it drops it;
- a **truncation** (`network_validator::repair_network_arguments`) — cuts `arguments` down to the bare `[f]` count.

Stage 1 stays non-destructive for `apply`: `initialize_…` uses `refresh_args = false`; `repair_node_network`'s generic populate special-cases `apply` to `refresh_args = false` and then runs the apply post-pass with the **preserving-args** variant; its argument-count fixer only *pads*, never truncates. So stage 1 leaves `apply` with an under-derived `[f]` layout but its `arguments` (incl. the unresolved `arg0` wire) intact. Stage 2's `validate_network` then runs the apply/map post-passes (preserving variants) **before** `repair_network_arguments`, so once the `f`-source is resolvable (dependency order) the real `[f, arg0, …]` layout is installed *with the wires preserved positionally*, and the now-no-op truncation/`validate_wires` follow. The `f` wire itself (index 0, and a `-1` source pin) is never at risk; only the derived `arg0…` pins are. See `structure_designer/AGENTS.md` (apply post-pass paragraph) and `doc/design_currying.md`.

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
// (no v4→v5 transform; see below)
if version < 6 { migrate_v5_to_v6(&mut root_value)?; }
if version < SERIALIZATION_VERSION { /* bump in-memory version field to 6 */ }
```

A v2 file chains all three passes; a v3 file runs v3→v4 then v5→v6; a v5 file runs only v5→v6; a v6 file runs none. Migrations are pre-deserialization because they synthesize new nodes (atom_fill split, `collect` insertion, `degrees(x)` insertion) — serde-level field defaults can't express that. Each migration is **frozen at its release version** (constants like `migrate_v3_to_v4::ITERATOR_PINS_V4` are hardcoded, not read from the live `NodeTypeRegistry`) so future registry changes don't retroactively alter how an old file gets up-converted. ID and position allocation is deterministic (read-only pre-pass + sorted mutation pass) for byte-identical re-runs and idempotent double-migration. `migrate_v5_to_v6` keys idempotency on the radian-era `data.angle` field (gone after the rename) and recurses into zone bodies via the node's `zone` field — **not** an HOF-name list, so `closure` / `zip_with` bodies are covered.

**No v4→v5 transform pass.** v4 and v5 are structurally identical, so v4 (and v3-chained-to-v4) files have their in-memory version field bumped with no v4→v5 rewrite. A `migrate_v4_to_v5` pass briefly existed on the `zones` branch — it rewrote main's legacy function-pin idiom (a node's `-1` pin feeding an HOF `f` pin, with some inputs wired as *captures* under main's parameters-first/captures-last convention) into a synthesized `closure` node. It was deleted: the function-pin synthesizer (`build_node_function_closure`) now reproduces the capture/parameter partition at **evaluation time**, so those files load and evaluate directly — the wire-storage shape conversion is handled by the custom `Argument` deserializer and everything else (zones, `body_width`/`body_height`, `collapse_mode`) by `#[serde(default)]`. Load-and-evaluate regressions for the legacy idiom live in `tests/structure_designer/zones_migration_test.rs` (fixtures still under `tests/fixtures/zones_migration/`). Design docs: `doc/design_iterators.md` §"Backward compatibility" (v3→v4), `doc/design_cnnd_migration_v2_to_v3.md` (v2→v3), `doc/design_node_function_pin_captures.md` (the v4→v5 removal).

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
- **`SerializableNode.collapse_mode: CollapseMode`** — the user's HOF body collapse choice (`Auto`/`Collapsed`/`Expanded`). `#[serde(default)]` + `#[derive(Default)]` (`Auto`) so older files load as `Auto` (compact iff `f` wired, expanded otherwise — no migration). Inert on non-HOF nodes. See `doc/design_hof_node_collapse.md`.
- **`SerializableNode.function_pin_roles: BTreeMap<usize, FunctionPinRole>`** — per-input-pin overrides of the `-1` function pin's parameter/capture partition, keyed by pin index (`doc/design_function_pin_roles.md`). `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`: additive + defaulted ⇒ old files load unchanged, and a node without overrides serializes **byte-identically** to the pre-roles format — which is what keeps the `.cnnd` fixtures and node snapshots green. No migration, no version bump. `BTreeMap` for deterministic key order. On load, `prune_auto_function_pin_roles` drops any hand-authored explicit `Auto` entry, re-establishing the in-memory invariant that `Auto` is represented by *absence*; `repair_network_arguments` additionally prunes entries whose pin index no longer exists.

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
