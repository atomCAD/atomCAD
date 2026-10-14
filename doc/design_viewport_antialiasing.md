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
mechanism for meshes and impostors), extends it to the circle where a bond
enters its atom, and renders the live viewport at the **device pixel ratio**.
Two preferences can turn MSAA and the device-pixel rendering off.

### Scope

- **In:** multisampled colour and depth for the live viewport, pipelines per
  sample count, the resolve (D1–D3); per-sample silhouette coverage in the
  three impostor shaders: opaque atoms, opaque bonds, x-ray ghosts, and
  through them the gadget guide dots (D4); per-sample bond–atom junctions
  (D5); device-pixel allocation of the live target and every consumer of the
  viewport's size in pixels (D7); two preferences, *Anti-aliasing* and *Full
  display resolution*, both on by default (D8).
- **Out (see Future work):** anti-aliasing of sphere–sphere contacts in the
  live view (still-camera refinement is the recorded fallback), post-process
  AA (FXAA/SMAA, needs the post-process chain of report §2.6), a GPU path
  from the renderer to Flutter that skips the CPU readback, MSAA for the
  thumbnail and the export.

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
- **Tests.** `tests/renderer/shader_test.rs` validates every `.wgsl` with naga
  and checks the six `CameraUniform` layouts; `line_geometry.rs` holds Rust
  twins of the line shader's arithmetic; the opt-in GPU harness
  `tests/renderer_gpu.rs` renders through the live, thumbnail and export paths
  and can dump raw images (`ATOMCAD_GPU_DUMP_DIR`) for before/after `cmp`.

## Decisions

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
not*), so it is the better image anyway. The thumbnail is small enough for MSAA, but one rule
("live is multisampled, offscreen is supersampled") is easier to keep than a
per-target exception, and the thumbnail gains nothing visible.

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
out of `Renderer` into one struct, built by one function:

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
- **Per-sample shading** (`@interpolate(perspective, sample)`). Exact, and
  it would also fix the intersections below, but it runs the whole shader
  four times on every impostor pixel, interior included. (Still-camera
  refinement in Future work uses it only on frames where the camera is at
  rest.)
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
  - **in ball-and-stick, the circle where every bond enters its atom.** This
    is on every atom, not a rare case. **D5 fixes it** for opaque bonds;
  - in space-filling, the contact curves between overlapping spheres,
    clearest between elements of very different colour (C against H, report
    shot `c11`). A sphere does not know its neighbours, so D5's approach does
    not carry over; this one stays, see below;
  - the junctions of x-ray ghost bonds (D5 leaves them as they are).

Exports and thumbnails have none of these limits, since supersampling shades
every sample. For the space-filling contacts in the live view, the fallback
is **still-camera refinement** (Future work): the walkthrough (Phase 4)
decides whether they matter enough to build it.

**The rim highlight** depends on `NdotV` at the shading point and is
unchanged.

**Transparent impostors** get the same treatment in both branches of
`transparent_impostor.wgsl`. A ghost's colour is blended into each covered
sample and the resolve averages, which is correct coverage for a blended
surface as well.

**Gadget impostors** (the guide dots and their connectors) are drawn with the
same atom and bond pipelines and are anti-aliased with no further work.

### D5 — A bond gives way to its end atoms per sample

The junction between a bond and its atom is jagged because two separate
draws settle it by depth, one value per pixel each. The bond shader can
settle it itself instead: it knows which two spheres it runs into, so on each
sample ray it can tell whether one of them is in front.

**The claim rule.** In step 3 of D4, the bond sets bit *i* only if its sample
ray hits the cylinder at `t_bond` **and** neither end sphere is hit by the
same ray at `t_sphere < t_bond`. That one test covers both sides of the atom:
on the near side, the part of the cylinder inside the sphere is behind the
sphere's surface; on the far side, the sphere is hit on the way to the bond.
The junction circle becomes an edge of the bond's sample mask, so it resolves
like an outline.

**The depth must agree with the mask.** The atom still covers the samples the
bond claims, and the atom's single depth at a junction pixel can be nearer
than the bond's single depth, in which case it would win the bond's samples
back in the depth test and the staircase would return from the other side.
So when the bond claims any sample on a pixel that one of its end spheres
also covers, its `frag_depth` is lowered to just in front of **the depth that
sphere writes on that pixel**:

```
frag_depth = min(own_depth, end_atom_depth(pixel) − ε)
```

The bond can compute `end_atom_depth(pixel)` exactly, because the atom
shader's choice of shading ray is deterministic (D4 step 5: the centre ray if
it hits, else the first sample ray that hits) and the bond shader runs the
same test with a copy of the same sphere code (D6). ε is a few units in the last place of
the depth value, enough to absorb a difference in floating-point contraction
between the two shaders. The bond now wins exactly the samples it claims
against its own atom, in either draw order. Against any third object the
lowered depth is off by at most the depth range of one pixel at the
junction, which is the same tolerance every impostor already has.

**What the bond needs to know.** Each end atom's centre and displayed radius.
The centres are not the cylinder's end points for the side cylinders of a
double or triple bond, which are offset sideways (`compute_bond_perpendicular`
in `atomcad-display/src/atomic_tessellator.rs`). `BondImpostorVertex` gains
the offset (`[f32; 3]`, centres = end points − offset) and the two radii
(`[f32; 2]`): 20 bytes, so 48 → 68 bytes per vertex. That is about 40 % more
bond vertex memory, which on the million-atom nanobeam is in the low hundreds
of MB; instancing the bond quads (Future work) would more than pay it back.

**Where the radii come from.** `tessellate_bond_impostor_inline` writes each
end atom's **effective** displayed radius, the same one that atom's own
impostor gets (per-atom style rules included), and **0** when the end atom is
not drawn as an opaque impostor: a ghost, a hidden atom, or no atom at all.
Radius 0 means "no sphere", and the shader skips that test. Every other
`add_bond_quad` caller (guidelines, anchor arrows, delete markers, guide-dot
connectors) passes 0 and is unchanged.

**Ghost bonds are left alone.** A translucent atom shows the bond running
into it; clipping the bond there would change what x-ray shows, not just its
edges. The transparent shader's bond branch therefore does not apply the
claim rule, and ghost junctions keep a per-pixel boundary. They are blended
and low in contrast, so this is the cheap end of the trade.

**Orthographic.** Today's orthographic ray starts two *bond* radii behind the
fragment, which can be inside an end sphere. The bond's sample rays start far
enough back to be behind both end spheres as well (the largest end radius
plus the side-cylinder offset is enough), so `t_sphere` and `t_bond` are
measured from one origin outside everything.

**Cost.** Per bond sample, two ray–sphere tests on top of the cylinder test,
and on junction pixels one replay of the atom's shading-ray choice (up to
five sphere tests). No extra pass, no change to the atom shader.

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
/// D5: the bond claims a sample iff it hits and neither end sphere
/// (radius 0 = none) is hit nearer along the same ray.
pub fn bond_claims_sample(t_bond: Option<f32>, end_sphere_t: [Option<f32>; 2]) -> bool;
/// D5: the depth the bond writes on a pixel one of its end atoms also covers.
pub fn bond_junction_depth(own_depth: f32, end_atom_depth: Option<f32>) -> f32;
```

Each WGSL copy carries a comment naming its twin. The GPU harness is the
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
| `update_gadget_pick_context` (`MIN_HIT_RADIUS_PX`, logical) | logical px | use a new `Renderer::get_viewport_logical_size()` (size ÷ `pixel_scale`) |
| `get_viewport_image_size` (export dialog default) | logical px | logical size; see open question 1 |
| CLI `capture_screenshot_impl`, no size given | what is on screen | render the live target as it is, no resize |
| CLI `capture_screenshot_impl`, size given | the requested px | a live target of exactly `w × h` at ratio 1.0 (sample count per the preference), then restore it from the saved request |
| Line widths, dashes, grid LOD | render px via `pixel_scale` | none |
| Flutter picking, projection, marquee selection | logical px | none (they never read the renderer's size) |
| `Renderer::new` | — | unchanged, ratio 1 until the first `set_viewport_request` |

**Displaying it.** Flutter's `Texture` widget maps the texture onto the
widget's logical rectangle, so a texture of exactly the device-pixel size is
shown 1:1, provided the widget sits on whole device pixels. At a fractional
ratio (125 %, 150 %) a panel edge can fall between device pixels, and the
default `FilterQuality.low` then resamples by a fraction of a pixel, a slight
softening; `FilterQuality.none` avoids that but can drop or repeat a column
when the rounded size and the rectangle differ by one. Which one looks right
is checked on a real display in the walkthrough (Phase 4); the code change
is one argument.

**Cost.** The live frame's CPU-side cost scales with the device pixel count,
because every frame is read back and copied through the plugin (Current
state). A 1600 × 1000 logical viewport is 6.4 MB per frame at 100 %, 14.4 MB
at 150 % and 25.6 MB at 200 %. GPU cost scales the same way, and the MSAA
memory of D1 by the same factor. Phase 3 measures `provide_texture` before
and after on the maintainer's display and on the million-atom nanobeam; the
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
| *Anti-aliasing* | on | the live target has 1 sample | recreates the live target (D2: the 1-sample pipelines exist anyway) |
| *Full display resolution* | on | the live target's ratio is 1.0 | recreates the live target (D7) |

Neither touches the thumbnail or the export: those are always supersampled,
at their own size.

**Not preferences, on purpose:**

- **Separate MSAA for meshes and for impostors.** Every pipeline drawing into
  the viewport must have the target's sample count, so meshes cannot be
  multisampled while impostors are not. "Impostor AA off" could only mean the
  impostor shaders claiming all four samples, which saves four hit tests per
  pixel, nothing measurable next to the shading.
- **The bond–atom junction (D5).** It costs two sphere tests per sample and
  does nothing with anti-aliasing off. It needs no second vertex layout
  either way (end radii of 0 already mean "no sphere"), but a switch that
  trades nothing is not worth a line in the dialog.

**Where they live.** A new `RenderingPreferences { anti_aliasing: bool,
full_display_resolution: bool }` in
`atomcad-structure-designer/src/preferences.rs`, a `#[serde(default)]` field
of `StructureDesignerPreferences` beside the others, with a `Default` of both
`true`, so an existing `preferences.json` without the section loads with both
on. It follows the existing sections' pattern through `api/` (twin and `From`
impls) to a new **Rendering** section in the Preferences dialog
(`lib/structure_designer/preferences_window.dart`).

**How they reach the renderer.** Like `surface_transparency_mode`: pushed on
every refresh (`set_structure_designer_preferences` refreshes), because the
renderer crate cannot see the preferences types. The renderer keeps the last
viewport request and the quality settings, and derives the live target from
both:

```rust
pub struct RenderQuality { pub anti_aliasing: bool, pub full_display_resolution: bool }

impl Renderer {
    /// From `set_viewport_size`: what Flutter laid out.
    pub fn set_viewport_request(&mut self, logical_w: f64, logical_h: f64, dpr: f64);
    /// From every refresh: the two preferences.
    pub fn set_render_quality(&mut self, quality: RenderQuality);
}

/// Pure, so it is tested without a device.
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

Same four layers as `design_screen_space_lines.md`, cheapest first. The
concrete tests are listed in the phase that builds what they test.

1. **Pure functions, CPU** (`tests/renderer/`, and the owning crate's tests
   for preferences and tessellation). Everything with arithmetic is a pure
   function: the target specs, the coverage twin (D6), and the bond claim and
   junction-depth rules (D5).
2. **Shader structure, naga** (`shader_test.rs`): the shaders validate and
   carry the builtins and constants the design relies on.
3. **API tests** (`rust/tests/renderer_api/`): the consumers of the viewport
   size that live in `api`.
4. **GPU harness** (`renderer_gpu.rs`, `--ignored`): pixel assertions with
   tolerances and counts, never golden images; byte-identity checks via
   `ATOMCAD_GPU_DUMP_DIR` dumps taken **before** a change and compared with
   `cmp` after it. Every existing GPU test must keep passing in every phase,
   notably the line coverage sums once the live target is multisampled.

As in the lines work, each new GPU test is confirmed to **fail with its fix
removed** before it is trusted; each phase names the mutation to try.

## Phases

Each phase leaves the build green and the app usable. Checks per phase:
`cargo fmt`, `cargo clippy`, `cargo test -j 4` (the renderer harness and the
api harnesses explicitly), the GPU harness (`cargo test -p atomcad-renderer
--test renderer_gpu -- --ignored`), and `flutter analyze`. The Flutter smoke
test is a manual step for the maintainer. Because `flutter run` loads the
release library, a GUI check needs `cargo build --release` first.

### Phase 1 — MSAA for the live viewport (meshes anti-aliased; impostors not yet)

- `TargetSpec` and the new `RenderTarget::new`; `sample_count` and
  `msaa_view` on the target (D1). Thumbnail and export targets via
  `TargetSpec::image`; the live one via `TargetSpec::viewport(w, h, 1.0,
  LIVE_SAMPLE_COUNT)` (the ratio arrives in Phase 3).
- `PipelineSet`, built twice; `draw_to_target` picks by sample count; the
  adapter check and fallback (D2).
- The pass and resolve changes (D3).
- The *Anti-aliasing* preference end to end (D8): `RenderingPreferences`,
  the api twin, the dialog's **Rendering** section, `RenderQuality`,
  `live_target_spec`, `set_render_quality`, and the push on refresh, all
  with `anti_aliasing` only and no device pixel ratio; Phase 3 adds both.
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
- The bond–atom junction (D5): the offset and end radii on
  `BondImpostorVertex` and in `tessellate_bond_impostor_inline`; the claim
  rule and the junction depth in `bond_impostor.wgsl` only. Both apply only
  when `SAMPLE_COUNT > 1`: with one sample the claim rule decides exactly
  what the depth test already decides, so the offscreen path keeps today's
  code and stays byte-identical.
- Tests (CPU):
  - `impostor_coverage`: the 4× offsets are WebGPU's and are centred (sum
    zero); the single-sample offset is the centre; a sphere hit by every
    sample gives `0b1111` and missed by all gives 0; **two half-planes
    meeting at the pixel centre give complementary masks** (OR = `0b1111`,
    AND = 0), the property D4 rests on; `sample_point` with orthogonal
    derivatives lands on the expected points; the hit tests agree with the
    shading code's intersection on hits, including the cylinder's length
    clip;
  - D5: `bond_claims_sample` on rays that hit the cylinder in front of,
    inside, and behind an end sphere (claimed, not, not); radius 0 never
    blocks; `bond_junction_depth` is below the atom's depth when the atom is
    nearer and unchanged otherwise;
  - bond tessellation (`atomcad-display` tests): the end atoms' effective
    radii and a double bond's side-cylinder offset are written, and 0 for a
    ghost or missing end atom.
- Tests (naga):
  - the three impostor shaders validate, declare `override SAMPLE_COUNT`,
    and have a `sample_mask` builtin in their fragment output;
  - the derivatives are taken before the first `discard` (checked on the
    WGSL source: a `dpdx` after a `discard` is the regression to catch).
- Tests (GPU):
  - **sphere silhouette:** the summed coverage of a sphere of radius *r* px
    is ≈ π r² (within a few percent), for r = 20 and for r = 0.7, where the
    single-sample render is all-or-nothing;
  - **no seam:** two opaque spheres overlapping in screen space, one in
    front, drawn in both orders: along the front sphere's silhouette over the
    back one, no pixel is closer to the background colour than to either
    sphere's;
  - **bond:** a cylinder's long edges have intermediate pixels; its flat
    ends do not paint outside the clip;
  - **bond–atom junction:** a bond entering a differently coloured atom,
    seen obliquely, in both draw orders: along the junction circle there are
    pixels strictly between the bond's and the atom's colours, and none
    outside the range between them; also for a double bond's side cylinder;
  - **ghost:** a transparent atom's silhouette has intermediate alpha
    composites, and every pixel of the read-back image is opaque;
  - **gadget impostor:** a guide dot in the gadget pass is anti-aliased;
  - **offscreen unchanged:** the export and thumbnail renders of the fixed
    scene are byte-identical to before the phase.

  Mutations: force the mask to `0b1111` (silhouette, no-seam and ghost tests
  fail); force the end radii to 0 (junction test fails).
- Expected: atom and bond silhouettes and the bond–atom junctions smooth in
  the live view, including at a distance and in orthographic; exports
  byte-identical.
- Effort: 3–4 d (the junction is about 1 d of it).

### Phase 3 — HiDPI

- `set_viewport_size(logical_w, logical_h, dpr)` in the api, regenerate the
  bindings, and the Flutter call with `MediaQuery.devicePixelRatioOf`.
- `set_viewport_request`, `get_viewport_logical_size`, and the consumers in
  D7's table (gadget pick context, export default size, CLI screenshot).
- The *Full display resolution* preference (D8): the second field of
  `RenderingPreferences` and of `RenderQuality`, the ratio in
  `live_target_spec`, the dialog checkbox and its guide entry.
- Measure `provide_texture` (its return value is already the frame time) at
  100 % and at the display's own scale, on a small scene and on the
  nanobeam, with MSAA on. Record the numbers in this document. If the frame
  time regresses noticeably, do reduction 1 of D7 in this phase and report
  back before going further.
- Tests (CPU):
  - `TargetSpec::viewport`: rounding at 1.25, 1.5 and 2; never below 1×1;
    `pixel_scale` equals the ratio;
  - `live_target_spec` at ratio 2: all four combinations of the two
    preferences;
  - `RenderingPreferences`: `full_display_resolution` defaults to `true`,
    and a preferences JSON with only the Phase 1 field loads with it `true`.
- Tests (API, `rust/tests/renderer_api/`): `update_gadget_pick_context` uses
  the logical height at a ratio of 2; `get_viewport_image_size` returns the
  logical size.
- Tests (GPU):
  - **HiDPI:** a live target at ratio 2 has twice the logical size, and a
    2 px line sums to ≈ 4 render pixels of coverage per column;
  - **resolution off:** at ratio 2 the image has the logical size, and
    toggling the setting off and on again restores the full-size image.

  Mutation: pass the logical size as the device size (HiDPI test fails).
- Effort: 1–1.5 d, plus 0.5 d if reduction 1 is needed (it touches the
  plugin on three platforms).

### Phase 4 — Walkthrough and documentation

- Manual GUI walkthrough (maintainer), on the 100 % display and, if
  available, a 150 % or 200 % one: a dense crystal near and far in both
  projections (silhouettes, far-field shimmer while orbiting), a CSG solid's
  edges, an isosurface, ghosts from `xray`, the gizmo and the guide dots,
  bond–atom junctions close up (single and double bonds), moving the window
  between monitors of different scale, the `FilterQuality` choice of D7,
  both preferences toggled on and off while a scene is shown, a thumbnail,
  and an export compared with the screen.
- **Decide on still-camera refinement:** look at space-filling contacts
  between C and H at rest and while orbiting, and record in this document
  whether the refinement in Future work is worth building.
- Reference guide: the **Rendering** preferences were documented in Phases 1
  and 3; check them against the final behaviour. `ui.md` says the exported
  image is "always anti-aliased"; that stays true, and it should add that
  the export does not depend on the two preferences.
- Effort: 0.5 d.

## Future work

All of these build on this design without changing it:

- **Still-camera refinement**, the fallback for the space-filling contacts
  (D4, *What is anti-aliased and what is not*), built only if the
  walkthrough asks for it. A third variant of the atom pipelines (and of the
  transparent one) with its sample-ray inputs declared
  `@interpolate(perspective, sample)`, so the shader runs once per sample
  and writes a depth per sample: exact intersections, at 4× the impostor
  shading cost. The viewport already re-renders on every Flutter frame
  (`provide_texture` from a persistent frame callback), so the renderer can
  switch to that variant once the camera and the meshes have been unchanged
  for a few frames, and back on the next change. Jaggies then exist only
  while orbiting, where they are hardest to see, and the 4× cost is paid
  only on frames nobody is waiting for. Risks to judge on screen: a visible
  pop at the switch, and the cost on the nanobeam if one refined frame is
  slower than a user's next click.
- **Instanced bond quads.** One instance per bond instead of four vertices
  would cut bond vertex memory about 4× and more than repay D5's 20 bytes.
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
  space-filling contacts matter and still-camera refinement does not suit.
