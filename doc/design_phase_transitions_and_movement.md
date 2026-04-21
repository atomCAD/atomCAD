# Phase 7: Phase Transitions, Movement Nodes, and Flutter API

This document refines **Phase 7** of the lattice-space refactoring
(`doc/design_lattice_space_refactoring.md`). Phases 1–6 are implemented;
Phase 7 introduces the three-phase model's transition operators, replaces
the old movement nodes with the new polymorphic set, and exposes the new
node surface end-to-end to Flutter so the refactor is fully testable.

The `.cnnd` migration script is deferred to Phase 8.

## Scope Summary

| Concern | Phase 7 action |
|---|---|
| `structure_move`, `structure_rot` | **Repurpose** existing `lattice_move`/`lattice_rot` |
| `free_move`, `free_rot` | **Repurpose** existing `atom_move`/`atom_rot` |
| `atom_lmove`, `atom_lrot` | Registrations removed (data/evaluator already shared — see below) |
| `atom_move`, `atom_rot` (old names) | Gone (names absorbed into `free_move`/`free_rot`) |
| `atom_trans` | Deleted outright |
| `materialize` | **Rename** of `atom_fill`, with inputs trimmed |
| `dematerialize`, `exit_structure`, `enter_structure` | New small nodes |
| Amber warning on `free_move`(Blueprint) | **Deferred** (noted as open UX work) |
| Flutter API / Dart UI | Regenerated to expose the new node set |

The overarching insight: three of the four new movement nodes already
exist in the codebase in near-final form. They just need **renaming +
abstract-typed input + concrete-preserving output + a small dispatch
tweak**. This keeps Phase 7 mostly mechanical.

## Key Codebase Findings

These findings drive the rename-vs-new-node decisions below. Verified by
reading the sources before writing this doc.

### `lattice_move` and `atom_lmove` already share implementation

`nodes/lattice_move.rs` defines a single `LatticeMoveData { translation,
lattice_subdivision, is_atomic_mode: bool }` and two `get_node_type_*`
factory functions that both hand out this data. The `eval()` branches on
`is_atomic_mode`:

- `is_atomic_mode == false` → input is Blueprint, output is Blueprint with
  a `GeoNode::transform` wrapped around the geo tree.
- `is_atomic_mode == true` → input is `Atomic` (Crystal | Molecule),
  output is the same concrete variant via `map_atomic`, with atom
  positions transformed.

Exactly the same structure in `nodes/lattice_rot.rs` for
`lattice_rot` / `atom_lrot`.

**Consequence:** unifying them into a single `structure_move` that
accepts `HasStructure` (Blueprint | Crystal) is mostly a matter of
rewriting the dispatch to branch on concrete `NetworkResult` variant
instead of `is_atomic_mode`, plus removing the `atom_lmove`
registration. The atomic branch already supports Crystal (phase 6 split
added it to `map_atomic`). What we *remove* is Molecule support from
this code path — Molecule becomes a type error at validation time.

Three additional observations drive the Phase 7a design:

1. **Atoms are stored in real-space coordinates.** Both branches
   already compute a single real-space `Transform` and apply it
   uniformly: `lattice_move.rs:139,149-151` converts lattice →
   real via `unit_cell.dvec3_lattice_to_real(...)` and feeds the same
   `real_translation` into `atoms.transform(...)` (atomic branch) and
   `GeoNode::transform(Transform::new(real_translation, ...))`
   (Blueprint branch). `lattice_rot.rs` does the same with
   `real_rotation_quat` + `pivot_real`. So for the unified Crystal
   branch, feeding the same `Transform` to both `atoms.transform` and
   `GeoNode::transform` on the geo shell composes naturally — no
   coordinate conversion gotcha.

2. **The `unit_cell` input pin on `atom_lmove`/`atom_lrot` becomes
   redundant.** It existed because `DataType::Atomic` carries no
   structure info. `HasStructure` inputs always provide
   `structure.lattice_vecs` inline, so `structure_move` and
   `structure_rot` can drop that pin entirely — a simplification over
   both predecessors.

3. **Current Blueprint output drops motif and motif_offset.** Both
   `lattice_move.rs:167-168` and `lattice_rot.rs:197-198` rebuild the
   output structure via
   `Structure::from_lattice_vecs(shape.structure.lattice_vecs.clone())`,
   which discards `motif` and `motif_offset`. That was fine before
   phase 5, but post-phase-5 Blueprints carry motif, and downstream
   `materialize` now reads motif from `BlueprintData.structure`. So
   `structure_move`/`structure_rot` must use `shape.structure.clone()`
   instead — a latent-bug fix bundled into Phase 7a.

### `atom_move` and `atom_rot` already operate on `Atomic`

`nodes/atom_move.rs` accepts `DataType::Atomic`, uses `map_atomic`, and
works unchanged for Crystal or Molecule. It does **not** currently
accept Blueprint.

**Consequence:** to repurpose as `free_move`, input becomes
`HasFreeLinOps` (Blueprint | Molecule), output becomes
`SameAsInput("molecule")` (renamed to `input` — see pin names below),
and we add a Blueprint branch that wraps geo tree with
`GeoNode::transform`, mirroring what `lattice_move` does in its Blueprint
branch. Crystal becomes a type error.

### `atom_fill` is not "pure carving"

The main-doc text says `materialize` should be "parameterless" / "pure
carving". Reality check: `atom_fill` carries seven semantic inputs —
shape, motif, m_offset, passivate, rm_single, surf_recon, invert_phase
— plus a parameter-element-value definition string. Passivation,
surface reconstruction, and inversion are genuine materialization
options; dropping them would be a regression.

However, phase 3 already introduced `Structure` as a value type
carrying `lattice_vecs + motif + motif_offset`. After phase 5,
primitives output Blueprints whose `BlueprintData.structure` already
holds motif and motif offset. So:

- `motif` and `m_offset` inputs are **now redundant** on atom_fill —
  they should be removed and read from `BlueprintData.structure`
  instead.
- `passivate`, `rm_single`, `surf_recon`, `invert_phase`, and
  `parameter_element_value_definition` stay — they are materialization
  options, not structure properties.

This matches the main-doc open question "passivation and surface
reconstruction settings — parameters on `materialize` or separate
Blueprint modifier nodes?" with the pragmatic answer "on `materialize`
for now". Factoring them into separate Blueprint modifier nodes can
happen later without touching the three-phase type system.

### Flutter API surface is small

`rust/src/api/structure_designer/structure_designer_api.rs` has an
`APIDataTypeBase` enum that already includes `Blueprint`, `Atomic`,
`Structure`, `LatticeVecs`. Phase 6 added `Crystal` and `Molecule`
internally but — need to verify — they may already be exposed. Phase 7
Flutter work is:

1. Ensure `APIDataTypeBase` covers `Crystal`, `Molecule`,
   `HasStructure`, `HasFreeLinOps`.
2. Regenerate FRB bindings after any node-registry or data-type changes.
3. Update `dart format` / Dart node-palette entries for the renamed and
   new nodes (mostly string-level).
4. Audit Dart code for hardcoded references to old node type names
   (`atom_fill`, `atom_move`, `lattice_move`, `atom_lmove`, etc.).

No new API *functions* are required — the existing network-editing
APIs already dispatch by node type name.

## Node-by-Node Design

### Movement Nodes

All four are **`is_atomic_mode`-flag eliminations** combined with
abstract-typed input + concrete-preserving output.

#### `structure_move` (from `lattice_move` + `atom_lmove`)

- Source files: `nodes/lattice_move.rs`, data struct `LatticeMoveData`.
- Action:
  - Rename file to `nodes/structure_move.rs`, struct to
    `StructureMoveData`. Delete `is_atomic_mode` field.
  - Single `get_node_type()` factory. Remove
    `get_node_type_atom_lmove`. Remove `atom_lmove` import and
    registration in `node_type_registry.rs`.
  - Pin 0 renamed to `input`, type `DataType::HasStructure`. Keep
    `translation` (IVec3) and `subdivision` (Int) pins.
  - **Drop the `unit_cell` pin** that `atom_lmove` carried (pin 3 in
    the old signature). `HasStructure` always provides
    `structure.lattice_vecs` inline, so the separate pin is redundant.
    Final pin list: `input`, `translation`, `subdivision` — three
    pins, matching the old `lattice_move` shape.
  - Output: `OutputPinDefinition::single_same_as("input")`.
  - `eval()`: one unified path.
    - Read `unit_cell` from `input.structure.lattice_vecs` on both
      branches (no more pin-3 fallback to `UnitCellStruct::cubic_diamond()`).
    - Compute `real_translation = unit_cell.dvec3_lattice_to_real(
      translation.as_dvec3() / subdivision as f64)` once.
    - `Blueprint(shape)` → wrap `geo_tree_root` with
      `GeoNode::transform(Transform::new(real_translation, IDENTITY), ...)`.
      **Preserve the full `structure`**: use `shape.structure.clone()`,
      not `Structure::from_lattice_vecs(...)`. The current
      `lattice_move` rebuilds via `from_lattice_vecs`, which drops
      `motif` and `motif_offset` — a latent bug since phase 5, because
      downstream `materialize` now reads motif from
      `BlueprintData.structure`. Fixing this is part of Phase 7a.
    - `Crystal(crystal)` → `atoms.transform(&IDENTITY, &real_translation)`
      **and** wrap `geo_tree_root` (when `Some`) with the same
      `GeoNode::transform`. Preserve `structure` unchanged. Atoms are
      stored in real-space coordinates (verified in
      `lattice_move.rs:149-151`), so the same real-space `Transform`
      applies cleanly to both — no coordinate conversion gotcha.
    - `Molecule(_)` / anything else → runtime_type_error_in_input(0).
      (Abstract-type validation at wire time should already prevent
      Molecule from connecting.)
  - Eval cache: populate `LatticeMoveEvalCache { unit_cell }` from
    `input.structure.lattice_vecs` on both branches — single code
    path, no conditional on `is_atomic_mode`.
  - Gadget: existing `LatticeMoveGadget` works unchanged.

#### `structure_rot` (from `lattice_rot` + `atom_lrot`)

Same structural changes as `structure_move`:

- Drop the `unit_cell` pin (pin 4 in old `atom_lrot`). Final pins:
  `input`, `axis_index`, `step`, `pivot_point`.
- Read `unit_cell` from `input.structure.lattice_vecs` on both
  branches; compute `symmetry_axes` and `real_rotation_quat` once.
- Compute `pivot_real = unit_cell.ivec3_lattice_to_real(pivot_point)`
  once; build `Transform::new_rotation_around_point(pivot_real,
  real_rotation_quat)`.
- Blueprint branch: wrap `geo_tree_root` with that `Transform`,
  preserve full `structure` via `shape.structure.clone()` (same
  motif-drop fix as `structure_move`).
- Crystal branch: apply the three-step pivot rotation to `atoms`
  (translate to origin, rotate, translate back — as in
  `lattice_rot.rs:166-172`) **and** wrap `geo_tree_root` (when `Some`)
  with the rotation-around-pivot `Transform`. Preserve `structure`.
- Molecule / other → runtime_type_error_in_input(0).
- Eval cache populated from `input.structure.lattice_vecs` on both
  branches.

#### `free_move` (from `atom_move`)

- Source: `nodes/atom_move.rs`, struct `AtomMoveData`.
- Action:
  - Rename file to `nodes/free_move.rs`, struct to `FreeMoveData`.
  - Pin 0 renamed from `molecule` to `input`, type changed from
    `DataType::Atomic` to `DataType::HasFreeLinOps`.
  - Output: `OutputPinDefinition::single_same_as("input")` (already
    same_as-style; just pin-name rename).
  - `eval()`: dispatch on concrete variant.
    - `Blueprint(shape)` → wrap `geo_tree_root` with
      `GeoNode::transform`, preserve `structure` unchanged (the
      structure stays; the geometry cutter moves off-lattice). Return
      Blueprint.
    - `Molecule(mol)` → transform atoms **and** geo shell (when
      present) with the translation. Return Molecule.
    - `Crystal(_)` → runtime_type_error_in_input(0) (abstract-type
      validation prevents this at wire time).
  - Gadget: existing `AtomMoveGadget` works unchanged (world-aligned
    XYZ).
- Note: the "amber warning" when the Blueprint cutter drifts off its
  structure is intentionally deferred (see Open Questions).

#### `free_rot` (from `atom_rot`)

Identical pattern to `free_move`. Rotation.

### Phase Transitions

#### `materialize` (rename of `atom_fill`)

- Source: `nodes/atom_fill.rs`, struct `AtomFillData`.
- Action:
  - Rename file to `nodes/materialize.rs`, struct to `MaterializeData`.
  - **Remove input pins** `motif` (pin 1) and `m_offset` (pin 2). These
    are now supplied by `BlueprintData.structure.motif` and
    `BlueprintData.structure.motif_offset`.
  - **Remove fields** `motif_offset` from the data struct (passes
    through from Structure now). Keep
    `parameter_element_value_definition`,
    `parameter_element_values`, `available_parameters`,
    `hydrogen_passivation`,
    `remove_single_bond_atoms_before_passivation`,
    `surface_reconstruction`, `invert_phase`, `error`.
  - Pin 0 stays `shape: Blueprint`. Remaining scalar input pins
    renumber: `passivate` (1), `rm_single` (2), `surf_recon` (3),
    `invert_phase` (4). Pin-name backward compat in text format is not
    a concern — Phase 8's migration script handles old `.cnnd` files.
  - `eval()`: pull motif and motif offset from
    `blueprint.structure`, pass to `LatticeFillConfig` exactly as
    before. Output pin already `Crystal` (set in phase 6). No other
    logic change.
  - Backward-compat deserialization: since we're removing a field
    (`motif_offset`) that used to be required, existing `.cnnd` files
    would fail deser. **Decision**: don't write a compat shim in the
    loader — the "no incremental backward compat on main" philosophy
    applies to user-facing file migration, which is Phase 8's job.
    For in-repo test fixtures, regenerate them as part of this
    sub-phase so `cnnd_roundtrip` stays green. User `.cnnd` files
    predating the rename will fail to load until Phase 8 ships; this
    is an accepted cost on a feature branch but **main must not land
    7b without 7c/8 or fixture regeneration keeping CI green**.

#### `dematerialize` — new

- File: `nodes/dematerialize.rs`, struct `NoData` style (no parameters).
- Pin 0: `input: Crystal`, output: `Blueprint`.
- `eval()`: strips `atoms`, returns `Blueprint { structure,
  geo_tree_root: crystal.geo_tree_root.unwrap_or(<empty geo>) }`.
  Question: if `geo_tree_root` is `None` (Crystal without geometry
  shell), what does dematerialization produce? Answer: error. A
  Blueprint *must* have geometry. Emit
  `NetworkResult::Error("dematerialize: Crystal has no geometry to
  return to Blueprint")`.

#### `exit_structure` — new

- File: `nodes/exit_structure.rs`, `NoData`.
- Pin 0: `input: Crystal`, output: `Molecule`.
- `eval()`: drops `structure`, returns
  `Molecule { atoms, geo_tree_root }`. No other transformation.

#### `enter_structure` — new

- File: `nodes/enter_structure.rs`, `NoData`.
- Pins: `input: Molecule`, `structure: Structure`. Output: `Crystal`.
- `eval()`: constructs `Crystal { structure, atoms, geo_tree_root }`.
  Pure packaging; does not re-snap atoms to lattice positions.

### Removals

- **`atom_trans`** — delete entirely. Its role (continuous
  translation+rotation of atomic structures) is fully covered by
  `free_move` + `free_rot` on Molecule.
- **`atom_lmove` / `atom_lrot` factories** — the `get_node_type_atom_*`
  functions are removed from `nodes/lattice_move.rs` /
  `nodes/lattice_rot.rs` (those files themselves renamed to
  `structure_move.rs` / `structure_rot.rs`).
- **`atom_move` / `atom_rot` files** — renamed/absorbed into
  `free_move.rs` / `free_rot.rs`.
- **`atom_fill`** — renamed to `materialize.rs`.

After Phase 7 the following node files exist:
`structure_move.rs`, `structure_rot.rs`, `free_move.rs`, `free_rot.rs`,
`materialize.rs`, `dematerialize.rs`, `exit_structure.rs`,
`enter_structure.rs`.

Node files deleted: `atom_trans.rs`. Node files *not* present anymore by
those names: `lattice_move.rs`, `lattice_rot.rs`, `atom_move.rs`,
`atom_rot.rs`, `atom_fill.rs`.

### `lattice_symop`

Not touched in Phase 7. It's a Blueprint-only space-group transformation
node and remains valid. (It could be renamed to `structure_symop` for
consistency later but that's cosmetic.)

## Implementation Order (Sub-phases)

Each sub-phase leaves the tree compiling and tests passing.

**7a. Repurpose movement nodes.**
Rename the four files, collapse the `is_atomic_mode` dispatch,
abstract-typed inputs + concrete-preserving outputs. Update registry,
tests, snapshots. Delete `atom_trans`.

**7b. Rename `atom_fill` → `materialize`, strip redundant inputs.**
File rename, field removal, pin removal. Update registry. Regenerate
affected snapshots.

**7c. Add phase transition nodes.**
New files for `dematerialize`, `exit_structure`, `enter_structure`.
Register. Tests for each.

**7d. Flutter API + Dart UI.**
Ensure `APIDataTypeBase` is complete. Run `flutter_rust_bridge_codegen
generate`. Update Dart node-palette entries for renamed/new nodes.
`dart format`, `flutter analyze`. Smoke test in the UI.

Subphases 7a–7c each map to their own commit. 7d may span multiple
commits if Dart-side work is substantial.

## Testing Plan

- **Unit**: new snapshot tests for each of the 8 final-state nodes
  under `rust/tests/structure_designer/nodes/snapshots/`. At minimum: a
  minimal network that produces a visible scene per node. For
  polymorphic nodes, one snapshot per concrete input variant
  (structure_move has Blueprint + Crystal; free_move has Blueprint +
  Molecule).
- **Type-system**: wire-validation tests that Crystal cannot connect
  to `free_move` and Molecule cannot connect to `structure_move`.
- **Roundtrip**: `cargo test cnnd_roundtrip` — in-repo fixtures
  referencing the old node names or the old `atom_fill` pin shape
  are **regenerated in the same sub-phase that renames the node**
  (7a for movement, 7b for materialize). Each sub-phase lands with
  roundtrip green. Add new fixtures for the transition nodes
  introduced in 7c.
- **Undo**: each new/renamed node must work with undo. Since the data
  structs are reused (LatticeMoveData → StructureMoveData, etc.) and
  the generic commands don't know node-type names, the undo tests
  should pass without changes beyond updating any hardcoded node-type
  strings in tests.
- **Flutter smoke**: after 7d, run the app and manually verify the
  Blueprint → materialize → structure_move → exit_structure → free_move
  pipeline renders correctly.

## Complexity Assessment

| Sub-phase | Files touched | New logic | Risk |
|---|---|---|---|
| 7a | ~8 src + snapshots | none (dispatch rewrite only) | low — shared data structs already prove the merge is safe |
| 7b | 2 src (rename + trim) | field/pin removal | low — logic unchanged |
| 7c | 3 new src files | pure `NetworkResult` repackaging | very low |
| 7d | FRB + Dart | regen + audit strings | low-medium — depends on Dart-side string density |

**Overall: lower complexity than Phase 6.** Phase 6 introduced new
type-system machinery (abstract types, SameAsInput). Phase 7 only
*consumes* that machinery and mostly renames / re-dispatches existing
code. No new concepts; no new invariants.

## Open Questions / Deferred Work

- **Amber warning for `free_move` / `free_rot` on Blueprint.** The
  main doc specifies an amber warning when a Blueprint cutter drifts
  off its structure. Deferred: this is a UX/feedback concern that
  belongs with diagnostics work, not with the type-system refactor.
  Tracking note in the main doc's Open Questions section.
- **`lattice_symop` naming.** Consider renaming to `structure_symop`
  for naming consistency. Not in Phase 7.
- **Factoring `passivate` / `surf_recon` out of `materialize`.** The
  main doc asks whether these should live on separate Blueprint
  modifier nodes. Phase 7 keeps them on `materialize` to preserve
  existing functionality verbatim. Revisit post-Phase 8.
- **Crystal without geometry in `dematerialize`.** Handled as an error
  at eval time. An alternative would be to forbid this at the type
  level (introduce `CrystalWithGeo` subtype) but that's
  over-engineering for a rare case.
- **`enter_structure` semantics.** Pure packaging vs snapping atoms to
  nearest lattice points. Phase 7 takes the "pure packaging" position:
  `enter_structure` only re-associates structure info; atoms stay where
  they are. Snapping, if desired, becomes a separate `snap_to_lattice`
  node later.
