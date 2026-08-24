# Design: the `isosurface` node — displaying scalar fields

## Motivation

`doc/design_scalar_fields.md` builds the ingestion half of scalar field
support: the `ScalarField` contract, the `.cube` loader, `DataType::ScalarField`,
and the `import_cube` / `sample_field` nodes. It deliberately defers the
visualization half, and says why:

> The visualization design turns on questions that **only real sample data can
> answer** [...] Designing those now would be designing on speculation.

This document is that deferred half. It designs the node that turns a
`ScalarField` into something you can look at: an **isosurface** — the surface
where the field equals some threshold — optionally painted with a second
field's values.

It assumes P1–P4 of `design_scalar_fields.md` have landed, i.e. that a
`ScalarField` can be imported and sampled. It does **not** assume P5
(multi-field cubes) or any Molden support; both remain additive.

## Scope of this document

**Designed in detail here:**

- where in the pipeline the mesh is extracted, and why (§Where the extraction
  happens)
- the `Isosurface` value type and `IsosurfaceData` payload
- the `isosurface` node — pins, properties, editor
- `DataType` / `NetworkResult` plumbing, with the full touchpoint list
- the extraction algorithm choice, and why not dual contouring or surface nets
- marching-cubes extraction: ambiguity handling, resolution policy, normals,
  winding, components
- transparent rendering: the pipelines, the blend mode, and what is *not*
  correct about the cheap approach
- a phased implementation plan, each phase carrying its own tests and, where
  there is UI, its own manual walkthrough

**Deliberately deferred:**

- GPU volume raymarching (§Rendering: what is not built)
- slice planes and true volume rendering
- automatic HOMO/LUMO selection (needs Molden metadata)
- making an isosurface composable with CSG (§Not a `GeoNodeKind`)

## Relationship to the sibling document

`design_scalar_fields.md` lists six open questions in its deferred section.
This document answers the structural ones and leaves the empirical ones to be
settled by looking at real data, with the apparatus to do the looking built in
(§The comparison modes, and their expiry condition).

| Open question from the sibling doc | Status here |
|---|---|
| What type does an extracted isosurface output? | **Answered** — a lazy `Isosurface`, not a mesh (§Where the extraction happens) |
| Is two-pass back/front-face transparency good enough? | **Apparatus built**, answer deferred to evidence |
| Are gradient-derived normals sufficient? | Gradient normals specified; smoothing left to evidence |
| Does an 80^3 grid look acceptable / is decimation needed? | Deferred to evidence; resolution is a preference so it can be swept |
| Sensible default threshold with no semantic tag? | Fixed constant for now; the percentile rule stays deferred (§The `level` default) |
| Is GPU raymarching worth building? | Deferred; the value type is chosen so it stays possible (§Rendering: what is not built) |

Two claims in the sibling document need refinement, recorded here so they are
not taken at face value:

1. It says the cheap transparency approach works for "closed blobs". It works
   for **convex** closed blobs. Orbitals are routinely 4–8 disconnected lobes,
   which is precisely the case it gets wrong. §Rendering explains the failure
   and the fix.
2. It says the mesh path "adds no shader, no bind group and no renderer
   surgery". That survives — but only because `ModelUniform` already exists and
   can carry alpha. It would *not* have survived a per-vertex alpha channel.
   §The shader delta records the actual cost.

## Where the extraction happens

This is the load-bearing decision, and the sibling document flags it as "the
one genuinely open design question". It is settled by looking at what the
existing geometry pipeline actually does.

### What the `Blueprint` precedent actually is

A common summary is "the value is an SDF and the display code tessellates it."
That is directionally right, but there are **three** stages, not two, and the
boundary between them is what matters:

| Stage | Type | Where | When |
|---|---|---|---|
| Network value | `NetworkResult::Blueprint(BlueprintData)` → `geo_tree_root: GeoNode` | `atomcad-structure-designer` | every wire traversal; resolution-free |
| Scene output | `NodeOutput::PolyMesh` / `SurfacePointCloud` | `network_evaluator.rs` `generate_explicit_mesh_output`, calling `atomcad_display::csg_to_poly_mesh` | **only for displayed nodes**, at scene generation |
| GPU mesh | `Mesh` / `LineMesh` | `scene_tessellator.rs` → `poly_mesh_tessellator` | per tessellation |

Three properties of that arrangement decide this design:

**The stage-1→2 conversion is driven entirely by preferences.**
`generate_explicit_mesh_output` branches on
`GeometryVisualization::{SurfaceSplatting, ExplicitMesh}` and reads
`samples_per_unit_cell`, `sharpness_angle_threshold_degree`,
`wireframe_geometry` and `mesh_smoothing` from
`crates/atomcad-structure-designer/src/preferences.rs`. The *same* Blueprint
value renders as a point cloud or as a polygon mesh, at a user-chosen
resolution, **with no re-evaluation**.

**It has its own cache.** `self.csg_conversion_cache` on the evaluator is
separate from the eval memoization. The expensive derived form is cached where
it is produced, not in the value graph.

**Semantic parameters live in the value; quality parameters live in
preferences.** A `sphere` node's `radius` is not a display concern — it goes
into the value as `GeoNodeKind::Sphere { radius }`, and display tessellates.
Isolevel is a semantic parameter. Extraction resolution is a quality parameter.

### The decision

**The node outputs a lazy surface specification. Marching cubes runs in the
stage-1→2 conversion, in `atomcad-display`.**

```
   isosurface node
        |
        v
  NetworkResult::Isosurface(IsosurfaceData)     <- the network value
  { field, level, coloring, alpha }                resolution-free
        |
        |  atomcad_display::isosurface::extract
        |  (marching cubes; resolution from preferences;
        |   cached like csg_conversion_cache)
        v
  NodeOutput::Isosurface(SurfaceMesh)           <- the scene output
  positions + normals + per-vertex albedo          resolution fixed
  + component ranges
        |
        |  scene_tessellator
        v
  renderer Mesh + component ranges              <- GPU
```

### Why not output a mesh

The alternative — run marching cubes in `eval()` and carry an explicit mesh on
the wire — loses on four counts, in descending order of how much they matter:

**The isolevel slider.** This is the most-used control in any orbital viewer;
chemists scrub it constantly. With a mesh value, every tick re-runs marching
cubes *inside the evaluator*, dirtying the node and invalidating downstream
memoization. The memoization work took evaluation from 6.63 s to 0.68 s;
putting a scrubbable multi-megabyte computation into the eval graph works
directly against it. With a lazy value the node's `eval` is nearly free — it
packages a field reference and a number — and the expensive part sits behind its
own cache in the display conversion, exactly where `csg_conversion_cache`
already sits.

**The implicit form is strictly better for downstream computation.** This runs
opposite to intuition and is the argument that settles it. If a later node asks
"which atoms lie inside the density envelope", the field answers exactly with
`sample(p) > level`. A triangle soup answers it badly or not at all — the
sibling document already notes that "a triangle soup has no cheap signed
distance". Outputting a mesh throws away the better representation and keeps
the derived one.

**Resolution becomes uneditable without re-evaluation.** Every existing
geometry-quality knob is a preference that costs nothing to change. A mesh
value would make extraction resolution the only quality setting in the
application that invalidates the eval cache.

**Clone cost.** `NetworkResult` is cloned freely on every wire traversal. An
80^3 extraction is routinely 100k+ triangles. The sibling document already went
to `Arc<dyn ScalarField>` explicitly to keep clones cheap; a mesh value would
need the same treatment for a payload that is pure derived data.

The honest advantage of a mesh value is that it would give the project a
general `Mesh` type, useful later for STL import, mesh export, and mesh
booleans. That is real, but it is a separate feature with its own
justification, and by the sibling document's own *known use, not easily
determined otherwise* test it should not be built as a side effect of an
orbital viewer. If a `Mesh` type is wanted later it is additive and does not
invalidate anything here.

### Not a `GeoNodeKind`

An `Isosurface` variant on `GeoNodeKind` would make orbital surfaces composable
with CSG for free, which is tempting. Do not do it, for a reason more concrete
than the sibling document's:

`GeoNode` has **two** backends. The implicit path samples values; the exact
path (`to_csg_mesh_cached`) builds csgrs BSP trees from polygonal primitives. A
scalar field has no polygonal primitive, so it cannot participate in the second
one at all — a `GeoNodeKind::Isosurface` would make `to_csg_mesh` partial for
any tree containing it. On top of that, union-as-`min` stays *sign*-correct for
any implicit function but stops being distance-correct, which breaks anything
reading value magnitudes.

If CSG composability is ever wanted, the tractable bridge is a `GeoNodeKind`
that wraps a field *and* forces the whole tree onto the implicit path, with a
validation rule rejecting it in exact-path contexts. That is a real design, not
a variant addition, and it is out of scope here.

## The `Isosurface` value

### `IsosurfaceData`

```rust
/// A surface to be extracted from a scalar field at display time. Carries the
/// *specification*, never the mesh: resolution is a display-side quality
/// decision (§Where the extraction happens), so the same value renders coarse
/// or fine with no re-evaluation.
#[derive(Debug, Clone)]
pub struct IsosurfaceData {
    pub field: Arc<dyn ScalarField>,
    /// Level **magnitude**, strictly positive. Extraction runs at `+level` and
    /// `-level`; on a non-negative field the negative pass finds no crossings
    /// (§The two sign passes).
    pub level: f64,
    pub coloring: IsosurfaceColoring,
    /// 0..=1. Exactly `1.0` takes the opaque path and skips the transparency
    /// pipelines entirely (§The opaque fast path).
    pub alpha: f32,
}

/// How the extracted surface is painted. The two arms are the two authoring
/// modes settled in `design_scalar_fields.md`; making them an enum means "a
/// color field on a signed surface destroys phase information" is a
/// type-level fact rather than a rule someone has to remember.
#[derive(Debug, Clone)]
pub enum IsosurfaceColoring {
    /// No color field — one solid color per sign.
    Phase { positive: Vec3, negative: Vec3 },
    /// Color field supplied — per-vertex colormap over `range`.
    Field {
        field: Arc<dyn ScalarField>,
        /// Colormap domain. **Never** auto-fitted to the color field's
        /// extrema — see §Why the color range is not auto-fitted.
        range: (f64, f64),
        colormap: Colormap,
    },
}

/// Diverging blue-white-red, the conventional electrostatic-potential map.
/// One variant on purpose (§Why only one colormap).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Colormap {
    #[default]
    BlueWhiteRed,
}
```

Carried as `NetworkResult::Isosurface(IsosurfaceData)`.

**The enum encodes the settled 2×2 by construction.** `design_scalar_fields.md`
establishes that signedness decides *how many components* and color-field
presence decides *how they are colored*, and that neither decision consults the
other. Here the discriminant is color-field presence; signedness is read from
`field.value_range()` at extraction time. The two never meet.

**Inline, not `Arc<IsosurfaceData>`** — contrary to what the
`Arc<dyn ScalarField>` precedent might suggest. The struct is two `Arc`s plus
scalars, so `clone` is already two refcount bumps and a small memcpy.
`BlueprintData` is inline for the same reason. Wrapping it would add an
indirection for nothing.

### Crate placement

`IsosurfaceData`, `IsosurfaceColoring`, `Colormap`, `SurfaceMesh` and the
extractor all live in **`atomcad-display`**, in a new `src/isosurface/` module.

| Consideration | Resolution |
|---|---|
| Is it reachable from `NetworkResult`? | Yes — `atomcad-structure-designer` already depends on `atomcad-display` (`network_evaluator.rs` imports `csg_to_poly_mesh`). |
| Can it hold a `ScalarField`? | Yes — `atomcad-display` already depends on `atomcad-crystolecule` (it imports `DrawingPlane`). |
| Precedent for a display type in a structure-designer enum? | Yes — `NodeOutput::PolyMesh(PolyMesh)` and `SurfacePointCloud` are both display types. |
| Back-edge risk? | None. Nothing in `atomcad-display` learns about nodes, networks or `NetworkResult`. |

The counterargument is symmetry with `BlueprintData`, which lives in
`atomcad-structure-designer`. But `BlueprintData` carries domain data —
alignment, structure registration — whereas `IsosurfaceData` carries a level
and some paint. It is a presentation specification, so it belongs beside the
code that consumes it.

`isosurface/` is a new subsystem in `atomcad-display`, which has no `AGENTS.md`
today. This change adds one, and registers it in the root `AGENTS.md`
"Subdirectory Instructions" list.

### Why the color range is not auto-fitted

`design_scalar_fields.md` §Background establishes that every one of these
quantities is dominated by sharp peaks at the nuclei, with the interesting
structure in the low-magnitude tail: electron density spans six orders of
magnitude between the conventional surface threshold and its value at a carbon
nucleus. **Any auto-ranging over min/max produces a useless picture.** For a
colormap the failure is not a few bright dots but a uniformly white surface,
because every value on the surface falls in the middle fraction of a percent of
the domain.

So `range` is explicit, defaults to the electrostatic-potential convention of
`±0.05` hartree/e, and there is no "fit to data" button. A percentile-based
default is the only defensible automatic rule and it is deferred with the
threshold question (§The `level` default).

### Why only one colormap

`Colormap` has exactly one variant. This looks like a placeholder and is not.

Blue-white-red diverging is the convention for the one coloring pairing that
motivates the feature today: density at 0.002 colored by electrostatic
potential. The other pairing the sibling document names — reduced density
gradient colored by `sign(lambda_2)*rho` for NCI analysis — conventionally uses
a blue-green-red map, but nothing in the application can produce a reduced
density gradient yet, so adding that variant now would be adding an untestable
option for an unreachable workflow.

The enum exists rather than the map being hardcoded because adding a variant is
then a one-line change plus a colormap function, and because it makes the
serialized node data forward-compatible. That is the whole reason. It is not a
promise of more.

## Node: `isosurface`

| | |
|---|---|
| **Name** | `isosurface` |
| **Category** | `AtomicStructure` |
| **Input pin 0** | `field: ScalarField` — required |
| **Input pin 1** | `color_field: ScalarField` — optional; presence switches coloring mode |
| **Input pin 2** | `level: Float` — optional; overrides the stored property when wired |
| **Output pin 0** | `surface: Isosurface` |

`AtomicStructure` rather than `Geometry3D`: it groups the node with
`import_cube` in the palette, which is where a user who just imported a cube
will look, and it avoids implying the CSG composability that `Geometry3D`'s
other members have and this node does not (§Not a `GeoNodeKind`).

### Properties

All persisted in node data, editable in the node editor, `TextValue`-serializable.

| Property | Type | Default | Notes |
|---|---|---|---|
| `level` | `f64` | `0.02` | magnitude; extraction runs at `+level` **and** `-level` |
| `positive_color` | `DVec3` | `(0.20, 0.40, 0.90)` | 0–1 RGB, matching `apply_style`'s convention |
| `negative_color` | `DVec3` | `(0.90, 0.30, 0.25)` | |
| `alpha` | `f64` | `0.4` | `1.0` → opaque fast path |
| `colormap` | `Colormap` | `BlueWhiteRed` | only consulted when `color_field` is wired |
| `color_min` | `f64` | `-0.05` | only consulted when `color_field` is wired |
| `color_max` | `f64` | `0.05` | |

### Why `level` gets a pin and the colors do not

`free_sphere` establishes the pattern precisely: `center` and `radius` are both
input pins **and** stored properties, with the wired value winning when
connected (`import_xyz` does the same with `file_name`, using
`connected_input_pins` to suppress the property in the text format).

The line this design draws, and which should hold for later additions:
**parameters that change the geometry get pins; parameters that change only the
paint stay properties.** `level` changes which surface exists, so it is
pin-worthy and genuinely useful to drive from a `parameter` node or an `expr`
when sweeping thresholds. Colors, alpha, colormap and colormap range change
nothing about the geometry, and nobody drives them from an expression.

### Why `alpha` defaults to 0.4

Not a round-number guess. The two-pass draw composites two layers of the same
surface, so the perceived opacity is `1-(1-a)^2`: at `a = 0.4` that is `0.64`,
which is roughly what `0.6` looks like on a single layer. Picking `0.5`
naively yields `0.75` — noticeably denser than intended. The doubling also
darkens silhouettes for free, where the two layers converge at grazing angles,
which is a desirable and physically honest effect rather than an artifact.

### The `level` default

`0.02` is the conventional MO amplitude threshold
(`design_scalar_fields.md` §Background). It is *wrong by an order of magnitude*
for an electron density, whose conventional molecular-surface threshold is
`0.002`.

This is knowingly accepted. The sibling document establishes that a field's
meaning is not recoverable from its numbers and deletes `FieldKind` for that
reason, so no fixed default can be right for both. The alternative — a
percentile-of-magnitude rule derived from `value_range` — remains the open
question the sibling document names, and answering it requires sweeping both
rules against real cubes. Until then a fixed constant plus a visible value
range (below) is the honest arrangement.

### Level-picking affordance

`design_scalar_fields.md` P2 lists `to_detailed_string` for
`NetworkResult::ScalarField` (dims and value range) as **optional**. This
design **promotes it to required**, because it is the level-picking affordance:
hovering the `import_cube` field output pin then shows the value range through
the existing hover-value machinery, which is exactly what a user needs in order
to choose a threshold.

That costs one match arm. A dedicated statistics readout in the node editor
would cost a new API getter plus a Dart widget for the same information, so it
is **not** included. Note that `design_scalar_fields.md` explicitly excludes
such a readout from `import_cube` on the grounds that it is "a visualization
concern" — this is that concern arriving, and the cheap answer covers it.

### Editor

Follows the established node-data editor pattern, registered in
`node_data/node_data_widget.dart`:

- `level` numeric field
- two color swatches for `positive_color` / `negative_color`, plus a
  **one-click swap button** between them
- `alpha` slider
- colormap range fields and colormap dropdown, **disabled unless
  `color_field` is connected**

The swap button is not a convenience. `design_scalar_fields.md` requires it:
an orbital's global sign is arbitrary, re-running a calculation can flip which
lobe is which, and flipping is how a user matches a published figure or
compares two orbitals.

### Errors and warnings

| Condition | Behavior |
|---|---|
| `field` unconnected | evaluation error |
| `level <= 0` | evaluation error naming the constraint (the sign is not a user choice; both signs are always drawn) |
| `level` exceeds the field's `value_range` magnitude | **non-blocking** `NodeDataError::warning` — "no crossings at this level" — because an empty render is otherwise indistinguishable from a broken import |
| surface field's `data_bounds` exceed the color field's | **non-blocking** warning (§The mismatched-bounds artifact) |

All warnings are non-blocking by the litmus in
`doc/design_error_management.md`: the node still produces a usable value, and
the user is told exactly what looks wrong.

### The mismatched-bounds artifact

`ScalarField`'s out-of-bounds rule is that `sample` returns exactly `0.0`, and
`design_scalar_fields.md` is emphatic that this is the *only* rule, with no
consumer layering a stricter one on top. This design honors that — and it is
also the one place where the rule produces a silently wrong picture.

If the surface extends past the **color** field's `data_bounds`, those vertices
sample `0.0` and get painted as though the potential there were exactly
neutral. On a blue-white-red map that is plain white: plausible, and wrong.
Mixing sources makes this easy to hit, and the sibling document explicitly
expects it — "orbitals and density from `.molden`, electrostatic potential from
a `.cube` file".

The fix is a warning, not an error and not a second out-of-bounds convention:
compare the two fields' `data_bounds` at eval time and warn when the surface
field's box is not contained in the color field's. The picture is still drawn.

## Value plumbing

### `DataType::Isosurface`

An ordinary first-class pin type: not an `Optional`-style modifier, not
abstract, no subtyping, no implicit conversions. The touchpoints mirror the
`DataType::ScalarField` list in `design_scalar_fields.md` §Value plumbing
exactly, and carry the same trap.

**Rust core (`atomcad-structure-designer`):**

| Site | Change |
|---|---|
| `data_type.rs` `enum DataType` | new variant |
| `data_type.rs` `impl fmt::Display` | `=> write!(f, "Isosurface")` |
| `data_type.rs` `from_string` keyword table | `"Isosurface" => Ok(DataType::Isosurface)` |
| `text_format/node_type_introspection.rs` | add to the "no text literal representation" list |
| `evaluator/network_result.rs` `infer_data_type` | `NetworkResult::Isosurface(_) => Some(DataType::Isosurface)` — **`_ => None` arm means omitting this compiles and silently mis-infers** |
| `evaluator/network_result.rs` `to_display_string` | short summary; exhaustive match |
| `evaluator/network_result.rs` `to_detailed_string` | level, coloring mode, and the field's dims |
| `evaluator/network_result.rs` `heap_of` | see below — **not** a compile error if missed |

**FRB boundary (root crate):** `APIDataTypeBase` variant, both conversion arms
in `structure_designer_api.rs`, then `flutter_rust_bridge_codegen generate`.

**Dart:** `data_type_input.dart`, `type_editor_dialog.dart`,
`node_widget.dart` `_apiDataTypeToString`, `node_network.dart` pin color.

Pin color: `ScalarField` is specified as a soft red (`0xFFE57373`) forming a new
"volumetric data" family. `Isosurface` is the rendered form of the same family,
so a deeper red — `0xFFC62828` — reads as related without being confusable.
Worth eyeballing against the live palette before committing.

### `heap_of` needs care

`network_result.rs` has memory accounting via
`atomcad_util::memory_size_estimator`. Two hazards specific to this variant,
neither of which the compiler catches:

- `SampledField` holds a `Vec<f32>` that is routinely megabytes, so omitting
  the arm silently under-reports by the largest payload in the tree.
- The **same `Arc` can appear twice** in one value when the surface field and
  the color field are the same field, and can be shared across many results.
  Naive recursion double-counts. Account for the `Arc`'d field once per
  distinct pointer, or exclude shared field storage from the estimate
  deliberately and say so in a comment.

### `NodeOutput::Isosurface`

```rust
/// Extracted, resolution-fixed surface, ready for tessellation.
Isosurface(SurfaceMesh),
```

`SurfaceMesh` is a new `atomcad-display` type rather than a reuse of
`PolyMesh`, for one concrete reason: **`PolyMesh` carries no per-vertex color.**
`poly_mesh_tessellator` assigns a single `Material` per mesh (plus a
`highlighted_material` for highlighted faces), and the colormap mode needs a
different albedo at every vertex.

Adding a per-vertex color channel to `PolyMesh` would benefit nothing else and
would sit awkwardly beside its face-centric adjacency model, which exists to
support `detect_sharp_edges` — the opposite of what a marching-cubes surface
wants, since it is smooth by construction and gets its normals from the field
gradient. Keeping the types separate keeps `PolyMesh` focused on the
CSG-derived case it was built for.

```rust
pub struct SurfaceMesh {
    pub positions: Vec<Vec3>,
    /// Outward normals from the field gradient (§Normals).
    pub normals: Vec<Vec3>,
    /// Per-vertex albedo. Solid in `Phase` mode, colormapped in `Field` mode —
    /// one representation, two authoring paths, exactly as
    /// `design_scalar_fields.md` specifies.
    pub albedo: Vec<Vec3>,
    pub indices: Vec<u32>,
    /// Connected components, each a contiguous slice of `indices`. Ordering
    /// these back-to-front at draw time is what makes multi-lobe transparency
    /// correct (§Rendering).
    pub components: Vec<SurfaceComponent>,
    pub alpha: f32,
}

pub struct SurfaceComponent {
    pub first_index: u32,
    pub index_count: u32,
    /// Centroid in world space; the sort key.
    pub centroid: Vec3,
}
```

## Extraction

Lives in `atomcad-display/src/isosurface/`, invoked from `network_evaluator`'s
result→`NodeOutput` conversion, cached alongside `csg_conversion_cache`.

Marching cubes is hand-rolled: no workspace dependency provides it, and the
algorithm is a case table plus edge interpolation. `rayon` is already a
workspace dependency and `ScalarField: Send + Sync` was specified in the
sibling document precisely so batched sampling can be parallel — the extraction
grid is the first consumer to use it.

### Why marching cubes

Recorded because "use marching cubes" is the kind of default that looks
unconsidered, and because what rules out the two obvious rivals is specific to
this application rather than general.

**Dual contouring is the wrong tool for a smooth field.** Its reason to exist
is sharp feature preservation — reconstructing edges and corners by solving a
QEF per cell from Hermite data, exactly right for CSG and SDF surfaces. An
orbital or density isosurface has **no sharp features**: `psi` is a sum of
smooth Gaussians and the surface is smooth everywhere at any conventional
threshold. What remains of DC is its costs — a per-cell QEF solve with the
numerical care that implies, and vertices that are *minimizers* rather than
points on the surface, so they can drift outside their cell and need clamping.
A marching-cubes vertex lies exactly on the isosurface by linear interpolation
along an edge, which for a visualization whose claim is "this is the
`psi = level` surface" is a correctness property, not a nicety.

**MC's poor triangle quality costs nothing here**, because §Normals takes
normals from the analytic gradient rather than from face normals. Slivers would
matter for decimation or smoothing; they do not affect shading.

**DC's adaptivity has little to buy.** Octree DC wins by being coarse in flat
regions, but the `ScalarField` contract already caps useful resolution at the
native grid — sampling finer buys interpolation, not information — so the
extraction budget is already right, and sampling a stored grid is cheap. The
savings would be in triangle count, at a scale (~60–100k triangles for an 80^3
field) that is not a problem.

**Naive surface nets is the closer rival, and loses on one point.** Dual, one
vertex per cell, no QEF, simpler than both MC and DC, better-shaped polygons,
and its only real weakness — rounding sharp features — is irrelevant here. But
one vertex per cell **cannot represent two surface sheets passing through the
same cell**, so it welds them. That merges two lobes into one connected
component, corrupting the union-find labeling the component-sorted transparency
depends on (§Connected components). The risk is modest — adjacent lobes in a d
orbital are opposite-sign and so are extracted in separate passes — but it
rises at coarse `isosurface_quality_multiplier` settings, which is exactly the
draft mode that preference exists to enable.

### Ambiguity: closed beats topologically optimal

This is the one genuine implementation risk in marching cubes, and it matters
more here than in a typical application.

The original 15-case table produces **holes** on ambiguous face configurations.
Normally that is a cosmetic annoyance. Here it is not: a hole makes the surface
open, and an open surface has no back-face/front-face pairing, so the two-pass
transparency assumption does not merely degrade — it collapses (§Rendering).
Holes also fragment connected components, so the sort keys go wrong too. Both
failures present as "transparency is buggy" rather than "extraction is buggy",
which is the most expensive kind of misdirection.

**The rule: use the full 256-entry table and resolve ambiguous faces
*consistently* — the same connectivity choice for complementary cases.** That
guarantees a closed manifold surface, at the price of occasionally choosing
topologically "wrong" connectivity: a tunnel where there should be two separate
sheets. That trade is right here, and worth stating as a principle because the
tunnel will look like a bug to whoever finds it:

> **Guaranteed-closed beats topologically-optimal.** The renderer assumes
> closed surfaces and does not care about a spurious tunnel. It cares very much
> about a hole.

The asymptotic decider (MC33) resolves the ambiguity *correctly* rather than
merely consistently, and can be added later if a tunnel is ever visible enough
to matter. It is not needed for the correctness of anything this document
builds.

**Escape hatch if ambiguity handling becomes a time sink: marching
tetrahedra.** Split each cell into six tets — 16 cases, three up to symmetry,
unambiguous and closed by construction, with no case table to get wrong. It
costs roughly 2x the triangles, irrelevant at these sizes, and introduces a
faint directional bias from the split direction, which is the only reason not
to lead with it.

### Resolution policy

```
native_grid().map(|g| g.spacing() / quality_multiplier)
             .unwrap_or(preference_spacing)
```

The default is the field's **own grid, verbatim**, which is the fidelity fast
path the `ScalarField` contract describes: sampling a stored field anywhere
else blends eight stored values per point and smooths the field for no gain.
A preference multiplier allows coarser (draft) or finer (interpolated, smoother
silhouettes) extraction.

When `native_grid()` returns `None` — every analytic field, i.e. all future
Molden support — the fallback is a preference spacing in Ångström over
`suggested_bounds()`. **This is the consumer that proves the contract**: the
sibling document says the first Molden field will immediately expose any
consumer written against a grid, and this is the consumer most likely to have
been.

Cells are formed between adjacent sample points, so a `dims = [nx, ny, nz]`
grid yields `(nx-1)(ny-1)(nz-1)` cells. The node-centered bounds convention
(`design_scalar_fields.md` §Bounds convention) makes this exact with no
half-voxel adjustment; iterating exclusively of `max` would silently drop the
last plane.

### The two sign passes

Run the **same** extractor twice with a sign flag, comparing `sign * psi(p)`
against `level` and multiplying the gradient by `sign`:

- `sign = +1` → the positive lobe, colored `positive_color`
- `sign = -1` → the negative lobe, colored `negative_color`

This is worth stating as a rule because the alternative — a separate negative
code path, or post-hoc winding correction — is where the bugs live. With the
sign folded into the comparison, winding and normals come out consistent
automatically and only the color differs between the calls.

`value_range()` short-circuits the negative pass when `min >= 0`. That is a
pure optimization: on a non-negative field the negative pass finds no crossings
and produces empty geometry anyway, so the one-component case falls out with no
semantic knowledge and no branch — exactly as `design_scalar_fields.md`
specifies.

### Normals

Per-vertex, from `ScalarField::gradient`, normalized, multiplied by `-sign`.

For the positive lobe the field decreases outward, so outward is `-gradient`;
for the negative lobe the interior is `psi < -level` and the field increases
outward, so outward is `+gradient`. Folding `sign` into the comparison (above)
makes `-sign * normalize(gradient)` the single expression for both.

`SampledField` overrides `gradient` with central differences on the stored
samples, so these are exact with respect to the stored data and cheaper than
interpolating.

Whether gradient normals are visually sufficient, or whether mesh smoothing is
needed, is one of the sibling document's open questions; nothing here forecloses
adding smoothing later.

### Winding, and why it is load-bearing

Triangles must be emitted **counter-clockwise viewed from outside**, consistent
with the outward normal above. This is not cosmetic: the two-pass transparency
draw is `cull_mode: Front` followed by `cull_mode: Back`, so an inverted
component draws near-before-far and composites wrongly.

The failure is subtle — a wrongly-wound lobe looks *almost* right, just slightly
off in a way that is easy to attribute to shading. This is the same class of bug
as the csgrs `det<0` winding fix made for `structure_invert`. A test asserts it
directly (§P2).

### Connected components

After extraction, union-find over triangle vertex indices labels connected
components; triangles are then re-emitted grouped by component so each occupies
a contiguous index range, and a centroid is computed per component.

The two sign passes are already separate; components are labeled *within* each
pass, and the resulting `SurfaceComponent` list spans both.

This costs one union-find over the index buffer, and it is what makes the
multi-lobe transparency case correct (§Rendering).

## Rendering

### What is not correct about the cheap approach

`design_scalar_fields.md` proposes "a two-pass back-faces-then-front-faces draw
rather than porting the per-quad sort to triangles" for "closed blobs". The
approach is sound but the caveat needs stating, because the case it gets wrong
is the common one.

The two-pass trick assumes that along any view ray the surface is hit exactly
twice, with the back-face hit always farther. **That holds for a convex closed
surface.** It fails three ways:

1. **Multiple components.** A p orbital is two lobes, a d orbital four, a real
   MO at a working threshold routinely 4–8. A ray through two lobes crosses
   four times: `F1 B1 F2 B2`. Two-pass emits `B1 B2 F1 F2`, with both front
   faces composited last, after geometry behind them. Wherever two lobes
   overlap on screen the result is wrong — and overlapping lobes are what
   people look at orbitals to see.
2. **Non-convex single components.** One connected piece can be hit more than
   twice: a saddle region, a torus, a lobe with a waist near the nodal surface.
   Within a pass there is no ordering at all.
3. **No ordering between transparent pipelines.** Nothing writes depth, so the
   isosurface pass and the existing transparent-impostor pass composite in
   draw-call order. `transparent_sort.rs` sorts quads *within* the impostor
   mesh and cannot interleave triangles from another pipeline. An x-ray ghost
   atom inside a lobe is therefore always wholly in front of or wholly behind
   it. This one is independent of convexity and does not go away with better
   mesh sorting.

### The fix: sort components, two-pass within each

Components are spatially disjoint for lobes, so a centroid sort orders them
correctly essentially always. Draw order becomes: components back-to-front by
view-space z; within each, back faces then front faces. That fixes failure
mode 1 exactly and leaves 2 and 3.

The sort happens **at draw time in the renderer**, not at tessellation time. It
needs only the view matrix and reorders *draw calls*, not indices — so unlike
`transparent_sort.rs` there is no index-buffer rewrite and no
`queue.write_buffer`. With N components in the single digits it is free.

Failure mode 3 is accepted for now and recorded in §Rendering: what is not
built.

### Pipelines and blend mode

Two new pipelines, identical except for `cull_mode`:

```rust
blend:                Some(BlendState::ALPHA_BLENDING),
cull_mode:            Some(wgpu::Face::Front),   // pass 1
// then                Some(wgpu::Face::Back),   // pass 2
depth_write_enabled:  false,
depth_compare:        wgpu::CompareFunction::Less,
```

`BlendState::ALPHA_BLENDING` is non-premultiplied source-over
(`SrcAlpha`/`OneMinusSrcAlpha` for color, `One`/`OneMinusSrcAlpha` for alpha),
the same blend both existing transparent pipelines use. Depth *test* stays on so
the opaque ball-and-stick — drawn first with depth writes on — correctly
occludes surface behind it. Depth *write* must be off, or the first transparent
fragment kills the surface's own second layer.

### The shader delta is one line, thanks to `ModelUniform`

`design_scalar_fields.md` predicts the mesh path "adds no shader, no bind group
and no renderer surgery". That holds, but the reason is worth recording because
it very nearly did not.

`mesh.wgsl`'s fragment stage ends `return vec4<f32>(color, 1.0);` — opacity is
hardcoded. Per-vertex albedo is already available (`Vertex` carries
`albedo`/`roughness`/`metallic`), so the colormap needs nothing new; **alpha is
the only missing channel.** Since alpha is uniform across a surface, it belongs
in the existing per-mesh `ModelUniform` (group 1, currently `model_matrix` +
`normal_matrix`) rather than in the vertex format:

- `ModelUniform` gains an `alpha: f32`
- `mesh.wgsl` returns `vec4<f32>(color, model.alpha)`
- `ModelUniform::new()` defaults it to `1.0`, so every existing opaque consumer
  is unchanged

**Padding gotcha.** Two `mat4x4<f32>` is 128 bytes; a bare trailing `f32`
leaves the struct un-padded to a 16-byte boundary and WGSL layout rules will
not match the Rust `#[repr(C)]` struct. Add explicit padding — or declare the
slot as a `vec4<f32>` with the remaining lanes reserved. This project has been
bitten by exactly this before: the ortho impostor fix (issue #269) was a
`CameraUniform` WGSL padding bug that produced wrong output with no error. A
test asserting `size_of::<ModelUniform>() % 16 == 0` is cheap insurance.

### The opaque fast path

`alpha == 1.0` routes to the **existing** opaque mesh pipeline: one draw,
`cull_mode: Back`, depth write on, no component sort, no transparency mode.

This is not just an optimization. It means the first user-visible milestone
(§P3) needs no transparency work at all, and it gives a permanent escape hatch:
whenever transparency looks wrong, setting alpha to 1 produces a picture whose
correctness is not in question.

### Rendering: what is not built

**GPU volume raymarching.** The sibling document names it the better eventual
answer — a free isolevel slider, resolution-independent lobes, and true
volumetric rendering that surface extraction cannot do at all. Choosing a lazy
value type keeps it reachable: a raymarcher wants a dense grid derived from
`sample_batch`, which is exactly what `IsosurfaceData` already carries, so it
would be a new *consumer* of the same value rather than a change to it.

**Weighted-blended OIT.** The order-independent option that would fix failure
mode 3 for the isosurface *and* the existing ghost impostors together,
eventually making `transparent_sort.rs` deletable. It needs the main pass to go
MRT (an accumulation target plus a revealage target) plus a fullscreen resolve
pass; `renderer.rs` currently builds two passes each with a single color
attachment, so this is the only option that changes the renderer's structure
rather than adding to it. Deferred until the evidence says component sorting is
insufficient.

**Additive blending.** Order-independent and nearly free at the pipeline level,
but Lambert/PBR shading under additive blending blows out lit areas and drops
dark ones, so it needs a Fresnel rim term — a shader variant, not a blend-state
flip. It also produces a glow rather than a surface, which is a different
picture rather than a cheaper version of this one. If wanted, it belongs later
as a user-facing display style with its own justification.

## Preferences

Four new entries, following the existing plumbing:
`crates/atomcad-structure-designer/src/preferences.rs` (core) →
`rust/src/api/structure_designer/structure_designer_preferences.rs` (API twin,
then codegen) → `lib/structure_designer/preferences_window.dart`, in the
**Geometry Visualization** section.

| Preference | Type | Default | Purpose |
|---|---|---|---|
| `isosurface_quality_multiplier` | `f64` | `1.0` | divides the native grid spacing; `<1` coarser (draft), `>1` finer |
| `isosurface_fallback_spacing` | `f64` | `0.15` Å | extraction spacing when `native_grid()` is `None` |
| `surface_transparency_mode` | enum | `ComponentSorted` | see below |
| `isosurface_cell_budget` | `usize` | `4_000_000` | refuse-and-warn ceiling so a fine multiplier on a large grid cannot hang the UI |

Putting these in preferences rather than on the node is the same call made for
every other geometry-quality knob, and it is what keeps them changeable without
re-evaluating the network (§Where the extraction happens).

### The comparison modes, and their expiry condition

```rust
pub enum SurfaceTransparencyMode {
    /// One draw, no culling. The control: isolates whether ordering helps.
    SinglePass,
    /// Back faces then front faces, components unordered.
    TwoPass,
    /// Components back-to-front, two-pass within each. Default.
    ComponentSorted,
}
```

These exist to answer the sibling document's open question — *is two-pass
transparency good enough for concave lobes, or is a per-triangle sort
required?* — with the evidence it names, on the same scene at the same camera.
They are nearly free: once components are contiguous index ranges, `TwoPass` is
"skip the sort" and `SinglePass` is "one draw, no culling".

**They are scaffolding, and this document sets their expiry.** Remove
`SinglePass` and `TwoPass` once both of the following are recorded in this
document:

1. whether `ComponentSorted` is visibly better than `TwoPass` on a multi-lobe
   signed field, and
2. whether either is good enough that per-triangle sorting or OIT is
   unnecessary.

Without a stated removal criterion these become permanent options that every
future renderer change has to keep working — which is precisely the
speculative-option problem `design_scalar_fields.md` argues against when it
deletes `FieldKind`. The difference between scaffolding and cruft is whether
the removal criterion was written down at the same time.

**The comparison is only informative on the right subject.** An electron
density envelope is one near-convex component; all three modes look identical
on it, and concluding "sorting is pointless" from a water density would be
wrong. Use a multi-lobe signed field: a d orbital, or a p orbital viewed down an
axis where the lobes overlap on screen.

## Implementation plan

Each step ends green: `cargo test -j 4`, `cargo clippy`, `flutter analyze`.
Every phase carries its own tests; every phase with UI carries its own manual
walkthrough. Per `AGENTS.md`, the Flutter smoke test is a **human-only** manual
step and is never run by an agent.

Fixtures reuse `scripts/make_cube_fixtures.py` from `design_scalar_fields.md`,
extended with the shapes this document's tests need (§P2).

### P1 — `Isosurface` value plumbing

**Work**

- `IsosurfaceData`, `IsosurfaceColoring`, `Colormap` in
  `atomcad-display/src/isosurface/mod.rs`
- `DataType::Isosurface` and every touchpoint in §Value plumbing, plus the FRB
  codegen run and the four Dart sites
- `NetworkResult::Isosurface(IsosurfaceData)` with its `to_display_string`,
  `to_detailed_string`, `infer_data_type` and `heap_of` arms
- promote `to_detailed_string` for `NetworkResult::ScalarField` from optional
  to implemented (§Level-picking affordance)

**Tests**

| Test | Asserts |
|---|---|
| `DataType` text round-trip | `from_string("Isosurface")` and `Display` agree |
| `APIDataType` round-trip, both directions | catches a missed FRB conversion arm |
| `infer_data_type` on a `NetworkResult::Isosurface` | returns `Some(DataType::Isosurface)` — **the one site with no compiler backstop** |
| `heap_of` with the same `Arc` as surface and color field | counted once, not twice |
| `to_detailed_string` on a `ScalarField` | reports dims and value range |
| Existing registry-validation suite | stays green |

**Deliverable:** a wireable type carrying no surfaces yet.

### P2 — Marching cubes extractor (backend only)

**Work**

- `atomcad-display/src/isosurface/extract.rs`: `IsosurfaceData` → `SurfaceMesh`
- the 256-entry case table with **consistent** complementary-case resolution
  (§Ambiguity), and edge-keyed vertex deduplication so shared vertices are
  genuinely shared — component labeling depends on it
- the sign-folded comparison, gradient normals, winding, union-find components
- resolution policy including the `native_grid() == None` fallback
- the cell-budget check

**Tests**

The critical tests are the ones whose failures are *silent* in a picture.

| Test | Asserts |
|---|---|
| Analytic sphere field (`r - R`), level 0 | every vertex within tolerance of radius `R`; triangle count in the expected band |
| Same, normals | every normal parallel to the vertex's radial direction, pointing **outward** |
| Same, **winding** | `cross(v1-v0, v2-v0) . outward_normal > 0` for **every** triangle — the two-pass draw depends on it and the failure is invisible in a still image |
| Same, **closedness** | every edge is shared by exactly **two** triangles, i.e. zero boundary edges. **A sphere with a hole passes the radius, normal and winding tests** — this is the only assertion that catches it, and a hole collapses the transparency assumption (§Ambiguity) |
| Same, Euler characteristic | `V - E + F == 2` for the sphere; catches non-manifold welding that a boundary-edge count alone can miss |
| A field posed to hit an ambiguous face configuration | still closed (zero boundary edges) — the complementary-case rule is what this asserts; the resulting topology is *not* asserted, per §Ambiguity |
| Two-sheet cell at coarse spacing | two same-sign lobes separated by less than one cell still yield **two** components — the failure mode that ruled out surface nets, asserted against the chosen algorithm |
| Analytic 2p_z field, level `L` | exactly **two** components; their centroids on opposite sides of the nodal plane |
| Same, colors | the `+z` component gets `positive_color`, the `-z` component `negative_color` — catches a sign-pass mix-up |
| Non-negative field (density-like) | exactly one component; the negative pass short-circuits via `value_range` |
| Level above the field's max magnitude | empty mesh, no panic |
| Component index ranges | every `SurfaceComponent` range is contiguous, non-overlapping, and together they cover `indices` exactly |
| A field with `native_grid() == None` (a test-only analytic impl) | extraction succeeds at the fallback spacing — **the consumer that proves the contract** |
| Cell budget exceeded | refuses with a descriptive error rather than allocating |
| Colormap mode on a linear color field | per-vertex albedo varies monotonically along the gradient; values outside `range` clamp rather than wrap |

**Deliverable:** verified extraction. Nothing user-visible.

### P3 — `isosurface` node, editor, opaque rendering

**The first user-visible milestone**, and deliberately opaque-only.

**Work**

- node data, `eval`, registration in `nodes/mod.rs` and `node_type_registry.rs`
- `SurfaceMesh`, `NodeOutput::Isosurface`, the conversion in
  `network_evaluator`, and its cache
- `scene_tessellator` arm producing a renderer mesh
- `isosurface_editor.dart` and its `node_data_widget.dart` entry
- the three extraction preferences (not the transparency mode yet)
- reference guide: `doc/reference_guide/nodes/atomic.md`, and the preferences
  in `doc/reference_guide/ui.md`

**Tests**

| Test | Asserts |
|---|---|
| Node eval on a cube fixture | produces `NetworkResult::Isosurface` with the field and level from its properties |
| `level` input pin wired | overrides the stored property, mirroring the `free_sphere` / `import_xyz` tests |
| `level <= 0` | evaluation error |
| `level` above the field's range | non-blocking warning from `get_data_error`; the node still produces a value |
| `.cnnd` round-trip | all properties survive; the `Colormap` enum round-trips |
| Result → `NodeOutput` conversion | yields a `SurfaceMesh` with the expected component count |

**Manual walkthrough**

1. `import_cube` → `sample_data/cube/water.cube` → `isosurface`, alpha `1.0`.
   Display the surface. **Expect** an opaque blob roughly enclosing the
   molecule.
2. Display `import_cube`'s `molecule` pin at the same time. **Expect** the
   surface and the atoms to be *registered* — same position, same scale. A
   1.9x size mismatch means a Bohr conversion is wrong somewhere.
3. Scrub `level` upward. **Expect** the surface to shrink monotonically and
   eventually vanish with an amber warning, not an error.
4. On a signed orbital fixture, **expect two lobes in two colors**. Click the
   swap button; **expect** the colors to exchange and nothing else to change.
5. Change `isosurface_quality_multiplier` in preferences. **Expect** the
   surface to re-tessellate at the new resolution **without the node network
   re-evaluating** — this is the whole architectural claim of §Where the
   extraction happens, made visible.

**Deliverable:** import a `.cube` and see its isosurface, opaque, correctly
registered with the molecule.

### P4 — Transparency

**Work**

- `ModelUniform` gains `alpha` with explicit padding; `mesh.wgsl` returns it
- the two culled pipelines, `ALPHA_BLENDING`, depth-write off
- draw-time component sort by view-space z
- `surface_transparency_mode` preference and its three modes
- reference guide: transparency notes in `ui.md`

**Tests**

| Test | Asserts |
|---|---|
| `size_of::<ModelUniform>() % 16 == 0` | the padding gotcha, caught at compile-adjacent cost |
| `ModelUniform::new()` | `alpha == 1.0`, so existing opaque consumers are unchanged |
| Component sort | given synthetic centroids and a view matrix, ranges come back farthest-first (mirrors the `transparent_sort` tests) |
| `alpha == 1.0` | routes to the opaque pipeline — assert on the chosen pipeline, not the pixels |

**Manual walkthrough**

1. Set alpha to `0.4` on a **d-type or overlapping-p** orbital — not a density
   envelope, which cannot distinguish the modes (§The comparison modes).
2. Cycle `surface_transparency_mode` through all three at a fixed camera.
   **Expect** `SinglePass` to show obvious ordering errors where lobes overlap,
   `TwoPass` to improve within each lobe but still misorder between lobes, and
   `ComponentSorted` to be correct.
3. Orbit the camera in `ComponentSorted`. **Expect** no popping as the
   component order changes — a pop means the sort is not running per frame.
4. Display x-ray ghost atoms and the surface together. **Expect** the known
   failure mode 3: the ghosts do not interleave correctly with the lobes.
   Record what it looks like; that observation is the input to the OIT
   decision.
5. **Record the answers to the expiry criteria in this document** and delete
   the losing modes.

**Deliverable:** semi-transparent orbitals, and the evidence that decides
whether the transparency approach needs to go further.

### P5 — Color field and colormap

**Work**

- `color_field` input pin wiring, `IsosurfaceColoring::Field` construction
- per-vertex colormap sampling during extraction
- the mismatched-bounds warning
- editor: colormap dropdown and range fields, disabled unless connected
- reference guide update

**Tests**

| Test | Asserts |
|---|---|
| Color field wired | `coloring` is the `Field` arm; unwired reverts to `Phase` |
| Colormap range | values at `color_min` / `color_max` map to the palette ends; beyond them, clamp |
| Signed surface with a color field | still two components (topology is unaffected by coloring) but both painted per-vertex — the settled 2x2 |
| Surface bounds exceeding the color field's | non-blocking warning; surface still drawn |

**Manual walkthrough**

1. Wire a density cube into `field` and an electrostatic-potential cube into
   `color_field`. **Expect** the classic ESP map: a molecular envelope in
   red/white/blue.
2. Deliberately pair fields with mismatched boxes. **Expect** an amber warning
   naming the mismatch, and a visible white band where the surface leaves the
   color field's box — the artifact the warning exists to explain.
3. Repeat step 1 through `atomcad-cli` to confirm the headless path.

**Deliverable:** the standard density-colored-by-potential picture. **This
closes the scope of this document.**

## Documentation touchpoints

Per `AGENTS.md`, in the same change as the code:

- `doc/reference_guide/nodes/atomic.md` — the `isosurface` node (P3, updated P5)
- `doc/reference_guide/node_networks.md` — `Isosurface` in the pin-type list
  and its color (P1)
- `doc/reference_guide/ui.md` — the four new Geometry Visualization
  preferences (P3, P4)
- `crates/atomcad-display/src/AGENTS.md` — **new file**: module map including
  `isosurface/`, the invariant that coordinates crossing it are Ångström, and
  the outward-winding rule (§Winding). Also add it to the root `AGENTS.md`
  "Subdirectory Instructions" list.
- `doc/testing.md` — the winding **and closedness** assertions as required
  tests for any surface extractor, with the note that a holed surface passes
  every per-vertex check
- this document — the recorded answers to the expiry criteria (P4)
