# Energy Minimization for atomCAD

## 1. Research Summary

### The Problem

The atom_edit node lets users drag atoms to new positions, but those positions are imprecise. We need a "minimize" button that relaxes atom positions to lower-energy configurations. Requirements:

- **Generic**: Must handle any element (atomCAD targets diverse APM structures)
- **Easy to integrate**: Pure Rust, no external runtime dependencies (no Python, no C++ FFI)
- **Good enough, not perfect**: Interactive CAD quality, not publication-grade simulations

**Scope**: The initial implementation is **Tier 2: bond stretching, angle bending, torsion, and inversion.** No van der Waals, no electrostatics. These can be added later once the bonded terms are validated.

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

### Licensing

RDKit is **BSD-3-Clause** (permissive) — safe to port code structure and logic from. OpenBabel is **GPL-2** — use only as a cross-reference for understanding the math, do not copy code verbatim. The UFF parameter values are from a published paper (Rappé et al. 1992, JACS) and are public domain science.

### Error Handling for Unsupported Atom Types

UFF defines 126 atom types covering most of the periodic table, but some elements (e.g., copper, lanthanides) have no parameters or incomplete coverage. When the atom typer encounters an element/hybridization combination with no UFF parameters, the implementation should **return an error** (e.g., `Err("No UFF parameters for element Cu with 4 bonds")`). This propagates up to the node evaluation as a `NetworkResult::Error`, which the UI already displays. Do not silently skip atoms or use fallback parameters — incorrect silent behavior is worse than a clear error.

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

#### Phase 0 Completion Notes

**Status: COMPLETE.** Files generated:
- `rust/tests/crystolecule/simulation/test_data/generate_uff_reference.py`
- `rust/tests/crystolecule/simulation/test_data/uff_reference.json` (229 KB, 12,622 lines)

**What the JSON contains for each molecule:**
- `input_positions` — 3D coordinates from RDKit's ETKDGv3 distance geometry embedding (randomSeed=42). These are NOT at UFF equilibrium — they have non-trivial energy and gradients.
- `atoms` — per-atom info including `uff_type` (inferred, see caveat below)
- `bonds` — connectivity with bond orders
- `bond_params`, `angle_params`, `torsion_params`, `inversion_params`, `vdw_params` — per-interaction UFF parameters extracted from RDKit's `GetUFF*Params()` Python API
- `interaction_counts` — number of each interaction type
- `input_energy` — `{total, bonded, vdw}` where `bonded` was computed with `vdwThresh=0` to exclude vdW
- `input_gradients` — `{full, bonded}` — analytical gradients (dE/dx, negative of force) at input positions
- `gradient_verification` — numerical vs analytical gradient self-check (all passed, max rel error < 1e-5)
- `minimized_positions`, `minimized_energy`, `minimized_geometry` — after RDKit's UFF minimization
- `butane_dihedral_scan` — 72-point constrained scan (separate top-level key)

**Deviations from plan:**

1. **Per-component energies (item 6) partially available.** The RDKit Python API does not expose per-term-type energies (bond vs angle vs torsion separately). Instead, we store `bonded` (all bonded terms combined, excluding vdW) and `vdw` (the difference). Individual bond/angle/torsion/inversion energies will be validated against RDKit's C++ test values (section 2.2) which provide specific per-component reference numbers.

2. **Atom type labels are inferred, not from RDKit API.** `GetAtomLabel()` is not exposed in RDKit 2025.03.5 Python bindings. The script infers UFF types from element + hybridization (covers C, H, N, O, S). The definitive validation of atom typing is the per-interaction parameters — if our Rust typer produces types that yield the same kb, r0, ka, theta0 values, the typing is correct.

3. **Inversion params are inferred from UFF paper rules.** `GetUFFInversionParams()` returns None in RDKit 2025.03.5 Python bindings for all atoms (likely a wrapper bug). K values were set from the UFF paper: C sp2 → K=6.0, N sp2 → K=2.0. These are marked with `"source": "inferred"` in the JSON. Validate against RDKit's C++ test values (testUFFParamGetters: K=2.0 for amide nitrogen).

4. **Methanethiol (CH₃SH) chosen** as the "molecule with Si, S, or P" — tests sulfur atom typing and group 6 torsion handling.

**Validation performed on reference data:**
- Gradient self-check (numerical central difference vs analytical) passed for all 9 molecules
- Parameter spot-checks match RDKit C++ test values: C-C kb=699.59, r0=1.514; C=C kb=1034.69, r0=1.329; H-C-H theta0=109.47°
- Interaction counts are chemically correct (e.g., methane: 4 bonds, 6 angles, 0 torsions)
- All 9 minimizations converged
- Butane scan: anti=0.0, gauche=1.40, eclipsed=4.68, syn=11.13 kcal/mol (relative)

**How to use in Rust tests:**
```rust
// Load JSON, parse molecule data
// For each molecule:
//   1. Build AtomicStructure from input_positions + bonds
//   2. Run our atom typer → compare atom types
//   3. Compute UFF parameters → compare bond_params, angle_params, etc.
//   4. Evaluate energy at input_positions → compare input_energy.bonded
//   5. Evaluate gradients at input_positions → compare input_gradients.bonded
//   6. Run minimizer → compare minimized_energy.bonded, minimized geometry
// Note: our Tier 2 implementation has no vdW, so compare against "bonded" values.
```

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
| 20 | uff/energy.rs: van der Waals nonbonded terms + tests (see `doc/vdw_plan.md`) | Phase 15 |
| 21 | API + Flutter integration for atom_edit (see `doc/atom_edit_minimize_plan.md`) | Phase 20 |

**Guiding principle**: Every energy term is tested (gradient correctness + reference values) before the next term is implemented. Errors compound — a wrong bond stretch will make angle tests fail in confusing ways.

---

## 7. Practical Notes for Implementors

### Downloading RDKit C++ Source Files

Each phase requires reading specific RDKit C++ files. **Do not use LLM summarization tools (WebFetch, etc.) to read these files** — they lose numerical precision and skip critical details. Instead, download the raw files with `curl` and read them directly:

```bash
# Download to test_data/ for reference (these are BSD-3-Clause licensed)
cd rust/tests/crystolecule/simulation/test_data/

# Params (Phase 2 — already downloaded and verified)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Params.cpp" -o Params.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Params.h" -o Params.h

# Bond stretching (Phase 4)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/BondStretch.cpp" -o BondStretch.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/BondStretch.h" -o BondStretch.h

# Angle bending (Phase 6)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/AngleBend.cpp" -o AngleBend.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/AngleBend.h" -o AngleBend.h

# Torsion (Phase 8)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/TorsionAngle.cpp" -o TorsionAngle.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/TorsionAngle.h" -o TorsionAngle.h

# Inversion (Phase 10)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Inversion.cpp" -o Inversion.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/Inversion.h" -o Inversion.h

# Atom typer (Phase 12)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/GraphMol/ForceFieldHelpers/UFF/AtomTyper.cpp" -o AtomTyper.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/GraphMol/ForceFieldHelpers/UFF/AtomTyper.h" -o AtomTyper.h

# Builder — topology enumeration + param pre-computation (Phase 13, 15)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/GraphMol/ForceFieldHelpers/UFF/Builder.cpp" -o Builder.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/GraphMol/ForceFieldHelpers/UFF/Builder.h" -o Builder.h

# Test files with reference numerical values (Phase 3+)
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/ForceField/UFF/testUFFForceField.cpp" -o testUFFForceField.cpp
curl -sL "https://raw.githubusercontent.com/rdkit/rdkit/master/Code/GraphMol/ForceFieldHelpers/UFF/testUFFHelpers.cpp" -o testUFFHelpers.cpp

# OpenBabel cross-reference (single file, all energy terms)
curl -sL "https://raw.githubusercontent.com/openbabel/openbabel/master/src/forcefields/forcefielduff.cpp" -o forcefielduff.cpp
```

These downloaded `.cpp`/`.h` files are reference-only and should be listed in `.gitignore` (except `Params.cpp` which is used by `verify_params.py`). The Rust implementation must be original code — do not copy C++ verbatim, especially from OpenBabel (GPL-2).

### Verifying Ported Data Tables

When porting numerical tables (parameter values, test reference numbers), **always write a mechanical verification script** rather than relying on visual inspection. See `verify_params.py` for the pattern:

1. Download the C++ source to `test_data/`
2. Write a Python script that parses both the C++ source and the Rust file independently
3. Compare every value programmatically
4. Run the script and confirm zero mismatches

This catches transcription errors that are invisible to human review and undetectable by compilation.

### Phase Completion Status

| Phase | Status | Notes |
|-------|--------|-------|
| 0 | COMPLETE | `uff_reference.json` (229 KB), `generate_uff_reference.py` |
| 1 | COMPLETE | `simulation/` directory with 8 stub files, old Python stub deleted |
| 2 | COMPLETE | 127 entries, 3 constants, `get_uff_params()` lookup. Verified via `verify_params.py` (1,397 values, 0 errors) |
| 3 | COMPLETE | 27 tests (13 A1 param spot-checks + 14 A2 bond param tests). `calc_bond_rest_length()` and `calc_bond_force_constant()` ported from RDKit BondStretch.cpp. Cross-validated all 9 reference molecules' bond params against `uff_reference.json` (r0 tol=1e-4, kb tol=0.01). |
| 4 | COMPLETE | `BondStretchParams` struct, `bond_stretch_energy()` and `bond_stretch_energy_and_gradient()` in `uff/energy.rs`. Ported from RDKit BondStretch.cpp. Harmonic potential E=0.5·kb·(r-r0)², gradient with chain rule through distance. Degenerate zero-distance case handled (nudge, matching RDKit). |
| 5 | COMPLETE | 20 tests in `uff_energy_test.rs`. A3: energy at r=1.814 matches RDKit testUFF2 (E=31.4816, grad=±209.8775), equilibrium E=0, compressed bond, diagonal direction, multi-bond accumulation. A4: numerical vs analytical gradient (central difference h=1e-5, <1% rel error) for 10 bond types/geometries (C3-C3, C2=C2, C_R-C_R aromatic, O3-H, N3-H, C3-S, near-equilibrium, non-origin). Also tests gradient accumulation and degenerate zero-distance. |
| 6 | COMPLETE | `AngleBendParams` struct with `new()` constructor (pre-computes Fourier coefficients C0/C1/C2 for order=0). `angle_bend_energy()` and `angle_bend_energy_and_gradient()` in `uff/energy.rs`. Five coordination orders (0=general, 1=linear, 2=linear-cos2, 3=trigonal, 4=square). Angle correction penalty for near-zero angles (from OpenBabel). Chain rule gradient: dE/dTheta * dTheta/dx via dcos/dx. `calc_angle_force_constant()` and `ANGLE_CORRECTION_THRESHOLD` added to `params.rs`. |
| 7 | COMPLETE | 29 tests in `uff_angle_test.rs`. Force constant: amide C_R-N_R-C_3 ka=211.0 (matches RDKit testUFF3). Energy: equilibrium E=0 for all orders (0,2,3,4), displaced E>0, energy-only matches gradient version. A4: numerical vs analytical gradient (central difference h=1e-5, <1% rel error) for 12 configurations: orders 0-4, amide, H-C-H, 3D off-axis, near-equilibrium, non-origin. Force balance verified (gradient sum = 0). A5: steepest descent minimization (backtracking line search) matches RDKit testUFF4 targets: 90° (order 0), 90° 3D (order 0), 109.47° tetrahedral (order 0), 180° linear (order 2), 120° trigonal (order 3), 90° square planar (order 4). All converge to bond length 1.514±1e-3 and target angle ±1e-4 rad. |
| 8 | COMPLETE | `TorsionParams` struct with `new()` constructor. `torsion_energy()` and `torsion_energy_and_gradient()` in `uff/energy.rs`. `calc_torsion_params()` in `params.rs` covering sp3-sp3, sp2-sp2 (V=5.0 or V=2.0+isO+isN), sp2-sp3, and sp3-sp2 barrier heights. General group periodicity rules from Table I of UFF paper. |
| 9 | COMPLETE | 34 tests in `uff_torsion_test.rs`. Parameter tests: sp3-sp3, sp2-sp2, sp2-sp3, sp3-sp2, group 16, O-sp3 pairs. Energy: eclipsed E>0, staggered E≈0, C=C cis/trans. A4: numerical gradient (central difference h=1e-5, <1% rel error) for 10 geometries: ethane, ethylene, ONNO, 3D off-axis, near-equilibrium, non-origin, collinear, large torsion, asymmetric. Force balance and gradient accumulation verified. |
| 10 | COMPLETE | `InversionParams` struct, `calculate_cos_y()`, `inversion_energy()`, `inversion_energy_and_gradient()` in `uff/energy.rs`. `calc_inversion_coefficients_and_force_constant()` in `params.rs` for C/N/O (K=6, or K=50 for C=O) and group 15 (P/As/Sb/Bi from equilibrium angle). Wilson angle geometry: Y = angle between IJK-plane normal and J→L bond. Discovered and corrected sign bug in RDKit's C2 gradient term (harmless for C/N/O where C2=0, wrong for group 15). |
| 11 | COMPLETE | 32 tests in `uff_inversion_test.rs`. Coefficient tests: C, N, O, C=O, P, As, Sb, Bi (8 tests). calculate_cos_y: planar, perpendicular, 45°, collinear, zero-distance (5 tests). Energy: planar E=0, out-of-plane E>0, C=O higher, energy-only matches gradient (4 tests). A4: numerical gradient (central difference h=1e-5, <1% rel error) for 10 configs: C sp2, C=O, N sp2, P, Bi, 3D off-axis, non-origin, large displacement, near-planar, asymmetric. Force balance (C and P), gradient accumulation, planar equilibrium, three-permutations sum. |
| 12 | COMPLETE | `assign_uff_type(atomic_number, bonds)` and `assign_uff_types(atomic_numbers, bond_lists)` in `uff/typer.rs`. Covers all 103 elements (Z=1–103) mapping to UFF parameter table labels. Hybridization inferred from bond orders: aromatic→_R, triple→_1, double→_2, single→_3. Special handling for C/N/O/S (hybridization-dependent), forced-sp3 elements (Si, Ge, Sn, P, Al, Mg, etc.), metals with valence-dependent geometry (Ti, Fe, Mo, W, Re), and charge suffixes. Also: `bond_order_to_f64()` for InlineBond→f64 conversion, `hybridization_from_label()` for label→hybridization extraction. 45 tests: per-element typing (C/N/O/S with all hybridizations), RDKit testUFFTyper1 reference values, cross-validation of all 9 reference molecules from `uff_reference.json`, batch assignment, bond order conversion, hybridization extraction, metals, lanthanides, actinides, noble gases, error cases, deleted bond handling. |
| 13 | COMPLETE | `MolecularTopology` struct with `from_structure(&AtomicStructure)` in `topology.rs`. Enumerates bonds, angles, torsions, and inversions from the bond graph. `BondInteraction`, `AngleInteraction`, `TorsionInteraction`, `InversionInteraction` structs with topology indices. Atom ID mapping (`atom_ids`), atomic numbers, and flat position array extracted. Adjacency-based enumeration: angles = C(n_bonds,2) per vertex, torsions = all i-j-k-l chains per central bond (skips degenerate i==l in 3-rings), inversions = 3 permutations per sp2 center (C/N/O with double/aromatic bonds, or group 15 P/As/Sb/Bi with 3 bonds). |
| 14 | COMPLETE | 37 tests in `topology_test.rs`. B8: interaction counts for all 9 reference molecules from `uff_reference.json` (methane, ethylene, ethane, benzene, butane, water, ammonia, adamantane, methanethiol). RDKit testUFFBuilder1 cases: CC(O)C (3 bonds, 3 angles, 0 torsions), CCOC (3 bonds, 2 angles, 1 torsion). Edge cases: empty structure, single atom, two atoms, 3/4/5/6-membered rings. Inversion tests: sp2 C/N with double/aromatic bonds, sp3 C/N without, phosphorus group 15, P with 4 bonds. Integrity: index bounds, bond order preservation, position/ID/atomic number mapping, angle count formula C(n,2), inversion permutation structure. |
| 15 | COMPLETE | `UffForceField` struct in `uff/mod.rs` implementing `ForceField` trait. `from_topology(&MolecularTopology)` constructor: assigns UFF atom types, pre-computes all bond stretch, angle bend, torsion, and inversion parameters. Angle coordination order from vertex hybridization (SP→1, SP2/R→3, SP3D2→4, else→0). Torsion filtering: only SP2/SP3 central atoms (matching RDKit Builder.cpp). Torsion force constant scaling: divided by count of torsions per central bond (matching RDKit's `scaleForceConstant`). Inversion: detects sp2 C bound to sp2 O for K=50 case. `energy_and_gradients()` sums all four bonded terms. 33 tests in `uff_force_field_test.rs`: construction for all 9 reference molecules, parameter count validation, energy at input positions matches RDKit bonded energy (all 9 molecules, tol ≤0.1 kcal/mol), gradients match RDKit bonded gradients (all 9 molecules), numerical gradient verification (central difference, <1% rel error) for 5 molecules, force balance (gradient sum = 0) for all 9, torsion scaling uniformity, deterministic evaluation. |
| 16 | COMPLETE | `minimize_with_force_field()` in `minimize.rs`. L-BFGS two-loop recursion with backtracking Armijo line search. `MinimizationConfig` struct (max_iterations=500, gradient_rms_tolerance=1e-4, memory_size=8). `LbfgsResult` return type. Frozen atom support via gradient zeroing. No external optimizer dependency — algorithm implemented from scratch (~200 lines). Descent direction reset (steepest descent fallback) when L-BFGS direction is not a descent direction. Curvature condition check before storing (s,y) pairs. 27 tests in `minimize_test.rs`: 6 unit tests on quadratic/Rosenbrock functions, 4 frozen-dimension tests, 1 max-iterations test, 1 energy-decrease test, 8 per-molecule UFF convergence tests, 1 all-molecules convergence test, 1 minimized-energy-vs-reference test, 3 minimized-geometry tests (methane, ethylene, water bond lengths/angles), 2 frozen-atom UFF tests. All 9 reference molecules converge with default config. |
| 17 | COMPLETE | `minimize_energy()` in `simulation/mod.rs` wired up: builds `MolecularTopology` from `AtomicStructure`, constructs `UffForceField`, runs L-BFGS with default config (no frozen atoms), writes optimized positions back via `set_atom_position()`. Empty structure handled (returns early). Error propagation from `UffForceField::from_topology()`. Human-readable message with convergence status, iteration count, and final energy. The `relax` node is now fully functional — no changes needed to the node itself. |
| 18 | COMPLETE | 13 tests added to `minimize_test.rs` (total now 40). **B7**: ethylene planarity (all 6 atoms coplanar within 0.01 Å), C-H bond length uniformity. **B10**: energy decreases from input for all 9 molecules, energy self-consistency (recomputed at minimized positions matches reported), bonded-only minimum ≤ RDKit's bonded energy at vdW-optimized geometry (validates our bonded-only optimizer finds lower bonded energy than RDKit's vdW-constrained result). **B11**: end-to-end geometry for all 9 molecules — ethane (bonds ±0.002, angles ±1°), benzene (bonds ±0.002, angles ±0.5°, planarity), butane (bonds ±0.005, angles ±2°), ammonia (bonds ±0.001, angles ±1°, C3v symmetry), adamantane (bonds ±0.01, angles ±2°, 2000 iter), methanethiol (bonds ±0.002, angles ±1.5°). All-molecules bond rest length check (tol 0.005-0.01 Å), all-molecules angle equilibrium check (tol 1-2°). Note: 16 testGitHubIssue62 molecules require vdW (Tier 3) and are deferred. |
| 19 | COMPLETE | 7 tests added to `minimize_test.rs` (total now 47). **B9**: butane 72-point dihedral scan. Constrained scan freezes all 4 carbons, rotates C3 group in 5° steps (0°-355°), minimizes H positions at each angle. Validates bonded-only 3-fold cosine profile: barrier height matches UFF V=2.119 kcal/mol (±0.01), three-fold symmetry (eclipsed maxima equal within 0.05, staggered minima equal within 0.05), cos(3φ) shape fit (max residual <0.05 kcal/mol), smoothness (adjacent points <0.5 kcal/mol), rotation accuracy (<0.1° error at 10 targets), frozen carbon preservation (<1e-12 displacement), bonded barrier < reference full-UFF barrier (2.119 < 4.68). Note: bonded-only profile is perfectly symmetric 3-fold — anti/gauche asymmetry requires vdW (Tier 3). |
| 20 | COMPLETE | vdW nonbonded terms (see `doc/vdw_plan.md`). Phase 1: `NonbondedPairInteraction` in topology.rs, `calc_vdw_distance()`/`calc_vdw_well_depth()` in params.rs, `VdwParams`/`vdw_energy()`/`vdw_energy_and_gradient()` in energy.rs, vdw_params in UffForceField. Phase 2: uff_force_field_test.rs updated to compare against `input_energy.total` and `input_gradients.full`. Phase 3: minimize_test.rs updated — data structures include total energy, convergence tests use total, `uff_minimized_bonded_energy_near_zero` replaced with `uff_minimized_total_energy_vs_reference`, `b10_bonded_minimum_leq_rdkit_bonded` replaced with `b10_minimized_energy_matches_reference`, geometry tolerances widened for vdW pressure, butane scan reworked for asymmetric profile (anti < gauche < eclipsed < syn). All 1302 tests pass. |
| 21 | Not started | API + Flutter atom_edit integration (see `doc/atom_edit_minimize_plan.md`) |

---

## 8. References

- **Primary porting source**: RDKit `Code/ForceField/UFF/` — [GitHub](https://github.com/rdkit/rdkit/tree/master/Code/ForceField/UFF)
- **Atom typer source**: RDKit `Code/GraphMol/ForceFieldHelpers/UFF/AtomTyper.cpp`
- **Builder source**: RDKit `Code/GraphMol/ForceFieldHelpers/UFF/Builder.cpp`
- **Secondary reference**: OpenBabel `src/forcefields/forcefielduff.cpp` — [GitHub](https://github.com/openbabel/openbabel/blob/master/src/forcefields/forcefielduff.cpp)
- **UFF paper**: Rappé et al., "UFF, a Full Periodic Table Force Field for Molecular Mechanics and Molecular Dynamics Simulations", JACS 1992, 114, 10024-10035
- **IM-UFF paper**: Jaillet et al. 2017, "IM-UFF: Extending the Universal Force Field for Interactive Molecular Modeling" — validates the interactive editing use case
- **RDKit test data**: `Code/ForceField/UFF/testUFFForceField.cpp`, `Code/GraphMol/ForceFieldHelpers/UFF/testUFFHelpers.cpp`
- **OpenBabel test data**: `test/ffuff.cpp`, `test/files/forcefield.sdf`, `test/files/uffresults.txt`

---

## 9. Interactive Minimization (future, but architecturally planned for)

A natural evolution of the button-press approach: as the user drags an atom, the surrounding structure continuously relaxes in real-time (as SAMSON supports via IM-UFF).

**Performance is not a concern** for molecules under ~500 atoms. UFF energy+gradient evaluation is cheap arithmetic (dot products, trig functions). For 200 atoms, a single evaluation takes microseconds. At 30 FPS (~33ms per frame), running 5-10 relaxation steps per frame is comfortably within budget. The IM-UFF paper achieved real-time on molecules with thousands of atoms.

**The current plan's architecture supports this without fundamental changes.** What interactive mode adds:

1. **"Run N steps" mode** — the minimizer takes `max_iterations` as a parameter. Run 5-10 steps per frame instead of running to convergence. This is a parameter change, not an architectural change.
2. **Topology caching** — build the `UffForceField` once when the user starts dragging, reuse across frames. Only rebuild if bonds are created/broken. The `UffForceField` struct is already designed for this (constructed once, `energy_and_gradients()` called repeatedly with different positions).
3. **Dragged atom as frozen** — the atom under the cursor is pinned at the cursor position, everything else relaxes. Same frozen-atom mechanism as the button-press mode, just a different frozen set.

Nothing in the ForceField trait, energy terms, topology builder, or parameter table needs to change for interactive mode.

---

## 10. Other Future Work (not in this plan)

- **Van der Waals** (Tier 3) — Phase 20, see `doc/vdw_plan.md`
- **Molecular dynamics** — velocity Verlet integration with thermostat
- **DREIDING force field** — alternative FF; could use `dreid-kernel` crate or implement similarly
- **Explicit per-atom freeze flags** — user selects atoms to freeze before minimizing
- **Constraint support** — fix bond lengths, angles, or planes during minimization
