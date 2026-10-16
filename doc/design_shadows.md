# Shadows: Cascaded Shadow Maps for the Studio Key Light

Source: `reports/rendering_capabilities/rendering_capabilities_report_2026-09-21.md`
§2.3 (the shadow-technique note) and the Future work of
`doc/design_depth_perception.md`. Builds on that design (the lighting module,
the `ShadingUniform`, the Studio rig, the baked AO) and on
`doc/design_viewport_antialiasing.md` (pipeline sets, per-sample shading, the
refined frame). It changes neither; where it touches their decisions it says
so.

**Naming.** "D1"–"D11" are this document's decisions. The depth-perception
design's are written "DP D3", "DP D7"; the anti-aliasing design's "AA D5".

## Motivation

The depth-perception work gave every surface a direction (the Studio rig),
a distance (the depth cue) and a burial (baked AO). What is still missing is
the one cue that relates *two separate objects* to each other: a shadow.

- **A tool over a workpiece.** In a mechanosynthesis step the tool tip hangs a
  few Å over the surface. Nothing in the picture says how far: the tip is
  drawn over the surface either way, and AO is per structure (DP D6), so the
  tool does not even darken what it approaches. A shadow on the surface,
  offset from the tip along the light, shows the gap directly. This is the
  case the report singles out.
- **Relief.** An adatom, a dimer row, a step edge or a pit reads as relief
  only through shading. AO darkens creases; it does not say which side of a
  step is the high one. A shadow cast across a terrace does.
- **Blueprints over atoms.** A CSG solid shown with the atoms it was cut from,
  or a solid next to a structure, has no visual relation to it.
- **Pictures that leave the application.** Every hero render in the
  literature has shadows; their absence is part of why ours look like a
  viewer rather than a renderer.

### Scope

- **In:**
  - one directional shadow, from the Studio key light only, darkening the
    key light's direct term only (D1)
  - shadow maps, rendered from the light (D2)
  - casters: opaque atom and bond impostors **and** the opaque triangle mesh
    (CSG solids, opaque isosurfaces, surface splats, triangle-mesh atoms);
    receivers: every lit content surface (D3)
  - impostor and mesh caster passes (D4)
  - a camera-relative, stabilised fit to the casters (D5)
  - adaptively many cascades, one in the common case (D6)
  - the receiver: cascade selection, bias derived from the exact normal,
    fixed-softness PCF (D7)
  - the uniforms, bindings and pipeline layouts (D8)
  - one shadow map, cached by what it was rendered for (D9)
  - the coupling with baked AO (D10)
  - preferences: on by default, can be turned off (D11)
- **Out (Future work):**
  - contact-hardening (PCSS) or any softness that grows with the gap
  - shadows from the fill light, or under the *Headlight* preset
  - ray-traced or screen-space shadows
  - transparent casters (ghosts, transparent isosurfaces casting coloured or
    partial shadows)
  - a world-fixed light
  - fitting to the previous frame's visible depth range (SDSM)

## Current state (facts, with references)

All in `rust/crates/atomcad-renderer/src/` unless noted.

- **The rig.** `light_surface` (`lighting.wgsl`) computes
  `key · (diffuse + specular) · (N·L)⁺ · key_factor + fill + hemisphere`.
  The key direction is `ShadingUniform.key_light_dir`; under *Studio* it is
  built by `light_travel_direction(camera, 35°, 30°)` (`shading_uniform.rs`),
  so it turns with the camera. Under *Headlight* it is `head_light_dir`, 20°
  above the view direction.
- **AO touches the key.** `ao_factors` returns
  `key_factor = mix(1, ao(N), ao_strength · AO_KEY)` with the shader constant
  `AO_KEY = 0.35` (twin `shading::AO_KEY`). DP D7 calls this "a soft stand-in
  for the shadows of Future work".
- **Lit shaders are composed.** `compose_lit_shader` inserts `lighting.wgsl`
  into `atom_impostor.wgsl`, `bond_impostor.wgsl`,
  `transparent_impostor.wgsl` and `mesh.wgsl` at the `// ---- lighting ----`
  marker (DP D1). A lit colour is always
  `apply_depth_cue(to_display(light_surface(…)), p)` with the rim mixed in
  last; `shader_test.rs` checks the order.
- **Impostors already draw orthographically.** In orthographic mode the atom
  vertex shader builds its billboard from the view matrix rows
  (`camera_right`, `camera_up`), the bond vertex shader uses
  `camera_backward`, and `view_ray` starts parallel rays one diameter behind
  the quad. The hit gives an exact surface point, an exact normal and an
  exact `frag_depth`.
- **Impostors write `frag_depth`**, so early depth testing is off for them:
  every fragment inside a quad's circle runs the whole fragment shader,
  whatever is in front of it.
- **Pipelines.** `PipelineSet` (`pipelines.rs`) holds every pipeline once per
  sample count (AA D2). All lit pipelines and the impostor pipelines are built
  on `PipelineLayouts::main = [camera+shading, model]`. The label pipeline has
  its own group 2 (the font atlas), the line pipeline its own group 2 (the
  `LineLodUniform`).
- **Group 0** is `camera_bind_group_layout`: `CameraUniform` at binding 0,
  `ShadingUniform` at binding 1. Four bind groups, {live, offscreen} ×
  {content, gadget} (`ViewBindGroups`, DP D2).
- **`CameraUniform::new(camera, target_size, pixel_scale)`** (`renderer.rs`)
  is pure and takes a `Camera`, which can be orthographic
  (`Camera::build_projection_matrix` uses `orthographic_rh`, depth 0..1).
- **Draw order in `draw_to_target`.** Main pass: triangle mesh, opaque atom
  impostors, opaque bond impostors, labels (binds group 2 = atlas), lines
  (group 2 = LOD), transparent impostors, transparent isosurfaces. Gadget
  pass (depth cleared): lightweight triangles, gadget atoms and bonds, gadget
  lines.
- **What `main_mesh` holds** (`atomcad-structure-designer/src/scene_tessellator.rs`):
  CSG solids (`tessellate_poly_mesh`, unless wireframe mode is on), the
  geometry shell overlay of atomic outputs, surface splats
  (`tessellate_surface_point_cloud[_2d]`), triangle-mesh atoms
  (`AtomicRenderingMethod::TriangleMesh`) and opaque isosurfaces. Wireframe
  geometry goes to `wireframe_mesh`, a line mesh.
- **Bounds.** `content_bounds` (`thumbnail.rs`) is recorded on every content
  upload, over the triangle, wireframe, opaque and transparent impostor and
  transparent isosurface content.
- **Readback is synchronous.** Every `draw_to_target` submits, maps the output
  buffer and waits (`device.poll(Maintain::Wait)`).
- **The refined frame.** Once the live view rests, `render` draws it once more
  with the per-sample-shading impostor pipelines (AA D5) and keeps the bytes.
  Every mutating `pub fn` on `Renderer` bumps the live generation.
- **Device limits** are the defaults plus `max_buffer_size = 1 GiB`; no
  optional feature is requested. 2D array depth textures, comparison samplers
  and `textureSampleCompareLevel` need none.
- **Preferences.** `RenderingPreferences`
  (`atomcad-structure-designer/src/preferences.rs`) holds `lighting`,
  `depth_cue[_strength]`, `ambient_occlusion[_strength]`;
  `to_shading_settings` (`rust/src/api/api_common.rs`) turns them into
  `ShadingSettings { preset, depth_cue_strength, ao_strength }`, pushed with
  `Renderer::set_shading` on every refresh.

## Decisions

### D1 — One shadow, from the Studio key light, on the key's direct term only

```text
radiance = key · (diffuse + specular) · (N·L_key)⁺ · key_factor · key_visibility
         + fill · …                                                  (unchanged)
         + hemisphere · …                                            (unchanged)

key_visibility = 1 − shadow_strength · (1 − v)       v ∈ [0, 1], 1 = fully lit (D7)
```

- **The key only.** The fill and the hemisphere ambient stand in for indirect
  light (DP D3), and indirect light has no single direction to be shadowed
  from. Shadowing them would also need a second and third map. A shadowed
  surface therefore keeps its fill, its sky and its AO: it reads as "out of
  the sun", not as black.
- **Studio only.** The *Headlight* key sits 20° above the eye. Its shadows
  fall almost exactly behind what casts them, so all the user would see is a
  thin dark sliver under each object, which reads as an artefact. Under
  *Headlight* `shadow_strength` is 0 and no shadow pass runs (D9). *Headlight*
  remains exactly today's look.
- **Camera-relative, as the key already is.** The shadow always falls down
  and to the right of its caster, away from the viewer, where it is visible.
  The price: the light moves relative to the content as the camera orbits, so
  cast shadows **sweep across the scene while orbiting** — the tool's shadow
  slides over the workpiece, a step edge's shadow swings round. Shading on a
  single surface stays put on screen (DP D3); cast shadows do not. DP D3's
  reasons for a camera-relative key still hold (a world-fixed light would
  face its dark side to the user during half of every orbit), and the
  shadows are correct in every frame, but the sweep is a visible behaviour,
  judged in the walkthrough (Phase 5) on its own, not as shimmer.
- **The calibration constraint holds by construction.** An unshadowed surface
  has `v = 1`, hence `key_visibility = 1` exactly, and DP D3's front-facing
  white surface is unchanged.
- **`shadow_strength` < 1 by default (D11).** A fully shadowed surface keeps
  `1 − strength` of the key. Combined with the fill and the sky this gives
  soft, low-contrast shadows; on a lattice, hard black shadows read as noise.

### D2 — Shadow maps rendered from the light, not ray-traced shadows

The scene is mostly spheres, so the obvious alternative is to trace a shadow
ray per pixel through a uniform grid of the atoms, with an analytic soft
sphere test. It is exact at every zoom, needs no fitting and no bias. It is
rejected:

- **Impostor overdraw.** Impostors write `frag_depth`, so early-z is off and
  every fragment inside every quad's circle is shaded, including those that
  later lose the depth test. In a space-filling crystal that is several
  fragments per pixel, and each would trace a ray. A shadow-map lookup costs
  a fixed few texture taps; a grid traversal costs tens of cells, more on a
  terrace where the ray escapes through open space.
- **No deferred path.** Tracing once per pixel instead would need a G-buffer
  and the post-process chain (report §2.6), which do not exist.
- **Meshes.** CSG solids would need a BVH to cast.
- **Rebuild cost.** The grid would be rebuilt on every content upload, which
  happens on every non-lightweight refresh.

Also rejected:

- **Screen-space shadows** (marching the depth buffer towards the light):
  only on-screen casters cast, they need a resolved depth target and the
  post-process chain, and the live depth target is multisampled.
- **Variance or exponential shadow maps:** they exist to allow wide blur
  kernels and pay for it with light bleeding. Fixed-softness PCF (D7) needs
  neither.
- **Baking shadows on the CPU** like AO: the light moves with the camera, so
  there is nothing stable to bake.
- **A world-fixed light** (the report's option for caching the map across
  camera moves): DP D3 rejected it, because the dark side would face the user
  during half of every orbit.

Shadow maps suit impostors unusually well. The caster pass writes the
analytic depth of each sphere and cylinder, not a tessellation of it, and
the receiver knows its exact geometric normal. So the bias can be *derived*
rather than tuned (D7), and the classic acne and "shadow terminator" problems
of tessellated spheres do not arise for atoms.

### D3 — Casters and receivers

**Casters** (drawn into the shadow map, D4):

| Content | Casts | Why |
|---|---|---|
| Opaque atom impostors | yes | |
| Opaque bond impostors | yes | A stick's shadow on the atom below is part of the ball-and-stick look. |
| `main_mesh`: CSG solids, geometry shells, surface splats, triangle-mesh atoms, opaque isosurfaces | yes | One depth-only draw per cascade, no fragment shader. There is no performance or complexity reason to leave them out. |
| Transparent impostors (ghosts) | no | Ghosts are see-through by intent. A ghosted lattice around a highlighted dopant must leave the dopant lit, as for AO (DP D6). |
| Transparent isosurfaces | no | Same. A partial or coloured shadow would need a second map. |
| Lines, labels | no | Annotations. |
| The gadget pass (gizmo, pivot cube, guide dots) | no | Editing aids. |

**Receivers**: everything lit in the main pass — the triangle mesh, the
opaque and transparent impostors, the transparent isosurfaces. A ghost inside
a shadow darkens like any surface; it simply does not cast. Nothing in the
gadget pass receives: its `ShadingUniform` has `shadow_strength = 0`
(DP D2's per-pass split, extended in D8).

**Culled atoms do not cast, and need not.** Interior atoms beyond the cull
depth are not in the impostor mesh. The kept shell (8 Å in ball-and-stick,
3 Å in space-filling) is closed around the visible surface, so whatever a
shadow ray towards the light would hit inside the crystal is behind shell
atoms that already cast. This is unlike AO, whose occluders must include
culled atoms (DP D6), because AO looks in every direction from a surface atom,
inwards too.

**Shadows cross structures for free.** All displayed structures are merged
into one atom and one bond impostor mesh before upload, so the tool's shadow
falls on the workpiece with no extra work. This is the gap DP D6 left open
for AO.

**What the shadow map is fitted to: the casters' bounds.** A new
`caster_bounds` is recorded at upload next to `content_bounds`, over
`main_mesh`, the opaque atom impostors and the opaque bond impostors only. A
receiver outside it can be shadowed only by a caster inside it, so fitting to
the casters loses nothing (D5).

The casters are a subset of the content, so the two bounds share their
per-mesh code. `thumbnail.rs` gains `pub fn caster_bounds(main_mesh,
atom_impostor_mesh, bond_impostor_mesh)`, built on the existing
`BoundsAccumulator`, and `content_bounds` starts from the same accumulation
and adds the wireframe and the transparent meshes. No mesh's loop is written
twice, and a mesh added to the casters cannot be missing from the content.

### D4 — Caster passes: the light as an orthographic camera, through the existing shaders

**The light camera is a `Camera`.** Each cascade (D6) is an orthographic
`Camera` looking along the key light. `CameraUniform::new(&light_camera,
(N, N), 1.0)` then gives a uniform the existing vertex shaders read without
change:

- The atom vertex shader's orthographic branch builds the billboard from the
  light camera's view-matrix rows, so each quad faces the light.
- The bond vertex shader's orthographic branch uses `camera_backward`, the
  direction towards the light.
- `view_ray` starts parallel rays one diameter behind the quad.

**A new fragment entry point, `fs_shadow`,** in `atom_impostor.wgsl` and in
`bond_impostor.wgsl`:

- It discards outside the quad circle (atoms) or capsule (bonds) as `fs_main`
  does, runs `sphere_hit` / `cylinder_hit` on the light ray, discards on a
  miss and writes the hit's `frag_depth`. Nothing else: no colour output, no
  lighting, no rim, no per-sample coverage block (the map has one sample).
- It sits **above** the AA D5 per-sample marker, so `without_per_sample_shading`
  never cuts it.
- It must not reach `ShadingUniform` or group 2 (D8). `shader_test.rs` checks
  this from naga's per-entry-point global usage.

**The mesh casts through a vertex-only depth pipeline:** `mesh.wgsl`'s
`vs_main` with no fragment stage (`fragment: None`). Cull mode `None`,
because:

- CSG meshes are not guaranteed closed;
- a surface splat or a sketch fill is a single sheet, which must cast from
  either side.

**No hardware depth bias in any caster pipeline.** An impostor's quad faces
the light, so its rasterised slope is zero, and a slope-scaled hardware bias
would do nothing to it. One bias rule, applied at the receiver from the
receiver's exact normal, serves impostors and meshes alike (D7).

**`ShadowCasterPipelines`, not `PipelineSet`.** The caster pipelines draw
only into the shadow map, which has one sample and no colour attachment,
whatever target the frame is for. They live in `pipelines.rs` beside
`PipelineSet`, and are built once from the **same** `Shaders` modules (no
second compilation of the composed lit shaders), on
`PipelineLayouts::main` (`[camera+shading, model]`), with
`sample_count_constants(1)` for the impostors' `override SAMPLE_COUNT`:

- atom impostor (`fs_shadow`)
- bond impostor (`fs_shadow`)
- triangle mesh (vertex only)

AA D2's rule ("a new pipeline goes into `PipelineSet`") exists because a
pipeline must match its target's sample count. These have exactly one target,
so the rule does not apply; `rust/AGENTS.md` gains that exception (Phase 4).

**Group 0 of a caster pass** pairs cascade `k`'s light camera buffer with the
content `ShadingUniform` buffer, which no caster entry point reads. Four
camera buffers and four bind groups, one per possible cascade (D6), created
once and owned by `ShadowMap` (D9).

**One render pass per cascade.** Each targets layer `k` of the shadow-map
array (D9), clears depth to 1 and uses `Less`. It draws, in order: the
triangle mesh, the opaque atom impostors, the opaque bond impostors. All
cascades are recorded into the frame's encoder **before** the main pass, in
the same submit.

### D5 — The fit: the casters' bounding sphere, seen from a camera-relative light, stabilised

**The light frame.** With `L` the key's travel direction (unit), the light's
up is the camera's up made perpendicular to `L`.

`L` must be *the* key direction the receiver shades with, not a second
derivation of it: a map built along a slightly different direction than
`ShadingUniform.key_light_dir` would put shadows where `N·L` says there is
light. So `shading_uniform.rs` factors the key out of `ShadingUniform::new`
into `pub fn key_light_direction(camera, preset) -> DVec3` (f64; the uniform
casts it), which both `ShadingUniform::new` and `shadow_setup` call. The
camera basis (`forward`, `right`, `up`) is computed three times in that file
today (`light_travel_direction`, `camera_up`, and now the fit); it becomes
one `pub(crate) fn camera_basis(camera)`.

The light's up is then
`u = normalize(up_cam − L (up_cam · L))`. `L` is never parallel to
`up_cam`: the key sits 35° above the view direction, so `up_cam · L` is
about −0.57. Light-space coordinates `(x_L, y_L, z_L)` measure along
`u × L`, `u` and `L` from the **world origin**.

Because the key is camera-relative, light space is a fixed rotation of view
space. That property drives the whole fit.

**Depth range: the whole caster sphere.** With `(c, R)` the centre and half
diagonal of `caster_bounds`, every caster lies within `z_L ∈ [c·L − R,
c·L + R]`. The light camera covers that range plus a margin of one texel at
each end (`z_near_l = c·L − R − t`, `depth_range = 2R + 2t`). So every
caster is in the map, on screen or not:

- A tool tip above the top of the screen still shadows the visible surface.
- A receiver closer to the light than every caster (depth < 0 after
  projection) is lit: its reference depth is below every stored depth, so
  every comparison passes.
- A receiver behind every caster (depth > 1) is compared against what is
  actually in the map at that texel, which is correct.

**Footprint: the visible slice of the view, cut to the casters.** For a
cascade covering view depths `[a, b]` (D6), fitted over `[a', b]`:

0. `a' = a` for cascade 0. For every later cascade `k`, `a'` reaches back
   over the previous cascade's blend band (D7):
   `a' = a − SHADOW_BLEND_FRACTION · (b_{k−1} − a_{k−1})`. A receiver in that
   band is shaded with *both* cascades, so it must be inside both squares.
   Fitting only `[a, b]` would leave it outside cascade `k`'s square, where
   it reads as lit, and every split would show a bright fringe.
1. Take the eight corners of the view frustum between depths `a'` and `b`:
   half extents `d·tan(fovy/2)·(aspect, 1)` at each depth in perspective,
   `ortho_half_height·(aspect, 1)` in orthographic.
2. Project them to light space and take their bounding rectangle `F`.
3. Intersect `F` with the casters' square `Q = c_L.xy ± R`, the light-space
   shadow of the bounding sphere. An empty intersection drops the cascade.
4. Make it square, of side `s`, and add a margin of
   `filter radius + normal offset + one texel` on each side (D7), so that
   no filter tap of a receiver in the slice leaves the cascade.

**Stabilisation.**

- **Size in steps.** `s` is rounded up to a quarter-octave step,
  `2^(⌈4·log₂ s⌉ / 4)`, so a zoom changes the texel size in steps of about
  19 % rather than continuously.
- **Origin on the texel grid.** The square's centre is snapped to a multiple
  of the texel size `t = s / N` in light space. Since light-space coordinates
  are measured from the world origin, a camera **pan** (translation, no
  rotation) leaves the light frame unchanged and moves the square by whole
  texels: casters fall on the same texels and shadow edges do not crawl.
- **Orbit.** The casters' square `Q` is a sphere's shadow, so its size does
  not change under rotation. When the whole caster sphere is in view (the
  common case: one cascade, `F ⊇ Q`), the footprint is `Q` and orbiting
  about the casters' centre keeps `s` fixed. When it is not, `F` changes
  with the splits, which follow the casters' AABB along the view axis (D6),
  so `s` can change by a quantisation step during an orbit. That is
  acceptable: the steps are discrete, not a continuous breathing.

  What orbiting does change is the light's direction relative to the
  content. The shadows therefore sweep across the scene (D1), and on top of
  that their edges shimmer slightly, because the content turns against the
  texel grid. Both are inherent to a camera-relative light; PCF (D7) keeps the
  shimmer low, and the refined frame at rest (AA D5) is stable.

**The light camera of cascade `k`:**

- orthographic, aspect 1, `ortho_half_height = s/2`
- `up = u`
- its eye at light-space `(centre_l, z_near_l)`, the start of the depth
  range; its target one unit further along `L`
- `znear = 0`, `zfar = depth_range`

This is `ShadowSetup::light_camera(k)`.

**The fit is one pure function** in a new module `shadow_fit.rs`:

```rust
/// The shadow cascades for drawing `camera`'s view of casters with
/// `caster_bounds` into a target of `target`, under `settings`; `None` when
/// no shadow is drawn (the strength is 0, the preset is Headlight, there are
/// no casters, or every caster is behind the camera). Pure, so it is tested
/// without a device.
pub fn shadow_setup(
    camera: &Camera,
    caster_bounds: Option<(DVec3, DVec3)>,
    settings: &ShadingSettings,
    target: &TargetSpec,
) -> Option<ShadowSetup>;

#[derive(Clone, Debug, PartialEq)]
pub struct ShadowSetup {
    pub light_dir: DVec3,                // L
    pub light_up: DVec3,                 // u
    pub cascades: Vec<ShadowCascadeFit>, // 1..=MAX_CASCADES
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCascadeFit {
    pub centre_l: DVec2,      // the square's centre in (x_L, y_L), snapped
    pub side: f64,            // s, quantised
    pub z_near_l: f64,        // where the depth range starts along L
    pub depth_range: f64,     // its length
    pub split_near: f64,      // view depth where the cascade starts
    pub split_far: f64,       // and ends
    pub texel_world: f64,     // t = s / N, in Å
}

impl ShadowSetup {
    /// Cascade `k`'s light camera, built from the fit above.
    pub fn light_camera(&self, k: usize) -> Camera;
}
```

The setup holds the fit's numbers, not `Camera`s. `Camera` derives neither
`PartialEq` nor `Copy` and carries fields that mean nothing to a light
(`pivot_point`, `nav_up`, the `nav_up_label` string), so a setup made of
cameras would need a hand-written comparison and would clone a `String` per
cascade per frame. With plain numbers, `PartialEq` is derived, which is what
the cache (D9) compares; the `Camera` is built only where `CameraUniform::new`
needs one.

**Rejected:**

- **The content's AABB corners instead of its bounding sphere.** The corners
  are tighter, but their light-space rectangle changes size as the camera
  orbits, so the texel would breathe on every rotation.
- **Fitting to the previous frame's visible depth range (SDSM).** The
  synchronous readback would make it cheap, but it lags one frame behind
  during motion, and D6's cascades already concentrate resolution where the
  visible pixels are. It stays a lever (Future work).

### D6 — Adaptively many cascades: as many as the view needs, usually one

A single map fitted to everything visible is fine while the whole caster
sphere is in view: the map spans the object, and so does the screen. It fails
on two kinds of view:

- **A perspective close-up on a large structure.** On the nanobeam the
  visible slice runs from a few Å in front of the camera to hundreds of Å
  behind it. A map spanning the far end has texels of ~0.5 Å, smeared over
  50 pixels on a near atom.
- **An orthographic view with a long depth.** The light is tilted 35° from
  the view axis, so a slice `D` deep spreads `D·sin 35°` across the map. A
  rod seen end-on makes the map hundreds of Å tall for a 50 Å view.

In both, an opaque structure puts nearly every visible pixel near its *front*.
So the cascades are slices of view depth starting at the casters' front, each
as deep as it can be while its texel stays close to a screen pixel at its
near edge.

**The criterion.** The world size of an output pixel at view depth `d`:

```text
p(d) = 2·d·tan(fovy/2) / H_out           perspective
p(d) = 2·ortho_half_height / H_out       orthographic
H_out = target height / pixel_scale      (output pixels, so HiDPI and the
                                          export's 2× supersampling count once)
```

A cascade `[a, b]` is acceptable when its texel

```text
t(a, b) ≤ max(κ · p(a), t_floor)
κ       = SHADOW_TEXELS_PER_PIXEL = 2
t_floor = SHADOW_SOFTNESS_A / 1.5          (0.1 Å)
```

- **`κ · p(a)`:** a shadow texel may cover at most two output pixels where
  the cascade starts.
- **`t_floor`:** below it, a finer texel buys nothing. The PCF radius is
  `max(SHADOW_SOFTNESS_A, 1.5 · t)` in world units (D7), so every texel of
  0.1 Å or less gives the same 0.15 Å penumbra. Without the floor, a
  close-up (where `p(a)` may be 0.01 Å) would spend cascades resolving detail
  the filter then blurs away.

With the floor, one cascade covers about `t_floor · N ≈ 200 Å` of light-space
footprint at full quality, so cascades appear only on structures larger than
that.

**The splits.**

- `front` and `back` are the support of `caster_bounds` along the view axis,
  clamped to `[znear, zfar]`. If `back ≤ front`, there are no shadows.
  `depth_cue_range` already computes `front` this way; its support-point
  arithmetic moves into `pub(crate) fn view_depth_extent(camera, bounds) ->
  (f64, f64)` in `shading_uniform.rs`, which both call, rather than being
  written a second time in the fit.
- Cascade 0 starts at `a₀ = front`. Each cascade's `b` is the deepest value in
  `(a, back]` that meets the criterion, found by bisection. `t(a, b)` grows
  with `b`, so bisection finds it.
- The next cascade starts at that `b`.
- The cascade that reaches `back`, or the `MAX_CASCADES`-th (4), ends at
  `back` whatever its texel.
- If not even a thin slice meets the criterion, the cascade takes an even
  share of what is left: `b = a + (back − a) / (MAX_CASCADES − k)`. That
  happens only on an output wider than `κ·N = 4096` pixels whose near slice
  is also above the floor, because the frustum's own width at `a` then
  exceeds `κ·N` pixels.
- The criterion is evaluated on the slice the cascade is fitted over,
  `[a', b]`, blend extension included (D5).

**What this gives:**

| View | Cascades |
|---|---|
| A molecule, a tool, a block in full view, either projection | 1 (`2R / N` is far below `κ·p`) |
| A tool over a workpiece of up to ~200 Å, any view | 1 (the floor) |
| A slab face-on, orthographic | 1 |
| A slab of hundreds of Å at a grazing angle, perspective | 2–3 |
| A close-up on the nanobeam | 2–4 |
| A rod end-on, orthographic | 2–3 (the near cascade covers the end face) |
| An export (the same view, larger `H_out`) | the same or one more, automatically |

The count is part of `ShadowSetup` and is recomputed every frame. It changes
only when the view genuinely changes, so it does not flicker.

**Rejected:**

- **A fixed cascade count with practical (log/uniform) splits.** It pays for
  four passes on every compact view, the common case, for nothing.
- **The report's "focus map" around the camera target.** It is a two-cascade
  special case, and it is wrong on a grazing slab, where the target is only
  one point on a surface that spans every depth.

### D7 — The receiver: cascade by view depth, bias from the exact normal, fixed-softness PCF

**One function in `lighting.wgsl`:**

```wgsl
// 1 where the key light reaches `p`, 0 in full shadow. `geo_normal` is the
// surface's geometric normal (D7), not a shading normal.
fn shadow_visibility(p: vec3<f32>, geo_normal: vec3<f32>) -> f32
```

`light_surface` gains a last parameter, `key_visibility: f32`, which
multiplies the key term (D1). Each host computes it and passes it in. Hosts
call `light_surface` from code that runs after `discard`, so
`shadow_visibility` uses only `textureSampleCompareLevel`, which needs no
derivatives and is valid outside uniform control flow.

**Early outs**, in order:

1. `shading.shadow_strength <= 0` → 1 exactly. With this, every image drawn
   with shadows off is byte-identical to today's, as `apply_depth_cue` does
   at strength 0.
2. `N·L ≤ 0` (the surface faces away from the key) → 1. The key contributes
   nothing there anyway.

**Cascade selection by view depth.** `d = −(view · p).z`. The cascade is the
first `k` with `d ≤ split_far[k]`; beyond the last split, the last cascade.
Within the last `SHADOW_BLEND_FRACTION` (10 %) of a cascade's depth range,
the result is mixed with the next cascade's, so a split never shows as a
seam. Selection by the same depths the fit used (D6) guarantees the point is
inside its cascade's square, margin included. The next cascade's fit reaches
back over the band (D5 step 0), so a point in the band is inside both
squares. A point outside it anyway (a receiver beyond the casters'
footprint) is lit. That takes an explicit test on the projected `uv`: the
sampler clamps to the edge, so without it such a point would repeat the
border texels and pick up a caster that happens to touch the border.

**The geometric normal.**

- **Impostors:** the analytic sphere or cylinder normal they already compute.
  It is exact.
- **Meshes:** the face normal from screen derivatives,
  `normalize(cross(dpdx(p), dpdy(p)))`, flipped to face the viewer. The
  derivatives are taken at the top of `mesh.wgsl`'s `fs_main`, which never
  discards. The *shading* normal of a smooth mesh is interpolated and differs
  from the face it lies on. Biasing by the face itself is what removes the
  classic shadow-terminator bands on coarse tessellations.

**Bias, derived rather than tuned.** The caster stored in a texel is the
analytic surface along that texel's light ray. A receiver on the same surface
differs from it only by the receiver's lateral offset from the texel and the
filter taps' offsets. For a surface tilted by `θ` from the light, depth then
changes by `offset · tanθ`. So:

1. **Normal offset.** Look up at `p + geo_normal · NORMAL_OFFSET_TEXELS · t · sinθ`,
   with `NORMAL_OFFSET_TEXELS = 1`. The offset moves the lookup off the
   surface by about one texel and vanishes where the surface faces the light.
2. **Receiver-plane depth per tap.** Each PCF tap compares against the depth
   of the receiver's tangent plane at that tap's offset, computed from
   `geo_normal` in light space (`∂z/∂x_L = −n_x/n_z`, with `tanθ` clamped at
   4). A wide filter on a tilted surface then does not self-shadow.
   - On a **convex** receiver (every atom and bond) the surface curves away
     from the light behind its tangent plane, so the plane can only be
     nearer than the stored depth: self-shadowing is impossible.
   - On a **concave** mesh corner, the neighbouring face can be nearer: that
     is a real occluder.
3. **A residual** of `SLOPE_BIAS_TEXELS = 0.71` texel × `min(tanθ, 4)`,
   plus a constant `CONST_BIAS_TEXELS = 0.5` texel. This covers the hardware
   comparison's 2×2 footprint, which compares one reference against texel
   centres up to ~0.71 texel away.

All three are in texels of the cascade, so they scale with it. The constants
are starting values. Phase 1's acne detector bounds them from below and its
contact detector from above (Testing layer 4); the walkthrough looks for
light leaking at contacts in real structures. AO already darkens the
contact crease (D10), so a slight leak there is not visible.

**Fixed-softness PCF.**

- 16 taps on a fixed Poisson disk, each a hardware bilinear comparison
  (`textureSampleCompareLevel` with a `LessEqual` comparison sampler with
  linear filtering), averaged.
- The disk's radius is fixed in **world** units, not texels:
  `max(SHADOW_SOFTNESS_A, 1.5 · t)`, with `SHADOW_SOFTNESS_A = 0.15 Å` to
  start. The penumbra is then the same width in every cascade and at every
  zoom where the map can resolve it, and the blend between cascades is
  invisible. Where a cascade is too coarse, the clamp widens it rather than
  letting it alias.
- No per-pixel rotation of the disk. Without temporal accumulation that only
  trades banding for grain.
- The cost is 16 taps per shaded fragment, 32 in a blend band, and four times
  that in the refined frame (per-sample shading, AA D5), which is drawn once
  at rest.

**Rejected:**

- **An id buffer to suppress self-shadowing.** It would work: a convex
  primitive cannot shadow its own lit side, so "skip texels written by
  myself" removes acne completely. But it costs a second attachment and
  manual taps instead of hardware comparisons, and the receiver-plane bias
  already makes convex receivers safe. It is the fallback if the walkthrough
  finds acne the bias does not cure.
- **Contact hardening (PCSS).** Not needed, by decision. The gap between a
  tip and a surface still reads from the shadow's offset along the light.

### D8 — Uniforms, bindings and layouts

**`ShadingUniform` gains two fields** and grows from 80 to 96 bytes:

| Field | Offset | Meaning |
|---|---|---|
| `shadow_strength` | 80 | 0 = no shadow. 0 in the gadget pass, under *Headlight*, without casters, or with shadows off. |
| `ao_key` | 84 | AO's share of the key (D10). Replaces the shader constant `AO_KEY`. |
| *pad* | 88, 92 | |

It stays per pass and is written on every draw (DP D2), so the gadget pass
needs nothing new: its variant has `shadow_strength = 0`. `ShadingUniform::new`
gains the frame's `Option<&ShadowSetup>`: strength is 0 when it is `None`.

**Group 2 of the lit pipelines** is a new layout, `shadow_bind_group_layout`:

| Binding | Resource | Stage |
|---|---|---|
| 0 | `ShadowUniform` | fragment |
| 1 | the shadow map, `texture_depth_2d_array` | fragment |
| 2 | `sampler_comparison` (`LessEqual`, linear, clamp to edge) | fragment |

```wgsl
struct ShadowCascade {
  world_to_clip: mat4x4<f32>,  // the light camera's view_proj
  split_far: f32,              // view depth where the cascade ends (D6)
  texel_world: f32,            // t, in Å
  filter_radius_uv: f32,       // max(SHADOW_SOFTNESS_A, 1.5 t) / s
  depth_range: f32,            // zfar − znear of the light camera, in Å
};                             // 80 bytes
struct ShadowUniform {
  cascades: array<ShadowCascade, 4>,
  count: u32,
  blend_fraction: f32,
  debug_tint: u32,             // D9: colour each pixel by its cascade
  _pad: u32,
};                             // 336 bytes
```

It has a Rust twin in `shadow_fit.rs` with the same offset table and
`offset_of!` checks as `ShadingUniform`, in `shader_test.rs`. It is built by
a pure `ShadowUniform::new(setup: Option<&ShadowSetup>, debug_tint: bool)`
(`None`: `count = 0`, every slot zeroed) and written on every draw by
`ShadowMap::prepare` (D9). Each `world_to_clip` is
`CameraUniform::new(&setup.light_camera(k), (N, N), 1.0).view_proj`, the
matrix the caster pass draws with. A cascade's start, needed for the blend
band, is the previous cascade's `split_far` (cascade 0 never blends from
behind), so it is not stored.

**A new pipeline layout, `PipelineLayouts::lit = [camera+shading, model,
shadow]`**, for every lit pipeline:

- triangle
- the three impostor pipelines and their refined variants
- the three transparent-surface pipelines

The caster pipelines (D4) stay on `main`, which has no group 2. Their entry
points do not use group 2, so they never sample the shadow map in the pass
that writes it. wgpu validates a pipeline against the resources its entry
points statically use, so a module that declares group 2 is valid on a
layout without it.

**Group 2 must be re-bound before every lit draw.** In the main pass the
labels bind the font atlas at group 2 and the lines bind their LOD uniform
there, and the transparent impostors and isosurfaces are drawn after both.
A pipeline switch to a layout with a different group 2 invalidates the
binding. `draw_to_target` therefore binds the shadow group after every
`set_pipeline` of a lit pipeline, through one helper, so no later draw can
forget it. The gadget pass binds it too; its strength is 0, so it is never
sampled.

### D9 — One shadow map, cached by what it was rendered for

**One resource**, not one per view:

- A `Depth32Float` 2D array texture, `SHADOW_MAP_SIZE = 2048` square,
  `MAX_CASCADES = 4` layers, usage `RENDER_ATTACHMENT | TEXTURE_BINDING`:
  64 MB.
- It is created and dropped by `set_shading`, the one place that knows
  whether shadows are effectively on (strength > 0 under *Studio*): created
  when they turn on, dropped when they turn off. A shadow-free session holds
  none. Not lazily on the first draw: `draw_to_target` runs under `&self`, and
  creating the texture there would put the texture and the group-2 bind group
  behind a lock too, for no gain. `set_shading` is `&mut self` and already
  bumps the live generation on a change.
- While it does not exist, group 2 binds a 1×1×1 placeholder, which is never
  sampled because the strength is 0. The placeholder is cleared to depth 0,
  which shadows everything, so a path that samples it by mistake shows up as
  a dark image in the GPU tests rather than as a silently correct one.

**Cached by what it holds.** The map remembers the `ShadowSetup` it was
rendered for, or nothing:

- The non-lightweight branch of `update_all_gpu_meshes` (the only place
  `main_mesh` and the opaque impostor meshes are replaced) calls
  `invalidate()`, which forgets it. No generation counter: forgetting is
  enough.
- `draw_to_target` computes `shadow_setup(…)`. When the setup is `Some` and
  differs from the remembered one, it records the caster passes (D4) and
  remembers the new setup. When it is equal, it records nothing.
- This is correct by construction, like the `ShadingUniform`: the map is
  re-rendered before any draw that disagrees with what it holds, so no view
  can see another view's shadows. It is therefore exempt from the "per-view
  uniforms come in pairs" rule (`rust/AGENTS.md`), and that note says so
  (Phase 4).

**What the cache buys:**

| Draw | Shadow passes |
|---|---|
| A camera move | re-rendered (camera-relative light) |
| The refined frame after the view rests (AA D5) | none: same camera, same casters |
| The thumbnail's second background (light and dark) | none |
| A lightweight refresh (gizmo drag, selection) | none: casters unchanged |
| An export, then the next live frame | re-rendered for each |

**Observed, not inferred.** The renderer counts the cascade passes it
records, as a per-instance `shadow_passes_drawn()`. Tests assert on it, never
on timing, as with `last_exposure_stats` (DP Testing layer 1). The remembered
setup and the counter are the only state a draw changes; they sit together
in one `Mutex`, as for `LineLodBinding::was_visible`, because
`draw_to_target` runs under `&self`.

**One owner: `ShadowMap`** (`shadow_map.rs`). `renderer.rs` is already
~2 000 lines, and the shadow state is self-contained, so it does not spread
over `Renderer`'s fields. `ShadowMap` owns the array texture (or none), the
placeholder, the comparison sampler, `shadow_bind_group_layout` and the
group-2 bind group, the `ShadowUniform` buffer, the four light-camera buffers
and their group-0 bind groups, the remembered setup and the pass counter. Its
surface is small:

- `new(device, camera_bind_group_layout, content_shading_buffer)`, created
  in `Renderer::new` **before** `PipelineLayouts`, which takes its
  `shadow_bind_group_layout` for `PipelineLayouts::lit`
- `set_enabled(&mut self, device, on)`, from `set_shading`: creates or drops
  the array texture and rebuilds the group-2 bind group over it or over the
  placeholder; a new texture remembers no setup
- `set_cascade_debug(&mut self, on)`, from `set_shadow_cascade_debug`
- `invalidate(&self)`, from the upload
- `prepare(&self, queue, encoder, setup: Option<&ShadowSetup>, casters)`:
  writes the `ShadowUniform` and, when the setup is `Some` and differs from
  the remembered one, the light-camera buffers and the caster passes
- `bind_group(&self)`, for the re-bind helper (D8)
- `passes_drawn(&self)`, `allocated(&self)`, exposed to tests as
  `Renderer::shadow_passes_drawn()` and `Renderer::shadow_map_allocated()`

`casters` is a small borrowed struct of the three caster meshes and the
`ShadowCasterPipelines`; `Renderer` builds it in `draw_to_target`.

**A debug tint**, `Renderer::set_shadow_cascade_debug(bool)`, colours each
receiver by its cascade (red, green, blue, yellow at 50 %). It is the only
way to see where the splits fall. The GPU tests use it, and so does the
walkthrough. It is a `pub fn(&mut self)`, so it bumps the live generation.
It is not a preference.

**Rejected:** a live and an offscreen map. 64 MB each, for a saving that
occurs only when switching between a live frame and an export, which
re-renders either way because the cameras differ.

### D10 — Baked AO cedes the key to the shadows

DP D7 let AO dim the key by `AO_KEY = 0.35` as "a soft stand-in for
shadows". With real shadows on, that would darken the creases twice: once by
burial, once by the shadow cast into them. So:

```text
ao_key = mix(AO_KEY, AO_KEY_WITH_SHADOWS, shadow_strength)
AO_KEY = 0.35   (unchanged)     AO_KEY_WITH_SHADOWS = 0.1   (starting value)
```

`shadow_strength` here is the **uniform's** (D8), which is 0 whenever no
shadow is drawn (no setup: *Headlight*, no casters, shadows off), not the
preference. `ao_key` is computed by `ShadingUniform::new` and read by
`ao_factors` in place of the constant (D8), with the twin changed alike.
Wherever no shadow is drawn it is exactly `AO_KEY`, so every existing AO
test and image is unchanged.

The two remain complementary. Shadows give direction and relate separate
objects; AO gives the contact crease, which a 2048 map spread over a large
scene cannot resolve. `AO_KEY_WITH_SHADOWS` is not 0, because a little AO on
the key keeps the creases legible where the key washes over them.

### D11 — Preferences: *Shadows*, on by default

| Preference | Default | Range | Reaches |
|---|---|---|---|
| *Shadows* | on | — | renderer (`ShadingSettings.shadow_strength`) |
| *Shadow strength* | 0.7 | 0–1 | renderer |

- **Storage.** Two new `#[serde(default)]` fields of `RenderingPreferences`,
  `shadows: bool` and `shadow_strength: f64`, with their API twin. They go in
  the dialog's *Lighting* sub-heading, as a checkbox-plus-slider pair like
  *Depth cue*.
- **Path.** `to_shading_settings` sets `shadow_strength` to 0 when the box is
  unticked and clamps it to [0, 1]. `set_shading` already bumps the live
  generation when the settings change. The renderer's neutral default stays
  strength 0 (DP D2), so every existing GPU test runs unchanged.
- **Off means zero cost.** No shadow pass, no map in memory (D9), and the
  receiver's first early out returns before any texture tap. The renderer
  alone handles it: shadows touch neither evaluation nor tessellation, so
  toggling re-evaluates and re-tessellates nothing.
- **Under *Headlight*** both rows are disabled in the dialog, with a one-line
  hint that shadows come from the Studio key light. The stored values are
  kept.
- **Exports and thumbnails follow the viewport** (DP D8: appearance, not
  quality). Their quality needs no knob: the cascade criterion (D6) is per
  output pixel, so a larger export gets as many cascades as it needs.
- **The map size is not a preference.** It is fixed at 2048. If the
  walkthrough finds the cost too high on integrated graphics, the levers are
  in Phase 3, in the order given there.

## Where the code goes

Everything except the preference plumbing is in `atomcad-renderer`; no new
crate edge. Shadows touch neither `atomcad-display` nor the tessellators: the
casters are the meshes the renderer already holds.

| Where | What |
|---|---|
| `atomcad-renderer/src/shadow_fit.rs` (new) | Pure: `shadow_setup`, `ShadowSetup`, `ShadowCascadeFit`, the cascade criterion, `ShadowUniform` and `ShadowUniform::new`, the constants the CPU needs (`SHADOW_MAP_SIZE`, `MAX_CASCADES`, `SHADOW_SOFTNESS_A`, `SHADOW_TEXELS_PER_PIXEL`, `SHADOW_BLEND_FRACTION`) |
| `atomcad-renderer/src/shadow_map.rs` (new) | GPU: `ShadowMap` (D9), the caster passes it records |
| `atomcad-renderer/src/shading_uniform.rs` | `ShadingSettings.shadow_strength`, the two new uniform fields, `ao_key` (D10); the shared `key_light_direction`, `camera_basis`, `view_depth_extent` |
| `atomcad-renderer/src/thumbnail.rs` | `caster_bounds`, with `content_bounds` built on it |
| `atomcad-renderer/src/pipelines.rs` | `PipelineLayouts::lit`, `ShadowCasterPipelines` |
| `atomcad-renderer/src/renderer.rs` | Fields: one `ShadowMap`, one `ShadowCasterPipelines`, `caster_bounds`. `draw_to_target` calls `shadow_setup` and `ShadowMap::prepare` before the main pass and re-binds group 2 through the helper; `set_shading` calls `set_enabled`; the upload calls `invalidate`; `set_shadow_cascade_debug` |
| `lighting.wgsl` | The group-2 declarations, `shadow_visibility`, the `key_visibility` parameter |
| `atom_impostor.wgsl`, `bond_impostor.wgsl` | `fs_shadow` |
| `shading.rs` | The twin's `key_visibility` and `ao_key` |
| `atomcad-structure-designer/src/preferences.rs`, `rust/src/api/structure_designer/structure_designer_preferences.rs` | The two fields and their API twin (D11) |
| `rust/src/api/api_common.rs` | `to_shading_settings` |

**One home per constant.** The constants that the CPU uses (the softness,
the blend fraction, the map size) are in `shadow_fit.rs` only and reach the
shader through `ShadowUniform` (`filter_radius_uv`, `blend_fraction`,
`texel_world`), so WGSL never restates them. The bias constants
(`NORMAL_OFFSET_TEXELS`, `SLOPE_BIAS_TEXELS`, `CONST_BIAS_TEXELS`) and the
16-tap disk are used only by the shader and live only in `lighting.wgsl`.

## Open questions

1. **Sheets in `main_mesh`.** A 2D sketch's fill and the surface splats are
   in `main_mesh`, so they cast (D3), and a sketch on the drawing plane will
   cast a shadow onto atoms below it. Recommendation: let them cast; an opaque
   surface that does not cast would be the surprise. If the walkthrough finds
   it confusing, the fix is a separate non-casting mesh for sketch fills, not
   a shader flag.
2. **Constants.** `SHADOW_SOFTNESS_A` (0.15 Å), the default strength (0.7)
   and `AO_KEY_WITH_SHADOWS` (0.1) are starting values for the walkthrough.
   The bias constants are bounded by the acne and contact detectors first
   (Phase 1), then the walkthrough.
3. **The terminator band on smooth meshes.** D7's face-normal bias should
   remove it. If a coarse tessellated sphere (a triangle-mesh atom) still
   shows bands near its terminator, the fallback is to fade
   `1 − key_visibility` out over `N·L ∈ [0, 0.1]`.

## Testing

The five layers of the anti-aliasing and depth-perception designs. Concrete
tests are listed in the phase that builds what they test.

1. **Pure functions, CPU.**
   - `shadow_setup` (`shadow_fit_test.rs`, new, in `tests/renderer/`) covers
     the light frame, the depth range, the footprint, the stabilisation, the
     cascade criterion and splits, and the cases that return `None`.
   - `ShadowUniform::new`: each cascade's `world_to_clip` equals
     `CameraUniform::new(&setup.light_camera(k), …).view_proj`, so the
     receiver projects with exactly the matrix the caster pass drew with;
     the derived fields (`filter_radius_uv`, `texel_world`, `split_far`);
     unused cascade slots zeroed.
   - `ShadingUniform::new`'s new fields.
   - `caster_bounds`, and `content_bounds` unchanged by being rebuilt on it.
   - The preferences' serde defaults and `to_shading_settings`.
2. **Shader structure, naga** (`shader_test.rs`).
   - Every lit shader validates as composed, with the new group 2.
   - The `ShadingUniform` and `ShadowUniform` offsets.
   - `fs_shadow` and the vertex entry points use no group-2 global and do
     not read `shading` (from naga's per-entry-point global usage).
   - `fs_shadow` sits above the per-sample marker.
   - Every `light_surface` call in a host passes a `key_visibility` computed
     by `shadow_visibility` or the literal `1.0`.
3. **The Rust twin** (`shading.rs`): `light_surface` gains `key_visibility`,
   `key_visibility(strength, v)` is new, and `ao_factors` reads `ao_key`. The
   shadow lookup itself has no twin. It is a texture fetch, so the GPU tests
   check it directly against twin colours for `v = 1` and `v = 0`.
4. **GPU harness** (`renderer_gpu.rs`, `--ignored`), with the established
   rules: dumps first, capability skips, no timing, tolerances and counts,
   never committed golden images, `ATOMCAD_GPU_COMPARE_DIR` for the
   byte-identical claims. Each new test is confirmed to fail with its fix
   removed; each phase names the mutation.
   - **The anchor:** with shadows off (the renderer default) every image is
     byte-identical to the previous phase.
   - **The acne detector** (bias too small): a scene with nothing to
     shadow, drawn with shadows on at strength 1, equals the same scene with
     shadows off. Examples: a lone sphere, a lone bond, a lone CSG cube, a
     lone tessellated sphere mesh, all lit from the key. The tolerance is
     2/255 at every pixel for impostors; for meshes, at most 0.5 % of the
     object's pixels may differ by more than 2/255. Self-shadowing is the
     only thing that can make them differ. Each scene is drawn at the finest
     and at the coarsest texel the tests produce (about 0.01 Å and at least
     0.3 Å, the latter by zooming out): the bias is in texels, and the coarse
     end is where acne shows.
   - **The contact detector** (bias too large). The acne detector alone
     would pass a bias of any size; this bounds it from above. A sphere
     resting on a CSG slab, and a sphere touching a layer of spheres: the
     shadow reaches the contact. The slab pixel just beside the contact, on
     the shadow side, is darker than the slab far from the sphere by at least
     half the full-shadow difference (from the twin at `v = 0` and `v = 1`).
     Drawn at the same two texel sizes.
   - **The position check:** a shadow lands where the light puts it. The
     centroid of the darkened pixels of a sphere's shadow on a layer lies
     within two output pixels of the CPU projection of the sphere's centre
     along `L` onto the layer. A wrong matrix, a half-texel offset or a
     wrong depth range fails it; "darker below and to the right" alone does
     not.
5. **No new Dart logic.** The two dialog rows are thin editor UI and are
   checked in the walkthrough.

**Not automated, on purpose** (Phase 5's walkthrough):

- whether the shadows look right, and the constants
- light leaking at contacts in real structures, beyond the contact
  detector's two simple scenes
- shimmer while orbiting, and the sweep of shadows across the scene (D1)
- the frame rate on the maintainer's machine and on integrated graphics

## Phases

Each phase leaves the build green and the app usable. Checks per phase:

- `cargo fmt`, `cargo clippy`
- `cargo test -j 4`, naming each touched harness:
  - `-p atomcad-renderer --test renderer`
  - root crate: `--test renderer_api`, `--test structure_designer_api`
  - `-p atomcad-structure-designer --test structure_designer` (Phase 4,
    preferences)
- the GPU harness:
  `cargo test -p atomcad-renderer --test renderer_gpu -- --ignored`
- `flutter analyze`

The Flutter smoke test is a manual step for the maintainer. A GUI check
needs `cargo build --release` first.

Shadows stay invisible in the app until Phase 4: the renderer default is
strength 0, and nothing pushes another value until the preference exists. As
in DP Phase 4b, **no uncached shadow path ever reaches the app**.

### Phase 1 — Caster passes, the single-cascade fit and the receiver (no visible change)

- **Before any code:** dump the whole GPU harness from the current commit
  (`ATOMCAD_GPU_DUMP_DIR`).
- The shared helpers first, as a pure refactor under the anchor test:
  `key_light_direction`, `camera_basis` and `view_depth_extent` in
  `shading_uniform.rs` (D5, D6), and `content_bounds` rebuilt on
  `caster_bounds` in `thumbnail.rs` (D3), recorded at upload.
- `shadow_fit.rs` with `shadow_setup` restricted to one cascade over
  `[front, back]` (D5 complete, D6 not yet), `ShadowSetup`, `ShadowUniform`
  and its twin.
- `fs_shadow` in the atom and bond shaders, `ShadowCasterPipelines` in
  `pipelines.rs` (D4).
- `shadow_map.rs`: `ShadowMap` with the map (created and dropped by
  `set_shading`), the placeholder, the comparison sampler,
  `shadow_bind_group_layout`, the four light camera buffers and bind groups;
  `PipelineLayouts::lit` and the group-2 re-bind helper (D8, D9). In this
  phase `prepare` re-renders the map on every draw that needs it: no cache
  yet. The pass counter and `Renderer::shadow_passes_drawn()` /
  `shadow_map_allocated()` come now, because this phase's tests use them.
- `ShadingUniform` grows to 96 bytes (`shadow_strength`, `ao_key = AO_KEY`
  always, D10 not yet). `ShadingSettings` gains `shadow_strength` (default 0).
- `shadow_visibility` in `lighting.wgsl` (D7, without the cascade blend), the
  `key_visibility` parameter of `light_surface` in all four hosts, and the
  face normal from derivatives in `mesh.wgsl`.
- **Tests:**
  - `shadow_fit_test.rs`:
    - the light frame is camera-relative: orbiting the camera leaves `L` and
      `u` unchanged in camera space; `u ⊥ L`
    - `L` equals `ShadingUniform::new(…).key_light_dir` (cast to f32) for
      the same camera, at several orientations
    - the depth range contains every corner of `caster_bounds`
    - random points of the caster sphere inside the view slice project inside
      the square, margin included (1 000 points, fixed seed)
    - the side is a quarter-octave step ≥ the unquantised side
    - pan stability: translating the camera by a fraction of a texel moves the
      square's centre by a whole number of texels (0 or 1) in light space
    - orbit stability: with the whole caster sphere in view, orbiting about
      its centre leaves `s` unchanged at eight orientations (D5)
    - `None` for strength 0, for *Headlight*, without casters, and with every
      caster behind the camera
  - also in `shadow_fit_test.rs` (the test file mirrors the source file, and
    `ShadowUniform` lives in `shadow_fit.rs`; registered in
    `tests/renderer.rs` like its siblings): the layer-1 checks of
    `ShadowUniform::new`, including `world_to_clip` against
    `CameraUniform::new` of `light_camera(k)`.
  - `thumbnail_test.rs`: `caster_bounds` covers `main_mesh` and the opaque
    impostors (radius included) and ignores the wireframe and transparent
    meshes; the existing `content_bounds` tests pass unchanged.
  - `shading_uniform_test.rs`: the new offsets and size; the gadget variant
    has `shadow_strength = 0`; `None` for the setup gives strength 0; the
    refactored `depth_cue_range` returns what it did (the existing tests).
  - `shader_test.rs`: the layer-2 checks above.
  - Twin: `key_visibility(0, v) == 1` for every `v`; at strength 1 and
    `v = 0` only the key term goes.
  - GPU:
    - The anchor, against the dump (renderer default): every image is
      byte-identical. Mutation: drop the strength from `key_visibility` (use
      `v` directly); the comparison must fail, because the receiver then
      reads the placeholder, which shadows everything (D9). (Skipping early
      out 1 is *not* a usable mutation: at strength 0 the formula gives 1
      anyway.)
    - Pipeline creation of the caster pipelines on `main` succeeds (D8).
    - The acne detector (Testing layer 4) in perspective and orthographic,
      with MSAA off and on, and the refined frame. Mutation: remove the
      normal offset and the receiver-plane depth; acne pixels must appear.
    - The contact detector (Testing layer 4). Mutation: multiply
      `CONST_BIAS_TEXELS` by 10; the contact must lose its shadow.
    - **Direction and depth:** a sphere 4 Å above a square layer of spheres,
      looked at from above, under *Studio* with strength 1. The layer is
      darker below and to the right of the sphere's screen position, not
      above and to the left. A layer sphere's centre pixel in the shadow's
      core matches the twin at `v = 0` within 2/255, one outside it at
      `v = 1`. The position check (Testing layer 4) on the same scene, in
      both projections. Mutation: flip `L` in the light camera; separately,
      offset the light camera by half a texel.
    - **Off-screen caster:** the same sphere moved above the top edge of the
      view, its shadow still on screen: the shadow is drawn. Mutation: clip
      the depth range or the footprint to the view instead of the caster
      sphere (D5).
    - **Outside the footprint is lit:** a single caster sphere at the edge
      of a large ghost (transparent) layer that is not a caster: ghost
      pixels far from the sphere, outside every cascade square, equal
      shadows off. Mutation: remove the explicit `uv` bounds test (D7).
    - A bond above the layer casts; a CSG quad under an atom receives; a CSG
      cube above the layer casts (casters and receivers of every kind, D3).
    - A ghost sphere (transparent impostor) and a transparent isosurface
      above the layer cast nothing (the layer equals shadows off). A ghost
      inside the shadow is darker than outside it (receivers). The scene
      also holds a label and a grid line, so the transparent draws come
      after both have taken group 2: this is what exercises the re-bind
      helper (D8). Mutation: skip the re-bind before the transparent
      impostors; the draw must fail validation or the ghost must stop
      darkening.
    - A gadget-pass atom inside the shadow is not darkened.
    - A selected atom in shadow keeps its rim colour within 2/255 (the rim is
      mixed after lighting, DP D4).
    - *Headlight* with shadow strength 1 is byte-identical to strength 0, and
      `shadow_passes_drawn` stays 0.
- Effort: 3 d.

### Phase 2 — Adaptive cascades

- The full D6 criterion, the splits and `MAX_CASCADES`; the cascade blend
  band in `shadow_visibility`; the debug tint (D9).
- **Tests:**
  - `shadow_fit_test.rs`:
    - a compact block in full view: one cascade, in both projections and for
      a 2048 export of the same view
    - a perspective close-up on a 1 000 × 1 000 × 20 Å slab at a grazing
      angle: more than one cascade, at most four; the splits are contiguous
      and cover `[front, back]`; every cascade but the last meets
      `t ≤ max(κ·p(a), t_floor)`
    - the floor: a close-up on a 150 Å block, where `κ·p(a)` is far below
      `t_floor`, gives one cascade. Mutation: drop `t_floor` from the
      criterion; the count must rise.
    - the blend band is covered: on the grazing slab, random points (fixed
      seed) of each cascade's blend band project inside the *next* cascade's
      square, margin included. Mutation: fit cascade `k` over `[a, b]`
      instead of `[a', b]`.
    - an orthographic rod end-on: more than one cascade; cascade 0 meets the
      criterion
    - the even-share rule: an output wider than `κ·N = 4096` pixels on a
      deep slab, zoomed out so that `κ·p(a)` is above `t_floor`, where no
      thin slice meets the criterion: the cascades share what is left
      evenly, and the count is still at most four
    - the count is stable under a small orbit step about the slab's centre
      (no change between neighbouring orientations, ten steps)
  - GPU:
    - Debug tint on the grazing slab (built directly as an impostor lattice,
      about 40 000 spheres): the pixels nearest the camera carry cascade 0's
      tint and the farthest a higher one. Mutation: select the cascade by
      `split_near` instead of `split_far`.
    - **No seam:** a straight shadow edge that crosses a split stays
      continuous: its position on either side of the blend band differs by
      at most one output pixel. Mutation: set `blend_fraction` to 0.
    - **World-space softness:** the same shadow edge measured in a view whose
      cascade texel is 0.02 Å and in one whose texel is 0.06 Å has the same
      penumbra width in Å within 30 %. Mutation: radius in texels instead of
      Å.
    - The Phase 1 acne detector, repeated on the grazing slab with nothing
      above it.
    - The Phase 1 position check, with the caster sphere over the grazing
      slab at a depth that falls in cascade 1 or later (the debug tint
      confirms which), so a cascade other than 0 is checked against the CPU
      projection. Mutation: index `world_to_clip` with `k − 1`.
- Effort: 1.5 d.

### Phase 3 — The cache, the cost, the levers

- The cache in `ShadowMap`: the remembered setup, and `invalidate` from the
  upload (D9).
- **Tests** (GPU):
  - Two renders with nothing changed: the second records no shadow pass.
  - The refined frame after a rest records none.
  - A camera move records `count` passes; a non-lightweight content upload
    records them; a lightweight upload does not.
  - A thumbnail (two backgrounds) records one set.
  - An export, then a live frame: each records its own, and the live frame
    shows its own shadows (it matches a live frame rendered with no export
    before it). Mutation: leave the remembered setup unchanged on an
    offscreen draw; the live frame must then fail.
  - Turning shadows off frees the map: a renderer-side `shadow_map_allocated()`
    is false afterwards. Turning them on again draws correct shadows on the
    next frame (the new map remembers no setup).
- **Measure and record** in the phase notes, release build, an `#[ignore]`d
  harness test that prints and asserts nothing. The frame time with shadows
  off and on, on a 1 M-sphere impostor lattice (ball-and-stick, bonds
  included) and on a CSG solid of about 100 k triangles, at 2560 × 1440 with
  4× MSAA:
  - a moving frame, one cascade
  - a moving frame, four cascades: a close-up on the lattice, chosen so
    that `shadow_setup` returns four; the test asserts that count before it
    times anything, so a fit change cannot silently turn it into a
    one-cascade measurement
  - the refined frame
- **Budget,** over shadows off on the 1 M lattice: a moving frame with one
  cascade at most **+60 %**; with four cascades (the close-up on a large
  structure, the case cascades exist for) at most **+150 %**. The four-cascade
  frame draws the whole caster set four times, with early depth testing off
  for the impostors, so it is budgeted, not just recorded. If either is
  missed, the levers in order:
  1. 8 PCF taps in normal frames, 16 in the refined frame (the refined frame
     replaces the moving one when the view rests, so the difference is
     transient)
  2. `MAX_CASCADES = 3`
  3. `SHADOW_MAP_SIZE = 1536`

  Each changes the picture, so the choice is the walkthrough's.
- Effort: 1 d.

### Phase 4 — Preferences, the AO coupling and documentation (shadows become visible)

- `shadows` and `shadow_strength` in `RenderingPreferences`, the API twin,
  `to_shading_settings`, the two dialog rows with their *Headlight* disabling
  (D11).
- `ao_key` from D10 in `ShadingUniform::new`, and its twin.
- **Tests:**
  - Preferences: a `preferences.json` without the fields loads with shadows
    on at 0.7; the API twin round-trips; an unticked box gives strength 0;
    the strength is clamped.
  - `shading_uniform_test.rs`: `ao_key == AO_KEY` exactly at strength 0, and
    `AO_KEY_WITH_SHADOWS` at strength 1. With `settings.shadow_strength = 0.7`
    but a `None` setup (*Headlight*, or no casters), `ao_key == AO_KEY`
    exactly: AO keeps its full share of the key wherever no shadow is drawn.
    Mutation: compute `ao_key` from `settings.shadow_strength` alone,
    ignoring the setup.
  - GPU: with shadows off and AO on, byte-identical to Phase 3 (the coupling
    is inert at strength 0). The same with shadows on under *Headlight*.
- **Docs**, in this phase, because this is where the change becomes visible:
  - `doc/reference_guide/ui.md`, Rendering section:
    - A *Shadows* row: what casts and receives (atoms, bonds, solids,
      isosurfaces; not x-ray ghosts, transparent surfaces, lines, labels or
      the gizmo), that shadows come from the Studio key light and move with
      the camera, that a structure shadows another, the slider, on by
      default, no effect under *Headlight*. Its cost: every camera move draws
      the scene once more from the light, more on a close-up of a very large
      structure; unticking removes it entirely.
    - The *Ambient occlusion* row: "a little less of the key light" becomes
      "a little less of the key light, less still with shadows on".
    - The intro sentence lists *Shadows* among the appearance settings that
      exports and thumbnails follow.
  - `rust/AGENTS.md`, renderer bullets:
    - `ShadowMap` (`shadow_map.rs`) owns all shadow GPU state; the shadow
      map is rewritten whenever the setup it remembers disagrees with the
      draw's, so it is exempt from the per-view pairs rule
    - `ShadowCasterPipelines` is the one exception to "a new pipeline goes
      into `PipelineSet`", because it has a single target
    - lit pipelines are on `PipelineLayouts::lit`, and group 2 is re-bound
      after every lit `set_pipeline`
    - a new opaque content mesh must be added to the caster passes and to
      `caster_bounds`, or it neither casts nor fits
    - `fs_shadow` must not reach group 2 or `shading`
    - the shadow fit and the shading take the key from the one
      `key_light_direction`; a second derivation puts shadows on lit sides
  - `doc/design_depth_perception.md`, Future work: the shadow-map bullet
    points to this document.
- Effort: 1 d.

### Phase 5 — Walkthrough

- **Maintainer**, in the GUI after `cargo build --release`:
  - A mechanosynthesis step: the tool's shadow on the surface, at two tip
    heights.
  - A (100) 2×1 reconstruction and a step edge, ball-and-stick and
    space-filling. Look for acne, leaking at contacts, and the crease with
    AO on and off.
  - The T-centre scene and the 80-cell rod, perspective and orthographic,
    with the cascade debug tint on and off.
  - The nanobeam: a close-up, a full view, and an orbit (shimmer).
  - Orbiting a mechanosynthesis step: the shadows sweep across the scene
    with the camera-relative light (D1). Is it acceptable, or distracting?
  - A CSG solid beside atoms; a sketch on the drawing plane over atoms (Open
    question 1); a triangle-mesh-atom structure (Open question 3).
  - *Headlight*: no shadows, rows disabled. Shadows off: today's picture.
  - An exported image and a thumbnail.
  - Frame rate while orbiting the nanobeam, on the maintainer's machine and,
    if available, on integrated graphics.
  - Tune `SHADOW_SOFTNESS_A`, the default strength, `AO_KEY_WITH_SHADOWS`,
    and the Phase 3 levers if the budget was missed.
- Effort: 0.5 d.

**Total: about 7–7.5 d.** The report costed 4–6 d for one map plus 1–2 d for
a focus map. This design replaces the focus map with adaptive cascades (the
same cost), and adds the cache, the AO coupling and the GPU tests the report
did not count.

## Future work

- **Contact-hardening softness (PCSS):** a blocker search before the PCF, so
  a shadow is sharp at the contact and softens with the gap. It only adds a
  pass inside `shadow_visibility`; the map and the fit do not change.
- **SDSM:** fitting the splits to the visible depth range of the previous
  frame, read back alongside the colour (the readback is synchronous anyway).
  It tightens the far cascades on views where much of `[front, back]` is
  hidden.
- **Transparent casters:** ghosts or transparent isosurfaces casting a
  partial shadow, through a second, coloured map.
- **Fill-light shadows**, if the rig ever gains a second directional light
  that should read as one.
- **Ray-traced shadows for exports**, where the overdraw argument of D2 does
  not apply to a one-off render.
