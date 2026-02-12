# Energy Minimization for atomCAD

## 1. Research Summary

### The Problem

The atom_edit node lets users drag atoms to new positions, but those positions are imprecise. We need a "minimize" button that relaxes atom positions to lower-energy configurations. Requirements:

- **Generic**: Must handle any element (atomCAD targets diverse APM structures)
- **Easy to integrate**: Pure Rust, no external runtime dependencies (no Python, no C++ FFI)
- **Good enough, not perfect**: Interactive CAD quality, not publication-grade simulations

### Alternatives Considered

**FFI to C++ libraries (RDKit, OpenBabel, OpenMM, LAMMPS):**
Rejected. RDKit has battle-tested UFF and MMFF94, but adding a C++ dependency to the build is complex, fragile on Windows, and violates the "pure Rust backend" architecture. OpenMM/LAMMPS are even heavier.

**Python via pyo3 (the existing stub):**
The current `crystolecule/simulation.rs` tried this approach with OpenMM+OpenFF via pyo3. It was abandoned (commented out). Python runtime dependency is unacceptable for a shipped desktop app.

**Pure Rust crates — DREIDING (`dreid-kernel` + `dreid-typer`):**
These are pure Rust, MIT licensed, `no_std`, and ready-made. However they only implement DREIDING (fewer atom types, less universal). The crates are brand new (v0.3-0.4, Feb 2026) with minimal adoption. Viable fallback, but UFF is a better fit.

**Pure Rust crates — `potentials`, `lumol`:**
`potentials` provides mathematical primitives (LJ, harmonic bonds) but no atom typing or parameterization. `lumol` is a full simulation engine but alpha quality, unmaintained since 2024. Neither gives us a ready-to-use force field.

### Why UFF (Universal Force Field)?

1. **Covers the entire periodic table** with a single compact parameter table (126 atom types, 11 values each). All cross-parameters are derived from per-atom values via combination rules — no massive pair/triple tables.
2. **Well-documented**: Original paper (Rappé et al. 1992, JACS) has ~4,000 citations. The math is straightforward: harmonic springs, cosine series, Lennard-Jones.
3. **Two clean reference implementations exist**: RDKit (~2,800 lines C++) and OpenBabel (~1,800 lines C++), both open source with test suites.
4. **Atom typing is simplified for atomCAD**: We already store explicit bond orders (including `BOND_AROMATIC`), so we don't need SMARTS perception or ring-finding. RDKit's 735-line atom typer reduces to ~150 lines for our case.
5. **Proven for interactive editing**: The IM-UFF paper (Jaillet et al. 2017) validates exactly our use case — user manipulates atoms, UFF provides corrective forces in real-time.
6. **Electrostatics can be skipped entirely** (standard practice — the original paper parameterized without them).

### Why Implement from Scratch (not use a crate)?

No pure-Rust UFF implementation exists. The DREIDING crates exist but cover a different force field. Implementing UFF is a well-scoped porting task (~1,500 lines of Rust) with excellent reference implementations and test data. Full control over the code means we can optimize for our specific use case (frozen atoms, incremental updates, etc.).

### Porting Source: RDKit structure, cross-referenced with OpenBabel

Two clean open-source UFF implementations exist. We use both:

**RDKit** (~2,800 lines C++, modular) — primary structural guide. Each energy term is a separate file (BondStretch.cpp, AngleBend.cpp, TorsionAngle.cpp, Inversion.cpp, Nonbonded.cpp). This modularity maps directly to our incremental implementation strategy: port one term, test it, then move to the next. RDKit's test suite also provides the most granular per-component reference values (exact bond rest lengths, force constants, energies at specific distances).

**OpenBabel** (~1,800 lines C++, single file) — cross-reference and independent validation. Easier to see the complete flow in one place. Its test suite validates analytical gradients numerically (forward difference, 5-8% tolerance per component) for all 18 test molecules — something RDKit's tests don't do. OpenBabel's 18-molecule regression set serves as an independent cross-validation dataset.

When porting a specific energy term, the implementor should read both RDKit's modular file and the corresponding section in OpenBabel's single file, and resolve any discrepancies by consulting the original UFF paper.

---

## 2. Numerical Correctness Strategy

This is the highest-risk aspect. A force field with subtle numerical errors in energy or gradients will produce plausible-looking but physically wrong geometries, and the errors may go unnoticed for months. The testing strategy must be rigorous.

### 2.0 Phase 0: Generate Ground-Truth Reference Data from RDKit

**Neither RDKit's nor OpenBabel's test suite provides end-to-end "input positions → expected output positions" tests.** They check energies at given positions, or geometric properties (bond lengths, angles) after minimization — but never absolute minimized coordinates. This is because different optimizers can take different paths to the same local minimum, so absolute positions aren't reproducible across implementations.

However, for our purposes, we can do better. Before writing any Rust code, we generate our own reference dataset using RDKit's Python bindings (easy: `pip install rdkit`). A Python script produces a JSON file containing, for each test molecule:

1. **Input atom positions** (3D coordinates from .mol/.sdf files)
2. **Per-atom UFF type assignments** (what RDKit's typer produces)
3. **Interaction term counts** (number of bonds, angles, torsions, inversions, nonbonded pairs)
4. **Per-term parameters** (r0, k for each bond; theta0, k for each angle; etc.)
5. **Total energy at the input positions** (before minimization)
6. **Per-component energies** (bond stretch, angle bend, torsion, inversion, vdw — separately)
7. **Analytical gradients at the input positions** (per-atom force vectors)
8. **Minimized atom positions** (after RDKit's UFF optimization)
9. **Total energy after minimization**
10. **Key geometric measurements after minimization** (bond lengths, angles, dihedrals)

This gives us layered validation at every stage of the pipeline:
- Wrong params? Caught at step 4.
- Wrong atom typing? Caught at steps 2-3.
- Wrong energy formula? Caught at steps 5-6.
- Wrong gradient? Caught at step 7 (and by numerical gradient tests).
- Wrong minimizer integration? Caught at steps 8-10.

**Test molecules for reference generation** (chosen to cover the key UFF code paths):
- Methane (CH₄) — tetrahedral sp3, simplest 3D molecule
- Ethylene (C₂H₄) — sp2 planar, double bond
- Ethane (C₂H₆) — sp3-sp3 torsion
- Benzene (C₆H₆) — aromatic, inversion terms
- Butane (C₄H₁₀) — gauche/anti torsion conformations
- Water (H₂O) — non-carbon, bent geometry
- Ammonia (NH₃) — nitrogen sp3, inversion
- A small diamond fragment (C₁₀H₁₆, adamantane) — directly relevant to atomCAD's use case
- A molecule with Si, S, or P — tests atom typing beyond C/H/N/O

The reference JSON file is checked into the repo at `rust/tests/crystolecule/simulation/test_data/uff_reference.json`. The Python generation script is checked in alongside it for reproducibility.

**This is the "no-bullshit" end-to-end test**: if our Rust implementation produces the same energy, gradients, and geometric properties as RDKit for these molecules, the port is correct.

### 2.1 The Core Guarantee: Numerical Gradient Verification

**Neither RDKit nor OpenBabel test analytical gradients against numerical gradients.** This is the single most important test we must add.

For every energy term (bond, angle, torsion, inversion, vdw), we verify:

```
analytical_gradient ≈ (E(x + h) - E(x - h)) / (2h)
```

Using central difference (more accurate than forward difference). Step size h = 1e-5 Å. Tolerance: relative error < 1% for each component. This catches sign errors, missing chain rule terms, and wrong coordinate transformations — the most common porting bugs.

OpenBabel uses forward difference with 5-8% tolerance per component. We should be stricter since we're starting fresh.

### 2.2 Porting RDKit's Test Suite

RDKit has two test files with concrete numerical values we can port directly:

**From `Code/ForceField/UFF/testUFFForceField.cpp`:**

| Test | What it validates | Key reference values |
|------|-------------------|---------------------|
| testUFFParams | Parameter lookup | C_3: r1=0.757, theta0=109.47°, x1=3.851, D1=0.105, Z1=1.912, V1=2.119 |
| testUFF1 | Bond rest length + force constant | C_3–C_3: r0=1.514 Å, k=699.5918; C_2=C_2: r0=1.32883, k=1034.69; C_3–N_3: r0=1.451071, k=1057.27 |
| testUFF2 | Bond stretch energy + gradient | At r=1.814: E=31.4816, gradient=±209.8775. At equilibrium: E=0 |
| testUFF3 | Angle force constant | C_3–C_3–C_3 with theta0=109.47°; amide C_R–N_R–C_3 with k=211.0 |
| testUFF4 | Angle minimization geometries | Targets: 90°, 109.47° (tetrahedral), 120° (trigonal), 180° (linear) |
| testUFF5/8 | Ethylene (C₂H₄) full optimization | C=C and C-H bond lengths, all angles = 120° |
| testUFF6 | Van der Waals equilibrium | C_3–C_3 equilibrium at d=3.851 Å |
| testUFF7 | Torsion for all hybridization pairs | sp3-sp3: cos(φ)=0.5; sp2-sp2: cos(φ)=1.0; sp2-sp3: cos(φ)=0.5; group6-group6: cos(φ)=0.0 |
| testUFFButaneScan | 72-point dihedral energy profile | Anti minimum ~1.76, gauche ~2.91, eclipsed ~5.78, syn ~9.46 kcal/mol. Tolerance ±0.2 |
| testGitHubIssue62 | 16 molecules with known energies | beta-lactam: 38.687, cyclopropane: 267.236, caffeine: 346.203, etc. Tolerance ±1.0 |

**From `Code/GraphMol/ForceFieldHelpers/UFF/testUFFHelpers.cpp`:**

| Test | What it validates | Key reference values |
|------|-------------------|---------------------|
| testUFFTyper1 | Atom type labels | C_3, C_R, C_2, O_R, O_2, N_R, Si3, S_3+2, S_3+4, S_3+6, P_3+3, P_3+5, F_, Cl, Br, I_, Li, Na, K_ |
| testUFFBuilder1 | Interaction term counting | CC(O)C: 3 bonds, 3 angles, 0 torsions. CCOC: 3 bonds, 2 angles, 1 torsion |
| testUFFParamGetters | Per-interaction parameters | C-C bond: kb=699.592, r0=1.514; C-C-N angle: ka=303.297, theta0=109.47°; torsion V=0.976; inversion K=2.0; vdW: x_ij=3.754, D_ij=0.085 |
| testUFFBuilderSpecialCases | Trigonal bipyramidal geometry | Axial-axial dot = -1.0, axial-eq = 0.0, eq-eq = -0.5 |

### 2.3 Porting OpenBabel's Test Data

OpenBabel tests 18 molecules from `test/files/forcefield.sdf` with expected energies in `test/files/uffresults.txt` (tolerance: 1e-3). Key molecules:

| Molecule | Energy (kcal/mol) |
|----------|-------------------|
| propylamine | 3.674 |
| acetamide | 20.677 |
| caffeine | 346.203 |
| cholesterol | 827.842 |
| C60 fullerene | 10563.001 |

**Important caveat**: RDKit and OpenBabel reference values are self-generated (each validates against its own previous output). They don't cross-validate against each other, and there are known discrepancies (e.g., amide angle force constant differs by 2x from the original paper). Our Rust implementation should validate against RDKit's per-component values (since we're using RDKit's modular structure as the primary guide) and against OpenBabel's whole-molecule energies as an independent cross-check. Where the two disagree, consult the original UFF paper.

### 2.4 Test Data Files to Include

Copy into `rust/tests/crystolecule/simulation/test_data/`:

- RDKit's 16 molecules from `Issue62.smi` (SMILES + expected energies)
- RDKit's `benzene.mol`, `toluene.mol`, `small1.mol` (propene), `tbp.mol` (trigonal bipyramidal)
- The butane scan reference data (72 dihedral/energy pairs)
- A few molecules from OpenBabel's `forcefield.sdf` for cross-validation

These mol/sdf files contain 3D coordinates, so we need XYZ or mol parsing (we already have XYZ import in `crystolecule::io`).

### 2.5 Testing Tiers

**Tier A — Must pass before any integration (blocks all other work):**
1. Parameter table spot-checks (exact values for C_3, N_3, O_3, H_, Si3)
2. Bond rest length and force constant calculations (testUFF1 values)
3. Bond stretch energy at specific distances (testUFF2 values)
4. **Numerical vs analytical gradient for every energy term** (our addition, not in RDKit)
5. Angle minimization to target geometries (90°, 109.47°, 120°, 180°)
6. Torsion equilibrium angles for all hybridization pairs

**Tier B — Must pass before shipping:**
7. Ethylene full optimization (bond lengths + angles)
8. Topology enumeration correctness (bond/angle/torsion counts match reference JSON)
9. Butane dihedral scan (72-point energy profile, ±0.2 kcal/mol)
10. Known-molecule energies (RDKit's 16 molecules, ±1.0 kcal/mol)
11. **End-to-end minimization against reference JSON**: for each molecule, minimize from input positions, compare output bond lengths/angles/dihedrals and final energy against RDKit's minimized results
12. Minimization convergence for adamantane (diamond fragment — directly relevant to atomCAD)

**Tier C — Nice to have:**
13. Cross-validation against OpenBabel's 18-molecule energies
14. Multithreaded reproducibility
15. Edge cases: zero-length bonds, linear molecules, missing parameters

---

## 3. Porting Map: RDKit + OpenBabel → Rust

### Source Structure

Primary source (RDKit, guides the file structure):
```
Code/ForceField/UFF/
├── Params.h + Params.cpp          → uff/params.rs
├── BondStretch.h + .cpp           → uff/energy.rs (bond section)
├── AngleBend.h + .cpp             → uff/energy.rs (angle section)
├── TorsionAngle.h + .cpp          → uff/energy.rs (torsion section)
├── Inversion.h + .cpp             → uff/energy.rs (inversion section)
├── Nonbonded.h + .cpp             → uff/energy.rs (vdw section)
Code/GraphMol/ForceFieldHelpers/UFF/
├── AtomTyper.h + .cpp             → uff/typer.rs (simplified)
├── Builder.h + .cpp               → topology.rs + uff/mod.rs
Code/ForceField/
├── ForceField.h + .cpp            → force_field.rs (trait) + minimize.rs
```

Cross-reference (OpenBabel, read alongside for each energy term):
```
src/forcefields/forcefielduff.cpp   # All energy terms in one file
src/forcefields/forcefielduff.h     # Structs for each interaction type
data/UFF.prm                        # Parameter table (142 entries with SMARTS)
```

### Target Structure (Rust)

```
crystolecule/simulation/
├── mod.rs              # Public API: minimize_energy(), MinimizationResult
│                       # Replaces the current simulation.rs Python stub
├── force_field.rs      # ForceField trait (energy_and_gradients)
├── topology.rs         # MolecularTopology: flatten AtomicStructure into interaction lists
│                       # Ported from: Builder.cpp topology enumeration
├── minimize.rs         # L-BFGS wrapper using `lbfgs` crate, frozen atom support
└── uff/
    ├── mod.rs          # UffForceField: implements ForceField trait
    │                   # Ported from: Builder.cpp (field construction + param pre-computation)
    ├── params.rs       # Static UFF_PARAMS table + lookup
    │                   # Ported from: Params.cpp (126 entries, verbatim values)
    ├── typer.rs        # assign_uff_type(atomic_number, bonds) → UffAtomType
    │                   # Simplified port from: AtomTyper.cpp
    └── energy.rs       # Energy terms + analytical gradients
                        # Ported from: BondStretch.cpp, AngleBend.cpp,
                        #              TorsionAngle.cpp, Inversion.cpp, Nonbonded.cpp
```

### Key Porting Notes

**Params.cpp → params.rs**: Nearly verbatim. The 126-entry table with 11 f64 values per entry. Store as `const UFF_PARAMS: &[UffParams] = &[...]`. Use a HashMap<&str, usize> for label→index lookup, built lazily or at init.

**AtomTyper.cpp → typer.rs**: Heavily simplified. RDKit's typer uses RDKit's molecule representation (hybridization enum, ring info, SMARTS). We use atomCAD's `(atomic_number, &[InlineBond])`. The mapping logic is the same but the input interface is different. Port the decision tree, not the RDKit-specific accessors.

**BondStretch.cpp → energy.rs**: Direct port. The `getEnergy()` and `getGrad()` methods become Rust functions. Key formulas: `calcBondRestLength()` and `calcBondForceConstant()` from Params.h. RDKit stores the bond contribution as a class with `d_at1Idx`, `d_at2Idx`, `d_r0`, `d_forceConstant`. In Rust, pre-compute these into a `BondParams` struct.

**AngleBend.cpp → energy.rs**: Most complex term due to 7 coordination geometry cases. Port the switch on `coordination` (linear/trigonal/tetrahedral/square planar). The cosine Fourier coefficients C0/C1/C2 are computed differently for each case. Cross-reference with OpenBabel's `OBFFAngleCalculationUFF::Compute()` for the same logic in a different style. Also port the empirical near-zero-angle correction from OpenBabel (exponential penalty for θ < ~30°) which RDKit does not include.

**TorsionAngle.cpp → energy.rs**: Port the switch on `(hybridization_j, hybridization_k)` for the central bond. Key function: `calcTorsionForceConstant()` which returns (V, n). Special handling for group 6 elements (O, S, Se, Te, Po).

**Inversion.cpp → energy.rs**: Port the Wilson angle calculation and the C0/C1/C2 coefficients for sp2 centers. Special values for C, N, O central atoms and group 15 elements (P, As, Sb, Bi).

**Builder.cpp → topology.rs + uff/mod.rs**: Split into two concerns. `topology.rs` handles the graph traversal (enumerate bonds, angles, torsions, inversions from the bond graph). `uff/mod.rs` handles parameter pre-computation for each interaction.

**ForceField.cpp → minimize.rs**: RDKit uses its own minimizer. We use the `lbfgs` crate instead. The interface is: flatten DVec3 positions into a &[f64], call the optimizer, unflatten back. Support frozen atoms by zeroing their gradient components.

---

## 4. Module Organization

### Location: `crystolecule/simulation/` (replace failed Python experiment)

The current `crystolecule/simulation.rs` is a failed experiment — a Python/pyo3 bridge to OpenMM+OpenFF that was never completed (all code is commented out, the function returns `Err("Not implemented yet")`). We delete this entirely and replace it with a `crystolecule/simulation/` directory containing the pure Rust UFF implementation.

The `relax` node (`structure_designer/nodes/relax.rs`) already imports `crate::crystolecule::simulation::minimize_energy` and calls it. By preserving the same `minimize_energy(&mut AtomicStructure) -> Result<MinimizationResult, String>` signature, the relax node works with the new implementation without any changes. The relax node is currently non-functional (returns an error) — this plan makes it functional.

### Why inside `crystolecule`?

- Replaces an existing module at the same path (backward compatible imports)
- Tightly coupled to `AtomicStructure`, `Atom`, `InlineBond` types
- Same precedent as `lattice_fill/` (5 files, substantial logic on crystolecule types)
- No circular dependency — simulation only depends on crystolecule types + glam + lbfgs

### New dependency

Add `lbfgs` crate to `rust/Cargo.toml`. Pure Rust port of libLBFGS. This is the standard optimizer for molecular geometry (same algorithm Avogadro 2 uses via cppoptlib).

---

## 5. Integration with atom_edit

### Frozen Atom Strategy

The minimizer takes a set of frozen atom indices as a parameter (empty set = nothing frozen). The atom_edit integration exposes this as a user-facing choice:

**Supported from the start:**

1. **Freeze base atoms** — only diff atoms (user-edited/added) move. Useful when the user wants to refine just their edits without disturbing the surrounding structure.
2. **No freezing** — all atoms move freely. Useful when the user wants the whole neighborhood to relax after an edit, as atoms would in reality.

**Future extensions (not in this plan):**

3. **Explicit freeze flags** — user selects specific atoms and marks them frozen before minimizing. Requires UI for setting per-atom freeze state.

The frozen set is a parameter passed to the minimizer, not hardcoded. The API and UI determine which mode to use; the minimizer itself is agnostic.

### The Anchor Problem

The atom_edit diff uses spatial matching via anchors. When minimization moves atoms:

- Atoms already in the diff (moved/added by user): their positions update, anchors stay intact. This already works — `move_in_diff()` sets the anchor on first move, subsequent position changes don't touch it.
- Base atoms that were NOT in the diff but moved by minimization (in "no freezing" mode): these must be added to the diff with anchors at their original positions, so `apply_diff()` still matches them correctly. This is the same operation as `apply_transform` already does when the user transforms base atoms.

### API Surface

The existing `minimize_energy(&mut AtomicStructure) -> Result<MinimizationResult, String>` signature stays for the relax node (no frozen atoms — the relax node operates on a standalone structure).

A new `minimize_atom_edit(node_id, freeze_mode)` API function handles the atom_edit case: evaluates the full structure, runs the minimizer with the chosen freeze strategy, and writes moved positions back into the diff.

---

## 6. Implementation Order

| Phase | What | Depends on |
|-------|------|-----------|
| 0 | **Generate reference data**: Python script using RDKit to produce uff_reference.json (energies, gradients, minimized positions for ~9 molecules). Check in both script and JSON. | — |
| 1 | Module scaffold: simulation.rs → simulation/ directory, delete Python stub, preserve public API | — |
| 2 | uff/params.rs: port parameter table from RDKit Params.cpp | — |
| 3 | **Tests A1-A2**: parameter spot-checks, bond rest length + force constant values. Cross-check atom types and per-term params against reference JSON. | Phase 0, 2 |
| 4 | uff/energy.rs: bond stretching energy + gradient | Phase 2 |
| 5 | **Tests A3-A4**: bond stretch energy values, numerical vs analytical gradient | Phase 4 |
| 6 | uff/energy.rs: angle bending energy + gradient | Phase 2 |
| 7 | **Tests A4-A5**: angle gradient verification, angle minimization targets | Phase 6 |
| 8 | uff/energy.rs: torsion energy + gradient | Phase 2 |
| 9 | **Tests A4,A6**: torsion gradient verification, equilibrium angles by hybridization | Phase 8 |
| 10 | uff/energy.rs: inversion energy + gradient | Phase 2 |
| 11 | **Tests A4**: inversion gradient verification | Phase 10 |
| 12 | uff/typer.rs: atom typing from connectivity | Phase 2 |
| 13 | topology.rs: build interaction lists from AtomicStructure | Phase 12 |
| 14 | **Tests B8**: topology enumeration counts for known molecules | Phase 13 |
| 15 | uff/mod.rs: UffForceField struct, ForceField impl | Phases 4-11, 13 |
| 16 | minimize.rs: L-BFGS wrapper with frozen atom support | Phase 15 |
| 17 | mod.rs: wire up minimize_energy() public API | Phase 16 |
| 18 | **Tests B7,B10,B11**: ethylene optimization, known-molecule energies, convergence tests | Phase 17 |
| 19 | **Tests B9**: butane 72-point dihedral scan | Phase 17 |
| 20 | API + Flutter integration for atom_edit | Phase 17 |
| 21 | (Later) uff/energy.rs: van der Waals nonbonded terms | Phase 15 |

**Guiding principle**: Every energy term is tested (gradient correctness + reference values) before the next term is implemented. Errors compound — a wrong bond stretch will make angle tests fail in confusing ways.

---

## 7. References

- **Primary porting source**: RDKit `Code/ForceField/UFF/` — [GitHub](https://github.com/rdkit/rdkit/tree/master/Code/ForceField/UFF)
- **Atom typer source**: RDKit `Code/GraphMol/ForceFieldHelpers/UFF/AtomTyper.cpp`
- **Builder source**: RDKit `Code/GraphMol/ForceFieldHelpers/UFF/Builder.cpp`
- **Secondary reference**: OpenBabel `src/forcefields/forcefielduff.cpp` — [GitHub](https://github.com/openbabel/openbabel/blob/master/src/forcefields/forcefielduff.cpp)
- **UFF paper**: Rappé et al., "UFF, a Full Periodic Table Force Field for Molecular Mechanics and Molecular Dynamics Simulations", JACS 1992, 114, 10024-10035
- **IM-UFF paper**: Jaillet et al. 2017, "IM-UFF: Extending the Universal Force Field for Interactive Molecular Modeling" — validates the interactive editing use case
- **RDKit test data**: `Code/ForceField/UFF/testUFFForceField.cpp`, `Code/GraphMol/ForceFieldHelpers/UFF/testUFFHelpers.cpp`
- **OpenBabel test data**: `test/ffuff.cpp`, `test/files/forcefield.sdf`, `test/files/uffresults.txt`

---

## 8. Future Work (not in this plan)

- **Van der Waals** (Tier 3) — add after bonded terms are validated; handles steric clashes
- **Molecular dynamics** — velocity Verlet integration with thermostat
- **DREIDING force field** — alternative FF; could use `dreid-kernel` crate or implement similarly
- **Interactive minimization** — run batches of 5-10 steps, update display in real-time (IM-UFF approach)
- **Selective minimization** — minimize only selected atoms in atom_edit
- **Constraint support** — fix bond lengths, angles, or planes during minimization
