# Simulation Module - Agent Instructions

Energy minimization for atomCAD using the Universal Force Field (UFF). Provides `minimize_energy()` to relax atomic structures to lower-energy configurations.

## Subdirectory Instructions

- Working in `uff/` → Read `uff/AGENTS.md`

## Module Structure

```
simulation/
├── mod.rs          # Public API: minimize_energy(), MinimizationResult
├── force_field.rs  # ForceField trait (energy_and_gradients)
├── topology.rs     # MolecularTopology: interaction list enumeration
├── minimize.rs     # L-BFGS optimizer with frozen atom support
└── uff/            # UFF force field implementation (see uff/AGENTS.md)
```

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `MinimizationResult` | `mod.rs` | Result struct: energy, iterations, converged, message |
| `ForceField` (trait) | `force_field.rs` | `energy_and_gradients(&self, positions, energy, gradients)` |
| `MolecularTopology` | `topology.rs` | Flat arrays of positions, atomic numbers, atom IDs, and interaction lists (bonds, angles, torsions, inversions, nonbonded pairs) |
| `MinimizationConfig` | `minimize.rs` | L-BFGS settings: max_iterations=500, gradient_rms_tolerance=1e-4, memory_size=8 |
| `LbfgsResult` | `minimize.rs` | Optimizer output: energy, iterations, converged |

## Data Flow

```
AtomicStructure
  → MolecularTopology::from_structure()      # Flatten atoms, enumerate interactions
  → UffForceField::from_topology()           # Assign atom types, pre-compute parameters
  → minimize_with_force_field()              # L-BFGS optimization
  → Write positions back to AtomicStructure
```

## Public API

```rust
// Main entry point (used by relax node)
pub fn minimize_energy(structure: &mut AtomicStructure) -> Result<MinimizationResult, String>

// Lower-level (used by atom_edit minimize)
pub fn minimize_with_force_field(ff: &impl ForceField, positions: &mut [f64],
    config: &MinimizationConfig, frozen: &[usize]) -> LbfgsResult
```

## MolecularTopology

Built from `AtomicStructure` bond graph via `from_structure()`. Enumerates:

- **Bonds**: direct connections from `InlineBond`
- **Angles**: C(n_bonds, 2) per vertex (all pairs of bonds at each atom)
- **Torsions**: all i-j-k-l chains per central bond (sp2/sp3 central atoms only)
- **Inversions**: 3 permutations per sp2 center (C/N/O with double/aromatic, or group 15 P/As/Sb/Bi with 3 bonds)
- **Nonbonded pairs**: all (i,j) where i < j, excluding 1-2 (bonded) and 1-3 (angle endpoints)

Positions stored as flat `Vec<f64>` in `[x0,y0,z0,x1,y1,z1,...]` layout. `atom_ids: Vec<u32>` maps topology index → structure atom ID.

## L-BFGS Minimizer

Custom implementation (~200 lines), no external optimizer dependency. Features:
- Two-loop recursion with backtracking Armijo line search
- Frozen atom support via gradient zeroing
- Steepest descent fallback when L-BFGS direction is not a descent direction
- Curvature condition check before storing (s,y) correction pairs

## Frozen Atom Support

Frozen atoms have their gradient components zeroed, so the optimizer never moves them. Used by:
- `minimize_energy()`: no frozen atoms (relax node)
- `minimize_atom_edit()`: FreezeBase mode freezes base atoms, FreeAll mode freezes none

## Integration Points

- **Relax node** (`structure_designer/nodes/relax.rs`): calls `minimize_energy()` on standalone structures
- **atom_edit node** (`structure_designer/nodes/atom_edit/atom_edit.rs`): calls `minimize_with_force_field()` with frozen atom support, writes positions back to diff
- **API** (`api/structure_designer/atom_edit_api.rs`): `atom_edit_minimize(freeze_mode)` exposes to Flutter

## Testing

Tests in `rust/tests/crystolecule/simulation/`. Registered in `rust/tests/crystolecule.rs`.

```
tests/crystolecule/simulation/
├── uff_params_test.rs          # Parameter table spot-checks (27 tests)
├── uff_energy_test.rs          # Bond stretch energy + gradient (20 tests)
├── uff_angle_test.rs           # Angle bend energy + gradient (29 tests)
├── uff_torsion_test.rs         # Torsion energy + gradient (34 tests)
├── uff_inversion_test.rs       # Inversion energy + gradient (32 tests)
├── uff_typer_test.rs           # Atom type assignment (45 tests)
├── topology_test.rs            # Interaction enumeration (37 tests)
├── uff_force_field_test.rs     # Full force field energy + gradients (33 tests)
├── uff_vdw_test.rs             # Van der Waals tests (C1-C4 series)
├── minimize_test.rs            # L-BFGS + end-to-end minimization (47 tests)
└── test_data/
    ├── uff_reference.json      # Ground-truth from RDKit (9 molecules)
    ├── generate_uff_reference.py  # Script to regenerate reference data
    ├── verify_params.py        # Validates params.rs against RDKit source
    └── *.cpp / *.h             # Downloaded RDKit sources (reference only)
```

**Running:** `cd rust && cargo test crystolecule` (runs all ~300+ simulation tests)

**Reference data**: `uff_reference.json` contains per-molecule input positions, UFF atom types, interaction parameters, energies, gradients, and minimized geometries from RDKit. Tests validate against both bonded and total (bonded + vdW) values.

## Design Decisions

- **Pure Rust, no FFI**: No C++/Python dependencies. Portable across all platforms.
- **UFF chosen over DREIDING**: Covers entire periodic table (126 atom types). Well-documented (Rappé et al. 1992).
- **Ported from RDKit** (BSD-3-Clause): modular structure. Cross-referenced with OpenBabel (GPL-2, read-only reference).
- **No electrostatics**: Standard UFF practice. Can be added later.
- **Custom L-BFGS**: Avoids external optimizer crate dependency. ~200 lines.

## Detailed Plans

- `doc/energy_minimization_plan.md` — full implementation plan with phase details
- `doc/vdw_plan.md` — van der Waals implementation specifics
- `doc/atom_edit_minimize_plan.md` — atom_edit integration details
