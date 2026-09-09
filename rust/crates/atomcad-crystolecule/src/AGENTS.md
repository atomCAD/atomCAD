# `atomcad-crystolecule` - Agent Instructions

`atomcad-crystolecule` implements atomic structure representation, crystal lattice geometry, lattice-filling algorithms, and energy minimization for atomically precise manufacturing (APM).

It was `rust/src/crystolecule/` until Phase 4 of `doc/design_rust_crate_split.md`
made it its own crate. Consequences for anything you write here:

- It is imported as **`atomcad_crystolecule::…`**, never `crate::crystolecule::…`,
  from every other package *and* from every test outside this crate. Inside the
  crate, use `crate::…`.
- Its dependencies are `atomcad-util` and `atomcad-geo-tree` and nothing else.
  The "never depend on `renderer` or `display`" constraint below is now a **build
  failure**, not a review comment — which is the point of the split.
- Its tests live in `crates/atomcad-crystolecule/tests/crystolecule/`, beside
  `tests/crystolecule.rs`. The repeated directory name is load-bearing (it keeps
  every `#[path]` string and the UFF `CARGO_MANIFEST_DIR` test-data paths valid);
  do not tidy it.
- Shared test fixtures stay at `rust/tests/fixtures/` and are addressed through
  `atomcad_test_support::fixture_path` — never a local `CARGO_MANIFEST_DIR` join.
- No `#[frb(...)]` attribute belongs here. flutter_rust_bridge stays confined to
  the root crate's `src/api/`; a Dart-facing type living here keeps a same-named
  twin in `api/` with `From` impls (`visualization::AtomicStructureVisualization`
  and `atomic_structure::SelectModifier` are the two worked examples).

## Subdirectory Instructions

- Working in `simulation/` or any descendant → Read `simulation/AGENTS.md`
- Working in `simulation/uff/` → Also read `simulation/uff/AGENTS.md`

## Module Structure

```
crates/atomcad-crystolecule/src/
├── lib.rs                          # Crate root: module declarations (all submodules pub)
├── atomic_constants.rs             # Element database (symbol, radius, color)
├── atomic_structure_utils.rs       # Auto-bonding, selection, cleanup helpers
├── crystolecule_constants.rs       # Diamond unit cell size, default motif text
├── drawing_plane.rs                # 2D drawing plane embedded in 3D crystal
├── motif.rs                        # Motif struct (sites, bonds, parameters)
├── motif_parser.rs                 # Text format parser for motifs
├── guided_placement.rs             # Guided atom placement geometry (bond directions, saturation)
├── hydrogen_passivation.rs         # General-purpose H passivation for arbitrary structures
├── miller.rs                       # Miller-index arithmetic: reduction, enumeration, symmetry families {hkl}
├── patch.rs                        # Surface-patch domain model: tile extraction, cell selection, apply_patch
├── weld.rs                         # weld_coincident_atoms(): fuse atoms at the same position (surface patches)
├── structure.rs                    # `Structure` value type (lattice_vecs + motif + motif_offset)
├── unit_cell_struct.rs             # Unit cell geometry & coordinate conversion
├── unit_cell_symmetries.rs         # Crystal system classification (7 systems)
├── visualization.rs                # AtomicStructureVisualization (hit_test pickability)
├── atomic_structure/
│   ├── mod.rs                      # AtomicStructure container
│   ├── atom.rs                     # Atom struct (position, element, bonds)
│   ├── bond_reference.rs           # Order-insensitive bond pair ID
│   ├── inline_bond.rs              # 4-byte compact bond (29-bit id + 3-bit order)
│   └── atomic_structure_decorator.rs  # Display/selection metadata
├── motif_bond_inference.rs          # Bond inference on motif fractional coords (cross-cell)
├── field/
│   ├── mod.rs                      # ScalarField trait, FieldBounds, GridGeometry, SampledField
│   ├── distribution.rs             # ValueDistribution: isovalue <-> enclosed-mass fraction, log histogram
│   └── isosurface.rs               # IsosurfaceData/IsosurfaceColoring/Colormap (the surface *spec*, not a mesh) + auto_level
├── io/
│   ├── cube_loader.rs              # Gaussian .cube import (volumetric scalar data + atom block)
│   ├── mol_exporter.rs             # MOL V3000 export
│   ├── xyz_loader.rs               # XYZ import
│   ├── xyz_saver.rs                # XYZ export
│   └── cif/
│       ├── mod.rs                  # Public API: load_cif() → CifLoadResult
│       ├── parser.rs               # CIF text format parser (data blocks, tags, loops)
│       ├── structure.rs            # Extract crystallographic data from parsed CIF
│       ├── symmetry.rs             # Symmetry operation parsing and expansion
│       └── space_groups.rs         # Lookup table for 230 space groups (symmetry ops)
├── mechanosynth/
│   ├── mod.rs                      # Build-sequence replay: re-exports
│   ├── schema.rs                   # OpLibrary/Operation/Pattern/BuildScript/Step + MechanosynthError
│   ├── parse.rs                    # JSON parse + validation of both files
│   ├── apply.rs                    # apply_step, replay (with the highlight tag)
│   └── compare.rs                  # compare_structures: position-tolerant Vec<Mismatch>
├── lattice_fill/
│   ├── concave_rebond.rs           # Concave-corner clash → host-host bond rewrite
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
| `SelectModifier` | `atomic_structure/mod.rs` | Replace / Toggle / Expand — how a new selection combines with the old one. Serialized (`edit_atom`'s `SelectCommand` persists in `.cnnd`), so variant names are load-bearing. Dart-facing twin in `api/common_api_types.rs` |
| `AtomicStructureVisualization` | `visualization.rs` | BallAndStick / SpaceFilling. Needed here only so `hit_test` can decide bond pickability; `display::preferences` re-exports it and `api/` keeps the Dart-facing twin |
| `Atom` | `atomic_structure/atom.rs` | id(u32), position(DVec3), atomic_number(i16), bonds(SmallVec<[InlineBond;4]>), flags(u16). Flags layout: bit 0 selected, bit 1 hydrogen_passivation, bit 2 frozen, bits 3-4 hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1), bit 5 display-ghost (`is_ghost`/`set_ghost` — transient motif_edit neighbour-cell render state), bit 6 patch-ghost (`is_patch_ghost`/`set_patch_ghost` — durable surface-patch flag; survives serialization, drives weld survivorship; distinct from bit 5) |
| `InlineBond` | `atomic_structure/inline_bond.rs` | 4-byte bond: 29-bit atom_id + 3-bit order. Supports 7 bond types |
| `BondReference` | `atomic_structure/bond_reference.rs` | Unordered (atom_id1, atom_id2) pair, hashable |
| `UnitCellStruct` | `unit_cell_struct.rs` | Basis vectors (a,b,c), lattice↔real coordinate conversion |
| `Structure` | `structure.rs` | Bundles `lattice_vecs: UnitCellStruct` + `motif: Motif` + `motif_offset: DVec3`. Factories: `Structure::diamond()` (cubic diamond + zincblende motif + zero offset), `Structure::from_lattice_vecs(...)` (default motif + zero offset). Carried by `BlueprintData` and `CrystalData` as a first-class value |
| `Motif` | `motif.rs` | Sites (fractional coords), bonds (with cell offsets), parameter elements |
| `SiteSpecifier` | `motif.rs` | Site index + IVec3 relative cell offset |
| `DrawingPlane` | `drawing_plane.rs` | Miller-indexed 2D plane with 2D↔3D transforms. Built via `from_spec(miller, u, v, …)` (the case matrix in `doc/design_drawing_plane_explicit_axes.md`): auto-pick both in-plane axes from the Miller index, or pin one/both in-plane lattice directions `[u v w]` explicitly, or derive the Miller index from `u × v`. `DrawingPlane::new` is a thin `from_spec` wrapper. `is_compatible` compares the resolved `u_axis`/`v_axis`, not just the Miller index |
| `CompatibilityReport` | `patch.rs` | Welded / orphaned / over-coordination stats from `apply_patch`. Dart-facing twin `APICompatibilityReport` in `api/` |
| `SelectedCell` | `patch.rs` | One tiling site chosen by `select_patch_cells`: an in-plane lattice `offset` plus the `k` indices it spans along the free (non-periodic) direction |
| `LatticeFillConfig` | `lattice_fill/config.rs` | Unit cell + motif + geometry + options for filling |
| `PlacedAtomTracker` | `lattice_fill/placed_atom_tracker.rs` | CrystallographicAddress → atom ID mapping |
| `AtomInfo` | `atomic_constants.rs` | Element properties (symbol, radii, color) |
| `GuidedPlacementResult` | `guided_placement.rs` | Computed guide dot positions for bonded atom placement |
| `Hybridization` | `guided_placement.rs` | Sp3 / Sp2 / Sp1 orbital hybridization |
| `BondMode` | `guided_placement.rs` | Covalent (element-specific max) vs Dative (geometric max) |
| `BondLengthMode` | `guided_placement.rs` | Crystal (lattice-derived table) vs Uff (force field formula) |
| `CifDocument` | `io/cif/parser.rs` | Parsed CIF file: list of data blocks |
| `CifDataBlock` | `io/cif/parser.rs` | Single data block with tags and loops |
| `CifLoop` | `io/cif/parser.rs` | Loop section: column headers + rows |
| `SymmetryOperation` | `io/cif/symmetry.rs` | Parsed affine transform (3×4 matrix) from Jones notation |
| `CifAtomSite` | `io/cif/symmetry.rs` | Atom label, element, fractional coords, occupancy |
| `CifCrystalData` | `io/cif/structure.rs` | Extracted unit cell, atoms, symmetry ops, bonds from CIF block |
| `CifBond` | `io/cif/structure.rs` | Explicit bond from `_geom_bond_*` with symmetry codes |
| `CifLoadResult` | `io/cif/mod.rs` | Unit cell + expanded atom sites (fractional coords) |
| `ExpandedAtomSite` | `io/cif/mod.rs` | Label, atomic number, fractional position |
| `ScalarField` | `field/mod.rs` | Trait: a scalar function of 3D space. `sample` / `sample_batch` / `gradient`, `data_bounds` / `suggested_bounds`, `native_grid`, `value_range`, `description`. `Send + Sync` |
| `SampledField` | `field/mod.rs` | `ScalarField` stored as `f32` samples on a regular grid, trilinearly interpolated |
| `GridGeometry` | `field/mod.rs` | Origin + three axis vectors + counts. **Node-centered**: the origin IS sample (0,0,0) |
| `FieldBounds` | `field/mod.rs` | Axis-aligned box, Ångström. The workspace has no general AABB type to reuse |
| `IsosurfaceData` | `field/isosurface.rs` | A surface to extract at display time: field + level + coloring + alpha, plus the `LevelBasis` the level came from. Carries no mesh |
| `LevelBasis` / `AutoBasis` | `field/isosurface.rs` | How a resolved level was arrived at — absolute, an enclosed fraction, or one of the four `Auto` outcomes. Readout only, never persisted |
| `CubeFile` | `io/cube_loader.rs` | Parsed `.cube`: `atoms`, `fields`, and an advisory `units_warning` |
| `OpLibrary` / `Operation` / `Pattern` | `mechanosynth/schema.rs` | A named before/after rewrite in a local frame; comparing the two patterns *by pattern id* is the rewrite, there is no diff syntax |
| `BuildScript` / `Step` | `mechanosynth/schema.rs` | An ordered list of (operation name, translation, rotation); `p_workpiece = r · p_local + t` |
| `Mismatch` | `mechanosynth/compare.rs` | One difference found by `compare_structures` — unmatched atom, element, bond presence or bond order |

## Core Concepts

**Crystal Lattice**: Unit cell basis vectors define a periodic 3D grid. Atoms sit at fractional coordinates within cells. `UnitCellStruct` handles lattice↔real-space conversion via matrix math (Cramer's rule).

**Motif**: A template of atom sites and bonds that repeats at every lattice point. Sites use fractional coordinates; bonds reference sites with relative cell offsets (e.g., `SiteSpecifier { site_index: 0, relative_cell: IVec3(1,0,0) }` means site 0 in the +x neighboring cell). Parameter elements allow substitutional flexibility (e.g., PRIMARY=Carbon).

**Lattice Filling**: `fill_lattice()` recursively subdivides a bounding box, evaluates an SDF geometry at motif sites, places atoms where SDF ≤ 0.01, creates bonds from the motif template, then applies cleanup → surface reconstruction → hydrogen passivation → concave rebonding.

The last step must stay last. `hydrogen_passivate` decides whether a bond is dangling from the **motif**, not from an atom's actual bonds, so any pass that adds a non-lattice bond before it runs will not stop it placing a terminator on that same direction as well — leaving the host over-coordinated. `concave_rebond` therefore runs *after* passivation and repairs the result rather than pre-empting it. `reconstruct_surface` hands it the set of {100} surface atoms it classified but could not pair, and an empty set makes the pass a no-op — which is what gates it on `surf_recon` without a second flag. See `doc/design_concave_rebonding.md`.

**Memory Layout**: `InlineBond` packs atom_id (29 bits) + bond_order (3 bits) into 4 bytes. `SmallVec<[InlineBond; 4]>` keeps up to 4 bonds inline per atom. Spatial grid (FxHashMap, cell size 4.0 Å) enables O(1) neighbor queries. `AtomicStructure` no longer carries a `frame_transform` — movement nodes bake transforms directly into atom positions (see `doc/design_lattice_space_refactoring.md` Appendix B).

**Guided Placement** (`guided_placement.rs`): Computes chemically valid candidate positions for bonded atom placement. Given an anchor atom, determines hybridization (sp3/sp2/sp1 via UFF type assignment or manual override), checks saturation, computes bond distance, and returns guide dot positions at correct bond angles. Three placement modes: `FixedDots` (deterministic positions), `FreeSphere` (bare atom, click anywhere), `FreeRing` (single bond without dihedral reference, rotating dots on cone). Includes a crystal bond length table for ~20 semiconductor compounds (diamond cubic / zinc blende lattice parameters) with UFF fallback. Dative bond mode unlocks lone pair / empty orbital positions but does not persist any bond kind distinction — dative is a placement-time consideration only. Design doc: `doc/atom_edit/guided_atom_placement.md`.

**Miller Indices** (`miller.rs`): Integer `(h,k,l)` triples naming a lattice plane. Three operations, all pure number theory over `IVec3` with no lattice, no structure and no rendering: `simplify_miller_index` reduces a triple by its GCD, `generate_possible_miller_indices` enumerates every reduced index within a bound, and `symmetry_equivalent_indices` expands one index into its symmetry family `{hkl}` — every permutation of the absolute components with every sign combination, skipping the sign flip on a zero component, so `(1,1,1)` yields 8 members and `(1,2,3)` the full 48-member orbit. The enumeration order is deterministic and callers index into it, so it is pinned by `miller_test.rs`.

**Surface Patches** (`patch.rs`): The node-free core of the surface-patch feature — testable on plain `AtomicStructure`s without the node network. `validate_tiling_vectors` and `extract_patch_tile` are the authoring half; `select_patch_cells` and `region_center_depths` choose which lattice cells receive a tile; `apply_patch` runs the cut → place → weld → drop → passivate pipeline and reports welded / orphaned / over-coordinated counts as a `CompatibilityReport`. Patch ghosts are bit 6 of `Atom.flags` and drive weld survivorship. Design docs: `doc/design_surface_patches.md`, `doc/design_patch_cell_selection.md`.

**Scalar Fields** (`field/`): Volumetric data about a molecule — orbital
amplitudes, electron density, electrostatic potential — behind one trait,
`ScalarField`. Two invariants govern everything here:

- **Every coordinate crossing `ScalarField` is real-space Ångström**, matching
  `AtomicStructure`. Each loader converts from its file's units exactly once, at
  load time; no consumer ever sees Bohr. Field *values* are the exception and are
  passed through unconverted in their native atomic units — converting them would
  invalidate every published threshold convention in the chemistry literature.
- **No consumer may be written against a grid.** `native_grid()` is a fidelity
  fast path that returns `None` for an analytic source (the future Molden path),
  so anything that only works when it is `Some` is broken. Write against
  `sample` / `sample_batch` and take an explicit box and resolution.

`sample` outside `data_bounds` returns exactly `0.0` — never an error. A finite
box is a *window* onto a field that decays to zero, and the rule keeps every
consumer free of an error path in its innermost loop. The boundary itself is
**not** a knife edge: a `.cube` writer emits its step vector in Bohr with six
decimals, so the outermost sample plane of a real file lands ~1e-7 Å off where
the nominal spacing puts it, and a strict comparison would answer a sample at
its own nominal position with `0.0` — a jump of the whole data range. Hence
`BOUNDARY_INDEX_TOLERANCE`; keep it when touching `axis_cell`.

The `.cube` loader always reads coordinates as Bohr and uses the atom block only
as a **plausibility check**: implausible interatomic distances set an advisory
`units_warning` and never rescale the parse (`io/cube_loader.rs` documents why).
Design doc: `doc/design_scalar_fields.md`.

**`description()` is the field's only semantic tag, and it is producer text —
never derive one.** Nothing about a field's *values* says whether they are an
orbital amplitude, a density or a potential, yet those are drawn at isolevels an
order of magnitude apart, so a reader with only numbers cannot pick one. The
`.cube` comment lines are the sole carrier of that knowledge (the loader used to
read past them). Normalization — trim, drop blank lines, join, cap at
`MAX_DESCRIPTION_CHARS` — belongs in `SampledField::with_description`, so every
producer arrives in the same shape and no consumer has to distinguish "absent"
from "present but blank". The method defaults to `None`: an analytic field is
not obliged to invent a label, and inventing one from the numbers would be a
guess presented as a fact.

The **source file name is prepended** to that text, by `cube_loader::load_cube`
and not by `load_cube_from_str` — only the former has a path. It is the same
channel rather than a field of its own because it answers the same question, and
a second accessor would mean every consumer learning about it. It leads, so a
truncated description keeps it, and only the **final path component** goes in: a
readout is a narrow tooltip and a directory prefix would push the comments out
of it. This matters more than it sounds: a `.cube`'s comment lines are routinely
a program banner or blank, while the name a producer chose
(`si-cluster-S3-vacancy_spin.cube`) is often the only place the quantity is
named at all.

**Isosurfaces** (`field/isosurface.rs`): `IsosurfaceData` is the *specification*
of a surface — which field, which level, which paint — and never a mesh. It lives
here, beside the `ScalarField` it wraps, because **every `NetworkResult` payload
comes from the domain layer or from `structure_designer` itself**; display types
enter one stage later, in `NodeOutput`. The extracted mesh (`SurfaceMesh`) and the
marching-cubes extractor accordingly live in `atomcad-display`, which depends on
this crate and so consumes `&IsosurfaceData` directly — no twin, no conversion.
`Colormap` is node data that round-trips through the `.cnnd`, which is the second
reason it is here: `atomcad-display` has no `serde` dependency and should not
acquire one to host a persisted enum.

Two rules the type encodes, both easy to undo by accident:

- **Semantic parameters live in the value; quality parameters live in
  preferences.** Isolevel is here; extraction *resolution* is not — it comes from
  `GeometryVisualizationPreferences` at the display conversion, exactly as it does
  for `Blueprint`. Adding a resolution field here would bake a quality setting
  into the project file.
- **Narrowing is by consumer.** Values the *renderer* consumes are `f32`/`Vec3`
  (`alpha`, the two phase colors); values compared against *field samples* stay
  `f64` (`level`, the colormap `range`), because `ScalarField::sample` returns
  `f64` and narrowing a threshold would put a rounding difference between the
  comparison and the data it compares to.

**Choosing the level** (`auto_level`, same file): given a field and nothing else,
pick an isovalue. Signedness — read from `value_range` with a *tolerance*, never
`min >= 0` — chooses the coordinate: a non-negative field takes the absolute
`DENSITY_LEVEL` convention if the plausibility window accepts it, everything else
takes `iso_for_fraction(LOCALIZED_FRACTION)`. Every constant is calibrated
against a 16-file cube zoo and the evidence lives in the design document, not
here; the two committed branch fixtures only pin which branch is taken.

Three invariants this half depends on:

- **The distribution is over stored samples, never the extraction lattice**, so a
  resolved level does not move when the quality preference changes. Anything that
  makes the level depend on how finely the surface is meshed has reintroduced
  exactly the quality-parameter-in-the-value problem the split above avoids.
- **Both `ValueDistribution` queries return `Option` and neither may be
  unwrapped.** An all-zero field is the combination easy to miss:
  `value_distribution()` is `Some` while `iso_for_fraction` is `None`.
- **`LevelBasis` is a readout passenger.** It rides in `IsosurfaceData` because
  the node resolves the level in `eval` and the pin readout renders the value, so
  that is the one channel connecting them. Nothing downstream may branch on it,
  and it is never persisted.

Design docs: `doc/design_isosurface_node.md`, `doc/design_isosurface_level.md`.

**Mechanosynthesis replay** (`mechanosynth/`): replays an ordered build script
— positionally controlled reactions — onto a workpiece, so that "the structure
after the first `k` steps" is a value the caller can ask for. An **operation** is
a before/after pair of small atom lists in a local frame; a **step** names one
and gives a rigid transform into workpiece coordinates. Three properties are
load-bearing:

- **Coordinates, not graphs.** Matching is nearest-atom-within-tolerance on
  position and element, through the spatial grid, and *ignores bonds entirely* —
  a `before` pattern's bonds exist only to express deletions and order changes.
  No subgraph isomorphism, no chemical perception. Adding a bond check would only
  add a way for a correct script to fail.
- **Ideal geometry.** Added atoms land exactly where the operation says, and
  nothing here relaxes anything. That is what keeps tolerances tight (0.3 Å, far
  below half a bond length) and every intermediate state deterministic. Wire
  `relax` downstream if a settled geometry is wanted.
- **A kept atom is never snapped.** Position and element are compared between the
  two *patterns*, not against the workpiece: an id in both patterns at the same
  position stays exactly where the workpiece has it, while a *moved* one lands at
  `r · after.pos + t`. Snapping would quietly rewrite a reconstructed surface
  every time an operation touched it.

Deliberately independent of `atomic_structure_diff` / `apply_diff`, which solve
the more general problem of anchoring arbitrary diffs across bases; nothing is
shared beyond `AtomicStructure`. `compare_structures` is the position-tolerant
comparison the generators verify with — it returns a *list* of differences rather
than a bool, because a bare `false` leaves the caller nothing to print, and it is
not `atomcad_test_support::assert_structures_equivalent` (test-only, panics,
O(n²), compares flags and tag names). Design doc:
`design_mechanosynth_node.md`, in the external mechanosynth working folder.

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
- `CifParseError` (io/cif/parser) — syntax errors in CIF text
- `CifError` (io/cif/symmetry, structure) — symmetry operation or crystal data extraction errors
- `CifLoadError` (io/cif/mod) — top-level load errors (wraps parse/extraction/IO)
- `CubeError` (io/cube_loader) — Io / Parse / Unsupported / Field variants
- `FieldError` (field) — grid description problems (zero dimension, sample-count
  mismatch, degenerate axes, non-finite sample)
- `MechanosynthError` (mechanosynth/schema) — Io / Json / Invalid (a validation
  failure naming the file, the operation or step, and the field) / NoMatch (a
  `before` atom that found nothing within tolerance)

All use `thiserror` derive macros.

## Internal Dependencies

```
lattice_fill  →  AtomicStructure, UnitCellStruct, Motif, GeoNode (from geo_tree)
drawing_plane →  UnitCellStruct
motif_parser  →  Motif, atomic_constants
atomic_structure_utils → AtomicStructure, atomic_constants
io/cif/*      →  UnitCellStruct, Motif, AtomicStructure, atomic_constants
io/*          →  AtomicStructure, atomic_constants
io/cube_loader → AtomicStructure, atomic_constants, field
field         →  glam only (no crystolecule types at all)
motif_bond_inference → Motif, UnitCellStruct, atomic_constants
miller        →  glam only (no crystolecule types at all)
patch         →  AtomicStructure, UnitCellStruct, weld, hydrogen_passivation, guided_placement, GeoNode
mechanosynth  →  AtomicStructure, atomic_constants (serde_json for the two JSON files)
guided_placement → AtomicStructure, simulation/uff (typer, params)
hydrogen_passivation → AtomicStructure, atomic_constants, guided_placement
```

`GeoNode` (from `geo_tree`) is the only external module dependency — used as the SDF geometry input to `fill_lattice()`.

**Architectural constraint:** This crate is independent of rendering concerns. Never add dependencies on `atomcad-renderer` or `display` here — `display` is the adapter that converts crystolecule types into renderable meshes. Since Phase 4 the compiler enforces this: those crates are simply not in `Cargo.toml`, and adding them would also create a cycle.

The single deliberate exception is `visualization::AtomicStructureVisualization`, and it is not really one: `AtomicStructure::hit_test` has to know whether bonds are pickable, and space-filling atoms have no visible bonds. The enum carries no rendering code, and it used to be *worse* — `atomic_structure/mod.rs` reached up into `crate::api::…` for it, one of the four back-edges this refactor exists to delete.

## Testing

Tests live in `crates/atomcad-crystolecule/tests/crystolecule/` (never inline `#[cfg(test)]`). Test modules are registered in `crates/atomcad-crystolecule/tests/crystolecule.rs`.

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
├── motif_bond_inference_test.rs   # Bond inference on fractional coords, cross-cell bonds
├── miller_test.rs                 # Index reduction, enumeration, {hkl} symmetry families
├── mechanosynth_test.rs           # Parse/validate, id-rule matrix, matching, replay, highlight, compare
├── field_test.rs                  # ScalarField contract: bounds, interpolation, gradients
├── patch_test.rs                  # Cell selection, region depths, apply_patch pipeline
├── patch_build_test.rs            # Tiling-vector validation, tile extraction
├── concave_rebond_test.rs         # Concave-corner rebonding; clash detector re-derived independently
├── io/
│   ├── mol_exporter_test.rs       # V3000 format, molecules, bond types
│   ├── xyz_roundtrip_test.rs      # Save/load cycles, precision, edge cases
│   ├── cube_loader_test.rs        # .cube parsing: axis order, units check, malformed input
│   ├── cif_parser_test.rs         # CIF syntax: tags, loops, quotes, uncertainties
│   ├── cif_symmetry_test.rs       # Symmetry operation parsing, expansion, dedup
│   ├── cif_structure_test.rs      # Crystal data extraction, old/new tags, bonds
│   ├── cif_load_test.rs           # End-to-end load_cif() with fixture files
│   └── cif_space_groups_test.rs   # Space group lookup table tests
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

**Running:** `cd rust && cargo test -p atomcad-crystolecule -j 4` (builds without
`wgpu` and without `frb_generated.rs`), or `cargo test crystolecule -j 4` to filter
by name across the workspace.

**Tolerances:** Round-trip conversions use 1e-10; spatial/angle checks use 1e-6; I/O roundtrips use 1e-5. Simulation energy tolerances: 0.01-0.5 kcal/mol depending on molecule size; gradient numerical tests use <1% relative error.

## Modifying This Module

**Adding an element property**: Update `atomic_constants.rs` lazy-static maps (`ATOM_INFO`, `CHEMICAL_ELEMENTS`).

**Adding a new I/O format**: Create `io/format_name.rs`, add `pub mod` in `io/mod.rs`, define an error type with `thiserror`. For complex formats, use a subdirectory (see `io/cif/` as an example).

**A new scalar-field source** (Molden, a different volumetric format): implement
`ScalarField` beside `SampledField` and add a loader under `io/`. Do **not** add
methods that only a grid can answer — the two `Option`-returning methods
(`data_bounds`, `native_grid`) exist precisely so an analytic source can say
"unbounded" and "no preferred lattice" without lying.

**CIF-related changes**: The CIF parser (`io/cif/parser.rs`) is a generic STAR/CIF parser. Symmetry operations are in `io/cif/symmetry.rs`, crystal data extraction in `io/cif/structure.rs`, and the 230 space group lookup table in `io/cif/space_groups.rs`. Test fixtures are in `rust/tests/fixtures/cif/` (diamond, nacl, hexagonal, multi_block, with_bonds) — outside this crate, reached via `atomcad_test_support::fixture_path("cif/…")`, because the same fixture tree is read from three packages.

**Changing the motif format**: Update `motif_parser.rs` parse functions and `motif.rs` structs. Update `DEFAULT_ZINCBLENDE_MOTIF` if the syntax changes.

**A new Miller-index helper**: It goes in `miller.rs`, not in the node or tessellator that wants it. The test is whether the function needs anything beyond an `IVec3` — if it reasons about `(h,k,l)` as integers, it is crystallography and belongs here, even when its only caller today is a gadget. `simplify_miller_index` and `generate_possible_miller_indices` were filed in `structure-designer`'s `half_space_utils` until `doc/design_push_domain_code_down.md` (§2.2, D5) split them out — they had no rendering dependency at all, and the file they sat in only happened to be their first caller.

**New lattice fill feature**: Add to `lattice_fill/` as a separate file, wire into `fill_algorithm.rs` pipeline. The pipeline order is: place atoms → create bonds → remove lone atoms → remove single-bond atoms → surface reconstruction → hydrogen passivation → concave rebonding.

Two things to know before inserting a step. (1) Anything that reasons about *dangling* bonds must account for `hydrogen_passivate` deriving them from the motif rather than from actual bonds — see the note under **Lattice Filling**. (2) A geometric rule that fires on interatomic distance needs more than a distance threshold if it must hold for every passivant: `passiv_elem` ranges over H/F/Cl/Br/I, and a bulky halogen reaches ~0.6 Å further from its host while any vdW-scaled threshold grows with it, so distance alone stops discriminating. `concave_rebond` conjoins the distance test with two element-independent geometric ones for exactly this reason.
