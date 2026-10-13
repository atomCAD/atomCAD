# Screen-Space Lines

Source: `reports/rendering_capabilities/rendering_capabilities_report_2026-09-21.md`
§2.1 (anti-aliasing of lines) and §2.2 (screen-space width lines, dashes).

## Motivation

Every line in the viewport (the ground grid, the axes, the lattice basis
vectors, the drawing-plane grid, the unit cell, CSG wireframes, tool-envelope
cages, guided-placement rings, the gizmo's lines) is a hardware 1-pixel
`LineList` primitive. That has four visible consequences:

- **No weight.** A line cannot be made thicker, so the axes cannot stand out
  from the grid, and a CSG wireframe is an unreadable tangle of identical
  strokes.
- **No anti-aliasing.** Every line is a hard 1-pixel staircase, and the
  distant grid turns into moiré that moves as the camera does.
- **No alpha.** A line is fully opaque or absent.
- **Dashes in Ångström.** The only dashed line, the lattice basis vectors, is
  cut into 0.5 Å pieces on the CPU, so the dash pattern grows and shrinks with
  zoom and becomes a solid line or a few dots at the extremes.

This document replaces the line primitive with **screen-space lines**:
instanced segment quads, expanded in the vertex shader to a width given in
pixels, with an analytic anti-aliased edge, alpha, and dashes measured in
pixels.

The one non-obvious requirement is the interaction with *File > Export
viewport image*, which renders at 2× and box-downsamples. A line authored as
"2 px" must be 4 render pixels wide in that target, or it comes out as a 1 px
line in the image. The same holds for the network thumbnail, which also
renders at 2×. D2 is how that is handled without any line producer, or the
export code, knowing about it.

### Scope

- **In:** the line pipeline, the `LineMesh` API, per-segment width, colour
  with alpha and dash pattern, the render-target pixel scale, conversion of
  every existing line producer, and deletion of the CPU dash splitter.
- **Also in:** grid level-of-detail (minor and major lines dropped when too
  dense) and a distance fade (D9).
- **Out (but not precluded, see Future work):** hidden-line wireframes,
  feature edges over solid geometry, a user preference for line weight,
  HiDPI viewport allocation, MSAA.

## Current state (facts, with references)

All in `rust/crates/atomcad-renderer/src/` unless noted.

- **Vertex format.** `line_mesh.rs`: `LineVertex { position: [f32; 3], color:
  [f32; 3] }`, indexed in pairs (`LineMesh::indices`). `add_line_with_positions`
  allows a colour gradient; nothing outside `line_mesh.rs` uses it, every
  producer calls `add_line_with_uniform_color`.
- **Dashes.** `LineMesh::add_dotted_line` splits a segment into world-space
  pieces. Its only caller is `add_dotted_axis_line` in
  `atomcad-display/src/coordinate_system_tessellator.rs`
  (`DOT_LENGTH = GAP_LENGTH = 0.5` Å), used for non-Cartesian lattice axes.
- **Shader.** `line_mesh.wgsl`: `view_proj * model * position`, outputs the
  vertex colour with alpha 1.
- **Pipelines.** `Renderer::create_line_pipeline` builds `line_pipeline` and
  `background_line_pipeline`, which are identical (both have a zero depth
  bias). `LineList`, `BlendState::REPLACE`, depth write on, `Less`.
- **The four line meshes** and their producers:

  | GPU mesh | Producer | Drawn in |
  |---|---|---|
  | `wireframe_mesh` | CSG wireframe (`poly_mesh_tessellator.rs`), unit cell (`unit_cell_wireframe_tessellator.rs`), tool envelopes (loop in `atomcad-structure-designer/src/scene_tessellator.rs`), guided-placement sphere/ring (`guided_placement_tessellator.rs`) | main pass, **first** |
  | `reference_line_mesh` | drawing-plane grids and axes (`coordinate_system_tessellator.rs`) | main pass, editing aids |
  | `background_mesh` | ground grid, Cartesian axes, lattice axes (`tessellate_background_coordinate_system`, uploaded by `update_background_mesh` from `rust/src/api/common_api.rs` and `api_common.rs`) | main pass, editing aids |
  | `gadget_line_mesh` | gadget tessellation output (`scene_tessellator.rs`) | gadget pass |

- **Depth bias.** The opaque `triangle_pipeline` has `constant: -8,
  slope_scale: -1.0`, so a mesh face wins coplanar ties against a wireframe
  edge lying in it.
- **Camera uniform.** `CameraUniform` in `renderer.rs` (view-proj, view,
  proj, eye, headlight, `is_orthographic`, `ortho_half_height`, 12 bytes of
  padding; 240 bytes). It contains **no viewport size**, so no shader today
  can convert between clip space and pixels. Six `.wgsl` files declare their
  own copy of the struct.
- **Render targets.** `RenderTarget { texture, …, size }`. Three users:
  - the live viewport (`Renderer::target`, resized by `set_viewport_size`,
    which also serves the CLI screenshot by resizing it temporarily);
  - the thumbnail (`thumbnail_target`, `THUMBNAIL_RENDER_SIZE` = 2 ×
    `THUMBNAIL_SIZE`, downsampled by `thumbnail::downsample_2x2`);
  - the export (`render_image` creates a target of
    `IMAGE_EXPORT_SUPERSAMPLING` × the image size, `image_export.rs`, and
    downsamples it the same way).
- **Uniform write order.** `set_viewport_size` calls `update_camera_buffer`
  *before* it recreates the target. Harmless today; wrong once the uniform
  holds the target size (D3).
- **Pipeline layout.** Every pipeline except the label pipeline uses one
  shared layout, `[camera (group 0), model (group 1)]`; the label pipeline
  adds the font atlas at group 2.
- **Alpha-blended pipelines.** Labels, transparent impostors and the
  transparent-surface pipelines use `BlendState::ALPHA_BLENDING`.
- **No GPU tests.** `rust/crates/atomcad-renderer/tests/renderer/` covers
  CPU-side code only (sorting, atlas, thumbnail helpers, export sizing).

## Decisions

### D1 — Lines are instanced segment quads

Wide lines are not portable in wgpu (WebGPU has no line width), and hardware
lines cannot be anti-aliased per fragment. Each segment becomes **one
instance**; the pipeline draws 6 vertices (two triangles) per instance with a
vertex buffer whose `step_mode` is `Instance`, and the vertex shader places
each corner from `vertex_index`.

No index buffer. `GPUMesh` gains an instance-count path (`num_instances`,
`draw(0..6, 0..num_instances)`) next to its indexed path.

Instance layout (`LineSegmentInstance`, `#[repr(C)]`, Pod):

| Field | Type | Meaning |
|---|---|---|
| `p0`, `p1` | `[f32; 3]` ×2 | segment ends, world space (the model matrix still applies) |
| `color` | `[f32; 4]` | linear RGBA, **not** premultiplied |
| `width_px` | `f32` | width in logical pixels (D2) |
| `dash_on_px`, `dash_off_px` | `f32` ×2 | dash pattern in logical pixels; `dash_off_px == 0` means solid |
| `dash_phase` | `f32` | world-space arc length of `p0` along its polyline (D5); 0 for a lone segment |
| `lod_slot` | `u32` | grid level-of-detail slot (D9); 0 = never hidden or faded |

60 bytes per segment. The largest line producer, a dense CSG wireframe, is in
the tens of thousands of edges, so that is a few MB at most, about the same
as the old two-vertex-plus-two-index form (2 × 24 + 8 = 56 bytes).

Style is stored per segment rather than in a style table indexed per segment.
A table saves bytes but adds a second buffer, a binding, and an indirection in
the shader, for meshes that are small.

### D2 — Lengths are written in logical pixels; the render target owns the scale

Every screen-space length a producer writes (width, dash on, dash off) is in
**logical pixels**: pixels of the image the user finally sees. The GPU
converts them with one factor:

> `pixel_scale` = render-target pixels per output pixel.

It is a property of the **render target**, set where the target is created
and nowhere else:

```rust
struct RenderTarget {
    // … as today …
    size: Extent3d,      // render pixels
    pixel_scale: f32,
}

impl RenderTarget {
    /// A target that renders an `out_width × out_height` image at
    /// `supersampling`× (size = out × supersampling, pixel_scale = supersampling).
    fn new(device: &Device, out_width: u32, out_height: u32, supersampling: u32) -> Self;
}
```

| Target | `supersampling` → `pixel_scale` |
|---|---|
| Live viewport | 1 |
| CLI screenshot (resizes the live target) | 1 |
| Thumbnail | `THUMBNAIL_RENDER_SIZE / THUMBNAIL_SIZE` = 2 |
| Viewport image export | `IMAGE_EXPORT_SUPERSAMPLING` = 2 |

The thumbnail target is then created as `RenderTarget::new(device,
THUMBNAIL_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING)`, and `render_image`
as `RenderTarget::new(device, width, height, IMAGE_EXPORT_SUPERSAMPLING)`;
both downsample calls already take the target size.

Why here and not elsewhere:

- **Not in the producers.** The display crate says "the axes are 2 px" and
  never learns that supersampling exists. If the export moves to 4×, one
  constant changes.
- **Not in the camera.** The thumbnail and the export borrow one camera
  buffer for different targets, and the live camera is reused for the export.
  The scale describes the target, so the target carries it.
- **Not in the export code.** `render_image` multiplies nothing. The bug this
  design exists to prevent (a supersampled render with lines at their
  unscaled width) cannot be written without constructing a target by hand.
- **HiDPI is the same knob.** When the viewport is allocated at device-pixel
  size (report §2.10), the live target's `pixel_scale` becomes the device
  pixel ratio and lines keep their apparent width with no line work at all.
  Any later pixel threshold (grid LOD, label declutter, screen-space label
  size) reads the same field.

**The anti-aliasing fringe is not scaled.** The soft edge stays one *render*
pixel wide (D4). In a 2× target the box downsample already supplies the rest
of the smoothing; scaling the fringe too would blur thin lines.

**Exporting at a size other than the viewport's.** When a 1000 px viewport is
exported at 2000 px, lines keep their authored width: a 2 px axis is 2 px in
the 2000 px image, as it would be on a 2000 px screen. "Screen space" means
pixels of the image being produced; only supersampling scales lines. A
"make the image look like an enlarged copy of the screen" option, if ever
wanted, is one more factor multiplied into `pixel_scale` at export.

### D3 — The camera uniform carries the target's size and scale

`CameraUniform` gains two fields after `ortho_half_height`, replacing the
current tail padding and growing the struct:

| Field | WGSL type | Offset |
|---|---|---|
| … existing, unchanged … | | 0–227 |
| *(padding)* | | 228 |
| `viewport_size_px` | `vec2<f32>` | 232 |
| `pixel_scale` | `f32` | 240 |
| *(padding to 16)* | | 244–255 |

The size becomes 256 bytes. The Rust struct mirrors this field for field,
with explicit padding (the #269 bug was this kind of mismatch, so a test pins
the size and the offsets, Phase 1).

One `Renderer` method writes every per-view uniform:

```rust
/// Writes `camera_buffer` from `camera` and `target`, and (D9) every line
/// mesh's `LineLodUniform`. `live` = this is the live viewport, the only
/// target whose LOD choices use and update the hysteresis state.
fn write_view_uniforms(&mut self, camera_buffer: &Buffer, camera: &Camera,
                       target: &RenderTarget, live: bool);
```

The uniform's contents come from a pure constructor,
`CameraUniform::new(camera, target_size, pixel_scale)`, so they can be
tested without a device. `write_view_uniforms` replaces the inline
`CameraUniform::refresh` + `write_buffer` pairs in `update_camera_buffer`,
`render_thumbnail` and `render_image`, so a uniform's size and scale can
only ever come from the target it is drawn into. `set_viewport_size`
recreates the target **first** and writes the uniforms after.

All six shaders get the extended struct, so the struct is the same everywhere
(the others would still work with the old prefix, but a struct that differs
between files is a trap for the next person to add a field).

### D4 — The vertex shader expands, the fragment shader computes coverage

**Vertex shader**, per corner:

1. Transform `p0`, `p1` by `model` and `view_proj` to clip space.
2. **Clip the segment against the near plane in clip space**, before any
   divide by `w`: if one end has `z < 0`, move it along the segment to
   `z = 0`; if both do, emit a degenerate quad. `z = 0` is where wgpu's
   rasteriser clips, whatever the matrix. Note the matrices are not uniform:
   perspective is `DMat4::perspective_rh_gl` (`camera.rs`), whose `z = 0` lies
   at about 2 × `znear` rather than at `znear`, and orthographic is
   `orthographic_rh` (0..1). For both, `z ≥ 0` implies `w > 0`, which is what
   the divide needs. Without this, a ground-grid line passing behind the
   camera projects through infinity and paints a wedge across the screen. It
   is the classic bug of screen-space lines and gets a test (Phase 2).

   *Note (Phase 2):* the wedge is what you get when the corners are emitted
   at `w = 1`. Because step 5 emits each corner at its own end's `w`, the
   hardware clipper sees a quad straddling `w = 0` instead, and the symptom
   of a missing clip here is different: every line running towards the
   horizon **vanishes**. The GPU test therefore checks both: no pixel above
   the horizon, and receding lines present below it. It was confirmed to
   fail with the clip removed.
3. Divide by `w`, convert to render pixels with `viewport_size_px`.
4. Let `hw = 0.5 * max(width_px * pixel_scale, 1.0)` and `pad = hw + 1.0`
   (one render pixel of fringe). Offset the corner by `±pad` along the
   perpendicular and by `pad` beyond each end along the direction (room for
   a round cap).

   **A segment can have no screen direction.** When both ends project to
   (nearly) the same pixel, e.g. the Z axis in a top-down view, any axis
   seen end-on, or a segment collapsed by the clip in step 2, the
   direction is a zero vector and normalising it gives NaN. Below a screen
   length of 1e-4 render pixels the shader substitutes the direction
   `(1, 0)`. The quad is then a `2·pad` square around the point, and the
   round-cap distance draws it as a dot of the line's width.
5. Convert back to NDC and multiply by **that end's own `w`**, so depth
   interpolates perspective-correctly and the line is depth-tested exactly
   where its centre is.

Outputs to the fragment shader, all in render pixels: signed distance across
the line, distance along it from (unclipped, see D5) `p0`, segment length,
`hw`, the colour, and the dash pattern already multiplied by `pixel_scale`.

**The screen-space outputs are declared `@interpolate(linear)`.** Step 5
gives each corner its own `w`, and WGSL's default `perspective`
interpolation would then divide these values by `w` across each of the
quad's two triangles. They are screen-space quantities, so that would bend
them: uneven dash spacing, a lopsided edge and misplaced caps on any line
whose ends sit at different depths, which is every grid line running towards
the horizon. Only depth (the built-in position) is meant to interpolate
perspective-correctly. Colour is constant per instance and can use `flat`.

**Fragment shader:**

- Distance to the segment: across-distance in the body, distance to the end
  point beyond either end (round caps).
- `coverage = clamp(hw + 0.5 - distance, 0, 1)`.
- **Sub-pixel widths:** when `width_px * pixel_scale < 1`, the geometry is
  drawn 1 render pixel wide and `coverage *= width_px * pixel_scale`. A thin
  line fades instead of breaking into dots, which is what removes the distant
  grid's shimmer.
- Dashes (D5) multiply in a second coverage term.
- `discard` below 1/255; output `vec4(color.rgb, color.a * coverage)`.

Caps are round, there are no joins. Opaque lines overlap invisibly at
polyline joints; a line with alpha < 1 shows a slightly denser dot at each
joint. That is accepted, since translucent polylines are rare and miter joins
need neighbour data per segment.

### D5 — Dashes are in pixels; polylines carry an arc-length phase

A dash pattern is `(on_px, off_px)` in logical pixels. With `period = on +
off` (both scaled), the dash term is the coverage of the "on" interval at
`t = (along + phase_px) mod period`, with the same one-pixel soft edge as
the sides, so dash ends are anti-aliased too.

- **Within one segment** this is exact: the pattern is constant in pixels at
  any zoom, which is the point.
- **Along a polyline** (a tessellated circle or ring), each segment would
  restart the pattern without help. `LineMesh::add_polyline` therefore writes
  each segment's starting world arc length into `dash_phase`, and the vertex
  shader converts it to pixels with the pixels-per-world-unit at `p0`. This
  is exact in orthographic projection and visually continuous on finely
  tessellated curves in perspective (the scale differs negligibly between
  neighbouring segments).
- **`along` is measured from the unclipped `p0`.** When `p0` is clipped by
  the near plane, the shader still measures from where `p0` would project.
  When `p0` is behind the camera that is undefined, and the measurement
  starts at the clipped point instead. The pattern then slides as the camera
  moves, on that segment only.
- **Screen-space dashes slide when the camera moves in perspective**, because
  the segment's pixel length changes. This is inherent to dashes in pixels
  and is the accepted trade for a pattern that reads the same at every zoom.

World-space dashes are not kept. Their only use (lattice axes) is the case
that motivated this design.

### D6 — Lines are blended, depth-tested, not depth-writing, and drawn after the opaque content

Lines use straight-alpha blending for colour (`SrcAlpha`,
`OneMinusSrcAlpha`), depth test `Less`, **depth write off**, no depth bias.

**The alpha component must be `One`, `OneMinusSrcAlpha`.** Blending alpha
with `SrcAlpha` as well would make a fringe fragment of alpha 0.5 over the
opaque clear (a = 1) leave a destination alpha of 0.5² + 0.5 = 0.75.
Readback keeps alpha: `bgra_to_rgba_in_place` only swaps channels, and
`downsample_2x2` and `encode_png` pass it through. Every anti-aliased line
pixel would come out translucent in an exported PNG and in a thumbnail. With
`One, OneMinusSrcAlpha` the destination stays at a = 1.

*Correction (Phase 2):* an earlier draft said wgpu's
`BlendState::ALPHA_BLENDING` blends alpha with `SrcAlpha` and that the other
blended pipelines therefore had a latent defect. They do not: in wgpu 23
`ALPHA_BLENDING.alpha` is `BlendComponent::OVER`, i.e. `One,
OneMinusSrcAlpha`, already. Phase 2 still defines the blend state once, as
`STRAIGHT_ALPHA_BLENDING` in `renderer.rs` (equal to `ALPHA_BLENDING`), uses
it in every blended pipeline, and pins its factors in a CPU test so a wgpu
upgrade that changed them would fail.

Why no depth write: the anti-aliasing fringe is a real fragment at line
depth. If lines wrote depth, the fringe of one grid line would reject the
crossing grid line at the intersection (same plane, same depth, arbitrary
winner), leaving notches at every grid crossing.

Draw order in the main pass becomes:

1. opaque meshes, atom and bond impostors (write depth)
2. labels (blend, write depth)
3. **all lines**: wireframe, then reference and background lines (these two,
   like labels, only when editing aids are drawn, as today)
4. transparent impostors (ghosts)
5. transparent isosurfaces

Today the wireframe is drawn *first*; it moves to step 3. Correctness of each
pair:

- **Opaque content vs a line:** the line tests against finished opaque depth.
  The triangle pipeline's negative bias still makes a face win a coplanar tie
  against an edge lying in it, as it does today.
- **Labels before lines:** a line behind a label is rejected inside the
  glyph, since labels write depth and `discard` empty texels. A line in front
  passes and blends over. Both correct.
- **Lines before ghosts:** a ghost behind a line is depth-tested against the
  opaque buffer only, so it composites over the line and tints it slightly.
  The other order would draw a line behind a ghost crisply on top of it,
  which reads as "in front". A tint is the milder error.

`line_pipeline` and `background_line_pipeline` merge into one
`line_pipeline`, since they never differed. Its layout is
`[camera, model, line LOD (group 2, D9)]`, its own rather than the shared
one.

**The gadget pass follows the same rule: lines last.** Today it draws
`gadget_line_mesh` first, right after its depth clear, and the gadget
triangles and guide-dot impostors after it. That is correct only because
lines write depth. Without depth writes, everything drawn after a line paints
over it, even when the line is in front, and the gizmo would lose its
lines wherever they cross its own arrow heads or rings. The gadget pass
becomes: depth clear, gadget triangles, gadget atom and bond impostors, then
`gadget_line_mesh` with the same `line_pipeline`.

### D7 — `LineMesh` API

`LineMesh` (keeping its name and its role as the CPU-side mesh every producer
fills) becomes a list of `LineSegmentInstance`:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineStyle {
    pub color: [f32; 4],          // linear RGBA
    pub width_px: f32,            // logical pixels
    pub dash: Option<LineDash>,
    pub lod_slot: u32,            // D9; 0 = none. Added in Phase 4.
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineDash { pub on_px: f32, pub off_px: f32 }

impl LineStyle {
    /// 1 px, solid, opaque: exactly what every line is today.
    pub const fn solid(color: [f32; 3]) -> Self;
    pub const fn with_width(self, width_px: f32) -> Self;
    pub const fn with_alpha(self, alpha: f32) -> Self;
    pub const fn dashed(self, on_px: f32, off_px: f32) -> Self;
}

impl LineMesh {
    pub fn add_segment(&mut self, p0: &Vec3, p1: &Vec3, style: &LineStyle);
    /// Consecutive segments through `points`, the dash phase continuing
    /// along them (D5). `closed` adds the segment back to the first point.
    pub fn add_polyline(&mut self, points: &[Vec3], closed: bool, style: &LineStyle);

    /// Kept for producers not yet given a style: `LineStyle::solid(color)`.
    pub fn add_line_with_uniform_color(&mut self, start: &Vec3, end: &Vec3, color: &[f32; 3]);
}
```

For the grids, D9 (Phase 4) also adds a `lod_slots: Vec<LodSlot>` table to
`LineMesh`, with `LineMesh::add_lod_slot(slot: LodSlot) -> u32` returning
the id to put in `LineStyle::lod_slot`.

Removed: `LineVertex`, `indices`, `add_vertex`, `add_line`,
`add_line_with_positions` (the gradient), `add_dotted_line`.

`thumbnail::content_bounds` reads line positions; it switches to iterating
`p0`/`p1` of the segments.

### D8 — Styles for the existing producers

Each producer picks its style where it emits lines. The widths below are
starting points; the walkthrough (Phase 5) tunes them on screen.

| Producer | Style |
|---|---|
| Ground grid, minor lines | 1.0 px |
| Ground grid, major lines (every 10) | 1.5 px |
| Cartesian axes | 2.0 px |
| Lattice basis axes (non-Cartesian) | 2.0 px, dashed 6 / 4 px |
| Drawing-plane grid / axes | same as the ground grid / axes |
| Unit cell | 1.5 px |
| CSG wireframe | 1.0 px |
| Tool envelopes | 1.5 px |
| Guided-placement sphere and ring | 1.5 px, closed polylines |
| Gadget lines | 2.0 px |

Grid colours keep their current preferences. Alpha is available but not
introduced anywhere in this change, so that what moves is width and
anti-aliasing, not colour. The grids' lines also carry a `lod_slot` (D9).
Every other producer leaves it at 0.

### D9 — Grid level of detail: whole levels drop, the rest fades with eye distance

Seen from far away or at a low angle, a grid's lines get closer on screen
than a pixel and turn into a grey smear that crawls as the camera moves. Two
mechanisms deal with it. Neither puts any grid knowledge into the shader.

- **Level of detail, decided on the CPU.** A whole level of the grid (all
  minor lines, or all major lines) is shown or hidden, with no fade in
  between. When zooming out, minor lines disappear at once; that pop is
  accepted.
- **Distance fade, in the fragment shader.** A visible level fades out
  between two eye distances, `fade_start` and `fade_end`. The shader knows
  only those two numbers, not planes, normals or angles.

The angle, which is what makes a low camera's grid dense, is handled
entirely by the CPU, through the choice of those numbers.

**LOD slots.** A slot is one family of grid lines that are equally spaced
and in one plane:

```rust
pub struct LodSlot {
    pub plane_point: Vec3,
    pub plane_normal: Vec3,
    pub spacing: f32,   // world distance between neighbouring lines of this level
}
```

The ground grid registers two slots, minor (`s`, the cell edge) and major
(`10 s`), each with the grid's plane; the dual-grid mode registers two more
for the lattice grid. Each drawing-plane grid registers its own two, so
several drawing planes in one `reference_line_mesh` each keep their own
plane. For a parallelogram grid (`u` ≠ `v`), `spacing` is the smaller line
distance. Slot ids run from 1 to 31; id 0 is reserved for "no slot". When a
mesh's table is full, `add_lod_slot` returns 0 with a `log::warn!`, and those
lines never fade.

**The slot rule** (a pure function, `line_lod.rs` in the renderer crate,
evaluated per slot, per draw):

```rust
pub struct SlotLod { pub visible: bool, pub fade_start: f32, pub fade_end: f32 }

pub fn slot_lod(slot: &LodSlot, camera: &Camera, target_size: (u32, u32),
                pixel_scale: f32, was_visible: Option<bool>) -> SlotLod;
```

It takes the target's size and scale, not a `RenderTarget`, so it can be
tested without a device. **"No fade" is `fade_end <= fade_start`** (written
as `0, 0`). The shader skips the fade then, since WGSL's `smoothstep` is
undefined when its edges are equal or reversed.

All thresholds are in logical pixels × `pixel_scale` (D2), so a 2× export
makes the same choices as the screen.

- **Perspective.** Let `h` = the eye's distance from the slot's plane and
  `f` = the focal length in render pixels (`target height / 2 / tan(fovy /
  2)`). At eye distance `D`, the densest lines of the slot, those running
  across the view, are about `s · f · h / D²` pixels apart. Lines running
  towards the horizon are `s · f / D` apart, which is never smaller.
  - *Visible* when the slot is sparse enough even straight below the eye
    (`D = h`), where its lines are farthest apart: `s · f / h ≥ T_drop`.
  - *Fade distances*: where that spacing falls to `T_fade_start` and
    `T_fade_end`: `fade_start = √(s · f · h / T_fade_start)` and `fade_end =
    √(s · f · h / T_fade_end)`.

  A low camera (small `h`) automatically gets short fade distances. With the
  eye in the plane (`h < 1e-6`) the slot is hidden, which is the edge-on
  grid. This is a case of its own: the formulas would give
  `fade_start = fade_end = 0`, i.e. "no fade".
- **Orthographic.** Eye distance means nothing here, so there is no distance
  fade (`fade_start = fade_end = 0`). Let `k` = render pixels per world unit
  (`target height / (2 · ortho_half_height)`) and `c = |view_dir ·
  plane_normal|`. The across-view lines are `s · k · c` apart.
  - *Visible* when `s · k · c ≥ T_drop`.

  An exactly edge-on orthographic view (`c = 0`), such as a front view of
  the ground grid, therefore hides the grid. Today that view shows it as a
  single line.
- **Hysteresis.** A hidden slot comes back only at `1.25 · T_drop`, so an
  orbit hovering at the threshold doesn't flicker. Only the live viewport
  passes `was_visible`, which the renderer keeps per mesh and slot. The
  thumbnail and the export pass `None` and take the plain threshold.

Starting values, tuned in the walkthrough: `T_drop = 4 px`,
`T_fade_start = 4 px`, `T_fade_end = 1.5 px`.

**On the GPU.** Each line mesh gets a second uniform, `LineLodUniform`,
holding one `vec4(visible, fade_start, fade_end, 0)` per slot id (32
entries including the reserved 0; 512 bytes), at bind group 2 of the line
pipeline. `write_view_uniforms` (D3) fills it for every line mesh that has
slots, since it depends on the same camera and target as the camera
uniform. Entry 0 is always `(1, 0, 0, 0)`: visible, no fade. A mesh without
slots binds one shared, constant uniform holding that value in every entry.

- **Vertex shader:** a segment whose slot is not visible becomes a
  degenerate quad. Otherwise the slot's `fade_start` and `fade_end` go to the
  fragment shader as `flat` outputs, together with the world position of the
  point on the segment (`p0` or `p1` after the near-plane clip, not the
  expanded corner). That position is interpolated perspective-correctly,
  unlike the screen-space outputs of D4.
- **Fragment shader:** when `fade_end > fade_start`, `alpha *= 1 −
  smoothstep(fade_start, fade_end, distance(world_pos, eye))`, where `eye` is
  the camera uniform's `camera_position`.

**No retessellation.** Dropping a level is a uniform write, not a new mesh,
so camera moves need no hook into the display crate or the API. Every
target (live, CLI screenshot, thumbnail, export) gets its own correct
choice at its own size for free.

**What this does not fix.**
- An orthographic view at a moderate tilt can keep a major level that is
  still somewhat dense. It's busy, not smeared; the walkthrough judges it.
- The grid is still finite (`grid_size` cells). Zoomed out far enough in
  perspective, its border can show before the fade reaches it. This is
  unchanged from today.

## Open questions

1. **Line-weight preference.** A single "line weight" multiplier in
   preferences would be one more factor into `pixel_scale` for the live
   viewport only (or for all targets). Left out until the walkthrough shows
   whether the D8 defaults need it.
2. **Gadget lines in the CLI screenshot.** Unchanged (the CLI screenshot
   still draws the gadget pass), but they will now be wider. Mentioned for
   anyone diffing old CLI screenshots.

## Testing

The renderer has no GPU tests today, and most of what this design changes
runs on the GPU: blending, draw order, interpolation, and the shader's
arithmetic. CPU unit tests alone would leave the bugs this document already
fixed once (D4, D6) without a guard. Four layers, cheapest first:

1. **Pure functions, CPU, in the default `cargo test`.** Everything with
   arithmetic is written as a pure function that `tests/renderer/` can call
   without a device:
   - `RenderTarget::dims(out_w, out_h, supersampling) -> (Extent3d, f32)`.
     `RenderTarget::new` needs a `Device`, so the size and scale rule lives
     here and `new` calls it.
   - `CameraUniform::new(camera, target_size, pixel_scale)` (D3).
   - `slot_lod` (D9).
   - **Rust twins of the shader math** in `line_geometry.rs`: near-plane clip,
     zero-length fallback, edge coverage (including sub-pixel widths), dash
     coverage, distance fade. The WGSL keeps a comment pointing at each twin.
     A twin can drift from the WGSL silently; layer 4 is the cross-check.
2. **Shader structure, CPU, in the default `cargo test`**, via `naga` (already
   in the tree through `wgpu`; added as a dev-dependency at the lockfile's
   version with `wgsl-in`):
   - every `.wgsl` file in the crate parses and validates, so a broken shader
     fails `cargo test` instead of the app's start-up;
   - the `CameraUniform` layout naga computes **for each of the six shaders**
     equals the Rust struct's `offset_of!` offsets and `size_of`. This is the
     #269 bug class tested on the WGSL side as well, not just pinned in Rust;
   - `line_mesh.wgsl`'s vertex outputs carry the interpolation D4 and D9
     require: `linear` for the screen-space values, `flat` for colour and
     fade distances, `perspective` for the world position.
3. **Producer tests, CPU, existing harnesses.** Segment counts, endpoints,
   styles and LOD slots asserted on the `LineMesh` the producers emit.
4. **GPU harness, opt-in.** A new `crates/atomcad-renderer/tests/renderer_gpu.rs`
   builds a real `Renderer` offscreen (`Renderer::new` blocked on with the
   workspace's `tokio`), uploads small hand-made meshes, renders, and asserts
   on the read-back pixels. It is behind `#[ignore]`, so plain `cargo test`
   stays GPU-free. It runs with `cargo test -p atomcad-renderer --test
   renderer_gpu -- --ignored`, at the end of every phase. If no adapter is
   found, a test prints that and returns rather than failing.

   Assertions use tolerances and pixel counts, never committed golden
   images, which would break across drivers. Examples: the coverage summed
   across a line ≈ its width; zero lit pixels in a region; a pixel's
   colour within a few levels of the expected one. This is a new kind of test
   in the repo: Phase 1 adds it to the Testing section of `rust/AGENTS.md`
   (what it is, how to run it, that it is opt-in).

The CLI-screenshot checks in the phases remain. They show a person whether
the result looks right, which no assertion does, but they are no longer the
only evidence that it works.

## Phases

Each phase leaves the build green and the app usable. Checks per phase:
`cargo fmt`, `cargo clippy`, `cargo test -j 4` (renderer, display and
structure-designer test harnesses explicitly), the GPU harness from Phase 1
on (`cargo test -p atomcad-renderer --test renderer_gpu -- --ignored`), and
`flutter analyze`. The Flutter smoke test is a manual step for the
maintainer.

### Phase 1 — Pixel scale and the extended uniform (no visible change)

- `RenderTarget { pixel_scale }` and `RenderTarget::new(device, out_w,
  out_h, supersampling)`; thumbnail and export targets built through it
  (D2). Add `THUMBNAIL_SUPERSAMPLING` next to the thumbnail sizes.
- `CameraUniform` gains `viewport_size_px` and `pixel_scale` (D3);
  `CameraUniform::new` builds it and `write_view_uniforms` becomes the only
  writer (camera uniform only until Phase 4); `set_viewport_size` reorders.
- All six shaders get the extended struct.
- Tests (CPU, `tests/renderer/`): the uniform's size (256) and the offsets of
  the new fields; `RenderTarget::dims` for the live, thumbnail and export
  cases, including a clamped export size; `CameraUniform::new` puts the
  target's size and scale in the new fields.
- Tests (naga, Testing layer 2): all shaders validate; the WGSL
  `CameraUniform` layout of all six shaders matches the Rust struct.
- The GPU harness (Testing layer 4) with its first test: a fixed scene
  rendered by the live, thumbnail and export paths has the expected size and
  no all-background image. Document the harness in `rust/AGENTS.md`.
- Expected result: pixel-identical output before and after. **Write the
  harness first**, against the unchanged renderer; it only uses the public
  `Renderer` API, which this phase does not change. Render its scene, keep
  the bytes outside the repo, then make the changes and compare.

### Phase 2 — The instanced line pipeline (all lines 1 px, anti-aliased)

- `LineSegmentInstance`, the new `LineMesh` (D7) with
  `add_line_with_uniform_color` kept as the 1 px shortcut, so producers
  compile unchanged except the removed gradient and dotted APIs (the lattice
  axes temporarily draw solid, fixed in Phase 3).
- `GPUMesh` instance path; one `line_pipeline` (D6); new `line_mesh.wgsl`
  with near-plane clipping, expansion, coverage and sub-pixel fade (D4).
- Draw-order change (D6) in both passes: lines after the opaque content in
  the main pass, and last in the gadget pass.
- Alpha component `One, OneMinusSrcAlpha` on the line pipeline and every
  other alpha-blended pipeline: labels, transparent impostors, transparent
  surfaces (D6).
- The line pipeline's own layout with the group-2 slot (D6). Until Phase 4,
  every line mesh binds the shared "visible, no fade" `LineLodUniform`, so
  the layout doesn't change again.
- `content_bounds` on segments.
- **Port the existing tests that read `vertices` / `indices`**, since this
  phase deletes them and they stop compiling. Each gets a segment-based
  accessor (`segments()` yielding `(p0, p1)`):
  - `atomcad-display/tests/display/poly_mesh_tessellator_test.rs`
    (`segment_count`)
  - `atomcad-structure-designer/tests/structure_designer/unit_cell_wireframe_test.rs`
  - `atomcad-structure-designer/tests/structure_designer/mechanosynth_trajectory_node_test.rs`
    (tool-envelope wireframe)
  - `atomcad-renderer/tests/renderer/thumbnail_test.rs` (`content_bounds`)
- Tests (CPU): `LineMesh` segment emission. Rust twins in `line_geometry.rs`
  for:
  - **near-plane clip:** both ends in front, one behind, both behind, an end
    exactly on the plane;
  - **zero-length fallback:** both ends projecting to the same pixel;
  - **coverage:** the centre, the edge and the far fringe; a sub-pixel width
    giving total coverage ≈ width.

  The clip tests build their matrices with the real `Camera` in both
  projections, not a textbook 0..1 projection, since perspective is
  `perspective_rh_gl`.
- Tests (naga): the new shader validates, and its vertex outputs carry the
  D4 interpolation.
- Tests (GPU harness):
  - **alpha:** every pixel of an export and a thumbnail containing
    anti-aliased lines has alpha 255;
  - **near-plane:** a grid at a grazing angle with the camera inside its
    extent lights no pixel above the horizon (no wedge);
  - **end-on:** the Z axis seen from straight above lights only a small blob
    at the centre, not zero pixels and not a streak;
  - **width:** a 1 px horizontal line's coverage, summed down each column,
    is ≈ 1;
  - **main-pass order:** a line behind an opaque triangle is hidden and one
    in front is visible;
  - **gadget order:** a gadget line in front of a gadget triangle keeps its
    colour where they cross.
- Check by CLI screenshot: the same scenes, by eye.

*As implemented:* `LineSegmentInstance` already carries all of D1's fields,
`lod_slot` included, so the instance layout does not change again in Phase 4;
every producer writes slot 0, and the vertex shader already drops a segment
whose slot is marked hidden in the bound `LineLodUniform`. `LineStyle` has
D7's builders including `dashed`, which fills the dash fields, but the shader
ignores them until Phase 3. `add_polyline` is Phase 3's. The twins live in
`line_geometry.rs` with tests in `tests/renderer/line_geometry_test.rs` and
`line_mesh_test.rs`.

### Phase 3 — Dashes, polylines and the producers' styles

- Dash coverage in the fragment shader, `dash_phase` from `add_polyline`
  (D5).
- Convert every producer to D8 styles; guided-placement circles become
  closed polylines; lattice axes dashed in pixels.
- Tests (CPU): `add_polyline` phases (open and closed, cumulative lengths);
  `LineStyle` builders; the dash-coverage twin (on, off and a soft dash end);
  the producer tests ported in Phase 2 extended to assert the D8 widths;
  new tests for `coordinate_system_tessellator` (which has none): grid line
  count, minor/major widths, dashed lattice axes, solid Cartesian axes.
- Tests (GPU harness):
  - **D2 end to end:** a 2 px line's summed coverage is ≈ 2 in a 1× render
    and ≈ 2 in a 2× export after the downsample;
  - **dashes:** the dash period measured along a dashed line matches
    `on + off` in both images.
- Check by CLI screenshot and a 2× export, by eye.

### Phase 4 — Grid level of detail and distance fade

- `LodSlot`, `LineMesh::add_lod_slot`, `lod_slot` in `LineStyle` and the
  instance (D9).
- `line_lod.rs`: `slot_lod`, with per-mesh, per-slot hysteresis state in the
  renderer for the live viewport.
- Per-mesh `LineLodUniform`s for meshes with slots, filled in
  `write_view_uniforms`; slot visibility and the distance fade in
  `line_mesh.wgsl`.
- The ground grid (both grids in dual mode) and the drawing-plane grids
  register a minor and a major slot each.
- Tests (CPU, `tests/renderer/`), built with the real `Camera`:
  - perspective top-down: the minor slot drops exactly at `s · f / h = T_drop`;
  - a lower camera gives proportionally shorter fade distances (`∝ √h`);
  - an eye in the plane hides the slot;
  - "no fade" is always written as `fade_end <= fade_start` (orthographic,
    entry 0);
  - orthographic: tilted (drops by `c`), exactly edge-on (hidden), and no
    distance fade in either;
  - hysteresis: no flip-back between `T_drop` and `1.25 T_drop`;
  - a 2× target with `pixel_scale` 2 makes the same choices as 1×.
- Tests (`atomcad-display`): the grid producers register the expected slots
  and tag minor and major lines with them; axes stay at slot 0.
- Tests (CPU): the distance-fade twin (1 before `fade_start`, 0 after
  `fade_end`, monotone between).
- Tests (naga): the world position output is `perspective` and the fade
  distances are `flat`.
- Tests (GPU harness):
  - a top-down grid zoomed out past the minor threshold lights no pixel
    where only minor lines would be;
  - in perspective at low elevation, grid pixels near the horizon are dimmer
    than those near the bottom of the image.
- Check by CLI screenshot: perspective at high, middle and grazing
  elevation, zoomed in and out; orthographic top and tilted.

### Phase 5 — Walkthrough and documentation

- Manual GUI walkthrough (maintainer): grid near and far, orthographic and
  perspective, zooming out through both LOD drops and orbiting down to the
  horizon (no smear, no flicker at the thresholds), lattice axes at several
  zooms, CSG wireframe, unit cell, tool envelopes in a mechanosynth scene,
  guided placement, gizmo, a thumbnail, and an export compared with the
  screen.
- Tune the D8 widths and the D9 thresholds from the walkthrough.
- Reference guide: `doc/reference_guide/ui.md` describes *Show Lattice
  Axes* as "dotted lines"; change it to dashed if the walkthrough keeps the
  6 / 4 px pattern. Nothing else there describes line appearance.

## Future work

All of these build on this design without changing it:

- **Grid fade inside the model's bounding box** (report §2.9): one more
  per-mesh factor in the fragment shader beside the D9 distance fade.
- **A grid that follows the camera:** recentring and resizing the grid on
  zoom would remove the finite-grid border D9 leaves visible. It needs a
  camera-change hook into the display crate, which D9 avoids.
- **Hidden-line wireframe:** a depth-only mesh prepass, then wireframe edges
  behind faces drawn dimmed or dashed (a second draw with depth compare
  `Greater` and a dashed style).
- **Feature edges over solid geometry** (§2.7): needs a line variant that
  wins coplanar ties (a negative bias or a small view-space offset towards
  the eye in the vertex shader).
- **HiDPI** (§2.10): set the live target's `pixel_scale` to the device pixel
  ratio.
- **MSAA** for meshes (§2.1): the line pipeline only needs the sample count;
  its coverage is analytic and unaffected.
