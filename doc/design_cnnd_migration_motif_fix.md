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
  - Static validator rejects a non-`HasStructure` source (e.g. a Motif output) on the input pin — no wire can be created.
  - Defense-in-depth: directly invoke `eval` with a non-`HasStructure` `NetworkResult` (which the validator would normally prevent) and assert it returns `runtime_type_error_in_input(0)`.
- **`rust/tests/structure_designer/with_structure_test.rs`:**
  - Blueprint in, Structure with non-diamond motif in, Blueprint out carries the new Structure; `geo_tree_root` is preserved bit-for-bit.
  - Non-Blueprint `shape` input (e.g. a Crystal) produces a runtime type error (the validator should also reject it statically; assert the eval-time guard anyway).
- **Snapshot tests:** add entries for both nodes in the existing node-type snapshot suite (`rust/tests/structure_designer/node_snapshots_test.rs` — pattern match existing nodes).

## Revised migration algorithm

File: `rust/src/structure_designer/serialization/migrate_v2_to_v3.rs`.

Replace the body of `synthesise_structure_for_atom_fill(network_json)` (around line 523). The algorithm is the cross product of two independent predicates per `atom_fill` node `N`:

- `can_chain` ≡ `N.arguments[0]` (v2 shape pin) is wired.
- `needs_S` ≡ `N.arguments[1]` (v2 motif pin) is wired **OR** `N.arguments[2]` (v2 motif_offset pin) is wired.

|                | `needs_S`                          | `!needs_S`        |
|----------------|------------------------------------|-------------------|
| `can_chain`    | **A:** `G` + `S` + `W` spliced inline | **B:** rename only |
| `!can_chain`   | **C:** dangling `S`                | **D:** rename only |

Procedure, applied per `atom_fill`:

1. **If `needs_S` and `can_chain`:** allocate fresh ids `G`, `S`, `W` from `next_node_id` in that order (bump by 3). **Else if `needs_S`:** allocate `S` only (bump by 1). **Else:** no allocations.
2. **If `needs_S`:** create `S` = `structure` at `N.position + (-200, -40)`:
   - `S.arguments[0]` (base structure) ← wire from `G` output pin 0 if `can_chain`, else empty.
   - `S.arguments[1]` (lattice_vecs override) ← empty.
   - `S.arguments[2]` (motif override) ← `N.arguments[1]` if wired, else empty.
   - `S.arguments[3]` (motif_offset override) ← `N.arguments[2]` if wired, else empty.
3. **If `needs_S` and `can_chain`:**
   - Create `G` = `get_structure` at `N.position + (-330, -40)` with `G.arguments[0]` ← clone of `N.arguments[0]`.
   - Create `W` = `with_structure` at `N.position + (-90, 0)` with `W.arguments[0]` ← clone of `N.arguments[0]` and `W.arguments[1]` ← wire from `S` output pin 0.
   - Rewrite `N.arguments[0]` to be a wire from `W` output pin 0 (replacing the original shape wire).
4. **Always:** re-index `N.arguments[3..=6]` to `materialize`'s layout: `[3]→[1]`, `[4]→[2]`, `[5]→[3]`, `[6]→[4]`. This overwrites the original pin-1 and pin-2 slots, which is safe — if their wires were load-bearing they were already moved to `S` in step 2.
5. **Always:** rename `N.node_type_name` from `"atom_fill"` to `"materialize"` (and the `data_type` tag likewise).
6. **Always:** translate `AtomFillData` → `MaterializeData` by dropping `motif_offset` (carry over the rest verbatim).

### Why `needs_S` includes the motif_offset wire

A v2 user could wire `motif_offset` while leaving `motif` at its default (diamond zincblende) — to shift the default motif inside the unit cell. If `needs_S` were gated only on the motif wire, step 4's re-index would silently overwrite `N.arguments[2]` and the user's offset wire would vanish: silent data loss. Broadening the predicate routes the offset wire onto `S.arguments[3]` whenever it was wired. When only the offset is wired, `S` overrides offset only and leaves `get_structure`'s default-diamond motif intact — exactly matching v2 atom_fill semantics for that input.

### Dangling S (row C)

When `needs_S` but not `can_chain`, `S` is created but its output is not connected to anything. The file was already invalid in v2 in this state (no shape input → `atom_fill` could not evaluate), so a dangling `S` that costs the user a single drag-to-reconnect is strictly better than dropping the wires.

### Iteration order

Process `atom_fill` nodes in the order they appear in the JSON `nodes` array, matching the existing migration code. Allocating ids via HashMap iteration would produce non-deterministic v3 output and break the re-save round-trip byte-identity test in Phase 3.

### Position offsets

The offsets chosen (`-330, -40` for `G`; `-200, -40` for `S`; `-90, 0` for `W`) keep the new nodes to the left of `N` and slightly above, so auto-layout preserves a readable left-to-right flow on first open. Tune during implementation if the defaults look cramped.

### Interaction with phase 4 (primitive adaptation)

Unchanged. The primitive-adaptation pass still inserts a `structure` adapter ahead of any primitive that had a v2 `unit_cell` wire, to translate that wire into the primitive's new `Structure` input. That adapter supplies the lattice to the primitive's Blueprint; `get_structure` then re-reads that same lattice (plus diamond-default motif) and the `structure(S)` node patches in the user's motif. No changes needed in phase 4.

## Error policy

Unchanged from the original design. If a v2 `atom_fill` node's JSON is malformed (non-object node, missing `arguments`, etc.), the migration returns `MigrationError` with a locator pointing at the offending network/node.

## Sample / demolib impact

The sample files affected by this fix — every project containing an `atom_fill` whose `motif` pin was wired — will migrate to different v3 shapes than the current code produces. This includes at minimum `MOF5-motif.cnnd` (confirmed), likely others.

**Strategy: keep `samples/` and `demolib/` on disk in v2 format; migrate at load time.** This avoids a re-hydration + snapshot-review ceremony every time a migration bug is found. The migration code path also stays continuously exercised by the real fixtures, which is better coverage than synthetic tests alone. Once the migration has been stable for a release or two, a single final re-hydration pass can drop the v2→v3 code path.

The current repo state has `samples/*.cnnd` and `demolib/*.cnnd` already re-saved as v3 (commits `9e1a0d38` and `b4e929e5`). Part of this fix is to revert them to v2:

1. Restore v2 copies of `samples/*.cnnd` and `demolib/*.cnnd` from the `pre-lattice-refactor` tag:
   ```
   git checkout pre-lattice-refactor -- samples/ demolib/
   ```
   The `pre-lattice-refactor` tag (commit `058258a3`) is the last commit on `main` before the lattice-space-refactoring merge; its files are guaranteed v2.
2. Do **not** load-and-save through the migration. Leave the files as v2 on disk.
3. Update snapshot tests that currently assert the v3-on-disk shape to instead assert the post-migration shape (i.e. migration output from the v2 file). `cargo insta review` the shifts and accept.

## Test plan

Extend `rust/tests/integration/lattice_space_migration_test.rs` and the fixtures under `rust/tests/fixtures/lattice_space_migration/`:

### Update existing fixtures

- **`atom_fill_split.cnnd`**: expected post-migration shape changes from "1 synthesized `structure` dangling" to "`get_structure` + `structure` + `with_structure` inline, wired into materialize". Rewrite the test's assertions against the new shape.
- **`shared_unit_cell.cnnd`**: same. The shared-unit-cell case now produces a chain with both the primitive adapter (phase 4) and the new W/G/S triplet (phase 5) composing cleanly. Assert the output of evaluating `materialize` against a known motif actually produces atoms.

### New fixtures

- **`motif_mof5.cnnd`** (smaller stand-in if the full MOF5 is unwieldy): row A coverage with both motif and offset effectively present. A custom network returning `Motif`, with an `atom_fill` on a side-chain display, a wired `motif` input to `atom_fill`, a wired `unit_cell` on the primitive, and a non-default motif. Post-migration, evaluate the materialize node and assert at least one atom of the motif's element is present in the output. This is the regression fixture — failure means we're back to diamond.
- **`atom_fill_unwired_shape.cnnd`**: row C coverage — motif wired, shape unwired. Assert `S` is created dangling, materialize has no shape wire, validation reports the missing shape as a user-visible error (not a migration failure).
- **`motif_offset_only_chained.cnnd`**: row A coverage with motif unwired and motif_offset wired (e.g. to a `vec3` literal). Specifically guards the `needs_S` broadening: assert `G` + `S` + `W` are emitted, `S.arguments[2]` (motif override) is empty, `S.arguments[3]` (motif_offset override) holds the user's wire, and evaluating materialize produces atoms shifted by the offset relative to the same fixture with offset unwired.
- **`motif_offset_only_unchained.cnnd`**: row C coverage with motif unwired and motif_offset wired. Same predicate-broadening guard as above but in the dangling-`S` branch: assert `S` is created dangling, `S.arguments[3]` holds the user's offset wire, `S.arguments[2]` is empty, and the offset wire is not silently overwritten by the re-index step.

### Real-sample smoke

Copy `MOF5-motif.cnnd` from `c:\atomcad_v0.3.0\samples\` or `git show main:samples/MOF5-motif.cnnd` into `rust/tests/fixtures/lattice_space_migration/real_mof5.cnnd`. Test: load, migrate, re-save, reload, idempotent. Assert the `motif_MOF5` network's materialize node, when evaluated, produces atoms whose elements include Zn / O / C (the MOF5 elements), not C-only (which would indicate diamond output).

### Negative / edge tests

- Double-migration idempotence on a Case-A file.
- Re-save round-trip byte-identity after first v3 save.
- `with_structure` rejects Crystal at validation time (static type check, not just runtime).

## Phased implementation plan

Four phases, each independently committable. Phases 1 and 2 ship standalone node features with no dependency on the migration rewrite; phase 3 depends on both nodes existing; phase 4 depends on the migration being correct.

### Phase 1 — `get_structure` node

**Scope:**
- Create `rust/src/structure_designer/nodes/get_structure.rs` per the spec above.
- Register in `nodes/mod.rs` and `node_type_registry.rs::create_built_in_node_types()`.
- Add unit tests at `rust/tests/structure_designer/get_structure_test.rs` (Blueprint, Crystal, static rejection of non-`HasStructure` source on the input pin, plus a defense-in-depth direct-`eval` test asserting `runtime_type_error_in_input(0)`).
- Add the node's entry to the existing node-type snapshot suite (`node_snapshots_test.rs`); `cargo insta review` and accept.

**Done means:** `cargo test get_structure` green, snapshot suite green, `cargo clippy` + `cargo fmt --check` green. The node is usable in the UI via the registry but not yet referenced by any migration code.

### Phase 2 — `with_structure` node

**Scope:**
- Create `rust/src/structure_designer/nodes/with_structure.rs` per the spec above, Blueprint-only.
- Register in `nodes/mod.rs` and `node_type_registry.rs`.
- Add unit tests at `rust/tests/structure_designer/with_structure_test.rs`:
  - Blueprint in + non-diamond Structure in → Blueprint out carries the new Structure; `geo_tree_root` preserved bit-for-bit.
  - Non-Blueprint `shape` input (Crystal) produces a runtime type error.
  - Static validator rejects Crystal on the `shape` pin (not just eval-time).
- Add the node's entry to `node_snapshots_test.rs`; `cargo insta review` and accept.

**Done means:** `cargo test with_structure` green, snapshot suite green, lint/format green. Node is registered but not yet used by migration.

### Phase 3 — Migration algorithm rewrite + fixtures

**Scope:**
- Rewrite `synthesise_structure_for_atom_fill` in `rust/src/structure_designer/serialization/migrate_v2_to_v3.rs` to emit the W/G/S triplet for Case A and handle cases B/C/D per the algorithm above.
- Bump `next_node_id` correctly (3 in A, 1 in C, 0 in B/D).
- Update existing migration fixtures under `rust/tests/fixtures/lattice_space_migration/`:
  - `atom_fill_split.cnnd`: expected-output assertions rewritten for the new shape.
  - `shared_unit_cell.cnnd`: same, plus assert materialize evaluation produces atoms against a known motif.
- Add new fixtures:
  - `motif_mof5.cnnd` (smaller stand-in if full MOF5 is unwieldy): row A regression fixture; post-migration evaluation must produce atoms of the motif's element, not diamond.
  - `atom_fill_unwired_shape.cnnd`: row C coverage with motif wired.
  - `motif_offset_only_chained.cnnd`: row A coverage with motif unwired and motif_offset wired — guards the `needs_S` broadening so the offset wire isn't silently dropped.
  - `motif_offset_only_unchained.cnnd`: row C coverage with motif unwired and motif_offset wired — same broadening guard in the dangling-`S` branch.
  - `real_mof5.cnnd`: copy from `c:\atomcad_v0.3.0\samples\MOF5-motif.cnnd` or `git show pre-lattice-refactor:samples/MOF5-motif.cnnd`. Assert materialize on `motif_MOF5` evaluates to atoms including Zn / O / C, not C-only.
- Add negative / edge tests: double-migration idempotence on Case A, re-save round-trip byte-identity after first v3 save, `with_structure` static rejection of Crystal.

**Done means:** full `cargo test` green including the new fixtures; MOF5 regression test proves motif survives migration; lint/format green. Migration produces correct v3 shapes for all four cases.

### Phase 4 — Sample / demolib restoration + snapshot acceptance

**Scope:**
- Restore v2 copies of the sample files: `git checkout pre-lattice-refactor -- samples/ demolib/`.
- Do **not** re-save through the app — files stay as v2 on disk and migrate at load time.
- Update snapshot tests that currently assert the v3-on-disk shape (`text_format_snapshot_test`, `node_snapshots_test`) to instead assert the post-migration shape. `cargo insta review` and accept.
- Final full green: `cargo test` + `cargo clippy` + `cargo fmt --check` + `flutter analyze`.

**Done means:** `samples/` and `demolib/` are v2 on disk; loading any of them in the app produces correct v3 in-memory graphs (MOF5 shows its real motif, not diamond); all snapshots reflect migration output, not on-disk shape; full test / lint / format green.

## Non-goals

- No behaviour change for `materialize`, `structure`, or any primitive.
- No change to the phase 4 primitive adaptation.
- No user-facing UI for the new nodes beyond the automatic registry entries.
- No Crystal-input support on `with_structure` — see rationale in the spec above.
- Fixing CSG operations to preserve full `Structure` (rather than dropping motif) is a separate design (`doc/design_csg_structure_propagation.md`) and is **not** a prerequisite for this migration fix. The `with_structure` insertion point is after all CSG ops, so the fix is correct regardless of CSG's current motif-stripping behaviour.
