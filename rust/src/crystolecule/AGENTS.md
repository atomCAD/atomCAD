# Crystolecule Module - Agent Instructions

The crystolecule module implements atomic structure representation, crystal lattice geometry, and lattice-filling algorithms for atomically precise manufacturing (APM).

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
├── simulation.rs                   # Python energy minimization interface (stub)
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
└── lattice_fill/
    ├── config.rs                   # LatticeFillConfig, Options, Result, Statistics
    ├── fill_algorithm.rs           # Recursive lattice filling (SDF sampling)
    ├── hydrogen_passivation.rs     # H termination of dangling bonds
    ├── placed_atom_tracker.rs      # CrystallographicAddress → atom ID map
    └── surface_reconstruction.rs   # Diamond (100) 2×1 dimer reconstruction
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

## Core Concepts

**Crystal Lattice**: Unit cell basis vectors define a periodic 3D grid. Atoms sit at fractional coordinates within cells. `UnitCellStruct` handles lattice↔real-space conversion via matrix math (Cramer's rule).

**Motif**: A template of atom sites and bonds that repeats at every lattice point. Sites use fractional coordinates; bonds reference sites with relative cell offsets (e.g., `SiteSpecifier { site_index: 0, relative_cell: IVec3(1,0,0) }` means site 0 in the +x neighboring cell). Parameter elements allow substitutional flexibility (e.g., PRIMARY=Carbon).

**Lattice Filling**: `fill_lattice()` recursively subdivides a bounding box, evaluates an SDF geometry at motif sites, places atoms where SDF ≤ 0.01, creates bonds from the motif template, then applies cleanup → surface reconstruction → hydrogen passivation.

**Memory Layout**: `InlineBond` packs atom_id (29 bits) + bond_order (3 bits) into 4 bytes. `SmallVec<[InlineBond; 4]>` keeps up to 4 bonds inline per atom. Spatial grid (FxHashMap, cell size 4.0 Å) enables O(1) neighbor queries.

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
```

`GeoNode` (from `geo_tree`) is the only external module dependency — used as the SDF geometry input to `fill_lattice()`.

**Architectural constraint:** This module is independent of rendering concerns. Never add dependencies on `renderer` or `display` here — the `display` module is the adapter that converts crystolecule types into renderable meshes.

## Testing

Tests live in `rust/tests/crystolecule/` (never inline `#[cfg(test)]`). Test modules are registered in `rust/tests/crystolecule.rs`.

```
tests/crystolecule/
├── atomic_structure_test.rs       # CRUD, grid, bonds, selection, transforms
├── drawing_plane_test.rs          # Plane axes, Miller indices, 2D↔3D mappings
├── lattice_fill_test.rs           # Tracker, statistics, integration with sphere geometry
├── unit_cell_test.rs              # Round-trip conversions, multiple cell types
├── unit_cell_symmetries_test.rs   # All 7 crystal systems, symmetry preservation
├── motif_parser_test.rs           # Tokenization, all commands, error cases
└── io/
    ├── mol_exporter_test.rs       # V3000 format, molecules, bond types
    └── xyz_roundtrip_test.rs      # Save/load cycles, precision, edge cases
```

**Running:** `cd rust && cargo test crystolecule`

**Tolerances:** Round-trip conversions use 1e-10; spatial/angle checks use 1e-6; I/O roundtrips use 1e-5.

## Modifying This Module

**Adding an element property**: Update `atomic_constants.rs` lazy-static maps (`ATOM_INFO`, `CHEMICAL_ELEMENTS`).

**Adding a new I/O format**: Create `io/format_name.rs`, add `pub mod` in `io/mod.rs`, define an error type with `thiserror`.

**Changing the motif format**: Update `motif_parser.rs` parse functions and `motif.rs` structs. Update `DEFAULT_ZINCBLENDE_MOTIF` if the syntax changes.

**New lattice fill feature**: Add to `lattice_fill/` as a separate file, wire into `fill_algorithm.rs` pipeline. The pipeline order is: place atoms → create bonds → remove lone atoms → surface reconstruction → hydrogen passivation.
