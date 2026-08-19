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
- `.cube` file parsing, including the multi-field variant
- `DataType` / `NetworkResult` plumbing for a new value kind
- the `import_cube` node
- the `sample_field` node — a probe that makes ingestion verifiable in the app
  before any rendering exists
- an implementation plan and test strategy covering the above

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

Three consequences drive the design:

1. **A field's meaning is not recoverable from its numbers** — but the two
   properties that actually drive rendering are: whether it is **signed**
   (extract at `±level`, two colors) and its **magnitude scale**. Both follow
   from the data's value range, so no semantic tag is needed. See §Why there
   is no `FieldKind`.
2. **All three quantities are dominated by sharp peaks at the nuclei**, with
   all the interesting structure in the low-magnitude tail. Electron density
   spans roughly six orders of magnitude between the conventional surface
   threshold (0.002) and its value at a carbon nucleus (order 100). Any
   auto-ranging over a field's min/max produces a useless picture of a few
   bright dots. Defaults must come from the semantics, never the extrema.
3. **Field magnitudes are in atomic units** and vary by quantity, so a single
   global default threshold cannot exist.

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
             bounds / native_grid / value_range
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
/// `Send + Sync` is required: consumers evaluate in parallel batches, mirroring
/// `atomcad_geo_tree::BatchedImplicitEvaluator`.
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
    /// This is what replaces a semantic type tag (§Why there is no
    /// `FieldKind`). `min >= 0` means the field is non-negative, so a consumer
    /// can skip the negative-level extraction; the span sets a log slider's
    /// bounds. Both are *derived*, so they work for any scalar quantity
    /// chemistry produces, not just the three or four anyone thought to name.
    fn value_range(&self) -> Option<(f64, f64)>;
}
```

**The out-of-bounds rule is `0.0`, not an error.** A finite cube box is a
*window* onto a field that decays to zero; `0.0` is the physically correct
answer just outside it, and it keeps every consumer free of an error path in
its innermost loop. Note this is a property of the *trait*; the `sample_field`
node layers a stricter, more diagnostic behavior on top — see below.

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

Stating this matters because both conventions exist in the wild, and picking one
silently is how a half-voxel offset creeps in. It also makes the fencepost work
out: a consumer walking from `min` to `max` **inclusive** at the native spacing
generates exactly `nx` points landing precisely on the stored values. A consumer
that iterates exclusively of `max` silently drops the last plane — which is
another reason to prefer `native_grid` verbatim over reconstructing a lattice
from a box and a step.

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

```rust
/// Axis-aligned box in real-space Ångström.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldBounds {
    pub min: DVec3,
    pub max: DVec3,
}
```

If a general AABB is later wanted elsewhere it should move to `atomcad-util`;
it is defined here for now to avoid speculatively widening a shared crate.

### Why there is no `FieldKind`

An earlier draft carried a semantic tag — `Amplitude` / `Density` /
`Potential` / `Unknown` — guessed from the cube comment line and overridable on
the node. It is deliberately **not** in this design. Recorded here because its
absence is the kind of thing a later contributor will want to "fix".

It was claimed to drive four things. Three evaporate under inspection:

- **Surface count.** Extract at `+level` and `-level` unconditionally. On a
  non-negative field the negative pass finds no crossings and yields empty
  geometry, so the one-surface case degenerates on its own, with no semantic
  knowledge. (An earlier draft worried about speckle from negative noise in
  density data. That was wrong: a density is computed as a sum of squares, so
  it is non-negative to roundoff, and trilinear interpolation of non-negative
  values is a convex combination and stays non-negative.)
- **Slider scale.** The level is `±L` with `L > 0`, so the slider spans
  positive values only and log scale is right for every quantity.
- **Forms-vs-paints role.** Never enforced, only advisory — and an ESP
  isosurface is a real object (an equipotential surface), just an unusual one.

The fourth, **default threshold**, is real but is not solved by a tag either:
the interesting levels span orders of magnitude, and the honest answers are a
fixed constant plus a log slider, or a percentile-of-magnitude rule. Both are
data-driven, and *which* is right is an explicitly deferred question
(§Deferred). A tag in the ingestion contract serving an unvalidated
visualization decision is precisely the speculation §Scope forbids.

The decisive objection is generality. Chemistry produces electron density, MO
amplitude, ESP, spin density, deformation density, ELF, reduced density
gradient, ALIE, Fukui functions, the Laplacian of the density, and more. A
four-variant enum cannot name that space — everything interesting lands in
`Unknown`, or the enum grows forever. Meanwhile the two properties that
actually drive rendering, **signedness** and **magnitude scale**, are derivable
from the data for all of them. Hence `value_range` on the trait.

What is genuinely lost is a machine-readable label, and nothing downstream needs
to branch on one.

### Why there is no `FieldMetadata` either

An earlier draft carried a `FieldMetadata` struct — `label`, `field_index`,
`orbital_index`, `energy`, `occupancy`, `spin`, `symmetry` — with the
Molden-only members declared as `Option` up front so that adding Molden would
change only a loader. Removed for the same reason as `FieldKind`, and recorded
here for the same reason.

Every member fails the test *"is there a known use, and is it not easily
determined otherwise?"*:

| Member | Verdict |
|---|---|
| `energy`, `occupancy`, `spin`, `symmetry` | **Permanently `None` for this document's entire scope.** No loader here can populate them; only Molden could, and Molden is not being built. Pure dead weight. |
| `field_index` | Trivially derivable — it *is* the index into the containing `Vec` / `Array[ScalarField]`. Duplicating a container's own indexing inside its elements only creates a value that can go stale. |
| `orbital_index` | Exists only in the multi-field cube variant, which is P5: provisional, and untestable until a Gaussian-produced file is available. It belongs to that feature, not to the base contract. |
| `label` | Populated, but with no in-scope consumer. See below. |

**Where field identity actually comes from in scope: the node, not the value.**
PySCF writes one field per file, so a network importing `homo.cube` and
`lumo.cube` has two `import_cube` nodes, each already labelled by its file name
in its subtitle. The node disambiguates; the value does not need to. Per-field
identity only starts earning its place when one node emits many fields — which
is exactly P5 and Molden, where whatever identity those need can be added
shaped by what they actually need rather than guessed now.

So the `Spin` enum and the `metadata()` trait method go too. The cube comment
lines are parsed and discarded; surfacing them on `CubeFile` is a one-line
addition whenever a use appears.

The counter-argument the earlier draft made — that declaring the Molden members
now means Molden changes a loader rather than a signature — does not hold up.
There are exactly two implementors of `ScalarField`, and the Molden work edits
both anyway.

### `SampledField` — the `.cube` implementation

```rust
/// A field stored as samples on a regular grid, with trilinear interpolation
/// between them.
#[derive(Debug, Clone)]
pub struct SampledField {
    grid: GridGeometry,
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

Touchpoints for adding it, all in existing per-variant tables:

| Site | Change |
|---|---|
| `data_type.rs` `enum DataType` | new variant |
| `data_type.rs` `impl fmt::Display` (~line 332) | `=> write!(f, "ScalarField")` |
| `data_type.rs` parser keyword table (~line 1430) | `"ScalarField" => Ok(DataType::ScalarField)` |
| `lib/structure_designer/node_network/node_network.dart` (~line 196) | pin color entry |

`canonicalize_data_type` needs no change — the variant has no nested type.

Pin color: the existing map is organized in families (indigo/blue matrices,
purple geometry, green phases, teal crystal, amber functions, grey `Unit`). A
soft red such as `0xFFE57373` is free and reads as a distinct family for
"sampled volumetric data". Worth eyeballing against the live palette before
committing.

### `NetworkResult::ScalarField`

```rust
/// A scalar field value. `Arc` because `NetworkResult` is cloned freely
/// throughout evaluation and the payload is megabytes; every other large
/// variant predates this concern and clones deeply.
ScalarField(Arc<dyn ScalarField>),
```

**`Arc<dyn ScalarField>`, deliberately, on both counts.** `dyn` is what lets one
variant carry both a sampled and an analytic field, so adding Molden adds no
variant and touches no `match`. `Arc` is what keeps `NetworkResult::clone`
cheap: the evaluator clones results freely, and a deep clone of a multi-megabyte
grid on every wire traversal would be a real performance defect. This makes
`ScalarField` the first `Arc`-shared payload in `NetworkResult`; that is a
justified departure rather than an inconsistency, and worth a comment at the
variant saying so.

Consequences to handle:

- `NetworkResult` derives `PartialEq` and `Default`. `dyn ScalarField` supports
  neither. Compare by `Arc::ptr_eq` — fields are immutable once loaded, so
  pointer identity is a sound conservative equality (two independently loaded
  copies of one file comparing unequal is acceptable; nothing depends on
  structural field equality).
- `NetworkResult` is `Debug`; the trait requires `Debug`, and implementations
  print a summary (dims and value range), never their samples.
- `NetworkResult` is never serde-serialized — results are recomputed from the
  network — so there is **no `.cnnd` surface** and no migration.

## Node: `import_cube`

| | |
|---|---|
| **Name** | `import_cube` |
| **Category** | `AtomicStructure` — beside `import_xyz` / `import_cif` |
| **Input pin 0** | `file_name: String` — optional; wired value overrides the stored property, matching `import_xyz` |
| **Input pin 1** | `index: Int` — optional, default `0`; selects which field within a multi-field file |
| **Output pin 0** | `field: ScalarField` — the field selected by `index` |
| **Output pin 1** | `fields: Array[ScalarField]` — every field in the file |
| **Output pin 2** | `molecule: Molecule` — atoms from the file's atom block |
| **Property** | `file_name: Option<String>` |

Node data follows `ImportXYZData` (`nodes/import_xyz.rs`) closely: `file_name`
persisted, parsed payload `#[serde(skip)]`, a `node_data_loader` that reloads
after deserialization, and a `node_data_saver` that relativizes the path via
`try_make_relative` so projects stay portable. This keeps megabytes of samples
out of the `.cnnd` file, exactly as `AtomicStructure` is kept out today.

**Why three output pins, with pin 0 redundant against pin 1.** Pin 0 serves the
overwhelmingly common case — PySCF writes one field per file — with no
`array_at` node in the graph. Pin 1 serves the multi-field case and, later, a
Molden file's full orbital set, where mapping an isosurface over every field to
build a gallery is the natural network. Both are `Arc` clones, so the redundancy
costs nothing at runtime. Pin 2 is what makes ingestion verifiable before any
field rendering exists.

**Why an `index` input pin when cube files almost always hold one field.** It is
the pin that lets a Molden file be useful without a graph change. For `.cube`
with one field it is inert. Selecting by *position* is admittedly the wrong
affordance for Molden — "the HOMO" is what a user wants — but that selection
needs `occupancy`, which only Molden supplies; a metadata-driven selector is
future work, and `index` remains the primitive underneath it.

Out-of-range `index` is an evaluation error naming the valid range.

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
| **Output** | `Float` |

Evaluates the field at one point. Small, pure, and the thing that makes the
whole ingestion half testable inside the running application: wire a `vec3` into
it, wire the result into `print`, and read values off the console.

**Out-of-bounds is an error here, unlike in the trait**, and the message names
the field's bounds:

```
point (5.20, 0.00, 0.00) is outside field bounds
  (-2.65, -2.65, -2.65) .. (2.65, 2.65, 2.65)
```

The reasoning is diagnostic. The most likely ingestion bug is a units mistake —
missing the Bohr-to-Ångström conversion scales every coordinate by 1.89. Under
the trait's permissive rule that presents as `0.0` everywhere, which says
"something is wrong" but not what. An error that prints the bounds says it
immediately, and delivers the one piece of statistics that matters for
diagnosis on demand rather than as permanent UI. Consumers that legitimately
sample outside a box (the future isosurface node, whose sampling box may exceed
the data box) use the trait directly and get `0.0`.

Beyond debugging, this node has standalone value: sampling an orbital at a
specific point, or along a bond via `map` over a generated point array, is a
genuine analysis capability and works through the headless CLI.

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
**invisible in any axis-symmetric test case**. See the test strategy.

**6. Units.** Coordinates and step vectors are Bohr in everything PySCF and
Gaussian write. A convention exists whereby a negative voxel count signals
Ångström, but it is documented inconsistently across sources — some describe
negative as Bohr — so **do not rely on either reading**.

Detect instead, from data already in the file:

- default to Bohr for positive voxel counts (what every real producer writes)
- compute nearest-neighbour distances across the atom block and compare against
  covalent radii, which `crystolecule` already has. A C–C single bond is 1.54 Å
  or 2.91 Bohr — a factor of 1.89 apart, with no plausible ambiguity.
- if the distances contradict the assumed units, **warn loudly** and use the
  detected units. The loader itself only records `units_warning`; the
  `import_cube` node surfaces it as a non-blocking `NodeDataError::warning` from
  `get_data_error`, which is correct by the blocking litmus in
  `doc/design_error_management.md` — the node still produces a usable field.

A single-atom file has no distances to check; accept the default silently.

**7. Malformed input yields a descriptive error, never a panic.** Truncated
value blocks, non-numeric tokens, zero dimensions, and non-finite samples are
all rejected with the byte or token offset. The loader signature mirrors
`load_xyz`: `Result<CubeFile, io::Error>`.

### Loader output

```rust
pub struct CubeFile {
    pub atoms: AtomicStructure,
    pub fields: Vec<SampledField>,   // one per field; shares `grid`
    /// Set when the units heuristic contradicted the assumed units.
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
metadata (§Why there is no `FieldMetadata` either), and `Wavefunction` is the
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
`Array[ScalarField]` of orbitals plus `Molecule`, matching `import_cube`. That
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

With `.cube` support already in place, exporting both formats from one PySCF
calculation gives a half-million-point ground truth from a trusted
implementation, for free:

```python
cubegen.orbital(mol, 'ref.cube', mf.mo_coeff[:, i])
molden.from_scf(mf, 'ref.molden')
```

Sample the Molden evaluator at the cube file's exact grid points and compare
(agreement to ~1e-6 relative). Every ordering, normalization and sign bug
surfaces immediately as a numerical mismatch instead of as a subtly wrong
picture months later. This oracle exists *only because* the cube path was built
first.

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

### P1 — Field abstraction and cube loader (backend only)

- `crystolecule/src/field/mod.rs`: `ScalarField`, `FieldBounds`,
  `GridGeometry`, `SampledField`
- `crystolecule/src/io/cube_loader.rs`: `load_cube` producing `CubeFile`,
  single-field path plus the units heuristic; `value_range` accumulated in the
  same pass that reads the samples
- unit tests per the test strategy below; no Flutter, no node, no API

Deliverable: a parser with numerically verified axis ordering and unit handling.
Nothing user-visible.

### P2 — `DataType` and `NetworkResult` plumbing

- `DataType::ScalarField` and its four touchpoints
- `NetworkResult::ScalarField(Arc<dyn ScalarField>)`, with `PartialEq` by
  `Arc::ptr_eq` and a summary `Debug`
- Dart pin color

Deliverable: a wireable type carrying no values yet. Verified by a type
round-trip test through `from_string` / `Display` and by the existing
registry-validation suite staying green.

### P3 — `import_cube` node and editor

- node data, `eval`, loader/saver following `ImportXYZData`
- registration in `nodes/mod.rs` and `node_type_registry.rs`
- `import_cube_editor.dart` (file picker only) and its
  `node_data_widget.dart` entry
- reference guide: `doc/reference_guide/nodes/atomic.md`

Deliverable: **the first user-visible milestone.** Import a `.cube` and see the
molecule render through the existing impostor path off the `molecule` pin. This
validates the atom block, the Bohr-to-Ångström conversion, and path
relativization with zero new rendering code.

### P4 — `sample_field` node

- node, registration, reference guide entry in
  `doc/reference_guide/nodes/math_programming.md`
- integration test: load a fixture cube, sample at points with known values,
  assert; assert the out-of-bounds error text names the bounds

Deliverable: ingestion is fully verifiable in the running application and from
the headless CLI. **This closes the scope of this document.**

### P5 — Multi-field cube support (provisional)

Deliberately last, and separable: it cannot be validated without a
Gaussian-produced multi-field file. Adds the negative-`natoms` branch, the index
line, de-interleaving, and the `fields` / `index` pins.

If no such file is available when P4 lands, P5 waits rather than shipping
untested parsing. The single-field path must reject a negative `natoms` with a
clear "multi-field cube files are not yet supported" error rather than
misparsing it.

## Testing strategy

Tests live in `crates/atomcad-crystolecule/tests/crystolecule/` for the loader
and field, and under the structure-designer crate's `tests/` for the nodes, per
the "tests go in the owning crate's `tests/`" rule.

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

Other cases:

| Test | Asserts |
|---|---|
| Ramp field, 3x4x5 | axis order, no mirroring, no transposition |
| Trilinear midpoint | interpolation between known samples |
| Synthetic 2p_z | sign changes across the nodal plane; negative values preserved, not clamped |
| Atom block vs reference `.xyz` | element, count and position agreement |
| Bohr fixture | positions convert to Ångström; bond lengths chemically sane |
| Ångström-scaled fixture | units heuristic fires and sets `units_warning` |
| Single-atom fixture | heuristic stays silent, no spurious warning |
| Truncated / non-numeric / zero-dim fixtures | descriptive error, no panic |
| `value_range` on every fixture | matches the min/max of the fixture's literal values |
| Out-of-bounds `sample` | exactly `0.0` |
| `gradient` on an analytic fixture | central difference matches the known derivative within tolerance |
| `sample_field` out of bounds | error text contains the bounds |
| Negative `natoms` before P5 | clear unsupported-variant error |

Fixtures: small hand-written `.cube` files under `rust/tests/fixtures/`,
addressed through `atomcad_test_support::fixture_path`. Keep them tiny (a 3x4x5
grid is 60 values) so they are readable and diffable in review. Generating them
with a short numpy script is fine — numpy is available on the maintainer's
machine — but the committed fixtures are the hand-checkable artifact.

A real PySCF-generated cube is worth committing once as a smoke fixture (one
water HOMO at low resolution), but the correctness tests must rest on fixtures
whose expected values are derivable by hand.

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

This is why there is no `FieldKind` (§Why there is no `FieldKind`) — the
property that governs topology is the sign of the data, which the data already
states.

**Two affordances to build in:**

- **The two phase colors must be swappable by one click.** Blue/red is common but
  not universal, and more importantly the global sign of an orbital is
  *arbitrary* — re-running a calculation can flip which lobe is which. Flipping
  is how a user makes two orbitals comparable side by side, or matches a
  published figure. Trivial to implement, disproportionately useful.
- **A color field on a signed surface destroys phase information**, and that is
  the user's call rather than something to block. The default for a signed
  field stays phase coloring. If both channels are wanted at once, the escape is
  to distinguish components by *material* rather than color — the negative lobe
  rendered more matte — since `Vertex` carries roughness and metallic alongside
  albedo.

**Where self-coloring does become meaningful:** a **slice plane** or **direct
volume rendering**, where the field genuinely varies across what is drawn. There
"color by the field itself" is the natural default. A slice plane is nearly free
against this contract — it needs only `sample` over a 2D grid — and is worth
considering as a debugging view well before isosurface extraction lands.

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
- `doc/testing.md` — the asymmetric-ramp requirement for grid fixtures (P1)
