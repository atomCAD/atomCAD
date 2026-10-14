# Viewport Anti-Aliasing and HiDPI

Source: `reports/rendering_capabilities/rendering_capabilities_report_2026-09-21.md`
§2.1 (anti-aliasing: MSAA for meshes, analytic silhouette AA for impostors)
and §2.10 (HiDPI). Builds on `doc/design_screen_space_lines.md`, whose D2
(the render target owns its pixel scale) this design relies on throughout.

## Motivation

Two of the report's anti-aliasing items are already done: lines are
screen-space quads with an analytic anti-aliased edge
(`design_screen_space_lines.md`), and *File > Export viewport image* and the
network thumbnails render at 2× and box-downsample. What is left is the
**live viewport**, where everything except lines and labels is still a hard
one-pixel staircase:

- **Atom and bond impostors.** The silhouette comes from `discard` in the
  fragment shader: a pixel is either the sphere or the background. This is
  the edge the user looks at most, and in a dense lattice far away it turns
  into shimmer that moves with the camera (report shots `c16`, `c61`, `c62`).
- **Meshes.** CSG solids, isosurfaces, the gizmo's arrows and the pivot cube
  have real triangle edges with no coverage at all (`c01`).
- **HiDPI.** The viewport texture is allocated at *logical* size. On a 150 %
  or 200 % display Flutter scales it up, so the user sees the jaggies above
  *and* a blur, and lines authored as 1 px are drawn 1.5 or 2 device pixels
  wide and soft.

This document makes the live viewport anti-aliased with **4× MSAA**, gives the
impostors **exact per-sample silhouette coverage** inside that MSAA (one
mechanism for meshes and impostors), renders one **refined frame with
per-sample impostor shading** once the view comes to rest, so that where
impostors meet (bond–atom junctions, sphere–sphere contacts) is exact too,
and renders the live viewport at the **device pixel ratio**. Two preferences
can turn MSAA (and with it the refinement) and the device-pixel rendering
off.

### Scope

- **In:** multisampled colour and depth for the live viewport, pipelines per
  sample count, the resolve (D1–D3); per-sample silhouette coverage in the
  three impostor shaders: opaque atoms, opaque bonds, x-ray ghosts, and
  through them the gadget guide dots (D4); the still-camera refined frame
  (D5); device-pixel allocation of the live target and every consumer of the
  viewport's size in pixels (D7); two preferences, *Anti-aliasing* and *Full
  display resolution*, both on by default (D8).
- **Out (see Future work):** exact impostor–impostor intersections *while the
  view moves* (they are exact only in the refined frame), post-process AA
  (FXAA/SMAA, needs the post-process chain of report §2.6), a GPU path from
  the renderer to Flutter that skips the CPU readback, MSAA for the thumbnail
  and the export.

## Current state (facts, with references)

All in `rust/crates/atomcad-renderer/src/` unless noted.

- **Targets.** `RenderTarget { texture, texture_view, depth_texture_view,
  output_buffer, size, pixel_scale }` (`renderer.rs`), built by
  `RenderTarget::new(device, out_w, out_h, supersampling)`; the size and scale
  rule is the pure `RenderTarget::dims`. Three users: the live viewport
  (`Renderer::target`, `supersampling` 1, rebuilt by `set_viewport_size`),
  the thumbnail (`thumbnail_target`, 2×) and the export (`render_image`, a
  target per call at `IMAGE_EXPORT_SUPERSAMPLING` = 2). Colour is
  `Bgra8Unorm`, depth `Depth32Float`; `create_texture` /
  `create_depth_texture` hard-code `sample_count: 1`.
- **Pipelines.** Nine, all built once in `Renderer::new`, all with
  `MultisampleState { count: 1, .. }`: `triangle_pipeline`, `line_pipeline`,
  `atom_impostor_pipeline`, `bond_impostor_pipeline`,
  `transparent_impostor_pipeline`, three `surface_*_pipeline`s and
  `label_pipeline`. The gadget pass reuses `triangle_pipeline`,
  `atom_impostor_pipeline`, `bond_impostor_pipeline` and `line_pipeline`.
- **Passes.** `draw_to_target` runs a main pass (clear colour and depth) and,
  for the live view only (`PassFlags::gadgets`), a gadget pass that loads the
  colour and re-clears depth. Then `copy_texture_to_buffer` from
  `target.texture` and a blocking map: **every live frame is read back to the
  CPU.**
- **To Flutter.** `provide_texture` (`rust/src/api/common_api.rs`) is called
  from a persistent frame callback (`lib/common/cad_viewport.dart`); it
  renders, then `send_texture` hands the BGRA bytes to the vendored
  `packages/texture_rgba_renderer`. On Windows that plugin copies them into a
  `std::vector`, swaps R and B per pixel on the CPU (`texture_rgba.cpp`,
  `switch_rgba`) and publishes a `FlutterDesktopPixelBuffer`, which Flutter
  uploads to the GPU again. macOS (`TextRgba.swift`) and Linux
  (`texture_rgba_renderer_plugin.cc`) are also CPU pixel-buffer textures.
  **The per-frame cost of the live view is proportional to its pixel count,
  outside the GPU.**
- **Frames are on demand.** The frame callback is persistent, but Flutter
  only runs it when a frame is scheduled: `CadViewport.renderingNeeded`
  (`scheduleFrame`) after a camera move, a refresh, a resize, or any other UI
  repaint. A viewport at rest renders nothing.
- **Viewport size.** `CadViewport.build` (`lib/common/cad_viewport.dart`)
  calls `setViewportSize(width: viewportWidth.toInt(), height:
  viewportHeight.toInt())` from a `LayoutBuilder`: logical pixels, truncated.
  `devicePixelRatio` is read nowhere in `lib/` except `offscreen_capture.dart`,
  which pins it to 1.0 on purpose.
- **Who reads the size back** (`Renderer::get_viewport_size`):
  `provide_texture` (texture size), `update_gadget_pick_context`
  (`rust/src/api/api_common.rs`, world units per pixel for the gizmo's
  `MIN_HIT_RADIUS_PX` = 8), `get_viewport_image_size` (the export dialog's
  default size) and the CLI `capture_screenshot_impl`, which resizes the live
  target to the requested size and back.
- **Flutter-side pixel math** (`getRayFromPointerPos`, `_projectWithCamera`,
  the atom_edit marquee in `atom_edit_api.rs`) uses Flutter's own logical
  `viewportWidth` / `viewportHeight` and `camera.aspect`, never the renderer's
  size.
- **Impostor shaders.** `atom_impostor.wgsl`: billboard quad at 1.15 × radius;
  the fragment shader discards outside the quad's inscribed circle
  (`dot(quad_uv, quad_uv) > 1`), then ray-casts one ray per pixel through the
  interpolated `world_position`, discards on a miss, shades (PBR + rim), and
  writes `frag_depth` from the hit point. `bond_impostor.wgsl`: the same with
  a ray–cylinder test clipped flat at both ends. `transparent_impostor.wgsl`
  contains copies of both branches, selected per quad by `kind`, and blends.
  The three files share no code.
- **Tests** (in `rust/crates/atomcad-renderer/tests/`).
  `renderer/shader_test.rs` validates every `.wgsl` with naga and checks the
  six `CameraUniform` layouts; `src/line_geometry.rs` holds Rust twins of
  the line shader's arithmetic; the opt-in GPU harness `renderer_gpu.rs`
  renders through the live, thumbnail and export paths
  and can dump raw images (`ATOMCAD_GPU_DUMP_DIR`) for before/after `cmp`.

## Decisions

**Where the code goes.** Everything below except the preferences and the api
is in the renderer crate, and none of it goes into `renderer.rs` (already
over 2000 lines) beyond the calls. Three new modules; the pure functions
in them get test files under `tests/renderer/`:

| Module | Holds |
|---|---|
| `render_target.rs` | `RenderTarget` (moved out of `renderer.rs`), `TargetSpec`, `RenderQuality`, `live_target_spec` (D1, D7, D8) |
| `pipelines.rs` | `PipelineSet`, `RefinedImpostorPipelines`, the `create_*_pipeline` functions moved out of `renderer.rs`, the `PipelineLayouts`/`Shaders` they are built from (D2, D5) |
| `live_refinement.rs` | `LiveRefinement` (generation, change time, kept image), `live_frame_kind`, `REFINE_DELAY` (D5) |

`impostor_coverage.rs` (D6) is a fourth new module. `renderer.rs` keeps
`Renderer`, `draw_to_target` and the passes (D3). `RenderingPreferences` is
in `atomcad-structure-designer/src/preferences.rs` beside the other
preference sections, and reaches the renderer as a `RenderQuality` through
the api, like `surface_transparency_mode` (D8).

### D1 — MSAA 4× for the live viewport; the offscreen targets stay supersampled

The live viewport renders into 4-sample colour and depth and resolves into
the single-sample texture that is read back. The thumbnail and the export
stay single-sample: they already render at 2× and box-downsample, which is
four samples per output pixel.

Why MSAA and not supersampling for the live view:

- **Shading cost.** MSAA runs the fragment shader once per pixel per
  primitive; only coverage and depth are per sample. 2×2 supersampling runs
  it four times everywhere, and the impostor shaders (a ray cast plus PBR)
  are the frame's main cost on a large crystal.
- **No downsample pass.** The resolve is part of the render pass. A
  supersampled live view would need a GPU downsample before readback (the
  CPU box filter the export uses is out of the question at 60 fps), which is
  the post-process infrastructure of report §2.6 that does not exist yet.
- **The readback is unchanged.** Only the resolved image leaves the GPU, so
  the per-frame CPU cost is the same as today.

Why not MSAA for the offscreen targets too: memory. The export renders up to
4096 × 4096 (`IMAGE_EXPORT_MAX_SIZE` × 2); at 4 samples its colour and depth
would be 4096² × 4 × (4 + 4) bytes = 512 MiB, allocated for one image. The
supersampled image already has four samples per pixel and, unlike MSAA, also
smooths every impostor intersection (D4, *What is anti-aliased and what is
not*), so it is the better image anyway. The thumbnail is small enough for
MSAA, but one rule ("live is multisampled, offscreen is supersampled") is
easier to keep than a per-target exception, and the thumbnail gains nothing
visible.

Why 4: WebGPU guarantees sample counts 1 and 4 for both `Bgra8Unorm` and
`Depth32Float`, so 4 is portable without probing. 8 is not guaranteed and the
step from 4 to 8 is small next to the step from 1 to 4.

**The target owns its sample count**, the same way it owns its pixel scale
(`design_screen_space_lines.md` D2): it is fixed where the target is created,
and nothing drawn into it chooses. `RenderTarget` gains:

```rust
pub struct RenderTarget {
    texture: Texture,                 // single-sample: resolve target, COPY_SRC
    texture_view: TextureView,
    /// The multisampled colour attachment; `None` when `sample_count == 1`.
    msaa_view: Option<TextureView>,
    depth_texture_view: TextureView,  // `sample_count` samples
    output_buffer: Buffer,
    size: Extent3d,
    pixel_scale: f32,
    sample_count: u32,
}
```

The size rule moves into a pure, device-free spec (replacing
`RenderTarget::dims`, whose tests are ported):

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetSpec { pub size: Extent3d, pub pixel_scale: f32, pub sample_count: u32 }

impl TargetSpec {
    /// An offscreen image (`out_w × out_h`) rendered at `supersampling`×:
    /// size = out × supersampling, pixel_scale = supersampling, 1 sample.
    pub fn image(out_w: u32, out_h: u32, supersampling: u32) -> Self;
    /// The live viewport (D7): `logical_w × logical_h` logical pixels at
    /// `device_pixel_ratio`, multisampled at `sample_count`.
    pub fn viewport(logical_w: f64, logical_h: f64, device_pixel_ratio: f64,
                    sample_count: u32) -> Self;
}

impl RenderTarget { pub fn new(device: &Device, spec: TargetSpec) -> Self; }
```

**Memory of the live target** per device pixel goes from 8 bytes (colour +
depth) to 36 (4 × 4 colour, 4 × 4 depth, 4 resolve). A 1600 × 1000 viewport:
13 MB → 58 MB; the same at a device pixel ratio of 2 (D7): 230 MB. That is
acceptable on the discrete GPUs atomCAD is used on, and the reason MSAA can
be turned off (D8).

### D2 — Pipelines exist once per sample count

A wgpu pipeline is compiled for one sample count and can only draw into
attachments of that count. Since the live target is multisampled and the
offscreen ones are not, every pipeline exists twice. The nine pipelines move
out of `Renderer` into one struct in `pipelines.rs`, built by one function:

```rust
struct PipelineSet {
    sample_count: u32,
    triangle: RenderPipeline,
    line: RenderPipeline,
    atom_impostor: RenderPipeline,
    bond_impostor: RenderPipeline,
    transparent_impostor: RenderPipeline,
    surface_no_cull: RenderPipeline,
    surface_front_cull: RenderPipeline,
    surface_back_cull: RenderPipeline,
    label: RenderPipeline,
}

impl PipelineSet {
    fn new(device: &Device, layouts: &PipelineLayouts, shaders: &Shaders,
           sample_count: u32) -> Self;
}
```

`Renderer` holds `live_pipelines` (count `LIVE_SAMPLE_COUNT` = 4) and
`offscreen_pipelines` (count 1), both built in `Renderer::new`. The
`create_*_pipeline` functions take the sample count and put it in their
`MultisampleState`; nothing else in them changes (`mask: !0`,
`alpha_to_coverage_enabled: false`). `draw_to_target` picks the set whose
`sample_count` equals `target.sample_count` and asserts that one exists.

The point of the struct is that a pipeline cannot be added for one sample
count and forgotten for the other: a tenth pipeline goes into `PipelineSet`,
which is the one place that makes it exist in both counts. `rust/AGENTS.md`
says so (Phase 1).

**Turning MSAA off needs no rebuild.** With anti-aliasing off (D8) the live
target is created with 1 sample, and `draw_to_target` then picks
`offscreen_pipelines`, which already exist at that count. So the set a
target uses follows from the target alone, and switching the preference only
recreates the live target.

**Fallback.** `Renderer::new` checks
`adapter.get_texture_format_features(..).flags.sample_count_supported(4)` for
both formats. If either says no (not expected on any WebGPU-conformant
adapter), `live_pipelines` is not built, the live target always gets 1
sample whatever the preference says, and the renderer prints one line saying
anti-aliasing is unavailable. Everything else works unchanged.

### D3 — Both passes draw into the multisampled attachments; the last one resolves

- **Main pass:** colour attachment `msaa_view` (clear), depth
  `depth_texture_view` (clear). If the gadget pass follows, the multisampled
  colour is **stored** for it and there is no resolve here. Otherwise the
  pass resolves into `texture_view` (`resolve_target: Some(..)`) and the
  multisampled colour is **discarded** (`StoreOp::Discard`).
- **Gadget pass:** loads `msaa_view`, clears depth as today, resolves into
  `texture_view`, discards the multisampled colour.
- **Depth** is never read after a pass, so both passes discard it.
- For a single-sample target (`msaa_view == None`) the passes are exactly
  today's: colour `texture_view`, no resolve, everything stored.

A small helper builds the colour attachment from the target and a
`resolve: bool`, so the two cases are written once.

The readback copies `target.texture`, the resolved image, as today: the
readback code, `output_buffer`, the row padding and everything downstream of
`draw_to_target` are untouched.

Every pipeline's correctness carries over to 4 samples without change, which
is worth stating per kind because each was designed against a single sample:

| Kind | Under MSAA |
|---|---|
| Opaque meshes | Coverage and depth per sample from the triangle; edges and mesh–mesh intersections resolve. The negative depth bias still wins coplanar ties against lines. |
| Lines | Analytic coverage in alpha, as now. The quad covers all samples inside it, depth is interpolated per sample. Coverage sums are unchanged; the existing 1 px and 2 px line tests must keep passing (Phase 1). |
| Labels | SDF coverage in alpha, depth written for every covered sample, as now. |
| Transparent surfaces | Blended per sample, then resolved: the same image at a smoother edge. |
| Impostors | Unchanged by MSAA alone: the quad covers every sample and `discard` kills the fragment for all of them, so the silhouette stays a staircase. **D4 is what fixes them.** |

### D4 — Impostor silhouettes: per-sample ray tests written to `sample_mask`

The fragment shader keeps shading **once** per pixel, but decides coverage
**per sample**: it casts one cheap hit-only ray per sample position and
writes the result as `@builtin(sample_mask)`. The rasteriser ANDs this mask
with the quad's own coverage, and each sample is then depth-tested and
written independently. The silhouette is therefore resolved exactly as a
triangle edge would be, at 5 coverage levels.

**Sample positions.** WebGPU fixes the 4-sample pattern, in pixel units from
the pixel's top-left corner: `(0.375, 0.125)`, `(0.875, 0.375)`,
`(0.125, 0.625)`, `(0.625, 0.875)`; sample *i* is bit *i* of the mask. (D3D,
Vulkan's standard locations and Metal all use this pattern for 4×; were a
driver to differ, coverage would be off by a fraction of a pixel, not wrong.)
With one sample the only position is the pixel centre, `(0.5, 0.5)`.

The shaders get the count as a pipeline-overridable constant, set by
`PipelineSet::new` through `PipelineCompilationOptions::constants`:

```wgsl
override SAMPLE_COUNT: u32 = 1u;
```

**The ray of a sample.** The billboard quad is planar, so the point on it
under a sample is the interpolated `world_position` plus a linear offset:

```wgsl
// First statements of fs_main: derivatives must be taken in uniform control
// flow, before anything can discard.
let dwx = dpdxFine(input.world_position);   // world offset per +1 px in x
let dwy = dpdyFine(input.world_position);   // ... per +1 px in y (downwards)
// for sample i, with o = position_i - (0.5, 0.5):
let p = input.world_position + dwx * o.x + dwy * o.y;
```

and the sample's ray is built from `p` exactly as today's pixel ray is built
from `world_position`: from the eye through `p` in perspective, along the
view direction starting two radii behind `p` in orthographic. The offset is
exact to first order across 0.4 px, which is far below anything visible.

**The fragment, in order:**

1. Derivatives (above).
2. Early-out: today's `dot(quad_uv, quad_uv) > 1` test, widened by the
   sample footprint (the largest sample offset, 0.4 px, converted to
   `quad_uv` units with the derivative of `quad_uv`). With `SAMPLE_COUNT = 1`
   the widening is zero, so the single-sample path discards exactly what it
   does today.
3. For each of the `SAMPLE_COUNT` sample positions: a **hit-only** test
   (sphere: the `d2 > radius2` comparison and the `tca`/`t` sign checks;
   cylinder: the discriminant, the `t` sign and the length clip). Set bit *i*
   on a hit.
4. Mask zero: `discard`.
5. **Shading ray:** the pixel-centre ray if it hits; otherwise the first
   sample ray that hits. Full intersection, normal, rim, PBR and
   `frag_depth` from that ray, exactly today's code.
6. Write colour, depth and `sample_mask`.

Steps 3 and 5 share one intersection function per primitive
(`sphere_hit(origin, dir, centre, radius) -> f32` returning `t` or a
negative miss, and the same for the clipped cylinder), so the hit test and
the shading cannot disagree about where the silhouette is.

**Cost.** Four extra hit tests per impostor fragment: for a sphere about a
dozen multiply-adds each, for a cylinder about twice that, next to a PBR
evaluation that costs several times all four. No extra pass, no extra vertex
work, no sort.

**Why this and not the alternatives:**

- **Analytic coverage written into alpha, with blending.** Atoms are not
  sorted, so a fringe blends with whatever was drawn before it, and it writes
  depth, so an atom drawn later behind it is rejected there: a background-
  coloured halo around every atom in front of another. Avoiding that needs a
  second, fringe-only pass with depth writes off (doubling the vertex work of
  a million-atom scene) and still double-blends where two fringes overlap. It
  also leaves the meshes aliased.
- **Alpha-to-coverage.** Turns alpha into a mask with a fixed,
  implementation-defined pattern that knows nothing about which side of the
  pixel the edge is on. Two atoms whose half-covered edges meet in one pixel
  both pick the *same* two samples, and the background shows through as a
  seam. The positional mask above picks complementary samples for
  complementary edges, which is what makes neighbouring atoms seamless.
- **Per-sample shading** (the whole fragment shader run once per sample).
  Exact, and it would also fix the intersections below, but it costs four
  times the impostor shading on every pixel, interior included. (D5 uses
  it, but only for the one frame rendered once the view is at rest.)
- **Post-process AA (FXAA/SMAA).** Needs the post-process chain that does
  not exist yet, softens the already-anti-aliased lines and labels, and
  guesses edges from colour rather than knowing them.

**What is anti-aliased and what is not.** The depth buffer is multisampled
(4 depths per pixel); what is *not* per sample is the depth an impostor
writes. `frag_depth` is one value per fragment, written to every sample in
its mask.

- **One impostor in front of another with a depth gap between them** is a
  silhouette case and comes out right. On a pixel of the near atom's outline
  the near atom writes only the samples its rays hit; the far atom fails the
  depth test at those samples and wins the others; the resolve mixes the two
  in proportion. Draw order does not matter. This is most of what a crystal
  shows, and the *no seam* GPU test (Phase 2) checks it.
- **Two impostors that intersect or touch** are not. Where both cover the
  same sample and which one is in front changes *inside* the pixel, the one
  depth per fragment makes one of them win all of its covered samples, so
  the boundary is decided per pixel and stays a staircase. That happens
  wherever the two surfaces are within about one pixel's depth range of each
  other:
  - in ball-and-stick, the circle where every bond enters its atom (on every
    atom, not a rare case);
  - in space-filling, the contact curves between overlapping spheres,
    clearest between elements of very different colour (C against H, report
    shot `c11`);
  - the same junctions and contacts between x-ray ghosts.

  All of these are one problem: two impostors meeting, each knowing only
  itself. D4 deliberately does not special-case any of them (a bond shader
  that knows about its end atoms would fix the junction and leave the
  contacts, at the price of coupling the shaders and growing the bond
  vertex). Instead they are exact in the **refined frame** (D5), which is
  what the user sees whenever the view is at rest, and per-pixel only while
  it moves.

Exports and thumbnails have none of these limits, since supersampling shades
every sample.

**The rim highlight** depends on `NdotV` at the shading point and is
unchanged.

**Transparent impostors** get the same treatment in both branches of
`transparent_impostor.wgsl`. A ghost's colour is blended into each covered
sample and the resolve averages, which is correct coverage for a blended
surface as well.

**Gadget impostors** (the guide dots and their connectors) are drawn with the
same atom and bond pipelines and are anti-aliased with no further work.

### D5 — At rest, one refined frame with per-sample impostor shading

Once the view stops changing, the renderer draws one more frame in which the
impostor shaders run **once per sample** instead of once per pixel. Each
sample then has its own ray, its own hit, its own colour and its own depth, so
every impostor–impostor boundary (bond–atom junctions, sphere–sphere
contacts, ghost junctions) is resolved exactly as a mesh intersection is, at
4× the impostor shading cost of a normal frame, paid once. While the view
moves, frames are D4's; when it stops, the jaggies D4 leaves disappear. The
refinement exists only with *Anti-aliasing* on (D8): it needs the
multisampled target.

**Why per-sample shading on the live target, not a supersampled frame.**
Both give four shaded samples per pixel. A separate 2× target would cost its
own memory (about 200 MB at a device pixel ratio of 2, D7, on top of the MSAA
target of D1) and a downsample, which on the GPU is the missing post-process
chain and on the CPU is the export's box filter at 4× the live pixel count.
Per-sample shading reuses the MSAA target and its resolve: nothing is
allocated, and the readback is unchanged.

**The shaders.** Each impostor shader (atom, bond, and both branches of the
transparent one) gets a second fragment entry point, `fs_main_per_sample`,
that takes `@builtin(sample_index)`. Taking that builtin is what makes the
GPU run the entry point per sample. The inputs keep their default
(pixel-centre) interpolation, so the vertex stage and its output struct are
unchanged: the entry point takes the derivatives first (as in D4, before
anything can discard), builds its sample's ray with D4's machinery,
`sample_point(world_position, dwx, dwy, offset[sample_index])`, then runs the
full intersection, shading and `frag_depth` of today's code on that ray, and
discards on a miss. It writes no `sample_mask`: a per-sample invocation only
ever covers its own sample. Everything below the ray construction is shared
with `fs_main`, so the two entry points cannot disagree about the surface.

Meshes, lines and labels need no refined variant: their depth is already per
sample under MSAA (D3), and their colour does not change across a pixel in
any way that matters.

**The pipelines.** `PipelineSet` gains

```rust
/// D5: per-sample-shading variants of the impostor pipelines. `Some` only
/// in a multisampled set, and only on an adapter with
/// `DownlevelFlags::MULTISAMPLED_SHADING`.
refined: Option<RefinedImpostorPipelines>,  // atom, bond, transparent
```

and `PassFlags` gains `refined: bool`. With it set, `draw_to_target` draws
the impostor meshes of **both** passes (so also the guide dots) with the
refined pipelines and everything else as usual. `Renderer::new` reads
`MULTISAMPLED_SHADING` from the adapter's downlevel capabilities; every
DX12, Vulkan and Metal adapter has it, only GL adapters can lack it. Without
it there is no refinement and the renderer says so once, like the MSAA
fallback of D2.

**When to refine.** The renderer keeps a **live generation** counter that
changes whenever what the live frame shows may have changed. Because D5 keeps
and reuses the refined image, a *missed* bump is a bug the user sees: the
viewport keeps showing an old picture until something else changes. A
*spurious* bump only postpones the refined frame. So the rule is written to
over-bump, and it is placed where every change already passes, not at the
callers:

- **every `pub fn(&mut self)` on `Renderer` except `render*` and getters**
  bumps: mesh uploads (`update_all_gpu_meshes`, `update_background_mesh`),
  camera setters, `set_surface_transparency_mode`, `set_viewport_request`,
  `set_render_quality`, and whatever is added later;
- **`write_view_uniforms(View::Live, ..)`** bumps too. That covers the api
  code that edits the `pub camera` field directly, because every such edit
  is followed by `update_camera_buffer()`, which writes through it. That
  method takes `&self`, so the counter is a `Cell<u64>`.

The background colour is passed into every render rather than set, so
`render` compares it with the previous one and treats a difference like a
bump. `Renderer::render` then decides:

| State | What it renders |
|---|---|
| Refinement unavailable (1-sample live target, no refined pipelines, or delay `None`) | a normal frame, always |
| Generation or background changed since the last render | a normal frame (D4), and remembers the time of the change |
| Unchanged, but less than `REFINE_DELAY` since the change | a normal frame |
| Unchanged, `REFINE_DELAY` elapsed, not refined yet | **the refined frame**, and keeps its read-back bytes |
| Unchanged, already refined | the kept bytes, with no GPU work |

The table is a pure function, `live_frame_kind(generation_changed,
since_change, already_refined, refinement_available) -> LiveFrameKind`,
tested without a device (its first row is `refinement_available = false`).
It lives in a new module `live_refinement.rs` of the renderer crate,
together with the state it reads, which `Renderer` owns and `render`
consults; `renderer.rs` keeps only the call:

```rust
pub struct LiveRefinement {
    generation: Cell<u64>,         // bumped through `&self` (above)
    seen_generation: u64,          // the generation of the last render
    background: Option<[u8; 3]>,   // the background of the last render
    changed_at: Instant,           // when either last changed
    delay: Option<Duration>,       // `set_refine_delay`; default `Some(REFINE_DELAY)`
    last_kind: Option<LiveFrameKind>,
    kept: Option<Vec<u8>>,         // the refined frame's bytes, until the next change
}
pub enum LiveFrameKind { Normal, Refined, Kept }
```

The delay is a field, not only a constant: `set_refine_delay(Option<Duration>)`
sets it (`None` means `render` never refines on its own, and `refine_in`
below is then always `None`), so tests can make the decision deterministic (Testing, layer
4). Two read-only accessors, `live_generation() -> u64` and
`last_frame_kind() -> Option<LiveFrameKind>`, expose the decision to tests
and to debugging.

The delay matters because a frame with an unchanged generation does not mean
the user has stopped: any unrelated Flutter repaint (a hover in the node
network, a panel animation) runs the frame callback too, and a refined frame
in the middle of a drag would stall the next mouse move. `REFINE_DELAY` is
about 150 ms (tuned in the walkthrough, Phase 5).

**The deferred frame.** Frames are on demand (Current state), so after the
last frame of a camera drag nothing would ever ask for the refined one. The
Rust side says when one is owed, in the return value of the frame itself.
`provide_texture` returns an `f64` frame time today, which Dart ignores
(`cad_viewport.dart`); it becomes

```rust
// rust/src/api/common_api.rs
pub struct APIFrameInfo {
    pub frame_time: f64,
    /// Seconds until a refined frame is due; `None` when none is owed
    /// (already refined, anti-aliasing off, or unsupported).
    pub refine_in: Option<f64>,
}
```

so the answer comes from the same render and costs no second FFI call per
frame. After each `provideTexture`, `CadViewport` hands `refine_in` to a
small `RefinementTimer` (`lib/common/refinement_timer.dart`, a plain class
so it can be unit-tested without the viewport): it (re)arms a single `Timer`
for that long whose callback is `renderingNeeded()`, cancels it when
`refine_in` is `None`, and cancels it on dispose. A new frame cancels and
re-arms it, so it only fires once the view has been still for
`REFINE_DELAY`.

**Stills elsewhere.** `Renderer::render_still(background)` renders the
refined frame unconditionally (a normal frame when refinement is not
available), bypassing `live_frame_kind` and leaving the kept image alone.
The CLI `capture_screenshot_impl` calls it, since a screenshot is a still by
definition, and the Phase 3 GPU tests use it to get a refined frame without
waiting out `REFINE_DELAY`. The export and the thumbnail are untouched
(always supersampled, D1).

**Cost and risk.** The refined frame shades every impostor sample: about four
times the impostor cost of a normal frame. `provide_texture` blocks the UI
thread, so on a large scene the refined frame delays whatever input arrives
during it, at most once per rest. Camera drags are coalesced
(`_flushPendingCameraDrag`), so the delay is one late frame, not a backlog.
Phase 3 measures it on the nanobeam. If the refined frame takes too long to
be acceptable there, the remedy is a longer `REFINE_DELAY`, not a different
mechanism. The switch from the D4 frame to the refined one is a visible
change only at impostor–impostor boundaries; whether that reads as a pop is
judged on screen (Phase 5).

### D6 — One Rust twin for the coverage arithmetic

The three shaders do not share code (WGSL has no includes, and each `.wgsl`
is validated standalone by `shader_test.rs`), so the sample positions, the
sample-ray construction and the two hit tests are written into each file.
As with the lines, the arithmetic gets a Rust twin, `impostor_coverage.rs`
in the renderer crate:

```rust
pub const SAMPLE_POSITIONS_4X: [[f32; 2]; 4];
pub fn sample_offsets(sample_count: u32) -> &'static [[f32; 2]];  // relative to the centre
pub fn sample_point(world_position: Vec3, dwx: Vec3, dwy: Vec3, offset: [f32; 2]) -> Vec3;
pub fn sphere_hit(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32>;
pub fn cylinder_hit(origin: Vec3, dir: Vec3, start: Vec3, end: Vec3, radius: f32) -> Option<f32>;
pub fn coverage_mask(hits: impl Iterator<Item = bool>) -> u32;
```

`sample_point` serves both D4's coverage rays and D5's per-sample shading
ray. Each WGSL copy carries a comment naming its twin. The GPU harness is the
cross-check that a twin has not drifted, as for the lines.

### D7 — HiDPI: the live target is allocated in device pixels, and its pixel scale is the device pixel ratio

`design_screen_space_lines.md` D2 already decided where this goes: every
screen-space length is authored in logical pixels, and the target's
`pixel_scale` converts. With both preferences on, the live target becomes
`TargetSpec::viewport(logical_w, logical_h, dpr, LIVE_SAMPLE_COUNT)`:

- `size = (round(logical_w · dpr), round(logical_h · dpr))`, each at least 1;
- `pixel_scale = dpr`.

Lines keep their apparent width, grid level-of-detail keeps its thresholds,
and impostors and meshes simply get more pixels. No shader changes.

With *Full display resolution* off (D8) the ratio passed to
`TargetSpec::viewport` is 1.0 instead: the target is the logical size,
Flutter scales it up as it does today, and every consumer below still gets
the right numbers, because they read the size and the scale from the target.

**The API takes logical size and the ratio, as doubles:**

```rust
// rust/src/api/common_api.rs
pub fn set_viewport_size(logical_width: f64, logical_height: f64, device_pixel_ratio: f64);
```

Flutter passes `constraints.maxWidth`, `constraints.maxHeight` and
`MediaQuery.devicePixelRatioOf(context)` without truncation. Reading the
ratio through `MediaQuery` inside `build` makes the viewport rebuild, and so
re-size, when the window moves to a monitor with a different scale. The
renderer side becomes `Renderer::set_viewport_request(logical_w, logical_h,
dpr)`, which derives the target together with the preferences (D8), and
`camera.aspect` is the target's width over its height (it differs from the
logical aspect by under a pixel of rounding).

**Every consumer of the size, and which size it needs:**

| Consumer | Needs | Change |
|---|---|---|
| `provide_texture` → `send_texture` | device px | none; `get_viewport_size` keeps returning the target's size |
| `update_gadget_pick_context` (`MIN_HIT_RADIUS_PX`, logical) | logical px | use a new `Renderer::get_viewport_logical_size()` (size ÷ `pixel_scale`); the arithmetic moves into a pure `gadget_pick_context(camera, logical_viewport_height)` beside it, for testing |
| `get_viewport_image_size` (export dialog default) | logical px | logical size; see open question 1 |
| CLI `capture_screenshot_impl`, no size given | what is on screen | `render_still` on the live target as it is (D5), no resize |
| CLI `capture_screenshot_impl`, size given | the requested px | save `Renderer::viewport_request()` (new getter: the last `(logical_w, logical_h, dpr)`), `set_viewport_request(w, h, 1.0)`, `render_still`, then `set_viewport_request` with the saved values |
| Line widths, dashes, grid LOD | render px via `pixel_scale` | none |
| Flutter picking, projection, marquee selection | logical px | none (they never read the renderer's size) |
| `Renderer::new` | — | ratio 1 until the first `set_viewport_request`; `RenderQuality::default()` has both fields `true`, matching the preference defaults |

**Displaying it.** Flutter's `Texture` widget maps the texture onto the
widget's logical rectangle, so a texture of exactly the device-pixel size is
shown 1:1, provided the widget sits on whole device pixels. At a fractional
ratio (125 %, 150 %) a panel edge can fall between device pixels, and the
default `FilterQuality.low` then resamples by a fraction of a pixel, a slight
softening; `FilterQuality.none` avoids that but can drop or repeat a column
when the rounded size and the rectangle differ by one. Which one looks right
is checked on a real display in the walkthrough (Phase 5); the code change
is one argument.

**Cost.** The live frame's CPU-side cost scales with the device pixel count,
because every frame is read back and copied through the plugin (Current
state). A 1600 × 1000 logical viewport is 6.4 MB per frame at 100 %, 14.4 MB
at 150 % and 25.6 MB at 200 %. GPU cost scales the same way, and the MSAA
memory of D1 and the refined frame of D5 by the same factor. Phase 4
measures `provide_texture` before and after on the maintainer's display and on the million-atom nanobeam; the
design does not assume the result. A user can always turn *Full display
resolution* off (D8); if the frame time is a problem with it on, the cheap
reductions, in order, are:

1. render into `Rgba8Unorm` and drop the plugin's per-pixel R/B swap
   (`switch_rgba` on Windows, and its counterparts), which is one full pass
   over the frame on the CPU;
2. drop the copy into a fresh `std::vector` in
   `FlutterRgbaRendererPluginOnRgba` by swapping in the caller's buffer.

### D8 — Two preferences: *Anti-aliasing* and *Full display resolution*, both on by default

Both trade speed for quality in a way the user can feel, and neither adds a
code path the design does not already have:

| Preference | Default | Off means | What a change does |
|---|---|---|---|
| *Anti-aliasing* | on | the live target has 1 sample, and there is no refined frame (D5) | recreates the live target (D2: the 1-sample pipelines exist anyway) |
| *Full display resolution* | on | the live target's ratio is 1.0 | recreates the live target (D7) |

Neither touches the thumbnail or the export: those are always supersampled,
at their own size.

**Not preferences, on purpose:**

- **Separate MSAA for meshes and for impostors.** Every pipeline drawing into
  the viewport must have the target's sample count, so meshes cannot be
  multisampled while impostors are not. "Impostor AA off" could only mean the
  impostor shaders claiming all four samples, which saves four hit tests per
  pixel, nothing measurable next to the shading.
- **The refined frame (D5) on its own.** It is part of *Anti-aliasing*: it
  needs the multisampled target, and with the target it costs nothing until
  the view is at rest. If it proves too slow on a large scene, the remedy is
  a longer `REFINE_DELAY`, not a switch.

**Where they live.** A new `RenderingPreferences { anti_aliasing: bool,
full_display_resolution: bool }` in
`atomcad-structure-designer/src/preferences.rs`, a `#[serde(default)]` field
of `StructureDesignerPreferences` beside the others, with a `Default` of both
`true`, so an existing `preferences.json` without the section loads with both
on. It follows the existing sections' pattern through `api/` (twin and `From`
impls) to a new **Rendering** section in the Preferences dialog
(`lib/structure_designer/preferences_window.dart`).

**How they reach the renderer.** Like `surface_transparency_mode`: pushed on
every refresh, in `refresh_structure_designer` (`api_common.rs`) next to
`set_surface_transparency_mode` (and `set_structure_designer_preferences`
refreshes), because the
renderer crate cannot see the preferences types. The renderer keeps the last
viewport request and the quality settings, and derives the live target from
both:

```rust
/// `Default`: both `true`, like the preferences.
pub struct RenderQuality { pub anti_aliasing: bool, pub full_display_resolution: bool }

impl Renderer {
    /// From `set_viewport_size`: what Flutter laid out.
    pub fn set_viewport_request(&mut self, logical_w: f64, logical_h: f64, dpr: f64);
    /// From every refresh: the two preferences.
    pub fn set_render_quality(&mut self, quality: RenderQuality);
}

/// Pure, so it is tested without a device. In `render_target.rs`.
pub fn live_target_spec(logical_w: f64, logical_h: f64, dpr: f64,
                        quality: RenderQuality, msaa_supported: bool) -> TargetSpec;
```

Each setter recomputes `live_target_spec` and recreates the live target (and
rewrites the live uniforms) **only when the spec changed**, so the push on
every refresh costs one comparison.

## Open questions

1. **Export at device size.** *Export viewport image* at "viewport size"
   writes the logical size, so on a 200 % display the PNG has half the
   pixels the screen shows (with lines at their authored width, i.e. the same
   picture, smaller). Offering the device size as a third choice in the
   dialog is easy; it needs `pixel_scale = 2 × dpr` for the export target so
   that lines scale too. Recommendation: not now; the custom size covers it.

## Testing

Five layers, cheapest first: the four of `design_screen_space_lines.md`
plus Dart unit tests for the one piece of Flutter logic. The concrete tests
are listed in the phase that builds what they test.

1. **Pure functions, CPU** (`crates/atomcad-renderer/tests/renderer/`, and
   the owning crate's tests for preferences). Everything with arithmetic or
   a decision is a pure function: the target specs, the coverage twin (D6),
   when to refine (`live_frame_kind`, D5). One source-text check also lives
   here: **every mutating `pub fn` on `Renderer` bumps the live generation**
   (D5). It reads `renderer.rs` the way `shader_test.rs` reads the `.wgsl`
   files, finds each `pub fn …(&mut self` in `impl Renderer`, and requires
   its body to call the bump unless it is on a short allowlist (`render`,
   `render_still`, `render_thumbnail`, `render_image`). A behavioural test
   can only cover the setters that exist when it is written; this one also
   catches the next one.
2. **Shader structure, naga** (`shader_test.rs`): the shaders validate and
   carry the entry points, builtins and constants the design relies on.
3. **API tests** (`rust/tests/renderer_api/`). This harness has **no GPU and
   no `CADInstance`**, so it tests pure api helpers only. Anything that needs
   a `Renderer` is layer 4, and api logic that should be tested is first
   split into a pure function (Phase 4 does this for the gadget pick
   context).
4. **GPU harness** (`crates/atomcad-renderer/tests/renderer_gpu.rs`,
   `--ignored`): pixel assertions with tolerances and counts, never golden
   images; byte-identity checks via `ATOMCAD_GPU_DUMP_DIR` dumps. Every
   existing GPU test must keep passing in every phase, notably the line
   coverage sums once the live target is multisampled. Four rules for this
   design:
   - **Dumps first.** Each phase starts by dumping the harness from the
     previous commit; its "byte-identical" checks compare against that.
   - **Capability skips.** A test that needs 4× MSAA or
     `MULTISAMPLED_SHADING` and runs on an adapter without it prints so and
     returns, like the existing no-adapter skip. The fallbacks themselves are
     covered on the CPU (`live_target_spec`, `live_frame_kind`).
   - **No timing.** The harness's `try_renderer` switches automatic
     refinement off (`set_refine_delay(None)`, D5), so the existing tests
     that call `render` twice and compare stay deterministic however slow
     the machine is. Refinement tests switch it on with a zero delay; no
     test sleeps.
   - **Assert decisions, not only pixels.** `Renderer::live_generation()`
     and `Renderer::last_frame_kind()` (D5) let a test check *why* a frame
     looks the way it does (normal, refined or kept image), which pixels
     alone cannot distinguish when nothing on screen changes.
5. **Dart unit tests** (`flutter test`, `test/`, no Rust library; not the
   human-only `integration_test/`). The refinement timer is the only Flutter
   logic in this design, and if it breaks, every Rust test still passes and
   the refined frame simply never comes. It is a small class,
   `RefinementTimer` in `lib/common/`, that `CadViewport` owns; its tests
   drive it with `testWidgets` and `tester.pump(duration)`.

As in the lines work, each new GPU test is confirmed to **fail with its fix
removed** before it is trusted; each phase names the mutation to try.

**Not automated, on purpose** (Phase 5's walkthrough): how the texture looks
in Flutter at a fractional scale (`FilterQuality`), whether the switch to
the refined frame reads as a pop, whether `REFINE_DELAY` feels right, moving
the window between monitors, and the `MediaQuery` read that feeds the
device pixel ratio (one line, only observable on a real display).

## Phases

Each phase leaves the build green and the app usable. Checks per phase:
`cargo fmt`, `cargo clippy`, `cargo test -j 4` (the renderer harness and the
api harnesses explicitly), the GPU harness (`cargo test -p atomcad-renderer
--test renderer_gpu -- --ignored`), `flutter analyze`, and `flutter test`
(the Dart unit tests in `test/`; from Phase 3 on). The Flutter smoke
test is a manual step for the maintainer. Because `flutter run` loads the
release library, a GUI check needs `cargo build --release` first.

### Phase 1 — MSAA for the live viewport (meshes anti-aliased; impostors not yet)

- First, as a pure move with no behaviour change: `RenderTarget` into
  `render_target.rs`, the pipeline constructors into `pipelines.rs` (Where
  the code goes). The GPU harness dumps before and after the move must be
  byte-identical.
- `TargetSpec` and the new `RenderTarget::new`; `sample_count` and
  `msaa_view` on the target (D1). Thumbnail and export targets via
  `TargetSpec::image`; the live one via `TargetSpec::viewport(w, h, 1.0,
  LIVE_SAMPLE_COUNT)` (the ratio arrives in Phase 4).
- `PipelineSet`, built twice; `draw_to_target` picks by sample count; the
  adapter check and fallback (D2).
- The pass and resolve changes (D3).
- The *Anti-aliasing* preference end to end (D8): `RenderingPreferences`,
  the api twin, the dialog's **Rendering** section, `RenderQuality`,
  `live_target_spec`, `set_render_quality`, and the push on refresh, all
  with `anti_aliasing` only and no device pixel ratio; Phase 4 adds both.
  The live viewport request is still today's `set_viewport_size(w, h)`.
  Reference guide: the new section in `doc/reference_guide/ui.md`'s
  Preferences Dialog.
- `rust/AGENTS.md`: next to *Per-view uniforms come in pairs*, a line that
  pipelines exist once per sample count in `PipelineSet`, and a new pipeline
  goes there.
- Tests (CPU):
  - `TargetSpec::image`: the live, thumbnail and export cases ported from
    the `RenderTarget::dims` tests; `sample_count` is 1;
  - `live_target_spec`: anti-aliasing on gives 4 samples, off gives 1, on
    but MSAA unsupported gives 1;
  - `RenderingPreferences` (`atomcad-structure-designer` tests):
    `anti_aliasing` defaults to `true`, and a preferences JSON without the
    section loads with it `true`.
- Tests (GPU):
  - **mesh edge:** a tilted opaque square in the live view has pixels
    strictly between its colour and the background along its edge;
  - **offscreen unchanged:** the export and thumbnail renders of the fixed
    scene are byte-identical to before the phase;
  - **anti-aliasing off:** the live render of the fixed scene is
    byte-identical to the pre-phase live render, and toggling the setting
    off and on again restores the anti-aliased image;
  - **gadget pass:** the existing gadget-line test passes at 4 samples (the
    multisampled colour survives from the main pass into the gadget pass).

  Mutation: force the live sample count to 1 and confirm the mesh-edge test
  fails.
- Expected: mesh edges in the live view smooth; lines and labels look the
  same (their coverage was already analytic); impostors unchanged; exports
  and thumbnails byte-identical. Write the byte-identity
  dumps **before** the change.
- Effort: 2–2.5 d (the preference plumbing is about half a day).

### Phase 2 — Per-sample impostor coverage

- `impostor_coverage.rs` (D6).
- `atom_impostor.wgsl`, `bond_impostor.wgsl` and both branches of
  `transparent_impostor.wgsl`: derivatives first, widened early-out, sample
  hit tests, shading ray, `sample_mask` output (D4); `SAMPLE_COUNT` set by
  `PipelineSet::new`.
- Measure `provide_texture` on the nanobeam, far enough out that atoms are a
  few pixels across, before and after the phase. D4 shades fragments whose
  centre misses but a sample hits, and the rasteriser now emits fragments for
  any covered sample, so the number of shaded impostor fragments grows most
  in exactly this view. Record the numbers in this document; report back
  before Phase 3 if the regression is large.

  **Measured (2026-10-14): the regression is large.** Not on the nanobeam
  itself (it needs the whole app; the GUI was not driven) but on a stand-in
  with its atom count, rendered through `Renderer::render` in a temporary
  release-mode harness: a diamond slab of 60 × 60 × 37 cells, 1.07 M atom
  impostors (r = 0.45 Å) and 2.1 M bond impostors (r = 0.12 Å), a
  1600 × 1000 live target, mean of 20 frames after 3 warm-up frames,
  readback included, viewed obliquely from three distances:

  | Frame time (ms) | far, ~3 px/atom | mid, ~10 px/atom | near, ~30 px/atom |
  |---|---|---|---|
  | *iGPU (AMD Radeon, Vulkan; the app's default adapter)* | | | |
  | Anti-aliasing off (the pre-design path) | 29 | 67 | 63 |
  | Phase 1 (4× MSAA, impostors per pixel) | 45 | 95 | 85 |
  | Phase 2 | 78 | 160 | 124 |
  | *dGPU (RTX 3060 Laptop, Vulkan)* | | | |
  | Phase 1 | 8.7 | 14.1 | 10.9 |
  | Phase 2 | 15.9 | 26.7 | 18.0 |

  So Phase 2 costs +45–90 % over Phase 1 on both GPUs, at every zoom, not
  only far out. D4's cost argument ("four hit tests … next to a PBR
  evaluation that costs several times all four") does not hold. The impostor
  shaders write `frag_depth`, so there is no early depth test, and on a deep
  crystal every fragment of every quad, occluded or not, runs the coverage
  code. Diagnostic variants on the iGPU (far / mid):

  | Variant | far | mid |
  |---|---|---|
  | Phase 2 shaders on the 1-sample path, mask `0xF` (cost of writing `sample_mask`) | 47 | 98 |
  | Phase 2 arithmetic, mask widened to `0xF` on output (cost of partial masks) | 80 | 158 |
  | Phase 2 without the per-sample loop | 50 | 105 |
  | Per-sample tests replaced by a linearised distance (`d + ∂d/∂x·o.x + ∂d/∂y·o.y`, prototype) | 67 | 144 |

  Writing a sample mask and partial coverage cost next to nothing; the
  per-sample loop is the cost, and it is paid even where the shipped
  early-outs (below) let a fragment skip it, presumably because a wave runs
  the loop if any of its fragments needs it, and small impostors put an edge
  in almost every wave. Swapping the loop for the linearised test recovers
  only about a third. Two early-outs were added to step 3 of D4 and kept
  (sound, unit-tested against perspective billboards, a few percent at mid
  range): a pixel-centre ray further than `sample_reach` outside the
  silhouette discards without per-sample tests, and for spheres one further
  inside takes all four samples.
- Tests (CPU):
  - `impostor_coverage`: the 4× offsets are WebGPU's and are centred (sum
    zero); the single-sample offset is the centre; a sphere hit by every
    sample gives `0b1111` and missed by all gives 0; **two half-planes
    meeting at the pixel centre give complementary masks** (OR = `0b1111`,
    AND = 0), the property D4 rests on; `sample_point` with orthogonal
    derivatives lands on the expected points; the hit tests agree with the
    shading code's intersection on hits, including the cylinder's length
    clip.
- Tests (naga):
  - the three impostor shaders validate, declare `override SAMPLE_COUNT`,
    and have a `sample_mask` builtin in their fragment output;
  - the derivatives are taken before the first `discard` (checked on the
    WGSL source: a `dpdx` after a `discard` is the regression to catch).
- Tests (GPU):
  - **sphere silhouette:** the summed coverage of a sphere of radius 20 px is
    ≈ π r² (within a few percent). For r = 0.7 px a single position is too
    coarse to compare with π r² (4 samples quantise to 0.25 px² against an
    area of 1.54 px²): render it at 16 sub-pixel offsets and require the
    mean to be ≈ π r² within 10 %, and that the offsets do not all give the
    same coverage (the single-sample render is all-or-nothing);
  - **no seam:** two opaque spheres overlapping in screen space, one in
    front, drawn in both orders: along the front sphere's silhouette over the
    back one, no pixel is closer to the background colour than to either
    sphere's;
  - **bond:** a cylinder's long edges have intermediate pixels; its flat
    ends do not paint outside the clip;
  - **ghost:** a transparent atom's silhouette has intermediate alpha
    composites, and every pixel of the read-back image is opaque;
  - **gadget impostor:** a guide dot in the gadget pass is anti-aliased;
  - **offscreen unchanged:** the export and thumbnail renders of the fixed
    scene are byte-identical to before the phase.

  Mutation: force the mask to `0b1111` (silhouette, no-seam and ghost tests
  fail).
- Expected: atom and bond silhouettes smooth in the live view, including at
  a distance and in orthographic; bond–atom junctions and sphere contacts
  still per pixel (Phase 3); exports byte-identical.
- Effort: 2–3 d.

### Phase 3 — Still-camera refinement

- First, before any other change of the phase: `set_refine_delay`,
  `live_generation` and `last_frame_kind` (D5), and the GPU harness's
  `try_renderer` switching automatic refinement off (Testing, layer 4), so
  the existing GPU tests never see a refined frame.
- `fs_main_per_sample` in the three impostor shaders (both branches of the
  transparent one), sharing everything below the ray construction with
  `fs_main` (D5).
- `RefinedImpostorPipelines` in the multisampled `PipelineSet`, gated on
  `DownlevelFlags::MULTISAMPLED_SHADING`; `PassFlags::refined`, which
  switches the impostor draws of both passes.
- `live_refinement.rs`: `LiveRefinement`, `LiveFrameKind`,
  `live_frame_kind`, `REFINE_DELAY`. The generation bumps of D5 (every
  mutating `pub fn` on `Renderer`, plus `write_view_uniforms(View::Live,
  ..)`); the background comparison; the call from `Renderer::render`;
  `render_still`.
- `provide_texture` returns `APIFrameInfo`; regenerate the bindings;
  `lib/common/refinement_timer.dart` and its use in `CadViewport` (D5).
- CLI `capture_screenshot_impl` calls `render_still`.
- `rust/AGENTS.md`, beside the `PipelineSet` line of Phase 1: a new `pub
  fn(&mut self)` on `Renderer` bumps the live generation, or the refined
  image goes stale.
- Measure the refined frame's `provide_texture` time on a small scene and on
  the nanobeam; record it here.

  **Measured (2026-10-14).** As in Phase 2, on a stand-in rather than the
  nanobeam itself: the same 1.07 M-atom / 2.1 M-bond diamond slab, a
  1600 × 1000 live target, viewed obliquely, through `Renderer::render`
  (normal, mean of 20 after 3 warm-ups) and `render_still` (refined, mean
  of 10), readback included, in a temporary release-mode harness; the
  high-performance adapter `Renderer::new` now picks (RTX 3060 Laptop,
  Vulkan). The small scene is the same slab at 2 × 2 × 2 cells (64 atoms).

  | Frame time (ms) | far, ~3 px/atom | mid, ~10 px/atom | near, ~30 px/atom |
  |---|---|---|---|
  | Slab, normal frame (D4) | 15.2 | 22.0 | 21.0 |
  | Slab, refined frame (D5) | 19.1 | 33.4 | 33.7 |
  | Small scene, normal | 2.4 | 2.5 | 2.4 |
  | Small scene, refined | 2.4 | 2.3 | 2.5 |

  The refined frame costs 1.3–1.6× a normal one, not the 4× of the impostor
  shading alone: it skips D4's per-sample hit loop, and the rest of the frame
  (vertex work, meshes, readback) is unchanged. At ~34 ms, paid once per
  rest, it is not worth a longer `REFINE_DELAY` (150 ms for now; Phase 5
  tunes it on screen). A small scene is all readback.

  **Two deviations from D5 found during implementation:**

  - *The per-sample block is cut off without `MULTISAMPLED_SHADING`.* D5
    gated only the refined *pipelines* on the downlevel flag, but wgpu
    validates the whole shader module against the device's capabilities, so
    a module that contains an entry point using `sample_index` fails to
    compile on such an adapter even if no pipeline uses it, which would take
    the ordinary impostor pipelines down with it. Each impostor shader
    therefore ends with its per-sample block after a marker line, and
    `Shaders::new` builds the modules from the source cut at the marker when
    the flag is missing (`pipelines::without_per_sample_shading`, tested with
    naga against capabilities lacking the flag).
  - *The sample offset is measured, not assumed.* D5 builds a sample's ray
    as `sample_point(world_position, dwx, dwy, offset[sample_index])`, which
    is right only if `world_position` is interpolated at the pixel centre.
    WGSL says it is, but whether a backend running the shader per sample
    keeps centre interpolation is not something to bet the picture on (if
    it interpolated at the sample, the offset would be applied twice). The
    shaders instead project `world_position` back to the target
    (`target_pixel_of`) and offset from wherever it actually was
    (`per_sample_offset`), which is right under either rule; the Rust twins
    are tested for both.
- Reference guide: the *Anti-aliasing* entry in `ui.md` says that a still
  view sharpens once more after it comes to rest.
- Tests (CPU):
  - `live_frame_kind`: every row of D5's table; `refinement_available =
    false` never gives a refined frame; a generation change after a refined
    frame gives a normal frame again; a delay of `None` never refines;
  - **every mutating `pub fn` bumps** (source-text check, Testing layer 1).
    Mutation: remove the bump from `set_surface_transparency_mode`.
- Tests (naga):
  - the three impostor shaders have an `fs_main_per_sample` entry point
    taking `sample_index`;
  - Phase 2's derivatives-before-`discard` check covers the new entry point
    too.
- Tests (GPU). Refined frames for pixel tests come from `render_still`;
  tests of the decision set the delay to zero and check `last_frame_kind`.
  - **bond–atom junction:** a bond entering a differently coloured atom,
    seen obliquely, refined: along the junction circle there are pixels
    strictly between the bond's and the atom's colours; the same scene in a
    normal frame has none (the D4 staircase), which pins down that the
    refined frame is what fixes it;
  - **sphere contact:** two overlapping spheres of different colour in
    space-filling, refined: intermediate pixels along the contact curve;
  - **refined matches normal away from edges:** for the fixed scene and for
    a ghost over an opaque atom, pixels not on an impostor edge differ
    between the refined and the normal frame by at most a couple of levels
    per channel, and every pixel of the refined frame is opaque. This
    catches a per-sample entry point that shades or blends differently from
    `fs_main` (a ghost blended once per sample *and* weighted as if it were
    the whole pixel, a wrong sample offset), which the edge tests above
    would not;
  - **the decision:** with a zero delay, `render` after an upload is
    `Normal`, the next one `Refined`, the one after that `Kept` with bytes
    equal to the refined ones; a camera move makes the next one `Normal`;
  - **every setter invalidates:** each mutating `pub fn` of `Renderer`
    (mesh upload, camera, ortho mode, surface transparency mode, viewport
    request, render quality) strictly increases `live_generation()`. The
    source-text check catches a setter that is missing from this list;
  - **anti-aliasing off:** with a zero delay, no `render` is ever `Refined`;
  - **offscreen unchanged:** export and thumbnail byte-identical.

  Mutations: draw the refined frame with the normal pipelines (junction and
  contact tests fail); in the transparent per-sample entry point, multiply
  the ghost's alpha by 4 (refined-matches-normal fails).
- Tests (Dart, `test/refinement_timer_test.dart`): a frame with `refine_in`
  = 0.15 s fires the callback once after 0.15 s; a second frame at 0.1 s
  re-arms it so it fires at 0.25 s, not 0.15 s; `None` cancels a pending
  timer; dispose cancels it. Mutation: arm without cancelling the previous
  timer (the re-arm test sees two callbacks).
- Expected: when the view comes to rest, junctions, contacts and ghost
  junctions turn smooth after `REFINE_DELAY`; moving the camera again shows
  the D4 frame at once.
- Effort: 2–2.5 d.

### Phase 4 — HiDPI

- `set_viewport_size(logical_w, logical_h, dpr)` in the api, regenerate the
  bindings, and the Flutter call with `MediaQuery.devicePixelRatioOf`.
- `set_viewport_request`, `viewport_request`, `get_viewport_logical_size`,
  and the consumers in D7's table (gadget pick context, export default size,
  CLI screenshot). For the gadget pick context, first split its arithmetic
  out of `update_gadget_pick_context` into a pure
  `gadget_pick_context(camera, logical_viewport_height)` in `api_common.rs`,
  so it can be tested without a `CADInstance` (Testing, layer 3).
- The *Full display resolution* preference (D8): the second field of
  `RenderingPreferences` and of `RenderQuality`, the ratio in
  `live_target_spec`, the dialog checkbox and its guide entry.
- Measure `provide_texture` (`APIFrameInfo::frame_time`) at
  100 % and at the display's own scale, on a small scene and on the
  nanobeam, with MSAA on, for a normal and for a refined frame. Record the
  numbers in this document. If the frame time regresses noticeably, do
  reduction 1 of D7 in this phase and report back before going further.
- Tests (CPU):
  - `TargetSpec::viewport`: rounding at 1.25, 1.5 and 2; never below 1×1;
    `pixel_scale` equals the ratio;
  - `live_target_spec` at ratio 2: all four combinations of the two
    preferences;
  - `RenderingPreferences`: `full_display_resolution` defaults to `true`,
    and a preferences JSON with only the Phase 1 field loads with it `true`.
- Tests (API, `rust/tests/renderer_api/`): `gadget_pick_context` at a
  logical height of 500 gives the world-per-pixel of a 500 px viewport,
  whatever the device height.
- Tests (GPU):
  - **HiDPI:** a live target at ratio 2 has twice the logical size, and a
    2 px line sums to ≈ 4 render pixels of coverage per column;
  - **resolution off:** at ratio 2 the image has the logical size, and
    toggling the setting off and on again restores the full-size image;
  - **logical size:** at ratio 2, `get_viewport_logical_size` is the
    requested logical size and `get_viewport_size` twice it (the export
    dialog default and the pick context read the first);
  - **sized capture restores:** at ratio 2, the CLI's sequence (save
    `viewport_request()`, `set_viewport_request(w, h, 1.0)`, `render_still`,
    restore) renders `w × h` and leaves the live target at the device size
    again, still multisampled.

  Mutation: pass the logical size as the device size (HiDPI test fails).
- Effort: 1–1.5 d, plus 0.5 d if reduction 1 is needed (it touches the
  plugin on three platforms).

### Phase 5 — Walkthrough and documentation

- Manual GUI walkthrough (maintainer), on the 100 % display and, if
  available, a 150 % or 200 % one: a dense crystal near and far in both
  projections (silhouettes, far-field shimmer while orbiting), a CSG solid's
  edges, an isosurface, ghosts from `xray`, the gizmo and the guide dots,
  moving the window between monitors of different scale, the
  `FilterQuality` choice of D7, both preferences toggled on and off while a
  scene is shown, a thumbnail, and an export compared with the screen.
- **The refined frame:** bond–atom junctions close up (single and double
  bonds) and space-filling C–H contacts, while orbiting and at rest; whether
  the switch reads as a pop; whether `REFINE_DELAY` feels right (neither
  refining mid-gesture nor noticeably late); a click right after the camera
  stops on the nanobeam. Record the chosen delay in this document.
- Reference guide: the **Rendering** preferences were documented in Phases 1,
  3 and 4; check them against the final behaviour. `ui.md` says the exported
  image is "always anti-aliased"; that stays true, and it should add that
  the export does not depend on the two preferences.
- Effort: 0.5 d.

## Future work

All of these build on this design without changing it:

- **Exact impostor intersections while moving.** D5 makes them exact only at
  rest. If the walkthrough finds the moving junctions distracting, the
  options are a bond shader that knows its end atoms (rejected in D4 for
  coupling the shaders and covering only one of the cases) or post-process
  AA (below).
- **Instanced bond quads.** One instance per bond instead of four vertices
  would cut bond vertex memory about 4×; independent of anti-aliasing, but
  the obvious next memory saving on the nanobeam.
- **A GPU path to Flutter.** The CPU readback is what makes device-pixel
  rendering expensive (D7). Sharing a GPU texture with Flutter (a D3D11
  shared handle on Windows, an `IOSurface` on macOS) removes it, but is
  platform code in the plugin and a wgpu interop question, and is its own
  design.
- **MSAA for the thumbnail and export.** A sample count in
  `TargetSpec::image` (the 4-sample pipelines then serve those targets too)
  once memory allows, e.g. 4 samples with 1× supersampling for small
  exports.
- **Post-process AA** (FXAA/SMAA): orthogonal, and only worth it if the
  intersections while moving turn out to matter.
