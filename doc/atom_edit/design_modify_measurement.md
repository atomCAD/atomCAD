# Modify Measurement Feature — Design Document

## Overview

The atom_edit node's default tool displays a measurement card when 2–4 atoms are selected: **distance** (2 atoms), **angle** (3 atoms), or **dihedral angle** (4 atoms). This feature adds a **"Modify"** button to that card. Pressing it opens a dialog where the user can enter a precise target value and the structure is adjusted accordingly — atoms are moved along bond axes, rotated around vertices, or rotated around torsion axes.

This is a standard capability in molecular modeling tools (Avogadro, GaussView, SAMSON). It bridges the gap between free-form dragging (imprecise) and parametric guided placement (only at creation time), giving the user exact numerical control over internal coordinates post-placement.

## Motivation

1. **Precision**: Free-form dragging can't achieve exact bond lengths, angles, or dihedrals. Numerical entry can.
2. **Exploration**: Users can sweep parameters (e.g. dihedral scan) by entering successive values.
3. **Correction**: After energy minimization or import, a specific geometric parameter may need manual adjustment.
4. **Teaching**: Students can observe how changing one internal coordinate propagates through a structure.

---

## Common Design Elements

### Fragment Selection Algorithm

All three cases share a core question: when one atom moves, which other atoms should move with it? The answer uses **graph-theoretic distance** (BFS shortest path through the bond graph):

1. For every atom X in the structure, compute:
   - `d_move` = shortest path length from X to the **moving atom** M
   - `d_fixed` = shortest path length from X to the **reference atom** F
2. X moves with M if `d_move < d_fixed`.
3. X stays fixed if `d_move > d_fixed`.
4. Ties (`d_move == d_fixed`): X stays fixed (conservative default — keeps more of the structure stable).

This algorithm:
- Naturally handles **cycles** (rings): atoms on M's side of the ring move, others don't.
- Naturally handles **disconnected fragments**: unreachable atoms have infinite distance to both, so they stay fixed.
- Requires no special-casing for bond removal or graph partitioning.
- Is O(N) via two BFS passes (one from M, one from F).

The "move connected atoms" option is **on by default**. When off, only the single selected atom moves (or in the dihedral case, only the end atom of the chain).

### Move-Atom Default Selection

The dialog pre-selects the **last-selected atom** as the moving atom. This is the most intuitive default: the user clicks atoms in sequence, and the most recently clicked one is the one they're "thinking about" moving.

- **2 atoms (distance)**: The last-selected atom is pre-selected as the moving atom.
- **3 atoms (angle)**: The last-selected non-vertex atom is pre-selected as the moving arm. (If the last-selected atom happens to be the vertex, fall back to the second-to-last.)
- **4 atoms (dihedral)**: The end of the chain (A or D) closest to the last-selected atom in the chain ordering is pre-selected.

### Prerequisite: Selection Order Tracking

**The current selection system does not track selection order.** `AtomEditSelection` stores atoms in `HashSet<u32>` (both `selected_base_atoms` and `selected_diff_atoms`), which is unordered. The `compute_selection_measurement` function iterates these sets in arbitrary order.

This must be addressed before the Modify feature can use "last-selected" as the default. The implementation should add a `Vec<(SelectionProvenance, u32)>` alongside the hash sets, maintaining insertion order. Since entries are removed on toggle-off and the vec is cleared on Replace/clear, it only ever contains currently-selected atoms — its size is implicitly bounded by the selection size. No explicit capacity limit is needed.

### Dialog Structure (Common)

All three dialogs share a common layout:

```
┌─────────────────────────────────────────────┐
│  Modify [Distance / Angle / Dihedral]       │
├─────────────────────────────────────────────┤
│                                             │
│  Value: [___1.545___] Å / °    [Default]    │
│                                             │
│  Move atom: ○ C₁ (id 3)  ● C₂ (id 7)      │
│                                             │
│  ☑ Move connected fragment                  │
│                                             │
├─────────────────────────────────────────────┤
│                     [Cancel]  [Apply]       │
└─────────────────────────────────────────────┘
```

- **Value field**: Pre-filled with the current measured value. The user can type any value. Units shown next to the field (Å for distance, ° for angles).
- **Default button**: Sets the value field to the force-field equilibrium value (details per case below).
- **Move atom selector**: Chooses which atom (or side) moves. Details per case below.
- **Move connected fragment checkbox**: Whether to drag the graph-theoretic fragment along. On by default.
- **Cancel / Apply**: Cancel discards; Apply executes the modification as a diff operation.

### Integration with the Diff System

All modifications operate on the **result structure** (applied diff). The actual position changes are written back to the **diff** via `move_in_diff()`, which preserves anchor positions for undo semantics. This is the same mechanism used by drag-to-move in the default tool.

### Triggering the Dialog

The "Modify" button appears in the existing blue measurement card, next to the value display. It is **always enabled** for all three measurement types (distance, angle, dihedral). For distance, the "Default" button inside the dialog is disabled when the two atoms are not bonded (no bond order → no meaningful equilibrium length).

---

## Case 1: Distance Modification (2 Atoms Selected)

### Precondition

None. The Modify button is always enabled when two atoms are selected. The two atoms do not need to be bonded — translation along the connecting axis is well-defined regardless. When the atoms are not bonded, the only difference is that the "Default" button in the dialog is disabled (no bond → no equilibrium length).

### Input Fields

| Field | Description |
|-------|-------------|
| **Distance** | Target distance in Å. Pre-filled with current distance (3 decimal places). |
| **Default button** | Sets the value to the crystal or UFF equilibrium bond length (see below). Disabled when atoms are not bonded. |
| **Move atom** | Radio: which of the two bonded atoms moves. Labeled with element symbol and atom ID (e.g. "C (id 3)"). |
| **Move connected fragment** | Checkbox, default on. |

### Default Bond Length

The "Default" button computes the equilibrium bond length using the **same system as guided atom placement**:

1. **Crystal mode** (if available): Look up in the `CRYSTAL_BOND_LENGTHS` table using both atoms' atomic numbers. This table covers ~20 sp3 semiconductor material pairs (C-C diamond = 1.545 Å, Si-Si = 2.352 Å, etc.).
2. **UFF mode** (fallback): Use `calc_bond_rest_length(bond_order, params_i, params_j)` from the UFF parameter table, using the **actual bond order** of the existing bond (not hardcoded to single). This means a double bond will have a shorter default than a single bond.

Which mode is used: the current bond length mode setting on the atom_edit node (the same Crystal/UFF toggle used by the Add Atom tool). If Crystal mode is selected but the element pair isn't in the table, UFF is used as fallback.

### Movement Geometry

The moving atom (and its fragment, if enabled) translates along the **bond axis**:

```
axis = normalize(position_moving - position_fixed)
delta = (target_length - current_length) * axis
new_position = old_position + delta     (for all atoms in the moving fragment)
```

This is a pure translation — no rotation. All moved atoms shift by the same vector.

### Fragment Selection

- **Moving atom** M = the atom the user chose to move.
- **Reference atom** F = the other (fixed) atom.
- BFS from M and BFS from F → move atoms closer to M.

### Edge Cases

- **Bond length = 0**: Reject (atoms would overlap). Minimum should be enforced (e.g. 0.1 Å).
- **Very large values**: Allow, but warn above some threshold (e.g. > 5 Å) since this would likely break bonding.
- **Ring bond**: Fragment algorithm handles this correctly — some atoms on M's side of the ring move, others don't. The ring geometry will be distorted (this is expected and matches other molecular editors).

---

## Case 2: Angle Modification (3 Atoms Selected)

### Vertex Identification

The measurement system already identifies the **vertex atom** V and the two **arm atoms** A₁ and A₂ (using bonding heuristics then geometric fallback). The dialog uses this same assignment.

### Input Fields

| Field | Description |
|-------|-------------|
| **Angle** | Target angle in degrees. Pre-filled with current angle (1 decimal place). |
| **Default button** | Sets the value to the UFF equilibrium angle `theta0` for the vertex atom (see below). |
| **Move atom** | Radio: which arm atom moves (A₁ or A₂). The vertex V is always fixed. Labeled with element symbol and atom ID. |
| **Move connected fragment** | Checkbox, default on. |

### Default Angle

The "Default" button sets the angle to the UFF `theta0` parameter for the vertex atom's UFF type:

| Vertex UFF type | theta0 | Geometry |
|-----------------|--------|----------|
| C_3 (sp3 carbon) | 109.471° | Tetrahedral |
| C_R, C_2 (sp2 carbon) | 120.0° | Trigonal planar |
| C_1 (sp carbon) | 180.0° | Linear |
| N_3 (sp3 nitrogen) | 106.7° | Pyramidal |
| O_3 (sp3 oxygen) | 104.51° | Bent |
| Si3 (sp3 silicon) | 109.471° | Tetrahedral |

The vertex atom's UFF type is determined by `assign_uff_type()` from its element and bond connectivity — the same function used by the energy minimizer.

### Movement Geometry

The moving arm atom (and its fragment) **rotates** around an axis through the vertex V:

```
rotation_axis = normalize((A_moving - V) × (A_fixed - V))
rotation_angle = target_angle - current_angle
```

All atoms in the moving fragment rotate around V along this axis. The rotation preserves distances from V (it's a rigid rotation, not a scaling).

If the three atoms are collinear (cross product ≈ 0), any perpendicular axis works — pick an arbitrary one from the null space.

### Fragment Selection

- **Moving atom** M = the arm atom the user chose to move.
- **Reference atom** F = the vertex atom V.
- BFS from M and BFS from V → move atoms closer to M.

This correctly keeps V fixed, keeps the other arm and its fragment fixed, and rotates M's entire sub-branch.

### Edge Cases

- **Angle = 0° or 180°**: Allow. 180° = linear; 0° = folded back (physically unusual but geometrically valid).
- **Collinear atoms**: Cross product is zero. Pick any perpendicular axis. Should show a note in the UI that the rotation plane is arbitrary.

---

## Case 3: Dihedral Angle Modification (4 Atoms Selected)

### Chain Identification

The measurement system already identifies the **chain A-B-C-D** where A and D are end atoms, B and C are center atoms, and the dihedral is measured as the angle between planes A-B-C and B-C-D. The dialog uses this same chain.

### Input Fields

| Field | Description |
|-------|-------------|
| **Dihedral angle** | Target dihedral in degrees (-180° to 180°). Pre-filled with current value (1 decimal place). |
| **Default button** | Sets the value to a common equilibrium dihedral (see below). |
| **Move atom** | Radio: which end rotates — "A-side" or "D-side". Labeled with element symbol and atom ID of the end atom. |
| **Move connected fragment** | Checkbox, default on. |

### Default Dihedral — Postponed

The "Default" button is **not shown** for the dihedral case in the initial implementation. The equilibrium dihedral depends on the hybridization of both central atoms B and C, the bond order between them, sp2 end-atom special cases, group 16 element rules, and the torsion potential has multiple minima (e.g. sp3-sp3: 60°, 180°, -60°). This requires extracting and refactoring logic from the UFF force field builder (`compute_torsion_params`). It can be added in a follow-up iteration without changing the dialog layout — just show the button when the computation becomes available.

### Movement Geometry

The moving end (and its fragment) **rotates** around the **B-C axis**:

```
rotation_axis = normalize(C - B)
rotation_center = B   (any point on the B-C line works)
rotation_angle = target_dihedral - current_dihedral
```

All atoms in the moving fragment rotate rigidly around the B-C axis. B and C themselves are on the axis, so even if they fall in the moving fragment, their positions are unchanged by the rotation.

### Fragment Selection

- **Moving atom** M = the end atom on the side the user chose (A or D).
- **Reference atom** F = the end atom on the opposite side (D or A).
- BFS from M and BFS from F → move atoms closer to M.

B and C may end up in either fragment (B is typically closer to A, C to D). Since both are on the rotation axis, their classification doesn't affect the result — rotation around an axis leaves points on that axis unchanged.

### Edge Cases

- **Collinear B-C**: If B and C are at the same position (degenerate), the rotation axis is undefined. This should be detected and an error shown ("Center atoms overlap — cannot define rotation axis").
- **Wraparound**: The dihedral angle is periodic. When computing rotation, use the **shortest rotation path** (don't rotate 350° when -10° gives the same result). Actually, no — use the literal `target - current` since the user may intentionally want the "long way around." The value field accepts -180° to 180°, matching the measurement output.

---

## UI / UX Details

### Modify Button Placement

The Modify button is placed inside the existing blue measurement card, right-aligned:

```
┌──────────────────────────────────────────────────────┐
│ 📏 Distance: 1.545 Å                     [Modify]   │
└──────────────────────────────────────────────────────┘
```

For all three types, the button is always enabled. (For distance between non-bonded atoms, the **Default** button inside the dialog is disabled — not the Modify button itself.)

### Dialog as Modal

The modify dialog is a standard Flutter `showDialog()` modal. It blocks interaction with the viewport while open. This is appropriate because:
- The operation is a discrete, confirm-or-cancel action.
- The viewport state (selection, measurement) shouldn't change while the dialog is open.

### Atom Labels in the Dialog

Atoms are labeled as `Element (id N)` where Element is the chemical symbol and N is the result-space atom ID. Examples: "C (id 3)", "Si (id 12)", "H (id 45)". This matches the atom ID display elsewhere in the application.

For the move-atom radio buttons:
- **2 atoms (distance)**: `○ Move C (id 3)  ● Move Si (id 7)`
- **3 atoms (angle)**: `Vertex: N (id 5)` (shown as label, not selectable), then `○ Move C (id 3)  ● Move O (id 8)` for the two arms.
- **4 atoms (dihedral)**: `○ Move A-side: C (id 1)  ● Move D-side: H (id 10)`. Show the full chain "A(1)—B(2)—C(5)—D(10)" as a label above.

### Validation

- **Bond length**: Minimum 0.1 Å. No hard maximum, but warn above 5 Å.
- **Angle**: 0° to 180° inclusive.
- **Dihedral**: -180° to 180° inclusive.
- Empty or non-numeric input: Apply button disabled.

### Preview (Future Enhancement)

A possible future enhancement: live preview in the viewport as the user types or uses arrow keys in the value field. This would require the dialog to be non-modal or use a side panel instead. Not in scope for the initial implementation.

---

## Interaction with Existing Features

### Diff System

All modifications write through `move_in_diff()`. If an atom hasn't been moved before, this creates an anchor (storing the original position). If it has been moved, the anchor remains at the original position. This means:
- Undo = revert the diff (remove the movement entries).
- Sequential modifications accumulate correctly.

### Energy Minimization

After modifying a bond length/angle/dihedral, the user can run energy minimization to relax the rest of the structure. The fragment-following algorithm already does a good job of maintaining local geometry, but minimization can clean up any strain.

### Selection Stability

The modify operation doesn't change the selection. After applying, the same atoms remain selected, the measurement card updates to reflect the new value, and the user can immediately modify again (e.g. for iterative adjustment).

---

## Summary Table

| Property | Distance (2 atoms) | Angle (3 atoms) | Dihedral (4 atoms) |
|----------|-------------------|-----------------|-------------------|
| **Precondition** | None | None | None |
| **Value range** | ≥ 0.1 Å | 0°–180° | -180°–180° |
| **Default** | Crystal / UFF bond length (disabled if not bonded) | UFF theta0 for vertex | Postponed (button hidden) |
| **Motion type** | Translation along bond axis | Rotation around axis through vertex | Rotation around B-C axis |
| **Atom choice** | Which of the 2 atoms moves | Which arm moves (vertex fixed) | Which end rotates (B-C fixed) |
| **Fragment ref atom** | The fixed atom | The vertex | The opposite end atom |

---

## Implementation Plan

The implementation is split into six phases. Each phase produces a testable, self-contained increment. Phases 1–4 are pure Rust (no UI), Phase 5 bridges Rust to Flutter, and Phase 6 is pure Flutter UI.

---

### Phase 1: Selection Order Tracking

**Goal**: `AtomEditSelection` tracks the order in which atoms were selected, so the dialog can default to "move the last-selected atom."

**Files to modify**:
- `rust/src/structure_designer/nodes/atom_edit/types.rs` — Add ordered buffer to `AtomEditSelection`
- `rust/src/structure_designer/nodes/atom_edit/selection.rs` — Push to ordered buffer on every selection change
- `rust/src/structure_designer/nodes/atom_edit/default_tool.rs` — Marquee selection: append in deterministic order (e.g. by atom ID)

**Design**:
- Add `selection_order: Vec<(SelectionProvenance, u32)>` to `AtomEditSelection`, where `SelectionProvenance` is `Base` or `Diff`. Each click-select appends to this vec; `Replace` modifier clears it first; `Toggle` removing an atom also removes it from the vec; marquee appends all newly added atoms sorted by ID.
- On `clear()`, the vec is also cleared.
- Add `pub fn last_selected_atoms(&self, count: usize) -> Vec<(SelectionProvenance, u32)>` helper that returns the last N entries.
- The existing `HashSet` fields remain unchanged — they continue to be the source of truth for "is this atom selected?". The vec is a supplementary ordering structure.

**Tests** (`rust/tests/structure_designer/`):
- Selection order is maintained across add/toggle/replace/clear operations.
- Marquee selection appends in ID order.
- Removing an atom via toggle removes it from the order vec.

**Serialization**: The order vec is transient state (not serialized), same as the rest of `AtomEditSelection`.

---

### Phase 2: BFS Fragment Selection Algorithm

**Goal**: Given a moving atom M and a reference atom F in an `AtomicStructure`, compute the set of atom IDs that should move with M.

**Files to create/modify**:
- `rust/src/crystolecule/atomic_structure/fragment.rs` (new) — The BFS fragment algorithm
- `rust/src/crystolecule/atomic_structure/mod.rs` — Add `mod fragment;` and re-export

**Design**:
```rust
/// Compute the fragment of atoms that are graph-theoretically closer to `moving_atom`
/// than to `reference_atom`. Uses BFS shortest path through the bond graph.
///
/// Returns a HashSet of atom IDs that should move (always includes `moving_atom`).
/// `reference_atom` is never included. Ties go to the fixed side.
/// Atoms unreachable from both (disconnected components) stay fixed.
pub fn compute_moving_fragment(
    structure: &AtomicStructure,
    moving_atom: u32,
    reference_atom: u32,
) -> HashSet<u32>
```

**Algorithm**:
1. BFS from `moving_atom` → `dist_m[x]` for all reachable atoms.
2. BFS from `reference_atom` → `dist_f[x]` for all reachable atoms.
3. Return `{ x | dist_m[x] < dist_f[x] }`.

Both BFS passes are O(V + E). For typical molecular structures (E ≈ 2V for sp3), this is effectively O(N).

**Tests** (`rust/tests/crystolecule/`):
- Linear chain: A—B—C—D, moving A ref B → fragment {A}.
- Ring: 6-membered ring, moving atom 1 ref atom 4 → fragment {1, 2, 6} (nearest 3).
- Branched tree: correct partitioning at branch points.
- Disconnected component: disconnected atoms stay fixed.
- Single bond (2 atoms): moving atom's fragment is just itself.
- Same atom for M and F: return empty set (edge case guard).

---

### Phase 3: Measurement Modification Operations

**Goal**: Three Rust functions that modify atom positions to achieve a target distance, angle, or dihedral. These are pure geometry operations on the diff, with optional fragment following.

**Files to create/modify**:
- `rust/src/structure_designer/nodes/atom_edit/modify_measurement.rs` (new) — The three modification functions
- `rust/src/structure_designer/nodes/atom_edit/mod.rs` — Add `mod modify_measurement;` and re-export

**Design**:

Each function follows the three-phase borrow pattern established by `operations.rs`:

```rust
/// Which atom to move in a 2-atom distance modification.
pub enum DistanceMoveChoice {
    /// Move the first atom in the measurement (atoms[0]).
    First,
    /// Move the second atom in the measurement (atoms[1]).
    Second,
}

/// Which arm to move in a 3-atom angle modification.
pub enum AngleMoveChoice {
    /// Move arm A (the first non-vertex atom).
    ArmA,
    /// Move arm B (the second non-vertex atom).
    ArmB,
}

/// Which end to rotate in a 4-atom dihedral modification.
pub enum DihedralMoveChoice {
    /// Rotate the A-side (chain[0] end).
    ASide,
    /// Rotate the D-side (chain[3] end).
    DSide,
}

pub fn modify_distance(
    structure_designer: &mut StructureDesigner,
    target_distance: f64,
    move_choice: DistanceMoveChoice,
    move_fragment: bool,
) -> Result<(), String>

pub fn modify_angle(
    structure_designer: &mut StructureDesigner,
    target_angle_degrees: f64,
    move_choice: AngleMoveChoice,
    move_fragment: bool,
) -> Result<(), String>

pub fn modify_dihedral(
    structure_designer: &mut StructureDesigner,
    target_angle_degrees: f64,
    move_choice: DihedralMoveChoice,
    move_fragment: bool,
) -> Result<(), String>
```

**Prerequisite refactoring**: The code that builds a `Vec<SelectedAtomInfo>` from the current `AtomEditSelection` is currently embedded in the measurement-display path. This must be extracted into a reusable helper (e.g. `fn selected_atom_infos(&self) -> Vec<SelectedAtomInfo>` on the relevant context struct) so that both the measurement card and the modify functions can call it without duplication.

**Internal flow** (same for all three):
1. **Phase 1 (Gather)**: Build the `SelectedAtomInfo` list from the current selection (via the extracted helper). Call `compute_measurement()` from `measurement.rs` — this returns `MeasurementResult`, which already carries atom role assignments: `vertex_index` for angles and `chain: [usize; 4]` for dihedrals. These are indices into the `SelectedAtomInfo` array, giving both the role mapping and the positions. Validate that the right number of atoms are selected.
2. **Phase 2 (Compute)**: Use the role indices from `MeasurementResult` to identify which atom is the moving atom and which is the reference atom (based on the `MoveChoice` enum). Compute the axis/rotation and delta transform. If `move_fragment` is enabled, call `compute_moving_fragment` with the moving and reference atom IDs. Map fragment result-atom-IDs back to diff/base provenance IDs.
3. **Phase 3 (Mutate)**: For each atom to move: ensure it exists in the diff (promote base atoms to diff if needed via `ensure_diff_atom_for_base()`), then call `move_in_diff()` with the new position.

**Promoting base atoms**: When a base atom needs to be moved, it must first be "promoted" to the diff. This is the same pattern used by `drag_selected_by_delta` in `operations.rs` — copy the atom into the diff at its current position, then move it.

**Rotation helper**: For angle and dihedral, rotate a point around an axis through a center:
```rust
fn rotate_point_around_axis(point: DVec3, center: DVec3, axis: DVec3, angle_rad: f64) -> DVec3 {
    let q = DQuat::from_axis_angle(axis, angle_rad);
    center + q * (point - center)
}
```

**Tests** (`rust/tests/structure_designer/`):
- Build a small structure (e.g. H—C—C—H chain), select 2 atoms, call `modify_distance`, verify positions.
- Three-atom angle modification: verify the arm atom rotates to the target angle.
- Four-atom dihedral modification: verify the end atom rotates to the target dihedral.
- Fragment following: verify that connected atoms move along.
- Fragment disabled: verify only the single atom moves.
- Edge case: collinear atoms for angle (arbitrary perpendicular axis).
- Edge case: target equals current (no-op).

---

### Phase 4: Default Value Computation

**Goal**: Compute equilibrium bond length and angle for the "Default" button. Dihedral is postponed.

**Files to create/modify**:
- `rust/src/structure_designer/nodes/atom_edit/modify_measurement.rs` — Add default-value query functions

**Design**:

```rust
/// Compute the default (equilibrium) bond length for the two selected atoms.
/// Returns None if atoms are not bonded or if UFF typing fails.
pub fn compute_default_bond_length(
    structure_designer: &StructureDesigner,
    bond_length_mode: BondLengthMode,
) -> Option<f64>

/// Compute the default (equilibrium) angle for the three selected atoms.
/// Returns None if UFF typing fails for the vertex atom.
pub fn compute_default_angle(
    structure_designer: &StructureDesigner,
) -> Option<f64>
```

**Bond length default**:
1. Resolve the two selected atoms to result-space atoms.
2. Verify they are bonded; get the bond order.
3. Get both atoms' atomic numbers and UFF type labels (via `assign_uff_type()`).
4. Call `bond_distance()` from `guided_placement.rs` (for Crystal mode) or `calc_bond_rest_length()` from UFF params (with actual bond order).

**Angle default**:
1. Build the `SelectedAtomInfo` list and call `compute_measurement()` to get the `MeasurementResult::Angle { vertex_index, .. }`. This reuses the same vertex-identification logic (`find_angle_vertex`) that the measurement display uses — no duplication.
2. Get the vertex atom's UFF type via `assign_uff_type()`.
3. Look up `theta0` from `get_uff_params()`.

**Tests**:
- C-C single bond default ≈ 1.545 Å (crystal) or ≈ 1.514 Å (UFF).
- C=C double bond default shorter than single.
- sp3 carbon vertex angle default ≈ 109.47°.
- sp2 carbon vertex angle default ≈ 120°.
- Non-bonded pair returns None for bond length default.

---

### Phase 5: API Layer

**Goal**: Expose the modification operations and default-value queries to Flutter via FRB.

**Files to modify**:
- `rust/src/api/structure_designer/structure_designer_api_types.rs` — Enrich `APIMeasurement`, add new API types
- `rust/src/api/structure_designer/atom_edit_api.rs` — New API functions
- Run `flutter_rust_bridge_codegen generate` to regenerate bindings

**Enrich `APIMeasurement`**:

The current `APIMeasurement` only carries the numeric value. The dialog needs atom identities and roles. Add enriched variants:

```rust
pub enum APIMeasurement {
    Distance {
        distance: f64,
        /// Result-space atom IDs for the two atoms.
        atom1_id: u32,
        atom2_id: u32,
        /// Element symbols for display labels.
        atom1_symbol: String,
        atom2_symbol: String,
        /// Whether the two atoms are bonded (enables Default button).
        is_bonded: bool,
    },
    Angle {
        angle_degrees: f64,
        /// Vertex atom identity.
        vertex_id: u32,
        vertex_symbol: String,
        /// Arm atoms (indices 0 and 1 for move choice).
        arm_a_id: u32,
        arm_a_symbol: String,
        arm_b_id: u32,
        arm_b_symbol: String,
    },
    Dihedral {
        angle_degrees: f64,
        /// Chain A-B-C-D atom identities.
        chain_a_id: u32,
        chain_a_symbol: String,
        chain_b_id: u32,
        chain_b_symbol: String,
        chain_c_id: u32,
        chain_c_symbol: String,
        chain_d_id: u32,
        chain_d_symbol: String,
    },
}
```

Also add a field to carry the last-selected result atom ID so Flutter can determine the default move choice:
```rust
pub struct APIAtomEditData {
    // ... existing fields ...
    pub measurement: Option<APIMeasurement>,
    /// Result-space ID of the most recently selected atom (for dialog defaults).
    pub last_selected_result_atom_id: Option<u32>,
}
```

**New API functions**:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_modify_distance(
    target_distance: f64,
    move_first: bool,       // true = move atom1, false = move atom2
    move_fragment: bool,
) -> String                  // success/error message

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_modify_angle(
    target_angle_degrees: f64,
    move_arm_a: bool,       // true = move arm A, false = move arm B
    move_fragment: bool,
) -> String

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_modify_dihedral(
    target_angle_degrees: f64,
    move_a_side: bool,      // true = move A-side, false = move D-side
    move_fragment: bool,
) -> String

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_get_default_bond_length() -> Option<f64>

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_get_default_angle() -> Option<f64>
```

All modification functions call `refresh_structure_designer_auto()` after mutation, consistent with all other atom_edit API functions.

**After adding these**: Run `flutter_rust_bridge_codegen generate`.

---

### Phase 6: Flutter Dialog UI

**Goal**: Add the "Modify" button to the measurement card and implement the modal dialog.

**Files to modify**:
- `lib/structure_designer/node_data/atom_edit_editor.dart` — Add Modify button to `_buildMeasurementDisplay`, create dialog widget
- `lib/structure_designer/structure_designer_model.dart` — Add model methods wrapping the new API functions

**Step 6a: Model methods**

Add to `StructureDesignerModel`:
```dart
String atomEditModifyDistance(double target, bool moveFirst, bool moveFragment) {
    final msg = atom_edit_api.atomEditModifyDistance(
        targetDistance: target, moveFirst: moveFirst, moveFragment: moveFragment);
    refreshFromKernel();
    return msg;
}
// Similar for angle and dihedral

double? atomEditGetDefaultBondLength() => atom_edit_api.atomEditGetDefaultBondLength();
double? atomEditGetDefaultAngle() => atom_edit_api.atomEditGetDefaultAngle();
```

**Step 6b: Modify button in measurement card**

Extend `_buildMeasurementDisplay` to include a right-aligned "Modify" button:
```dart
Row(
  children: [
    Icon(icon, ...),
    Text('$label: ', ...),
    Text(value, ...),
    const Spacer(),
    SizedBox(
      height: 28,
      child: OutlinedButton(
        onPressed: () => _showModifyDialog(measurement),
        child: const Text('Modify', style: TextStyle(fontSize: 12)),
      ),
    ),
  ],
)
```

**Step 6c: Modify dialog**

Create `_showModifyDialog(APIMeasurement measurement)` which calls `showDialog()` with a `StatefulWidget` dialog. The dialog contains:

1. **Title**: "Modify Distance" / "Modify Angle" / "Modify Dihedral"
2. **Value field**: `TextFormField` with numeric validation, pre-filled with current value. Suffix text showing unit (Å or °).
3. **Default button**: `OutlinedButton` labeled "Default" that queries the Rust API and fills the value field. For distance: disabled if `!measurement.is_bonded`. For dihedral: hidden.
4. **Move atom radio**: Two `RadioListTile` widgets labeled with element symbol and atom ID. Pre-selected based on `last_selected_result_atom_id` from `APIAtomEditData`.
5. **Move fragment checkbox**: `CheckboxListTile`, default checked.
6. **Actions**: Cancel (pops dialog) and Apply (calls model method, pops dialog).

The dialog is a single shared widget that adapts its layout based on the `APIMeasurement` variant. This avoids code duplication across the three cases — the differences (title, unit suffix, default button availability, atom labels) are driven by the measurement data.

**Validation in the dialog**:
- Distance: parse as double, reject < 0.1.
- Angle: parse as double, reject outside 0–180.
- Dihedral: parse as double, reject outside -180–180.
- Apply button disabled while input is invalid.

---

### Phase Dependencies

```
Phase 1 (Selection Order) ──────────────────┐
                                             │
Phase 2 (BFS Fragment) ── Phase 3 (Modify) ─┤
                                             ├── Phase 5 (API) ── Phase 6 (Flutter UI)
Phase 4 (Default Values) ───────────────────┘
```

- Phases 1, 2, and 4 are independent of each other and can be developed in parallel.
- Phase 3 depends on Phase 2 (uses the BFS fragment algorithm).
- Phase 5 depends on Phases 1, 3, and 4 (exposes everything to Flutter).
- Phase 6 depends on Phase 5 (consumes the API).

