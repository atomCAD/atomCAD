# UFF Module - Agent Instructions

Universal Force Field (UFF) implementation. Ported from RDKit's modular C++ code (BSD-3-Clause), cross-referenced with OpenBabel.

## Module Structure

```
uff/
├── mod.rs      # UffForceField struct, ForceField trait impl
├── params.rs   # Static parameter table (126 atom types) + derived calculations
├── typer.rs    # Atom type assignment from element + bond connectivity
└── energy.rs   # Five energy terms with analytical gradients
```

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `UffForceField` | `mod.rs` | Pre-computed parameters for all interactions. Implements `ForceField` trait |
| `UffAtomParams` | `params.rs` | Per-atom-type parameters: r1, theta0, x1, d1, zeta, z1, v1, u1, xi, hard, radius |
| `BondStretchParams` | `energy.rs` | Pre-computed bond: idx1, idx2, r0, force_constant |
| `AngleBendParams` | `energy.rs` | Pre-computed angle: indices, force_constant, theta0, order, C0/C1/C2 |
| `TorsionAngleParams` | `energy.rs` | Pre-computed torsion: indices, V, n, phi0 |
| `InversionParams` | `energy.rs` | Pre-computed inversion: indices, K, C0/C1/C2 |
| `VdwParams` | `energy.rs` | Pre-computed vdW pair: idx1, idx2, x_ij, d_ij |

## UffForceField Construction

`UffForceField::from_topology(topology)` performs these steps:
1. Assign UFF atom types via `assign_uff_types()` (typer.rs)
2. Pre-compute bond stretch parameters (r0, kb from combination rules)
3. Pre-compute angle bend parameters (theta0, ka, Fourier coefficients by coordination order)
4. Pre-compute torsion parameters (V, n from hybridization of central bond atoms)
5. Pre-compute inversion parameters (K, C0/C1/C2 for sp2 centers)
6. Pre-compute vdW parameters (x_ij, D_ij from geometric mean combination rules)

Torsion force constants are **scaled by the count of torsions per central bond** (matching RDKit's `scaleForceConstant`).

## Energy Terms (energy.rs)

Each term has two functions: `*_energy()` (energy only) and `*_energy_and_gradient()` (energy + gradient accumulation).

### Bond Stretch
- Harmonic: `E = 0.5 * kb * (r - r0)^2`
- Rest length from combination rule: `r0 = r_i + r_j + r_BO + r_EN` (bond order + electronegativity corrections)
- Force constant: `kb = 664.12 * (z_i * z_j) / r0^3`

### Angle Bend
- Five coordination orders: general (0), linear (1), linear-cos2 (2), trigonal (3), square planar (4)
- General: Fourier expansion `E = ka * (C0 + C1*cos(theta) + C2*cos(2*theta))`
- Force constant: `ka = 664.12 * beta / (r_ij * r_jk) * ...`
- Near-zero angle correction (exponential penalty, from OpenBabel)

### Torsion
- Cosine: `E = 0.5 * V * (1 - cos(n * phi) * cos_phi0)`
- Barrier height V depends on hybridization pair of central bond atoms (sp3-sp3, sp2-sp2, sp2-sp3)
- Special rules for group 16 elements (O, S, Se, Te, Po)

### Inversion (Out-of-plane)
- Wilson angle Y between IJK-plane normal and J→L bond
- `E = K * (C0 + C1*sin(Y) + C2*cos(2*Y))`
- Special K values: C sp2 → 6.0, C=O → 50.0, N sp2 → 2.0, group 15 from equilibrium angle

### Van der Waals (Lennard-Jones 12-6)
- `E = D_ij * [-2*(x_ij/r)^6 + (x_ij/r)^12]`
- Combination rules: `x_ij = sqrt(x_i * x_j)`, `D_ij = sqrt(D_i * D_j)`
- Applied to all 1-4+ atom pairs (excludes 1-2 bonded and 1-3 angle pairs)

## Atom Typer (typer.rs)

`assign_uff_type(atomic_number, bonds)` maps element + connectivity to one of 126 UFF labels (e.g., `C_3`, `C_R`, `N_2`, `H_`).

- Hybridization inferred from bond orders: aromatic → `_R`, triple → `_1`, double → `_2`, single → `_3`
- Special handling for C/N/O/S (hybridization-dependent), metals (valence-dependent geometry)
- Covers all elements Z=1-103

## Parameter Table (params.rs)

- 126 entries + 1 special (He_4_4), each with 11 f64 values from Rappé et al. 1992
- Lookup: `get_uff_params(label) -> Option<&UffAtomParams>`
- Constants: `BOND_ORDER_CORRECTION_CONSTANT = -0.1332`, `ELECTRONEGATIVITY_CONSTANT = 0.043844`
- Derived functions: `calc_bond_rest_length()`, `calc_bond_force_constant()`, `calc_angle_force_constant()`, `calc_torsion_params()`, `calc_inversion_coefficients_and_force_constant()`, `calc_vdw_distance()`, `calc_vdw_well_depth()`
- `Hybridization` enum: SP, SP2, SP3, Aromatic, SP3D2

## Gradient Implementation

All gradients use chain rule from internal coordinates to Cartesian. Pattern:
```
dE/dx_i = (dE/dq) * (dq/dx_i)
```
where q is the internal coordinate (distance, angle, dihedral, Wilson angle).

Every gradient implementation is validated with central difference numerical tests (h=1e-5, <1% relative error).

## Numerical Validation

All values validated against RDKit's C++ test suite and `uff_reference.json`:
- Parameter spot-checks match RDKit exactly
- Bond/angle/torsion/inversion energies match reference values
- Full force field energies match RDKit (bonded + vdW) for 9 test molecules
- Gradients match RDKit and pass numerical gradient verification
- Minimized geometries match RDKit's optimized structures

## Modifying This Module

**Adding a new energy term**: Add struct + energy/gradient functions in `energy.rs`, add parameter pre-computation in `mod.rs`, add loop in `energy_and_gradients()`. Write numerical gradient tests.

**Adding a new force field**: Implement `ForceField` trait in a new submodule. The optimizer (`minimize.rs`) is force-field-agnostic.

**Updating parameters**: Modify `UFF_PARAMS` in `params.rs`. Re-run `verify_params.py` to validate against RDKit source.
