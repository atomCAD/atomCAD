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

#### Where DataType strings appear in saved v2 files

A v2 `.cnnd` file only stores DataType strings in **user-authored** locations. Built-in nodes do not serialise their per-instance pin declarations — on load, the v3 `NodeTypeRegistry` supplies pins from the v3 definitions, so every surviving built-in node (`atom_edit`, `atom_cut`, `atom_union`, `relax`, …) automatically gets its v3 pin signature (abstract input, `SameAsInput("input")` output) regardless of what v2 called them. Polymorphic pin types (`SameAsInput(..)`) are also not a concern: they are resolved from the connected input at validation time and never written to disk.

The rewrite pass therefore only needs to reach:

1. `SerializableNodeType.parameters[].data_type` — every custom network's declared parameter pins.
2. `SerializableNodeType.output_pins[].data_type` and the legacy `output_type: Option<String>` — every custom network's declared output pins.
3. `ParameterData.data_type` inside `parameter` node data — drives the enclosing network's parameter pin type.
4. `ExprData.data_type`, `MapData.output_type`, `SequenceData.element_type` — `NodeData` fields that embed a `DataType`.
5. `Array[..]` wrappings of any of the above.

#### Straight 1:1 renames

These have a single unambiguous v3 counterpart, valid in every location listed above:

| v2 | v3 |
|---|---|
| `"Geometry"` | `"Blueprint"` |
| `"UnitCell"` | `"LatticeVecs"` |

Apply as a blanket string substitution.

#### `Atomic` needs a different treatment — not a rename

The v2 `Atomic` variant was the **concrete** atomic-structure type. In v3 the same identifier (now spelled `HasAtoms`) names the **abstract** supertype of `Crystal` and `Molecule`. A blanket `"Atomic" → "HasAtoms"` rewrite is wrong for two independent reasons:

- **Validation would reject it.** `network_validator.rs` walks every user-declared DataType field — parameter-node `data_type`, custom-network parameters, sequence `element_type`, including inside `Array[..]` — and rejects any abstract type with a hard error ("abstract phase types are not allowed on parameter pins"). Every location where `"Atomic"` can appear in a v2 file is exactly such a field. The file would load and immediately fail.
- **The migration story runs through node replacement, not type rewriting.** The v2 nodes that produced/consumed `Atomic` fall into three categories, and the migration already handles them at the node layer: deleted-and-synthesised (`atom_fill` → `structure + materialize`, producing `Crystal`), renamed with semantics absorbed (`atom_lmove`/`atom_lrot`/`lattice_move`/`lattice_rot` → `structure_move`/`structure_rot`, polymorphic over `HasStructure`), and surviving with v3 pins auto-supplied by the registry (`atom_edit`, `atom_cut`, `atom_union`, `relax`, …). Once those node replacements are in place, a user-authored `"Atomic"` string elsewhere in the file is no longer a type on a wire — wire typing comes from the pin definitions the registry now supplies — it is a user-chosen placeholder for "this is where my concrete atoms go".

The concrete v3 type that best captures what a v2 author meant by `Atomic` is `Molecule`. Reasoning:

- In v2, `atom_fill` discarded structure; every "free" atomic operator (`atom_move`, `atom_rot`, `atom_edit`, `atom_cut`, `atom_union`, `relax`, …) operated on atoms without a structure association — exactly `Molecule`'s role in v3.
- Structure-carrying flows went through `atom_lmove`/`atom_lrot` in v2; those are rewritten to `structure_move`/`structure_rot` on `Crystal` by the node-rename pass and do not rely on any serialised `"Atomic"` string.
- Defaulting to `Molecule` cannot produce a runtime type mismatch. Defaulting to `Crystal` would require a structure association the migration has no way to supply.

Rule applied at every location listed in the "where" subsection:

| v2 | v3 |
|---|---|
| `"Atomic"` | `"Molecule"` |
| `"[Atomic]"` (any `Array[..]` nesting) | `"[Molecule]"` |

A rare user whose v2 intent on a custom-network output pin really was structure-bound will get a valid network that they can retype to `Crystal` in one edit. This is strictly recoverable; a hard validation failure on load is not.

#### `StructureBound` / `Unanchored`

Not handled. These identifiers existed only on intermediate commits of the refactoring branch before the final `Has*` spelling was chosen. No v2 save file in the wild can contain them.

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

This is the structurally non-trivial case: a single v2 node splits into two v3 nodes. Pre-v3 `atom_fill` produced concrete atoms from a motif tiled on a lattice. In v3 that computation decomposes into:

- a **`structure`** node that packages `motif` + `motif_offset` + `lattice_vecs` into a `Structure` value;
- a **`materialize`** node that consumes a `Blueprint` (a shape carrying a `Structure` field) and outputs a `Crystal`.

The v3 `structure` output type is `Structure`, and `materialize`'s input type is `Blueprint`. The two are **not** directly wire-compatible: a `Structure` must first pass through a primitive (or any other `Structure → Blueprint` node) before it can reach `materialize`. The migration cannot reliably infer which primitive in the user's network is the intended bounding shape for these atoms, so it does not guess. It synthesises the `structure` node to preserve the user's motif/offset/lattice wiring and leaves `materialize`'s `Blueprint` input disconnected. Network validation then flags the missing input on load — the same lenient, user-surfaced policy already used for deleted nodes.

#### Algorithm (per `atom_fill` node `N`)

1. **Allocate** a new ID `S` from the network's `next_node_id`; increment the JSON field.
2. **Create `S`** with `node_type_name = "structure"` at position `N.position + (-150, 0)` so auto-layout on next load is not disrupted.
3. **Move wires** from `N`'s inputs onto `S`:
   - `N.motif` → `S.motif`
   - `N.m_offset` → `S.motif_offset`
   - `N.lattice_vecs` → `S.lattice_vecs`
4. **Rename** `N.node_type_name` from `"atom_fill"` to `"materialize"`. Drop any `atom_fill`-specific fields on `N` that have no equivalent on `materialize` (`materialize` is parameterless).
5. **Do not add any wire between `S` and `N`.** The synthesised `Structure` output is type-incompatible with `N`'s `Blueprint` input. `N.Blueprint` is left disconnected.

Downstream wires leaving `N` are preserved unchanged. They now carry `Crystal` from `materialize` in place of `Atomic` from `atom_fill`, which is the intended v3 typing.

#### Example A — standalone `atom_fill`

Pre-v3:

```
motif ────┐
vec3 ─────┼─> atom_fill ─> display
unit_cell ┘
```

Post-migration:

```
motif ────┐
vec3 ─────┼─> structure(S)         (output dangling)
unit_cell ┘

materialize(N) ─> display
       ↑
  (Blueprint disconnected — validation error)
```

All of the user's motif/offset/lattice inputs are preserved on `S`. The user receives a clear "materialize needs a Blueprint" error on load and inserts a primitive between `S` and `N` themselves.

#### Example B — `atom_fill` shares `unit_cell` with a primitive

Pre-v3:

```
motif ─────┐
vec3 ──────┼─> atom_fill ─> display_A
unit_cell ─┤
           └─> cuboid ─> display_B
```

Post-migration (renames + primitive adaptation + atom_fill split):

```
motif ──────┐
vec3 ───────┼─> structure(S) ─> cuboid ─> display_B
unit_cell ──┘

            materialize(N) ─> display_A
                ↑
           (Blueprint disconnected)
```

The primitive-adaptation step (below) routes `S → cuboid.structure` because `cuboid` had a live `LatticeVecs` wire. The migration still refuses to fabricate `cuboid.Blueprint → materialize.Blueprint`: whether `cuboid` is the intended bounding shape for the `atom_fill`'s atoms is a semantic decision the migration is not qualified to make. The user wires that on first open.

#### Example C — `atom_fill → atom_lmove → display`

Pre-v3:

```
... ─> atom_fill ─> atom_lmove ─> display
```

Post-migration: `atom_lmove` is renamed to `structure_move` by the node-rename pass; `atom_fill` splits into `structure + materialize` with `materialize.Blueprint` disconnected per the algorithm. The `structure_move → display` tail is preserved; it no longer receives a `Crystal` upstream until the user wires a primitive into `materialize`, at which point the whole chain comes live.

### Primitive input adaptation: `LatticeVecs` → `Structure`

Phase 5 of the refactoring replaced the `LatticeVecs` input pin on primitives (cuboid, sphere, extrude, ...) with a `Structure` input pin.

For each primitive node in a network:

- If the old `lattice_vecs` input pin had no wire: nothing to do. The new `structure` input defaults to diamond.
- If the old pin had a wire from some source node of output type `LatticeVecs`: insert a synthesised `structure` node between the source and the primitive. The `structure` node's `lattice_vecs` input takes the original wire; its output feeds the primitive's new `structure` input.

### Interaction between `atom_fill` split and primitive adaptation

Both passes synthesise `structure` nodes; they compose cleanly because neither attempts to guess wiring that spans them. The `atom_fill` split creates one `structure` per `atom_fill` (feeding no one, awaiting the user). The primitive-adaptation step creates one `structure` per primitive that had a `LatticeVecs` wire. In the common "shared `unit_cell`" pattern (Example B above), this produces two synthesised `structure` nodes both reading the same `unit_cell` output — semantically identical but not deduplicated.

Deduplication (folding multiple synthesised `structure` nodes into one when their inputs match) is a polish item. Fresh-per-consumer is correct under evaluator semantics; revisit after opening real fixtures if the duplication is visually objectionable.

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
