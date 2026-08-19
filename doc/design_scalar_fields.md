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

1. **A field's meaning is not recoverable from its numbers**, and it changes
   how the field must be presented — one surface or two, what a sensible
   threshold is, whether it forms surfaces or paints them. So the meaning is
   part of the value: see `FieldKind`.
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
             bounds / kind / metadata
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
| `ScalarField`, `FieldKind`, `FieldMetadata`, `GridGeometry`, `SampledField` | `atomcad-crystolecule/src/field/` | The cube loader must produce an `AtomicStructure` from the file's atom block, so ingestion cannot live below `crystolecule`. Volumetric data about a molecule belongs beside the molecule. |
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
/// analytically. Coordinates are real-space Ångström; values are in whatever
/// atomic unit the quantity uses (see `FieldKind`).
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
    /// normals. Default is a central difference stepped at
    /// `suggested_spacing() * 0.5`; analytic sources override with the exact
    /// derivative.
    fn gradient(&self, point: DVec3) -> DVec3 { /* central difference */ }

    /// Region outside which `sample` is defined to return `0.0`.
    /// `None` = defined everywhere (analytic sources).
    fn data_bounds(&self) -> Option<FieldBounds>;

    /// Box a consumer should sample when it has no better instruction.
    /// Sampled sources return their stored box; analytic sources derive one
    /// from atom positions plus a margin.
    fn suggested_bounds(&self) -> FieldBounds;

    /// Grid step a consumer should use absent better instruction, in Ångström.
    /// For a sampled source this is the stored spacing — sampling finer buys
    /// interpolation, not information.
    fn suggested_spacing(&self) -> f64;

    fn kind(&self) -> FieldKind;
    fn metadata(&self) -> &FieldMetadata;
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

**Why `suggested_spacing` exists at all.** It is the honest expression of the
asymmetry between the two sources: a sampled field has an intrinsic resolution
limit and an analytic one does not. A consumer that respects it will not waste
work over-sampling a coarse cube, and will not under-sample a Molden field.

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

### `FieldKind` — the semantic tag

```rust
/// What a field's numbers *mean*. Not derivable from the numbers themselves,
/// so it travels with the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    /// Signed wavefunction amplitude (a molecular orbital). Rendered as a
    /// pair of surfaces at +/-level in two colors.
    Amplitude,
    /// Non-negative electron density. One surface, one color.
    Density,
    /// Signed potential; diverges at nuclei. Used to color another field's
    /// surface, not to form one.
    Potential,
    /// Meaning unknown — the file did not say and the user has not.
    Unknown,
}
```

`FieldKind` is the reason the deferred visualization design can be written
without revisiting ingestion: surface *count*, default *threshold*, slider
*scale*, and whether a field forms or paints a surface are all functions of this
one enum.

Nothing in a `.cube` file states the kind. PySCF writes a descriptive comment
line, but that is free text, not a specified field. So:

- the loader **guesses** from the comment lines and the file stem (matching
  `orbital` / `mo` -> `Amplitude`, `density` / `rho` -> `Density`,
  `esp` / `mep` / `potential` -> `Potential`), falling back to `Unknown`
- the guess is a **default, always user-overridable** on the node
- a `Density` field whose samples contain a significantly negative value is a
  contradiction; the loader downgrades the guess to `Unknown` rather than
  propagating a kind the data refutes

### `FieldMetadata`

```rust
/// Provenance and identity for one field. Fields a `.cube` file cannot supply
/// are `None` today and populated by the Molden loader later — which is the
/// whole point of declaring them now.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldMetadata {
    /// Human-readable label for UI. From the cube comment line or file stem.
    pub label: Option<String>,
    /// Index within the source file (0-based). Distinct from `orbital_index`.
    pub field_index: usize,
    /// Orbital index as the *producing program* numbered it. `.cube`
    /// multi-field files carry this on their index line; single-field files
    /// do not.
    pub orbital_index: Option<i32>,
    /// Orbital energy, hartree. Molden only.
    pub energy: Option<f64>,
    /// Occupation number. Molden only. Enables automatic HOMO selection.
    pub occupancy: Option<f64>,
    /// `Alpha` / `Beta` for unrestricted calculations. Molden only.
    pub spin: Option<Spin>,
    /// Symmetry label, e.g. `1a1`. Molden only.
    pub symmetry: Option<String>,
}
```

Declaring the Molden-only fields now as `Option` is deliberate: it means adding
Molden changes a *loader*, not a struct signature that every consumer and UI
touches.

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
    kind: FieldKind,
    metadata: FieldMetadata,
}

/// Grid placement in real space. General enough for a sheared grid even
/// though PySCF only ever writes axis-aligned ones — the format permits it,
/// and the generality costs one matrix instead of three scalars.
#[derive(Debug, Clone, Copy)]
pub struct GridGeometry {
    /// Position of sample (0,0,0), Ångström.
    pub origin: DVec3,
    /// Step vectors along the three grid axes, Ångström.
    pub axes: [DVec3; 3],
    /// Sample counts along each axis.
    pub dims: [usize; 3],
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
  print a summary (kind, dims, label), never their samples.
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
| **Property** | `file_name: Option<String>`, plus a `FieldKind` override |

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

**Explicitly not included: a statistics readout in the node editor.** Min, max,
percentiles, and dimensions would help choose thresholds, but they are a
visualization concern, and `sample_field` plus the error messages specified
below already make the parser verifiable. Adding a stats panel now would be
building part of the deferred half on speculation. (Bounds *do* appear in
`sample_field`'s out-of-bounds error, which is where they are actually needed
for diagnosis.)

The editor widget follows
`lib/structure_designer/node_data/import_xyz_editor.dart` (file picker plus a
`FieldKind` dropdown), registered in `node_data/node_data_widget.dart`. The
picker uses the remembered import directory, per the existing last-directories
behavior.

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
 Comment line 1                                <- free text; kind heuristic reads this
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

Molden files store the wavefunction **analytically** — a Gaussian basis set plus
the MO coefficient matrix — rather than as samples. That buys resolution
independence (no fixed grid, so no chunky lobes when zoomed in, and a
GPU-evaluated field becomes possible), all orbitals in one ~100 KB file instead
of ~6 MB per orbital, and per-orbital energy, occupancy, spin and symmetry
metadata that `.cube` throws away — which is what makes automatic HOMO/LUMO
selection possible.

It costs implementing contracted-Gaussian evaluation, whose traps are well known
and mostly silent: Cartesian versus spherical shells (signalled by the presence
or absence of `[5D]` / `[7F]` / `[9G]` sections), component ordering within a
shell (which differs per producing program, and where a wrong order yields a
plausible but wrong field), normalization conventions (a wrong assumption
rescales the field, so the shape looks perfect and the threshold becomes
meaningless), and per-program format variation.

**This is why the ordering matters.** With `.cube` support already in place,
exporting both formats from one PySCF calculation gives a half-million-point
ground truth from a trusted implementation for free:

```python
cubegen.orbital(mol, 'ref.cube', mf.mo_coeff[:, i])
molden.from_scf(mf, 'ref.molden')
```

Sample the Molden evaluator at the cube file's exact grid points and compare.
Every ordering, normalization and sign bug surfaces as a numerical mismatch
instead of as a subtly wrong picture months later.

### What Molden support will and will not touch

The point of this section is to make the claim checkable rather than
aspirational.

**Unchanged:** `ScalarField`, `FieldKind`, `FieldMetadata`, `FieldBounds`,
`DataType::ScalarField`, `NetworkResult::ScalarField`, `sample_field`, the pin
color, and every deferred visualization consumer.

**Added:** `io/molden_loader.rs`; an `AnalyticField` implementing the trait
(overriding `sample_batch` for per-shell setup reuse and `gradient` with the
exact derivative, returning `None` from `data_bounds`, and deriving
`suggested_bounds` from atom positions plus a ~1.6 Å margin — the equivalent of
`cubegen`'s default 3 Bohr); an `import_molden` node emitting the same three
pins as `import_cube`; and populated `energy` / `occupancy` / `spin` /
`symmetry` metadata.

**The one asymmetry the contract already absorbs:** an analytic field is
unbounded, so `data_bounds` returns `None` while `suggested_bounds` returns a
derived box. Every consumer takes an explicit box with "auto from the field" as
its default, so both sources look identical from the outside and Molden's extra
freedom surfaces as a resolution control rather than a special case.

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

- `crystolecule/src/field/mod.rs`: `ScalarField`, `FieldBounds`, `FieldKind`,
  `FieldMetadata`, `Spin`, `GridGeometry`, `SampledField`
- `crystolecule/src/io/cube_loader.rs`: `load_cube` producing `CubeFile`,
  single-field path plus the units heuristic and the kind guess
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
- `import_cube_editor.dart` (file picker, `FieldKind` override) and its
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
| Synthetic 2p_z | sign changes across the nodal plane; `Amplitude` handled signed |
| Atom block vs reference `.xyz` | element, count and position agreement |
| Bohr fixture | positions convert to Ångström; bond lengths chemically sane |
| Ångström-scaled fixture | units heuristic fires and sets `units_warning` |
| Single-atom fixture | heuristic stays silent, no spurious warning |
| Truncated / non-numeric / zero-dim fixtures | descriptive error, no panic |
| Negative sample in a `Density`-guessed file | kind downgraded to `Unknown` |
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

**Open questions, and what answers them:**

| Question | Evidence needed |
|---|---|
| Does a typical 80^3 grid look acceptable at working zoom, or is it visibly faceted? | Render a real `homo.cube` isosurface in a debug view at 1x and 4x zoom |
| Are gradient-derived normals sufficient, or is mesh smoothing needed? | Same render, comparing face normals against `gradient` normals |
| Is two-pass back/front-face transparency good enough for concave lobes, or is a per-triangle sort required? | Render a d-type orbital, which is strongly concave, at alpha 0.5 |
| Is marching-cubes triangle count at cube resolution acceptable, or is decimation needed? | Triangle counts and frame times for an 80^3 extraction |
| What is a sensible default threshold per `FieldKind`, and what slider scale? | Sweep thresholds against the conventional values on real data |
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

- multi-field coloring: sampling a `Potential` field onto a `Density` surface.
  The contract supports it (per-vertex albedo already exists); the node shape
  and colormap UI do not.
- automatic HOMO/LUMO selection — needs Molden metadata, so it follows Molden.
- localized orbitals attached to bonds. Canonical MOs are delocalized and often
  unrecognizable to a chemist; a localizing transform (Boys, Pipek-Mezey) yields
  orbitals that sit on individual bonds, which atomCAD already represents as
  first-class objects. "Show the orbital on this bond" is a CAD-native framing
  no general-purpose chemistry viewer offers. Well beyond this document,
  recorded because it argues for keeping `FieldMetadata` extensible.

## Documentation touchpoints

Per `AGENTS.md`, in the same change as the code:

- `doc/reference_guide/nodes/atomic.md` — `import_cube` (P3)
- `doc/reference_guide/nodes/math_programming.md` — `sample_field` (P4)
- `doc/reference_guide/node_networks.md` — `ScalarField` in the pin-type list
  and its color (P2)
- `crates/atomcad-crystolecule/src/AGENTS.md` — `field/` module-map entry and
  the "coordinates crossing `ScalarField` are Ångström" invariant (P1)
- `doc/testing.md` — the asymmetric-ramp requirement for grid fixtures (P1)
