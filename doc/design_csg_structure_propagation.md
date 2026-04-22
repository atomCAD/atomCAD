# CSG Operations — Propagate Full `Structure` (Not Just Lattice)

## Status

Bug-fix design. Scope: the three Blueprint CSG nodes — `union`, `intersect`, `diff`. Independent of the migration fix in `doc/design_cnnd_migration_motif_fix.md`; either can land first.

## Problem

Today, `union`, `intersect`, and `diff` accept any set of input Blueprints whose `lattice_vecs` are approximately equal, silently discard the motif and motif_offset from every input, and emit a Blueprint whose `Structure` has only the shared lattice and a **default (zincblende) motif**. Specifically:

- `rust/src/structure_designer/nodes/union.rs:87, 92, 106`
- `rust/src/structure_designer/nodes/intersect.rs:85, 89, 103`
- `rust/src/structure_designer/nodes/diff.rs:115, 191, 195, 210`

All three build their output with `Structure::from_lattice_vecs(first_lattice_vecs)`. The `from_lattice_vecs` constructor (`rust/src/crystolecule/structure.rs:24`) attaches `DEFAULT_ZINCBLENDE_MOTIF` and zero `motif_offset`. Any motif or motif_offset the user had set upstream is erased.

This is semantically wrong. Within a single crystal design, CSG on Blueprints is carving different shapes **out of the same infinite crystal field**. "Same crystal field" means the same `Structure` — lattice, motif, and motif_offset. Combining shapes from two different structures has no meaningful interpretation at materialize time: the carved region's atoms come from which motif? The current behaviour silently picks zincblende, which is almost never what the user wanted.

## Fix

Require **full `Structure` equality** across all CSG inputs. Propagate the shared `Structure` to the output unchanged.

### Semantic rule

A CSG node's output Blueprint carries a `Structure` `S` where:
- All input Blueprints' structures equal `S` (field-by-field, with appropriate tolerances).
- On the first input's structure mismatching any other input's, the node returns `structure_mismatch_error()` instead of a Blueprint.
- If there are no inputs (empty array for `union`/`intersect`, or empty subtrahend set for `diff`), the existing empty-input error paths stay unchanged.

Lattice compatibility is a consequence of full Structure equality — no separate lattice check remains.

### What "equal" means

Add `Structure::is_approximately_equal(&self, other: &Structure) -> bool` on `rust/src/crystolecule/structure.rs`. Semantics:

- `self.lattice_vecs.is_approximately_equal(&other.lattice_vecs)` — reuses the existing `UnitCellStruct::is_approximately_equal` (already used by `BlueprintData::has_compatible_lattice_vecs`).
- `self.motif == other.motif` — structural equality on `Motif`.
- `self.motif_offset.abs_diff_eq(other.motif_offset, 1e-9)` — `glam` tolerance-aware equality on `DVec3`.

`Motif` does not currently derive `PartialEq` (check `rust/src/crystolecule/motif.rs:33`). Adding `#[derive(PartialEq)]` is acceptable if every field supports it; otherwise implement `PartialEq` explicitly, comparing:
- `sites: Vec<MotifSite>` by full-field equality (fractional coords with `1e-9` tolerance, element/label exact).
- `bonds: Vec<MotifBond>` by set equality (order-insensitive, because a motif's bond list has no intrinsic order).
- `parameters` by full-field equality.

If defining strict `PartialEq` on `Motif` is awkward because of ordering or floating-point fields, add a dedicated `Motif::is_approximately_equal(&Motif) -> bool` instead and have `Structure::is_approximately_equal` call it. The CSG nodes should call `Structure::is_approximately_equal` regardless — do not inline comparisons in the node files.

### New helper on `BlueprintData`

Replace `BlueprintData::all_have_compatible_lattice_vecs` (file `rust/src/structure_designer/evaluator/network_result.rs:187`) with:

```rust
pub fn all_have_same_structure(blueprints: &[BlueprintData]) -> bool {
    if blueprints.len() <= 1 { return true; }
    let first = &blueprints[0].structure;
    blueprints.iter().skip(1).all(|bp| first.is_approximately_equal(&bp.structure))
}
```

Delete `all_have_compatible_lattice_vecs` and `has_compatible_lattice_vecs` if no other callers remain. Search for remaining callers before deleting — grep `is_approximately_equal`/`compatible_lattice` across `rust/src/` to confirm.

### New error constructor

Add alongside `unit_cell_mismatch_error` in `rust/src/structure_designer/evaluator/network_result.rs:968`:

```rust
pub fn structure_mismatch_error() -> NetworkResult {
    NetworkResult::Error("Structure mismatch: CSG inputs must share the same lattice, motif, and motif_offset.".to_string())
}
```

Keep `unit_cell_mismatch_error` only if some other node still needs a lattice-only mismatch; otherwise remove it after migrating callers.

## Node changes

### `union` (`rust/src/structure_designer/nodes/union.rs`)

Replace the lattice compatibility block (lines ~87–92) and the output construction (lines ~105–110):

- Change `all_have_compatible_lattice_vecs` call to `all_have_same_structure`.
- On mismatch, return `structure_mismatch_error()` instead of `unit_cell_mismatch_error()`.
- Replace `structure: Structure::from_lattice_vecs(first_lattice_vecs)` with `structure: blueprints[0].structure.clone()`.
- Drop the now-unused `first_lattice_vecs` local.
- Remove the `use crate::crystolecule::structure::Structure;` import if no other references remain.

### `intersect` (`rust/src/structure_designer/nodes/intersect.rs`)

Same three edits, same lines (~85, ~89, ~103).

### `diff` (`rust/src/structure_designer/nodes/diff.rs`)

This node has two compatibility checks — one for the primary input's shape, one for the subtracted shapes collection. Both must use `all_have_same_structure`. The output Structure is the primary input's structure (lines ~191–210). Specifically:

- Line ~191: replace the lattice-only check with a full-Structure check across the full input set (primary + subtracted).
- Line ~195: no longer extract `first_lattice_vecs` for the output; use the primary input's Structure directly.
- Line ~210: change the `structure: Structure::from_lattice_vecs(...)` line to `structure: primary.structure.clone()` (or whatever the local variable holding the primary input is named — preserve the existing flow).

`diff` is subtly different from `union`/`intersect` because its subtracted shapes don't contribute atoms (they only carve away geometry), but the same Structure-equality rule still applies to them: subtracting a shape from a *different* crystal field is nonsensical.

## Tests

Follow the no-inline-test convention. All new tests go under `rust/tests/`.

### New structure equality helper

`rust/tests/crystolecule/structure_test.rs` (create if it doesn't exist; register in `rust/tests/crystolecule.rs`):

- Two identical structures are equal.
- Structures differing only in `motif_offset` by more than 1e-9 are not equal.
- Structures differing in lattice (beyond the existing tolerance) are not equal.
- Structures differing in motif (different site count, site element, site position, bond set, parameter list) are not equal — one test per field.
- Two empty motifs are equal regardless of any non-motif field difference elsewhere in the Structure? No — the test asserts field-by-field AND. Do not accept vacuously-equal motifs.

### New CSG behaviour

Add a file per node (or one file with three modules) under `rust/tests/structure_designer/`, registered in `rust/tests/structure_designer.rs`:

- **`csg_structure_propagation_test.rs`**:
  - `union_preserves_shared_structure`: two Blueprints with identical non-default Structure → output's Structure equals input's (motif field by field, motif_offset bit-equal to 1e-9).
  - `union_rejects_motif_mismatch`: two Blueprints, same lattice, different motifs → error. Assert the error message mentions "Structure mismatch".
  - `union_rejects_lattice_mismatch`: existing behaviour preserved, now routed through the Structure check.
  - `union_rejects_motif_offset_mismatch`: same lattice and motif, different offsets → error.
  - Same four cases duplicated for `intersect_*` and `diff_*`.
  - `diff_rejects_subtracted_structure_mismatch`: primary and subtracted with different motifs → error.
  - `single_input_union`: one-Blueprint union passes through the input's Structure (trivially; the `len() <= 1` fast path).

### Existing tests

Grep for `all_have_compatible_lattice_vecs`, `unit_cell_mismatch_error`, and `Structure::from_lattice_vecs` across `rust/tests/`. Any test that constructs Blueprints with diamond-default motifs and runs them through `union`/`intersect`/`diff` will continue to pass (they all share the zincblende default). Tests that deliberately supplied mismatched lattices to assert the old error will need the error-string expectation updated to "Structure mismatch".

### Snapshot tests

`union`, `intersect`, `diff` node-type signature snapshots don't change (no parameter or output-pin changes). `text_format_snapshot_test` and `node_snapshots_test` should be unaffected. If `cargo insta review` flags unexpected shifts, investigate before accepting.

## User-visible impact

Files that previously evaluated silently under the old (buggy) behaviour may now surface a `structure_mismatch_error` at validation or eval time. Specifically, a network that unioned two primitives whose upstream `structure` adapters carried different motifs would have been reduced to diamond silently; now it errors. This is the desired behaviour — the old result was never correct.

No known production file ships with genuinely mismatched structures into a CSG op. The MOF5 case is the motivating example and it has a single consistent Structure throughout; it will pass the new check cleanly.

## Interaction with the migration fix

None required. The migration fix (`doc/design_cnnd_migration_motif_fix.md`) inserts `with_structure` immediately before `materialize`, so the Structure seen by `materialize` is determined by the migration-inserted node regardless of what CSG does with structures upstream. The two fixes are independent and can land in either order.

However: once the migration fix lands, migrated files that route the motif through `with_structure` will **not** depend on CSG preserving the motif, because the motif never rides through the CSG chain in those files — it is injected at the end. This means this CSG fix primarily benefits newly-authored v3 networks that route motifs through primitives into CSG ops, which is the idiomatic v3 pattern.

## Implementation order

1. Add `Structure::is_approximately_equal` (plus `Motif::is_approximately_equal` or `PartialEq` as decided).
2. Add tests for `Structure::is_approximately_equal`.
3. Add `BlueprintData::all_have_same_structure`; delete `all_have_compatible_lattice_vecs` after confirming no other callers.
4. Add `structure_mismatch_error`.
5. Update `union`, `intersect`, `diff` to use the new helper, return the new error, and propagate `blueprints[0].structure.clone()` to the output.
6. Add CSG propagation/rejection tests.
7. Full `cargo test` + `cargo clippy` + `cargo fmt --check` green.

## Non-goals

- No semantic change to `materialize`, primitives, or any non-CSG node.
- No relaxation of the equality check (e.g., "same lattice, ignore motif differences"). Equal-or-error is the point.
- No attempt to "merge" compatible-but-unequal Structures (e.g., union of identical motifs but different motif_offsets). If the offsets differ, the crystal fields differ; error.
- No change to `atom_union` or any HasAtoms-level CSG (those operate on already-materialized atoms, not Blueprints, and have different semantics).
