# Depth Perception: Lighting Rig, Depth Cue and Baked Ambient Occlusion

Source: `reports/rendering_capabilities/rendering_capabilities_report_2026-09-21.md`
§2.3 (depth perception) and the rim-band item of §2.4. Builds on
`doc/design_screen_space_lines.md` and `doc/design_viewport_antialiasing.md`.
It relies on the per-sample impostor shading (AA D4/D5) and the per-view
uniform rule of the anti-aliasing design, and it changes neither; the new
`ShadingUniform` is outside that rule by construction (D2).

**Naming.** "D1"–"D10" always mean this document's decisions. Decisions of
the anti-aliasing design are written "AA D4", "AA D5": the *AA D5 marker*
is the source marker `without_per_sample_shading` cuts at, and the *AA D4
coverage block* is the impostor coverage code at the top of `fs_main`.

## Motivation

The anti-aliasing and screen-space line work removed the jaggies, the moiré
and the blur. With the edges now clean, the flat lighting is the most visible
thing left. Every lit surface in the viewport is shaded by the same rig:

- **One headlight.** It sits 20° above the view direction
  (`Camera::calc_headlight_direction`), so it moves with the eye. Every
  sphere therefore looks the same: bright in the middle and darker towards
  the outline, the same amount on all sides. Nothing tells the eye which way
  a surface faces, except whether it faces the viewer.
- **A constant ambient of 0.2.** A hydrogen deep in a crevice is lit exactly
  like one sticking out of an adatom. An atom on a flat terrace looks the same
  as one at the bottom of a groove. Report shots `c11` (space-filling) and
  `c15` (ball-and-stick) show it.
- **No depth cue.** Along a long rod, the far end looks exactly like the near
  end (`60_rod_depth_bs`). The only depth signals are occlusion and
  perspective, and orthographic projection removes perspective too.
- **CSG solids** get the same headlight, so the visible faces of a cube come
  out in nearly one shade (`01_geo_solid`).

This hurts the tasks the application exists for. Examples: reading the shape
of a surface reconstruction, spotting a vacancy or an adatom on a terrace,
telling a tool tip from the workpiece behind it, and every picture that leaves
the application.

This document adds three things:

- A **camera-relative studio lighting rig**: a key light, a fill light and a
  hemisphere ambient. The old headlight remains available as a preset.
- A **depth cue**: fog towards the background colour, measured across the
  scene's depth.
- **Per-atom baked ambient occlusion**: an exposure value computed on the CPU
  once per evaluated structure and cached with it, which darkens the ambient
  light of buried atoms. It can be switched off, and then it costs nothing.

All three are cheap per pixel and need no new render target, no post-process
pass and no extra draw. Along the way the lighting code, now copied into four
shaders, moves into one shared WGSL file. The selection/frozen/marked rim band
becomes a screen-space ring drawn after lighting, because the new rig would
otherwise light it unevenly around the outline.

### Scope

- **In:**
  - one shared lighting module for the four lit shaders (D1)
  - a per-pass `ShadingUniform` (D2)
  - the studio rig and the *Headlight* preset (D3)
  - the rim band drawn after lighting, at a screen-space width (D4)
  - the depth cue (D5)
  - per-atom exposure for impostor atoms and bonds (D6, D7), with a bent
    vector so the shading varies across each sphere (D10)
  - preferences for all of it (D8)
  - caching exposure on the scene, so it is not recomputed on every
    refresh (D9)
- **Out (see Future work):**
  - shadow maps
  - screen-space AO
  - HDR and the post-process chain
  - emissive materials
  - fog on lines and labels
  - occlusion between different displayed structures
  - AO on the polygonal (triangle-mesh) atom path, which the report also
    left out of scope
  - AO on CSG meshes and isosurfaces

## Current state (facts, with references)

All in `rust/crates/atomcad-renderer/src/` unless noted.

- **Four lit shaders, four copies of the lighting.** `atom_impostor.wgsl`,
  `bond_impostor.wgsl`, `transparent_impostor.wgsl` and `mesh.wgsl` each
  declare `fresnel_schlick`, `distribution_ggx`, `geometry_schlick_ggx` and
  `geometry_smith`. The three impostor files also have their own
  `calculate_pbr_lighting`; `mesh.wgsl` inlines the same code in `fs_main`.
  The constants are the same everywhere:
  - `light_color = 2.0`
  - `ambient_light_color = 0.2`
  - Reinhard tone mapping `c / (c + 1)`
  - gamma `pow(c, 1/2.2)`
- **Dead code.** `bond_impostor.wgsl` also contains `calculate_bond_lighting`
  (Lambert + Phong), which nothing calls.
- **Light direction.** `CameraUniform.head_light_dir` (offset 208) is
  computed by `Camera::calc_headlight_direction` (`camera.rs:94`): the
  forward vector rotated 20° downward about the camera's right axis. Every
  lit shader uses `L = -head_light_dir`.
- **Orthographic inconsistency in `mesh.wgsl`.** The impostor shaders build
  `V` with `camera_view_vector`, which in orthographic mode uses the camera's
  backward axis. `mesh.wgsl:99` uses
  `normalize(camera_position - world_position)` in both projections, so the
  specular highlights on orthographic meshes are slightly wrong.
- **Rim band.**
  - `shade_atom_hit` (in `atom_impostor.wgsl` and in the atom branch of
    `transparent_impostor.wgsl`) blends `rim_color` into the **albedo**
    before lighting, with weight
    `smoothstep(0.18, 0.22, 1 − N·V) × rim_color.a`.
  - That band covers everything beyond about 35° from the view axis: an
    outer ring holding roughly two thirds of the sphere's disc, whatever the
    sphere's size on screen. Report shot `71` shows the result.
  - The band is lit, so its brightness depends on `N·L` around the outline.
  - Delete markers rely on it: they get a dark neutral albedo
    (`atomic_tessellator.rs`, "rim provides the red signal").
- **Bonds have no material.** `shade_bond_hit` lights them with roughness
  0.5 and metallic 0 (literals).
- **Passes.**
  - `draw_to_target` (`renderer.rs`) opens a main pass. In order it draws:
    the triangle mesh, the opaque impostors, labels, lines, transparent
    impostors and transparent isosurfaces.
  - It then opens an optional gadget pass, which clears depth and draws the
    lightweight triangles (gizmo, pivot cube), the gadget atom and bond
    impostors (guide dots) and the gadget lines.
  - **Both passes use the same pipelines and the same group-0 bind group.**
- **Per-view uniforms.**
  - Group 0 is `CameraUniform` alone. It exists once per `View` (live,
    offscreen).
  - The live one is rewritten only when the camera moves or the target
    changes.
  - `draw_to_target` receives `background_color_rgb` on every call. The
    thumbnail draws the same view twice, on a light and a dark background,
    and keeps one of the two (`renderer.rs:1171`).
- **Content bounds already exist.** `content_bounds` (`renderer.rs:270`) is
  recorded on every content upload by `thumbnail::content_bounds`. It is the
  AABB of the triangle, wireframe, opaque and transparent impostor and
  isosurface content, without gadgets, labels or background lines.
- **Atom vertices.**
  - `AtomImpostorVertex` is 15 floats (60 bytes): centre, quad offset,
    radius, albedo, roughness, metallic, rim colour.
  - `BondImpostorVertex` has start, end, quad offset, radius and colour.
  - `TransparentImpostorVertex` serves both atoms and bonds.
- **Impostor tessellation** (`atomcad-display/src/atomic_tessellator.rs`).
  `tessellate_atomic_structure_impostors(meshes…, structure, viz_prefs)`
  is called once per displayed atomic structure by
  `scene_tessellator.rs:279` in `atomcad-structure-designer`. It runs
  sequentially. It skips culled atoms (`should_cull_atom`, by
  `in_crystal_depth`) and atoms with alpha 0. It routes atoms with
  alpha < 1 to the transparent mesh. The default cull depths are 8 Å in
  ball-and-stick and 3 Å space-filling
  (`atomcad-structure-designer/src/preferences.rs:301`).
- **Tessellation is not cached; evaluation is.**
  - Every refresh that is not lightweight re-tessellates **every** displayed
    node from scratch (`tessellate_non_lightweight_content`, reached from
    `refresh_structure_designer`, `api_common.rs:562`). Nothing keeps
    impostor meshes between refreshes.
  - Evaluation is incremental. A partial refresh re-evaluates only the nodes
    downstream of a change (`refresh_partial`,
    `structure_designer.rs:2025`) and keeps every other `NodeSceneData`.
  - Selecting a different node re-evaluates both the previously and the
    newly selected displayed node (`structure_designer.rs:2167`), because
    `from_selected_node` changes. Their atoms are unchanged.
  - Measured on the 1.08 M-atom nanobeam (`doc/design_rust_crate_split.md`,
    Phase 0 benchmark, release, thin LTO): evaluate (CSG + materialize)
    2,089 ms; impostor tessellation 433 ms.
  - So anything added to `tessellate_atomic_structure_impostors` is paid on
    every edit anywhere in the network, while materialize is paid once.
- **Depth inside the crystal.** `Atom::in_crystal_depth`
  (`atomcad-crystolecule/.../atom.rs:10`) is set by lattice fill and copied
  by a few structure operations. Other atoms carry 0.
- **Neighbour queries.**
  - `AtomicStructure` keeps a sparse grid with 4 Å cells and
    `get_atoms_in_radius`, which allocates a `Vec` per call.
  - `atomcad_crystolecule::simulation::spatial_grid::SpatialGrid` builds a
    grid over a flat position array and exposes `for_each_neighbor`, which
    allocates nothing.
- **Preferences.**
  - `RenderingPreferences { anti_aliasing, full_display_resolution }`
    (`atomcad-structure-designer/src/preferences.rs:640`) is pushed to the
    renderer on every refresh as `RenderQuality` (`api_common.rs:604`).
  - The renderer's rule: a setter called on every refresh bumps the live
    generation only when its value changed (`rust/AGENTS.md`).
  - The user-facing rows are in `doc/reference_guide/ui.md:1177`.

## Decisions

### D1 — One lighting module, composed into the four lit shaders

WGSL has no `#include`. The renderer already edits shader source before
compiling it: `without_per_sample_shading` cuts the impostor files at the
AA D5 marker. So this design adds one source composition step: a new file,
`lighting.wgsl`, is **prepended** to `atom_impostor.wgsl`,
`bond_impostor.wgsl`, `transparent_impostor.wgsl` and `mesh.wgsl` before
the module is created.

`lighting.wgsl` holds:

- the `ShadingUniform` struct and its binding (D2)
- the four PBR helpers
- `light_surface(world_position, N, albedo, roughness, metallic, exposure) -> vec3`,
  which returns linear colour. `exposure: vec4<f32>` is the packed
  `(4b, E)` exactly as the vertex delivers it (D10); it is unpacked inside.
  Callers without per-atom exposure (`mesh.wgsl`, `transparent_impostor.wgsl`)
  pass `vec4(0.0, 0.0, 0.0, 1.0)`, the fully exposed value.
- `to_display(linear) -> vec3`, which applies Reinhard and gamma, unchanged
- `apply_depth_cue(color, world_position) -> vec3` (D5)
- `apply_rim(color, rim_color, rim_weight) -> vec3` (D4), applied last

The display-space order is fixed: `to_display` → `apply_depth_cue` →
`apply_rim`. The rim is a UI signal and is neither lit nor fogged.

`camera_backward` and `camera_view_vector` move into `lighting.wgsl` as
well, so every lit shader builds `V` the same way. This fixes the
orthographic `mesh.wgsl` inconsistency. The host files drop their own
copies of these two functions and of the PBR helpers; `camera_right` and
`camera_up` stay where they are used. The unused `calculate_bond_lighting`
is deleted.

`CameraUniform` is still declared in each file, because `line_mesh.wgsl`
and `label.wgsl` need it and are not lit. `lighting.wgsl` therefore reads
`camera` but does not declare it: it is valid only when prepended after the
host file's declarations. To make that work, the composition puts the
**host's header** first (everything up to and including the group-1
`ModelUniform` binding), then `lighting.wgsl`, then the rest of the host
file. The split point is a marker comment, `// ---- lighting ----`, placed
in each host file. This is the same mechanism as the AA D5 marker, applied
in the other direction.

Composition is a pure function, `pipelines::compose_lit_shader(host: &str) -> String`.
`Shaders::new` calls it, and so does `shader_test.rs`. The naga validation
and the `CameraUniform` offset checks therefore run on exactly the source
the GPU compiles. The composed source passes through
`without_per_sample_shading` afterwards, as now.

**Rejected:** a `naga_oil`-style preprocessor (a new dependency for one
include), and building every shader from `format!` fragments (it hides the
shader from anyone reading the `.wgsl`).

### D2 — A `ShadingUniform` at group 0, binding 1, one per pass, written on every draw

The rig's directions follow the camera. The depth cue depends on the camera,
the content bounds and the background colour, and the background colour can
differ between two draws of the same view (the thumbnail). So:

- **A new uniform, `ShadingUniform`, at `@group(0) @binding(1)`**, visible
  to the fragment stage. It is added to the one shared group-0 bind group
  layout (`camera_bind_group_layout`), so every pipeline layout sees it,
  the line and label layouts included. `line_mesh.wgsl` and `label.wgsl`
  do not declare it, which wgpu allows.
- **Written by `draw_to_target` on every call**, from the pure
  `ShadingUniform::new(camera, content_bounds, background, settings, pass)`.
  It is a few dozen bytes and the write is queued before the draw's submit.
  Writing it on every draw means no value can survive from an offscreen
  render into a live frame. That is the failure the "per-view uniforms come
  in pairs" rule guards against, and it cannot happen here.
- **One buffer per pass, not per view.** The gadget pass gets a variant
  with the depth cue off (D5): gizmos and guide dots are editing aids and
  must not fade. `draw_to_target` writes the content variant and the gadget
  variant, and the camera bind group becomes **four** bind groups:
  {live, offscreen} × {content, gadget}. Each pairs that view's camera
  buffer with that pass's shading buffer.
- **Rejected: a per-mesh flag in `ModelUniform`.** The split that matters is
  content against editing aids, and the two passes already make that split
  exactly. Both triangle meshes and both impostor pairs are already divided
  between the passes along that line.
- **Rejected: extending `CameraUniform`.** That struct is rewritten only when
  the camera moves, and it must match byte for byte across six files. The
  background colour does not belong to the camera.

**The renderer's default `ShadingSettings` is today's look:** *Headlight*,
depth-cue strength 0, AO strength 0. The user's defaults (*Studio*, cue
and AO on, D8) live in the preferences and reach the renderer through
`set_shading` on every refresh. Keeping the renderer default neutral is
what lets every existing GPU test, and the "byte-identical" checks of
the phases, run unchanged without setting anything.

`head_light_dir` stays in `CameraUniform`. It is the *Headlight* preset's
key direction (D3), and `ShadingUniform::new` copies it from there, so
the `CameraUniform` layout does not change.

`ShadingUniform` gets the same offset table and `offset_of!` checks as
`CameraUniform`, in `shader_test.rs` and a new `shading_uniform_test.rs`.

### D3 — A camera-relative studio rig, with the old headlight as a preset

```text
radiance = key  · (diffuse + specular) · (N·L_key)⁺       · key_factor
         + fill · diffuse · (N·L_fill)⁺                   · ambient_factor
         + mix(ground, sky, ½ + ½ N·up_cam) · albedo      · ambient_factor
```

`ambient_factor` and `key_factor` come from the exposure (D7, "From
exposure to light"). Both are 1 for a fully exposed surface and whenever
AO is off.

Specular ambient and metallic blending stay as they are. Only the ambient's
magnitude changes, from 0.2 to the hemisphere value.

- **Key light** from the upper left, about 35° above and 30° to the left of
  the view direction. It is camera-relative, as the report recommends, so the
  shading stays fixed while the user orbits. With a world-fixed light, the
  dark side would turn to face the user during half of every orbit.
  Directions are built on the CPU from the camera basis, the way
  `head_light_dir` is now. The upper-left convention is the one molecular
  graphics and technical illustration use.
- **Fill light** from the lower right, about 20° below and 40° to the right,
  diffuse only. Two specular highlights per sphere read as two light sources,
  and that is a distraction on a lattice of identical spheres.
- **Hemisphere ambient** around the camera's up axis. It is brighter from
  above (sky) than from below (ground), so even the unlit side of a sphere
  shows which way it faces.
- **No rim light.** A bright outline on every atom would compete with the
  rim band, the one channel reserved for selection, frozen and marked state
  (D4).
- **Exposure** (D6) acts on the fill and the ambient in full, and on the key
  only weakly (`AO_KEY`, D7). Ambient occlusion stands in for missing
  indirect light, and the fill is a stand-in for indirect light too. The key
  is direct light: darkening it fully by burial would look like a shadow
  without a shadow's direction. A small share is still applied so that the
  key does not wash out the creases (D7).

**Calibration constraint.** The new look must not shift the colour palette.
A surface facing the viewer (`N = V`) with white albedo, roughness 0.5 and
metallic 0 must come out within ±5 % of today's luminance before tone
mapping. Without that, every element colour and every saved screenshot
would brighten or darken at once, and that would read as a palette change,
not a lighting change. The starting constants are key 2.2, fill 0.6, sky
0.26, ground 0.12. They meet the constraint on a back-of-the-envelope
estimate; Phase 2 tunes them by a test (below), not by eye.

**The *Headlight* preset** consists of these values:

- key direction = `head_light_dir`
- key intensity = 2.0
- fill = 0
- sky = ground = 0.2

With those values the formula reproduces today's shading exactly, or within
float reordering. That is both the user's way back (D8) and Phase 1's
regression oracle.

The rig's constants are named constants in `shading_uniform.rs`. They are
not preferences, because the space of good rigs is small and a slider per
light would be a trap. The preference is the preset (D8).

### D4 — The rim band is applied after lighting, at a screen-space width

- **After lighting and after the depth cue.** The rim colour is mixed into
  the display-space output (`apply_rim`, after `to_display` and after
  `apply_depth_cue`), not into the albedo. Selection orange, frozen cyan and
  marked magenta then read the same all around the outline, under either
  preset and at any depth. They are UI signals, and a UI signal should be
  neither lit nor fogged: a selected atom at the back of the scene must stay
  as clearly selected as one at the front. Delete markers keep working:
  their red signal now comes from an unlit ring around a dark sphere,
  which is if anything clearer.
- **Screen-space width.** The band is where the view ray passes within
  `w` render pixels of the silhouette. Here
  `w = min(RIM_WIDTH_PX × pixel_scale, 0.35 × r_px)`, with
  `RIM_WIDTH_PX = 3` and a 1-pixel smooth inner edge. `r_px` is the sphere's
  radius in pixels, so a sphere a few pixels across does not turn entirely
  rim-coloured.
- **How it is computed.** The shaders already have what this needs:
  - `sphere_ray_distance` gives the ray's distance outside the silhouette in
    world units (negative inside).
  - The derivatives taken at the top of `fs_main` (and of
    `fs_main_per_sample`) give the world size of a pixel.
  - The pixel distance is `−distance / pixel_world`.

  The early-out structure of the AA D4 coverage block is unchanged, because
  the computation runs only on fragments that are already shaded.
- Both `shade_atom_hit` copies change, the opaque one and the transparent
  one. Bonds have no rim.

This is the report's "rim band tuning" item, with one change. The report
asked only for the width. Applying the rim after lighting is new, and the
rig change makes it necessary: under a side key light, the far side of a
lit rim would fall into shade.

### D5 — Depth cue: fog to the background colour, from the nearest content over a fixed span

```text
depth   = view-space distance along the view axis (both projections)
c       = view depth of the content AABB's centre
h       = the AABB's half extents;  a = the view axis in world space
front   = max(c − (h.x·|a.x| + h.y·|a.y| + h.z·|a.z|), near)
span    = 2R                    R = |h|, half the AABB's diagonal (the radius
                                of its circumscribed "bounding sphere")
f       = strength · clamp((depth − front) / span, 0, 1)
out.rgb = mix(out.rgb, background, f)   (after to_display, before apply_rim)
```

- **The fog starts at the nearest content.** `front` is the depth of the
  AABB's nearest corner (its support point along the view axis), so the
  front of the content is never fogged, whatever its shape. Anchoring at the
  bounding sphere's front instead would fog all content that sits inside
  the sphere: a 100 × 100 × 10 Å slab seen from above (R ≈ 71 Å) would get
  f ≈ 0.23–0.27 on every atom, a uniform 25 % wash towards the background
  with no depth information in it, and even a cube's front face would get
  f ≈ 0.1. That would also defeat the D3 calibration constraint on exactly
  the views the application is used for.
- **The span is fixed at the bounding sphere's diameter**, not the AABB's
  projected depth range, and not the near and far planes. The far plane is
  2400 Å, and a cue spread over that range would be invisible on any real
  structure. A fixed span keeps the cue's gradient (fog per Å) constant
  while the camera orbits: thin content seen face-on gets almost no cue
  (it has almost no depth), and the same content seen edge-on gets the
  full gradient across its depth.
- **Rotation behaviour.** `front` is a support function, so it moves
  continuously as the camera orbits, by at most `R − min(h)`. An atom's fog
  therefore changes smoothly and slowly during a rotation; nothing jumps.
  The nearest content stays at f = 0 at every orientation, which matters
  more than a perfectly constant per-atom value.
- **Mixed in display space, towards the actual background.** The fully
  fogged colour must equal the background pixel exactly, on the light and on
  the dark background. Mixing after tone mapping and gamma, towards the same
  sRGB value the pass clears to, guarantees that. A cue in linear space would
  converge on a slightly different colour.
- **`strength` < 1 by default (0.5).** The far end of the scene is dimmed,
  never erased: the cue says "further away", not "gone".
- **What fogs.** Everything lit in the main pass:
  - the triangle mesh (CSG solids, opaque isosurfaces)
  - the opaque and transparent impostors
  - the transparent isosurfaces

  For blended pipelines only `rgb` is mixed and alpha is unchanged.
- **What does not fog.**
  - Lines and labels (unlit annotations that must stay legible). Lines
    already have their own distance fade on the grid, D9 of the lines
    design.
  - Everything in the gadget pass, whose `ShadingUniform` has `strength = 0`
    (D2).
- **No content** (`content_bounds == None`): `strength = 0`.
- **Not the same thing as `fade_depth`.** The style rule `fade_depth` fades
  atoms by depth *inside the crystal*, and its xray counterpart works the
  same way. That is a property of the structure. The depth cue fades by
  depth *from the viewer*. The two compose without interacting.

**Known limitation**, also Open question 2: in a close-up on a very large
structure, such as the million-atom nanobeam, the span is hundreds of Å
and the visible part covers a small slice of it, so the cue is weak there.

### D6 — Per-atom exposure: a solid-angle visibility from the atom's centre, baked on the CPU

**What is computed.** For each atom `i`, its exposure `E_i ∈ [0, 1]`:

1. Take a fixed set of `D = 128` directions, a Fibonacci sphere,
   deterministic.
2. Every occluder `j` within `R_MAX = 6 Å` of `i` blocks the directions
   inside the cone from `i`'s centre that subtends `j`'s occluder sphere.
   The cone's half-angle is `asin(min(1, r_j / d_ij))`.
3. `E_i` is the fraction of directions that no occluder blocks.

Results by position:

| Atom | `E_i` |
|---|---|
| Isolated | 1 |
| Adatom | high |
| Atom on a flat terrace | about ½ |
| Atom at the bottom of a groove or below a step | lower |
| Atom fully inside the bulk | about 0 |

This is the half-sphere-exposure family of burial measures, not physical
ambient occlusion. On its own, a scalar would darken a whole sphere
evenly: it shows which atoms are exposed and which are buried, but not the
crease where two atoms touch, which is most of what makes ambient occlusion
read as such. So the same pass also keeps the *direction* of the open sky,
a bent vector `b_i`, and the shader evaluates visibility per pixel from the
pair `(E_i, b_i)`. That is D10.

**Occluder radius = covalent radius, for every display style.** With van
der Waals radii, each bonded neighbour sits closer than its own radius and
would block a full hemisphere, so every bonded atom would come out buried.
With the displayed radius, a ball-and-stick structure would hardly occlude
at all, and its exposure would change when its style changed. Covalent radii
block a solid angle in proportion to atom size: four diamond neighbours at
1.54 Å block about a quarter of the sphere, and the second and third shells
fill most of the rest. The value is also the same for a structure shown
space-filling or ball-and-stick, and for an atom whose `apply_style`
`render_style` differs from its neighbours'. The radius is computed in one
function, `occluder_radius(atom)`, so Open question 1 can be decided in a
single line.

**Who occludes, who receives.**

- **Occluders:** every atom of the structure whose **structure alpha**
  `get_atom_alpha(id)` is 1, **culled atoms included**. Culling hides
  interior atoms only to save rendering time. Those atoms still exist, and
  leaving them out would make the surface atoms above them look open from
  below. Ghosts (alpha < 1, set by `xray`) do not occlude: a ghosted lattice
  around a highlighted dopant should leave the dopant looking lit, and here
  it does.
  - The test is on the structure's own alpha, **not** the display alpha the
    tessellator draws with (`global_alpha × get_atom_alpha`,
    `atomic_tessellator.rs`), because `global_alpha` comes from the
    *Scene transparency* preference. Using it would make exposure depend on
    a preference, which "depends on the structure alone" below rules out
    and D9 relies on.
- **Receivers:** atoms drawn opaque and unculled, i.e. by the tessellator's
  effective alpha, preference included. These are the only ones whose value
  is used, so the computation iterates over them alone. Culled interior
  atoms, typically most of a lattice-filled crystal, cost nothing as
  receivers. With *Scene transparency* on, every atom is a ghost and there
  are no receivers; the cached values stay valid for when it is turned off.
- **An atom's exposure depends on the structure alone**: the positions,
  occluder radii and structure alphas of the atoms within `R_MAX`. It does
  not depend
  on which other atoms are receivers, on the display style or on any
  preference. D9 relies on this. A value computed once stays valid while
  nothing within `R_MAX` of the atom changes, even if the cull depths change
  the set of receivers.
- **Exposure 1** (written as the packed `(0, 0, 0, 1)`):
  - ghosts (the transparent mesh, whose vertex keeps its layout)
  - delete and unchanged markers
  - anchor spheres
  - every atom of a diff structure (`is_diff()`; these are edit overlays,
    not matter): `ao_receivers` returns no receivers for one
  - gadget impostors
  - any receiver that has no value yet (absent from `AtomExposure`)

**Scope of the neighbourhood: one `AtomicStructure`.** Exposure is computed
per displayed atomic output, so a tool does not darken the surface beneath
it when the two are separate displayed outputs. Occlusion across structures
needs `update_exposure` (D9) to gather every displayed structure first.
That is Future work, and it is the natural companion of shadows.

**Rejected:**

- **`in_crystal_depth` as the darkening factor**, the report's
  "crystal-depth" option. Only lattice fill sets it, edits do not update it,
  and it measures distance to the blueprint surface, not exposure. An adatom
  and the terrace atom under it both have depth 0.
- **Neighbour count.** It cannot tell a terrace atom (open above) from an
  atom with the same number of neighbours on all sides.
- **Shrake–Rupley relative SASA.** It is exact for a solvent probe but blind
  to openness beyond the probe size: an atom at the bottom of a wide pit is
  fully "accessible".
- **GPU visibility maps.** QuteMol's approach renders many depth maps and
  counts per-atom visibility, which is exact and works across structures,
  but it is a compute path in the renderer and a multi-day design of its
  own.

### D7 — Cap masks from a precomputed table; the exact cone test is the test oracle

The exact test costs `D` dot products per neighbour: about 100 neighbours ×
128 directions × 10⁶ atoms ≈ 10¹⁰ operations on the nanobeam. That is several
seconds on every content refresh. So:

- **Each occluder contributes a precomputed 128-bit mask**, and an atom's
  blocked set is the OR of its occluders' masks. `E_i` is
  `1 − popcount / 128`.
- **The table.**
  - It is indexed by the occluder's direction, quantised to a cube-map cell
    (6 faces × 16 × 16), and by its half-angle, quantised to 32 buckets
    over [0°, 90°].
  - Each entry holds the directions within the half-angle of the cell
    centre, so the cell's own angular size is absorbed into the half-angle.
  - It is 6 × 256 × 32 × 16 bytes ≈ 0.8 MB, built once in a `OnceLock`.
- **Cost.**
  - About 100 table lookups and ORs per receiver, with an early exit as soon
    as the mask is full.
  - Receivers are processed with `rayon`, which `atomcad-display` adds from
    `[workspace.dependencies]`.
  - Neighbours come from one `SpatialGrid` built over the occluders, with a
    6 Å cell, through `for_each_neighbor` (no allocation per atom).
- **Accuracy.** `E_i` is a shading input. A few percent of quantisation
  error is invisible after the mapping below, and a test bounds it (Phase 4).
- **Expected cost (an estimate, to be replaced by Phase 4's measurement).**
  The worked case is a 1 M-atom diamond cuboid, about 178 Å on a side,
  shown in ball-and-stick:

  | Step | Work | Estimate |
  |---|---|---|
  | Grid over the occluders | every opaque atom, culled ones included: a linear pass | 50–100 ms |
  | Receivers | the 8 Å unculled shell, about 25 % of the atoms: ≈ 250 k | — |
  | Per receiver | about 1 000 grid candidates; 80–160 within `R_MAX`, each one table lookup and an OR | 3–5 µs on one core |
  | Total, with `rayon` | | ≈ 150–250 ms |

  - With culling off, or on atoms that have lost `in_crystal_depth`, all
    1 M atoms are receivers. Buried ones stop early once their mask is
    full, and the estimate is ≈ 300–400 ms.
  - Against the measured nanobeam numbers (2,089 ms evaluate, 433 ms
    tessellation), one full computation is about 10–20 % of
    materialize. It is never slower than materialize, but it is comparable
    to the whole impostor tessellation.
  - That is acceptable **once per evaluated structure** and unacceptable on
    every refresh, which is why D9 caches it.
- **Budget (absolute).** One full computation, as `atom_exposure` on every
  receiver, takes **at most 15 % of the evaluate time** of the same
  structure. This is measured in Phase 4 on a lattice-filled 1 M-atom cuboid
  and on the 80-cell rod (33 445 atoms), and recorded in the phase notes.
  If the target is missed, the levers in order are:
  1. `D = 64` (a `u64` mask);
  2. a smaller `R_MAX` (5 Å);
  3. parallelising the grid build.

**Where it lives.** A new module, `atomcad-display/src/ambient_occlusion.rs`.
Every function in it is pure, and none reads a preference:

```rust
/// The packed `(4b, E)` (D10) of each atom in `receivers`, keyed by atom
/// id; absent ids are fully exposed. Builds one `SpatialGrid` over the
/// occluders; runs the receivers under `rayon`.
pub fn atom_exposure(
    structure: &AtomicStructure,
    receivers: &[u32],
) -> (AtomExposure, ExposureStats);

/// The occluder state of every atom: id → (position, occluder radius,
/// occluder or not). About 24 bytes per atom. O(atoms).
pub fn occluder_snapshot(structure: &AtomicStructure) -> OccluderSnapshot;

/// The atoms whose occluder state differs between `old` and `new`
/// (position, occluder radius, occluder or not, added, removed), with
/// their old and new positions. O(atoms).
pub fn occlusion_changes(old: &OccluderSnapshot, new: &AtomicStructure) -> OcclusionChanges;

/// The union of two change sets, keeping every old and new position of
/// either. Used to fold a new diff into changes still pending.
pub fn merge_changes(pending: OcclusionChanges, new: OcclusionChanges) -> OcclusionChanges;

/// `previous` brought up to date for `structure`, holding values for
/// `receivers` only. Drops the values within `R_MAX + max occluder radius`
/// of a changed position, old or new, and the values of atoms that are no
/// longer receivers, then computes every receiver that has no value. When
/// at most `FULL_PATH_FRACTION` (¼) of the receivers need computing, they go
/// through `AtomicStructure`'s own grid; above it, through
/// `atom_exposure`'s path (the "full path").
pub fn update_atom_exposure(
    previous: &AtomExposure,
    changes: &OcclusionChanges,
    structure: &AtomicStructure,
    receivers: &[u32],
) -> (AtomExposure, ExposureStats);

/// What one call did: receivers computed, and whether it took the full
/// path. Returned per call, never kept in a global (Testing layer 1).
pub struct ExposureStats { pub receivers_computed: usize, pub full_path: bool }
```

`AtomExposure` is an `FxHashMap<u32, [i8; 4]>` wrapped in a newtype: the
packed `(4b, E)` of D10, in the vertex format it is uploaded in. Both
paths compute each receiver with the same kernel over the same neighbour
set, and both use one shared `within_r_max(d²)` predicate, so the two grids
cannot disagree on a neighbour at the boundary. Since the mask is an OR,
neighbour order does not matter. **So an incremental update is
bit-identical to a full computation**, and Phase 4 tests exactly that.

The diff runs against an `OccluderSnapshot`, not the previous
`AtomicStructure`. That way the cache never needs the old structure to be
alive (D9, "Carried across a re-evaluation"), at a cost of about 24 MB per
million atoms while AO is on.

The tessellator does not call these functions: it reads the exposure from
the scene's cache, which a separate pass fills (D9).
`tessellate_atomic_structure_impostors` gains an
`exposure: Option<&AtomExposure>` parameter, where `None` means AO is off
and every vertex is fully exposed (`b = 0`, `E = 1`). `atomic_tessellator`
also exposes the receiver selection it already does (`should_cull_atom`,
alpha) as `ao_receivers(structure, viz_prefs) -> Vec<u32>`. The cache pass
uses that function, so the two can never disagree.

**Why `atomcad-display` and not `atomcad-crystolecule`.** The kernel reads
nothing but an `AtomicStructure`, so it could live in the domain crate. It
does not belong there, because every rule in it is a rendering policy:
- covalent radius as a stand-in for an occluder (D6);
- the ghost rule, which follows the `xray` alpha;
- exposure 1 for diff structures (edit overlays);
- the receiver set, which follows the cull depths;
- the packed `Snorm8x4` value, which is a vertex format.

Placing it in crystolecule would make the domain crate carry rendering
rules. Placing it in the display crate matches its role as the domain →
renderer adapter, and the crate already depends on crystolecule for
`SpatialGrid` and the covalent radii. `atomcad-structure-designer` owns
only the cache and the policy of *when* to update it (D9). It already
depends on the display crate through `scene_tessellator`, so the DAG does
not change.

**Into the vertices.**

- `AtomImpostorVertex` gains `exposure: [i8; 4]`, uploaded as vertex
  format `Snorm8x4` and read in WGSL as `vec4<f32>`: the packed
  `(4b.x, 4b.y, 4b.z, E)` (D10) in 4 bytes. The vertex grows from 60 to
  64 bytes.
- `BondImpostorVertex` gains `exposure_start, exposure_end`, two more
  `[i8; 4]` / `Snorm8x4`. The bond shader mixes the two by the hit point's
  axial parameter `t = projection_length / cylinder_length` (both already
  computed in `shade_bond_hit`), so a bond darkens towards its buried end
  and on the side facing its neighbours.
- `TransparentImpostorVertex` is unchanged (ghosts are fully exposed).
- `add_atom_quad` and `add_bond_quad` keep their signatures and write the
  fully exposed value `(0, 0, 0, 1)`. New `…_with_exposure` variants serve
  the tessellator. Every other producer, including gadgets, anchors and
  markers, is unchanged and comes out fully exposed.
- The memory cost on the nanobeam in ball-and-stick, at 4 vertices per
  quad and about 2 bonds per atom: about 16 MB for atoms and 64 MB for
  bonds. That is the same as plain `f32` exposures would cost, because the
  bent vector rides in the same 4 bytes. Instanced bond and atom quads (AA
  design, Future work) would recover it several times over.

**From exposure to light**, in `light_surface`, with `V(N)` the per-pixel
visibility of D10:

```text
ao(N)          = AO_FLOOR + (1 − AO_FLOOR) · smoothstep(0, AO_OPEN, V(N))
ambient_factor = mix(1, ao(N), ao_strength)              // fill + hemisphere
key_factor     = mix(1, ao(N), ao_strength · AO_KEY)     // key light
```

Starting values: `AO_FLOOR = 0.25`, `AO_OPEN = 0.6`, `AO_KEY = 0.35`.
- With these, the open side of an adatom and of a terrace atom is unshaded.
  Their sides facing neighbours darken, and so does the floor of a groove.
- `AO_KEY` lets AO dim the key light a little: a soft stand-in for the
  shadows of Future work. Without it, the strong key light washes out the
  creases on every surface that faces the viewer.
- `AO_KEY` does not touch the D3 calibration, which is measured on a
  surface that is fully exposed.
- The constants are shader constants, tuned in the walkthrough.
  `ao_strength` comes from the `ShadingUniform`, so moving the strength
  slider changes no geometry.

### D8 — Preferences: a lighting preset, a depth cue and ambient occlusion, all on by default

| Preference | Default | Range | Reaches |
|---|---|---|---|
| *Lighting* | Studio | Studio, Headlight | renderer (`ShadingUniform`) |
| *Depth cue* | on | — | renderer |
| *Depth cue strength* | 0.5 | 0–1 | renderer |
| *Ambient occlusion* | on | — | display tessellation **and** renderer |
| *Ambient occlusion strength* | 1.0 | 0–1 | renderer |

**Storage and path.**

- All five are new `#[serde(default)]` fields of `RenderingPreferences`,
  next to the anti-aliasing pair, and they follow its path to the
  **Rendering** section of the Preferences dialog under a *Lighting*
  sub-heading. The checkbox-plus-slider pairs work like *Scene transparency*.
- They travel along two paths, both converted in the root crate's
  `api_common.rs`. That is where the domain preferences already become
  `RenderQuality` and `DisplayPreferences`, so neither the renderer nor the
  display crate learns about `RenderingPreferences`:
  - **To the renderer**, as
    `ShadingSettings { preset, depth_cue_strength, ao_strength }`, built
    by a new `to_shading_settings` next to `to_render_quality` and passed
    to a new `Renderer::set_shading`. That setter is pushed on every
    refresh next to `set_render_quality`, bumps the live generation only
    when the value changed, and treats an "off" checkbox as strength 0.
  - **To the display layer**, only the *Ambient occlusion* checkbox, as
    `ambient_occlusion: bool` on the display crate's
    `AtomicStructureVisualizationPreferences`, set by
    `to_display_preferences`. `update_exposure` and `scene_tessellator`
    both read it from the `DisplayPreferences` they already receive.
- **The *Ambient occlusion* checkbox is the off switch for huge
  structures, and off means zero cost**, not strength 0. With it off:
  - `scene_tessellator` passes `exposure: None`, and every vertex gets
    1.0;
  - `update_exposure` returns at once, so no exposure is computed;
  - no cache is carried over (D9), so the `occlusion_changes` diff is
    skipped as well. The refresh code reads the checkbox from
    `StructureDesigner`'s own preferences.

  The strength slider, by contrast, is renderer-only and changes no
  geometry.
- **Toggling it re-evaluates nothing.** The preference is read after
  evaluation (by `update_exposure` and the tessellator), never during it,
  which is the display crate's invariant. Saving the preferences already
  ends in `refresh_structure_designer_auto`, which re-tessellates. Switching
  AO on fills the caches of the displayed nodes on that refresh. Switching it
  off leaves filled caches unused; they are dropped with their scene entries
  as usual.
- The reference guide row says this outright: on a structure of several
  hundred thousand atoms, AO adds a computation of the order of a tenth of
  materialize each time the structure changes. Unticking the box removes
  it.

**Defaults on.** The point of the design is a better default picture. A
user who dislikes it has the *Headlight* preset and two checkboxes, which
together restore today's look exactly, apart from the rim (D4).

**Exports and thumbnails use the same settings as the viewport.** These are
appearance choices, not quality knobs. The AA design made exports ignore its
preferences because an exported image should always be the *best* picture.
For lighting there is no single best picture, and an export should look
like the screen the user composed it on.

### D9 — Exposure is cached on the scene, per evaluated output, and carried across re-evaluations

Recomputing exposure in the tessellator would pay D7's cost on every
refresh that is not lightweight: an edit to an unrelated node, a node
selection, a property change. Evaluation already avoids repeating work, and
exposure should be paid on the same terms.

**1. Cached with the evaluated output, filled by its own pass.**
- `NodeSceneData` gains `exposure: ExposureCache`, a plain field (no
  interior mutability). It holds one entry per atomic output pin.
- An entry holds three things: the exposure values, the
  `OccluderSnapshot` of the structure they were computed for, and the
  **pending changes** not yet applied to the values.
- The cache is filled by a separate pass, **between evaluation and
  tessellation**: `StructureDesigner::update_exposure(&mut self, prefs:
  &DisplayPreferences, lightweight: bool)`, in a new module
  `atomcad-structure-designer/src/scene_exposure.rs`.
  `refresh_structure_designer` (`api_common.rs`) calls it
  **unconditionally**, right after `structure_designer.refresh(changes)` and
  before `tessellate_scene_content`. The function returns at once on a
  lightweight refresh or with AO off. The gating lives inside it so that
  the structure-designer tests exercise it: the root-crate call site needs
  a GPU-backed `CADInstance`, which no automated harness builds. For each
  atomic output of each displayed node:
  1. `receivers = ao_receivers(structure, viz_prefs)`.
  2. If the entry has a value for every receiver and no pending changes,
     keep it.
  3. Otherwise run `update_atom_exposure` with the pending changes (or
     `atom_exposure` on an empty entry), store the result, clear the
     pending changes and take a fresh snapshot — subject to the drag rule
     (point 3).
- `tessellate_scene_content` stays a **pure function** of the scene, the
  camera and the display preferences. It only reads the entries, and
  passes `Some(&values)` to `tessellate_atomic_structure_impostors` when AO
  is on, `None` when it is off.
- This is the existing pattern, "derive and park on `NodeSceneData`"
  (`surface_color_distribution`, `atomcad-structure-designer/src/AGENTS.md`),
  moved one step later: after evaluation instead of during it. Evaluation
  stays independent of the AO preference, and AO off computes nothing.
- The pass is timed on its own, as `exposure_ms` in the refresh profile
  (`refresh_profile.rs`, next to `tessellate_ms`), so its cost is neither
  hidden in tessellation nor inferred from it.
- So a partial refresh that does not re-evaluate a node reuses its
  exposure. So does a node restored from the invisible-node cache, whose
  `NodeSceneData` keeps its entry.
- A cull-depth change needs no special case. Receivers that gain
  visibility have no value and are computed. Values for atoms that are no
  longer receivers stay correct (D6: a value does not depend on the
  receiver set); they are simply unused until `update_atom_exposure` next
  runs and drops them.
- The entry counts in `NodeSceneData`'s `MemorySizeEstimator`: about 8 bytes
  per receiver for the values and about 24 bytes per atom for the snapshot.

**2. Carried across a re-evaluation.**
- A re-evaluated node gets a fresh `NodeSceneData`. When it replaces an
  older entry under the same `NodeRef`, the old exposure entry (values,
  snapshot, pending changes) moves into the new one, and its pending
  changes become `merge_changes(pending, occlusion_changes(snapshot, new))`.
  The snapshot is **not** replaced here: the values still correspond to it
  plus the pending changes, and only an update (step 3 above) advances it.
- **Pending changes are merged, never replaced.** A drag (point 3)
  re-evaluates on every tick and may skip the update on each one. If a
  tick's carry-over replaced the pending set, the motion of the earlier
  ticks would be forgotten, the closing update would see only the last
  tick, and the values near the earlier positions would stay stale for
  good, with nothing to report it.
- The diff runs against the entry's snapshot, so the outgoing structure is
  not needed. The carry-over is a move of the entry: no structure is kept
  alive for it.
- This happens at the two insertion sites: the end of `refresh_partial`'s
  Step 5, and in `refresh_full`. `refresh_full` **takes the exposure
  entries out of the outgoing scene** (a move per entry, keyed by
  `NodeRef`) before it drops that scene at `fresh_scene()`, then attaches
  them as the new entries are inserted. Keeping the whole outgoing scene
  alive until the inserts are done instead would hold two copies of every
  displayed structure through evaluation — `NodeOutput` owns its
  `AtomicStructure` — which on the nanobeam is hundreds of MB.
  Both sites go through one helper, `carry_over_exposure(old_entry,
  &mut new)`, so neither can forget it.
- It runs only when AO is on and the old entry has values.
- Its cost is the O(atoms) diff: about 20–30 ms per million atoms against a
  full recomputation of 150–400 ms.
- The cases it serves:
  - **Node selection** (the both-nodes re-evaluation above): an empty diff,
    every value reused.
  - **An atom_edit selection or style change**: no position, radius or
    alpha changes, so every value is reused.
  - **Moving, adding or deleting a few atoms**: only the receivers within
    `R_MAX + max r` of the changed atoms are recomputed. That is about a
    hundred receivers per moved atom, through the structure's own grid.
  - **An upstream change that rebuilds the structure** (a new materialize,
    a relax): the diff touches most atoms, and `update_atom_exposure` falls
    back to the full path. This is a computation the user would pay anyway,
    once, beside the re-evaluation that caused it.

**3. Drags.**
- `update_exposure` is a `StructureDesigner` method, so it reads
  `self.open_interaction()` directly. Nothing about drags is threaded into
  the tessellator.
- While an interaction is open, an update that would take the full path
  (more than `FULL_PATH_FRACTION` of the receivers to compute, counted
  over the accumulated pending changes, D7) is **not** run. The previous values are used for every atom that has one,
  new receivers get 1.0, and the entry keeps its pending changes, which
  the next tick's carry-over extends (point 2).
- The first `update_exposure` after the drag ends completes the update over
  the whole accumulated set, so its result equals a full computation on the
  final structure.
- So dragging a large selection costs one full update when the drag ends,
  not one per tick. A small drag (a few atoms, the usual atom_edit case)
  stays exact on every tick through the incremental path.
- The refresh that closes a drag must run `update_exposure` after the
  interaction is closed, i.e. with `open_interaction()` already `None`.
  Phase 4b checks that it does, for the atom_edit drag and the gadget drag.

**Rejected:**
- **Filling the cache lazily inside the tessellator**, behind a `Mutex` on
  `NodeSceneData`, through the `&StructureDesignerScene` it receives. It
  would make `tessellate_scene_content` a hidden writer: the half-dozen
  tests that call it directly would start paying for and mutating exposure,
  its time would land in `tessellate_ms`, and the drag state would have to
  be threaded into a function that has no `StructureDesigner` to ask. An
  explicit `&mut` pass at the one production call site has none of these
  problems.
- **Computing in `generate_scene`, eagerly.** That would make evaluation
  read the AO preference, which turns a checkbox toggle into a
  re-evaluation. It would also compute exposure for outputs that are never
  tessellated.
- **A cache keyed by a hash of the structure.** That costs the same
  O(atoms) as the diff, but can only answer "all or nothing", so moving one
  atom would recompute all of them.
- **Freezing exposure during an atom_edit session.** It is simple, but it
  is wrong on exactly the edits a user is looking at.

### D10 — Directional exposure: a bent vector per atom, visibility evaluated per pixel

**Why.** A scalar per atom darkens buried atoms but leaves each sphere
evenly shaded. The look people call "ambient occlusion" in ChimeraX's soft
lighting, QuteMol or a ray tracer is mostly something else: the dark crease
where two atoms touch, and light falling off across a sphere towards its
neighbours. D7's 128-bit mask already records which directions are
blocked. Reducing it to a popcount throws that information away. Keeping
its first moment costs almost nothing, and it gives the crease.

**What is computed.** From the same mask `M_i` (D7), with `d_k` the Fibonacci
directions:

```text
E_i = |{k ∉ M_i}| / D                 // as before: the open fraction
b_i = (1/D) · Σ_{k ∉ M_i} d_k         // the bent vector: mean open direction, scaled by E
```

- `b_i` points towards the open sky, and its length says how lopsided the
  sky is:
  - isolated atom: `b = 0`
  - fully buried atom: `b = 0`
  - atom on a flat terrace: `b = ¼ · surface normal`
- `|b| ≤ ¼` always: the hemisphere is the extreme case.
- The sum runs over the set bits in index order, so it is deterministic,
  and D7's "incremental is bit-identical to full" still holds.
- It adds one pass over the open directions per receiver. That is ≤ 128
  vector adds, small next to the ~100 neighbour lookups.

**What the shader evaluates.** For the surface normal `N` at the pixel:

```text
V(N) = clamp(E + 2 · dot(b, N), 0, 1)
```

- This is the **cosine-weighted visible fraction** of the hemisphere
  around `N`, computed from the first two spherical-harmonic bands of the
  visibility function. It is the irradiance the ambient term would receive
  from a uniform sky.
- It is **exact for a half-space sky at any tilt**, which is the terrace
  case: a terrace atom gives `V = (1 + cos θ)/2`, where θ is the angle
  between `N` and the terrace normal. So:
  - the top of the atom is fully lit;
  - its equator is at ½;
  - its underside, facing the crystal, is dark.
- For an atom in a groove or beside a step, the side facing the wall
  darkens. Where two spheres touch, both sides of the contact darken
  together, and that makes the crease.
- An isolated atom (`E = 1`, `b = 0`) gets `V = 1` everywhere, so the D3
  calibration is untouched.
- Bonds interpolate `(b, E)` between their two ends and evaluate `V` with
  the bond surface's normal. A bond thus darkens on the side towards the
  bulk.

**Packing.**
- Each atom's value travels as one `Snorm8x4`: `(4b.x, 4b.y, 4b.z, E)`.
  Scaling by 4 uses the full ±1 range, since `|b| ≤ ¼`.
- The quantisation step is 1/127, so `V` is off by at most about 0.02.
  That is invisible after the smoothstep mapping.
- The cost is the 4 bytes a plain `f32` exposure would have taken (D7,
  "Into the vertices").
- The shader unpacks with `V = clamp(E + 0.5 · dot(b4, N), 0, 1)`.

**Rejected:**
- **Uploading the whole 128-bit mask** and looking up the open direction
  nearest to `N`. It is sharper, but it needs 16 bytes per vertex, and on a
  large sphere the 128 directions show as visible facets.
- **More spherical-harmonic bands** (L2, nine coefficients). That is
  9× the data for detail the L1 band already gets right on the terrace,
  the case that matters most.
- **Bending the lighting normal** (using `b` instead of `N` for the
  diffuse term). That changes the lit shape of the sphere, and the D3
  calibration then fails. `b` is used for visibility only.
- **Per-texel visibility maps** (QuteMol). This is the GPU path D6
  rejects.

## Open questions

1. **Occluder radius: covalent or displayed?** D6 chooses covalent, so that
   exposure means burial independently of display style. The cost: a
   ball-and-stick view, where the background is visible between the
   atoms, still darkens buried atoms. Recommendation: keep covalent, look
   at both styles in the walkthrough, and switch `occluder_radius` if the
   ball-and-stick picture reads wrong.
2. **Depth-cue range on close-ups of huge structures** (D5). One option, if
   the walkthrough on the nanobeam finds the cue too weak: clamp the span
   to the part of the content inside the view frustum. That changes with
   zoom. Recommendation: ship the fixed span and decide on evidence.
3. **Does a style rule need "no AO"?** An atom highlighted with
   `apply_style` that sits in a groove will be darkened like its neighbours.
   An `emissive` style field (report §2.4) is the real answer, and it is out
   of scope here. Recommendation: no new style field in this design.

## Testing

The five layers of the anti-aliasing design. Concrete tests are listed in
the phase that builds what they test.

1. **Pure functions, CPU.**
   - `ShadingUniform::new` covers the rig's directions from a camera, the
     presets, the depth cue's `front` and span from bounds and the gadget
     variant.
   - `compose_lit_shader`.
   - `atom_exposure` and the cap-mask table, against the exact cone test as
     oracle.
   - `update_atom_exposure`, against `atom_exposure` as oracle:
     bit-identical by construction (D7), so asserted with `==`.
   - The D9 cache through refreshes, driven by a `StructureDesigner`. Reuse
     is observed, not inferred from timing. The display functions return
     an `ExposureStats` per call (D7), and `StructureDesigner` sums them
     into `last_exposure_stats` (receivers computed, full-path updates,
     plus diffs run by the carry-over), reset at the start of each refresh.
     These are **per-instance**
     statistics, never a process-global counter, because `cargo test` runs
     the tests of one harness in parallel threads, and a static counter
     would count the neighbouring tests' work. "Re-evaluated nothing" is
     asserted the same way, through the refresh's own
     `RefreshSubPhases::eval_ms == None`.
   - The vertex layouts: each `desc()` attribute offset equals the
     `offset_of!` of its field, and the stride equals `size_of` (64 bytes
     for `AtomImpostorVertex`). Without this, a wrong offset for the new
     `Snorm8x4` attribute shows up only as a pipeline-creation panic in
     the GPU harness, which CI does not run.
   - The preferences' serde defaults.
   - The live-generation source check of `live_refinement_test.rs` covers
     `set_shading` automatically.

   **Fixtures.** The display crate cannot build a node network, so its
   tests build lattices with a small helper that writes diamond positions
   straight into an `AtomicStructure` (no bonds needed: exposure reads
   positions and radii only), and pass receiver lists explicitly instead of
   relying on `in_crystal_depth`. Random neighbourhoods use a fixed seed.
   Structure-designer tests set every new preference explicitly, never
   relying on its default: the designer reads the user's
   `preferences.json`, so a default-dependent test passes or fails
   depending on the machine.
2. **Shader structure, naga** (`shader_test.rs`). Each lit shader is
   validated **as composed**. `ShadingUniform`'s offsets are checked as
   `CameraUniform`'s are, and the per-sample entry points still compile
   after composition and the AA D5 cut.
3. **A Rust twin of the shading** (`shading.rs`): `light_surface`, the AO
   mapping (`ao(N)`, `V(N)`), `to_display` and `apply_depth_cue`. It is
   used to tune the constants against the D3 calibration constraint and to
   compute expected pixel values for the GPU tests. A change to
   `lighting.wgsl` is mirrored there, as with `line_geometry.rs` and
   `impostor_coverage.rs`. What catches a twin that has drifted is one
   explicit GPU test, **twin agreement** (Phase 2, extended in Phases 3
   and 4): sampled pixels of a lit sphere match the twin within 2/255.
   Without that test, "measured against the twin" in the other GPU tests
   would compare two things that may have drifted together.
4. **GPU harness** (`renderer_gpu.rs`, `--ignored`), with the AA design's
   rules: dumps first, capability skips, no timing, pixel tolerances and
   counts, never committed golden images.
   - The anchor test of the whole design is that **the *Headlight* preset
     with depth cue and AO off reproduces the pre-design image**.
   - Today `ATOMCAD_GPU_DUMP_DIR` only *writes* images, and the comparison
     between two dump directories is done by hand. Phase 1 adds
     `ATOMCAD_GPU_COMPARE_DIR`: when it is set, `dump()` also reads
     `<name>.raw` from that directory and asserts the largest per-channel
     difference is within a tolerance. The reference stays a local,
     uncommitted dump of the previous commit on the same machine, so the
     "never golden images" rule (drivers differ) still holds. The
     pixel-neutral claims of later phases ("byte-identical to Phase N")
     use the same switch.
   - The existing harness draws too little lit content for that anchor to
     mean much: one opaque quad, one atom and one bond. Phase 1 therefore
     first adds a **lit-pipelines scene**: every lit pipeline (opaque mesh,
     atom and bond impostors, a selected and a frozen atom with rims,
     transparent atom and bond impostors, a transparent isosurface, a
     gadget-pass atom), in perspective and orthographic, with MSAA off and
     on (the per-sample entry points), on a light and a dark background.
     It dumps **one image per pipeline and variant** under its own name, so
     a later phase can exempt one image (an orthographic mesh, a rimmed
     atom) without losing the comparison on the rest. It is committed and
     dumped **before** the refactor.
   - GPU tests that need a particular shading set it with
     `set_shading`; all others run on the renderer's neutral default (D2).
5. **No new Dart logic.** The dialog rows are thin editor UI and are
   checked in the walkthrough.

Each new GPU test is confirmed to **fail with its fix removed** before it is
trusted; each phase names the mutation to try.

**Left to the walkthrough because no harness reaches it:** the one call to
`update_exposure` in `refresh_structure_designer` (`api_common.rs`).
Driving that function needs a `CADInstance` with a live renderer, which
only the Flutter smoke test has. The gating (lightweight, AO off) lives
inside `update_exposure`, so it is covered by the structure-designer
tests; what is left unverified is a single unconditional call.

**Not automated, on purpose** (Phase 5's walkthrough):

- whether the rig's look is better
- the AO and depth-cue constants
- the rim's width at 100 % and 200 %
- ball-and-stick vs space-filling under AO (Open question 1)
- the nanobeam close-up (Open question 2)

## Phases

Each phase leaves the build green and the app usable. Checks per phase:

- `cargo fmt`, `cargo clippy`
- `cargo test -j 4`, naming each harness this design touches, because
  `cargo test --workspace` stops at the first failing harness:
  - `-p atomcad-renderer --test renderer`
  - `-p atomcad-display --test display`
  - `-p atomcad-structure-designer --test structure_designer`
  - root crate: `--test structure_designer_api`, `--test integration`,
    `--test renderer_api`
- the GPU harness:
  `cargo test -p atomcad-renderer --test renderer_gpu -- --ignored`
- `flutter analyze`

The Flutter smoke test is a manual step for the maintainer. A GUI check
needs `cargo build --release` first, because `flutter run` loads the release
library.

### Phase 1 — Shared lighting module and `ShadingUniform` (no visible change)

- **Before any code:**
  - Add the lit-pipelines GPU scene (Testing layer 4) and the
    `ATOMCAD_GPU_COMPARE_DIR` switch in `dump()`, and commit both on their
    own. This commit changes no shader.
  - Then dump the whole GPU harness from that commit
    (`ATOMCAD_GPU_DUMP_DIR`).
- Add `lighting.wgsl`, the `// ---- lighting ----` markers and
  `compose_lit_shader` (D1). Remove the four copies and
  `calculate_bond_lighting`.
- Add `ShadingUniform` at group 0, binding 1, with the two per-pass buffers
  and the four bind groups (D2). It is written in `draw_to_target` from the
  renderer's neutral default `ShadingSettings` (D2); `set_shading` and the
  preferences come in Phase 2. So in this phase it carries only the
  *Headlight* preset: depth cue 0, exposure unused, and the fill and
  hemisphere terms present in the formula but with the preset's values.
- `mesh.wgsl` keeps its old `V` in this phase (a local override marked for
  Phase 2), so the phase is pixel-neutral.
- **Tests:**
  - `shader_test.rs`: every lit shader validates as composed; the
    `ShadingUniform` offsets; per-sample entry points survive composition
    plus the AA D5 cut.
  - `shading_uniform_test.rs` (new, in `tests/renderer/`, registered in
    `tests/renderer.rs`): the *Headlight* preset copies `head_light_dir`
    and has fill 0, sky = ground = 0.2; the gadget variant has depth-cue
    strength 0; size and offsets.
  - `shader_test.rs`: `compose_lit_shader` puts the host header before the
    module, keeps the AA D5 marker after it, and fails loudly on a host
    without the lighting marker.
  - GPU, run with `ATOMCAD_GPU_COMPARE_DIR` pointing at the dump:
    **every harness image, the lit-pipelines scene included, is
    byte-identical.** Float reordering may make this ≤ 1/255 per channel
    instead; if so, record the measured maximum in the phase notes and
    make it the switch's tolerance.
  - Mutation: temporarily swap the key and fill intensities of the
    *Headlight* preset; the comparison must fail.
- Effort: 1.5 d (half a day of it for the lit-pipelines scene and the
  compare switch).
- **Phase notes (2026-10-15).** Implemented as above. Against a dump of the
  pre-refactor build (all 31 harness tests, 194 images, 120 of them the
  lit-pipelines scene, on a DX12 discrete adapter) every image is
  byte-identical: the measured maximum difference is 0, so
  `GPU_COMPARE_TOLERANCE` stays 0. The key/fill swap mutation fails the
  comparison (up to 86 levels). Two details the text above leaves open:
  `mesh.wgsl` keeps its old `V` by calling `light_surface_from(V, …)`, the
  inner half of `light_surface`; and the hemisphere is computed as
  `ground + (sky − ground)·t` rather than `mix`, which is what keeps
  sky = ground exactly constant.

### Phase 2 — The studio rig and the rim after lighting

- Add the `Studio` preset (D3), the shared `camera_view_vector` in
  `mesh.wgsl`, `apply_rim` and the screen-space rim width (D4) in both
  `shade_atom_hit` copies. Add the Rust twin `shading.rs` with
  `light_surface` and `to_display`; Phase 3 adds `apply_depth_cue` to it
  and Phase 4 the AO mapping.
- Add the preferences: `lighting` in `RenderingPreferences`, the API twin,
  and the *Lighting* dropdown in the dialog's Rendering section.
  `Renderer::set_shading` with `ShadingSettings`, built by
  `to_shading_settings` in `api_common.rs` and pushed on every refresh.
- **Tests:**
  - `shading_test.rs` (twin): the D3 calibration constraint for `Studio`
    against `Headlight` (front-facing white within ±5 %); the hemisphere is
    brighter for `N = up` than for `N = −up`; with sky = ground the
    hemisphere is constant.
  - `shading_uniform_test.rs`: Studio key is above-left of the view axis
    and fill below-right, both unit length, and both rotate with the camera
    (orbiting 90° leaves them unchanged in camera space).
  - Preferences: a `preferences.json` without the new field loads as
    `Studio`; the api twin round-trips.
  - GPU:
    - **Twin agreement**: under each preset, pixels sampled at known
      normals of a lit sphere (centre, and four points at 60° from the
      view axis) match `shading.rs` within 2/255, in perspective and
      orthographic. Mutation: change one rig constant in the twin only.
    - A lit sphere under Studio is brighter in its upper-left quadrant than
      in its lower-right.
    - A selected sphere's rim pixels: their count on a 200-px sphere and on
      a 40-px sphere stays within the `RIM_WIDTH_PX` ring area ± tolerance
      (the old NdotV band fails this on the large sphere: mutation).
    - The rim colour is the same on the key side and the fill side within
      2/255 (lighting the rim fails it: mutation).
    - The rim is as wide in output pixels in the exported image (2×
      supersampled) as in the live view, within one pixel, the way the
      line tests check line widths. Dropping `pixel_scale` from `w` fails
      it (mutation).
    - On the renderer's default (*Headlight*), against the Phase 1 dump:
      every image is byte-identical **except** the rimmed-atom images
      (the rim changed, tested above) and the orthographic triangle-mesh
      images. Those change on purpose, because `mesh.wgsl` now builds `V`
      with the shared `camera_view_vector` (D1). For them, a separate
      assertion: an orthographic flat quad is evenly lit across its width
      (the old per-pixel `V` was not).
- Effort: 1–1.5 d.
- **Phase notes (2026-10-15).** Implemented as above; the starting constants
  (key 2.2, fill 0.6, sky 0.26, ground 0.12) meet the calibration constraint
  as they are: a front-facing white surface under *Studio* is 0.984 × its
  *Headlight* luminance, so nothing was tuned. The twin agrees with the GPU
  within 1/255. Details the text above leaves open:
  - The light directions are built by `light_travel_direction(camera,
    elevation, azimuth)` in `shading_uniform.rs`: elevation 35° and azimuth
    30° (left) for the key, −20° and −40° for the fill.
  - The world size of a render pixel for the rim is the mean length of the
    two world-position derivatives (`pixel_world_size`); the band's soft
    inner edge is centred on `w`, so the ring's area is that of a ring `w`
    wide. Measured: 1866 px² against 1857 for a 100-px radius, 355 against
    349 for 20 px.
  - `light_surface_from` is gone: with `mesh.wgsl` on the shared `V` it had
    no caller left.
  - Against the Phase 1 dump the changed images are the rimmed atoms (up to
    152 levels) and three top-down orthographic flat squares
    (`main_pass_order`, `gadget_order`, `aa_mesh_edge`, up to 4 levels). The
    oblique `lit_mesh_ortho_*` and `lit_surface_ortho_*` images did **not**
    change measurably, so they stay asserted; `GPU_COMPARE_EXEMPT` in
    `renderer_gpu.rs` lists exactly the measured set. Every other image is
    byte-identical.
  - Each GPU test's mutation fails it: a twin constant changed alone (twin
    agreement), the old `N·V` band (ring area 20 121 px² against 1857),
    lighting the rim (key/fill colour), dropping `pixel_scale` (exported
    rim 1.5 px against 3), the old eye-position `V` (flat quad).

### Phase 3 — Depth cue

- Add the depth range from `content_bounds` in `ShadingUniform::new`
  (D5), `apply_depth_cue` in the four lit shaders, the gadget variant at
  strength 0, and the *Depth cue* checkbox and strength in preferences.
- **Tests:**
  - `shading_uniform_test.rs`:
    - `front` and `span` from a known AABB and camera, in both projections
    - `front` clamped at the near plane when the camera is inside the
      content
    - strength 0 when there is no content
    - the span does not change when the camera orbits the AABB's centre,
      and `front` changes continuously (small orbit steps, bounded change)
    - a 100 × 100 × 10 Å slab seen face-on: its top face is at f = 0 and its
      bottom face below 0.05 (anchoring at the bounding sphere's front fails
      this: mutation)
    - a cube at several orientations: its nearest corner is at f = 0
  - Preferences: a `preferences.json` without the new fields loads with
    the cue on at strength 0.5; the api twin round-trips.
  - GPU:
    - Two identical spheres at the front and the back of the content: the
      back one is measurably closer to the background colour, on a light
      and on a dark background; the front one equals the cue-off image.
      The same scene goes once through `render_thumbnail`, which draws the
      view twice in a row on the two backgrounds. The kept image must be
      fogged towards *its own* background, which is what writing the
      `ShadingUniform` on every draw (D2) guarantees.
    - Twin agreement, extended: the back sphere's centre pixel matches the
      twin's `apply_depth_cue` within 2/255.
    - A sphere at the far end with strength 1 equals the background within
      2/255 (display-space mixing: a linear-space mix fails, as a mutation).
    - A selected sphere at the far end keeps its rim colour within 2/255 of
      the same sphere at the front (fogging after `apply_rim` fails it:
      mutation).
    - A gadget-pass sphere at the same depth is unfogged (writing the
      content variant for the gadget pass fails it).
    - Depth cue off: byte-identical to Phase 2 (`ATOMCAD_GPU_COMPARE_DIR`).
- Effort: 0.5–1 d.
- **Phase notes (2026-10-15).** Implemented as above. Against a dump of the
  Phase 2 build every one of the 209 harness images is byte-identical, with
  `GPU_COMPARE_EXEMPT` empty; the twin agrees with the GPU on the fogged
  spheres within 0.53/255. Details the text above leaves open:
  - `front` and `span` come from a pure `depth_cue_range(camera, bounds)`
    in `shading_uniform.rs`; the twin's `depth_cue_fraction` /
    `apply_depth_cue` are in `shading.rs`. Both passes carry the same range;
    only the strength tells the gadget pass apart. Without content, or with
    an AABB of no extent, the strength is 0 and the span 1, so the shader's
    division is always defined.
  - `apply_depth_cue` returns the colour untouched when the strength is 0,
    rather than mixing by 0. That is what keeps the cue-off images
    byte-identical whatever the compiler does with `mix`.
  - The preferences are `depth_cue: bool` and `depth_cue_strength: f64`;
    `to_shading_settings` turns an unticked box into strength 0 and clamps
    the strength to [0, 1].
  - "A sphere at the far end with strength 1": since the span is the
    bounding sphere's diameter, content reaches f = 1 only where its depth
    is nearly all of that diameter. The test uses two 0.5 Å spheres 200 Å
    apart along the view axis, whose back sphere is 99.5 % of the way in.
  - The thumbnail test renders a bright and a dark scene, so the thumbnail
    keeps each background once, and compares the back sphere with the twin
    within 3/255 (the thumbnail is box-downsampled).
  - A structural `shader_test.rs` check asserts that every `to_display`
    result in a lit shader goes straight into `apply_depth_cue`, and that
    the rim is mixed into that fogged colour.
  - Each GPU test's mutation fails it: the linear-space mix (converging on
    `to_display(background)`) fails the far-end test, fogging after
    `apply_rim` the rim test, the content variant in the gadget pass the
    gadget test, a fixed background the thumbnail test, and a constant
    changed in the twin alone the twin agreement. On the CPU side, anchoring
    `front` at the bounding sphere's front fails the slab, cube and `front`
    tests.
  - The strength slider applies on every tick, as *Scene transparency*'s
    does: each tick is a full refresh and a preferences write. That is
    cheap on ordinary structures; on a huge one the slider is sluggish.

### Phase 4 — Baked ambient occlusion, the computation

- Add `ambient_occlusion.rs` with `atom_exposure`, `occluder_snapshot`,
  `occlusion_changes`, `merge_changes`, `update_atom_exposure` and the
  cap-mask table (D6, D7). Add the `exposure`
  vertex fields and `…_with_exposure` producers, `ao_receivers`, the
  `exposure: Option<&AtomExposure>` parameter of
  `tessellate_atomic_structure_impostors`, and the shader mapping.
- `scene_tessellator` passes `None` in this phase, so the app shows no AO
  yet. **No uncached path ever reaches the app**: AO becomes visible in 4b,
  together with the cache and the two preferences.
- **Tests** (`atomcad-display/tests/display/ambient_occlusion_test.rs`,
  registered in `tests/display.rs`):
  - An isolated atom has exposure 1. A two-atom pair at 1.54 Å: each is
    blocked by about the cap of one covalent sphere, matching the closed
    form `(1 − cos θ)/2` within quantisation.
  - A lattice-filled diamond block: an interior atom (unculled for the
    test) is below 0.1; a (100) terrace atom lies between an adatom placed
    on it and an atom in a trench cut into it.
  - Atoms left out of `receivers` (culled) still occlude; ghosts (structure
    alpha < 1) do not; `ao_receivers` returns nothing for a diff
    structure. (`atom_exposure` and `occluder_snapshot` take no
    preferences, so *Scene transparency* cannot reach them by
    construction; no test is needed for it.)
  - The table against the exact cone test on 1 000 random neighbourhoods:
    mean absolute error < 0.03, maximum < 0.08.
  - Determinism: the same structure gives bit-identical values across runs
    and thread counts. The test runs `atom_exposure` inside local
    `rayon::ThreadPool`s of 1 and 4 threads (`install`), so the result
    does not depend on the machine's core count, and the `rayon` order
    must not leak into results.
  - Two symmetry-equivalent atoms of a lattice fill get equal values.
  - Bent vector (D10):
    - isolated and fully buried atoms have `b = 0`
    - a (100) terrace atom's `b` is within 10° of the surface normal, with
      length below ¼
    - `|b| ≤ ¼` on 1 000 random neighbourhoods
    - against the closed form: a test-built half-space sky gives
      `V(N) = (1 + cos θ)/2` within quantisation, at five tilts
    - packing round-trips within 1/127
  - Display-side tessellation tests: vertices of opaque atoms carry the
    packed value; gadget, anchor and marker vertices carry `(0, 0, 0, 1)`;
    `exposure: None` leaves every vertex at `(0, 0, 0, 1)`.
  - Renderer CPU tests: the `AtomImpostorVertex` and `BondImpostorVertex`
    `desc()` offsets and strides against `offset_of!`/`size_of`
    (Testing layer 1). The hand-built vertex literal in
    `thumbnail_test.rs` gains the new field.
  - Incremental updates (`ambient_occlusion_test.rs`), each compared with
    `==` against `atom_exposure` on the new structure, with the changes
    taken from `occlusion_changes(occluder_snapshot(old), new)`:
    - one atom moved by 0.5 Å in a lattice-filled block; also one added and
      one deleted
    - an atom's alpha set below 1 (it stops occluding)
    - a changed receiver set (a different cull depth) with no geometry
      change
    - an empty diff, which returns the previous values without computing any
      receiver (`ExposureStats::receivers_computed == 0`)
    - a diff over most atoms, which takes the full path
      (`ExposureStats::full_path`), and one just under `FULL_PATH_FRACTION`,
      which does not
    - three successive moves of the same atoms, with the diffs folded by
      `merge_changes` and a single update at the end (replacing instead of
      merging fails it: mutation)
    - an occluder placed at exactly `R_MAX` from a receiver, and one ulp
      inside and outside it, updated through the structure-grid path: equal
      to the full path in all three cases (the shared `within_r_max`
      predicate, D7)
  - GPU (these set `ao_strength` 1 with `set_shading`, since the renderer
    default is 0):
    - Two spheres with exposure 0 and 1: the occluded one's shaded-side
      pixels are darker by the mapping's factor (twin agreement, extended
      to the AO mapping).
    - `ao_strength` 0 makes them identical.
    - A bond with exposure 0 → 1 brightens monotonically from start to end.
    - A sphere with `E = ½` and `b` pointing to screen-right: its right
      half is brighter than its left, matching the twin. With `b = 0` the
      two halves are symmetric, which is the mutation: drop the `dot(b, N)`
      term and the first assertion must fail.
    - On the renderer's default (`ao_strength` 0) and with every vertex
      fully exposed, all images are byte-identical to Phase 3
      (`ATOMCAD_GPU_COMPARE_DIR`): the new vertex field and the mapping
      change nothing while exposure is 1.
  - **Measure and record** one full `atom_exposure` and the evaluate time
    on a lattice-filled 1 M-atom cuboid and on the 80-cell rod, in a release
    build (D7's absolute budget: at most 15 % of evaluate). The measurement
    is an `#[ignore]`d test in the structure-designer harness, which can
    lattice-fill. It prints both times and asserts nothing: timing is never
    asserted, and the budget is checked by hand and recorded in the phase
    notes.
- Effort: 2.5 d.
- **Phase notes (2026-10-15).** Implemented as above; `scene_tessellator`
  passes `None`, so the app shows no AO yet. Against a dump of the Phase 3
  build every one of the 224 harness images is byte-identical, with
  `GPU_COMPARE_EXEMPT` empty; the twin agrees with the GPU on the AO pair
  within 0.48/255. The cap table's error against the exact cone test is
  0.010 mean, 0.055 at most. Details the text above leaves open:
  - **The 128 directions are antipodal pairs**, a Fibonacci spiral over the
    upper hemisphere and its mirror, not a plain Fibonacci sphere. A plain
    one's directions sum to about 4·10⁻⁴ (per direction), which gave an
    isolated atom a nonzero `b`. With pairs, an isolated and a fully buried
    atom get `b = 0` exactly and a half-space blocks exactly 64 directions.
    With them a (100) terrace atom's `b` is 6.3° from the normal; with the
    plain sphere it was 10.7°, just outside the test's 10°.
  - The 32 half-angle buckets are the values `b·90°/31`, so a fully
    blocking occluder (90°) is represented exactly.
  - The kernel takes no square root and no `asin`: the cube-map cell is
    found from the raw offset (it does not depend on length), and the bucket
    from `r²/d²` against precomputed `sin²` boundaries. A test checks it
    against `cap_mask` on random occluders.
  - Delete and unchanged markers neither occlude nor receive.
    `ao_receivers` leaves them out, and the tessellator writes them fully
    exposed even when the values passed in hold an entry for them.
  - The drop radius around a changed place is `R_MAX` plus *that atom's*
    occluder radius, not the largest radius of any element. When the
    changed places alone outnumber the full-path limit, every value is
    dropped without querying the grid around each place.
  - `AO_FLOOR`, `AO_OPEN` and `AO_KEY` live in `lighting.wgsl` and their
    twins in `shading.rs`, beside `light_surface_exposed` and
    `decode_exposure`. `ao_strength ≤ 0` returns factors of exactly 1, as
    the depth cue does at strength 0.
  - Mutations that fail a test: replacing instead of merging changes (the
    three-move test), a strict `<` in one path's neighbour predicate (the
    `R_MAX` test), dropping `dot(b, N)` (the bent-vector GPU test), a twin
    constant changed alone (AO twin agreement), and a bond reading only its
    start exposure (the bond GPU test).
  - **The budget is missed.** Release build, 16 threads, ball-and-stick:

    | Structure | Evaluate | Receivers | `atom_exposure` | Share |
    |---|---|---|---|---|
    | 1 M-atom cuboid (50³ cells, 1 075 735 atoms) | 805 ms | 309 606 (8 Å cull) | 250 ms | 31 % |
    | same | | 1 075 735 (no cull) | 768 ms | 95 % |
    | 80-cell rod (80 × 7 × 7, 43 505 atoms) | 27 ms | 38 882 | 23 ms | 83 % |

    250 ms is within D7's own estimate (150–250 ms). D7's 15 % share, though,
    was set against the nanobeam's 2 089 ms evaluate, and a plain
    lattice-filled cuboid evaluates in 0.8 s, so the share doubles. The
    first measurement was 389 ms: a hash map from id to index took 114 ms,
    and per-neighbour `sqrt` + `asin` the rest of the excess. Both are gone
    and the values are bit-identical. What remains are D7's levers, `D = 64`
    and `R_MAX = 5 Å`. Both change the picture, so they are left to the
    walkthrough. The rod's 23 ms is negligible in absolute terms. On the rod
    the receivers are most of the atoms, so the share is high whatever the
    budget. (The report's rod had 33 445 atoms; its exact extent is not
    recorded, and 80 × 7 × 7 cells gives 43 505.)
  - **For Phase 4b: the merge mutation may not bite there.** D9 diffs each
    tick against the entry's snapshot, which does not advance during a drag.
    Then every tick's diff already contains the snapshot's positions and the
    current ones. The intermediate positions never fed into any value, so
    *replacing* the pending set gives the same final update as merging it.
    Merging matters only when the diff is taken against a snapshot that
    advances, and that is what this phase's three-move test does. 4b's
    "replace instead of merge" mutation should be checked against its own
    drag test before it is relied on.

### Phase 4b — Exposure cache on the scene (D9)

- Add `ExposureCache` to `NodeSceneData` (`new()`, the `generate_scene`
  literal, `MemorySizeEstimator`, and the direct-construction sites in
  `multi_output_unit_test.rs`). Add `carry_over_exposure` at the
  insertion sites of `refresh_partial` and `refresh_full`; in
  `refresh_full`, take the exposure entries out of the outgoing scene
  before `fresh_scene()` (D9). Add `scene_exposure.rs` with
  `StructureDesigner::update_exposure` and `last_exposure_stats`, call it
  unconditionally from `refresh_structure_designer` between `refresh` and
  `tessellate_scene_content`, and have `scene_tessellator` read the
  entries. Add `exposure_ms` to the refresh profile (the domain struct, its
  API twin and the profiling panel row).
- Add the two AO preferences (D8): the domain fields, the API twin, the
  dialog rows and `ambient_occlusion` on the display crate's
  `AtomicStructureVisualizationPreferences`, set in
  `to_display_preferences`.
- **Tests** (`atomcad-structure-designer/tests/structure_designer/ambient_occlusion_cache_test.rs`),
  on a small lattice-filled network. Each test sets the AO preference
  explicitly and drives `refresh` followed by `update_exposure`, the
  sequence `refresh_structure_designer` runs. "Computes nothing" and
  "computes only…" are read from `last_exposure_stats`; "without
  re-evaluating" is `RefreshSubPhases::eval_ms == None` (Testing layer 1).
  - A lightweight refresh (`update_exposure(…, lightweight: true)`)
    computes nothing: the gating inside `update_exposure`.
  - A second refresh after editing an unrelated node computes nothing.
  - Selecting another node (both re-evaluated) computes nothing.
  - An atom_edit selection change computes nothing; moving one atom
    computes only receivers within `R_MAX + max r` of it.
  - A changed cull depth computes only the newly unculled receivers.
  - AO off: nothing computed, nothing carried over, and the
    `occlusion_changes` diff is never run. Switching it back on through
    `set_preferences` computes, and the refresh that follows re-evaluates
    nothing (`eval_ms == None`): changing `RenderingPreferences` must not
    mark a full refresh.
  - Hide → show restores the node from the invisible cache with its
    exposure: nothing computed.
  - A large drag (moving more than `FULL_PATH_FRACTION` of the atoms) of
    several ticks with an interaction open computes nothing and
    accumulates pending changes; the drag-end refresh
    completes them, and the cache then equals `atom_exposure` on the final
    structure (`==`). This is checked for an atom_edit drag and a gadget
    drag, through the real drag-end entry points, which is what proves the
    closing refresh runs `update_exposure` with the interaction closed.
  - `tessellate_scene_content` with AO on, on a scene whose entries are
    empty (no `update_exposure` run), writes `(0, 0, 0, 1)` on every
    vertex: the tessellator only reads the cache and treats an absent
    value as fully exposed.
  - Preferences: a `preferences.json` without the AO fields loads with AO
    on at strength 1.0; the api twin round-trips.
  - A full refresh (e.g. an undo that triggers `RefreshMode::Full`) with an
    empty diff computes nothing: the entries were carried out of the
    outgoing scene.
  - Mutations: drop `carry_over_exposure` from `refresh_partial` (the
    node-selection test must fail); replace the pending set instead of
    merging it (the multi-tick drag test must fail).
- **Measure and record** the refresh time of an unrelated-node edit on the
  1 M-atom cuboid with AO on and off. The two should agree within noise,
  and `exposure_ms` should be the cost of the O(atoms) diff alone.
- Effort: 1 d.
- **Phase notes (2026-10-16).** Implemented as above, with two deviations
  forced by one finding, and the reference guide and `AGENTS.md` updates of
  Phase 5 made here already (the repository's rule is that a user-visible
  change updates the guide in the same change).
  - **`apply_diff` renumbers atoms, so the diff matches by place.** Moving one
    atom of a 1 810-atom `atom_edit` block gave 699 unmoved atoms another id.
    With D9's id-keyed diff every atom_edit edit — the main case D9 exists
    for — reported most of the structure as changed and silently took the
    full path. The carry-over now uses `occlusion_changes_by_place` (display
    crate): atoms that kept their id and state are matched first, the rest by
    the exact bits of position, occluder radius and occluder flag, which is
    all an exposure depends on. It returns an `AtomRenumbering`, and
    `AtomExposure::remapped` re-keys the values through it. The entry
    keeps its values keyed by the snapshot's ids and holds the re-keyed copy
    the tessellator reads. Exactness is unchanged: a matched receiver whose
    neighbourhood holds no changed place sees the same occluder spheres, so
    its value is bit-identical, and the tests still compare with `==`.
  - **Pending changes are replaced, not merged.** As Phase 4's notes
    predicted, merging is not load-bearing here: every diff starts from the
    entry's snapshot, which only an update advances, so the newest diff
    already holds every change since it. Merging would only grow the pending
    set by the moved atoms' positions on every deferred drag tick (about
    32 bytes per place, unbounded over a long drag of a large selection)
    without changing which values are dropped. `carry_over_exposure`
    therefore assigns the diff, and the "replace instead of merge" mutation
    was not run; the mutation that does bite the drag test is *advancing the
    snapshot on a deferred tick*. `merge_changes` stays in the display crate
    for its own test.
  - Details the text above leaves open:
    - AO is computed only when it is on **and** atoms are drawn as impostors
      (`AtomicStructureVisualizationPreferences::ambient_occlusion_active`,
      and `StructureDesigner::ambient_occlusion_active` on the domain side):
      the triangle-mesh drawing does not read exposure.
    - `exposure_ms` is `None`, not 0, when the pass returned at once
      (lightweight, AO off); the profiler panel's **AO** column shows `—`
      there. It is a field of `RefreshSubPhases`, filled by the API layer.
    - An entry with no receivers (a diff structure, or every atom a ghost) is
      left empty rather than paying for a snapshot; a first computation is a
      full one and is deferred during an interaction like any full-path
      update.
    - "Re-evaluated nothing" cannot be `eval_ms == None`: a partial refresh
      that finds no work reports `Some(0.0)` (`refresh_profile.rs`, D6 of the
      profiling design). The tests arm the per-node profiler and assert that
      it produced no table.
    - Gadget-drag ticks are lightweight, so they compute nothing whatever the
      size of the drag; the deferral matters for the atom_edit drag, whose
      ticks re-evaluate. The atom drag test goes through
      `default_tool_pointer_down/move/up`, the gadget test through
      `gadget_start_drag / gadget_drag / gadget_end_drag` on `structure_move`.
  - Mutations that fail a test: dropping `carry_over_exposure` from
    `refresh_partial` (four tests, the node-selection one among them) and from
    `refresh_full` (the full-refresh test), diffing by id instead of by place
    (the one-atom move), and advancing the snapshot on a deferred tick (the
    atom drag).
    - An entry remembers the display preferences it was last brought up to
      date for; while they are unchanged and it was not carried over, the pass
      skips it without running `ao_receivers`. Without this the pass scanned
      every atom on every refresh: 10 ms on the 1 M-atom cuboid for an
      unrelated edit, against 0.1 ms with AO off.
    - The carry-over's time is reported in `exposure_ms`, not `eval_ms`
      (`refresh_partial` subtracts it from its evaluation bracket), so the
      profiler's **AO** column carries all of AO's cost.
  - **Measured** (`ambient_occlusion_cache_timing`, release, 16 threads, the
    1 M-atom cuboid of Phase 4 behind `structure_move`, 8 Å cull, 309 606
    receivers, an unrelated displayed `cuboid`):

    | Refresh | AO on | AO off |
    |---|---|---|
    | First fill (full refresh) | 1 824 ms, of it exposure 417 ms | 1 416 ms |
    | Unrelated-node edit | 0.0–0.1 ms, exposure 0.0 ms | 0.1 ms |
    | Selecting the structure (re-evaluates it; empty diff) | 1 542 ms, of it exposure 122 ms | 1 491 ms |

    - The unrelated edit agrees with AO off within noise, as required.
    - `exposure_ms` on a re-evaluation is not D9's "20–30 ms per million
      atoms": the carry-over (an O(atoms) id-first diff plus the hash lookups
      of the snapshot) and `ao_receivers` together take about 120 ms. The
      first version, which matched every atom by place, took about 900 ms;
      matching by id first is what brought it down.
    - The first fill costs 417 ms against Phase 4's 250 ms for
      `atom_exposure` alone: the rest is the 1 M-entry `OccluderSnapshot`
      and `ao_receivers`.

### Phase 5 — Walkthrough and documentation

- **Walkthrough** (maintainer):
  - Studio against Headlight on a CSG cube, a (100) 2×1 reconstruction, the
    T-centre scene, the 80-cell rod (perspective and orthographic) and a
    mechanosynthesis step.
  - The depth cue on the rod and the nanobeam.
  - AO in both display styles. Toggling *Ambient occlusion* in the
    Preferences dialog changes the picture at once, and the profiling
    panel shows `exposure` time only when the structure changed. This is
    the check on the `update_exposure` call in `api_common.rs`, which no
    automated test reaches.
  - The rim of selected, frozen and marked atoms at 100 % and 200 %.
  - An exported image and a thumbnail on a light background.
  - The crease between touching atoms on a (100) 2×1 reconstruction and
    at a step, space-filling, with `AO_KEY` at 0 and at its start value.
  - Tune `AO_FLOOR`, `AO_OPEN`, `AO_KEY`, the rig constants and the default
    strengths; settle Open questions 1 and 2.
- **Docs:**
  - `doc/reference_guide/ui.md`: the five rows of the Rendering section
    (what each does, the default, and that exports follow them). The
    *Ambient occlusion* row says what it costs on a large structure, when
    that cost is paid (when the structure changes, not on every edit), and
    that unticking it removes the cost entirely (D8). The profiling panel's
    description gains the new *exposure* column (D9).
  - `atomcad-structure-designer/src/AGENTS.md`, beside the
    `NodeSceneData` derived-fields paragraph: the exposure cache is filled
    by `update_exposure` between refresh and tessellation, never by the
    tessellator, which only reads it; and a new place that inserts a
    `NodeSceneData` over an older one goes through `carry_over_exposure`
    (pending changes are merged, never replaced).
  - `rust/AGENTS.md`, renderer bullets:
    - lit shaders are composed with `lighting.wgsl` at the
      `// ---- lighting ----` marker, and a change there is mirrored in
      `shading.rs`
    - `ShadingUniform` is per pass and written on every draw, so it is
      exempt from the per-view pairs rule
  - `atomcad-display/src/AGENTS.md`: the `ambient_occlusion` module, and
    the rule that occluders include culled atoms but not ghosts, judged by
    the structure's own alpha, never the preference-scaled display alpha.
- Effort: 0.5 d.

**Total: about 7–8 d.** That is the report's 3–5 d "Depth" bundle, plus the
shader unification, the rim fix, the exposure cache and the GPU comparison
scaffolding, none of which the report costed.

## Future work

All of these build on this design without changing it:

- **Shadow maps** (report §2.3). D3's key light is the light the report's
  shadow design assumes: camera-relative, upper side, with the ambient term
  separate. A shadow factor multiplies the key term in `light_surface`. The
  casters and the fit are as the report describes. **Done:** see
  `doc/design_shadows.md` (cascaded shadow maps for the Studio key light,
  which also lowers D7's `AO_KEY` while shadows are drawn).
- **Cross-structure exposure.** Gather all displayed structures in
  `update_exposure` and run `atom_exposure` over their union, so a tool
  tip darkens the surface it approaches. The pass already sees the whole
  scene, which is one more reason it is not part of the per-structure
  tessellator.
- **SSAO** for isosurfaces and arbitrary molecules, on top of the
  post-process chain. Per-atom exposure stays as the cheap default.
- **Emissive style field** (report §2.4). An emissive term added after the
  exposure factor and before the depth cue, so a highlighted atom stays
  bright in a groove and at the back of the scene.
- **Fog on content lines** (wireframes, cages), by declaring the
  `ShadingUniform` in `line_mesh.wgsl` with a per-mesh opt-in.
- **Removing `head_light_dir` from `CameraUniform`** once nothing reads it
  except the *Headlight* preset, which could compute it in
  `ShadingUniform::new` instead.
