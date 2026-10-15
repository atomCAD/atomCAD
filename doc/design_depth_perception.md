# Depth Perception: Lighting Rig, Depth Cue and Baked Ambient Occlusion

Source: `reports/rendering_capabilities/rendering_capabilities_report_2026-09-21.md`
§2.3 (depth perception) and the rim-band item of §2.4. Builds on
`doc/design_screen_space_lines.md` and `doc/design_viewport_antialiasing.md`.
It relies on the per-sample impostor shading (D4/D5) and the per-view uniform
rule of the anti-aliasing design, and it changes neither.

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
  - per-atom exposure for impostor atoms and bonds (D6, D7)
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
compiling it: `without_per_sample_shading` cuts the impostor files at the D5
marker. So this design adds one source composition step: a new file,
`lighting.wgsl`, is **prepended** to `atom_impostor.wgsl`,
`bond_impostor.wgsl`, `transparent_impostor.wgsl` and `mesh.wgsl` before
the module is created.

`lighting.wgsl` holds:

- the `ShadingUniform` struct and its binding (D2)
- the four PBR helpers
- `light_surface(world_position, N, albedo, roughness, metallic, exposure) -> vec3`,
  which returns linear colour
- `to_display(linear) -> vec3`, which applies Reinhard and gamma, unchanged
- `apply_rim(color, rim_color, rim_weight) -> vec3` (D4)
- `apply_depth_cue(color, world_position) -> vec3` (D5)

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
in each host file. This is the same mechanism as the D5 marker, applied in
the other direction.

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
  to the fragment stage. Every pipeline's group-0 layout gains the entry.
  `line_mesh.wgsl` and `label.wgsl` do not declare it, which wgpu allows.
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

`head_light_dir` stays in `CameraUniform`. It is the *Headlight* preset's
key direction (D3), and `ShadingUniform::new` copies it from there, so
the `CameraUniform` layout does not change.

`ShadingUniform` gets the same offset table and `offset_of!` checks as
`CameraUniform`, in `shader_test.rs` and a new `shading_uniform_test.rs`.

### D3 — A camera-relative studio rig, with the old headlight as a preset

```text
radiance = key  · (diffuse + specular) · (N·L_key)⁺
         + fill · diffuse · (N·L_fill)⁺           · exposure
         + mix(ground, sky, ½ + ½ N·up_cam) · albedo · exposure
```

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
- **Exposure** (D6) multiplies the fill and the ambient but **not** the key.
  Ambient occlusion stands in for missing indirect light, and the fill is a
  stand-in for indirect light too. The key is direct light: darkening it by
  burial would look like a shadow without a shadow's direction.

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

- **After lighting.** The rim colour is mixed into the display-space output
  (`apply_rim`, after `to_display`), not into the albedo. Selection orange,
  frozen cyan and marked magenta then read the same all around the outline
  and under either preset. They are UI signals, and a UI signal should not
  be lit. Delete markers keep working: their red signal now comes from an
  unlit ring around a dark sphere, which is if anything clearer.
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

  The early-out structure of the D4 coverage block is unchanged, because the
  computation runs only on fragments that are already shaded.
- Both `shade_atom_hit` copies change, the opaque one and the transparent
  one. Bonds have no rim.

This is the report's "rim band tuning" item, with one change. The report
asked only for the width. Applying the rim after lighting is new, and the
rig change makes it necessary: under a side key light, the far side of a
lit rim would fall into shade.

### D5 — Depth cue: fog to the background colour across the scene's bounding sphere

```text
depth   = view-space distance along the view axis (both projections)
front   = max(c − R, near)      c = view depth of the bounding sphere's centre
back    = c + R                 R = its radius (half the content AABB's diagonal)
f       = strength · clamp((depth − front) / (back − front), 0, 1)
out.rgb = mix(out.rgb, background, f)   (after to_display and apply_rim)
```

- **The range is the content's bounding sphere, not the near and far
  planes.** The far plane is 2400 Å, and a cue spread over that range would
  be invisible on any real structure. The sphere is used rather than the
  AABB's projected depth range because the sphere's range does not change
  as the camera orbits the content's centre. With the AABB range, an atom's
  fog would "breathe" during a rotation. This is the same reason the report
  fits the shadow map to a sphere.
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
structure, such as the million-atom nanobeam, the bounding sphere is
hundreds of Å deep and the visible part spans a small slice of it, so the
cue is weak there.

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
ambient occlusion. It is a single scalar per atom, so it cannot darken one
side of a sphere and not the other. What it does convey is the thing the
report says the user needs to read: which atoms are exposed and which are
buried. The report's estimate is 70 % of the AO look at a small fraction of
the cost.

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

- **Occluders:** every atom of the structure with display alpha 1, **culled
  atoms included**. Culling hides interior atoms only to save rendering
  time. Those atoms still exist, and leaving them out would make the
  surface atoms above them look open from below. Ghosts (alpha < 1) do not
  occlude: a ghosted lattice around a highlighted dopant should leave the
  dopant looking lit, and here it does.
- **Receivers:** opaque, unculled atoms. These are the only ones whose value
  is used, so the computation iterates over them alone. Culled interior
  atoms, typically most of a lattice-filled crystal, cost nothing as
  receivers.
- **An atom's exposure depends on the geometry alone**: the positions,
  occluder radii and alphas of the atoms within `R_MAX`. It does not depend
  on which other atoms are receivers, on the display style or on any
  preference. D9 relies on this. A value computed once stays valid while
  nothing within `R_MAX` of the atom changes, even if the cull depths change
  the set of receivers.
- **Exposure 1:**
  - ghosts (the transparent mesh, whose vertex keeps its layout)
  - delete and unchanged markers
  - anchor spheres
  - every atom of a diff structure (`is_diff()`; these are edit overlays,
    not matter)
  - gadget impostors

**Scope of the neighbourhood: one `AtomicStructure`.** The tessellator is
called once per displayed structure (`scene_tessellator.rs:279`), so a tool
does not darken the surface beneath it when the two are separate displayed
outputs. Occlusion across structures needs the scene tessellator to gather
every displayed structure first. That is Future work, and it is the natural
companion of shadows.

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
/// Exposure in [0, 1] of each atom in `receivers`, keyed by atom id;
/// absent ids are fully exposed. Builds one `SpatialGrid` over the
/// occluders; runs the receivers under `rayon`.
pub fn atom_exposure(structure: &AtomicStructure, receivers: &[u32]) -> AtomExposure;

/// The atoms whose occluder state differs between `old` and `new`
/// (position, occluder radius, occluder or not, added, removed), with
/// their old and new positions. O(atoms).
pub fn occlusion_changes(old: &AtomicStructure, new: &AtomicStructure) -> OcclusionChanges;

/// `previous` brought up to date for `structure`. Drops the values within
/// `R_MAX + max occluder radius` of a changed position, old or new, then
/// computes every receiver that has no value. Few missing receivers are
/// computed through `AtomicStructure`'s own grid, many through
/// `atom_exposure`'s path.
pub fn update_atom_exposure(
    previous: &AtomExposure,
    changes: &OcclusionChanges,
    structure: &AtomicStructure,
    receivers: &[u32],
) -> AtomExposure;
```

`AtomExposure` is an `FxHashMap<u32, f32>` wrapped in a newtype. Both
paths compute each receiver with the same kernel over the same neighbour
set. Since the mask is an OR, neighbour order does not matter. **So an
incremental update is bit-identical to a full computation**, and Phase 4
tests exactly that.

The tessellator does not call these functions directly: it takes the
exposure from the scene's cache (D9). `tessellate_atomic_structure_impostors`
gains an `exposure: Option<&AtomExposure>` parameter, where `None` means AO
is off and every vertex gets 1.0. It also exposes the receiver selection it
already does (`should_cull_atom`, alpha) as `ao_receivers(structure,
viz_prefs) -> Vec<u32>`. The cache uses that function, so the two can never
disagree.

**Into the vertices.**

- `AtomImpostorVertex` gains `exposure: f32`, growing from 60 to 64 bytes.
- `BondImpostorVertex` gains `exposure_start, exposure_end`. The bond shader
  mixes the two by the hit point's axial parameter, which `shade_bond_hit`
  already computes as `projection_length`, so a bond darkens towards its
  buried end.
- `add_atom_quad` and `add_bond_quad` keep their signatures and write 1.0.
  New `…_with_exposure` variants serve the tessellator. Every other
  producer, including gadgets, anchors and markers, is unchanged and comes
  out fully exposed.
- The memory cost on the nanobeam is 16 MB more vertex data, which instanced
  bond and atom quads (AA design, Future work) would recover several times
  over.

**From exposure to light**, in `light_surface`:

```text
ambient_factor = mix(1, AO_FLOOR + (1 − AO_FLOOR) · smoothstep(0, AO_OPEN, E), ao_strength)
```

Starting values: `AO_FLOOR = 0.25`, `AO_OPEN = 0.6`. With these, an adatom
and anything more open than it is unshaded, a terrace atom is mildly
darkened and a groove clearly so. The constants are shader constants, tuned
in the walkthrough. `ao_strength` comes from the `ShadingUniform`, so moving
the strength slider changes no geometry.

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
- The renderer receives them as
  `ShadingSettings { preset, depth_cue_strength, ao_strength }`, through a
  new `Renderer::set_shading`. That setter is pushed on every refresh next
  to `set_render_quality`, bumps the live generation only when the value
  changed, and treats an "off" checkbox as strength 0.
- **The *Ambient occlusion* checkbox is the off switch for huge
  structures, and off means zero cost**, not strength 0. With it off:
  - `scene_tessellator` passes `exposure: None`, and every vertex gets
    1.0;
  - no exposure is computed;
  - no cache is filled or carried over (D9), so the `occlusion_changes`
    diff is skipped as well.

  The strength slider, by contrast, is renderer-only and changes no
  geometry.
- **Toggling it re-evaluates nothing.** The preference is read at
  tessellation time, never during evaluation, which is the display crate's
  invariant. Saving the preferences already ends in
  `refresh_structure_designer_auto`, which re-tessellates. Switching AO on
  fills the caches of the displayed nodes on that tessellation. Switching it
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

**1. Cached with the evaluated output.**
- `NodeSceneData` gains `exposure: ExposureCache`. It holds one entry per
  atomic output pin, behind a `Mutex`, so the tessellator can fill it
  through the `&StructureDesignerScene` it already receives.
- This follows the existing pattern, "derive at `generate_scene` time and
  park on `NodeSceneData`" (`surface_color_distribution`,
  `atomcad-structure-designer/src/AGENTS.md`), with one difference: it is
  filled **lazily**, on the first tessellation that wants it. Evaluation
  stays independent of the AO preference, and AO off computes nothing.
- On a tessellation with AO on, for each atomic structure:
  1. `receivers = ao_receivers(structure, viz_prefs)`.
  2. If the entry has a value for every receiver and no pending changes,
     use it.
  3. Otherwise run `update_atom_exposure` (or `atom_exposure` on an empty
     entry) and store the result.
- So a partial refresh that does not re-evaluate a node reuses its
  exposure. So does a node restored from the invisible-node cache, whose
  `NodeSceneData` keeps its entry.
- A cull-depth change needs no special case. Receivers that gain
  visibility have no value and are computed; values for atoms that are no
  longer receivers are pruned (D6: a value does not depend on the receiver
  set).
- The entry counts in `NodeSceneData`'s `MemorySizeEstimator`, about 8 bytes
  per receiver.

**2. Carried across a re-evaluation.**
- A re-evaluated node gets a fresh `NodeSceneData`. When it replaces an
  older entry under the same `NodeRef`, the old exposure entry moves into
  the new one, together with `occlusion_changes(old, new)`.
- This happens at the two insertion sites: the end of `refresh_partial`'s
  Step 5, and in `refresh_full`, which keeps the outgoing scene until its
  inserts are done instead of dropping it at `fresh_scene()`. Both go
  through one helper, `carry_over_exposure(old, &mut new)`, so neither site
  can forget it.
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
- While `StructureDesigner::open_interaction()` reports a drag, the
  tessellator gets `interaction_open = true`, passed from
  `refresh_structure_designer` the way `lightweight` is.
- Under it, an update that would take the full path (more than a quarter of
  the receivers to compute) is **not** run. The previous values are used for
  every atom that has one, new receivers get 1.0, and the entry keeps its
  pending changes.
- The first tessellation after the drag ends completes the update.
- So dragging a large selection costs one full update when the drag ends,
  not one per tick. A small drag (a few atoms, the usual atom_edit case)
  stays exact on every tick through the incremental path.
- The refresh that closes a drag must reach the tessellator with
  `interaction_open = false`. Phase 4b checks that it does, for the atom_edit
  drag and the gadget drag.

**Rejected:**
- **Computing in `generate_scene`, eagerly.** That would make evaluation
  read the AO preference, which turns a checkbox toggle into a
  re-evaluation. It would also compute exposure for outputs that are never
  tessellated.
- **A cache keyed by a hash of the structure.** That costs the same
  O(atoms) as the diff, but can only answer "all or nothing", so moving one
  atom would recompute all of them.
- **Freezing exposure during an atom_edit session.** It is simple, but it
  is wrong on exactly the edits a user is looking at.
## Open questions

1. **Occluder radius: covalent or displayed?** D6 chooses covalent, so that
   exposure means burial independently of display style. The cost: a
   ball-and-stick view, where the background is visible between the
   atoms, still darkens buried atoms. Recommendation: keep covalent, look
   at both styles in the walkthrough, and switch `occluder_radius` if the
   ball-and-stick picture reads wrong.
2. **Depth-cue range on close-ups of huge structures** (D5). One option, if
   the walkthrough on the nanobeam finds the cue too weak: clamp the range
   to the part of the bounding sphere inside the view frustum. That is
   still rotation-stable around the target, but it changes with zoom.
   Recommendation: ship the sphere and decide on evidence.
3. **Does a style rule need "no AO"?** An atom highlighted with
   `apply_style` that sits in a groove will be darkened like its neighbours.
   An `emissive` style field (report §2.4) is the real answer, and it is out
   of scope here. Recommendation: no new style field in this design.

## Testing

The five layers of the anti-aliasing design. Concrete tests are listed in
the phase that builds what they test.

1. **Pure functions, CPU.**
   - `ShadingUniform::new` covers the rig's directions from a camera, the
     presets, the depth range from bounds and the gadget variant.
   - `compose_lit_shader`.
   - `atom_exposure` and the cap-mask table, against the exact cone test as
     oracle.
   - `update_atom_exposure`, against `atom_exposure` as oracle:
     bit-identical by construction (D7), so asserted with `==`.
   - The D9 cache through refreshes, driven by a `StructureDesigner` with a
     counter of exposure computations, so that a reuse is observed rather
     than inferred from timing.
   - The preferences' serde defaults.
   - The live-generation source check of `live_refinement_test.rs` covers
     `set_shading` automatically.
2. **Shader structure, naga** (`shader_test.rs`). Each lit shader is
   validated **as composed**. `ShadingUniform`'s offsets are checked as
   `CameraUniform`'s are, and the per-sample entry points still compile
   after composition and the D5 cut.
3. **A Rust twin of `light_surface`** (`shading.rs`), used to tune the
   constants against the D3 calibration constraint and to compute expected
   pixel values for the GPU tests. A change to `lighting.wgsl` is mirrored
   there, as with `line_geometry.rs` and `impostor_coverage.rs`. The GPU
   harness is what catches a twin that has drifted.
4. **GPU harness** (`renderer_gpu.rs`, `--ignored`), with the AA design's
   rules: dumps first, capability skips, no timing, pixel tolerances and
   counts, never golden images.
   - The anchor test of the whole design is that **the *Headlight* preset
     with depth cue and AO off reproduces the pre-design image**, compared
     against the dump taken before Phase 1.
5. **No new Dart logic.** The dialog rows are thin editor UI and are
   checked in the walkthrough.

Each new GPU test is confirmed to **fail with its fix removed** before it is
trusted; each phase names the mutation to try.

**Not automated, on purpose** (Phase 5's walkthrough):

- whether the rig's look is better
- the AO and depth-cue constants
- the rim's width at 100 % and 200 %
- ball-and-stick vs space-filling under AO (Open question 1)
- the nanobeam close-up (Open question 2)

## Phases

Each phase leaves the build green and the app usable. Checks per phase:

- `cargo fmt`, `cargo clippy`, `cargo test -j 4` (the renderer and display
  harnesses and the api harnesses explicitly)
- the GPU harness
- `flutter analyze`

The Flutter smoke test is a manual step for the maintainer. A GUI check
needs `cargo build --release` first, because `flutter run` loads the release
library.

### Phase 1 — Shared lighting module and `ShadingUniform` (no visible change)

- **Before any code:** dump the GPU harness (`ATOMCAD_GPU_DUMP_DIR`) from
  the current commit.
- Add `lighting.wgsl`, the `// ---- lighting ----` markers and
  `compose_lit_shader` (D1). Remove the four copies and
  `calculate_bond_lighting`.
- Add `ShadingUniform` at group 0, binding 1, with the two per-pass buffers
  and the four bind groups (D2). It is written in `draw_to_target`. In this
  phase it carries only the *Headlight* preset: depth cue 0, exposure
  unused, and the fill and hemisphere terms present in the formula but with
  the preset's values.
- `mesh.wgsl` keeps its old `V` in this phase (a local override marked for
  Phase 2), so the phase is pixel-neutral.
- **Tests:**
  - `shader_test.rs`: every lit shader validates as composed; the
    `ShadingUniform` offsets; per-sample entry points survive composition
    plus the D5 cut.
  - `shading_uniform_test.rs`: the *Headlight* preset copies
    `head_light_dir` and has fill 0, sky = ground = 0.2; the gadget variant
    has depth-cue strength 0; size and offsets.
  - `pipelines_test.rs` (or the existing shader test file):
    `compose_lit_shader` puts the host header before the module and fails
    loudly on a host without the marker.
  - GPU: **every existing harness image is byte-identical to the dump.**
    Float reordering may make this ≤ 1/255 per channel instead; if so,
    record the measured maximum in the phase notes and assert it.
  - Mutation: swap fill and key in the preset; the comparison must fail.
- Effort: 1 d.

### Phase 2 — The studio rig and the rim after lighting

- Add the `Studio` preset (D3), the shared `camera_view_vector` in
  `mesh.wgsl`, `apply_rim` and the screen-space rim width (D4) in both
  `shade_atom_hit` copies. Add the Rust twin `shading.rs`.
- Add the preferences: `lighting` in `RenderingPreferences`, the API twin,
  and the *Lighting* dropdown in the dialog's Rendering section.
  `Renderer::set_shading` with `ShadingSettings`, pushed on every refresh.
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
    - A lit sphere under Studio is brighter in its upper-left quadrant than
      in its lower-right, measured against the twin.
    - A selected sphere's rim pixels: their count on a 200-px sphere and on
      a 40-px sphere stays within the `RIM_WIDTH_PX` ring area ± tolerance
      (the old NdotV band fails this on the large sphere: mutation).
    - The rim colour is the same on the key side and the fill side within
      2/255 (lighting the rim fails it: mutation).
    - Under *Headlight*, unselected content is still byte-identical to the
      Phase 1 dump.
- Effort: 1–1.5 d.

### Phase 3 — Depth cue

- Add the depth range from `content_bounds` in `ShadingUniform::new`
  (D5), `apply_depth_cue` in the four lit shaders, the gadget variant at
  strength 0, and the *Depth cue* checkbox and strength in preferences.
- **Tests:**
  - `shading_uniform_test.rs`:
    - `front`/`back` from a known sphere and camera, in both projections
    - `front` clamped at the near plane when the camera is inside the
      sphere
    - strength 0 when there is no content
    - the range does not change when the camera orbits the sphere's centre
  - GPU:
    - Two identical spheres at the front and the back of the content: the
      back one is measurably closer to the background colour, on a light
      and on a dark background.
    - A sphere at the far end with strength 1 equals the background within
      2/255 (display-space mixing: a linear-space mix fails, as a mutation).
    - A gadget-pass sphere at the same depth is unfogged (writing the
      content variant for the gadget pass fails it).
    - Depth cue off: byte-identical to Phase 2.
- Effort: 0.5–1 d.

### Phase 4 — Baked ambient occlusion, the computation

- Add `ambient_occlusion.rs` with `atom_exposure`, `occlusion_changes`,
  `update_atom_exposure` and the cap-mask table (D6, D7). Add the `exposure`
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
  - Culled atoms occlude; ghosts (alpha < 1) do not; atoms of a diff
    structure are 1.
  - The table against the exact cone test on 1 000 random neighbourhoods:
    mean absolute error < 0.03, maximum < 0.08.
  - Determinism: the same structure gives bit-identical values across runs
    and thread counts (the `rayon` order must not leak into results).
  - Two symmetry-equivalent atoms of a lattice fill get equal values.
  - Display-side tessellation tests: vertices of opaque atoms carry the
    value; gadget, anchor and marker vertices carry 1.0; `exposure: None`
    leaves every vertex at 1.0.
  - Incremental updates (`ambient_occlusion_test.rs`), each compared with
    `==` against `atom_exposure` on the new structure:
    - one atom moved by 0.5 Å in a lattice-filled block; also one added and
      one deleted
    - an atom's alpha set below 1 (it stops occluding)
    - a changed receiver set (a different cull depth) with no geometry
      change
    - an empty diff, which returns the previous values without computing any
      receiver
    - a diff over most atoms, which takes the full path
  - GPU:
    - Two spheres with exposure 0 and 1: the occluded one's shaded-side
      pixels are darker by the mapping's factor, measured against the twin.
    - `ao_strength` 0 makes them identical.
    - A bond with exposure 0 → 1 brightens monotonically from start to end.
  - **Measure and record** one full `atom_exposure` and the evaluate time
    on a lattice-filled 1 M-atom cuboid and on the 80-cell rod, in a release
    build (D7's absolute budget: at most 15 % of evaluate).
- Effort: 2 d.

### Phase 4b — Exposure cache on the scene (D9)

- Add `ExposureCache` to `NodeSceneData` (`new()`, the `generate_scene`
  literal, `MemorySizeEstimator`, and the direct-construction sites in
  `multi_output_unit_test.rs`). Add `carry_over_exposure` at the
  insertion sites of `refresh_partial` and `refresh_full`. Thread the AO
  flag and `interaction_open` through `tessellate_scene_content`.
- Add the two AO preferences (D8): the domain fields, the API twin and the
  dialog rows.
- **Tests** (`atomcad-structure-designer/tests/structure_designer/ambient_occlusion_cache_test.rs`),
  on a small lattice-filled network with a test-only counter of receivers
  computed:
  - A second refresh after editing an unrelated node computes nothing.
  - Selecting another node (both re-evaluated) computes nothing.
  - An atom_edit selection change computes nothing; moving one atom
    computes only receivers within `R_MAX + max r` of it.
  - A changed cull depth computes only the newly unculled receivers.
  - AO off: nothing computed, nothing carried over, and the
    `occlusion_changes` diff is never run; switching it back on computes
    without re-evaluating the network (the evaluation counter is
    unchanged).
  - Hide → show restores the node from the invisible cache with its
    exposure: nothing computed.
  - A large drag with `interaction_open = true` computes nothing and
    leaves pending changes; the drag-end refresh completes them. This is
    checked for an atom_edit drag and a gadget drag, through the real
    drag-end entry points, which is what proves the closing refresh reaches
    the tessellator.
  - Mutation: drop `carry_over_exposure` from `refresh_partial`; the
    node-selection test must fail.
- **Measure and record** the refresh time of an unrelated-node edit on the
  1 M-atom cuboid with AO on and off. The two should agree within noise.
- Effort: 1 d.

### Phase 5 — Walkthrough and documentation

- **Walkthrough** (maintainer):
  - Studio against Headlight on a CSG cube, a (100) 2×1 reconstruction, the
    T-centre scene, the 80-cell rod (perspective and orthographic) and a
    mechanosynthesis step.
  - The depth cue on the rod and the nanobeam.
  - AO in both display styles.
  - The rim of selected, frozen and marked atoms at 100 % and 200 %.
  - An exported image and a thumbnail on a light background.
  - Tune `AO_FLOOR`, `AO_OPEN`, the rig constants and the default
    strengths; settle Open questions 1 and 2.
- **Docs:**
  - `doc/reference_guide/ui.md`: the five rows of the Rendering section
    (what each does, the default, and that exports follow them). The
    *Ambient occlusion* row says what it costs on a large structure, when
    that cost is paid (when the structure changes, not on every edit), and
    that unticking it removes the cost entirely (D8).
  - `atomcad-structure-designer/src/AGENTS.md`, beside the
    `NodeSceneData` derived-fields paragraph: the exposure cache is filled
    lazily by the tessellator, and a new place that inserts a
    `NodeSceneData` over an older one goes through `carry_over_exposure`.
  - `rust/AGENTS.md`, renderer bullets:
    - lit shaders are composed with `lighting.wgsl` at the
      `// ---- lighting ----` marker, and a change there is mirrored in
      `shading.rs`
    - `ShadingUniform` is per pass and written on every draw, so it is
      exempt from the per-view pairs rule
  - `atomcad-display/src/AGENTS.md`: the `ambient_occlusion` module, and
    the rule that occluders include culled atoms but not ghosts.
- Effort: 0.5 d.

**Total: about 6–7 d.** That is the report's 3–5 d "Depth" bundle, plus the
shader unification, the rim fix and the exposure cache, none of which the
report costed.

## Future work

All of these build on this design without changing it:

- **Shadow maps** (report §2.3). D3's key light is the light the report's
  shadow design assumes: camera-relative, upper side, with the ambient term
  separate. A shadow factor multiplies the key term in `light_surface`. The
  casters and the fit are as the report describes.
- **Cross-structure exposure.** Gather all displayed structures in
  `scene_tessellator` and run `atom_exposure` over their union, so a tool
  tip darkens the surface it approaches.
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
