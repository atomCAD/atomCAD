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

This must be addressed before the Modify feature can use "last-selected" as the default. The implementation should add an ordered sequence (e.g. a `Vec<u32>` or `IndexSet<u32>`) alongside or replacing the hash sets, maintaining insertion order. Only the last 4 entries matter for measurement purposes, so this can be a small bounded buffer rather than a full ordering of all selected atoms.

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

For distance, the button is greyed out if the two atoms are not bonded. For angle and dihedral, always enabled.

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

## Open Questions

1. **Keyboard shortcut**: Should there be a shortcut to open the modify dialog (e.g. `M` key when a measurement is displayed)? Probably yes, but can be added later.

2. **Batch modification**: Should the dialog support modifying multiple bonds at once (e.g. "set all selected bonds to 1.54 Å")? Not in initial scope, but the architecture should not preclude it.
