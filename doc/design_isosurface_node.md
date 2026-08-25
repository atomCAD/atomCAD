# Design: the `isosurface` node — displaying scalar fields

Companion to `doc/design_scalar_fields.md`, which builds ingestion (the
`ScalarField` contract, the `.cube` loader, `DataType::ScalarField`,
`import_cube`, `sample_field`) and defers visualization. This document is the
visualization half: the node that turns a `ScalarField` into a rendered
**isosurface**, optionally painted with a second field's values.

Assumes P1–P4 of `design_scalar_fields.md` have landed. Does not assume P5
(multi-field cubes) or Molden support.

**Deferred:** GPU volume raymarching, slice planes, automatic HOMO/LUMO
selection, CSG composability.

## Where the extraction happens

**Decision: the node outputs a lazy surface specification. Marching cubes runs
in the display conversion, in `atomcad-display`.**

```
   isosurface node
        |
        v
  NetworkResult::Isosurface(IsosurfaceData)     <- network value
  { field, level, coloring, alpha }                resolution-free
        |
        |  atomcad_display::isosurface::extract
        |  (marching cubes; resolution from preferences; cached)
        v
  NodeOutput::Isosurface(SurfaceMesh)           <- scene output
  positions + normals + per-vertex albedo          resolution fixed
  + component ranges
        |
        |  scene_tessellator
        v
  renderer Mesh + component ranges              <- GPU
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
(`csg_conversion_cache`), separate from the eval memoization. The governing
rule: **semantic parameters live in the value; quality parameters live in
preferences.** A `sphere` node's `radius` goes into the value as
`GeoNodeKind::Sphere { radius }`; resolution does not. Isolevel is semantic,
extraction resolution is quality.

**Why not output a mesh:**

- **The isolevel slider.** Chemists scrub it constantly. A mesh value re-runs
  marching cubes inside the evaluator on every tick, invalidating downstream
  memoization. A lazy value makes `eval` nearly free and confines the expensive
  work to a display-side cache.
- **The implicit form is better downstream.** "Which atoms are inside the
  density envelope" is `sample(p) > level` on a field, and unanswerable on a
  triangle soup.
- **Resolution would become uneditable** without re-evaluation — the only
  quality setting in the application that invalidates the eval cache.
- **Clone cost.** `NetworkResult` clones on every wire traversal; an 80^3
  extraction is 100k+ triangles.

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
/// A surface to be extracted at display time. Carries the *specification*,
/// never the mesh.
#[derive(Debug, Clone)]
pub struct IsosurfaceData {
    pub field: Arc<dyn ScalarField>,
    /// Level **magnitude**, strictly positive. Extraction runs at `+level`
    /// and `-level`.
    pub level: f64,
    pub coloring: IsosurfaceColoring,
    /// 0..=1. Exactly `1.0` takes the opaque path.
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

**Inline, not `Arc<IsosurfaceData>`** — two `Arc`s plus scalars is already a
cheap clone, and `BlueprintData` is inline for the same reason.

`Colormap` has one variant because the only reachable pairing today is density
colored by electrostatic potential. NCI's blue-green-red map has no producer
yet. The enum exists so adding one is a one-line change and the serialized
form is forward-compatible.

### Crate placement

`IsosurfaceData`, `IsosurfaceColoring`, `Colormap`, `SurfaceMesh` and the
extractor live in **`atomcad-display`**, in a new `src/isosurface/` module.
`atomcad-structure-designer` already depends on `atomcad-display`, which
already depends on `atomcad-crystolecule`, so `NetworkResult` can carry it and
it can hold a `ScalarField`. `NodeOutput` already carries display types
(`PolyMesh`, `SurfacePointCloud`). No back-edge.

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
| `alpha` | `f64` | `0.4` | `1.0` → opaque fast path |
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

To make level-picking possible, **promote `to_detailed_string` for
`NetworkResult::ScalarField` from optional (P2 of the sibling doc) to
required** — the value range then shows on pin hover via existing machinery.
One match arm, versus an API getter plus a Dart readout for the same
information.

### Editor

Registered in `node_data/node_data_widget.dart`: `level` field, two color
swatches with a **one-click swap button** (an orbital's global sign is
arbitrary, so flipping is how a user matches a published figure), `alpha`
slider, and colormap range/dropdown **disabled unless `color_field` is
connected**.

### Errors and warnings

| Condition | Behavior |
|---|---|
| `field` unconnected | evaluation error |
| `level <= 0` | evaluation error — both signs are always drawn, so the sign is not a user choice |
| `level` exceeds the field's `value_range` magnitude | **non-blocking** warning — an empty render is otherwise indistinguishable from a broken import |
| surface field's `data_bounds` exceed the color field's | **non-blocking** warning |

The last one matters: `ScalarField` returns `0.0` outside its bounds, so a
surface extending past the *color* field's box paints as neutral — plain white
on a blue-white-red map, plausible and wrong. Easy to hit, since orbitals come
from `.molden` and potentials from `.cube`. Warn, do not add a second
out-of-bounds convention.

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
| `evaluator/network_result.rs` `heap_of` | see below — **not** a compile error if missed |

**FRB boundary:** `APIDataTypeBase` variant, both conversion arms in
`structure_designer_api.rs`, then `flutter_rust_bridge_codegen generate`.

**Dart:** `data_type_input.dart`, `type_editor_dialog.dart`,
`node_widget.dart` `_apiDataTypeToString`, `node_network.dart` pin color.

Pin color: `ScalarField` is a soft red (`0xFFE57373`); `Isosurface` is its
rendered form, so a deeper red — `0xFFC62828`. Eyeball against the live palette
before committing.

**`heap_of` hazards**, neither caught by the compiler: `SampledField` holds a
`Vec<f32>` of megabytes, so a missing arm under-reports by the largest payload
in the tree; and the same `Arc` can appear twice in one value (surface and
color field) or be shared across results, so naive recursion double-counts.
Count each distinct pointer once, or exclude shared field storage deliberately
and say so in a comment.

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

In `atomcad-display/src/isosurface/`, invoked from `network_evaluator`'s
result→`NodeOutput` conversion, cached alongside `csg_conversion_cache`.
Hand-rolled — no workspace dependency provides it. `rayon` is available and
`ScalarField: Send + Sync`, so batch sampling can be parallel.

### Why marching cubes

- **Dual contouring is the wrong tool for a smooth field.** Its purpose is
  sharp-feature preservation via a per-cell QEF; an isosurface of a sum of
  Gaussians has no sharp features. What remains is DC's cost, plus vertices
  that are minimizers and can drift off the surface. MC vertices lie exactly on
  it by edge interpolation — a correctness property for something claiming to
  be "the `psi = level` surface".
- **MC's sliver triangles cost nothing here**, because normals come from the
  analytic gradient, not from face normals.
- **DC's adaptivity has little to buy.** The `ScalarField` contract already
  caps useful resolution at the native grid, and 60–100k triangles for an 80^3
  field is not a problem.
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

### Resolution

```
native_grid().map(|g| g.spacing() / quality_multiplier)
             .unwrap_or(preference_spacing)
```

Default is the field's own grid verbatim — the contract's fidelity fast path,
since sampling elsewhere blends eight stored values for no gain. The
`native_grid() == None` fallback (every analytic field, i.e. all future Molden
support) samples `suggested_bounds()` at a preference spacing; this is the
consumer most likely to have been wrongly written against a grid, so it is the
one that proves the contract.

Cells span adjacent sample points: `dims = [nx, ny, nz]` yields
`(nx-1)(ny-1)(nz-1)` cells. The node-centered bounds convention makes this
exact.

### The two sign passes

Run the **same** extractor twice with a sign flag, comparing `sign * psi(p)`
against `level` and multiplying the gradient by `sign`: `+1` gives the positive
lobe in `positive_color`, `-1` the negative lobe in `negative_color`. Folding
the sign into the comparison is what keeps winding and normals consistent
without a second code path.

`value_range().min >= 0` short-circuits the negative pass — a pure
optimization, since it would find no crossings anyway.

### Normals and winding

Per-vertex, `-sign * normalize(gradient)`. For the positive lobe the field
decreases outward; for the negative lobe it increases; the sign flag covers
both. `SampledField` overrides `gradient` with central differences on stored
samples, so these are exact with respect to the data.

Triangles must be **counter-clockwise viewed from outside**, consistent with
that normal. This is load-bearing: the two-pass draw is `cull_mode: Front` then
`Back`, so an inverted component draws near-before-far. The failure looks like
slightly-off shading, not obvious breakage — same class as the csgrs `det<0`
winding fix in `structure_invert`.

### Connected components

Union-find over triangle vertex indices, then re-emit triangles grouped so each
component occupies a contiguous index range, with a centroid per component.
The two sign passes are separate; components are labeled within each. Requires
edge-keyed vertex deduplication, or every triangle becomes its own component.

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

The sort happens **at draw time in the renderer**, reordering *draw calls*, not
indices — so unlike `transparent_sort.rs` there is no index-buffer rewrite. N
is single digits; it is free.

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

### The shader delta is one line

`mesh.wgsl` ends `return vec4<f32>(color, 1.0);` — opacity is hardcoded.
Per-vertex albedo already exists on `Vertex`, so the colormap needs nothing;
alpha is the only missing channel, and it is uniform per surface, so it goes in
the existing per-mesh `ModelUniform` (group 1) rather than the vertex format:

- `ModelUniform` gains `alpha: f32`
- `mesh.wgsl` returns `vec4<f32>(color, model.alpha)`
- `ModelUniform::new()` defaults it to `1.0`, leaving every opaque consumer
  unchanged

**Padding gotcha.** Two `mat4x4<f32>` is 128 bytes; a bare trailing `f32`
breaks the 16-byte boundary and the WGSL layout stops matching the `#[repr(C)]`
struct. Pad explicitly, or declare the slot as a `vec4<f32>`. Issue #269 was
exactly this bug in `CameraUniform` — wrong output, no error.

### The opaque fast path

`alpha == 1.0` routes to the existing opaque pipeline: one draw,
`cull_mode: Back`, depth write on, no sort. This lets P3 ship before any
transparency work, and gives a permanent escape hatch — a picture whose
correctness is not in question.

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
| `isosurface_quality_multiplier` | `f64` | `1.0` | divides native grid spacing; `<1` coarser, `>1` finer |
| `isosurface_fallback_spacing` | `f64` | `0.15` Å | spacing when `native_grid()` is `None` |
| `surface_transparency_mode` | enum | `ComponentSorted` | below |
| `isosurface_cell_budget` | `usize` | `4_000_000` | refuse-and-warn ceiling so a fine multiplier cannot hang the UI |

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
`atomcad-display/src/isosurface/mod.rs`; `DataType::Isosurface` and every
touchpoint above plus codegen and the four Dart sites;
`NetworkResult::Isosurface` with its four arms; promote
`to_detailed_string` for `ScalarField`.

| Test | Asserts |
|---|---|
| `DataType` text round-trip | `from_string` and `Display` agree |
| `APIDataType` round-trip, both directions | catches a missed FRB arm |
| `infer_data_type` on a `NetworkResult::Isosurface` | `Some(DataType::Isosurface)` — **the one site with no compiler backstop** |
| `heap_of` with the same `Arc` as surface and color field | counted once |
| `to_detailed_string` on a `ScalarField` | reports dims and value range |
| Existing registry-validation suite | stays green |

### P2 — marching cubes extractor (backend only)

**Work:** `isosurface/extract.rs`; the 256-entry table with consistent
complementary-case resolution and edge-keyed vertex dedup; sign-folded
comparison, gradient normals, winding, union-find components; resolution policy
including the `None` fallback; cell-budget check.

The critical tests are the ones whose failures are *silent* in a picture.

| Test | Asserts |
|---|---|
| Analytic sphere (`r - R`), level 0 | every vertex within tolerance of `R`; triangle count in band |
| Same, normals | parallel to the radial direction, pointing **outward** |
| Same, **winding** | `cross(v1-v0, v2-v0) . normal > 0` for every triangle |
| Same, **closedness** | zero boundary edges. **A holed sphere passes the radius, normal and winding tests** — this is the only assertion that catches it |
| Same, Euler characteristic | `V - E + F == 2`; catches non-manifold welding a boundary count can miss |
| Ambiguous face configuration | still closed; topology deliberately *not* asserted |
| Analytic 2p_z, level `L` | exactly two components, centroids on opposite sides of the nodal plane |
| Same, colors | `+z` gets `positive_color`, `-z` gets `negative_color` |
| Two same-sign lobes under one cell apart | still two components — the surface-nets failure mode |
| Non-negative field | one component; negative pass short-circuits |
| Level above the field's max magnitude | empty mesh, no panic |
| Component ranges | contiguous, non-overlapping, covering `indices` exactly |
| Field with `native_grid() == None` | succeeds at the fallback spacing |
| Cell budget exceeded | descriptive error, no allocation |
| Colormap on a linear color field | albedo varies monotonically; out-of-range clamps |

### P3 — node, editor, opaque rendering

**The first user-visible milestone**, deliberately opaque-only.

**Work:** node data, `eval`, registration; `SurfaceMesh`,
`NodeOutput::Isosurface`, the conversion and its cache; `scene_tessellator`
arm; `isosurface_editor.dart` and its widget entry; the three extraction
preferences; reference guide.

| Test | Asserts |
|---|---|
| Node eval on a fixture | `NetworkResult::Isosurface` with field and level from properties |
| `level` pin wired | overrides the stored property |
| `level <= 0` | evaluation error |
| `level` above the field's range | non-blocking warning; still produces a value |
| `.cnnd` round-trip | all properties survive, `Colormap` included |
| Result → `NodeOutput` | expected component count |

**Manual walkthrough**

1. `import_cube` → `water.cube` → `isosurface`, alpha `1.0`. Expect an opaque
   blob enclosing the molecule.
2. Display the `molecule` pin too. Expect surface and atoms *registered* — a
   1.9x mismatch means a Bohr conversion is wrong.
3. Scrub `level` up. Expect monotonic shrinking, then an amber warning, not an
   error.
4. On a signed orbital, expect two lobes in two colors; the swap button
   exchanges them and changes nothing else.
5. Change `isosurface_quality_multiplier`. Expect re-tessellation at the new
   resolution **without the network re-evaluating** — the architectural claim,
   made visible.

### P4 — transparency

**Work:** `ModelUniform` alpha with explicit padding and the `mesh.wgsl`
return; the two culled pipelines; draw-time component sort;
`surface_transparency_mode` and its three modes.

| Test | Asserts |
|---|---|
| `size_of::<ModelUniform>() % 16 == 0` | the padding gotcha |
| `ModelUniform::new()` | `alpha == 1.0`, opaque consumers unchanged |
| Component sort | synthetic centroids + view matrix come back farthest-first |
| `alpha == 1.0` | routes to the opaque pipeline — assert the pipeline, not pixels |

**Manual walkthrough**

1. Alpha `0.4` on a **d-type or overlapping-p** orbital — not a density
   envelope, which cannot distinguish the modes.
2. Cycle all three modes at a fixed camera. Expect `SinglePass` to misorder
   visibly, `TwoPass` to fix within lobes but not between them,
   `ComponentSorted` to be correct.
3. Orbit in `ComponentSorted`. Expect no popping; a pop means the sort is not
   per-frame.
4. Show ghost atoms and the surface together. Expect failure mode 3. Record
   what it looks like — that is the input to the OIT decision.
5. **Record the expiry answers here** and delete the losing modes.

### P5 — color field and colormap

**Work:** `color_field` wiring and `IsosurfaceColoring::Field`; per-vertex
colormap sampling; the mismatched-bounds warning; editor controls; reference
guide.

| Test | Asserts |
|---|---|
| Color field wired | `Field` arm; unwired reverts to `Phase` |
| Colormap range | ends map to palette ends; beyond them, clamp |
| Signed surface with a color field | still two components, both painted per-vertex |
| Bounds exceeding the color field's | non-blocking warning; still drawn |

**Manual walkthrough**

1. Density into `field`, electrostatic potential into `color_field`. Expect the
   classic red/white/blue ESP map.
2. Pair mismatched boxes deliberately. Expect an amber warning and a visible
   white band where the surface leaves the color field's box.
3. Repeat through `atomcad-cli` for the headless path.

**Deliverable: closes the scope of this document.**

## Documentation touchpoints

- `doc/reference_guide/nodes/atomic.md` — the node (P3, updated P5)
- `doc/reference_guide/node_networks.md` — `Isosurface` pin type and color (P1)
- `doc/reference_guide/ui.md` — the four preferences (P3, P4)
- `crates/atomcad-display/src/AGENTS.md` — **new file**: module map,
  Ångström-in/out invariant, outward-winding rule; register it in the root
  `AGENTS.md`
- `doc/testing.md` — winding **and closedness** assertions as required tests for
  any surface extractor, noting that a holed surface passes every per-vertex
  check
- this document — the expiry answers (P4)
