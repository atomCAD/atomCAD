# .cnnd Migration v2 → v3 (Lattice-Space Refactoring)

## Purpose

Phase 8 of `doc/design_lattice_space_refactoring.md`: convert existing `.cnnd` files saved by pre-refactoring atomCAD (format version 2) so they load on the lattice-space-refactoring branch. Without this, every existing project file is unreadable after the branch merges to main.

## Scope

This document covers **only** the one-time upgrade from the format as it was before the refactoring (version 2) to the format after the refactoring (version 3). It does not attempt to generalise the migration framework. Future migrations can reuse the infrastructure this phase introduces, but are out of scope here.

## Strategy Summary

- **In-process**, at file load time. No separate offline converter tool.
- **Bump `SERIALIZATION_VERSION` from 2 to 3.**
- **Two-layer migration**:
  1. **JSON pre-pass** (new): runs before strict deserialisation when the file's `version` is less than 3. Operates on `serde_json::Value` to rewrite type-name strings, DataType strings, and to synthesise new nodes where the shape of the network itself has changed (the `atom_fill` split).
  2. **Serde tolerance** (existing pattern): `#[serde(default)]` on new fields, silent drop of removed fields (serde ignores unknown fields by default — `frame_transform` is free to drop this way).
- **Version 3 files never go back through the pre-pass.** The pre-pass is purely a historical up-converter.

### Why in-process, not an offline tool

- The file is already JSON; `serde_json::Value` manipulation in Rust is trivial.
- The rewrite rules depend on node-registry knowledge (which pins exist, which types are valid). A separate tool would have to duplicate that knowledge.
- The existing `rust/tests/fixtures/*_migration/` + integration-test pattern is the natural home for the new fixture-based tests. An external tool would bypass it.
- There is no fleet of users to batch-convert. This is a branch merge.

### Why a JSON pre-pass and not just serde attributes

Previous migrations (v1 → v2 multi-output, inline atom metadata) were all **field-level**: one struct gained a field, serde `#[serde(default, skip_serializing_if = ...)]` handled reading the old shape and writing the new. That pattern does not stretch to this refactoring because of one change: **`atom_fill` becomes two nodes.** A single serialised node must be replaced by a new `structure` source node plus a `materialize` node, with a new wire between them and an ID allocated for the synthesised node. Serde field-level compat cannot express node synthesis. All other changes (string renames, removed fields) *could* be done at the serde level but are better kept together with the synthesis in one auditable migration module.

## Version Dispatch

Load flow in `rust/src/structure_designer/serialization/node_networks_serialization.rs::load_node_networks_from_file`:

1. Read file into `String`.
2. Parse into `serde_json::Value` (not into the typed `SerializableNodeTypeRegistryNetworks` yet).
3. Read the top-level `version` field.
4. If `version > SERIALIZATION_VERSION` (i.e. > 3): error as today.
5. If `version < SERIALIZATION_VERSION`: call `migrate_v2_to_v3(&mut json_value)`. Bump the in-memory `version` field to 3.
6. Deserialise the (possibly migrated) `Value` into `SerializableNodeTypeRegistryNetworks` via `serde_json::from_value`.
7. Continue with the existing path.

The migration runs **only in memory**. The file on disk is not touched until the user saves, at which point the network is written in v3 form.

## Migration Module Layout

New module: `rust/src/structure_designer/serialization/migrate_v2_to_v3.rs`.

Top-level entry point:

```rust
pub fn migrate_v2_to_v3(root: &mut serde_json::Value) -> Result<(), MigrationError>;
```

Internal helpers, roughly one per class of change:

- `rename_data_type_strings(root)` — walks every place a DataType name appears as a JSON string and applies the rename table.
- `rename_node_type_strings(root)` — walks every `node_type_name` / per-NodeType `name` and applies the rename table.
- `synthesise_structure_for_atom_fill(network_json)` — per network, for each `atom_fill` node, insert a `structure` source node and a `materialize` node, rewire.
- `adapt_primitives_lattice_to_structure(network_json)` — per network, for each primitive node, if a `LatticeVecs` wire is connected to the old input, insert a `structure` adapter; if nothing is connected, do nothing.
- `drop_deleted_nodes(network_json)` — remove `atom_trans` nodes and disconnect their wires.

Each helper is independent and tested individually.

## Change Catalogue

### DataType string renames

Applied wherever a DataType is written as a string (the serialised `data_type` of parameters, output pins, and any embedded node-data JSON that stores a DataType by name):

| Old | New |
|---|---|
| `"Geometry"` | `"Blueprint"` |
| `"UnitCell"` | `"LatticeVecs"` |
| `"Atomic"` | `"HasAtoms"` |
| `"StructureBound"` | `"HasStructure"` |
| `"Unanchored"` | `"HasFreeLinOps"` |

The `"Atomic"` → `"HasAtoms"` rename reflects both a redefinition (concrete → abstract) and a textual rename of the DataType variant. `"StructureBound"` and `"Unanchored"` are listed for completeness although no pre-v3 file could contain them — they were only ever in-memory names on the refactoring branch before the final `Has*` spelling was chosen — and a serde-level string rewrite costs nothing. Concrete uses of `"Atomic"` in saved networks typically referred to `atom_fill` output; since that node is synthesised away (see below), downstream wires end up pointing at `materialize` whose declared output is `"Crystal"`. Because DataType on a wire is not stored per wire (wires are by ID, typing comes from the pin definition), the abstract/concrete reclassification resolves itself once node types and pin declarations are in their v3 form.

### Node type renames

Applied to `SerializableNode.node_type_name` and to the `name` field inside any `SerializableNodeType` embedded in a custom-network definition:

| Old | New |
|---|---|
| `unit_cell` | `lattice_vecs` |
| `atom_lmove` | `structure_move` |
| `atom_lrot` | `structure_rot` |
| `atom_move` | `free_move` |
| `atom_rot` | `free_rot` |
| `lattice_move` | (merged into `structure_move`, see below) |
| `lattice_rot` | (merged into `structure_rot`, see below) |
| `lattice_symop` | deleted — see policy below |

The `lattice_move`/`lattice_rot` → `structure_move`/`structure_rot` merge is a simple rename because the new nodes absorbed the old behaviour.

### Node synthesis: `atom_fill` → `structure` + `materialize`

The structurally non-trivial case. For each `atom_fill` node `N` in a network:

1. Allocate a new ID `S` for a synthesised `structure` node (take `next_node_id`, increment it in the JSON).
2. **Rewire motif / motif_offset inputs.** The old `atom_fill` had input pins for `motif` and `m_offset` (and possibly `lattice_vecs`). Move the wires on those pins to the corresponding input pins of `S` (the new `structure` node accepts `structure`, `lattice_vecs`, `motif`, `motif_offset` as per the design doc — all optional).
3. **Rename `N`** from `atom_fill` to `materialize`.
4. **Connect `S` → `N`.** The new `materialize` node takes a `Blueprint` input (which itself now carries structure info via its field). But wait — `materialize: Blueprint → Crystal` reads structure *from the blueprint input*, not a separate structure input. So the synthesised `structure` node is **not** wired into the `materialize` node directly; it is wired into the upstream primitive via the primitive adaptation step below. See "Interaction with primitive adaptation" below.
5. **Position** the synthesised node near `N` (e.g. `N.position + (-150, 0)` in the 2D layout coordinates) so the auto-layout on next load does not trip.

**Drop** any `atom_fill` inputs/fields that have no equivalent on `materialize` (`materialize` is parameterless per the design).

### Primitive input adaptation: `LatticeVecs` → `Structure`

Phase 5 of the refactoring replaced the `LatticeVecs` input pin on primitives (cuboid, sphere, extrude, ...) with a `Structure` input pin.

For each primitive node in a network:

- If the old `lattice_vecs` input pin had no wire: nothing to do. The new `structure` input defaults to diamond.
- If the old pin had a wire from some source node of output type `LatticeVecs`: insert a synthesised `structure` node between the source and the primitive. The `structure` node's `lattice_vecs` input takes the original wire; its output feeds the primitive's new `structure` input.

### Interaction between `atom_fill` split and primitive adaptation

The two synthesis passes must compose cleanly:

- If the pre-v3 file connected a `unit_cell` output into both a primitive *and* an `atom_fill` (the common case: one unit cell shared across a design), the migration synthesises a single `structure` node that absorbs the `unit_cell` (now `lattice_vecs`) output once; both the primitive and the `materialize`-output-feeding blueprint end up consuming this one `structure`. Deduplication is a nice-to-have; an acceptable first version creates a fresh `structure` per consumer and relies on evaluator semantics being identical.

### Removed field: `frame_transform`

Per Appendix B of the refactoring design, `frame_transform` was removed from `BlueprintData` (was `GeometrySummary`) and `AtomicStructure`. serde ignores unknown JSON fields on deserialisation, so old files carrying this field load without complaint and lose it on next save. No explicit migration action required, but the migration test fixture should cover a file that *has* this field to prove it is dropped silently.

### Deleted nodes

Policy per deleted node:

- **`atom_trans`** — deleted outright per phase 7. Migration drops the node and disconnects its input/output wires. Downstream wires that consumed `atom_trans` output are left unconnected (they become errors on network validation, which is the correct user-visible signal: "this old node was removed, please replace it").
- **`lattice_symop`** — same policy as `atom_trans`. If no downstream consumers exist, silently drop.

An alternative would be to hard-fail the load. I prefer the drop-with-dangling-wires policy: it lets the user open the project, see the damage, and fix it. Hard-failing makes a partial recovery impossible.

### Custom networks

`SerializableNodeNetwork.node_type` (a `SerializableNodeType`) embeds parameter and output-pin DataType strings. The rename table must be applied here too. Easy to forget: the DataType walker must descend into every network's `node_type` struct, not only into the flat node list.

## Error Policy

- **Unknown pre-v3 node types** (not in the rename table and not a still-valid node type): log a warning, keep the node in the JSON unchanged. It will be flagged as an invalid node type by network validation. This is lenient; the alternative (hard fail) makes the user's old projects unloadable over a single stale node.
- **Malformed JSON, corrupt structure**: propagate the error up. The migration function returns `Result`.
- **Version > 3**: rejected before migration runs, same behaviour as today.

## Testing

New fixture directory: `rust/tests/fixtures/lattice_space_migration/`.

Minimum fixtures (one tiny `.cnnd` file each, each focused on one change class):

1. `pure_rename.cnnd` — a network using `Geometry`, `UnitCell`, `unit_cell`, `atom_move` types and nothing structural. Exercises the rename tables.
2. `atom_fill_split.cnnd` — a minimal `unit_cell → atom_fill` pipeline. Verifies node synthesis.
3. `primitive_with_lattice.cnnd` — a `unit_cell → cuboid` wire. Verifies primitive adapter synthesis.
4. `shared_unit_cell.cnnd` — one `unit_cell` feeding both a primitive and an `atom_fill`. Verifies composition.
5. `frame_transform_present.cnnd` — a file whose `GeometrySummary`-shaped payload carries `frame_transform`. Verifies silent drop.
6. `atom_trans_present.cnnd` — a file containing `atom_trans`. Verifies drop-with-dangling-wires policy.
7. `custom_network.cnnd` — a custom network whose `node_type` has parameters and output pins typed `Geometry`/`UnitCell`. Verifies the walker reaches into custom-network type definitions.

New integration test crate: `rust/tests/integration/lattice_space_migration_test.rs`, registered in `rust/tests/integration.rs`.

Per-fixture test shape:

- Load the old fixture through `load_node_networks_from_file`.
- Assert the in-memory form matches expectations (node types present, wires connected, DataTypes resolved).
- Re-save to a temp file.
- Reload the re-saved file. Verify it now has `version: 3` and no longer triggers the migration path.
- Verify round-trip idempotence: second save produces byte-identical output to the first save.

Existing `cnnd_roundtrip_test.rs` should continue to pass without changes (it operates on files the new code saves, which are all v3).

## Non-goals

- **No migration from v1.** v1 files have been auto-migrated by the existing serde-level code since v2 landed. They continue to load (the existing serde compat stays in place) and are immediately bumped through both hops (v1 → v2 via serde defaults, v2 → v3 via this pre-pass) on first load.
- **No user-facing migration UI.** Loading is silent except for warnings emitted through the existing log mechanism.
- **No batch "upgrade all files" command-line tool.** If a user wants to pre-convert a directory, they can load and save each file through atomCAD; out of scope for this phase.
- **No preservation of deleted-node semantics via user-defined replacement nodes.** If the user relied on `lattice_symop`, they must rebuild that part of the network using the new nodes. The migration will not attempt to synthesise an equivalent subgraph.

## Implementation Order

1. Add `SERIALIZATION_VERSION = 3` constant. Adjust version check so v2 is accepted and routed through migration.
2. Create `migrate_v2_to_v3.rs` skeleton with the entry point and empty helpers.
3. Fill in `rename_data_type_strings` + `rename_node_type_strings`. Ship with fixtures 1 and 7.
4. Fill in `drop_deleted_nodes`. Ship with fixtures 6.
5. Fill in `adapt_primitives_lattice_to_structure`. Ship with fixture 3.
6. Fill in `synthesise_structure_for_atom_fill`. Ship with fixtures 2 and 4.
7. Regenerate any snapshot tests that now save at version 3.
8. Merge branch to `main`.

Each step leaves the tree compiling and tests green.

## Open Questions

- **Deduplication of synthesised `structure` nodes** when the same `unit_cell` output was shared across multiple consumers. Fresh-per-consumer is simpler and correct; dedup is a polish item. Decision point before step 6 above.
- **Warnings surfacing**: do migration warnings go to stderr, to a log, or to a UI dialog on first successful load? Existing conventions in the codebase (if any) should be followed.
- **Position of synthesised nodes**: a fixed offset from the node they feed is adequate. If visual clutter is a concern in practice, revisit after opening a few real fixtures.
