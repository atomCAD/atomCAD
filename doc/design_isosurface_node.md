# Design: the `isosurface` node — displaying scalar fields

Companion to `doc/design_scalar_fields.md`, which builds ingestion (the
`ScalarField` contract, the `.cube` loader, `DataType::ScalarField`,
`import_cube`, `sample_field`) and defers visualization. This document is the
visualization half: the node that turns a `ScalarField` into a rendered
**isosurface**, optionally painted with a second field's values.

Assumes P1–P4 of `design_scalar_fields.md` have landed. Does not assume P5
(multi-field cubes) or Molden support.

**Deferred:** GPU volume raymarching, slice planes, automatic HOMO/LUMO
selection, CSG composability, extraction caching.

## Where the extraction happens

**Decision: the node outputs a lazy surface specification. Marching cubes runs
in the display conversion, in `atomcad-display`.**

```
   isosurface node
        |
        v
  NetworkResult::Isosurface(IsosurfaceData)     <- network value
  { field, level, coloring, alpha }                resolution-free
                                                   (atomcad-crystolecule)
        |
        |  atomcad_display::isosurface::extract
        |  (marching cubes; resolution from preferences)
        v
  NodeOutput::Isosurface(SurfaceMesh)           <- scene output
  positions + normals + per-vertex albedo          resolution fixed
  + component ranges + alpha                       (atomcad-display)
        |
        |  scene_tessellator  (merges every displayed surface;
        |                      alpha baked per-vertex; splits
        |                      opaque from transparent)
        v
  two renderer Meshes + component ranges        <- GPU
```

This mirrors the `Blueprint` pipeline, which has three stages, not two:

| Stage | Type | Where | When |
|---|---|---|---|
| Network value | `NetworkResult::Blueprint` → `geo_tree_root: GeoNode` | `atomcad-structure-designer` | every wire traversal; resolution-free |
| Scene output | `NodeOutput::PolyMesh` / `SurfacePointCloud` | `network_evaluator.rs` `generate_explicit_mesh_output` → `atomcad_display::csg_to_poly_mesh` | **only for displayed nodes** |
| GPU mesh | `Mesh` / `LineMesh` | `scene_tessellator.rs` | per tessellation |

The stage-1→2 conversion is driven by preferences
(`GeometryVisualization`, `samples_per_unit_cell`,
`sharpness_angle_threshold_degree`) and has its own cache
(`csg_conversion_cache`), separate from the eval memoization — this design
deliberately does **not** add an equivalent (§No extraction cache). The
governing
rule: **semantic parameters live in the value; quality parameters live in
preferences.** A `sphere` node's `radius` goes into the value as
`GeoNodeKind::Sphere { radius }`; resolution does not. Isolevel is semantic,
extraction resolution is quality.

**Why not output a mesh:**

- **Resolution would have nowhere to come from.** `NodeData::eval`
  (`node_data.rs:211`) receives no preferences —
  `GeometryVisualizationPreferences` enters only at the display conversion
  (`network_evaluator.rs:929` onward). A mesh-outputting node would have to
  make extraction resolution *node data*, baking a quality setting into the
  `.cnnd` and breaking the pattern every other geometry-quality knob follows.
  The alternative is threading preferences through the whole `NodeData` trait.
- **The implicit form is better downstream.** "Which atoms are inside the
  density envelope" is `sample(p) > level` on a field, and unanswerable on a
  triangle soup. Forward-looking — no such consumer exists yet.

**What it does not buy:** changing `level` costs a re-extraction either way.
`level` is node data, so the node re-evaluates and the surface is rebuilt
whichever representation the value carries.

A general `Mesh` type (for STL import/export, mesh booleans) is a separate
feature and stays additive.

**Not a `GeoNodeKind`.** `GeoNode` has two backends; the exact path
(`to_csg_mesh_cached`) builds csgrs BSP trees from polygonal primitives, and a
scalar field has none — an `Isosurface` variant would make `to_csg_mesh`
partial for any tree containing it. Union-as-`min` also stops being
distance-correct. If CSG composability is wanted later, the bridge is a
`GeoNodeKind` that forces its whole tree onto the implicit path, with a
validation rule rejecting it elsewhere.

## The `Isosurface` value

```rust
// atomcad-crystolecule/src/field/isosurface.rs

/// A surface to be extracted at display time. Carries the *specification*,
/// never the mesh.
#[derive(Debug, Clone)]
pub struct IsosurfaceData {
    pub field: Arc<dyn ScalarField>,
    /// Level **magnitude**, strictly positive. Extraction runs at `+level`
    /// and `-level`.
    pub level: f64,
    pub coloring: IsosurfaceColoring,
    /// 0..=1. `>= 1.0` takes the opaque path (§The opaque fast path).
    pub alpha: f32,
}

#[derive(Debug, Clone)]
pub enum IsosurfaceColoring {
    /// No color field — one solid color per sign.
    Phase { positive: Vec3, negative: Vec3 },
    /// Color field supplied — per-vertex colormap over `range`.
    Field {
        field: Arc<dyn ScalarField>,
        /// Colormap domain. Never auto-fitted: these quantities span orders of
        /// magnitude around the nuclei, so fitting to extrema paints the whole
        /// surface one flat color.
        range: (f64, f64),
        colormap: Colormap,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Colormap {
    /// Diverging blue-white-red — the conventional ESP map.
    #[default]
    BlueWhiteRed,
}
```

Carried as `NetworkResult::Isosurface(IsosurfaceData)`.

The coloring enum encodes the 2×2 settled in `design_scalar_fields.md`:
signedness (read from `value_range()` at extraction time) decides how many
components; the enum discriminant decides how they are painted. Neither
consults the other.

**Inline, not `Arc<IsosurfaceData>`** — at most two `Arc`s plus scalars is
already a cheap clone, and `BlueprintData` is inline for the same reason.

**Narrowing is selective, and the split is by consumer.** Every property is
stored as `f64`/`DVec3` at `TextValue::Float` precision. In `eval`, the ones the
*renderer* consumes narrow to `f32`/`Vec3` — `alpha`, `positive_color`,
`negative_color` — while the ones compared against *field values* stay `f64`:
`level`, and `color_min`/`color_max` inside
`IsosurfaceColoring::Field { range }`. `ScalarField::sample` returns `f64`, so
narrowing a threshold would introduce a rounding difference between the
comparison and the data it is compared to.

`Colormap` has one variant because the only reachable pairing today is density
colored by electrostatic potential. NCI's blue-green-red map has no producer
yet. The enum exists so adding one is a one-line change and the serialized
form is forward-compatible.

### Colormap orientation: open, and inverted with respect to the ESP convention

`BlueWhiteRed` is implemented as its name reads: blue at `range.0`, white at the
midpoint, red at `range.1`, with the red channel rising monotonically and the
blue channel falling across the ramp. P2 tested that orientation deliberately
(`the_colormap_is_monotonic_and_clamps`, `a_color_field_paints_per_vertex`), and
it is the orientation of matplotlib's `bwr` and of every ramp with this name.

**Applied to an electrostatic potential, that is backwards from the published
convention.** Chemistry colours *electron-rich* regions red and electron-poor
regions blue; a potential is *negative* where a molecule is electron-rich, so
the conventional map is red at the low end. Wiring a potential into
`color_field` today paints lone pairs blue and polar hydrogens red. Measured on
the P5 walkthrough fixtures: `V = -0.092` at the lone pairs maps to
`rgb(0.10, 0.25, 1.00)`, `V = +0.061` at the hydrogens to `rgb(1.00, 0.20, 0.15)`.

The two prose descriptions in this document ("the conventional ESP map" on the
enum, "the classic red/white/blue ESP map" in the P5 walkthrough) both described
the *chemistry* convention and so did not match the code; the walkthrough has
been corrected to describe what the code does, and the reference guide states
the orientation explicitly rather than leaving a reader to assume the
convention.

**Not resolved here, because it is a one-line change with three plausible
answers and no way to pick between them from inside this document:**

1. **Leave it.** The ramp is named for what it does, and a user who wants the
   chemistry convention negates the potential upstream with an `expr`. Costs a
   user the surprise once.
2. **Reverse the constants**, so `BlueWhiteRed` runs red-low to blue-high. Makes
   the default pairing correct and the enum's name a lie; inverts the three P2
   assertions, which were written on purpose.
3. **Add a second variant** (`RedWhiteBlue`) and make it the default for this
   pairing. Honest, costs a dropdown entry, and is what the enum was built to
   make cheap — the serialized form is already forward-compatible.

Note that **swapping `color_min` and `color_max` is not a fourth option**: an
inverted domain fails `max > min` and `sample_colormap` maps the whole surface
to the midpoint rather than reversing the ramp. That behaviour is itself
deliberate (a user mid-way through typing a negative minimum should not get a
red badge) and is pinned by
`an_inverted_color_domain_is_carried_rather_than_rejected`.

Whichever is chosen, it is a change to `colormap.rs` plus its P2 tests and the
two documentation spots named above — not to the node, the value, or the
extractor.

### Crate placement

The value and the mesh live in different crates, and the split falls on the
stage boundary:

| Type | Crate | Module |
|---|---|---|
| `IsosurfaceData`, `IsosurfaceColoring`, `Colormap` | `atomcad-crystolecule` | `src/field/isosurface.rs` |
| `SurfaceMesh`, `SurfaceComponent`, the extractor | `atomcad-display` | `src/isosurface/` |

**The value goes down to `crystolecule`, beside the `ScalarField` it wraps.**
Two crates have to see it: `atomcad-structure-designer`, because
`NetworkResult` carries it, and `atomcad-display`, because the extractor
consumes it. The crates visible to both are `display`, `crystolecule`,
`geo-tree`, `renderer` and `util` — so `display` would *work* too.
`crystolecule` is chosen because it preserves an invariant the value layer has
today: **every `NetworkResult` payload comes from the domain layer or from
`structure_designer` itself.** `UnitCellStruct`, `DrawingPlane`, `Motif`,
`Structure` and `Arc<dyn ScalarField>` come from `crystolecule`; `GeoNode`,
inside `BlueprintData`, from `geo-tree`; `Walker` and `ZoneClosure` from
`structure_designer`. Not one comes from `display` — display types enter one
stage later, in `NodeOutput` (`PolyMesh`, `SurfacePointCloud`). An
`IsosurfaceData` in `display` would be the first exception; in `crystolecule`
it is the same shape as `BlueprintData` holding a `GeoNode`.

The crate is also already equipped for it. `serde`, `glam` and `thiserror` are
in `crystolecule`'s manifest, so `Colormap`'s derive is free — and `Colormap`
is node data that round-trips through the `.cnnd`, so it needs `serde`
*wherever* it lives; `atomcad-display` has no `serde` dependency and should not
acquire one to host a persisted enum. RGB paint is not new down there either:
`AtomInfo` has carried `color: Vec3` (`atomic_constants.rs`) since long before
this document. And the direction has precedent — the Miller-index arithmetic
that `half_space_utils` used to carry went the same way, out of the adapter
layer and down to `crystolecule::miller`
(`doc/design_push_domain_code_down.md` D5).

**The mesh stays in `atomcad-display`**, which is what that crate is for.
`SurfaceMesh` is renderer-shaped — positions, normals, per-vertex albedo, alpha,
index ranges — and `NodeOutput` already carries display types. `display` depends on
`crystolecule`, so the extractor takes `&IsosurfaceData` directly: no twin, no
conversion, no back-edge in either direction.

**Rejected: the value in `structure_designer`, with an extractor-input twin in
`display`.** It keeps the colormap out of the domain crate, at the cost of a
second struct and a conversion at the call site — the
`GeometryVisualizationPreferences` shape, which exists in three copies
(`structure_designer` persisted, `api/` Dart-facing, `display` render-time) for
exactly that reason. Not worth it for a type whose whole content is a
`ScalarField` plus seven scalars.

`atomcad-display` has no `AGENTS.md` today; this change adds one and registers
it in the root `AGENTS.md`.

## Node: `isosurface`

| | |
|---|---|
| **Name** | `isosurface` |
| **Category** | `AtomicStructure` — groups with `import_cube`; `Geometry3D` would imply CSG composability it does not have |
| **Input pin 0** | `field: ScalarField` — required |
| **Input pin 1** | `color_field: ScalarField` — optional; presence switches coloring mode |
| **Input pin 2** | `level: Float` — optional; overrides the stored property when wired |
| **Output pin 0** | `surface: Isosurface` |

### Properties

| Property | Type | Default | Notes |
|---|---|---|---|
| `level` | `f64` | `0.02` | magnitude; extraction runs at `+level` **and** `-level` |
| `positive_color` | `DVec3` | `(0.20, 0.40, 0.90)` | 0–1 RGB, matching `apply_style` |
| `negative_color` | `DVec3` | `(0.90, 0.30, 0.25)` | |
| `alpha` | `f64` | `0.4` | `>= 1.0` → opaque fast path |
| `colormap` | `Colormap` | `BlueWhiteRed` | only when `color_field` is wired |
| `color_min` | `f64` | `-0.05` | only when `color_field` is wired |
| `color_max` | `f64` | `0.05` | |

`level` gets a pin as well as a property, following `free_sphere`
(`center`/`radius` are both) — wired value wins. The rule for later additions:
**parameters that change the geometry get pins; parameters that change only the
paint stay properties.**

`alpha` defaults to `0.4`, not `0.5`, because the two-pass draw composites two
layers: `1-(1-0.4)^2 = 0.64`, where `0.5` would give `0.75`.

`level` defaults to the MO amplitude convention, which is an order of magnitude
wrong for a density (`0.002`). Accepted: a field's meaning is not recoverable
from its numbers, so no fixed default serves both. The percentile-of-magnitude
alternative stays deferred.

To make level-picking possible, the `ScalarField` readout must carry the value
range, and it must do so **on the pin-hover path**.

> **Corrected in P3.** This section originally said "promote `to_detailed_string`
> ... the value range then shows on pin hover via existing machinery". That was
> wrong about the machinery, and the P1 test that was meant to lock it in
> asserted `to_detailed_string` while its own doc comment claimed to be about
> hover — so the suite agreed with the mistake. The tooltip renders
> `NodeView::output_pin_strings`, which is built from **`to_display_string`**;
> `to_detailed_string` is reachable only from the CLI/AI `evaluate_node
> --verbose` path. A GUI user could not see the value range at all.
>
> Two further facts the original text glossed over. **Value readouts exist only
> on output pins** — `_buildInputPin` passes no `outputString` — so the pin to
> hover is the upstream `import_cube`'s `field` output, never the `isosurface`
> node's own `field` input. And the range alone is not enough: it says *how big*
> the numbers are, not *what they are*, and an orbital amplitude and a density
> are drawn an order of magnitude apart. A `.cube`'s two comment lines are the
> only place that knowledge exists, and the loader was reading past them.

So the requirement is: `ScalarField`'s **`to_display_string`** is a summary block
— source description, grid, step, extent, box, value range with a signed /
non-negative tag, and memory — built by one helper that `to_detailed_string`
also calls (adding origin and axis vectors), so the two cannot drift.
`ScalarField` gains a `description()` trait method, defaulting to `None`, and
`SampledField::with_description` normalizes the producer's free text (trim, drop
blank lines, join, cap). One helper and one retained pair of comment lines,
versus an API getter plus a Dart readout for the same information.

### Editor

Registered in `node_data/node_data_widget.dart`: `level` field, two color
swatches with a **one-click swap button** (an orbital's global sign is
arbitrary, so flipping is how a user matches a published figure), `alpha`
slider, and colormap range/dropdown **disabled unless `color_field` is
connected**.

### Errors

| Condition | Behavior |
|---|---|
| `field` unconnected | evaluation error |
| `level <= 0` | evaluation error — both signs are always drawn, so the sign is not a user choice |
| extraction grid exceeds `isosurface_cell_budget` | error raised from the **display conversion** rather than `eval` (§Preferences); nothing is drawn, the node keeps its value |

**Nothing in this node warns, because there is no channel for it to warn
through.** Amber, non-blocking badges are a *validation*-time concept
(`ValidationError::warning`, `NodeDataError::warning`), and validation sees only
node data — never a connected field's contents, never the preferences. The
evaluation side has exactly one channel,
`NetworkEvaluationContext::node_errors: HashMap<NodeRef, String>`, and it
carries no severity: everything in it renders red.

So two conditions that would ideally warn are **deliberately silent**:

- **`level` exceeds the field's `value_range` magnitude.** The surface comes out
  empty, and an empty render is indistinguishable from a broken import.
  Mitigated by the promoted `to_detailed_string`: the value range is one hover
  away on the input pin, which is why that promotion is required rather than
  optional.
- **The surface field's `data_bounds` exceed the color field's.** `ScalarField`
  returns `0.0` outside its bounds, so a surface extending past the *color*
  field's box paints as neutral — plain white on a blue-white-red map, plausible
  and wrong. Easy to hit, since orbitals come from `.molden` and potentials from
  `.cube`. Explained in the reference guide instead (§Documentation touchpoints);
  do **not** add a second out-of-bounds convention to paper over it.

Both become warnings for free if an evaluation-time severity channel is ever
added — a separate change to the error subsystem
(`doc/design_error_management.md`), not something to fold in here.

## Value plumbing

`DataType::Isosurface` is an ordinary first-class pin type — no `Optional`
modifier, no subtyping, no implicit conversions.

**Rust core (`atomcad-structure-designer`):**

| Site | Change |
|---|---|
| `data_type.rs` `enum DataType` | new variant |
| `data_type.rs` `impl fmt::Display` | `=> write!(f, "Isosurface")` |
| `data_type.rs` `from_string` keyword table | `"Isosurface" => Ok(DataType::Isosurface)` |
| `text_format/node_type_introspection.rs` | add to the "no text literal representation" list |
| `evaluator/network_result.rs` `infer_data_type` | `=> Some(DataType::Isosurface)` — **`_ => None` arm, so omitting this compiles and silently mis-infers** |
| `evaluator/network_result.rs` `to_display_string` | short summary; exhaustive |
| `evaluator/network_result.rs` `to_detailed_string` | level, coloring mode, field dims |
| `evaluator/network_result.rs` `heap_bytes` | see below — a missing arm **is** a compile error, but a wrong one is silent |

**FRB boundary:** `APIDataTypeBase` variant, both conversion arms in
`structure_designer_api.rs`, then `flutter_rust_bridge_codegen generate`.

**Dart:** `data_type_input.dart`, `type_editor_dialog.dart`,
`node_widget.dart` `_apiDataTypeToString`, `node_network.dart` pin color.

Pin color: `ScalarField` is a soft red (`0xFFE57373`); `Isosurface` is its
rendered form, so a deeper red — `0xFFC62828`. Eyeball against the live palette
before committing.

**`infer_data_type` is the only site here with no compiler backstop.** Its
`_ => None` arm swallows a missing variant, and the result is a silently
mis-inferred pin type. Everything else in the table either fails to build or
fails a text round-trip. In particular `to_display_string` and
`NetworkResult::heap_bytes` (`network_result.rs`) are both **exhaustive matches
with no `_` arm**, so omitting either is a build failure — do not budget
vigilance for them.

**What `heap_bytes` does need care with is the arm's *contents*, which the
compiler cannot check.** `SampledField` holds a `Vec<f32>` of megabytes, and
`IsosurfaceData` can reach the same `Arc` twice — once as `field`, once as
`IsosurfaceColoring::Field { field }` — so the obvious
`surface.estimate_memory_bytes() + color.estimate_memory_bytes()` double-counts
the common case where a user paints a field with itself. **Count each distinct
`Arc::as_ptr` once within the value.** Sharing *across* results — the same field
also live as a `NetworkResult::ScalarField` elsewhere in the pass — stays
double-counted, unchanged from how the existing `ScalarField` deep tier already
behaves; fixing that would need a pass-wide pointer set, which the estimator has
no access to. Do not attempt it here.

### `NodeOutput::Isosurface(SurfaceMesh)`

`SurfaceMesh` is a new type rather than a `PolyMesh` reuse because **`PolyMesh`
carries no per-vertex color** — `poly_mesh_tessellator` assigns one `Material`
per mesh, and the colormap mode needs per-vertex albedo. `PolyMesh`'s
face-centric adjacency exists for `detect_sharp_edges`, which is the opposite
of what a smooth marching-cubes surface wants.

```rust
pub struct SurfaceMesh {
    pub positions: Vec<Vec3>,
    /// Outward normals from the field gradient.
    pub normals: Vec<Vec3>,
    /// Per-vertex albedo: solid in `Phase` mode, colormapped in `Field` mode.
    pub albedo: Vec<Vec3>,
    pub indices: Vec<u32>,
    /// Connected components, each a contiguous slice of `indices`. Ordering
    /// these back-to-front at draw time is what makes multi-lobe transparency
    /// correct.
    pub components: Vec<SurfaceComponent>,
    /// One value for the whole surface, straight from `IsosurfaceData::alpha`.
    /// It stays scalar *here* because a `SurfaceMesh` is one node's output.
    /// `scene_tessellator` reads it twice: to pick which of the two isosurface
    /// meshes this surface joins (§The opaque fast path), and to bake it onto
    /// every vertex it contributes, which is the only level at which
    /// per-surface alpha survives the merge (§Alpha is a vertex attribute, not
    /// a uniform).
    pub alpha: f32,
}

pub struct SurfaceComponent {
    pub first_index: u32,
    pub index_count: u32,
    /// World-space centroid; the sort key.
    pub centroid: Vec3,
}
```

## Extraction

In `atomcad-display/src/isosurface/`, taking `&IsosurfaceData` (a
`crystolecule` type — §Crate placement) and invoked from a new
`generate_isosurface_output` in `network_evaluator`, reached from an
**unconditional** `DataType::Isosurface` arm of `convert_result_to_node_output`
(§Preferences).
Hand-rolled — no workspace dependency provides it. `ScalarField: Send + Sync`,
so batch sampling *can* be parallel, but note that `rayon` is declared in
`[workspace.dependencies]` and is **not** currently a dependency of
`atomcad-display`; using it means adding that edge to the manifest. Do not add
it speculatively — only if the P3 step 6 timings ask for it.

### No extraction cache

**Do not add one.** Extraction is uncached in this design: every scene
generation re-extracts.

That is affordable because scene generation happens at user-action frequency —
network edits, selection changes, preference changes — not per frame.
`move_camera` touches only the renderer and never calls `generate_scene`, so
orbiting the camera costs no extraction at all.

The obvious analogy, `csg_conversion_cache`, does not transfer. It keys on
`GeoNode::hash()` and caches **recursively at every subtree**, so editing one
leaf of a 50-shape union reuses 49 cached sub-results. An isosurface has no
subtree structure — one field, one level, one extraction — so there is no
compositional reuse to harvest, only a top-level repeat when an unrelated edit
triggers a refresh.

Caching is **out of scope here, not an optimization left to the implementor's
taste.** It would need a sound key (an `Arc` address is not one: the allocation
can be freed and the address reused, so the cache would have to hold strong
references and pin megabytes of field data alive), an eviction policy, and an
invalidation story. P3 step 6 records the numbers that would say whether any of
that is warranted. **If they show a problem, that is a separate change with its
own design — not something to fold into this one.**

### Why marching cubes

- **Dual contouring is the wrong tool for a smooth field.** Its purpose is
  sharp-feature preservation via a per-cell QEF; an isosurface of a sum of
  Gaussians has no sharp features. What remains is DC's cost, plus vertices
  that are minimizers and can drift off the surface. MC vertices lie exactly on
  it by edge interpolation — a correctness property for something claiming to
  be "the `psi = level` surface".
- **MC's sliver triangles cost nothing here**, because normals come from the
  field's own gradient (§Normals and winding), not from face normals — so a
  near-degenerate triangle has no ill-conditioned cross product to compute.
- **DC's adaptivity has little to buy.** The `ScalarField` contract already
  caps useful resolution at the native grid, and a triangle count in the tens
  of thousands for an 80^3 field is not a problem.
- **Naive surface nets is the closer rival and loses on one point:** one vertex
  per cell cannot represent two sheets in the same cell, so it welds them,
  merging two lobes into one connected component and corrupting the union-find
  labeling the transparency sort depends on. The risk rises at coarse
  `isosurface_quality_multiplier` settings.

### Ambiguity: closed beats topologically optimal

The 15-case table produces **holes** on ambiguous face configurations. Normally
cosmetic; here fatal — an open surface has no back/front-face pairing, so the
two-pass transparency assumption collapses, and holes fragment connected
components so the sort keys go wrong too. Both present as "transparency is
buggy" rather than "extraction is buggy".

**Rule: the full 256-entry table, with ambiguous faces resolved *consistently*
— the same connectivity choice for complementary cases.** That guarantees a
closed manifold surface at the price of occasionally producing a tunnel where
there should be two sheets.

> **Guaranteed-closed beats topologically-optimal.** The renderer assumes
> closed surfaces and does not care about a spurious tunnel. It cares very much
> about a hole.

MC33's asymptotic decider resolves ambiguity *correctly* rather than merely
consistently, and can be added later if a tunnel is ever visible enough to
matter. Escape hatch if the case table becomes a time sink: **marching
tetrahedra** — six tets per cell, unambiguous and closed by construction, at
~2x the triangles plus a faint directional bias.

**The closure property, stated precisely** — because this is what P2 tests
rather than "looks closed":

> The set of segments a cell's patch leaves on one of its faces is a function of
> **that face's four corner signs alone**, and of nothing else in the cell.

Two cells sharing a face see the same four signs, so their patch boundaries on
it coincide edge for edge and cancel. Closure across the whole grid follows by
induction, for *any* field. This is the property to check, not a boundary-edge
count on one example: a table can be closed on every sphere anyone tries and
still violate it on a configuration a sphere never produces.

**Known limit, found in P2: the induction covers *interior* faces only.** A
level set that reaches the lattice's outer boundary is clipped there, and the
extractor — which marches exactly the lattice of §Resolution and adds no capping
layer — leaves it open. It is not reachable from a case-table bug; it is what a
box too small for the field looks like. Two consequences worth knowing:

- Every test fixture must pick a level above its field's **boundary maximum**,
  or it is asserting closure against a surface that legitimately has none. The
  P2 suite computes and records that number for each fixture it uses.
- If this ever bites a user — a density envelope whose `.cube` box was cut too
  close — the fix is a boundary cap pass, not a padding layer: padding would
  change the cell-count formulas of §Resolution that `isosurface_cell_budget` is
  defined against. Out of scope here.

### Degeneracies: the `psi == level` tie

Ties are not a corner case. A tie is any sample exactly equal to the level, and
they are routine: level `0.0` against the integer-valued grids of §P2 tables A
and B, and any level at all on a field with flat or quantized regions.

Note that the **extractor accepts level `0`** even though the *node* rejects
`level <= 0` (§Errors). The node's rule is about the two sign passes being
meaningless at zero; the extractor is a layer below it and is tested at zero
throughout. Do not push the node's validation down into the extractor.

**Rule: a corner is inside when `sign * psi(p) >= level`** — non-strict. This is
not a taste call: it makes the edge-interpolation denominator **provably
non-zero**. An edge is only cut when one endpoint is strictly below `level` and
the other is `>= level`, so `b - a != 0` by construction and
`t = (level - a) / (b - a)` can never be `0/0`. A strict test leaves the tie
producing NaN vertices, which propagate through normals and centroids into a mesh
that renders as nothing with no error anywhere.

**The tie's second effect is subtler and is the one to watch.** With the
non-strict rule a tied corner puts the vertex exactly *at* the grid corner. Up
to three cut edges of that cell meet there, and edge-keyed dedup gives each its
own vertex index — coincident in space, distinct in the index buffer. Union-find
over vertex indices then refuses to join them, so a surface passing exactly
through a corner **splits into spurious components**, which is a wrong
transparency sort and a wrong lobe count from an entirely correct case table.
Merge coincident vertices (position-keyed, quantized) **before** the union-find
pass, not after.

**Deterministic emission order.** Emit components sorted by their smallest vertex
index, and drive vertex dedup from a map with insertion-independent iteration.
Component *order* feeds nothing but the draw sort at runtime, so it is invisible
until a snapshot test flakes — and the seeded fuzz of §P2 table B is only
reproducible if identical input gives byte-identical output.

### Resolution: march the native lattice, not a spacing

**Extraction runs in the field's own index space, on `GridGeometry::axes` —
never on `GridGeometry::spacing()`.**

```rust
enum Lattice {
    /// `native_grid() == Some(g)`. Cell (i,j,k) is the parallelepiped spanned
    /// by `g.axes[a] / subdiv`, cornered at
    /// `g.origin + sum_a g.axes[a] * (index_a / subdiv)`.
    Native { grid: GridGeometry, subdiv: u32 },
    /// `native_grid() == None`. Axis-aligned cubes of
    /// `isosurface_fallback_spacing` filling `suggested_bounds()`.
    Fallback { bounds: FieldBounds, spacing: f64 },
}
```

with `subdiv = round(isosurface_quality_multiplier)` clamped to `>= 1`.

**Why not a spacing.** `GridGeometry::spacing()` returns three axis *lengths*,
and its own doc comment says it is "exact only for an axis-aligned grid, so a
consumer that must handle shear uses `GridGeometry::axes` directly". The
`.cube` format permits shear and `GridGeometry` was made shear-capable on
purpose. Rebuilding an axis-aligned lattice out of three lengths silently
rotates a sheared field's samples into the wrong places and reintroduces exactly
the eight-value trilinear blending the fidelity fast path exists to avoid — a
wrong surface, from a field that loaded without complaint. Marching index space
makes the axis-aligned case fall out unchanged and the sheared case correct,
for the price of carrying `axes` instead of three floats.

At `subdiv == 1` the `Native` cells are the grid's own cells and every corner
sample is a stored value read verbatim — the contract's fidelity fast path.
Making the multiplier an **integer subdivision** rather than a free divisor is
what preserves that: a non-integer factor puts every corner between stored
samples even at "quality 1.0". The preference stays an `f64` for UI continuity
and is rounded here; the editor should say so.

**Winding under shear.** The index-space→world map is
`m = mat3(axes[0], axes[1], axes[2])`. If `det(m) < 0` the axis triple is
left-handed and the map mirrors, so triangles emitted counter-clockwise in index
space come out clockwise in world space, inverting the front/back-face pairing
the two-pass draw depends on. **Check the determinant once per extraction and
flip triangle vertex order when it is negative.** Same class of bug — and the
same fix — as the csgrs `det < 0` winding correction in `structure_invert`;
`.cube` writers do emit left-handed axis triples.

**The `None` fallback** (every analytic field, i.e. all future Molden support)
samples `suggested_bounds()` at `isosurface_fallback_spacing`. This is the
consumer most likely to have been wrongly written against a grid, so it is the
one that proves the contract.

**Cell counts**, which are what `isosurface_cell_budget` is checked against:

| Lattice | Cells |
|---|---|
| `Native { grid, subdiv }` | `prod_a ((grid.dims[a] - 1) * subdiv)` |
| `Fallback { bounds, spacing }` | `prod_a ceil(bounds.size()[a] / spacing)` |

Cells span adjacent sample points, so `dims = [nx, ny, nz]` at `subdiv == 1`
yields `(nx-1)(ny-1)(nz-1)` — the node-centered bounds convention
(`GridGeometry::bounds`) makes this exact. The check is pre-flight: both rows
are computable before a single sample is taken.

### The two sign passes

Run the **same** extractor twice with a sign flag, comparing `sign * psi(p)`
against `level` and multiplying the gradient by `sign`: `+1` gives the positive
lobe in `positive_color`, `-1` the negative lobe in `negative_color`. Folding
the sign into the comparison is what keeps winding and normals consistent
without a second code path.

`value_range()` is `Option<(f64, f64)>`: when it is `Some((min, _))` with
`min >= 0.0`, skip the negative pass — a pure optimization, since it would find
no crossings anyway. When it is `None` (any analytic field), run both.

### Normals and winding

Per-vertex, `-sign * normalize(gradient)`. For the positive lobe the field
decreases outward; for the negative lobe it increases; the sign flag covers
both. `SampledField` overrides `gradient` with central differences on stored
samples, so these are exact with respect to the data.

Triangles must be **counter-clockwise viewed from outside**, consistent with
that normal. This is load-bearing: the two-pass draw is `cull_mode: Front` then
`Back`, so an inverted component draws near-before-far, and the failure looks
like slightly-off shading rather than obvious breakage. Orientation has two
independent chances to go wrong — the case table's own vertex order, and the
shear flip of §Resolution — and both land here.

### Connected components

Union-find over triangle vertex indices, then re-emit triangles grouped so each
component occupies a contiguous index range, with a centroid per component, in
deterministic order (§Degeneracies). The two sign passes are separate;
components are labeled within each.

Two dedup passes are needed, and skipping either corrupts the labeling in a way
that only shows up as a transparency bug. **Edge-keyed** dedup first, or every
triangle becomes its own component. **Position-keyed** merge second, or a
surface passing exactly through a grid corner splits into spurious components —
see §Degeneracies for why the tie rule makes that a routine occurrence rather
than a rarity.

## Rendering

### Why two-pass alone is not enough

The back-faces-then-front-faces draw assumes every view ray hits the surface
exactly twice, back face farther. That holds for a **convex** closed surface,
not merely a closed one. It fails three ways:

1. **Multiple components.** A p orbital is two lobes, a d orbital four, a real
   MO routinely 4–8. A ray through two lobes crosses four times (`F1 B1 F2 B2`)
   but two-pass emits `B1 B2 F1 F2`, compositing both front faces last. Wrong
   wherever lobes overlap on screen — which is what people look at orbitals to
   see.
2. **Non-convex single components** — a saddle, a torus, a lobe with a waist.
   Within a pass there is no ordering at all.
3. **No ordering between transparent pipelines.** Nothing writes depth, so the
   isosurface and the existing transparent-impostor pass composite in draw-call
   order; `transparent_sort.rs` cannot interleave triangles from another
   pipeline. An x-ray ghost atom inside a lobe is always wholly in front or
   wholly behind.

### The fix: sort components, two-pass within each

Components are spatially disjoint for lobes, so a centroid sort orders them
correctly essentially always. Draw order: components back-to-front by
view-space z; within each, back faces then front faces. Fixes mode 1; leaves 2
and 3.

**Known limit: nested components.** Two concentric shells have coincident
centroids, so their relative order is arbitrary and half the time wrong. Rare in
practice — it needs a field with an interior extremum inside a closed shell — and
the fix is a per-component depth range rather than a centroid, which is not worth
building until something produces one. Recorded because the P2 concentric-shells
test asserts the component *count*, and a reader would otherwise expect it to
assert the order too.

The sort happens **at draw time in the renderer**, reordering *draw calls*, not
indices — so unlike `transparent_sort.rs` there is no index-buffer rewrite. N
is single digits; it is free.

The sort is over **one flat pool of components spanning every transparent
surface on screen**, never per surface. The singleton-mesh model (§Alpha is a
vertex attribute, not a uniform) merges all of them into `isosurface_transparent_mesh`,
so the `SurfaceComponent` ranges arrive already pooled and which node a component came
from is not recorded — nor should it be, since two orbitals on screen
interpenetrate exactly the way two lobes of one orbital do.

The sort must run **per moved frame**, not at tessellation time — the same
trigger `transparent_sort.rs` uses, and what P4 manual step 3 checks.

### Pipelines and blend mode

Two new pipelines, identical but for `cull_mode`:

```rust
blend:                Some(BlendState::ALPHA_BLENDING),
cull_mode:            Some(wgpu::Face::Front),   // pass 1
// then                Some(wgpu::Face::Back),   // pass 2
depth_write_enabled:  false,
depth_compare:        wgpu::CompareFunction::Less,
```

`ALPHA_BLENDING` is non-premultiplied source-over, the same blend both existing
transparent pipelines use. Depth test on, so opaque geometry (drawn first with
depth writes) occludes correctly; depth write off, or the first transparent
fragment kills the surface's own second layer.

These two serve `isosurface_transparent_mesh` only. The opaque mesh reuses the
existing `triangle_pipeline` unchanged — no third pipeline is added (§The opaque
fast path).

### Alpha is a vertex attribute, not a uniform

`mesh.wgsl` ends `return vec4<f32>(color, 1.0);` — opacity is hardcoded.
Per-vertex albedo already exists on `Vertex`, so the colormap needs nothing;
alpha is the only missing channel. It goes on the **vertex**:

- `Vertex` (`atomcad-renderer/src/mesh.rs`) gains a trailing `alpha: f32`, and
  `Vertex::desc()` gains one `Float32` attribute at `shader_location: 5`,
  offset `size_of::<[f32; 11]>()`
- `Vertex::new(position, normal, material)` sets `alpha: 1.0`, so all ~51
  existing call sites and every opaque consumer are untouched; the isosurface
  tessellator uses a new `Vertex::new_translucent(.., alpha)`
- `mesh.wgsl` interpolates it through `VertexOutput` and returns
  `vec4<f32>(color, in.alpha)`

**Rejected: `ModelUniform` (group 1).** This is the obvious placement — alpha is
one number per surface, and a uniform is where one number per surface belongs —
and it does not work, because *the renderer has no per-surface mesh.*
`tessellate_scene_content` builds a **fixed set of singleton meshes**, one per
pipeline (`main_mesh`, `atom_impostor_mesh`, `transparent_impostor_mesh`, …),
each merging every displayed node, and `update_all_gpu_meshes` uploads one
`GPUMesh` — hence one `ModelUniform` — per pipeline. "Per-mesh" therefore means
**per-pipeline**, not per surface. Two displayed `isosurface` nodes would share
one alpha, and the opaque fast path makes the mixed case routine rather than
exotic: one opaque density envelope plus one transparent orbital is a normal
thing to want on screen at once. Per-vertex alpha varies at any granularity for
free, costs 4 bytes per vertex on a mesh of tens of thousands, and needs no
dynamic-offset uniform machinery.

**This also retires the padding gotcha.** `ModelUniform` stays two
`mat4x4<f32>` = 128 bytes, already 16-byte aligned, so the WGSL layout keeps
matching the `#[repr(C)]` struct. Had alpha gone in as a bare trailing `f32` it
would have broken that boundary silently — issue #269 was exactly this bug in
`CameraUniform`: wrong output, no error. Vertex attributes have no such rule,
which is a second reason to prefer them here.

### The opaque fast path, and why there are two isosurface meshes

`alpha >= 1.0` routes a surface to the opaque path: one draw, `cull_mode: Back`,
depth write on, no sort. Compare `>=`, not `==`, so a slider landing on
`0.9999999` still takes it.

**This is a per-surface routing decision made in `scene_tessellator` at merge
time, and it forces the scene to carry two isosurface meshes rather than one:**

| Mesh | Contents | Drawn by |
|---|---|---|
| `isosurface_opaque_mesh` | every displayed surface with `alpha >= 1.0` | the existing opaque `triangle_pipeline`, with the rest of the opaque geometry. Its vertices carry `alpha = 1.0`, so the shader change is a no-op for it |
| `isosurface_transparent_mesh` | every displayed surface with `alpha < 1.0` | the two culled pipelines, after all opaque content, with the component sort |

The split is not book-keeping, it is forced: the singleton-mesh model gives one
pipeline per mesh, so a surface cannot choose a pipeline without choosing a mesh.
A single merged mesh would mean either no fast path at all, or an opaque surface
drawn through the transparent pipelines with depth writes off — which costs it
its own self-occlusion.

P3 builds **only the opaque mesh**, which is what lets it ship before any
transparency work and leaves a permanent escape hatch — a picture whose
correctness is not in question. P4 adds the second.

### Not built

- **GPU volume raymarching.** The better eventual answer (free isolevel slider,
  resolution-independent lobes). A raymarcher wants a dense grid from
  `sample_batch`, which `IsosurfaceData` already carries, so it would be a new
  consumer of the same value, not a change to it.
- **Weighted-blended OIT.** The only option that fixes failure mode 3, for the
  ghost impostors too, eventually making `transparent_sort.rs` deletable. Needs
  MRT plus a resolve pass; `renderer.rs` currently builds two passes with one
  color attachment each, so it is the only option that changes the renderer's
  structure rather than adding to it. Deferred until the evidence says
  component sorting is insufficient.
- **Additive blending.** Order-independent and nearly free at the pipeline
  level, but PBR shading under additive blending needs replacing with a Fresnel
  rim term, and it produces a glow rather than a surface — a different picture,
  not a cheaper one.

## Preferences

Plumbing: `crates/atomcad-structure-designer/src/preferences.rs` →
`rust/src/api/structure_designer/structure_designer_preferences.rs` (then
codegen) → `lib/structure_designer/preferences_window.dart`, in the **Geometry
Visualization** section.

| Preference | Type | Default | Purpose |
|---|---|---|---|
| `isosurface_quality_multiplier` | `f64` | `1.0` | **integer** subdivision of the native grid, rounded and clamped to `>= 1` (§Resolution); `2` halves the step. Values below `1` do not coarsen — a sampled field's own grid is the floor |
| `isosurface_fallback_spacing` | `f64` | `0.15` Å | spacing when `native_grid()` is `None` |
| `surface_transparency_mode` | enum | `ComponentSorted` | §Comparison modes; added in P4 |
| `isosurface_cell_budget` | `usize` | `16_000_000` | ceiling on marching-cubes **cells**; refuses rather than hanging the UI |

**`isosurface_cell_budget` counts cells** on the extraction lattice — not
triangles and not bytes — by the two formulas in §Resolution. Cells are the only
one of the three knowable *before* doing the work: both formulas are pure
functions of the `Lattice`, so the check is pre-flight and costs nothing.
Triangle count is not known until extraction has already run.

For a sampled field the native grid is a natural ceiling and the budget only
bites at a high quality multiplier — `16_000_000` clears `subdiv` 2 on a
standard 80^3 cube (158^3 = 3,944,312 cells) and refuses `subdiv` 4
(316^3 = 31,554,496). For an
analytic field there is no natural ceiling at all, so the budget is the only
guard. **The default is a starting value, not a measured one; revisit it with
the P3 step 6 timings.**

**The breach must be reported from the display conversion**, by inserting into
`context.node_errors` — *not* from `get_data_error`, because node data cannot
see preferences any more than `eval` can, so it cannot know the cell count.

That works, but only because of an ordering that is easy to break:
`generate_scene_scoped` clears `context.node_errors` at the top of the pass,
runs evaluation, then runs `convert_result_to_node_output` for pin 0 and for
each extra pin, and only **after all of that** snapshots
`node_errors: context.node_errors.clone()` into the `NodeSceneData`. An insert
from the display conversion therefore reaches the scene, and from there
`get_all_node_errors` → `harvest_eval_errors` → the red node badge, the error
panel entry and jump-to-node. Five details, none of them optional:

- **Its own conversion function.** Add `generate_isosurface_output` beside
  `generate_explicit_mesh_output` and reach it from an **unconditional**
  `DataType::Isosurface` arm of `convert_result_to_node_output`. Do not route
  through `generate_explicit_mesh_output`: that one is CSG-shaped and is reached
  only from the `GeometryVisualization::ExplicitMesh` branches, so a user in
  `SurfaceSplatting` mode would see no isosurface at all. The arm already has
  `&mut context` in scope.
- **Key with `context.node_ref(node_id)`**, never a bare `u64`. The scope pops
  happen *after* the conversion, so `eval_scope_path` is still correct there and
  an `isosurface` inside a HOF body keys to the right address.
- **`entry().or_insert_with(..)`, not `insert`.** The map holds one string per
  node. If the node already failed in `eval`, that error is the root cause and
  must win.
- **Record no origin link.** Leaving `node_error_origins` empty for this node is
  what makes `resolve_root_cause` treat it as a root cause — correct, since
  nothing upstream caused a preference breach.
- **Name all three moving parts in the message**, because none of them is a node
  property the user can see from the node: the cell count, the budget, and the
  preference to lower. For example — `extraction grid is 31,554,496 cells, over
  the 16,000,000 budget; lower isosurface_quality_multiplier (currently 4.0) or
  raise isosurface_cell_budget`.

It is an error that is **reported but not poisoning**: cone-poisoning is decided
at validation time, and by the display conversion the `Isosurface` value has
already flowed. That is the behavior wanted here — the value is fine, only the
picture is missing. Freshness needs no extra machinery either: the
clear-at-top-of-pass means the error disappears on the first pass where the
budget fits.

**Rejected: clamping the multiplier down to fit the budget.** Silently handing
someone a coarser surface than they asked for is a worse failure than refusing,
and it would quietly falsify P3 manual step 5, whose whole point is that the
multiplier visibly controls resolution.

### Comparison modes and their expiry

```rust
pub enum SurfaceTransparencyMode {
    /// One draw, no culling. The control.
    SinglePass,
    /// Back then front faces, components unordered.
    TwoPass,
    /// Components back-to-front, two-pass within each. Default.
    ComponentSorted,
}
```

These answer the sibling document's open question — *is two-pass transparency
good enough, or is a per-triangle sort required?* — on the same scene at the
same camera. Nearly free: once components are contiguous ranges, `TwoPass` is
"skip the sort" and `SinglePass` is "one draw, no culling".

**They are scaffolding.** Delete `SinglePass` and `TwoPass` once this document
records (a) whether `ComponentSorted` beats `TwoPass` on a multi-lobe signed
field, and (b) whether either is good enough that OIT is unnecessary. Without
a stated removal criterion they become permanent options every future renderer
change must keep working.

**Only informative on the right subject.** A density envelope is one
near-convex component and looks identical in all three modes. Use a d orbital,
or a p orbital viewed down an axis where the lobes overlap.

## Implementation plan

Each step ends green: `cargo test -j 4`, `cargo clippy`, `flutter analyze`. Per
`AGENTS.md` the Flutter smoke test is human-only and never run by an agent.
Fixtures extend `scripts/make_cube_fixtures.py`.

### P1 — value plumbing

**Work:** `IsosurfaceData`, `IsosurfaceColoring`, `Colormap` in
`atomcad-crystolecule/src/field/isosurface.rs`, registered in `field/mod.rs`;
`DataType::Isosurface` and every
touchpoint above plus codegen and the four Dart sites;
`NetworkResult::Isosurface` with its four arms; promote
`to_detailed_string` for `ScalarField`.

| Test | Asserts |
|---|---|
| `DataType` text round-trip | `from_string` and `Display` agree |
| `APIDataType` round-trip, both directions | catches a missed FRB arm |
| `infer_data_type` on a `NetworkResult::Isosurface` | `Some(DataType::Isosurface)` — **the one site with no compiler backstop** |
| `heap_bytes` with the same `Arc` as surface and color field | counted once, not twice |
| `to_detailed_string` on a `ScalarField` | reports dims and value range |
| Existing registry-validation suite | stays green |

### P2 — marching cubes extractor (backend only)

**Work:** `isosurface/extract.rs`; the 256-entry table with consistent
complementary-case resolution and edge-keyed vertex dedup; the non-strict tie
rule and the coincident-vertex merge; sign-folded comparison, gradient normals,
winding, union-find components in deterministic order; the `Lattice` resolution
policy — index-space marching on `axes`, integer `subdiv`, the `det < 0` winding
flip, the `None` fallback; cell-budget check; colormap sampling for both
`IsosurfaceColoring` arms.

**Status: done.** The case table is *generated* from the face-locality rule
rather than transcribed — `face_segments` is that rule, and `case_triangles`
stitches its six faces into loops and fans them — so closure and
complementary-consistency hold by construction and the 256-mask tests confirm
rather than establish them. Two findings are recorded above and in
`crates/atomcad-display/src/AGENTS.md`: the boundary-clipping limit (§Ambiguity),
and that the coincident-vertex merge must be **quantized with a neighbour probe**
rather than keyed on exact bits, because `SampledField::sample` round-trips a
lattice corner through `inv_basis` and returns a tied value a few ULPs off, so
each cut edge places its own vertex a hair away from the corner in a different
direction.

This is the largest piece of genuinely tricky code in the design and the only
one that is fully testable in isolation — no node, no scene, no GPU — so it
carries the weight of the test plan. **The end-to-end checks on smooth analytic
fields (table C) are the weakest part of that plan, not the strongest.** A
sphere exercises perhaps a third of the 256 cases and systematically the easy
third; every configuration that produces a hole is one a smooth blob rarely
generates. Tables A and B are where the coverage actually comes from.

**Housekeeping.** Tests live in `crates/atomcad-display/tests/display/` and
**must be registered in `tests/display.rs`** with a `#[path]` module — an
unregistered file silently never compiles or runs, the same failure mode the
workspace `default-members` comment warns about. `atomcad-display` currently has
**no `[dev-dependencies]` at all**; the snapshot rows below need
`insta = { workspace = true }` added to its manifest.

**Write the invariants as shared helpers, not per-test assertions:**
`assert_closed_manifold`, `assert_on_isosurface(mesh, field, level, sign)`,
`assert_outward_winding`, `assert_components_partition_indices`. Every table
below then applies all four for free, and the fuzz rows become three lines each.
The earlier draft named closedness once, against the sphere; it belongs on
everything.

**A — the case table, directly and exhaustively.** No field, no grid, no
tolerance-fiddling: one unit cell, corner values `+1` / `-1` from a bitmask,
level `0`. All 256 run in microseconds.

| Test | Asserts |
|---|---|
| All 256 masks, vertex placement | every emitted vertex lies on a cell edge whose two corners classify oppositely; no vertex on an uncut edge |
| All 256 masks, patch validity | no degenerate (zero-area) triangle; every interior edge shared by exactly 2 triangles |
| All 256 masks, **face-locality** | the segments the patch leaves on each of the 6 faces are a function of **that face's 4 corner signs alone** — the closure property from §Ambiguity, and the only test that proves closure for fields nobody thought to try |
| All 256 complementary pairs | mask `m` and its complement give the **same geometry with reversed winding** — the "resolved consistently" rule, until now stated and unverified |
| Adjacent-cell pairs, all 2^12 corner combinations | the union of two face-sharing cells has **zero boundary edges on the shared face**. Implied by face-locality; keep it as the concrete corollary, because it fails legibly |

**B — randomized closure fuzz.** The highest-value rows in the plan: they reach
combinations A and C both miss — asymmetric values, levels away from zero,
near-ties — and each is three lines once the helpers exist.

| Test | Asserts |
|---|---|
| ~2000 fixed seeds, random `5^3` grid, random level in range | closed manifold, even Euler characteristic, every vertex on the isovalue, components partition the index buffer |
| Same, values drawn from `{-1, 0, +1}` at level `0` | ties on most corners — the degeneracy generator, and what catches the spurious-component split of §Degeneracies |
| Same, all-equal grid at exactly the level | empty mesh, no NaN, no panic |
| Determinism | the same seed twice gives byte-identical positions, indices and component order |

Seeds are a fixed list, never `rand::random()`: a fuzz failure that cannot be
replayed is a flake, and this suite has to be able to fail loudly in CI.

**C — analytic fields.** End-to-end plausibility, now backed by A and B instead
of carrying the argument alone.

| Test | Asserts |
|---|---|
| Analytic sphere (`r - R`), level 0 | every vertex within tolerance of `R`; triangle count in band; **plus all four shared invariants** |
| Same, normals | parallel to the radial direction, pointing **outward** |
| Same, Euler characteristic | `V - E + F == 2` |
| **Torus**, genus 1 | one component with `V - E + F == 0` — the sphere's `== 2` catches neither a handle welded shut nor a handle invented |
| **Two concentric shells** | two components. Also the documented limit of the centroid sort: the centroids coincide, so the draw order between them is arbitrary (§The fix) |
| Sampled field, normals | agree with **central differences on the stored samples**, not with the analytic gradient — `SampledField` overrides `gradient`, and on a coarse grid the two differ |
| Analytic 2p_z, level `L` | exactly two components, centroids on opposite sides of the nodal plane; **both passes land on their own `±L`** |
| Same, colors | `+z` gets `positive_color`, `-z` gets `negative_color` |
| Two same-sign lobes under one cell apart | still two components — the surface-nets failure mode |
| Non-negative field | one component; negative pass short-circuits |
| Level above the field's max magnitude | empty mesh, no panic |
| Colormap on a linear color field | albedo varies monotonically; out-of-range clamps |

**D — lattice and resolution.**

| Test | Asserts |
|---|---|
| Field with `native_grid() == None` | succeeds at the fallback spacing over `suggested_bounds()` |
| **Sheared** grid, analytic sphere sampled onto it | vertices still within tolerance of `R` — the assertion that fails if `spacing()` was used instead of `axes` |
| Same, closed and correctly wound | shear must not open the surface or invert it |
| **Left-handed** axis triple (`det(m) < 0`) | winding matches the outward normal — the flip is applied |
| `subdiv == 1` on a sampled field | every cell corner equals a stored sample exactly, no interpolation |
| `subdiv == 2` | cell count is `prod (dims-1)*2`; surface stays closed |
| `quality_multiplier` of `0.5` and `2.4` | clamp/round to `subdiv` `1` and `2` |
| Grid with an axis of `dims == 1` | zero cells, empty mesh, no panic — `SampledField::new` accepts it, only `0` is rejected |
| Cell budget exceeded, both lattice kinds | refuses **before allocating**; message names the cell count and the offending multiplier |

**E — regression snapshots.** `insta`, matching the `node_snapshots` convention.
Snapshot a *summary* — vertex / triangle / component counts, bounding box,
per-component centroid, a positions checksum — never the full vertex list, which
is unreviewable in `cargo insta review` and rewrites wholesale on any harmless
reorder.

| Test | Asserts |
|---|---|
| `water.cube` fixture at two levels | summary stable across refactors |
| The 2p_z field, both sign passes | summary stable, per component |

### P3 — node, editor, opaque rendering

**The first user-visible milestone**, deliberately opaque-only.

**Work:** node data, `eval`, registration; `SurfaceMesh`,
`NodeOutput::Isosurface`, `generate_isosurface_output`; the `scene_tessellator`
arm building **`isosurface_opaque_mesh` only**, drawn by the existing
`triangle_pipeline`; `isosurface_editor.dart` and its widget entry; the three
extraction preferences (`surface_transparency_mode` is P4); reference guide.

**Status: done**, with three decisions worth recording because a reader of the
plan above would expect otherwise.

- **`isosurface_opaque_mesh` is `main_mesh`.** The design's table says the opaque
  mesh is drawn by "the existing opaque `triangle_pipeline`, with the rest of the
  opaque geometry" — and the singleton-mesh model gives exactly one mesh per
  pipeline, so a *separate* opaque isosurface mesh would be a second `GPUMesh` on
  the same pipeline with the same uniforms: pure plumbing, buying nothing. P3
  therefore appends surfaces to `main_mesh`. The two-mesh split of §The opaque
  fast path becomes real in P4, when the *transparent* mesh arrives with its own
  culled pipelines and the routing decision has somewhere to route to. Nothing in
  the renderer changed in P3, which is what "a permanent escape hatch" was meant
  to buy.
- **All three input pins are declared now, and `color_field` is inert.** Pin
  order is `.cnnd` contract: inserting `color_field` ahead of `level` in P5 would
  silently re-target every saved wire. So the node declares
  `field` / `color_field` / `level` in the design's order from the start, and P3
  simply never reads pin 1 — `eval` always emits `IsosurfaceColoring::Phase`.
  This is the same rule the design already applies to `alpha` (stored, editable,
  no render effect yet, and the editor says so), extended to a pin.
- **The `ScalarField` hover readout was rebuilt, and §Errors corrected.** The
  design's stated prerequisite for level-picking — "the value range shows on pin
  hover" — was not true: the tooltip is fed by `to_display_string`, which said
  only `ScalarField 33x33x33`. Fixing it properly meant a summary block shared
  by both readouts, and a new `ScalarField::description()` carrying the `.cube`
  comment lines the loader used to discard, because the range says how big the
  numbers are and only the description says what they are. See the corrected
  §Errors.
- **`InputPinView` gained a `connected: bool`.** The editor's "colormap controls
  disabled unless `color_field` is connected" needs pin connectivity in Dart, and
  `build_node_view` already computed the set for `get_subtitle`. Surfacing it per
  pin is additive and reusable; the alternative — walking
  `model.nodeNetworkView.wires` in the editor — additionally needs the node's
  scope, which the property panel would have to thread in.

**Not done: step 6 of the manual walkthrough.** The extraction and refresh
timings require the running application; they are listed with the rest of the
manual steps for the maintainer, and §No extraction cache still stands
unmeasured until they are recorded here.

The `alpha` property is stored, serialized and editable in P3 but **has no
render effect yet** — every surface goes to the opaque mesh regardless. P4 adds
the transparent mesh, the routing between them, and the vertex attribute that
carries alpha to the shader (§The opaque fast path). Say so in the editor
tooltip rather than hiding the control, so the `.cnnd` written in P3 is already
correct.

| Test | Asserts |
|---|---|
| Node eval on a fixture | `NetworkResult::Isosurface` with field and level from properties |
| `level` pin wired | overrides the stored property |
| `level <= 0` | evaluation error |
| `level` above the field's range | still produces a value; **no** error entry — the empty surface is silent by design (§Errors) |
| `.cnnd` round-trip | all properties survive, `Colormap` included |
| Result → `NodeOutput` | expected component count |
| Cell budget exceeded on a displayed node | after `generate_scene_scoped`, the scene's `node_errors` holds one entry at `NodeRef::top(id)` naming cells, budget and multiplier, and the output is `NodeOutput::None` |
| Same, node inside a HOF body | the entry is keyed at the **scoped** `NodeRef`, not the bare id |
| Same, node whose `eval` already errored | the eval error survives — the budget message does not overwrite it |

**Manual walkthrough**

1. `import_cube` → `water.cube` → `isosurface`, alpha `1.0`. Expect an opaque
   blob enclosing the molecule.
2. Display the `molecule` pin too. Expect surface and atoms *registered* — a
   1.9x mismatch means a Bohr conversion is wrong.
3. Scrub `level` up. Expect monotonic shrinking, then an empty viewport with
   **no** badge — the silence is the designed behavior (§Errors), and the pin
   hover is where the value range is read.
4. On a signed orbital, expect two lobes in two colors; the swap button
   exchanges them and changes nothing else.
5. Change `isosurface_quality_multiplier`. Expect the surface to re-extract at
   the new resolution while **no node data changes** — nothing dirties, no undo
   entry appears, the `.cnnd` is untouched. That is the architectural claim
   made visible: resolution lives outside the value.
6. **Time one extraction** at native resolution on a real 80^3 cube (the
   existing per-node profiler covers the conversion), then edit an unrelated
   node in the network and judge whether the refresh visibly hitches. **Record
   both numbers in this document.** They are the input to any future decision
   about caching, which §No extraction cache places out of scope for this
   design.

### P4 — transparency

**Work:** the `Vertex` alpha attribute and the `mesh.wgsl` return; the
`isosurface_transparent_mesh` and the `alpha >= 1.0` split in
`scene_tessellator`; the two culled pipelines; the per-frame component sort over
the pooled ranges; `surface_transparency_mode` and its three modes.

**Status: done**, with four notes, three of them corrections to the plan above.

- **Three pipelines, not two.** §Pipelines and blend mode names two, differing
  only in `cull_mode` — but `SinglePass`, described separately as "one draw, no
  culling", needs a `cull_mode: None` pipeline of its own. The two-pass pair is
  as specified; the control mode adds a third,
  `create_surface_transparent_pipeline` being called once per cull mode. All
  three go away together if the comparison modes are deleted on their stated
  criterion, leaving the two the design names.
- **`0.9999999` does not survive the `f32`.** §The opaque fast path justifies
  `>=` over `==` with "a slider landing on `0.9999999` still takes it". It does
  not: `alpha` is an `f32`, the nearest representable value to `0.9999999` is
  `0.99999994`, and that is below `1.0` under either comparison. The surface
  takes the transparent path — harmlessly, since at that opacity it is visually
  solid, but the example is not the reason for the operator. **What `>=`
  actually buys is the other side of `1.0`**: a value that rounded *up* past it
  on the way from the node's `f64` property to the mesh's `f32` would fail `==`
  and be drawn as a transparent surface with depth writes off, losing its own
  self-occlusion. The test asserts that, not the example.
- **The transparent surfaces draw after the ghost impostors.** Failure mode 3
  says nothing orders fragments between two transparent pipelines, so this is a
  choice, not a derivation: surfaces last means the ghost atoms are seen
  *through* the membrane, which is what a surface enclosing a structure is
  normally for. A ghost genuinely in front of the surface is still composited
  behind it. That is the picture P4 manual step 5 is meant to record.
- **`TransparentSurfaceMesh` is a wrapper, not a field on `Mesh`.** The pooled
  `SurfaceComponentRange` list has to travel with the merged mesh, and putting
  it on `Mesh` itself would hang a permanently-empty `Vec` on the main,
  lightweight and gadget meshes while saying nothing about which of them the
  sort applies to. `atomcad-renderer` declares its own
  `SurfaceComponentRange` rather than naming `atomcad_display`'s
  `SurfaceComponent`, since it sits below `atomcad-display` in the DAG.

**Not done: the manual walkthrough** — every step needs the running
application, including step 6, whose expiry answers are what retire the two
comparison modes. Those modes therefore stay, and §Comparison modes and their
expiry stays unanswered, until the maintainer records them here.

| Test | Asserts |
|---|---|
| `Vertex::new(..)` | `alpha == 1.0` — every existing opaque consumer unchanged |
| `Vertex::desc()` | attribute count and the trailing offset match `size_of::<Vertex>()`; stride is `size_of::<Vertex>()` |
| `size_of::<ModelUniform>() == 128` | it did **not** grow — the regression guard on the rejected placement |
| Component sort | synthetic centroids + view matrix come back farthest-first |
| Sort across **two** surfaces | components from different `isosurface` nodes interleave by depth, not by node — they share one pooled range list |
| Two transparent surfaces at different alphas | each keeps its own in the merged mesh — the failure the vertex attribute exists to prevent |
| `alpha` of `1.0`, `0.9999999`, `0.999` | the first two land in `isosurface_opaque_mesh`, the third in `isosurface_transparent_mesh` — assert the mesh, not pixels |

**Manual walkthrough**

1. Alpha `0.4` on a **d-type or overlapping-p** orbital — not a density
   envelope, which cannot distinguish the modes.
2. Cycle all three modes at a fixed camera. Expect `SinglePass` to misorder
   visibly, `TwoPass` to fix within lobes but not between them,
   `ComponentSorted` to be correct.
3. Orbit in `ComponentSorted`. Expect the ordering to stay correct from every
   angle; a sort running only at tessellation time looks right from the
   original viewpoint and wrong from others.
4. Display **two** `isosurface` nodes at once, at different alphas — say an
   opaque envelope and a `0.4` orbital, then two transparent orbitals that
   overlap. Expect each to keep its own alpha and the lobes to interleave by
   depth. This is the singleton-mesh trap (§Alpha is a vertex attribute, not a
   uniform) made visible; one surface on screen cannot detect it.
5. Show ghost atoms and the surface together. Expect failure mode 3. Record
   what it looks like — that is the input to the OIT decision.
6. **Record the expiry answers here** and delete the losing modes.

### P5 — color field and colormap — **DONE** (walkthrough pending)

**Work:** `color_field` wiring and `IsosurfaceColoring::Field`; per-vertex
colormap sampling; editor controls; reference guide — including the
mismatched-bounds white band, which the guide explains because the node cannot
(§Errors).

**What was actually left to do**, since P1–P4 had already built more of this
phase than the plan assumed: only the node's `eval` still hardcoded
`IsosurfaceColoring::Phase`. The value type, the `color_field` pin declaration,
the node data (`colormap` / `color_min` / `color_max`) with its serde defaults
and text-format spelling, the extractor's `Field` arm, `sample_colormap`, the
editor's colormap group with its `colorFieldConnected` gating, and the `.cnnd`
and text-format round-trip tests were all in place. P5 wired pin 1 into the
value and added the four test rows below.

`eval` reads pin 1 with `evaluate_arg` (not `evaluate_arg_required`):
`NetworkResult::None` is the unwired signal and selects `Phase`. It is
evaluated **in pin order**, ahead of `level`, which fixes error precedence when
both are bad. The domain is passed through as `f64` — it is compared against
`sample()` output — and an inverted domain is deliberately *not* rejected.

One thing this phase found and did not change: §Colormap orientation.

| Test | Asserts | Where |
|---|---|---|
| Color field wired | `Field` arm; and *removing* the wire reverts to `Phase` — the direction that regresses silently, since the domain stays in the `.cnnd` | `isosurface_node_test.rs` |
| `color_min` / `color_max` properties | reach the value's `range` unnarrowed, and the field on pin 1 (not pin 0) is the one carried — the mapping itself is covered in P2 | `isosurface_node_test.rs` |
| An inverted domain | carried verbatim, not normalized and not rejected — pins the "no red badge mid-typing" behaviour that §Colormap orientation depends on | `isosurface_node_test.rs` |
| Signed surface with a color field | still two components, both painted per-vertex across the ramp rather than one flat color each; geometry unchanged from the `Phase` extraction | `isosurface_extract_test.rs` |
| Bounds exceeding the color field's | still drawn, **no** error entry, and the overhang is *exactly* the ramp midpoint — all three together, since each alone passes for the wrong reason | `isosurface_node_test.rs` |

**Manual walkthrough**

The three files this needs are written by
`python scripts/make_cube_fixtures.py manual` into `sample_data/cube/`; what
they model, and what they deliberately do not, is
`design_scalar_fields.md` §The colour-map pair.

1. `water_density.cube` into `field` at level `0.002`, `water_esp.cube` into
   `color_field` with range `-0.08 .. 0.08`. Expect a diverging map: **blue**
   over the lone-pair side of the oxygen, **red** caps on the two hydrogens,
   white around the girdle. Note that this is the ramp read literally — blue at
   `range.0`, red at `range.1` — and so is inverted with respect to the
   published ESP convention, which colours electron-rich regions red. See
   §Colormap orientation. Narrow the range to `±0.05` and expect both ends to
   saturate, which is also what a published map does and is why the range is a
   control rather than a fitted value.
2. Swap `color_field` to `water_esp_small.cube`, whose box is half the size
   while the surface is unchanged. Expect a visible white band where the
   envelope leaves that box — it reaches past ±1.5 Å along each O–H — and
   **no** badge explaining it. Check that the reference guide's description
   matches what you see; that page is the only thing standing between a user
   and a plausible-but-wrong picture.
3. Repeat through `atomcad-cli` for the headless path.

**Deliverable: closes the scope of this document.**

## Documentation touchpoints

- `doc/reference_guide/nodes/atomic.md` — the node (P3, updated P5). Must
  carry the two conditions the node cannot report (§Errors): a `level` above the
  field's range renders nothing, and a color field smaller than the surface
  paints the overhang neutral. Both are silent in the UI, so this page is where
  a user finds out.
- `doc/reference_guide/node_networks.md` — `Isosurface` pin type and color (P1)
- `doc/reference_guide/ui.md` — the four preferences (P3, P4)
- `crates/atomcad-display/src/AGENTS.md` — **new file**: module map,
  Ångström-in/out invariant, outward-winding rule; register it in the root
  `AGENTS.md`
- `crates/atomcad-crystolecule/src/AGENTS.md` — `field/` gains a second module;
  record that the isosurface *specification* lives beside `ScalarField` while
  the extracted mesh lives in `atomcad-display` (P1)
- `doc/testing.md` — the required-test set for any surface extractor: winding,
  **closedness**, and the **face-locality** property (§Ambiguity), noting that a
  holed surface passes every per-vertex check and that a per-example closedness
  count does not imply closure in general. Also record the seeded-fuzz pattern
  from P2 table B, which is reusable by any future extractor
- this document — the expiry answers (P4)
