# Design: scalar fields — `.cube` import and field sampling

## Motivation

Quantum chemistry packages compute *volumetric scalar data* about a molecule:
molecular orbital amplitudes, electron density, electrostatic potential. The
standard way to look at such data is an **isosurface** — the surface where the
value equals some threshold — drawn around the ball-and-stick molecule. Nothing
in atomCAD can represent or display this today.

The immediate driver is displaying molecular orbitals produced by
[PySCF](https://pyscf.org/). The longer-term value is broader: a density
isosurface is a physically grounded *shape* for a structure — the honest
alternative to a union of tabulated van der Waals spheres — which is directly
relevant to steric clearance questions in mechanosynthesis.

## Scope of this document

This document designs the **whole feature at a high level** and the **data
ingestion half in detail**. The visualization node is deliberately deferred.

**Designed in detail here:**

- the `ScalarField` abstraction — the interface every consumer sees
- `.cube` file parsing, including the multi-field variant (specified here,
  implemented in P5 — see §P5)
- `DataType` / `NetworkResult` plumbing for a new value kind
- the `import_cube` node
- the `sample_field` node — a probe that makes ingestion verifiable in the app
  before any rendering exists
- a phased implementation plan, each phase carrying its own tests and, where
  there is UI, its own manual walkthrough

**Deliberately deferred** (see [Deferred: the visualization
node](#deferred-the-visualization-node)):

- isosurface extraction, its output type, and its node specification
- transparency, coloring, and the renderer changes they need

The deferral is not laziness. The visualization design turns on questions that
**only real sample data can answer** — how coarse a typical grid looks at
working zoom, whether gradient normals suffice, whether the mesh needs
decimation, and what type an extracted surface should even have. Designing
those now would be designing on speculation. The deferred section lists each
open question together with the evidence that will settle it.

What *is* settled here is the **boundary**: the `ScalarField` interface, and a
stated direction for rendering precise enough to keep that interface from
foreclosing either option. A contract gets designed before either side of it.

## Background: what this data actually is

Written for a contributor with no quantum chemistry background; skip if you
have one.

A quantum chemistry calculation produces **molecular orbitals**. An orbital is
a function of position, `psi(x, y, z)`, returning a real number called an
amplitude. The amplitude is **signed** — the sign carries no meaning in
isolation (`psi` and `-psi` describe the same physical state) but the *pattern*
of signs is what chemists read, so it is always displayed. The conventional
picture draws two surfaces, at `+level` and `-level`, in two different colors.
This is why "orbital" pictures have two-colored lobes.

**Electron density** is a different quantity: the sum of `|psi_i|^2` over all
occupied orbitals, weighted by occupancy. It is therefore never negative, and
it is *not* the square of any single orbital. It is also dominated by core
electrons, so a density isosurface looks essentially like a smooth molecular
envelope.

**Electrostatic potential** is signed, and diverges at every nucleus. It is
essentially never drawn as its own isosurface; it is sampled *onto* a surface
formed by another field, producing the familiar red/blue-mapped molecular
surface.

Two consequences drive the design:

1. **A field's meaning is not recoverable from its numbers** — but the two
   properties that drive rendering are: whether it is **signed**, and its
   **magnitude scale**. Both follow from the data's value range, so no semantic
   tag is needed (§Why there is no semantic tag or metadata).
2. **All these quantities are dominated by sharp peaks at the nuclei**, with the
   interesting structure in the low-magnitude tail — density spans six orders of
   magnitude between the conventional surface threshold (0.002) and its value at
   a carbon nucleus (order 100). **Any auto-ranging over min/max produces a
   useless picture of a few bright dots.**

Reference magnitudes, for choosing defaults later:

| Quantity | Sign | Units | Conventional threshold | Extremes present in the data |
|---|---|---|---|---|
| MO amplitude | signed | bohr^(-3/2) | +/- 0.02 to 0.05 | several a.u. for core orbitals |
| Electron density | non-negative | e/bohr^3 | 0.002 (molecular surface) | 10^2 to 10^3 at heavier nuclei |
| Electrostatic potential | signed | hartree/e | n/a — used for coloring, range +/- 0.05 | diverges at nuclei |

## Architecture overview

```
   .cube file                         .molden file  (future)
        |                                   |
        v                                   v
  cube_loader.rs                     molden_loader.rs
  (parse grid + atoms)               (parse basis + MO coefficients)
        |                                   |
        v                                   v
  SampledField                       AnalyticField
  (stores samples,                   (evaluates Gaussians,
   trilinear interpolation)           unlimited resolution)
        |                                   |
        +--------------+--------------------+
                       |
                       v
             dyn ScalarField          <-- THE CONTRACT
             sample / sample_batch / gradient
             data_bounds / suggested_bounds
             native_grid / value_range
                       |
        +--------------+--------------------+
        |                                   |
        v                                   v
   sample_field node                  isosurface node
   (ScalarField, Vec3) -> Float       (DEFERRED)
   verification + scripting                 |
                                            v
                                     rendered surface
```

The single load-bearing decision: **everything downstream of the contract is
written once**, against `dyn ScalarField`, and never against a grid. A consumer
that takes `&[f32]` plus dimensions has hardcoded the `.cube` path and turns
Molden support into a rewrite of that consumer. A consumer that takes
`&dyn ScalarField`, a box, and a resolution works unchanged for both.

### Crate placement

| Piece | Location | Rationale |
|---|---|---|
| `ScalarField`, `FieldBounds`, `GridGeometry`, `SampledField` | `atomcad-crystolecule/src/field/` | The cube loader must produce an `AtomicStructure` from the file's atom block, so ingestion cannot live below `crystolecule`. Volumetric data about a molecule belongs beside the molecule. |
| `cube_loader.rs` | `atomcad-crystolecule/src/io/cube_loader.rs` | Beside `xyz_loader.rs` / `cif/`, following the established `io/` convention. |
| `import_cube`, `sample_field` nodes | `atomcad-structure-designer/src/nodes/` | Standard node location. |

`atomcad-crystolecule` depends only on `atomcad-util` and `atomcad-geo-tree`,
both of which suffice (`glam` for math). No new workspace dependency.

The DAG stays intact for the deferred rendering work too: `atomcad-renderer`
must never learn about `ScalarField`. When a GPU path is built,
`atomcad-display` converts a `ScalarField` into a plain dense grid and hands
*that* to the renderer — the same adapter role it already plays for atoms.

`field/` is a new subsystem in `crystolecule`, so
`crates/atomcad-crystolecule/src/AGENTS.md` gets a module-map entry in the same
change.

## The `ScalarField` contract

This is the part of the design that must be right, because it is what makes the
deferred half and the future Molden support additive rather than invasive.

### Coordinates and units

**Every coordinate crossing this interface is real-space Ångström**, matching
`AtomicStructure`. Each loader converts from its file's units exactly once, at
load time. No consumer ever sees Bohr. Field *values* are passed through
unconverted in their native atomic units — converting them would invalidate
every published threshold convention in the chemistry literature.

### The trait

```rust
/// A scalar function of 3D real space — sampled from a grid, or evaluated
/// analytically. Coordinates are real-space Ångström; values are passed
/// through in whatever atomic unit the source quantity uses (§Background has
/// the conventional ranges).
///
/// `Send + Sync` is required so that the deferred sampling consumers can
/// evaluate in parallel batches, mirroring
/// `atomcad_geo_tree::BatchedImplicitEvaluator`. Nothing in P1-P4 evaluates in
/// parallel — the node evaluator is single-threaded — but the bound is free to
/// hold now and expensive to add later (§Rendering direction).
pub trait ScalarField: Send + Sync + std::fmt::Debug {
    /// Value at `point`. Outside `data_bounds` this returns exactly `0.0`.
    /// Never errors, never returns NaN.
    fn sample(&self, point: DVec3) -> f64;

    /// Batched evaluation. Precondition: `out.len() == points.len()`.
    /// The default loops over `sample`; implementations with per-batch setup
    /// cost (Gaussian evaluation) override this.
    fn sample_batch(&self, points: &[DVec3], out: &mut [f64]) {
        for (p, o) in points.iter().zip(out.iter_mut()) {
            *o = self.sample(*p);
        }
    }

    /// Gradient at `point`, in value-units per Ångström. Used for isosurface
    /// normals.
    ///
    /// The default is a central difference stepped at half the native grid
    /// spacing when `native_grid()` is `Some`, and at `DEFAULT_GRADIENT_STEP`
    /// otherwise. Both concrete implementations are expected to override it —
    /// see §Gradient below — so the default is a fallback, not the norm.
    ///
    /// `DEFAULT_GRADIENT_STEP` is a `pub const f64` beside the trait in
    /// `field/mod.rs`. Use **0.05 Å**: comfortably below any spacing a cube
    /// writer produces (PySCF's default 80^3 box lands near 0.15-0.20 Å) and
    /// far above `f64` cancellation noise.
    fn gradient(&self, point: DVec3) -> DVec3 { /* central difference */ }

    /// Region outside which `sample` is defined to return `0.0`.
    /// `None` = defined everywhere (analytic sources).
    fn data_bounds(&self) -> Option<FieldBounds>;

    /// Box a consumer should sample when it has no better instruction.
    ///
    /// For a sampled source this is the box **through the outermost sample
    /// points** — node-centered, NOT extended by half a voxel (see §Bounds
    /// convention). For an analytic source it is derived from atom positions
    /// plus a margin.
    fn suggested_bounds(&self) -> FieldBounds;

    /// The field's intrinsic sample lattice, when it has one. `Some` for a
    /// sampled source; `None` for an analytic source, which has no preferred
    /// lattice at all.
    ///
    /// A consumer that wants zero information loss should use this grid
    /// verbatim when it is `Some`: sampling a stored field *anywhere else*
    /// blends eight stored values per point, which smooths the field for no
    /// gain — the stored lattice is already the resolution ceiling, so
    /// sampling finer buys interpolation, not information.
    ///
    /// This is a **fidelity fast path, not the interface.** Every consumer
    /// must still work correctly from `sample` alone when this returns `None`;
    /// a consumer that only functions when it is `Some` is broken and will
    /// fail against the first Molden field it meets.
    fn native_grid(&self) -> Option<GridGeometry>;

    /// Minimum and maximum over the field's data.
    ///
    /// Free for a sampled source — one pass during parsing, over samples
    /// already being read. `None` for an analytic source, which has no data
    /// to scan until something samples it.
    ///
    /// This replaces a semantic type tag (§Why there is no semantic tag or
    /// metadata). `min >= 0` means the field is non-negative, so a consumer can
    /// skip the negative-level extraction; the span sets a log slider's bounds.
    /// Both are *derived*, so they work for any scalar quantity chemistry
    /// produces, not just the three or four anyone thought to name.
    fn value_range(&self) -> Option<(f64, f64)>;
}
```

**The out-of-bounds rule is `0.0`, not an error.** A finite cube box is a
*window* onto a field that decays to zero; `0.0` is the physically correct
answer just outside it, and it keeps every consumer free of an error path in
its innermost loop. This is the *only* rule: `sample_field` returns `0.0` out of
bounds too (§Node: `sample_field`), so no consumer anywhere layers a second,
stricter convention on top.

**Why `data_bounds` and `suggested_bounds` are separate.** They coincide for
`.cube` and diverge for Molden, where the field is defined everywhere but a
consumer still needs a finite box to work in. Collapsing them into one method
would force the Molden implementation to lie about one or the other.

**There is deliberately no `suggested_spacing`.** The field knows *where* to
look — only it has the atom positions, which is why `suggested_bounds` is not
`Option` — but not *how finely*, which is a consumer-side quality decision
(draft versus final, zoom level, time budget). Its one genuine contribution to
resolution is `native_grid`. A consumer choosing its own lattice writes the
fallback at the call site:
`native_grid().map(|g| g.spacing()).unwrap_or(quality.default_spacing)`.

### Bounds convention

For a sampled field, `suggested_bounds` (and `data_bounds`) is the box **through
the outermost sample points**. Cube grids are **node-centered** — the origin *is*
sample `(0,0,0)`, not a voxel corner — so with `dims = [nx, ny, nz]` the box
spans `nx - 1` steps while containing `nx` samples.

Both conventions exist in the wild, and picking one silently is how a half-voxel
offset creeps in. It also fixes the fencepost: walking `min` to `max`
**inclusive** at the native spacing gives exactly `nx` points on the stored
values, whereas iterating exclusively of `max` silently drops the last plane —
one more reason to prefer `native_grid` verbatim over rebuilding a lattice from
a box and a step.

### Gradient

The trait's default central difference exists so a third implementation is never
*forced* to write one, but both concrete implementations should override it:

- **`AnalyticField`** — the exact analytic derivative. A Gaussian's derivative is
  as cheap as its value, so there is no reason to approximate.
- **`SampledField`** — central differences **on the stored samples directly**,
  not on interpolated values. Exact with respect to the stored data, cheaper than
  three interpolated pairs, and precisely what isosurface normals want.

### `FieldBounds`

No axis-aligned bounding-box type exists in the workspace to reuse, so:
`struct FieldBounds { min: DVec3, max: DVec3 }`, real-space Ångström. If a
general AABB is wanted elsewhere later it can move to `atomcad-util`.

### Why there is no semantic tag or metadata

Earlier drafts carried a `FieldKind` enum (`Amplitude` / `Density` /
`Potential` / `Unknown`, guessed from the comment line, overridable on the node)
and a `FieldMetadata` struct (`label`, `field_index`, `orbital_index`, `energy`,
`occupancy`, `spin`, `symmetry`). Both are gone — recorded here because an
absence is what a later contributor tries to "fix".

`FieldKind` was claimed to drive four things and survived none. **Surface
count**: extract at `+level` and `-level` unconditionally; on a non-negative
field the negative pass finds no crossings. (It cannot produce speckle either —
a density is a sum of squares, and trilinear interpolation of non-negative
values stays non-negative.) **Slider scale**: the level is `±L` with `L > 0`, so
log is right for everything. **Forms-vs-paints**: advisory, never enforced.
**Default threshold**: real, but the honest answers — a fixed constant, or a
percentile-of-magnitude rule — are data-driven, and which one wins is deferred.

The decisive objection is generality: chemistry produces density, amplitude,
ESP, spin density, deformation density, ELF, reduced density gradient, ALIE,
Fukui functions, the Laplacian. A four-variant enum cannot name that space,
while **signedness** and **magnitude scale** — the two properties that actually
drive rendering — are derivable from the data for all of them. Hence
`value_range`.

`FieldMetadata` fails the same test (*known use, not easily determined
otherwise*). Four members would be permanently `None` across this document's
whole scope; `field_index` is the container's own index; `orbital_index` belongs
to the P5 multi-field variant. `label` is populated but unconsumed — PySCF
writes one field per file, so each `import_cube` node is already labelled by its
file name, and the **node** disambiguates rather than the value. Per-field
identity earns its place only when one node emits many fields (P5, Molden),
where it can be shaped by what those actually need. `Spin` and `metadata()` go
with it.

### `SampledField` — the `.cube` implementation

```rust
/// A field stored as samples on a regular grid, with trilinear interpolation
/// between them.
#[derive(Debug, Clone)]
pub struct SampledField {
    grid: GridGeometry,
    /// Inverse of the 3x3 matrix whose columns are `grid.axes`, cached at
    /// construction so `sample` does not rebuild it per call. Lives here and
    /// not on `GridGeometry`, which stays a small `Copy` description of where
    /// the samples are.
    inv_basis: DMat3,
    /// Row-major with the LAST axis contiguous: index
    /// `(i * dims[1] + j) * dims[2] + k`. This matches the `.cube` traversal
    /// order, so the loader fills it sequentially with no transposition.
    samples: Vec<f32>,
    /// Min and max, accumulated during parsing — free there, a full grid
    /// rescan later.
    value_range: (f64, f64),
}

/// Grid placement in real space. General enough for a sheared grid even
/// though PySCF only ever writes axis-aligned ones — the format permits it,
/// and the generality costs one matrix instead of three scalars.
///
/// This is also what `ScalarField::native_grid` hands back, so it is the
/// complete answer to "where exactly are the stored samples" — origin, three
/// axis vectors, and counts, with no convention left implicit.
#[derive(Debug, Clone, Copy)]
pub struct GridGeometry {
    /// Position of sample (0,0,0), Ångström. Node-centered: this IS a sample
    /// point, not a voxel corner.
    pub origin: DVec3,
    /// Step vectors along the three grid axes, Ångström.
    pub axes: [DVec3; 3],
    /// Sample counts along each axis.
    pub dims: [usize; 3],
}

impl GridGeometry {
    /// Per-axis step lengths, Ångström. Convenience for consumers choosing
    /// their own lattice; exact only for an axis-aligned grid, so a consumer
    /// that must handle shear uses `axes` directly.
    pub fn spacing(&self) -> DVec3 { /* axis vector lengths */ }

    /// Box through the outermost sample points (see §Bounds convention).
    pub fn bounds(&self) -> FieldBounds { /* origin .. origin + (dims-1)*axes */ }
}
```

`f32` storage, not `f64`: the source data has nothing like `f64` precision, and
halving the footprint matters when a single field is a few megabytes and a
Molden-scale gallery could hold dozens. Interpolation is done in `f64`.

`sample` inverts the grid basis to get fractional indices, then interpolates
trilinearly. For the axis-aligned case this is three divisions; the general case
caches the inverse basis matrix at construction, as `GeoNodeKind::Ellipsoid`
already does for `inv_basis`.

## Value plumbing

### `DataType::ScalarField`

A new variant on `DataType` (`atomcad-structure-designer/src/data_type.rs`). It
is an ordinary first-class pin type — not an `Optional`-style modifier, not
abstract, with no subtyping relationships and no implicit conversions to or from
anything.

Touchpoints for adding it. Most are existing per-variant tables, but the list
spans Rust core, the FRB boundary and Dart — fourteen sites plus a codegen run,
not the handful it looks like. Note which ones the compiler catches:
`to_display_string` and the Dart switches are exhaustive and will fail the
build, but **`infer_data_type` has a `_ => None` arm, so omitting it compiles
and silently mis-infers the type**.

**Rust core (`atomcad-structure-designer`):**

| Site | Change |
|---|---|
| `data_type.rs` `enum DataType` | new variant |
| `data_type.rs` `impl fmt::Display` (~line 339) | `=> write!(f, "ScalarField")` |
| `data_type.rs` `from_string` keyword table (~line 1437) | `"ScalarField" => Ok(DataType::ScalarField)` |
| `text_format/node_type_introspection.rs` (~line 104) | add to the "no text literal representation" list, beside `Blueprint` / `Motif` / `Structure` |
| `evaluator/network_result.rs` `infer_data_type` (~line 320) | `NetworkResult::ScalarField(_) => Some(DataType::ScalarField)` — **not a compile error if missed** |
| `evaluator/network_result.rs` `to_display_string` (~line 880) | short summary; exhaustive match |
| `evaluator/network_result.rs` `to_detailed_string` (~line 952) | optional — dims and value range; otherwise falls through `_ =>` to `to_display_string` |

**FRB boundary (root crate):**

| Site | Change |
|---|---|
| `src/api/structure_designer/structure_designer_api_types.rs` `enum APIDataTypeBase` (~line 35) | new variant |
| `src/api/structure_designer/structure_designer_api.rs` (~line 277) | `APIDataTypeBase::ScalarField => DataType::ScalarField` |
| `src/api/structure_designer/structure_designer_api.rs` (~line 443) | `DataType::ScalarField => APIDataTypeBase::ScalarField` |
| — | re-run `flutter_rust_bridge_codegen generate` |

**Dart:**

| Site | Change |
|---|---|
| `lib/inputs/data_type_input.dart` (~line 321) | type-selector entry |
| `lib/inputs/type_editor_dialog.dart` (~line 82) | type-editor entry |
| `lib/structure_designer/node_network/node_widget.dart` `_apiDataTypeToString` (~line 445) | display name; exhaustive switch |
| `lib/structure_designer/node_network/node_network.dart` (~line 207) | pin color entry |

`canonicalize_data_type` needs no change — the variant has no nested type.

Pin color: the existing map is organized in families (indigo/blue matrices,
purple geometry, green phases, teal crystal, amber functions, grey `Unit`). A
soft red such as `0xFFE57373` is free and reads as a distinct family for
"sampled volumetric data". Worth eyeballing against the live palette before
committing.

### `NetworkResult::ScalarField`

```rust
/// A scalar field value. `Arc` because `NetworkResult` is cloned freely
/// throughout evaluation and the payload is megabytes; the other large
/// variants (`Molecule`, `Crystal`) clone deeply, which is affordable at
/// their size and would not be at this one.
ScalarField(Arc<dyn ScalarField>),
```

**Deliberate on both counts.** `dyn` lets one variant carry both a sampled and
an analytic field, so Molden adds no variant and touches no `match`. `Arc` keeps
`NetworkResult::clone` cheap — the evaluator clones freely, and deep-cloning a
multi-megabyte grid per wire traversal would be a real performance defect.
`Arc`-shared payloads are already established here — `Walker::from_array` holds
an `Arc<Vec<NetworkResult>>`, and evaluation contexts hold
`Arc<HashMap<CaptureKey, NetworkResult>>` — so this is a familiar shape, not a
departure. Unlike those two, this `Arc` wraps a `Send + Sync` payload, so it
needs no `#[allow(clippy::arc_with_non_send_sync)]`.

Consequences to handle — fewer than one might expect, because `NetworkResult`
derives only `Clone` and `Default`:

- **There is no `PartialEq` to satisfy.** `NetworkResult` does not derive it,
  and nothing in the tree compares two results. Do **not** hand-write an
  `Arc::ptr_eq` equality for this variant: it would be the only comparison in
  the enum and it has no caller.
- **There is no `Debug` to satisfy either** — `NetworkResult` is not `Debug`.
  The trait keeps its `Debug` supertrait on separate merit: it makes fields
  printable in loader tests and assertion messages. Implementations print a
  summary (dims and value range), never their samples.
- **`Default` is unaffected**: the derive is anchored to `#[default] None`, so
  adding a variant changes nothing.
- The human-readable paths are *not* derived and do need arms — see the
  touchpoint table above. `to_display_string` is exhaustive and will fail the
  build; `infer_data_type` has a `_ => None` arm and will not.
- `NetworkResult` is never serde-serialized — results are recomputed from the
  network — so there is **no `.cnnd` surface** and no migration.
- `NetworkResult` is deliberately **not** `Send`/`Sync` (see the note on
  `empty_captures` in `evaluator/network_evaluator.rs`), and the evaluator is
  single-threaded today. `ScalarField: Send + Sync` therefore buys nothing at
  this variant; it is required by the *sampling* consumers (§Rendering
  direction), and holding the bound on the trait costs nothing.

## Node: `import_cube`

| | |
|---|---|
| **Name** | `import_cube` |
| **Category** | `AtomicStructure` — beside `import_xyz` / `import_cif` |
| **Input pin 0** | `file_name: String` — optional; wired value overrides the stored property, matching `import_xyz` |
| **Output pin 0** | `field: ScalarField` — the file's field |
| **Output pin 1** | `molecule: Molecule` — atoms from the file's atom block |
| **Property** | `file_name: Option<String>` |

Node data follows `ImportXYZData` (`nodes/import_xyz.rs`) closely: `file_name`
persisted, parsed payload `#[serde(skip)]`, a `node_data_loader` that reloads
after deserialization, and a `node_data_saver` that relativizes the path via
`try_make_relative` so projects stay portable. This keeps megabytes of samples
out of the `.cnnd` file, exactly as `AtomicStructure` is kept out today.

`eval` takes `CubeFile.fields[0]` for pin 0 and `CubeFile.atoms` for pin 1.
Before P5 that indexing is total: parsing rule 1 rejects every multi-field file,
so `fields` always holds exactly one element.

Output pin 1 (`molecule`) is what makes ingestion verifiable before any field
rendering exists.

**The multi-field pins are deliberately absent.** An `index: Int` input and a
`fields: Array[ScalarField]` output are the right shape for a multi-field cube
and, later, a Molden file's full orbital set — but nothing can produce or test
either before P5, and P5 may wait indefinitely for a file to validate against
(§P5). Shipping them at P3 would mean an inert `index` and an output that always
carries a one-element array. Because this project's rule is that new pins are
**appended, never inserted**, adding them in P5 as input pin 1 and output pin 2
is free and breaks nothing built in the meantime — so there is no cost to
waiting and a small cost to not.

When P5 lands, `index` (default `0`) selects which field output pin 0 carries,
and an out-of-range `index` is an evaluation error naming the valid range.
Selecting by *position* is the wrong affordance for Molden — "the HOMO" is what
a user wants — but that needs occupancy data only Molden supplies, so a
metadata-driven selector is future work built on top.

**Explicitly not included: a statistics readout in the node editor.** Min, max
and percentiles would help choose thresholds, but that is a visualization
concern, and `sample_field` already makes the parser verifiable without it.

The editor widget follows
`lib/structure_designer/node_data/import_xyz_editor.dart` — a file picker and
nothing else, registered in `node_data/node_data_widget.dart`. The picker uses
the remembered import directory, per the existing last-directories behavior.

## Node: `sample_field`

| | |
|---|---|
| **Name** | `sample_field` |
| **Category** | `MathAndProgramming` |
| **Input pin 0** | `field: ScalarField` — required |
| **Input pin 1** | `point: Vec3` — required; real-space Ångström |
| **Output pin 0** | `value: Float` |

Evaluates the field at one point. Small, pure, and the thing that makes the
whole ingestion half testable inside the running application: wire a `vec3` into
it, wire the result into `print`, and read values off the console.

**Out-of-bounds returns `0.0`, exactly as the trait specifies.** One rule
everywhere, with no second convention to remember and no error path in the
node.

An earlier draft made this an evaluation error naming the field's bounds, on the
reasoning that the likeliest ingestion bug is a missed Bohr-to-Ångström
conversion. That diagnostic is not worth its cost. The same bug is caught one
phase earlier and far more legibly at P3, where the `molecule` pin renders the
structure 1.89x too large through the existing impostor path — a picture, not a
message. Meanwhile the error rule would break this node's other reason to exist:
sampling an orbital at a point, or along a bond via `map` over a point array, is
a genuine analysis capability that works through the CLI, and a hard error would
fail the whole evaluation the moment one point strayed past the box edge —
exactly the region where a decaying field is most interesting.

## The `.cube` format — parser specification

Plain ASCII. Structure, with the multi-field variant shown:

```
 Comment line 1                                <- free text; read past, not retained
 Comment line 2                                <- free text
   -3   -5.000000  -5.000000  -5.000000        <- natoms (SIGNED), origin xyz, [NVal]
    2    5.000000   0.000000   0.000000        <- N1, step vector along axis 1
    2    0.000000   5.000000   0.000000        <- N2, step vector along axis 2
    2    0.000000   0.000000   5.000000        <- N3, step vector along axis 3
    8   8.000000   0.000000   0.000000   0.220000    <- Z, charge, x, y, z
    1   1.000000   0.000000   1.430000  -0.880000
    1   1.000000   0.000000  -1.430000  -0.880000
    2    5    6                                <- multi-field ONLY: count, then indices
   1.0e-05  2.0e-05  3.0e-05  4.0e-05          <- values
   ...
```

### Parsing rules, in the order they bite

**1. `natoms` is signed and the sign is a flag, not a count.** A negative value
means *"this is the multi-field variant"* — the atom block is present and
complete regardless. Use `natoms.unsigned_abs()` for the atom loop and
`natoms < 0` for the flag. The classic bug is `for _ in 0..natoms` silently
iterating zero times on a negative value, after which every subsequent read is
misaligned and may still parse into plausible garbage.

**Before P5 the flag is a rejection, not a branch.** P1 reads the sign correctly
in order to *detect* the multi-field variant and then fails with a clear
"multi-field cube files are not yet supported" error. Rules 2 and 4 below
specify the multi-field parse for when P5 implements it; do not implement them
in P1.

**2. Line 3 has either four or five numbers.** The optional fifth is `NVal`,
values per grid point — redundant with the multi-field index line, but some
writers emit it. This is the one place where line structure matters: **read line
3 as a line and count its tokens**, then switch to token-stream parsing for
everything after.

**3. Parse the numeric body as a token stream, not line by line.** Files in the
wild vary in whitespace and wrapping more than the format's description
suggests. The nominal layout is six values per line with a break at the end of
each innermost run, but nothing should depend on that.

**4. Values are interleaved per grid point in the multi-field variant** — the
field index is the *fastest*-varying axis, effectively shape `(N1, N2, N3, m)`
in C order:

```
for i in 0..N1 { for j in 0..N2 { for k in 0..N3 { for f in 0..m { next } } } }
```

Assuming "all of field 0, then all of field 1" produces garbage. De-interleave
into contiguous per-field `Vec<f32>` at load time, which is what `SampledField`
wants anyway. PySCF will not produce a multi-field file to test against, so
**this rule must be validated against a real Gaussian-produced file before it is
trusted** — until then, treat the multi-field path as provisional and say so in
a code comment.

**5. Traversal order is x-slowest, z-fastest.** Matches `SampledField`'s
declared layout, so the loader appends sequentially with no transposition.
Getting this wrong transposes or mirrors the field, and — critically — is
**invisible in any axis-symmetric test case**. See the ramp test in §P1.

**6. Units.** Coordinates and step vectors are Bohr in everything PySCF and
Gaussian write. A convention exists whereby a negative voxel count signals
Ångström, but it is documented inconsistently across sources — some describe
negative as Bohr — so **do not rely on either reading**.

**Always read coordinates and step vectors as Bohr**, and use the atom block as
a *plausibility check* only — never as an override:

- compute nearest-neighbour distances across the atom block and compare against
  covalent radii, which `crystolecule` already has. A C–C single bond is 1.54 Å
  or 2.91 Bohr — a factor of 1.89 apart, with no plausible ambiguity.
- **concretely**: for each atom take the distance to its nearest neighbour, and
  divide by the sum of the two covalent radii; warn when the **median** of those
  ratios falls below `0.7` or above `1.6`. A correctly-read Bohr file lands near
  `1.0`. An Ångström file read as Bohr is scaled by 0.529, so it lands near
  `0.53` — well inside the lower trip. The window is wide enough that ordinary
  chemistry never trips it and narrow enough that a factor of 1.89 always does.
- when the check trips, record a `units_warning` naming the observed median
  ratio. The `import_cube` node surfaces it as a non-blocking
  `NodeDataError::warning` from `get_data_error`, which is correct by the
  blocking litmus in `doc/design_error_management.md` — the node still produces
  a usable field, and the user is told exactly what looks wrong.
- **do not re-interpret the file on the strength of the check.** Short contacts
  are not the only thing an atom block can hold: an ion pair, a van der Waals
  cluster, two separated fragments, or one stretched bond all produce distances
  the heuristic cannot distinguish from an Ångström file. Acting on that guess
  silently rescales every coordinate — and with them the grid, the field, and
  every threshold read off it — by 1.89, which is a wrong answer the user
  *cannot see*, traded against a wrong answer the warning already names.
  Warning-only keeps the whole diagnostic value and none of the failure mode.

A file with fewer than two atoms has no distances to check; stay silent.

**7. Malformed input yields a descriptive error, never a panic.** Truncated
value blocks, non-numeric tokens, zero dimensions, and non-finite samples are
all rejected with the byte or token offset. The loader signature mirrors
`load_xyz`: `Result<CubeFile, io::Error>`.

### Loader output

```rust
pub struct CubeFile {
    pub atoms: AtomicStructure,
    /// One per field in the file. Before P5 this always has exactly one
    /// element (rule 1). Every entry carries an identical `GridGeometry`.
    pub fields: Vec<SampledField>,
    /// Set when the atom block's interatomic distances look chemically
    /// implausible under the assumed Bohr units. Advisory only — coordinates
    /// are always read as Bohr regardless.
    pub units_warning: Option<String>,
}
```

## Forward compatibility: Molden

### What a Molden file actually contains

Exactly three things: the **geometry**, the **basis set** (contracted Gaussian
shells per atom), and the **MO coefficient matrix**. Nothing else. In
particular it contains **no sampled data at all** — no density, no potential,
no grid.

The `[MO]` section holds one entry per orbital, covering **every** orbital the
basis produces — the occupied ones and the empty (virtual) ones alike, which is
why LUMO and above are available. Each entry is a coefficient vector plus four
metadata items:

```
 Sym= 5a1
 Ene= -0.4977          <- a NUMBER, not a field
 Spin= Alpha
 Occup= 2.000000
    1    0.994216      <- M coefficients, one per basis function
    2    0.026068
   ...
```

**`Ene=` is not a field.** It is a single eigenvalue per orbital — nothing to
sample, nothing to draw. It, `Occup=`, `Spin=` and `Sym=` are per-orbital
metadata, and together they are what makes automatic HOMO selection possible:
*the highest-index orbital with `Occup > 0`*. Where they should live is a
question for the Molden work — the base contract deliberately carries no
metadata (§Why there is no semantic tag or metadata), and `Wavefunction` is the
natural home, since these describe orbitals in a file rather than properties of
a sampled scalar function.

### Stored versus derived quantities

The distinction that shapes the node design: a `.cube` file is a **snapshot** —
someone already chose what to compute and at what resolution, and you get
exactly that. A Molden file is a **source** — you hold the wavefunction and can
generate any derived quantity on demand, at any resolution.

| Quantity | In the file? | How many | Cost to derive |
|---|---|---|---|
| Orbital amplitude `psi_i` | yes, as coefficients | **one per orbital** (M of them) | cheap — evaluate the basis, contract with one column |
| Orbital energy / occupancy / spin / symmetry | yes, literally | one value per orbital | free; it is metadata, not a field |
| Electron density `rho` | **no** — derived | **exactly one** per molecule | moderate — build the density matrix, then a quadratic form per point |
| Spin density | **no** — derived | one (open-shell only) | moderate — the alpha/beta density difference |
| Electrostatic potential | **no** — derived | **exactly one** per molecule | **expensive** — nuclear-attraction integrals over all basis pairs, per point |

Density and potential are *one each*, not one per orbital, because both are
properties of the **total** electron distribution:

```
rho(r) = sum over occupied i of  occ_i * |psi_i(r)|^2
```

One molecule has one density and one potential regardless of how many orbitals
were summed to build them.

### Node shape: `Wavefunction` as the parsed file

Because the derived quantities must not be computed eagerly, `import_molden`
cannot be a straight clone of `import_cube`. The parsed file gets its own type,
and each field is a *view* of it:

```
import_molden ──> Wavefunction    (geometry + basis + coefficients + metadata)
              └─> Molecule

orbital(Wavefunction, index) ──> ScalarField     signed
density(Wavefunction)        ──> ScalarField     non-negative
esp(Wavefunction)            ──> ScalarField     signed   (see below)
```

Laziness falls out of the shape: nothing is evaluated until a node asks for it,
and asking for one orbital never costs a density.

A minimal first cut may skip `Wavefunction` and have `import_molden` emit
`Array[ScalarField]` of orbitals plus `Molecule`, matching the shape P5 gives
`import_cube`. That
is a reasonable staging decision, but the `Wavefunction` shape is the one to
aim at — retrofitting it later means changing a node's pins rather than adding
nodes, and new pins must be appended, never inserted.

**`DataType::Wavefunction` is still purely additive**, which is the claim this
whole section exists to make checkable. It adds a type and some nodes; it
changes nothing that already exists.

### Electrostatic potential: a node we will not build for a long time

`esp(Wavefunction)` is listed above for completeness, and it is **not planned**.

Computing an ESP from a wavefunction requires nuclear-attraction integrals over
every basis-function pair at every grid point. It is a substantially harder and
slower piece of quantum chemistry than orbital or density evaluation — slow
even inside PySCF, which has decades of optimized integral code behind it.
Reimplementing it in Rust would be a large effort for a quantity PySCF already
computes correctly via `cubegen.mep`.

**So the expected workflow, indefinitely and including after Molden support
lands, is: orbitals and density from `.molden`, electrostatic potential from a
`.cube` file.** The two paths compose without friction — a density from a
wavefunction and a potential from a cube meet at the isosurface node as two
ordinary `ScalarField`s, which is exactly what the deferred color-by design
needs. Nothing about mixing sources is a special case.

### What Molden buys, and what it costs

**Buys:** resolution independence — no fixed grid, so no faceted lobes when
zoomed in, and a GPU-evaluated field becomes possible. Also all orbitals in one
file rather than ~6 MB each, though the coefficient block is M×M and so grows
with the **square** of basis size: tens of kilobytes for a small molecule, tens
of megabytes for a large one.

**Costs:** implementing contracted-Gaussian evaluation, a known minefield whose
failures are mostly *silent* — spherical-versus-Cartesian shells (the `[5D]` /
`[7F]` / `[9G]` flags, Cartesian by default), component ordering within a shell,
normalization conventions, and per-program variation. Wrong ordering or
normalization yields a plausible-looking but wrong field, which is exactly what
the next subsection exists to catch.

### Why `.cube` first: it is the test oracle

Export both formats from one PySCF calculation — `cubegen.orbital(mol,
'ref.cube', mf.mo_coeff[:, i])` and `molden.from_scf(mf, 'ref.molden')` — then
sample the Molden evaluator at the cube's exact grid points and compare
(~1e-6 relative). That is a half-million-point ground truth from a trusted
implementation, for free, and it turns every ordering, normalization and sign
bug into an immediate numerical mismatch rather than a subtly wrong picture
months later. The oracle exists *only because* the cube path was built first.

### What Molden support will and will not touch

**Unchanged:** `ScalarField`, `FieldBounds`,
`DataType::ScalarField`, `NetworkResult::ScalarField`, `sample_field`,
`import_cube`, the pin color, and every deferred visualization consumer.

**Added:** `io/molden_loader.rs`; a `Wavefunction` type and
`DataType::Wavefunction`; an `AnalyticField` implementing `ScalarField`
(overriding `sample_batch` to reuse per-shell setup across a batch and
`gradient` with the exact derivative, returning `None` from **both**
`data_bounds` and `native_grid`, and deriving `suggested_bounds` from atom
positions plus a ~1.6 Å margin — the equivalent of `cubegen`'s default 3 Bohr);
the `import_molden`, `orbital` and `density` nodes; and per-orbital `energy` /
`occupancy` / `spin` / `symmetry` metadata, carried on `Wavefunction`.

**The two asymmetries the contract already absorbs**, both expressed as an
`Option` returning `None`:

- an analytic field is **unbounded**, so `data_bounds` is `None` while
  `suggested_bounds` still returns a derived box — which is why the latter is
  not optional.
- an analytic field has **no preferred lattice**, so `native_grid` is `None`.
  This is the load-bearing one: it is exactly why no consumer may be written
  against a grid, and why the first Molden field will immediately expose any
  consumer that was.

Every consumer takes an explicit box and resolution, with "auto from the field"
as the default, so both sources look identical from the outside and Molden's
extra freedom surfaces as a resolution control rather than a special case.

## Rendering direction

Stated here only insofar as it constrains the contract; the design is deferred.

**Direction: CPU isosurface extraction into the existing triangle-mesh pipeline
first, GPU volume raymarching as a possible successor.**

The existing `mesh.wgsl` pipeline takes vertices carrying position, normal,
albedo, roughness and metallic — per-vertex color included. That is exactly what
an extracted isosurface needs, which means the first implementation adds no
shader, no bind group and no renderer surgery. Per-vertex albedo also means
painting one field's values onto another field's surface (the standard
density-surface-colored-by-potential picture) needs no new pipeline either.

GPU raymarching of a 3D texture is the closer analogue of the existing atom
impostors and is the better eventual answer — a free isolevel slider,
resolution-independent lobes, and true volumetric rendering that surface
extraction cannot do at all. It is not first, because it needs a new pipeline
and careful depth interop with the impostor passes.

**What this costs the contract: nothing, provided sampling stays batchable and
thread-safe.** A dense grid for a 3D texture is derivable from `sample_batch`,
so `Send + Sync` and the batch method are the entire accommodation, and both are
specified above. The constraint to respect is negative: **do not make sampling
stateful or single-threaded.**

Two things the deferred design will have to confront, recorded here so they are
not rediscovered:

- **Transparency for meshes does not exist yet.** `transparent_impostor.wgsl`
  and `transparent_sort.rs` handle impostor quads only. Orbitals are
  conventionally semi-transparent. The cheap approach for closed blobs is a
  two-pass back-faces-then-front-faces draw rather than porting the per-quad
  sort to triangles.
- **A wavefunction is not an SDF.** `ImplicitGeometry3D` is the right interface
  *shape*, and `BatchedImplicitEvaluator`'s batching and rayon parallelism are
  worth mirroring, but a field must not become a `GeoNodeKind`: SDF composition
  relies on values being signed *distances*, where union is `min` and
  intersection is `max`, and those identities are false for an amplitude. Keep
  `ScalarField` a separate trait.

## Implementation plan

Each step ends green: `cargo test -j 4`, `cargo clippy`, `flutter analyze`.
Every phase below carries **its own tests**, and every phase with UI carries
**its own manual walkthrough**; a phase is not done until both pass. Only the
shared fixture machinery sits outside the phases, because P1 builds it and P3/P4
consume it.

### Shared: fixtures and sample data

Tests live in `crates/atomcad-crystolecule/tests/crystolecule/` for the loader
and field, and under the structure-designer crate's `tests/` for the nodes, per
the "tests go in the owning crate's `tests/`" rule.

Fixtures are small `.cube` files under `rust/tests/fixtures/cube/`, addressed
through `atomcad_test_support::fixture_path`. Keep them tiny (a 3x4x5 grid is 60
values) so they are readable and diffable in review. They are generated by
script but **committed**, and the committed file is the hand-checkable artifact
— a test asserts against literal values a reviewer can verify by eye, never
against whatever the generator happened to emit.

`scripts/make_cube_fixtures.py`, **numpy only**, beside the existing
`scripts/architecture_diagram/` helpers. Three subcommands:

| Command | Writes | Committed? |
|---|---|---|
| `tests` | `rust/tests/fixtures/cube/` — the fixtures the phase test tables name | yes, tiny |
| `manual` | `sample_data/cube/` (gitignored, `--out` to override) — the eyeball files below | no, regenerate on demand |
| `pyscf` | `rust/tests/fixtures/cube/water_homo.cube` — one low-resolution realism fixture | yes, once; **optional**, see below |

The malformed fixtures (truncated, non-numeric, zero-dim, negative `natoms`) are
hand-edited copies of a valid one rather than script output — the corruption is
the point, and it should be visible in the diff.

Files the `manual` subcommand writes, used by the P3 and P4 walkthroughs:

| File | Contents | What it exercises |
|---|---|---|
| `water.cube` | real water geometry in **Bohr** (O–H 0.958 Å, H–O–H 104.5°), coarse grid, a crude analytic 2p_z on the oxygen | P3: atom block, Bohr→Ångström, bonding |
| `water_angstrom.cube` | the same geometry written in Ångström | P3: the `units_warning` path |
| `ramp_3x4x5.cube` | the automated ramp fixture, copied for interactive use | P4: exact expected sample values |

#### PySCF: optional, and deliberately not on the critical path

**Nothing in P1–P4 depends on PySCF, and the plan must not acquire such a
dependency.** Within this document's scope no field is ever rendered, so a real
PySCF orbital shows the same three atoms a numpy-written file does, and yields a
sampled number that cannot be verified by inspection — whereas the ramp
fixture's expected value is exact. numpy covers every test and every walkthrough
step in this plan.

What PySCF buys is **realism a hand-written file cannot fake**: a real producer's
whitespace and line wrapping, `%13.5E` value formatting, six values per line with
a break at the end of each innermost run, 80^3 dimensions, a few megabytes, and
genuine Bohr coordinates — precisely the variation that parsing rule 3
(token stream, not lines) exists to survive. That is worth exactly one committed
smoke fixture: one water HOMO at low resolution, via
`cubegen.orbital(mol, 'water_homo.cube', mf.mo_coeff[:, i])`. PySCF becomes
genuinely load-bearing only later, as the Molden test oracle (§Why `.cube`
first: it is the test oracle).

**Setup cost on the maintainer's Windows machine: small but not zero.** PySCF
publishes no Windows wheels, so it runs under WSL; WSL2 with Ubuntu 24.04 is
already installed, with Python 3.12 but no `pip` and no `venv`, and Ubuntu 24.04
refuses system-wide `pip install` (PEP 668). So:

```bash
wsl -d Ubuntu                                  # then, inside:
sudo apt update && sudo apt install -y python3-venv   # one interactive password
python3 -m venv ~/pyscf && ~/pyscf/bin/pip install pyscf
```

A few minutes, one `sudo` prompt, and it pulls numpy/scipy/h5py. If that is ever
inconvenient, skip it — the `pyscf` subcommand is the only thing that needs it,
and everything else in this document still works.

### P1 — Field abstraction and cube loader (backend only)

**Work**

- `rust/crates/atomcad-crystolecule/src/field/mod.rs`: `ScalarField`,
  `FieldBounds`, `GridGeometry`, `SampledField`, `DEFAULT_GRADIENT_STEP`
- `rust/crates/atomcad-crystolecule/src/io/cube_loader.rs`: `load_cube`
  producing `CubeFile`,
  single-field path plus the units plausibility warning; `value_range`
  accumulated in the same pass that reads the samples
- `scripts/make_cube_fixtures.py` and the committed fixtures it writes
- no Flutter, no node, no API

**Tests**

**The ordering test must use an asymmetric field.** This is the single most
important test in the plan. The dominant bug class for grid parsers is axis
transposition and mirroring, and *any axis-symmetric fixture hides it
completely* — a centered s-type blob is identical under every permutation of
axes. Use a synthetic ramp:

```
value(i, j, k) = 100*i + 10*j + k
```

on a grid with three *different* dimensions (e.g. 3x4x5, so a transposition
cannot even preserve the shape). Then assert `sample` at each exact grid point
returns its own index code. This pins down every permutation and reversal at
once, and it is the test that would otherwise fail silently.

| Test | Asserts |
|---|---|
| Ramp field, 3x4x5 | axis order, no mirroring, no transposition |
| Trilinear midpoint | interpolation between known samples |
| Synthetic 2p_z | sign changes across the nodal plane; negative values preserved, not clamped |
| Atom block vs reference `.xyz` | element, count and position agreement |
| Bohr fixture | positions convert to Ångström; bond lengths chemically sane |
| Ångström-scaled fixture | median ratio near `0.53` trips the low bound, `units_warning` set — **and coordinates are still read as Bohr**, i.e. no silent rescale |
| Widely-spaced fixture (two separated fragments) | median ratio above `1.6` trips the high bound, positions unchanged — the check never alters the parse |
| Single-atom fixture | fewer than two atoms, so no distances to check: silent, no spurious warning |
| Truncated / non-numeric / zero-dim fixtures | descriptive error, no panic |
| `value_range` on every fixture | matches the min/max of the fixture's literal values |
| Out-of-bounds `sample` | exactly `0.0` |
| `SampledField::gradient` on a fixture sampled from a known formula (the 2p_z) | matches that formula's analytic derivative within tolerance |
| Negative `natoms` | clear "multi-field not yet supported" error (replaced by P5) |

**Deliverable:** a parser with numerically verified axis ordering and unit
handling. Nothing user-visible.

### P2 — `DataType` and `NetworkResult` plumbing

**Work**

- `DataType::ScalarField` and **all fourteen** touchpoints plus the FRB codegen
  run — `data_type.rs`, `node_type_introspection.rs`, `network_result.rs`, the
  three API sites, and four Dart sites (see the table above). Budget this as a
  cross-language step, not a one-line enum addition.
- `NetworkResult::ScalarField(Arc<dyn ScalarField>)` and its `to_display_string`
  / `infer_data_type` arms. No `PartialEq` and no `Debug` impl is needed.
- Dart pin color and type-selector entries

**Tests**

| Test | Asserts |
|---|---|
| `DataType` text round-trip | `from_string("ScalarField")` and `Display` agree |
| `APIDataType` round-trip, both directions | catches a missed conversion arm at the FRB boundary |
| `infer_data_type` on a `NetworkResult::ScalarField` | returns `Some(DataType::ScalarField)` — **the one site with no compiler backstop**, so it needs an explicit test |
| Existing registry-validation suite | stays green |

**Deliverable:** a wireable type carrying no values yet.

### P3 — `import_cube` node and editor

**Work**

- node data, `eval`, loader/saver following `ImportXYZData`
- registration in `nodes/mod.rs` and `node_type_registry.rs`
- `import_cube_editor.dart` (file picker only) and its
  `node_data_widget.dart` entry
- reference guide: `doc/reference_guide/nodes/atomic.md`

**Tests**

| Test | Asserts |
|---|---|
| Node eval on a fixture | `molecule` pin matches the fixture's atom block; `field` pin's `value_range` matches the fixture |
| `file_name` input pin wired | overrides the stored property, mirroring the `import_xyz` test |
| `.cnnd` round-trip | saved path is relativized; after reload `node_data_loader` has repopulated the `#[serde(skip)]` payload |
| Fixture with `units_warning` | surfaces as a **non-blocking** `NodeDataError::warning` from `get_data_error`, and the node still produces a usable field |

**Manual walkthrough**

1. `import_cube` → load `sample_data/cube/water.cube` → display the `molecule`
   output pin.
2. **Expect** three atoms with O–H ≈ 0.96 Å and H–O–H ≈ 104.5°, bonded. If the
   molecule looks roughly **1.9x too large**, the Bohr conversion is missing —
   this render *is* the units check the design leans on (§Node: `sample_field`).
3. Load `water_angstrom.cube`. **Expect** the identical geometry **plus** a
   non-blocking amber warning on the node. Specifically expect the molecule
   *not* to be rescaled: the check warns, it never re-interprets (§rule 6).
4. Save the project, move the `.cnnd` to another folder, reload. **Expect** the
   field to still load — path relativization via `try_make_relative`.

**Deliverable:** **the first user-visible milestone.** Import a `.cube` and see
the molecule render through the existing impostor path off the `molecule` pin.
This validates the atom block, the Bohr-to-Ångström conversion, and path
relativization with zero new rendering code.

### P4 — `sample_field` node

**Work**

- node, registration, reference guide entry in
  `doc/reference_guide/nodes/math_programming.md`

**Tests**

| Test | Asserts |
|---|---|
| Sample at exact grid points of the ramp fixture | each returns its own index code |
| Sample at a midpoint | the trilinear average of its neighbours |
| Sample outside the box | returns exactly `0.0`, matching the trait — **not** an error |

**Manual walkthrough**

1. Wire `import_cube.field` → `sample_field.field`, a `vec3` → `.point`, and the
   result → `print`.
2. On `ramp_3x4x5.cube`, sampling exact grid point `(i, j, k)` prints
   `100*i + 10*j + k`. Sampling the midpoint of two neighbours prints their
   average. Both are checkable in your head, which is the whole point of the
   ramp.
3. A point well outside the box prints `0`, **not** an error (§Node:
   `sample_field`).
4. Repeat step 2 through `atomcad-cli` to confirm the headless path.

**Deliverable:** ingestion is fully verifiable in the running application and
from the headless CLI. **This closes the scope of this document.**

### P5 — Multi-field cube support (provisional)

Deliberately last, and separable: it cannot be validated without a
Gaussian-produced multi-field file. If no such file is available when P4 lands,
P5 waits rather than shipping untested parsing — until then the single-field
path rejects a negative `natoms` with a clear "multi-field cube files are not
yet supported" error rather than misparsing it (tested in P1).

**Work**

- the negative-`natoms` branch, the index line, and de-interleaving
- **appended**, so P3's numbering is untouched: the `index` input pin and the
  `fields: Array[ScalarField]` output pin

**Tests**

| Test | Asserts |
|---|---|
| A real Gaussian-produced multi-field file | de-interleaving is right — **this file existing is the gate on the phase**, per parsing rule 4 |
| Negative `natoms` | now parses, and the atom block is read completely (the `unsigned_abs` rule); P1's unsupported-variant test is replaced |
| `index` selection | picks the right field; out-of-range errors naming the valid range |
| Pin numbering | pins 0 and 1 keep their P3 meanings; the new pins are input 1 and output 2 |

**Deliverable:** multi-field cubes load, with the parse validated against a file
this project did not write.

## Deferred: the visualization node

Not designed here. What follows is the question list and the evidence that
settles each, so the deferral is a plan rather than a gap. Revisit once P4 lands
and real cube data can be rendered in a throwaway debug view.

### Settled in advance: surface topology and coloring

One sub-question is already answered, and answered without any semantic input —
it follows from the sign of the data. Recorded here so it is not re-litigated.

**On an isosurface the field is constant by definition** — that is what an
isosurface *is*. So "if no color field is given, color by the isosurface field
itself" is vacuous: it maps a constant through a colormap and produces one flat
color. The no-color-field case is therefore not a degenerate colormap but a
distinct mode:

- **color field supplied** — per-vertex, sampled and mapped through a colormap
- **no color field** — **per-component solid color**

Both converge at the GPU, since `Vertex` already carries per-vertex albedo and a
solid component simply writes one albedo to every vertex. One rendering path,
two authoring paths.

**Topology and coloring are decided independently:**

| | no color field | color field supplied |
|---|---|---|
| **signed field** | two components, two solid colors — the conventional blue/red phase picture | two components, both painted per-vertex; **phase information is lost** |
| **non-negative field** | one component, one solid color | one component, painted per-vertex — the classic ESP map |

Signedness decides *how many components*; color-field presence decides *how they
are colored*. Neither decision consults the other.

**Extraction runs at `+level` and `-level` unconditionally.** On a non-negative
field the negative pass finds no crossings and produces empty geometry, so the
one-component case falls out with no semantic knowledge and no branch. The only
reason to test anything is to skip wasted work: `value_range().min >= 0` short-
circuits the negative pass.

This is why there is no semantic tag: the property governing topology is the
sign of the data, which the data already states.

Two affordances follow:

- **The two phase colors must be swappable by one click** — not for taste, but
  because an orbital's global sign is *arbitrary* and re-running a calculation
  can flip which lobe is which. Flipping is how a user compares two orbitals or
  matches a published figure.
- **A color field on a signed surface destroys phase information.** The user's
  call, not something to block; the default for a signed field stays phase
  coloring. If both channels are wanted at once, distinguish components by
  *material* instead — `Vertex` carries roughness and metallic alongside albedo.

**Self-coloring does become meaningful** for a **slice plane** or **volume
rendering**, where the field varies across what is drawn. A slice plane is nearly
free here — only `sample` over a 2D grid — and would make a good debugging view
well before isosurface extraction lands.

**Open questions, and what answers them:**

| Question | Evidence needed |
|---|---|
| Does a typical 80^3 grid look acceptable at working zoom, or is it visibly faceted? | Render a real `homo.cube` isosurface in a debug view at 1x and 4x zoom |
| Are gradient-derived normals sufficient, or is mesh smoothing needed? | Same render, comparing face normals against `gradient` normals |
| Is two-pass back/front-face transparency good enough for concave lobes, or is a per-triangle sort required? | Render a d-type orbital, which is strongly concave, at alpha 0.5 |
| Is marching-cubes triangle count at cube resolution acceptable, or is decimation needed? | Triangle counts and frame times for an 80^3 extraction |
| What is a sensible default threshold with no semantic tag — a fixed constant, or a percentile-of-magnitude rule derived from `value_range` and the samples? | Sweep both against the conventional values (0.02 for amplitudes, 0.002 for densities) on real cubes and see which lands closer, more often |
| Is GPU raymarching worth building, or does the mesh path suffice? | Answered by the four rows above, collectively |

**The one genuinely open design question**, which is why this half cannot be
specified yet: **what type does an extracted isosurface output?** `Blueprint` is
`GeoNode`, an implicit CSG tree, and a marching-cubes mesh is not one. The
candidates are a new `GeoNodeKind::Mesh` variant (making orbital surfaces
composable with CSG, at the cost of teaching the SDF evaluator about meshes —
which may not be tractable, since a triangle soup has no cheap signed distance),
a display-only surface type outside the geometry system (simple, but not
composable), or a hybrid that carries the field itself as an implicit geometry
and extracts at display time. Choosing requires knowing whether users want to
intersect an orbital with a `cuboid` — a product question, not a technical one.

Related deferred items:

- multi-field coloring: the *mode* is settled above; what remains deferred is
  the node shape (an optional second field input) and the colormap UI —
  palette, range, and the fact that range must never auto-fit to the color
  field's extrema, for the nuclear-cusp reason in §Background. The pairings
  that motivate it: density at 0.002 colored by electrostatic potential (the
  standard "ESP map"), and reduced density gradient colored by
  `sign(lambda_2)*rho` (NCI analysis, which separates attraction from steric
  repulsion and is a physically grounded alternative to sphere-radius clash
  detection). Both are ordinary `ScalarField` pairs.
- automatic HOMO/LUMO selection — needs Molden metadata, so it follows Molden.
- localized orbitals (Boys, Pipek-Mezey) attached to bonds, which atomCAD
  already represents as first-class objects. Well beyond this document.

## Documentation touchpoints

Per `AGENTS.md`, in the same change as the code:

- `doc/reference_guide/nodes/atomic.md` — `import_cube` (P3)
- `doc/reference_guide/nodes/math_programming.md` — `sample_field` (P4)
- `doc/reference_guide/node_networks.md` — `ScalarField` in the pin-type list
  and its color (P2)
- `crates/atomcad-crystolecule/src/AGENTS.md` — `field/` module-map entry and
  the "coordinates crossing `ScalarField` are Ångström" invariant (P1)
- `doc/testing.md` — the asymmetric-ramp requirement for grid fixtures, and
  `scripts/make_cube_fixtures.py` as their source (P1)
