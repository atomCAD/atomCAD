# Crystolecule Module - Agent Instructions

The crystolecule module implements atomic structure representation, crystal lattice geometry, lattice-filling algorithms, and energy minimization for atomically precise manufacturing (APM).

## Subdirectory Instructions

- Working in `simulation/` or any descendant → Read `simulation/AGENTS.md`
- Working in `simulation/uff/` → Also read `simulation/uff/AGENTS.md`

## Module Structure

```
crystolecule/
├── mod.rs                          # Module declarations (all submodules pub)
├── atomic_constants.rs             # Element database (symbol, radius, color)
├── atomic_structure_utils.rs       # Auto-bonding, selection, cleanup helpers
├── crystolecule_constants.rs       # Diamond unit cell size, default motif text
├── drawing_plane.rs                # 2D drawing plane embedded in 3D crystal
├── motif.rs                        # Motif struct (sites, bonds, parameters)
├── motif_parser.rs                 # Text format parser for motifs
├── guided_placement.rs             # Guided atom placement geometry (bond directions, saturation)
├── hydrogen_passivation.rs         # General-purpose H passivation for arbitrary structures
├── unit_cell_struct.rs             # Unit cell geometry & coordinate conversion
├── unit_cell_symmetries.rs         # Crystal system classification (7 systems)
├── atomic_structure/
│   ├── mod.rs                      # AtomicStructure container
│   ├── atom.rs                     # Atom struct (position, element, bonds)
│   ├── bond_reference.rs           # Order-insensitive bond pair ID
│   ├── inline_bond.rs              # 4-byte compact bond (29-bit id + 3-bit order)
│   └── atomic_structure_decorator.rs  # Display/selection metadata
├── io/
│   ├── mol_exporter.rs             # MOL V3000 export
│   ├── xyz_loader.rs               # XYZ import
│   └── xyz_saver.rs                # XYZ export
├── lattice_fill/
│   ├── config.rs                   # LatticeFillConfig, Options, Result, Statistics
│   ├── fill_algorithm.rs           # Recursive lattice filling (SDF sampling)
│   ├── hydrogen_passivation.rs     # H termination of dangling bonds
│   ├── placed_atom_tracker.rs      # CrystallographicAddress → atom ID map
│   └── surface_reconstruction.rs   # Diamond (100) 2×1 dimer reconstruction
└── simulation/
    ├── mod.rs                      # Public API: minimize_energy(), MinimizationResult
    ├── force_field.rs              # ForceField trait (energy_and_gradients)
    ├── topology.rs                 # Interaction list enumeration from bond graph
    ├── minimize.rs                 # L-BFGS optimizer wrapper, frozen atom support
    └── uff/
        ├── mod.rs                  # UffForceField: implements ForceField trait
        ├── params.rs               # Static UFF parameter table (126 atom types)
        ├── typer.rs                # Atom type assignment from connectivity
        └── energy.rs               # Energy terms + analytical gradients
```

## Key Types

| Type | Location | Purpose |
|------|----------|---------|
| `AtomicStructure` | `atomic_structure/mod.rs` | Main container: atoms (Vec with optional slots), spatial grid, bonds |
| `Atom` | `atomic_structure/atom.rs` | id(u32), position(DVec3), atomic_number(i16), bonds(SmallVec<[InlineBond;4]>), flags(u16) |
| `InlineBond` | `atomic_structure/inline_bond.rs` | 4-byte bond: 29-bit atom_id + 3-bit order. Supports 7 bond types |
| `BondReference` | `atomic_structure/bond_reference.rs` | Unordered (atom_id1, atom_id2) pair, hashable |
| `UnitCellStruct` | `unit_cell_struct.rs` | Basis vectors (a,b,c), lattice↔real coordinate conversion |
| `Motif` | `motif.rs` | Sites (fractional coords), bonds (with cell offsets), parameter elements |
| `SiteSpecifier` | `motif.rs` | Site index + IVec3 relative cell offset |
| `DrawingPlane` | `drawing_plane.rs` | Miller-indexed 2D plane with 2D↔3D transforms |
| `LatticeFillConfig` | `lattice_fill/config.rs` | Unit cell + motif + geometry + options for filling |
| `PlacedAtomTracker` | `lattice_fill/placed_atom_tracker.rs` | CrystallographicAddress → atom ID mapping |
| `AtomInfo` | `atomic_constants.rs` | Element properties (symbol, radii, color) |
| `GuidedPlacementResult` | `guided_placement.rs` | Computed guide dot positions for bonded atom placement |
| `Hybridization` | `guided_placement.rs` | Sp3 / Sp2 / Sp1 orbital hybridization |
| `BondMode` | `guided_placement.rs` | Covalent (element-specific max) vs Dative (geometric max) |
| `BondLengthMode` | `guided_placement.rs` | Crystal (lattice-derived table) vs Uff (force field formula) |

## Core Concepts

**Crystal Lattice**: Unit cell basis vectors define a periodic 3D grid. Atoms sit at fractional coordinates within cells. `UnitCellStruct` handles lattice↔real-space conversion via matrix math (Cramer's rule).

**Motif**: A template of atom sites and bonds that repeats at every lattice point. Sites use fractional coordinates; bonds reference sites with relative cell offsets (e.g., `SiteSpecifier { site_index: 0, relative_cell: IVec3(1,0,0) }` means site 0 in the +x neighboring cell). Parameter elements allow substitutional flexibility (e.g., PRIMARY=Carbon).

**Lattice Filling**: `fill_lattice()` recursively subdivides a bounding box, evaluates an SDF geometry at motif sites, places atoms where SDF ≤ 0.01, creates bonds from the motif template, then applies cleanup → surface reconstruction → hydrogen passivation.

**Memory Layout**: `InlineBond` packs atom_id (29 bits) + bond_order (3 bits) into 4 bytes. `SmallVec<[InlineBond; 4]>` keeps up to 4 bonds inline per atom. Spatial grid (FxHashMap, cell size 4.0 Å) enables O(1) neighbor queries.

**Guided Placement** (`guided_placement.rs`): Computes chemically valid candidate positions for bonded atom placement. Given an anchor atom, determines hybridization (sp3/sp2/sp1 via UFF type assignment or manual override), checks saturation, computes bond distance, and returns guide dot positions at correct bond angles. Three placement modes: `FixedDots` (deterministic positions), `FreeSphere` (bare atom, click anywhere), `FreeRing` (single bond without dihedral reference, rotating dots on cone). Includes a crystal bond length table for ~20 semiconductor compounds (diamond cubic / zinc blende lattice parameters) with UFF fallback. Dative bond mode unlocks lone pair / empty orbital positions but does not persist any bond kind distinction — dative is a placement-time consideration only. Design doc: `doc/atom_edit/guided_atom_placement.md`.

## Important Constants (`crystolecule_constants.rs`)

- `DIAMOND_UNIT_CELL_SIZE_ANGSTROM`: 3.567 Å
- `DEFAULT_ZINCBLENDE_MOTIF`: 8-site diamond motif text (CORNER, FACE_X/Y/Z, INTERIOR1-4)
- Bond distance multiplier: 1.15× covalent radii (auto-bonding)
- C-H bond length: 1.09 Å (passivation)

## Error Types

- `ParseError` (motif_parser) — line number + message
- `XyzError` (io/xyz_loader) — Io / Parse / FloatParse variants
- `XyzSaveError` (io/xyz_saver) — Io / ElementNotFound variants
- `MolSaveError` (io/mol_exporter) — Io / ElementNotFound variants

All use `thiserror` derive macros.

## Internal Dependencies

```
lattice_fill  →  AtomicStructure, UnitCellStruct, Motif, GeoNode (from geo_tree)
drawing_plane →  UnitCellStruct
motif_parser  →  Motif, atomic_constants
atomic_structure_utils → AtomicStructure, atomic_constants
io/*          →  AtomicStructure, atomic_constants
guided_placement → AtomicStructure, simulation/uff (typer, params)
hydrogen_passivation → AtomicStructure, atomic_constants, guided_placement
```

`GeoNode` (from `geo_tree`) is the only external module dependency — used as the SDF geometry input to `fill_lattice()`.

**Architectural constraint:** This module is independent of rendering concerns. Never add dependencies on `renderer` or `display` here — the `display` module is the adapter that converts crystolecule types into renderable meshes.

## Testing

Tests live in `rust/tests/crystolecule/` (never inline `#[cfg(test)]`). Test modules are registered in `rust/tests/crystolecule.rs`.

```
tests/crystolecule/
├── guided_placement_test.rs       # Guided placement geometry, saturation, bond distances
├── atomic_structure_test.rs       # CRUD, grid, bonds, selection, transforms
├── drawing_plane_test.rs          # Plane axes, Miller indices, 2D↔3D mappings
├── lattice_fill_test.rs           # Tracker, statistics, integration with sphere geometry
├── unit_cell_test.rs              # Round-trip conversions, multiple cell types
├── unit_cell_symmetries_test.rs   # All 7 crystal systems, symmetry preservation
├── hydrogen_passivation_test.rs   # General-purpose H passivation tests
├── motif_parser_test.rs           # Tokenization, all commands, error cases
├── io/
│   ├── mol_exporter_test.rs       # V3000 format, molecules, bond types
│   └── xyz_roundtrip_test.rs      # Save/load cycles, precision, edge cases
└── simulation/                    # Energy minimization tests (~300+ tests)
    ├── uff_params_test.rs         # Parameter table spot-checks
    ├── uff_energy_test.rs         # Bond stretch energy + gradient
    ├── uff_angle_test.rs          # Angle bend energy + gradient
    ├── uff_torsion_test.rs        # Torsion energy + gradient
    ├── uff_inversion_test.rs      # Inversion energy + gradient
    ├── uff_typer_test.rs          # Atom type assignment
    ├── topology_test.rs           # Interaction enumeration
    ├── uff_force_field_test.rs    # Full force field validation
    ├── uff_vdw_test.rs            # Van der Waals tests
    ├── minimize_test.rs           # L-BFGS + end-to-end minimization
    ├── steepest_descent_test.rs   # Steepest descent (continuous minimization)
    └── test_data/                 # Reference data from RDKit
```

**Running:** `cd rust && cargo test crystolecule`

**Tolerances:** Round-trip conversions use 1e-10; spatial/angle checks use 1e-6; I/O roundtrips use 1e-5. Simulation energy tolerances: 0.01-0.5 kcal/mol depending on molecule size; gradient numerical tests use <1% relative error.

## Modifying This Module

**Adding an element property**: Update `atomic_constants.rs` lazy-static maps (`ATOM_INFO`, `CHEMICAL_ELEMENTS`).

**Adding a new I/O format**: Create `io/format_name.rs`, add `pub mod` in `io/mod.rs`, define an error type with `thiserror`.

**Changing the motif format**: Update `motif_parser.rs` parse functions and `motif.rs` structs. Update `DEFAULT_ZINCBLENDE_MOTIF` if the syntax changes.

**New lattice fill feature**: Add to `lattice_fill/` as a separate file, wire into `fill_algorithm.rs` pipeline. The pipeline order is: place atoms → create bonds → remove lone atoms → surface reconstruction → hydrogen passivation.
