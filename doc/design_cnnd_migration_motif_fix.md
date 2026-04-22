# .cnnd Migration Fix — Preserve `atom_fill.motif` Across v2 → v3

## Status

Follow-up to `doc/design_cnnd_migration_v2_to_v3.md`. That design produces silently-broken v3 output whenever a v2 `atom_fill` node had its `motif` pin wired. This document replaces the `atom_fill` → `materialize` conversion algorithm in phase 5 of the original design, and introduces two new node types needed by the new algorithm.

The original design's phases 1–4 and 6–8 remain unchanged. Only phase 5 (and the sample/demolib re-hydration that depends on it in phase 7) is reworked.

## Problem

In v2, `atom_fill` took `shape` and `motif` as independent arguments. The motif reached the output via `atom_fill`, not via the shape wire. In v3, `materialize` has no motif pin — it reads the motif from `shape.structure.motif` (see `rust/src/structure_designer/nodes/materialize.rs:167`). A v2 `motif → atom_fill` wire therefore must be turned into something that writes the motif into the `Structure` that rides on the Blueprint entering `materialize`.

The current migration (`synthesise_structure_for_atom_fill` in `rust/src/structure_designer/serialization/migrate_v2_to_v3.rs`) creates a `structure` node `S` carrying the v2 motif, but leaves `S` dangling — its output is not connected to anything. The result is a v3 file in which `materialize` receives a Blueprint whose `Structure` has the wrong (default-diamond) motif. The user sees diamond atoms in place of their actual motif with no error or warning.

Concrete failure case: `samples/MOF5-motif.cnnd`'s `motif_MOF5` network. The user's MOF5 motif is wired into a dangling `structure` node; `materialize` silently produces a diamond crystal. Reproducible by migrating `c:\atomcad_v0.3.0\samples\MOF5-motif.cnnd` through the current code.

## Why the obvious fixes don't work

- **Wire the motif into the primitive's `structure` input.** Fails when the v2 shape chain passes through a CSG op. `union.rs:106`, `intersect.rs:103`, and `diff.rs:115` all construct their output Blueprint with `Structure::from_lattice_vecs(...)`, dropping any motif carried on their inputs. (A separate design fixes that CSG behaviour, but the migration must be correct even against today's CSG semantics.) Also bad against import nodes, transforms, and multi-primitive shapes.
- **Reach back through the v2 JSON to find whatever source fed the primitive's `unit_cell` input, and wire that into `S.lattice_vecs`.** Has the same multi-subcase zoo: no primitive at all (import_cif), multiple primitives with different `unit_cell` sources, primitives behind custom networks, etc. The migration would have to walk and guess.

## Fix: two new nodes + inject a structure-override right before `materialize`

Introduce:

1. **`get_structure`** — reads the `Structure` carried by any `HasStructure` value.
2. **`with_structure`** — replaces the `Structure` carried by a `Blueprint`. Blueprint-only by design: overriding the structure on a `Crystal` is not meaningful because its atoms are already materialized against a specific structure.

Migration fans the v2 shape wire into two consumers — the original chain that continues downstream plus `get_structure` — uses the extracted `Structure` as the **base** for a `structure` node that patches in the user's motif, and routes the patched Structure into `with_structure` immediately before `materialize`. The lattice (and `motif_offset` on the Structure) comes from the shape itself, so the migration never has to guess or walk the upstream graph.

### Pipeline produced by the new algorithm

```
shape-source (Blueprint) ─┬──────────────────────────────► with_structure ──► materialize
                          │                                       ▲
                          └──► get_structure ──► structure(S) ────┘
                                                   ▲ base   ▲ motif / motif_offset
                                                          (from v2 atom_fill)
```

### Rejected alternative

A single `set_motif(Blueprint, Motif, Vec3) → Blueprint` node would be tighter, but `get_structure` + `with_structure` are more composable, reusable outside the migration, and let `structure`'s existing per-field override semantics do the work. Not worth introducing a single-purpose node.

## Node specs

Both nodes go in `rust/src/structure_designer/nodes/`. Register each in `nodes/mod.rs` and in `node_type_registry.rs::create_built_in_node_types()`.

### `get_structure`

- **File:** `rust/src/structure_designer/nodes/get_structure.rs`
- **Parameters (in order):**
  - `input: HasStructure` (required)
- **Output pins:** `OutputPinDefinition::single_fixed(DataType::Structure)`
- **Category:** `NodeTypeCategory::OtherBuiltin`
- **Node data:** empty struct `GetStructureData {}` (use `no_data`-style saver/loader pattern or `generic_node_data_*` over a unit struct — match the conventions in `structure.rs`).
- **Eval:**
  - Evaluate arg 0 as a required input.
  - If the result is `NetworkResult::Blueprint(bp)`, return `EvalOutput::single(NetworkResult::Structure(bp.structure.clone()))`.
  - If it is `NetworkResult::Crystal(c)`, return `EvalOutput::single(NetworkResult::Structure(c.structure.clone()))`.
  - Any other variant: `runtime_type_error_in_input(0)`.
- **Gadget/metadata:** none.

### `with_structure`

- **File:** `rust/src/structure_designer/nodes/with_structure.rs`
- **Parameters (in order):**
  - `shape: Blueprint` (required)
  - `structure: Structure` (required)
- **Output pins:** `OutputPinDefinition::single_fixed(DataType::Blueprint)`
- **Category:** `NodeTypeCategory::OtherBuiltin`
- **Node data:** empty struct `WithStructureData {}`.
- **Eval:**
  - Evaluate arg 0; must be `NetworkResult::Blueprint(bp)`. On mismatch, `runtime_type_error_in_input(0)`.
  - Evaluate arg 1; must be `NetworkResult::Structure(s)`. On mismatch, `runtime_type_error_in_input(1)`.
  - Return `EvalOutput::single(NetworkResult::Blueprint(BlueprintData { structure: s, geo_tree_root: bp.geo_tree_root, alignment: bp.alignment, alignment_reason: bp.alignment_reason }))`.
- **Crystal support is explicitly out of scope.** Do not make `with_structure` accept `HasStructure`. A Crystal's atoms are positioned relative to a specific structure; silently reassigning the structure field would decouple atoms from lattice.

### Text format

Both nodes have no user-editable properties — `get_text_properties` returns an empty list; `set_text_properties` accepts an empty map. The pin names above are the text-format wire references.

### Tests

Follow `rust/tests/structure_designer/` conventions (no inline `#[cfg(test)]` in `src/`).

- **`rust/tests/structure_designer/get_structure_test.rs`:**
  - Construct a Blueprint with a known Structure; pass it through `get_structure`; assert the output Structure equals input.
  - Same for Crystal (use a small materialized crystal).
  - Non-HasStructure input (e.g. Motif) produces a runtime error.
- **`rust/tests/structure_designer/with_structure_test.rs`:**
  - Blueprint in, Structure with non-diamond motif in, Blueprint out carries the new Structure; `geo_tree_root` is preserved bit-for-bit.
  - Non-Blueprint `shape` input (e.g. a Crystal) produces a runtime type error (the validator should also reject it statically; assert the eval-time guard anyway).
- **Snapshot tests:** add entries for both nodes in the existing node-type snapshot suite (`rust/tests/structure_designer/node_snapshots_test.rs` — pattern match existing nodes).

## Revised migration algorithm

File: `rust/src/structure_designer/serialization/migrate_v2_to_v3.rs`.

Replace the body of `synthesise_structure_for_atom_fill(network_json)` (around line 523). Per `atom_fill` node `N`:

Let `shape_wire = N.arguments[0]` and `motif_wire = N.arguments[1]` (both are `Argument` JSON values — either wired or empty).

**Case A — both `shape_wire` and `motif_wire` are wired:**

1. Allocate three new ids from `next_node_id`: `G`, `S`, `W` (in that order; bump `next_node_id` by 3).
2. Create `G` = `get_structure` at `N.position + (-330, -40)`:
   - `G.arguments[0]` ← clone of `shape_wire`.
3. Create `S` = `structure` at `N.position + (-200, -40)`:
   - `S.arguments[0]` (structure / base) ← wire from `G` output pin 0.
   - `S.arguments[1]` (lattice_vecs) ← empty.
   - `S.arguments[2]` (motif) ← `motif_wire` (moved, not cloned — it is being migrated off `N`).
   - `S.arguments[3]` (motif_offset) ← move from `N.arguments[2]` if that was wired in v2; else empty.
4. Create `W` = `with_structure` at `N.position + (-90, 0)`:
   - `W.arguments[0]` (shape) ← clone of `shape_wire`.
   - `W.arguments[1]` (structure) ← wire from `S` output pin 0.
5. Rewrite `N.arguments[0]` to be a wire from `W` output pin 0 (replacing the original `shape_wire`).
6. Re-index `N.arguments[3..=6]` to `materialize`'s layout: `[3]→[1]`, `[4]→[2]`, `[5]→[3]`, `[6]→[4]`.
7. Rename `N.node_type_name` from `"atom_fill"` to `"materialize"`.
8. Translate `AtomFillData` → `MaterializeData` by dropping `motif_offset` (carry over the rest verbatim).

**Case B — `shape_wire` wired, `motif_wire` empty:**

No motif to preserve. Do not create `G`, `S`, or `W`. Re-index and rename as steps 6–8 above. `N.arguments[0]` stays as the original `shape_wire`.

**Case C — `shape_wire` empty, `motif_wire` wired:**

Cannot create `G` / `W` because there is no shape value to tap. Preserve the motif in a dangling `S` (this matches the original design doc's "S dangling" outcome for this edge case):

1. Allocate one new id `S`.
2. Create `S` = `structure` at `N.position + (-150, 0)`:
   - `S.arguments[0]` ← empty.
   - `S.arguments[1]` ← empty.
   - `S.arguments[2]` ← `motif_wire` (moved).
   - `S.arguments[3]` ← move from `N.arguments[2]` if wired, else empty.
3. Re-index and rename as in Case A steps 6–8. `N.arguments[0]` stays empty.

The dangling `S` here is load-bearing: the user's motif wire is preserved for them to reconnect. This is strictly better than losing the wire, and matches the file being invalid in v2 anyway (no shape input → `atom_fill` could not evaluate).

**Case D — both empty:**

No `G`/`S`/`W`. Just re-index and rename. Same as Case B modulo pin 1 being empty.

### Position offsets

The offsets chosen (`-330, -40` for `G`; `-200, -40` for `S`; `-90, 0` for `W`) keep the new nodes to the left of `N` and slightly above, so auto-layout preserves a readable left-to-right flow on first open. Tune during implementation if the defaults look cramped.

### next_node_id discipline

Bump `next_node_id` in the JSON by the number of nodes allocated (3 in Case A, 1 in Case C, 0 otherwise). The existing migration already does this for the single-`S` path; extend the arithmetic.

### Interaction with phase 4 (primitive adaptation)

Unchanged. The primitive-adaptation pass still inserts a `structure` adapter ahead of any primitive that had a v2 `unit_cell` wire, to translate that wire into the primitive's new `Structure` input. That adapter supplies the lattice to the primitive's Blueprint; `get_structure` then re-reads that same lattice (plus diamond-default motif) and the `structure(S)` node patches in the user's motif. No changes needed in phase 4.

## Error policy

Unchanged from the original design. If a v2 `atom_fill` node's JSON is malformed (non-object node, missing `arguments`, etc.), the migration returns `MigrationError` with a locator pointing at the offending network/node.

## Sample / demolib impact

The sample files affected by this fix — every project containing an `atom_fill` whose `motif` pin was wired — will re-migrate to different v3 shapes than the current code produces. This includes at minimum `MOF5-motif.cnnd` (confirmed), likely others. Phase 7 of the original design (re-hydrate and re-save) must be re-run after this fix ships:

1. Restore v2 copies of `samples/*.cnnd` and `demolib/*.cnnd` from `main` (`git checkout main -- <path>`).
2. Load-and-save through the updated migration to produce fresh v3 files.
3. `cargo insta review` the snapshot tests that will shift (`text_format_snapshot_test`, `node_snapshots_test`). Accept the new outputs.

## Test plan

Extend `rust/tests/integration/lattice_space_migration_test.rs` and the fixtures under `rust/tests/fixtures/lattice_space_migration/`:

### Update existing fixtures

- **`atom_fill_split.cnnd`**: expected post-migration shape changes from "1 synthesized `structure` dangling" to "`get_structure` + `structure` + `with_structure` inline, wired into materialize". Rewrite the test's assertions against the new shape.
- **`shared_unit_cell.cnnd`**: same. The shared-unit-cell case now produces a chain with both the primitive adapter (phase 4) and the new W/G/S triplet (phase 5) composing cleanly. Assert the output of evaluating `materialize` against a known motif actually produces atoms.

### New fixtures

- **`motif_mof5.cnnd`** (smaller stand-in if the full MOF5 is unwieldy): a custom network returning `Motif`, with an `atom_fill` on a side-chain display, a wired `motif` input to `atom_fill`, a wired `unit_cell` on the primitive, and a non-default motif. Post-migration, evaluate the materialize node and assert at least one atom of the motif's element is present in the output. This is the regression fixture — failure means we're back to diamond.
- **`atom_fill_unwired_shape.cnnd`**: Case C coverage — motif wired, shape unwired. Assert S is created dangling, materialize has no shape wire, validation reports the missing shape as a user-visible error (not a migration failure).

### Real-sample smoke

Copy `MOF5-motif.cnnd` from `c:\atomcad_v0.3.0\samples\` or `git show main:samples/MOF5-motif.cnnd` into `rust/tests/fixtures/lattice_space_migration/real_mof5.cnnd`. Test: load, migrate, re-save, reload, idempotent. Assert the `motif_MOF5` network's materialize node, when evaluated, produces atoms whose elements include Zn / O / C (the MOF5 elements), not C-only (which would indicate diamond output).

### Negative / edge tests

- Double-migration idempotence on a Case-A file.
- Re-save round-trip byte-identity after first v3 save.
- `with_structure` rejects Crystal at validation time (static type check, not just runtime).

## Implementation order

1. Add `get_structure` node type + tests + snapshot entries.
2. Add `with_structure` node type + tests + snapshot entries.
3. Rewrite `synthesise_structure_for_atom_fill` to emit the W/G/S triplet in Case A, and cover cases B/C/D.
4. Update existing migration fixtures' expected shapes; add `motif_mof5.cnnd` and `real_mof5.cnnd`.
5. Re-run phase 7 of the original design: re-hydrate `samples/` and `demolib/` from main, re-save through the fixed migration, `cargo insta review` the snapshot shifts.
6. Full `cargo test` + `cargo clippy` + `cargo fmt --check` green.

## Non-goals

- No behaviour change for `materialize`, `structure`, or any primitive.
- No change to the phase 4 primitive adaptation.
- No user-facing UI for the new nodes beyond the automatic registry entries.
- No Crystal-input support on `with_structure` — see rationale in the spec above.
- Fixing CSG operations to preserve full `Structure` (rather than dropping motif) is a separate design (`doc/design_csg_structure_propagation.md`) and is **not** a prerequisite for this migration fix. The `with_structure` insertion point is after all CSG ops, so the fix is correct regardless of CSG's current motif-stripping behaviour.
