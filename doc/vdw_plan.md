# Van der Waals (Nonbonded) Energy Implementation Plan

## 1. Overview

Add Lennard-Jones 12-6 van der Waals interactions to the UFF force field, completing the transition from Tier 2 (bonded-only) to Tier 3 (bonded + vdW). This enables:

- Correct steric repulsion (preventing atom overlaps)
- Asymmetric torsion profiles (anti vs gauche butane)
- Accurate total energies matching RDKit's full UFF
- Proper minimized geometries matching RDKit's vdW-optimized reference

**Scope**: vdW only — no electrostatics (standard UFF practice).

---

## 2. Physics

### 2.1 Energy Formula (UFF Paper, Rappé et al. 1992)

```
E_vdW = D_ij * [ -2 * (x_ij / r)^6 + (x_ij / r)^12 ]
```

Where:
- `r` = interatomic distance (Å)
- `x_ij` = equilibrium vdW distance (Å) — energy minimum at `r = x_ij`
- `D_ij` = well depth (kcal/mol) — `E(x_ij) = -D_ij`

This is the standard Lennard-Jones 12-6 potential. At `r = x_ij`, the energy is `-D_ij` (attractive minimum). As `r → 0`, the `r^12` repulsion dominates. As `r → ∞`, energy → 0.

### 2.2 Combination Rules (Geometric Mean)

```
x_ij = sqrt(x_i * x_j)
D_ij = sqrt(D_i * D_j)
```

Where `x_i = UffAtomParams.x1` and `D_i = UffAtomParams.d1` are per-atom vdW parameters already present in our parameter table (`params.rs:20-23`).

**Verification**: For C_R (x1=3.851) and H_ (x1=2.886):
- x_ij = sqrt(3.851 × 2.886) = 3.333765 ✓ (matches reference JSON)
- D_ij = sqrt(0.105 × 0.044) = 0.067971 ✓ (matches reference JSON)

### 2.3 Analytical Gradient

```
dE/dr = D_ij * [ 12*(x_ij/r)^6 / r  -  12*(x_ij/r)^12 / r ]
      = (12 * D_ij / r) * [ (x_ij/r)^6 - (x_ij/r)^12 ]
```

Chain rule to Cartesian coordinates:
```
dE/dx_i = (dE/dr) * (x_i - x_j) / r    (same pattern as bond stretch)
dE/dx_j = -(dE/dr) * (x_i - x_j) / r
```

### 2.4 Pair Selection (Which Atom Pairs Get vdW?)

**RDKit Builder.cpp** (lines 91-131, 457-458) builds a neighbor matrix. It only explicitly marks two relationships:
- 1-2 pairs (directly bonded) → `RELATION_1_2` (value 0)
- 1-3 pairs (separated by one atom/two bonds) → `RELATION_1_3` (value 1)

Everything else (1-4 and beyond) gets the default value `RELATION_1_X` (value 3). The nonbonded inclusion check (line 457-458) is:
```cpp
if (getTwoBitCell(neighborMatrix, ...) >= RELATION_1_4)  // RELATION_1_4 = 2
```

Since 1-4 pairs have the default value `RELATION_1_X` (3), they pass `>= 2`. So **1-4 pairs and beyond are all included in vdW**. Only 1-2 (bonded) and 1-3 (angle) pairs are excluded.

Additionally, RDKit applies a **distance threshold**: `dist < vdwThresh * x_ij` where `vdwThresh` defaults to 10.0. This is a performance optimization for large molecules.

**Implementation decision**: We include all 1-4+ pairs with NO distance cutoff. Reasons:
1. For atomCAD's typical molecule sizes (< 500 atoms), enumerating all 1-4+ pairs is fast
2. No distance cutoff means deterministic, position-independent pair lists
3. The distance threshold in RDKit is primarily a performance optimization for large molecules
4. For molecules under ~30 atoms (our reference set), all pairs are within `10 * x_ij` anyway, so the cutoff has zero effect on energy matching

### 2.5 Reference Data: What the JSON Contains

The reference JSON `uff_reference.json` has two sets of energy/gradient fields per molecule:
- `input_energy.total` / `input_gradients.full` — from RDKit's full UFF (bonded + vdW)
- `input_energy.bonded` / `input_gradients.bonded` — from RDKit with `vdwThresh=0` (bonded only)
- `input_energy.vdw` = `total - bonded`
- Same structure for `minimized_energy` (`total`, `bonded`, `vdw`)

The reference also has a `vdw_params` list per molecule with pre-computed `x_ij`/`D_ij` values. **Caveat**: this list was generated with `path_len >= 4` (line 325-330 of `generate_uff_reference.py`), counting bonds along the shortest path. `path_len >= 4` means **4+ bonds** between atoms, which in chemistry notation is **1-5+ pairs only**. It excludes 1-4 pairs (which have path_len = 3).

However, RDKit's actual C++ force field **does** include 1-4 vdW pairs (as shown in section 2.4), and the energy values (`input_energy.total`) reflect this. This is confirmed by the data:

| Molecule | vdw_params count | input_energy.vdw | Has 1-4 vdW? |
|----------|-----------------|------------------|--------------|
| methane | 0 | 0.0 | No (max path = 2) |
| ethylene | 0 | **0.4497** | **Yes** — from 4 H-H 1-4 pairs |
| ethane | 0 | **0.3254** | **Yes** — from 9 H-H 1-4 pairs |
| benzene | 15 | 11.6761 | Yes — from 15 listed + 21 unlisted 1-4 pairs |
| butane | 27 | 5.5347 | Yes — from 27 listed + 27 unlisted 1-4 pairs |
| water | 0 | 0.0 | No (max path = 2) |
| ammonia | 0 | 0.0 | No (max path = 2) |
| adamantane | 141 | 41.3534 | Yes — from 141 listed + 96 unlisted 1-4 pairs |
| methanethiol | 0 | **-0.0925** | **Yes** — from 3 H-H 1-4 pairs (attractive) |

**Testing strategy**: Validate x_ij/D_ij values against the reference's `vdw_params` (which are correct for the 1-5+ pairs they list), but validate energies against `input_energy.total` (which includes all vdW contributions including 1-4).

---

## 3. Expected Nonbonded Pair Counts

Our implementation excludes 1-2 (bond) and 1-3 (angle endpoint) pairs. The expected count is:

```
nonbonded_pairs = C(N, 2) - |exclusion_set|
exclusion_set = {(min(i,j), max(i,j)) : bond(i,j)} ∪ {(min(i,k), max(i,k)) : angle(i,vertex,k)}
```

For our 9 reference molecules (none have 3-rings, so bond pairs and angle endpoint pairs are disjoint):

| Molecule | N | C(N,2) | Bonds | Angles | Exclusions | Our NB pairs | Ref 1-5+ pairs |
|----------|---|--------|-------|--------|------------|-------------|----------------|
| methane | 5 | 10 | 4 | 6 | 10 | **0** | 0 |
| ethylene | 6 | 15 | 5 | 6 | 11 | **4** | 0 |
| ethane | 8 | 28 | 7 | 12 | 19 | **9** | 0 |
| benzene | 12 | 66 | 12 | 18 | 30 | **36** | 15 |
| butane | 14 | 91 | 13 | 24 | 37 | **54** | 27 |
| water | 3 | 3 | 2 | 1 | 3 | **0** | 0 |
| ammonia | 4 | 6 | 3 | 3 | 6 | **0** | 0 |
| adamantane | 26 | 325 | 28 | 60 | 88 | **237** | 141 |
| methanethiol | 6 | 15 | 5 | 7 | 12 | **3** | 0 |

These exact counts should be validated in tests (section 6).

---

## 4. Reference Energy Values

### Input energies (at non-equilibrium input positions):

| Molecule | Total | Bonded (Phase 15) | vdW |
|----------|-------|-------|-----|
| methane | 0.5300345608 | 0.5300345608 | 0.0 |
| ethylene | 2.2796201355 | 1.8298846306 | 0.4497355048 |
| ethane | 2.8559895000 | 2.5306041805 | 0.3253853196 |
| benzene | 14.1640163054 | 2.4878919809 | 11.6761243244 |
| butane | 13.3992308450 | 7.8645123858 | 5.5347184593 |
| water | 0.3092852765 | 0.3092852765 | 0.0 |
| ammonia | 1.0085000608 | 1.0085000608 | 0.0 |
| adamantane | 52.0616580077 | 10.7082918027 | 41.3533662050 |
| methanethiol | 4.2152680434 | 4.3077991051 | -0.0925310617 |

### Minimized energies (after RDKit's full UFF optimization):

| Molecule | Total | Bonded | vdW |
|----------|-------|--------|-----|
| methane | ~0.0 | ~0.0 | 0.0 |
| ethylene | 0.1445705447 | 0.0122472054 | 0.1323233392 |
| ethane | 0.1412752838 | 0.0322297087 | 0.1090455751 |
| benzene | 10.5447318010 | 1.0886202495 | 9.4561115514 |
| butane | 2.9109413383 | 1.5151903528 | 1.3957509855 |
| water | ~0.0 | ~0.0 | 0.0 |
| ammonia | 0.0 | 0.0 | 0.0 |
| adamantane | 22.5224721136 | 4.6390121115 | 17.8834600021 |
| methanethiol | -0.0335625617 | 0.0053562348 | -0.0389187965 |

Note: methanethiol has **negative** total minimized energy because the attractive vdW between H atoms on C and the S atom exceeds the residual bonded strain.

---

## 5. Implementation Phases

### Phase 20a: Nonbonded pair enumeration in `topology.rs`

**Goal**: Add 1-4+ pair enumeration to `MolecularTopology`.

**New struct** (in `topology.rs`, after `InversionInteraction`):
```rust
/// A nonbonded (van der Waals) pair interaction.
#[derive(Debug, Clone)]
pub struct NonbondedPairInteraction {
    /// Topology index of the first atom (idx1 < idx2).
    pub idx1: usize,
    /// Topology index of the second atom.
    pub idx2: usize,
}
```

**New field** in `MolecularTopology` (after `inversions`):
```rust
/// Nonbonded (1-4+) pair interactions for van der Waals.
pub nonbonded_pairs: Vec<NonbondedPairInteraction>,
```

**Algorithm** — new private method `enumerate_nonbonded_pairs()`:
1. Build exclusion set: all 1-2 pairs (from `bonds`) + all 1-3 pairs (from `angles`)
   - For each bond: insert `(min(idx1,idx2), max(idx1,idx2))`
   - For each angle: insert `(min(idx1,idx3), max(idx1,idx3))` (the two end atoms)
2. For every atom pair (i, j) where i < j and i < num_atoms: include if NOT in exclusion set
3. Return `Vec<NonbondedPairInteraction>`

The exclusion set uses `FxHashSet<(usize, usize)>` (already imported via `rustc_hash`).

**Signature**:
```rust
fn enumerate_nonbonded_pairs(
    num_atoms: usize,
    bonds: &[BondInteraction],
    angles: &[AngleInteraction],
) -> Vec<NonbondedPairInteraction>
```

**Wire into `from_structure()`**: Call after the existing `enumerate_inversions()` call (around line 162), before the final `MolecularTopology { ... }` construction. Add `nonbonded_pairs` to the struct literal.

**~40-50 lines of new code.**

### Phase 20b: vdW combination rules in `params.rs`

**Goal**: Add functions to compute cross-pair vdW parameters.

**New functions** (add at end of `params.rs`, after `calc_torsion_params()`):
```rust
/// Compute vdW characteristic distance for a pair (geometric mean).
/// x_ij = sqrt(x_i * x_j) where x_i = UffAtomParams.x1
pub fn calc_vdw_distance(params_i: &UffAtomParams, params_j: &UffAtomParams) -> f64 {
    (params_i.x1 * params_j.x1).sqrt()
}

/// Compute vdW well depth for a pair (geometric mean).
/// D_ij = sqrt(D_i * D_j) where D_i = UffAtomParams.d1
pub fn calc_vdw_well_depth(params_i: &UffAtomParams, params_j: &UffAtomParams) -> f64 {
    (params_i.d1 * params_j.d1).sqrt()
}
```

**~15 lines of new code.**

### Phase 20c: vdW energy + gradient in `energy.rs`

**Goal**: Add `VdwParams` struct and energy/gradient functions.

**New struct** (add new section at end of `energy.rs`, following the BondStretchParams pattern):
```rust
// ============================================================================
// Van der Waals (Lennard-Jones 12-6)
// ============================================================================
//
// E = D_ij * [ -2 * (x_ij / r)^6 + (x_ij / r)^12 ]
//
// Gradient (for atom i, atom j has opposite sign):
//   dE/dr = (12 * D_ij / r) * [ (x_ij/r)^6 - (x_ij/r)^12 ]
//   dE/dx_i = (dE/dr) * (x_i - x_j) / r
//
// Ported from RDKit's Nonbonded.cpp (BSD-3-Clause).

/// Pre-computed parameters for a single van der Waals pair.
#[derive(Debug, Clone)]
pub struct VdwParams {
    /// Index of the first atom.
    pub idx1: usize,
    /// Index of the second atom.
    pub idx2: usize,
    /// Equilibrium vdW distance x_ij in Angstroms.
    pub x_ij: f64,
    /// Well depth D_ij in kcal/mol.
    pub d_ij: f64,
}
```

**New functions** (following the bond stretch pattern):
```rust
/// Computes vdW energy for a single nonbonded pair.
pub fn vdw_energy(params: &VdwParams, positions: &[f64]) -> f64

/// Computes vdW energy and accumulates gradients for a single nonbonded pair.
pub fn vdw_energy_and_gradient(
    params: &VdwParams,
    positions: &[f64],
    gradients: &mut [f64],
) -> f64
```

**Implementation notes**:
- Compute `r` from positions (same dx/dy/dz pattern as bond stretch)
- **Degenerate case** (r < 0.01): clamp `r = max(r, 0.01)` to avoid division by zero. This matches the bond stretch degenerate handling pattern (look at `bond_stretch_energy_and_gradient` — it nudges zero-distance atoms).
- Compute `ratio = x_ij / r`, then `ratio6 = ratio^6`, `ratio12 = ratio6^2`
- Energy: `d_ij * (-2.0 * ratio6 + ratio12)`
- Gradient factor: `(12.0 * d_ij / r) * (ratio6 - ratio12)`, then apply chain rule `dE/dx_i += factor * dx / r`

**Download RDKit reference** before starting:
```bash
cd rust/tests/crystolecule/simulation/test_data/
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Nonbonded.cpp" -o Nonbonded.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Nonbonded.h" -o Nonbonded.h
```

**~50-60 lines of new code.**

### Phase 20d: Wire vdW into `UffForceField` in `uff/mod.rs`

**Goal**: Pre-compute vdW params and add to energy evaluation.

**Step 1: Update imports** in `uff/mod.rs` (line 8-12):
```rust
use energy::{
    AngleBendParams, BondStretchParams, InversionParams, TorsionAngleParams, VdwParams,
    angle_bend_energy_and_gradient, bond_stretch_energy_and_gradient,
    inversion_energy_and_gradient, torsion_energy_and_gradient,
    vdw_energy_and_gradient,
};
use params::{
    Hybridization, calc_angle_force_constant, calc_bond_force_constant, calc_bond_rest_length,
    calc_inversion_coefficients_and_force_constant, calc_torsion_params,
    calc_vdw_distance, calc_vdw_well_depth,
};
```

**Step 2: New field** in `UffForceField` struct (after `inversion_params`):
```rust
pub vdw_params: Vec<VdwParams>,
```

**Step 3: Update `from_topology()`**: After step 7 (inversion params, ~line 190), add:
```rust
// Step 8: Pre-compute van der Waals parameters for all nonbonded pairs
let vdw_params: Vec<VdwParams> = topology.nonbonded_pairs.iter().map(|pair| {
    let params_i = get_uff_params(typing.labels[pair.idx1]).unwrap();
    let params_j = get_uff_params(typing.labels[pair.idx2]).unwrap();
    VdwParams {
        idx1: pair.idx1,
        idx2: pair.idx2,
        x_ij: calc_vdw_distance(params_i, params_j),
        d_ij: calc_vdw_well_depth(params_i, params_j),
    }
}).collect();
```

Add `vdw_params` to the `Ok(Self { ... })` return and to the empty-molecule early return.

**Step 4: Update `energy_and_gradients()`** — add loop after inversion loop:
```rust
// Van der Waals (nonbonded) contributions
for vp in &self.vdw_params {
    *energy += vdw_energy_and_gradient(vp, positions, gradients);
}
```

**~30 lines of changes.**

---

## 6. Tests — New file: `uff_vdw_test.rs`

Register in `rust/tests/crystolecule.rs` (add before the closing blank line):
```rust
#[path = "crystolecule/simulation/uff_vdw_test.rs"]
mod uff_vdw_test;
```

This file needs the same helpers as `uff_force_field_test.rs` (load reference JSON, build structures, etc.). Copy the helper pattern from that file but add the fields needed for vdW testing.

**Data structures needed** (in `uff_vdw_test.rs`):
```rust
#[derive(serde::Deserialize)]
struct ReferenceData {
    molecules: Vec<ReferenceMolecule>,
}

#[derive(serde::Deserialize)]
struct ReferenceMolecule {
    name: String,
    atoms: Vec<ReferenceAtom>,
    bonds: Vec<ReferenceBond>,
    input_positions: Vec<[f64; 3]>,
    interaction_counts: InteractionCounts,
    input_energy: InputEnergy,
    input_gradients: InputGradients,
    #[serde(default)]
    vdw_params: Vec<ReferenceVdwParam>,
}

#[derive(serde::Deserialize)]
struct InteractionCounts {
    bonds: usize,
    angles: usize,
    vdw_pairs: usize, // reference 1-5+ count (subset of our pairs)
}

#[derive(serde::Deserialize)]
struct InputEnergy {
    total: f64,
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct InputGradients {
    full: Vec<[f64; 3]>,
    bonded: Vec<[f64; 3]>,
}

#[derive(serde::Deserialize)]
struct ReferenceVdwParam {
    atoms: [usize; 2],
    x_ij: f64,
    #[serde(rename = "D_ij")]
    d_ij: f64,
}
```

### C1 series — vdW parameter tests

**c1_vdw_combination_rules**: Verify geometric mean for known atom type pairs:
- C_3-C_3: x_ij = 3.851, D_ij = 0.105
- H_-H_: x_ij = 2.886, D_ij = 0.044
- C_R-H_: x_ij = sqrt(3.851 × 2.886) = 3.333765, D_ij = sqrt(0.105 × 0.044) = 0.067971
- N_3-O_3: compute from param table values
- Use `get_uff_params()`, `calc_vdw_distance()`, `calc_vdw_well_depth()` directly.

**c1_vdw_params_vs_reference**: For benzene, butane, and adamantane, build `UffForceField`, then for each entry in the reference JSON's `vdw_params` list, find the matching pair in our `ff.vdw_params` and compare x_ij (tol 1e-4) and d_ij (tol 1e-6). This validates the 1-5+ pairs that the reference does list. Note: our pair list is a superset (includes 1-4 pairs too), so search our list for the reference pair, not vice versa.

### C2 series — vdW energy unit tests

These test the `vdw_energy()` and `vdw_energy_and_gradient()` functions in isolation with synthetic two-atom systems.

**c2_vdw_energy_at_equilibrium**: Place two atoms at distance r = x_ij → E = -D_ij. Test with C_R-H_ params (x_ij=3.333765, D_ij=0.067971).

**c2_vdw_energy_repulsive**: Place two atoms at r = 0.5 * x_ij → E > 0 (r^12 term dominates). Verify E ≈ D_ij * (-2 * 2^6 + 2^12) = D_ij * (4096 - 128) = 3968 * D_ij.

**c2_vdw_energy_attractive**: Place two atoms at r = 1.5 * x_ij → -D_ij < E < 0. Verify E ≈ D_ij * (-2 * (1/1.5)^6 + (1/1.5)^12).

**c2_vdw_energy_zero_at_infinity**: Place two atoms at r = 100 * x_ij → |E| < 1e-10.

**c2_vdw_numerical_gradient**: Central difference (h=1e-5) vs analytical gradient for several configurations:
- Two C_R atoms at various distances (0.8*x_ij, 1.0*x_ij, 1.2*x_ij, 2.0*x_ij)
- Two atoms along diagonal (not axis-aligned)
- Two atoms at non-origin positions
- Tolerance: <1% relative error for each gradient component (same standard as bond/angle/torsion/inversion tests).

**c2_vdw_force_balance**: Sum of gradients on both atoms = 0 (Newton's third law). Test with several distances.

**c2_vdw_energy_only_matches_gradient_version**: `vdw_energy()` returns same value as `vdw_energy_and_gradient()`.

### C3 series — nonbonded pair enumeration

**c3_pair_counts**: For all 9 reference molecules, verify our exact nonbonded pair count matches the expected values from section 3:

```rust
let expected_counts = [
    ("methane", 0), ("ethylene", 4), ("ethane", 9),
    ("benzene", 36), ("butane", 54), ("water", 0),
    ("ammonia", 0), ("adamantane", 237), ("methanethiol", 3),
];
```

Build `MolecularTopology` from each reference molecule's structure and check `topology.nonbonded_pairs.len()`.

**c3_pair_exclusions**: For ethane (8 atoms, 7 bonds, 12 angles):
- Verify none of the 9 NB pairs are 1-2 (bonded) pairs
- Verify none are 1-3 (angle endpoint) pairs
- Verify all 9 are the expected H-H 1-4 pairs: (2,5), (2,6), (2,7), (3,5), (3,6), (3,7), (4,5), (4,6), (4,7) [topology indices — these are the 9 cross-methyl H-H pairs]

For butane: verify no bonded or angle pairs appear; verify specific known 1-4 pairs ARE present.

**c3_pair_symmetry**: For benzene: all pairs have idx1 < idx2, no duplicate pairs.

**c3_pair_counts_superset_of_reference**: For molecules with reference `vdw_pairs > 0` (benzene, butane, adamantane): verify `our_count > reference_vdw_pairs` (we include 1-4 pairs they don't).

### C4 series — full force field with vdW

**c4_total_energy_vs_reference**: For all 9 molecules, build `UffForceField` from topology, evaluate energy at input positions, compare to `input_energy.total`:

| Molecule | Reference total | Tolerance |
|----------|----------------|-----------|
| methane | 0.5300 | 0.01 |
| ethylene | 2.2796 | 0.05 |
| ethane | 2.8560 | 0.05 |
| benzene | 14.1640 | 0.1 |
| butane | 13.3992 | 0.1 |
| water | 0.3093 | 0.01 |
| ammonia | 1.0085 | 0.01 |
| adamantane | 52.0617 | 0.5 |
| methanethiol | 4.2153 | 0.05 |

Tolerances account for floating point accumulation over many pairs. For molecules with no vdW pairs (methane, water, ammonia), the value should match the bonded-only result exactly, confirming bonded terms are untouched.

**c4_total_gradients_vs_reference**: For all 9 molecules, compare full gradient (from `energy_and_gradients`) against `input_gradients.full`. Tolerance: per-component ≤0.1 kcal/(mol·Å) for molecules ≤ 14 atoms, ≤0.5 for adamantane (26 atoms, 237 NB pairs).

**c4_numerical_gradient_full**: For benzene and butane: central difference (h=1e-5) verification of the full (bonded + vdW) analytical gradient. <1% relative error per component. This is the most important test — it catches sign errors and missing chain rule terms.

**c4_bonded_energy_unchanged**: Regression test — verify that bonded-only energy has not changed. For each of the 9 molecules:
1. Build `UffForceField` from topology
2. Compute bonded-only energy by manually iterating over `ff.bond_params`, `ff.angle_params`, `ff.torsion_params`, `ff.inversion_params` and calling the individual `_energy()` functions (NOT `energy_and_gradients` which now includes vdW)
3. Compare against `input_energy.bonded`
4. Use the same tolerances as the original Phase 15 tests (0.01 for small, 0.1 for large molecules)

This ensures adding vdW did not accidentally perturb the bonded energy terms.

**c4_vdw_contribution_positive_for_non_equilibrium**: For benzene, butane, and adamantane: compute total energy minus bonded energy → should approximately equal `input_energy.vdw`. Tolerance ≤0.5 kcal/mol.

---

## 7. Tests — Updates to `uff_force_field_test.rs`

After adding vdW, the `UffForceField::energy_and_gradients()` method returns total energy (bonded + vdW). The existing tests compare against `input_energy.bonded` and `input_gradients.bonded`, which will now fail since our output includes vdW.

### Required changes:

**Step 1: Update data structures** (lines 57-65):

Change `InputEnergy` and `InputGradients` to include total/full fields:
```rust
#[derive(serde::Deserialize)]
struct InputEnergy {
    total: f64,
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct InputGradients {
    full: Vec<[f64; 3]>,
    bonded: Vec<[f64; 3]>,
}
```

Also add `vdw_pairs` to `InteractionCounts` (needed for new assertions):
```rust
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct InteractionCounts {
    bonds: usize,
    angles: usize,
    torsions: usize,
    inversions: usize,
    vdw_pairs: usize,
}
```

**Step 2: Update file header comment** (line 1-4):
Change "bonded-only energy and gradients" to "full UFF energy and gradients (bonded + vdW)".

**Step 3: Update all 10 energy tests** (lines ~212-353):
Change every `mol.input_energy.bonded` reference to `mol.input_energy.total`. Adjust tolerances:
- methane, water, ammonia: keep 0.01 (no vdW, same values)
- ethylene, ethane, methanethiol: change to 0.05
- benzene, butane: change to 0.1
- adamantane: change to 0.5

**Step 4: Update all 10 gradient tests** (lines ~367-466):
Change every `mol.input_gradients.bonded` reference to `mol.input_gradients.full`. Adjust tolerances:
- Small molecules (≤ 8 atoms): 0.05
- Medium (≤ 14 atoms): 0.1
- Large (adamantane): 0.5

**Step 5: Existing numerical gradient tests** should continue to pass without changes (they compute analytical vs numerical for our own force field — both now include vdW, so the comparison is still valid).

**Step 6: Update `compute_energy()` helper** — its doc comment says "bonded energy", update to say "total energy".

---

## 8. Tests — Updates to `minimize_test.rs`

This file has the most extensive changes because many tests were specifically designed as bonded-only workarounds. With vdW, these tests should be updated to their full potential.

### Step 1: Update data structures (lines 106-114)

```rust
#[derive(serde::Deserialize)]
struct InputEnergy {
    total: f64,
    bonded: f64,
}

#[derive(serde::Deserialize)]
struct MinimizedEnergy {
    total: f64,
    bonded: f64,
}
```

### Step 2: Per-molecule convergence tests (lines ~411-553)

Tests: `uff_methane_minimizes`, `uff_ethylene_minimizes`, ..., `uff_methanethiol_minimizes`.

These assert `result.energy < mol.input_energy.bonded + tol`. Change to:
```rust
assert!(result.energy < mol.input_energy.total + tol);
```

The minimizer should still decrease total energy from the input. Tolerances remain the same.

### Step 3: `uff_minimized_bonded_energy_near_zero` (lines ~577-621)

**Replace this test.** It was a workaround that tested near-zero bonded energy (only possible without vdW). With vdW, the minimized energy won't be near zero for molecules with nonbonded pairs (benzene: ~10.5, adamantane: ~22.5).

Replace with **`uff_minimized_total_energy_vs_reference`**: compare our minimized total energy against `mol.minimized_energy.total` for all 9 molecules. Tolerances:

| Molecule | Reference min total | Tolerance |
|----------|-------------------|-----------|
| methane | ~0.0 | 0.01 |
| ethylene | 0.1446 | 0.1 |
| ethane | 0.1413 | 0.1 |
| benzene | 10.5447 | 1.0 |
| butane | 2.9109 | 0.5 |
| water | ~0.0 | 0.01 |
| ammonia | 0.0 | 0.01 |
| adamantane | 22.5225 | 2.0 |
| methanethiol | -0.0336 | 0.1 |

Note: wider tolerances for larger molecules because our optimizer may find slightly different local minima than RDKit's (different line search, different convergence criteria). For adamantane, the optimizer may also need more iterations (use `max_iterations: 2000`).

### Step 4: Geometry tests (lines ~623-900)

Tests: `uff_methane_minimized_geometry`, `uff_ethylene_minimized_geometry`, `uff_water_minimized_geometry`.

These currently validate against UFF rest parameters as primary validation, with RDKit cross-checks only "where vdW is negligible". With vdW included, **all molecules can now be directly compared against RDKit's minimized geometry** (`mol.minimized_geometry.bond_lengths` and `mol.minimized_geometry.angles`).

Update each test:
- Remove "vdW negligible" caveats from comments
- Change the primary validation from "our rest params" to "RDKit reference geometry"
- Tolerances: bond lengths ±0.01 Å, angles ±2° (same as existing)
- For molecules with significant vdW (benzene, butane, adamantane), the geometry will now match RDKit more closely because both optimizations include the same energy terms

### Step 5: `b10_energy_decreases_from_input` (lines ~1023-1055)

Change `mol.input_energy.bonded` to `mol.input_energy.total`:
```rust
assert!(result.energy < mol.input_energy.total + 0.01);
```

### Step 6: `b10_bonded_minimum_leq_rdkit_bonded` (lines ~1062-1087)

**Replace this test.** It was a one-sided inequality workaround (our bonded-only minimum ≤ RDKit's bonded energy at vdW-optimized geometry). With vdW, we can do a proper comparison.

Replace with **`b10_minimized_energy_matches_reference`**: compare our minimized total energy to `mol.minimized_energy.total`. Use the same tolerances as the table in Step 3. This is a proper bidirectional comparison, not a one-sided inequality.

### Step 7: `b10_energy_self_consistent` (if exists)

Should work unchanged — it recomputes energy at minimized positions and checks it equals the reported energy. Both now include vdW, but the self-consistency check is the same.

### Step 8: B11 geometry tests (lines ~1090-1500)

Tests: `b11_ethane_geometry`, `b11_benzene_geometry`, `b11_butane_geometry`, `b11_ammonia_geometry`, `b11_adamantane_geometry`, `b11_methanethiol_geometry`.

These should now compare directly against RDKit's reference geometry (not just our own rest params). With vdW included, geometries for larger molecules (butane, adamantane) will be more physically realistic — bond lengths may shift slightly from pure-bonded optima due to vdW pressure.

Update comments from "After bonded-only minimization" to "After full UFF minimization".

Tolerances may need slight widening for larger molecules:
- ethane, benzene: bonds ±0.005, angles ±1.5°
- butane: bonds ±0.01, angles ±2°
- adamantane: bonds ±0.015, angles ±3° (use `max_iterations: 2000`)

### Step 9: Bond/angle quality tests (lines ~1452-1510)

Tests: `b11_all_molecules_bonds_near_rest_length`, `b11_all_molecules_angles_near_equilibrium`.

These verify that minimized bonds/angles are near their UFF rest values. With vdW, bonds/angles will be slightly perturbed from rest (vdW pressure shifts atoms). **Widen tolerances**:
- Bond lengths: from 0.005-0.01 Å to 0.02 Å
- Angles: from 1-2° to 3°

### Step 10: Butane dihedral scan (lines ~1519-2085)

The bonded-only butane scan produces a symmetric 3-fold cosine (degenerate staggered minima, degenerate eclipsed maxima). With vdW, the profile becomes asymmetric:
- **Anti (180°)** is the global minimum (no 1-4 H-H steric clash)
- **Gauche (60°, 300°)** is higher than anti by 0.5-2.0 kcal/mol (moderate H-H clash)
- **Eclipsed (0°, 120°, 240°)** maxima are no longer degenerate — syn (0°) is highest

The scan tests need significant rework:

**`b9_butane_dihedral_scan_72_points`**: Keep the 72-point scan infrastructure but update the validation:
- Remove 3-fold symmetry tests (staggered minima are no longer degenerate)
- Replace with asymmetry tests:
  - `E(anti=180°) < E(gauche=60°)` — anti is lower
  - `E(gauche=60°) ≈ E(gauche=300°)` within 0.1 kcal/mol (mirror symmetry still holds)
  - `E(eclipsed=120°) < E(syn=0°)` — syn is highest barrier
- Validate against reference dihedral scan: `anti=0.0`, `gauche=1.40`, `eclipsed=4.68`, `syn=11.13` kcal/mol (relative). Tolerance ±1.0 kcal/mol for each.

**`b9_butane_cos3phi_shape`**: The cos(3φ) fit test should be removed or replaced. The full-UFF profile is NOT a pure cos(3φ) — it's a sum of the 3-fold bonded term and the asymmetric vdW term. Instead, test that the scan is smooth (adjacent points differ by < max_step kcal/mol) and that the overall shape is qualitatively correct (3 minima, 3 maxima, correct ordering).

**`b9_butane_bonded_barrier_less_than_reference`**: **Delete this test.** It was a one-sided inequality (bonded barrier < full barrier). With vdW, our barrier should approximately match the reference full barrier.

Replace with **`b9_butane_barrier_vs_reference`**: barrier height (syn energy - anti energy) should be within ±2.0 kcal/mol of the reference value (11.13 kcal/mol).

---

## 9. Implementation Order

| Step | What | Files | Depends On |
|------|------|-------|------------|
| 20a | Nonbonded pair enumeration | `topology.rs` | — |
| 20b | vdW combination rules | `params.rs` | — |
| 20c | vdW energy + gradient functions | `energy.rs` | 20b |
| 20d | Wire into UffForceField | `uff/mod.rs` | 20a, 20b, 20c |
| 20e-1 | New `uff_vdw_test.rs`: C1-C4 tests | `uff_vdw_test.rs`, `crystolecule.rs` | 20d |
| 20e-2 | Update `uff_force_field_test.rs` | `uff_force_field_test.rs` | 20d |
| 20e-3 | Update `minimize_test.rs` | `minimize_test.rs` | 20d |

Steps 20a and 20b can be done in parallel (no dependencies). Steps 20e-1, 20e-2, and 20e-3 can be done in any order after 20d, but it's recommended to do 20e-1 first (validates the basic vdW implementation) before updating existing tests that depend on it.

**Build and test after each step:**
```bash
cd /c/machine_phase_systems/flutter_cad/rust && cargo test
```

After 20d, existing tests in `uff_force_field_test.rs` and `minimize_test.rs` will **FAIL** (they compare against bonded-only values, but the force field now includes vdW). This is expected. Steps 20e-2 and 20e-3 fix these failures.

**Recommended approach**: Implement 20a-20d, then immediately do 20e-1 to validate the vdW implementation with new standalone tests, then update existing tests in 20e-2 and 20e-3.

---

## 10. Files Modified

| File | Changes |
|------|---------|
| `rust/src/crystolecule/simulation/topology.rs` | Add `NonbondedPairInteraction` struct, `nonbonded_pairs` field in `MolecularTopology`, `enumerate_nonbonded_pairs()` method |
| `rust/src/crystolecule/simulation/uff/params.rs` | Add `calc_vdw_distance()`, `calc_vdw_well_depth()` functions |
| `rust/src/crystolecule/simulation/uff/energy.rs` | Add `VdwParams` struct, `vdw_energy()`, `vdw_energy_and_gradient()` functions |
| `rust/src/crystolecule/simulation/uff/mod.rs` | Add `vdw_params` field, import new types, compute vdW params in `from_topology()`, add vdW loop in `energy_and_gradients()` |
| `rust/tests/crystolecule.rs` | Register `uff_vdw_test` module |
| `rust/tests/crystolecule/simulation/uff_vdw_test.rs` | **New file** — C1-C4 test series (~300-400 lines) |
| `rust/tests/crystolecule/simulation/uff_force_field_test.rs` | Update data structures, change all energy comparisons from `bonded` to `total`, change all gradient comparisons from `bonded` to `full`, update tolerances |
| `rust/tests/crystolecule/simulation/minimize_test.rs` | Update data structures, replace bonded-only workaround tests with proper full-energy comparisons, update geometry tests to compare against RDKit reference, rework butane scan for asymmetric profile |

No changes needed to:
- `rust/src/crystolecule/simulation/mod.rs` — `minimize_energy()` automatically gets vdW via `UffForceField`
- `rust/src/crystolecule/simulation/force_field.rs` — trait is unchanged
- `rust/src/crystolecule/simulation/minimize.rs` — optimizer is force-field-agnostic

---

## 11. Reference Sources

- **RDKit `Nonbonded.cpp/h`**: Download to `test_data/` before starting (see section 5, Phase 20c)
- **RDKit `Builder.cpp`**: Already downloaded. Lines 91-131 (`buildNeighborMatrix`), lines 433-467 (`addNonbonded`)
- **UFF paper**: Rappé et al. 1992 JACS, Eq. 20 (vdW energy), Table 1 (parameters)
- **Reference JSON**: `uff_reference.json` — `vdw_params`, `input_energy.total`, `input_gradients.full`, `minimized_energy.total`, `minimized_geometry`
- **Reference generator**: `generate_uff_reference.py` — lines 325-340 (vdW pair enumeration, `path_len >= 4`)

---

## 12. Risk Assessment

**Low risk**: The vdW term is the simplest energy term mathematically (just LJ 12-6 — no angular dependence, no special cases per coordination). The main complexity is pair enumeration, which is validated by exact expected pair counts (section 3).

**Low risk** (revised from medium): 1-4 pair inclusion is well-understood. Our pair enumeration (exclude 1-2 and 1-3, include everything else) exactly matches RDKit's C++ behavior. The reference JSON's `vdw_params` list excludes 1-4 pairs, but the energy values (`input_energy.total`) include them. We validate against the energy values, not the pair list. The pair count table in section 3 provides exact expected values for independent verification.

**Medium risk**: Test updates in `minimize_test.rs` are extensive. Many tests need conceptual rethinking (not just tolerance adjustment). The butane scan rework is the most complex change. However, these are test-only changes — the production code changes (sections 5a-5d) are minimal (~150 lines).

**Estimated total**: ~150-200 lines implementation + ~500-700 lines test changes (including both new and updated tests).
