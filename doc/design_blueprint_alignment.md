# Blueprint Alignment — Design Document

**Status:** Draft (design only, no code yet)
**Related docs:** `design_lattice_space_refactoring.md`, `design_phase_transitions_and_movement.md`, `design_multi_output_pins.md`

## 1. Problem

A Blueprint carries a `Structure` (lattice vectors + motif + motif offset) and a `geo_tree`. In the three-phase model, a Blueprint is intended to be the "cookie cutter" for a structure: it is meaningful only insofar as its geometry is registered to an infinite crystal field whose symmetry is fixed by the Structure.

The four movement nodes can break this registration:

| Node | Acts on | Lattice-safe? | Motif-safe? |
|---|---|---|---|
| `structure_move` | `StructureBound` | iff `translation` divisible by `lattice_subdivision` componentwise | same |
| `structure_rot`  | `StructureBound` | always (axis is picked from the unit cell's point group) | depends on the motif |
| `free_move`      | `Unanchored`    | no (arbitrary real-space translation) | no |
| `free_rot`       | `Unanchored`    | no (arbitrary axis, arbitrary angle) | no |

Downstream nodes (boolean CSG, `materialize`, `atom_edit`, …) assume their Blueprint inputs share a common lattice registration. When two Blueprints from "the same" structure differ by a non-lattice-vector shift or a non-symmetry rotation, unioning/intersecting them creates garbage atoms. Today the evaluator has no way to flag this.

We do **not** want to prevent these operations — they are useful (strained structures, defect studies, molecules carried as pseudo-Blueprints). We want to **surface the risk** so the user sees it in the editor and later tooling can refuse to do things that require lattice registration.

## 2. Terminology

We propose the term **alignment** for the relationship between a Blueprint's current state and the symmetry of its underlying Structure, with three values:

- `aligned` — lattice registration and motif registration both preserved.
- `motif_unaligned` — lattice translational symmetry still holds, but the motif may not map to itself under the applied operations. Boolean combinations with other `aligned` blueprints are still safe *as long as the atoms are not yet materialized*; after materialization the atoms may not all sit on motif sites.
- `lattice_unaligned` — the blueprint is no longer registered to any integer translation of the structure's lattice. This is a superset: anything lattice-unaligned is also motif-unaligned by construction.

We explicitly **avoid "mismatch"** because "lattice mismatch" is an established crystallographic term for two lattices at an interface having different periodicities. Overloading that term will bite us when we add real heterostructure support.

Suggested alternative names we considered and rejected: `incommensurate` (too strict mathematically — e.g., an irrational rotation is incommensurate but ours typically aren't), `detached` (evocative but ambiguous), `broken-symmetry` (too physics-y, and it conflates with electronic symmetry breaking elsewhere in the literature).

Short forms in code: `Alignment::Aligned`, `Alignment::MotifUnaligned`, `Alignment::LatticeUnaligned`. Totally-ordered, so propagation is `max`.

## 3. When each node changes alignment

### 3.1 `structure_move`

`structure_move` computes `subdivided_translation = translation / lattice_subdivision`. The resulting real-space translation is a lattice vector iff each component of `subdivided_translation` is an integer, i.e. iff

```
translation.x % sub == 0 && translation.y % sub == 0 && translation.z % sub == 0
```

- If the check passes: **alignment unchanged**.
- If it fails: the blueprint is shifted by a fractional lattice vector. Atoms no longer sit on the Structure's atom sublattice → **promote to `lattice_unaligned`**.

(Rare edge case worth noting but not handling: if the fractional translation happens to coincide with a legitimate motif internal translation symmetry, alignment is technically preserved. We don't attempt to detect this. The conservative answer is correct.)

### 3.2 `structure_rot`

`structure_rot` always picks its rotation axis from `analyze_unit_cell_symmetries(unit_cell)`, so the lattice is preserved by construction. The rotation may or may not be a motif symmetry:

- If the rotation maps every motif site (with correct element and bond topology) to another motif site, modulo lattice translations → **alignment unchanged**.
- Otherwise → **promote to at least `motif_unaligned`**.

The detection algorithm is in §5. For common cases (diamond/zincblende in cubic lattice with zero motif offset), many lattice axes are also motif symmetries, so this check passes most of the time.

If the Structure's `motif_offset` is non-zero, the motif's center is not on a lattice point. A rotation around a lattice-point pivot will then generally NOT preserve the motif even if it's a valid lattice axis. This is how motif offset enters the picture — not as an alignment state in itself, but as a parameter that shrinks the set of motif-preserving rotations.

### 3.3 `free_move`

Unconditionally **promote to `lattice_unaligned`**. A zero translation is the only safe case, and we don't bother to special-case it (the node exists precisely because the user wanted a real-space translation; if they wanted zero they wouldn't have added the node).

### 3.4 `free_rot`

Same: unconditionally **promote to `lattice_unaligned`**.

### 3.5 Geometry primitives with `subdivision` (half_space, extrude, drawing_plane, half_plane, facet_shell)

These take a `subdivision` integer that lets the plane/extrusion sit at a fractional d-spacing. **They do NOT affect alignment.** The subdivision parameter controls the *cutting* geometry, not where atoms end up — atoms are always placed on motif sites during materialization. A fractional-d-spacing cut just decides which atoms survive the cookie cutter.

This is worth calling out in the doc because intuition says "subdivision = fractional = misaligned", but that's wrong for these nodes. Only `structure_move` uses subdivision to subdivide a *translation*, and there subdivision really can break lattice alignment.

### 3.6 Structure-producing nodes (`structure`, `lattice_vecs`, `motif`)

These don't produce Blueprints, so there is no alignment state to attach. The `motif_offset` input pin on `structure` sets a constant property of the Structure itself — every Blueprint sprouting from this Structure shares that offset as its canonical zero. It is NOT an alignment perturbation.

(If we later add a `motif_shift` node that mutates the offset downstream of a Structure construction, that node would become an alignment-relevant operation. See §10 for future work.)

### 3.7 Boolean CSG (`union`, `intersect`, `diff`)

These take an array of Blueprints that must already pass `all_have_compatible_lattice_vecs` (checked at `union.rs:85`, `intersect.rs:83`, `diff.rs:93`). This compatibility check is about the Structure's lattice vectors matching, NOT about alignment. It's orthogonal.

Propagation rule: **output alignment = max over input alignments.** A union of `aligned` and `motif_unaligned` is `motif_unaligned`; a union with any `lattice_unaligned` input is `lattice_unaligned`.

### 3.8 Phase transitions

- `materialize` (Blueprint → Crystal): Crystal inherits the Blueprint's alignment. The fact that atoms have been placed does not change alignment — only the *consequences* of misalignment change (e.g., `lattice_unaligned` atoms are no longer on motif sites at materialization time).
- `dematerialize` (Crystal → Blueprint): alignment preserved.
- `exit_structure` (Crystal → Molecule): Molecules have no structure, so alignment is dropped. But: if the crystal was not `aligned`, the atoms are already "off" relative to the discarded structure, which is fine since we're discarding it.
- `enter_structure` (Molecule + Structure → Crystal): the molecule's atoms may not sit on the structure's motif sites. This should produce **`lattice_unaligned`** on the output Crystal conservatively. (We don't run the expensive check "do these real-space atom positions happen to lie on this motif"; that's what `infer_bonds` / proximity checks are for elsewhere.)

### 3.9 `atom_edit` and atomic ops

`atom_edit` operates on Crystals/Molecules. Its output pin 0 is `SameAsInput`. **Alignment passes through unchanged** — atom_edit does not degrade alignment.

Reasoning: the primary use case for `atom_edit` on a Crystal is defect modelling (vacancies, substitutions, local relaxation probes). In those workflows the crystal is still aligned in every meaningful sense, and the blanket "non-empty diff → `lattice_unaligned`" rule would falsely flag every defect study. The principled alternative — actually checking whether each added/moved atom coincides with a motif site within tolerance — is possible but adds non-trivial detection logic on the hot path for a UX-only signal.

Known limitation: a user can do a "wild" atom_edit that moves atoms far off their motif sites, and the output Crystal will still report `Aligned`. This is acceptable because the alignment flag is currently a UX signal, not a gate on operations. If later consumers start refusing to run on non-aligned crystals, we revisit this (see §10).

`atom_union`, `atom_cut`, `relax`, `add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `apply_diff`, `atom_composediff`: same rule — pass alignment through unchanged. For `atom_union` on an array, the output alignment is `max` over input alignments (same rule as boolean CSG).

### 3.10 Summary table

| Operation | Alignment effect |
|---|---|
| construction (any shape node) | `aligned` |
| `structure_move`, divisible | pass-through |
| `structure_move`, not divisible | `max(in, lattice_unaligned)` |
| `structure_rot`, motif symmetry | pass-through |
| `structure_rot`, not motif symmetry | `max(in, motif_unaligned)` |
| `free_move`, `free_rot` | `max(in, lattice_unaligned)` |
| `union`, `intersect`, `diff` | `max` over inputs |
| `materialize`, `dematerialize` | pass-through |
| `exit_structure` | dropped (Molecules have no alignment) |
| `enter_structure` | `lattice_unaligned` |
| `atom_edit` | pass-through (see §3.9 for rationale) |
| `atom_union` | `max` over inputs |
| other atomic ops (`relax`, `add_hydrogen`, `atom_replace`, …) | pass-through |

## 4. Data model changes

### 4.1 Rust — payload structs

In `rust/src/structure_designer/evaluator/network_result.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Alignment {
    Aligned,
    MotifUnaligned,
    LatticeUnaligned,
}

impl Alignment {
    pub fn worsen_to(&mut self, other: Self) { *self = (*self).max(other); }
}

pub struct BlueprintData {
    pub structure: Structure,
    pub geo_tree_root: GeoNode,
    pub alignment: Alignment,   // NEW
}

pub struct CrystalData {
    pub structure: Structure,
    pub atoms: AtomicStructure,
    pub geo_tree_root: Option<GeoNode>,
    pub alignment: Alignment,   // NEW
}
```

Molecules have no structure reference so no alignment field is added to `MoleculeData`.

Default value at construction sites: `Alignment::Aligned`. Every existing call site that builds a `BlueprintData` or `CrystalData` literal has to be updated. That is a mechanical but wide diff (grep-able: every shape primitive, `materialize`, `dematerialize`, `union`, etc.).

### 4.2 FRB / Flutter API

In `rust/src/api/structure_designer/structure_designer_api_types.rs`, extend `OutputPinView`:

```rust
pub struct OutputPinView {
    pub name: String,
    pub data_type: String,
    pub resolved_data_type: Option<String>,
    pub index: i32,
    pub alignment: Option<AlignmentView>,   // NEW: None for non-Blueprint/Crystal
}

pub enum AlignmentView { Aligned, MotifUnaligned, LatticeUnaligned }
```

Populated in `structure_designer_api.rs` where `OutputPinView` is built, by reading the evaluated `NetworkResult` for this pin (which `NodeSceneData::pin_outputs` already carries).

### 4.3 Nothing on `Structure`

Motif symmetry is not cached. `structure_rot` runs the detection in §5 on each eval. For real motifs (diamond, zincblende, etc.) this is a few thousand float ops — dominated by the surrounding evaluator overhead. If this ever shows up in a profile we can revisit, but caching here would add an `OnceCell`, invalidation thinking around `motif_offset` edits, and a field that's a derived quantity of the other three — not worth the complexity up front.

## 5. Motif-symmetry detection algorithm

Given a `structure_rot` with `(axis_index, step, pivot_point)` on a Structure with lattice `L`, motif `M`, motif offset `o`:

1. Build the rotation in real space. `axis` is from `analyze_unit_cell_symmetries`; `n = axes[axis_index].n_fold`; angle = `step * 2π / n`.
2. Let `R = DMat3::from_axis_angle(axis, angle)`.
3. For every motif site `s` with fractional coord `f_s` and element `e_s`:
   a. Compute site real-space position: `r = L·f_s + o`.
   b. Rotate around pivot: `r' = R·(r - pivot_real) + pivot_real`.
   c. Subtract motif offset and convert back to fractional: `f' = L⁻¹·(r' - o)`.
   d. Reduce mod 1: `f'_reduced = f' - floor(f')`.
   e. Seek a site `s'` in the motif with `||f'_reduced - f_{s'}||∞ < TOL` (use the existing `LENGTH_EPSILON`/`ANGLE_EPSILON` tolerances scaled for fractional coords, or reuse `fract_distance` from `io/cif/symmetry.rs:272`) and `e_{s'} == e_s`.
   f. If none found → rotation is not a motif symmetry. **Return false.**
4. Repeat for bonds — **this is required, not optional**. A motif can have site-symmetric but bond-asymmetric arrangements (e.g. two equivalent atoms with a directional bond between them, or bonds of different order that would permute under the rotation). Site preservation alone does not imply motif preservation.

   For each `MotifBond` `(site_i, site_j, cell_offset, order)`:
   a. Let `(i', j', cell_offset')` be the image of the bond's endpoints and inter-cell offset under the rotation (the mapping from step 3 gives you `i'`, `j'`, and any fractional-coordinate wrap contributes to a shifted `cell_offset'`).
   b. Check that the motif contains a bond `(i', j', cell_offset', order)` (treat endpoints as unordered per `BondReference`).
   c. If any bond has no image → **return false**.
5. All sites and all bonds mapped → rotation is a motif symmetry. **Return true.**

Complexity: O(|sites|² + |bonds|²) per rotation per structure (bond comparison does at most |bonds| lookups against the motif's existing bond table — use `bonds_by_site1_index`/`bonds_by_site2_index` for O(1) lookups to drop this to O(|sites|² + |bonds|)). For diamond motif (8 sites, ~16 bond entries) × ~13 lattice axes × few steps per axis ≈ a few thousand ops per Structure per `structure_rot.eval()`. Still cheap enough to run directly without caching (§4.3).

The pivot cancels out in fractional coordinates only when it's at the motif's rotation center, which isn't generally true with nonzero `motif_offset`. So the pivot matters and must be included.

Bond check note: in the `Motif` struct, bonds are `(site_i, site_j, IVec3 cell_offset)`. After rotation, both the endpoint mapping and the cell offset rotate (cell_offset rotates with the lattice). Require that the rotated-and-translated bond exists in the motif's bond list.

**Where to call this:** directly in `structure_rot.eval()`, before applying the rotation. The detection takes `(structure, axis_index, step, pivot_point)` and returns a `bool`. If `false`, the output's alignment is at least `motif_unaligned`.

## 6. UI

### 6.1 Wire line style (node network editor)

Today `NodeNetworkPainter._drawWire()` draws a cubic Bezier with a solid stroke whose color is `getDataTypeColor(dataType)`. All wires solid, no dash support.

Proposed dash dictionary (driven by alignment of the source output value):

| Alignment | Line style |
|---|---|
| `Aligned` (or N/A, e.g. Int, Motif, Geometry2D) | solid |
| `MotifUnaligned` | long dashes (≈ 10-px dash, 4-px gap) |
| `LatticeUnaligned` | short dashes or dots (≈ 3-px dash, 3-px gap) |

Rationale for the ordering: `lattice_unaligned` is the "more broken" state, so give it the visually more fragmented style. "More broken up = more broken" is the mnemonic.

Implementation: add a `PathMetric`-based dash renderer to `_drawWire()` that walks the Bezier's arc-length and emits segments. This is ~30 LOC; Flutter's canvas has no built-in `dashPattern` for `Path`. See `dashPath` in `path_drawing` package if we add a dependency, or inline it (preferred — we don't need a dependency for this).

The alignment value arrives on Flutter side via `OutputPinView.alignment` (§4.3). Painter picks up: `alignment = sourceNode.outputPins[pinIndex].alignment`.

**Selection and hover:** selected wires still get the deep-orange glow; dashes are preserved under the glow stroke.

### 6.2 Output-pin tooltip

`PinViewWidget` in `lib/structure_designer/node_network/node_widget.dart:62` already builds a tooltip message from pin name + data type + output string preview. Extend it to append a **colored** alignment line for Blueprint/Crystal pins:

```
── result ──  Blueprint
Alignment: motif-unaligned          ← brown
  (structure_rot by an axis that is not a motif symmetry)
{... output string preview ...}
```

Colors:

| Alignment | Tooltip color |
|---|---|
| `Aligned` | default text color (no highlight) |
| `MotifUnaligned` | **brown** |
| `LatticeUnaligned` | **orange** |

Rationale for the two-color gradient: lattice-unaligned is the more severe state, so it gets the more alarming orange; motif-unaligned is the softer warning, so brown reads as "caution, but less urgent". Brown is also visually distinct from the orange shades in the data type palette (Bool / Int / Float), so the two roles don't collide. For orange specifically, reuse the existing `WIRE_COLOR_SELECTED = 0xFFD84315` so we don't invent a third orange.

**Flutter implementation.** `Tooltip` supports `richMessage: InlineSpan` as a first-class alternative to `message: String` — no package dependency. Refactor the tooltip builder in `PinViewWidget` to return a `List<TextSpan>` instead of a single `String`, and pass `Tooltip(richMessage: TextSpan(children: spans))`. `message` and `richMessage` are mutually exclusive, so the migration is all-at-once for this widget.

Example span list:
```dart
TextSpan(children: [
  TextSpan(text: '── result ──  Blueprint\n'),
  TextSpan(
    text: 'Alignment: motif-unaligned\n',
    style: TextStyle(color: Color(0xFF6D4C41)),  // brown
  ),
  TextSpan(text: '...output preview...'),
])
```

The parenthetical reason string is optional but helpful — for this we'd need each node to report not just the new alignment value, but a short reason string when it degrades alignment. For the first cut, just show the colored alignment value; reasons can come later (phase 4).

### 6.3 Output-pin shape

The output pin itself doubles as the alignment indicator. The pin is too small to hold both a circle and a badge, so we **replace the pin's circle with a warning-triangle-with-exclamation-mark glyph** in both unaligned states. Rules:

| Alignment | Pin shape |
|---|---|
| `Aligned` (or N/A) | filled circle (current) |
| `MotifUnaligned` | warning triangle (⚠), filled |
| `LatticeUnaligned` | warning triangle (⚠), filled |

The triangle is filled with the **same data type color** as the circle would have been — we are not giving up the type-color channel. A single shape covers both unaligned states because the wire dash style (§6.1) already distinguishes them, and the tooltip (§6.2) names the exact case. Division of labor:

- **Pin shape:** "is this output aligned?" (binary)
- **Wire dash:** "how unaligned?" (three-level)
- **Tooltip:** "why?" (full detail)

Precedent: abstract input pins already render as non-circular pie-sliced shapes, so we already have per-pin custom shape rendering in the widget. The triangle should fit within the circle's existing bounding box so wire-endpoint math and hit-testing continue to work unchanged.

Tone: the triangle is recognisable as "warning", which is the right call-out for an output whose downstream consumers may misbehave — but the tooltip and surrounding UI must treat it as information, not an error. Some workflows deliberately want unaligned blueprints.

This replaces the badge-overlay idea; it's cleaner and uses real estate we already have.

## 7. Propagation at the evaluator level

`NetworkResult` carries the alignment inside the payload (because it's the value that's (mis)aligned, not the wire). The evaluator does not need to know about alignment at all; each node's `eval()` is responsible for reading input alignments, combining, and writing the output alignment.

Concretely, every node that currently re-wraps `BlueprintData { structure, geo_tree_root }` needs to also carry `alignment` through. The `structure_move` / `structure_rot` / `free_move` / `free_rot` nodes additionally compute a new alignment from the old one plus their operation (see §3).

Boolean CSG (`union.rs`, `intersect.rs`, `diff.rs`) reduces `max` over input alignments.

`materialize.rs` reads `blueprint.alignment` and writes it into the output Crystal.

Nothing about the wire struct changes. Alignment is purely an emergent property of the value flowing through.

## 8. Serialization (.cnnd)

`BlueprintData` and `CrystalData` are not directly serialized — they are recomputed from the node graph on load. So no `.cnnd` schema changes are required for alignment. (Good — this is a pure-derived quantity.)

## 9. Phased rollout

**Phase 1 — backend plumbing (small, low-risk).**
- Add `Alignment` enum + field to `BlueprintData` and `CrystalData`.
- Default all construction sites to `Aligned`.
- `free_move` / `free_rot` always set `lattice_unaligned`.
- `structure_move` checks divisibility; `structure_rot` always keeps alignment (initially — we defer motif detection).
- Boolean CSG does max-propagation.
- `materialize` / `dematerialize` propagate.
- `atom_edit` and other atomic ops pass alignment through unchanged (see §3.9).
- Tests: unit tests in `rust/tests/structure_designer/` covering each node's alignment transition.

**Phase 2 — motif-symmetry detection.**
- Implement the §5 algorithm as a free function (e.g. `fn rotation_preserves_motif(structure, axis_index, step, pivot) -> bool` in `crystolecule/unit_cell_symmetries.rs` or a new sibling file).
- `structure_rot.eval` calls it and degrades alignment when it returns `false`.
- Tests with diamond (8 sites, zero offset) and non-zero offset cases.

**Phase 3 — Flutter UI.**
- Add `alignment: Option<AlignmentView>` to `OutputPinView` + FRB regen.
- Dashed wire rendering in `NodeNetworkPainter`.
- Tooltip line in `PinViewWidget`.
- Optional: badge icon on output pin.

**Phase 4 — reason strings and polish.**
- Per-node reason string ("non-symmetry axis", "fractional translation by (1, 0, 0)/2", etc.).
- Tooltip shows reason under alignment line.
- Optional: a "show alignment in subtitle" preference.

## 10. Open questions / future work

- **Refining `atom_edit`:** if alignment ever becomes a gate on downstream operations (rather than a pure UX signal), we will need to detect when an atom_edit genuinely moves atoms off motif sites. Algorithm: for every added or moved atom in the diff, check if its position coincides with a motif site within tolerance; if any doesn't → `lattice_unaligned`. Deletes and bond-only edits preserve alignment. Deferred until there is a real consumer.
- **`motif_shift` node:** if we add a node that perturbs `motif_offset` downstream, it becomes another alignment source. Rule: a shift by a fractional lattice vector of the motif coordinates is a motif symmetry (just a reindexing of sites) and alignment-preserving; any other shift is `lattice_unaligned`. We're not building this now.
- **`enter_structure` precision:** the current rule says always `lattice_unaligned`. Optionally, if the molecule's atom positions happen to sit on motif sites of the given structure within tolerance, we could report `aligned`. This is a separate detection problem.
- **Implicit dependency of `structure_rot` alignment on `pivot_point`:** a rotation around pivot A may preserve the motif while the same rotation around pivot B may not. Our mask check (§5) takes pivot into account. Cache key must include pivot in lattice coordinates, not just axis+step.
- **UX for "user meant to drift":** some workflows (strained-layer heterostructures, testing defect dynamics) deliberately want unaligned Blueprints. The UI should surface alignment but not shame users for using it. Treat dashes as *information*, not *warnings*.
- **Interaction with abstract pin types:** an abstract `StructureBound` input could be satisfied by either Blueprint or Crystal; the wire's alignment is whichever the concrete value has. No special-casing needed.

## 11. Not in scope

- Detecting heterostructure lattice mismatch between two *different* Structures at an interface. (Distinct problem; different vocabulary.)
- Snap-to-symmetry autocorrect for free_move / free_rot.
- Rewriting free_move as structure_move when the user happens to type in a lattice vector.
- Changes to `.cnnd` format.
