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

This is the structurally non-trivial case: a single v2 node splits into two v3 nodes.

Pre-v3 `atom_fill` consumed a `Geometry` shape (which implicitly carried lattice context), a motif, a motif offset, and four Bool flags, and produced `Atomic`. In v3 that computation decomposes into:

- a **`structure`** node that packages `motif` + `motif_offset` (and optionally an upstream `structure` and `lattice_vecs`) into a `Structure` value;
- a **`materialize`** node that consumes a `Blueprint` (a shape carrying a `Structure` field) plus the four Bool flags (`passivate`, `rm_single`, `surf_recon`, `invert_phase`) and outputs a `Crystal`.

The algorithm depends on the exact argument-vector layouts, so they are stated here explicitly (wires are indexed by position in each node's `arguments: Vec<Argument>`, not by pin name):

| Index | v2 `atom_fill` | v3 `materialize` | v3 `structure` |
|---|---|---|---|
| 0 | shape (Geometry) | shape (Blueprint) | structure (Structure) |
| 1 | motif (Motif) | passivate (Bool) | lattice_vecs (LatticeVecs) |
| 2 | m_offset (Vec3) | rm_single (Bool) | motif (Motif) |
| 3 | passivate (Bool) | surf_recon (Bool) | motif_offset (Vec3) |
| 4 | rm_single (Bool) | invert_phase (Bool) | — |
| 5 | surf_recon (Bool) | — | — |
| 6 | invert_phase (Bool) | — | — |

`AtomFillData` → `MaterializeData` is near-identical: `parameter_element_value_definition`, `hydrogen_passivation`, `remove_single_bond_atoms_before_passivation`, `surface_reconstruction`, `invert_phase` carry over verbatim. The one field without a v3 equivalent is `motif_offset: DVec3`, which in v3 lives only as a wire-able pin on the `structure` node, not as a stored default on `materialize`.

Under this layout the `shape` wire is already in place: v2 `atom_fill.shape` (arg 0) becomes v3 `materialize.shape` (arg 0) by re-indexing alone. The v3 primitives (`cuboid`, `sphere`, …) now declare `Blueprint` output in the registry, so an existing `cuboid → atom_fill.shape` wire becomes `cuboid → materialize.shape` without any adapter. The "`materialize.Blueprint` left disconnected" outcome only arises for v2 files that already had `atom_fill.shape` unwired — which would not have evaluated in v2 either.

There is **no** `S → N` wire: the synthesised `Structure` from `S` is not directly compatible with `N`'s `Blueprint` input. `S` exists purely to preserve the user's `motif` / `motif_offset` wires; its `structure` and `lattice_vecs` inputs are left unwired for the user to connect on first open (or delete if the node is not wanted). The lattice context that in v2 flowed through the `Geometry` shape now flows through the primitive's new `structure` input, handled by the primitive-adaptation pass in the next section.

#### Algorithm (per `atom_fill` node `N`)

1. **Allocate** a new ID `S` from the network's `next_node_id`; increment the JSON field.
2. **Create `S`** with `node_type_name = "structure"` at position `N.position + (-150, 0)` so auto-layout on next load is not disrupted. Its `NodeData` is the default empty `StructureData`; its `arguments` vector is four empty `Argument`s.
3. **Move motif/offset wires** from `N` to `S`, by v2 arg index:
   - `N.arguments[1]` (motif) → `S.arguments[2]` (motif).
   - `N.arguments[2]` (m_offset) → `S.arguments[3]` (motif_offset).
4. **Re-index `N`'s remaining arguments** to the v3 `materialize` layout:
   - v2 arg 0 (shape) → v3 arg 0 (shape). Kept as-is; wire source unchanged.
   - v2 arg 3 (passivate) → v3 arg 1.
   - v2 arg 4 (rm_single) → v3 arg 2.
   - v2 arg 5 (surf_recon) → v3 arg 3.
   - v2 arg 6 (invert_phase) → v3 arg 4.

   Explicit re-indexing is **required**. The validator's argument-count repair would otherwise truncate to 5 args and leave `motif` / `m_offset` sitting at positions 1 and 2, where v3 `materialize` expects Bool pins — a type error on load.
5. **Rename** `N.node_type_name` from `"atom_fill"` to `"materialize"`.
6. **Translate `N`'s NodeData** from `AtomFillData` to `MaterializeData`: drop the `motif_offset` field; keep the rest verbatim. If the user had a non-zero `motif_offset` without wiring the `m_offset` pin, it is silently dropped — they re-enter it on first open. (Lenient policy, consistent with deleted-node wires.)

Downstream wires leaving `N` are preserved unchanged. They now carry `Crystal` from `materialize` in place of `Atomic` from `atom_fill`, which is the intended v3 typing.

#### Example A — typical `atom_fill` with a primitive shape

Pre-v3 (v2 `atom_fill.shape` takes the primitive, which implicitly carries the lattice; `motif` and `m_offset` are separate wires):

```
unit_cell ─> cuboid ─> atom_fill ─> display
motif ────────────────^
vec3 ─────> m_offset ─^
```

Post-migration:

```
lattice_vecs ─> structure(adapter) ─> cuboid ─> materialize(N) ─> display

                structure(S)       (output dangling)
motif ────────────^ (motif)
vec3 ─────────────^ (motif_offset)
                  (structure and lattice_vecs inputs unwired)
```

The `cuboid → materialize.shape` wire carries over unchanged (same arg index 0, registry now declares it `Blueprint → Blueprint`). The primitive-adaptation pass inserts a `structure` adapter ahead of `cuboid` to supply the lattice context that v2 carried implicitly on the `Geometry` wire. `S` holds the user's `motif` and `motif_offset` wires; its own `structure` and `lattice_vecs` inputs are left for the user to connect on first open or to delete.

#### Example B — standalone `atom_fill` with unconnected shape

Pre-v3 (rare: `atom_fill.shape` left unwired; would not have evaluated in v2 either):

```
motif ─────> atom_fill ─> display
vec3 ──────> m_offset ─^
```

Post-migration:

```
             materialize(N) ─> display
                ↑
           (shape disconnected — validation error, same as v2)

             structure(S)
motif ──────────^ (motif)
vec3 ───────────^ (motif_offset)
                 (structure and lattice_vecs inputs unwired)
```

The missing-shape validation error is not a migration artefact — the v2 file was already invalid on this point. The migration faithfully preserves that state.

#### Example C — `atom_fill → atom_lmove → display`

Pre-v3:

```
... ─> cuboid ─> atom_fill ─> atom_lmove ─> display
```

Post-migration: the primitive-adaptation pass inserts a `structure` adapter ahead of `cuboid` if a `unit_cell` was wired to it. `atom_lmove` is renamed to `structure_move` — its surplus v2 `unit_cell` input (arg 3) is dropped by the validator's argument-count repair, leaving the first three args (`input`, `translation`, `subdivision`) aligned with v3 `structure_move`. `atom_fill` splits into `structure(S)` (dangling, holding the v2 motif/m_offset wires) and `materialize(N)` (which now feeds `structure_move`). The whole chain remains live.

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

### Scope

All migration testing lives entirely under `rust/tests/` — no new code in `src/` beyond the migration itself, no cross-branch test infrastructure, no golden-file scheme, no manifests. We rely on hand-crafted fixtures plus a small number of real-world smoke samples pulled from `main`, each tested through the public load/save API.

Cross-branch semantic comparison (evaluate on `main` → evaluate post-migration on this branch → diff) was considered and rejected as too expensive for the coverage it buys. The change classes with non-trivial semantic divergence (`atom_fill` split leaves the synthesised `structure(S)` dangling; `atom_trans` removal strands downstream wires) cannot be covered by evaluation comparison anyway, so the rest does not justify the machinery.

### Hand-crafted fixtures

New fixture directory: `rust/tests/fixtures/lattice_space_migration/`.

Minimum fixtures (one tiny `.cnnd` file each, each focused on one change class):

1. `pure_rename.cnnd` — a network using `Geometry`, `UnitCell`, `unit_cell`, `atom_move` types and nothing structural. Exercises the rename tables.
2. `atom_fill_split.cnnd` — a minimal `unit_cell → cuboid → atom_fill` pipeline with wires on all four Bool flag pins and non-default `AtomFillData` values. Verifies node synthesis, argument re-indexing, and NodeData translation.
3. `primitive_with_lattice.cnnd` — a `unit_cell → cuboid` wire. Verifies primitive adapter synthesis.
4. `shared_unit_cell.cnnd` — one `unit_cell` feeding a primitive that then feeds an `atom_fill.shape`, plus the `unit_cell` feeding a second primitive directly. Verifies composition of the primitive-adaptation and atom_fill-split passes.
5. `frame_transform_present.cnnd` — a file whose `GeometrySummary`-shaped payload carries `frame_transform`. Verifies silent drop.
6. `atom_trans_present.cnnd` — a file containing `atom_trans`. Verifies drop-with-dangling-wires policy.
7. `custom_network.cnnd` — a custom network whose `node_type` has parameters and output pins typed `Geometry`/`UnitCell`. Verifies the walker reaches into custom-network type definitions.

New integration test file: `rust/tests/integration/lattice_space_migration_test.rs`, registered in `rust/tests/integration.rs`.

Per-fixture test shape:

- Load the old fixture through `load_node_networks_from_file`.
- Assert the in-memory form matches expectations (node types present, wires connected, DataTypes resolved, expected validation errors for fixtures whose migration leaves dangling inputs).
- Re-save to a temp file.
- Reload the re-saved file. Verify it now has `version: 3` and no longer triggers the migration path.
- Verify round-trip idempotence: second save produces byte-identical output to the first save.

### Additional cheap checks

On top of the per-fixture loop, add four small class-of-bug guards that cost almost nothing to write and would each catch a real regression:

- **Double-migration idempotence.** Call `migrate_v2_to_v3` twice on the same `Value` and assert the second call is a no-op (`Value` unchanged). Guards against helpers that silently mutate already-migrated shapes — the kind of bug that only surfaces if the pre-pass is accidentally re-invoked.
- **v3 no-op check.** A fixture already at `version: 3` must skip the pre-pass entirely. Either instrument with a call counter or confirm byte-identity through the load path.
- **Corrupt-input clear-error check.** One malformed v2 fixture (truncated JSON, broken wire ID) should yield a `MigrationError` with a useful message — never a panic.
- **Real-sample smoke.** Copy one or two real files from `samples/`/`demolib/` into the fixture directory and test round-trip only — loads, migrates, re-saves, reloads, idempotent. No semantic comparison. Catches combinations of change classes the minimal fixtures don't hit.

### Sourcing real-sample fixtures

**The `samples/` and `demolib/` .cnnd files in this branch's working tree cannot be used as v2 fixtures.** They were partially hand-edited during the rename commits (`atom_fill` renamed in place to `materialize` without the required `structure` synthesis; `lattice_rot`/`lattice_move` stripped of their properties) and are therefore neither clean v2 nor correctly migrated v3. Pull clean v2 copies from `main`:

```sh
git show main:samples/diamond.cnnd \
  > rust/tests/fixtures/lattice_space_migration/real_diamond.cnnd
```

Pick one atom-heavy and one geometry-heavy sample for reasonable coverage. Keep the set small (≤ 2 files); the hand-crafted fixtures remain the primary test surface.

### Pre-existing snapshot breakage

As of the start of this migration work, running `cargo test` on this branch shows 17 pre-existing snapshot failures caused by the partially-edited branch .cnnd files: 10 `text_format_snapshot_test` cases and 7 `node_snapshots_test` evaluation cases across `diamond`, `extrude-demo`, `flexure-delta-robot`, `halfspace-demo`, `hexagem`, `mof5-motif`, `nut-bolt`, `rotation-demo`, `rutile-motif`, `truss`. These failures are not caused by the migration code (which does not yet exist) and are not addressed by the migration test suite. They are cleared in Phase 7 below by re-hydrating the branch files from `main` and letting the load-time migration rewrite them on save.

### Unchanged coverage

Existing `cnnd_roundtrip_test.rs` should continue to pass without changes (it operates on files the new code saves, which are all v3).

## Non-goals

- **No migration from v1.** v1 files have been auto-migrated by the existing serde-level code since v2 landed. They continue to load (the existing serde compat stays in place) and are immediately bumped through both hops (v1 → v2 via serde defaults, v2 → v3 via this pre-pass) on first load.
- **No user-facing migration UI.** Loading is silent except for warnings emitted through the existing log mechanism.
- **No batch "upgrade all files" command-line tool.** If a user wants to pre-convert a directory, they can load and save each file through atomCAD; out of scope for this phase.
- **No preservation of deleted-node semantics via user-defined replacement nodes.** If the user relied on `lattice_symop`, they must rebuild that part of the network using the new nodes. The migration will not attempt to synthesise an equivalent subgraph.

## Implementation Order

Eight phases. Each leaves the tree compiling and all tests green (except phase 7, which clears pre-existing failures).

### Fixture-to-phase map

At-a-glance summary of which test artifacts land in which phase:

| Phase | Migration work | Fixtures / test artifacts shipped |
|---|---|---|
| 1 | Scaffolding, version dispatch (identity migration) | Trivial v3-no-op fixture |
| 2 | `rename_data_type_strings` + `rename_node_type_strings` | `pure_rename.cnnd`, `custom_network.cnnd` |
| 3 | `drop_deleted_nodes` | `atom_trans_present.cnnd` |
| 4 | `adapt_primitives_lattice_to_structure` | `primitive_with_lattice.cnnd` |
| 5 | `synthesise_structure_for_atom_fill` | `atom_fill_split.cnnd`, `shared_unit_cell.cnnd` |
| 6 | Hardening | `frame_transform_present.cnnd`, double-migration idempotence, v3 no-op, corrupt-input, 1–2 real samples from `main` (via `git show main:…`) |
| 7 | Re-hydrate branch `samples/` and `demolib/` from `main`, rewrite to v3, refresh snapshots | — (chore) |
| 8 | Merge to `main` | — |

Two distinct "copy from `main`" operations exist:

- **Phase 6** uses `git show main:samples/<file>.cnnd > rust/tests/fixtures/lattice_space_migration/<file>.cnnd` to seed **test fixtures**.
- **Phase 7** uses `git checkout main -- samples/... demolib/...` to restore **the app's shipped sample files**, which are then rewritten to v3 by the load-time migration.

### Phase 1 — Scaffolding and version dispatch

- Bump `SERIALIZATION_VERSION` from 2 to 3 in `node_networks_serialization.rs`.
- Adjust the version check in `load_node_networks_from_file`: reject `version > 3` (current behaviour for `> 2`), accept `version < 3` and route through `migrate_v2_to_v3`.
- Create `rust/src/structure_designer/serialization/migrate_v2_to_v3.rs` with:
  - `MigrationError` enum (wrapping `serde_json::Error`, plus `MalformedStructure(String)`).
  - `pub fn migrate_v2_to_v3(root: &mut serde_json::Value) -> Result<(), MigrationError>` — entry point that currently returns `Ok(())` without touching the value.
  - Stub signatures for the five internal helpers listed in **Migration Module Layout**.
- Register new integration test file `rust/tests/integration/lattice_space_migration_test.rs` in `rust/tests/integration.rs` with a single trivial test: a v3-already fixture loads unchanged through the new dispatch.

Exit state: scaffolding compiles, old behaviour preserved, one trivial test passes.

### Phase 2 — String renames (DataType + node type)

- Implement `rename_data_type_strings` covering every location listed in the Change Catalogue's "Where DataType strings appear" subsection, including the `Array[..]` wrapping rule and descent into every custom network's embedded `node_type`. Apply both the straight 1:1 renames and the `Atomic → Molecule` rule.
- Implement `rename_node_type_strings` against `SerializableNode.node_type_name` and each `SerializableNodeType.name`, using the node rename table.
- Ship fixtures 1 (`pure_rename.cnnd`) and 7 (`custom_network.cnnd`) with per-fixture tests per the Testing section.

Exit state: pure-rename conversions are covered end-to-end.

### Phase 3 — Deleted-node drop

- Implement `drop_deleted_nodes` for `atom_trans` and `lattice_symop`. Remove the node entries and disconnect wires referencing them on either end (downstream wires are left dangling so network validation surfaces the missing node to the user).
- Ship fixture 6 (`atom_trans_present.cnnd`). The fixture's expected post-migration state includes a documented validation error list asserted by the test.

Exit state: deleted-node policy covered; lenient-drop behaviour verified.

### Phase 4 — Primitive input adaptation

- Implement `adapt_primitives_lattice_to_structure`. Walk each network's node list; for every primitive node (cuboid, sphere, extrude, …) with a live incoming wire on the old `lattice_vecs` input, synthesise a `structure` adapter node between the source and the primitive as specified in **Primitive input adaptation**. Leave the new `structure` input on the primitive pointing at the adapter's output; allocate IDs from `next_node_id`.
- Ship fixture 3 (`primitive_with_lattice.cnnd`).

Exit state: pure-adapt conversions are covered, including the dangling-input case (no-op) and the wired case (adapter synthesised).

### Phase 5 — `atom_fill` split

- Implement `synthesise_structure_for_atom_fill` strictly following the algorithm in the Change Catalogue:
  1. Allocate a new ID `S` from `next_node_id`; increment.
  2. Create `S` as `structure` at `N.position + (-150, 0)` with default empty `StructureData` and four empty `Argument`s.
  3. Move `N.arguments[1]` (motif) → `S.arguments[2]`; move `N.arguments[2]` (m_offset) → `S.arguments[3]`. Leave `S.arguments[0]` (structure) and `S.arguments[1]` (lattice_vecs) empty.
  4. Re-index `N`'s remaining arguments to v3 `materialize` order: `[0]→[0]` (shape), `[3]→[1]` (passivate), `[4]→[2]` (rm_single), `[5]→[3]` (surf_recon), `[6]→[4]` (invert_phase).
  5. Rename `N.node_type_name` from `"atom_fill"` to `"materialize"`.
  6. Translate `AtomFillData` → `MaterializeData`: drop `motif_offset`; preserve `parameter_element_value_definition`, `hydrogen_passivation`, `remove_single_bond_atoms_before_passivation`, `surface_reconstruction`, `invert_phase` verbatim.
  7. Do **not** wire `S → N`; the lattice context flows through the primitive's new `structure` input, handled by phase 4.
- Ship fixtures 2 (`atom_fill_split.cnnd`) and 4 (`shared_unit_cell.cnnd`). The shared fixture exercises composition with phase 4's adapter pass; assert that both synth passes compose cleanly. `atom_fill_split.cnnd` must include wires on **all four** Bool flag pins plus non-default NodeData values, so the arg re-indexing is verified.

Exit state: all migration helpers complete. Every hand-crafted fixture except 5 passes.

### Phase 6 — Hardening and real-sample smoke

- Ship fixture 5 (`frame_transform_present.cnnd`), verifying serde silently drops the removed field on load and it does not reappear on save.
- Add the four additional checks listed in **Additional cheap checks**: double-migration idempotence, v3 no-op check, corrupt-input clear-error, and real-sample smoke. Pull the 1–2 real-sample fixtures from `main` via `git show main:samples/<file>.cnnd > …`.
- Audit the `MigrationError` surface: every `Err` path emits a message locating the offending network / node / pin.

Exit state: full test matrix green; migration code is feature-complete.

### Phase 7 — Re-hydrate branch sample/demolib files and refresh snapshots

This is a chore, not migration-code work, but it must land before merge so the main branch tests are clean.

- For each of the 13 .cnnd files in `samples/` and `demolib/` that this branch modified: restore the clean v2 copy from `main` (`git checkout main -- <path>`), commit.
- Run the application (or a one-shot load-and-save CLI pass) on each restored file so the load-time migration rewrites it as v3 on disk. Commit the v3 forms.
- `cargo insta review` the 17 previously-failing snapshots (10 `text_format_snapshot_test`, 7 `node_snapshots_test`). Accept the new shapes; commit.
- Full `cargo test` sweep must be green.

Exit state: the working tree contains only v3 .cnnd files, snapshots reflect v3 output, all tests pass.

### Phase 8 — Merge to `main`

- Pre-merge checklist:
  - `cargo test`, `cargo clippy`, `cargo fmt --check`, `flutter analyze` all clean.
  - No regressions in `cnnd_roundtrip_test` (it should be untouched).
  - Manifest of migration helpers vs. the Change Catalogue: every class has at least one fixture.
- Merge `lattice-space-refactoring` → `main`.

After merge, the v2 → v3 pre-pass becomes historical infrastructure. Any future file loaded from a pre-refactoring checkout routes through it silently; every file saved from that point on is v3.

## Open Questions

- **Deduplication of synthesised `structure` nodes** when the same `unit_cell` output was shared across multiple consumers. Fresh-per-consumer is simpler and correct; dedup is a polish item. Decision point before step 6 above.
- **Warnings surfacing**: do migration warnings go to stderr, to a log, or to a UI dialog on first successful load? Existing conventions in the codebase (if any) should be followed.
- **Position of synthesised nodes**: a fixed offset from the node they feed is adequate. If visual clutter is a concern in practice, revisit after opening a few real fixtures.
