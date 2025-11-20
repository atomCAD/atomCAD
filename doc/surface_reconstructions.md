# Efficient Algorithm for (100) 2×1 Dimer Surface Reconstruction in Cubic Diamond

Doing surface reconstruction efficiently in crystals with hundreds of thousands of atoms is hard. We tackle the problem-space by solving a special case (the (100) 2×1 dimer surface reconstruction in cubic diamond) efficiently and try to generalize from the learnings and the developed tools to be able to do other surface reconstructions efficiently later.

## Current atom_fill Algorithm

1. **Add bulk atoms**: Place atoms from the infinite periodic motif (cubic diamond in this case) that lie inside the geometry. A small epsilon margin ensures boundary and corner atoms are included.

2. **Add bonds**: Create bonds according to the motif definition.

3. **Remove isolated atoms**: Eliminate atoms with no bonds (typically corner atoms).

4. **Optional under-coordinated atom removal**: Recursively remove atoms with only one bond.

5. **Hydrogen passivation** (if enabled): Identify under-coordinated surface atoms (those missing bonds they would have in the infinite bulk lattice). Add hydrogen atoms in the direction of the missing neighbor at the appropriate C-H bond distance.

## Proposed (100) 2×1 Dimer Reconstruction Algorithm

Surface reconstruction is inserted between steps 4 and 5 of the atom_fill algorithm, after structure generation but before hydrogen passivation.

**Applicability check**: This algorithm applies only to cubic diamond structures. We verify this by checking:

* The built-in zincblende motif is being used
* The unit cell matches cubic diamond
* Both PRIMARY and SECONDARY element parameters are set to carbon

**Algorithm steps:**

1. Classify surface orientation for each atom
2. Identify dimer pairs efficiently
3. Apply reconstruction to dimer pairs

### Step 1: Classify Surface Orientation for Each Atom

For each atom, we determine whether it lies on a {100} facet and identify its specific surface normal. Each carbon atom is classified into one of eight categories:

* **Bulk** (no reconstruction)
* **Unknown** (no reconstruction)
* **(100)** surface
* **(-100)** surface
* **(010)** surface
* **(0-10)** surface
* **(001)** surface
* **(00-1)** surface

The category indicates the surface normal direction. Fortunately, classification can be somewhat approximate: dimers form only when both constituent atoms share the same surface orientation, so misclassification requires two neighboring atoms to be incorrectly categorized simultaneously. We could even assign atoms to multiple categories using 6 **bit flags** for overlapping surface regions.

**Classification heuristics:**

1. **SDF depth value**: The signed distance field (SDF) depth is already computed during structure generation. Atoms with depth > 0.5 Å can be immediately classified as bulk for performance, as they are too far from any surface.

2. **Bond coordination analysis**: Examining the directions of existing C-C bonds provides strong classification signals. Surface atoms on {100} facets have exactly 2 bulk neighbors (with 2 dangling bonds), and the missing bond directions indicate the surface normal. This information is readily available at low computational cost.

3. **SDF gradient** (optional): For ambiguous cases, we can compute the local surface normal via the SDF gradient. This requires 4 additional SDF evaluations per atom and is only needed for atoms near the surface. While computationally expensive, it would only impact shallow atoms and cause approximately 1.5× slowdown depending on crystal depth distribution.

### Step 2: Identify Dimer Pairs Efficiently

The 2×1 dimer pattern on an unreconstructed (bulk-terminated) (100) diamond surface has two distinct registry phases—all other arrangements are periodic translations. We label these **Phase A** and **Phase B**.

 ![Phase A](/api/attachments.redirect?id=ec2d23a2-e9a6-4018-a35a-bea5e72703a0 " =780x741")


 ![Phase B](/api/attachments.redirect?id=6adfd7bd-2f27-4e01-8aac-1e1698b2f3cc " =780x741")


**Phase selection:** Different surface domains (terraces) could use different phases, but for simplicity we initially use a single global phase for each infinite plane. Phase selection can be hardcoded initially and refined later to handle domain boundaries. 

**Efficient lookup using lattice coordinates:**

The `atom_fill` process maintains a `PlacedAtomTracker` data structure:

```rust
pub struct PlacedAtomTracker {
  // (lattice_coords, basis_index) -> atom_id
  atom_map: FxIndexMap<(IVec3, usize), u32>,
}
```

Here `lattice_coords` (internally called `motif_space_pos`) specifies the unit cell position in integer lattice coordinates, while `basis_index` (internally `site_index`) identifies which basis atom within that unit cell. This maps each physical atom to its crystallographic address.

**Dimer pairing algorithm:**

For each dimer, we designate one atom as the **primary atom** and compute its **dimer partner** location. Given a lattice address `(lattice_coords, basis_index)`, we can efficiently determine (via lookup table):

- Whether this atom is a primary dimer atom for the selected phase
- If yes, the lattice address of its dimer partner

> **CRITICAL CONSTRAINT**: A dimer is only created if both atoms share the same surface orientation classification. This prevents incorrect dimer formation across surface edges or on different facets.

**Alternative approach:** The spatial grid acceleration structure in `AtomicStructure` could be used for neighbor queries, but leveraging `PlacedAtomTracker` is faster since crystallographic addresses make dimer partner locations trivial to compute. 

### Step 3: Apply Reconstruction to Dimer Pairs

Once both dimer atoms are identified, reconstruction proceeds independently for each dimer pair.

**Geometric reconstruction:**

1. **Create dimer bond**: Form a C-C bond between the two surface atoms
2. **Symmetric displacement**: Move both atoms symmetrically toward each other to achieve the characteristic dimer bond length (~1.4 Å, shorter than bulk C-C bonds)

**Hydrogen termination** (if passivation enabled):

For each atom in a reconstructed dimer, place hydrogen at a predetermined position based on the surface orientation. These atoms are flagged as **passivated** so the generic passivation algorithm in step 5 skips them.

This approach ensures dimer atoms receive proper sp²-like hydrogen termination appropriate for the reconstructed geometry, rather than sp³ termination for bulk-like dangling bonds.