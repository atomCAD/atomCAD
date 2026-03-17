# Rim Highlight Visual State System

## Motivation

atomCAD uses impostor-based rendering (ray-sphere intersection in fragment shader) for atoms. Several atom states currently override the atom's element color entirely, destroying element identity:

- **Selected atoms**: full magenta override — element color lost
- **Marked atoms** (measurement/guided placement): full yellow/blue override in impostor mode (crosshairs only work in triangle mesh mode)
- **Delete markers**: full red — confusable with oxygen
- **Frozen atoms**: no visual at all (only tooltip on hover)
- **Unknown atoms** (atomic_number=0): gray `(0.5, 0.5, 0.5)` — no distinctive marking

This design introduces a **rim highlight** system: a colored glow at silhouette edges of atom spheres, computed cheaply in the fragment shader using the `dot(normal, view_dir)` Fresnel-like term. This preserves element color at the sphere center while clearly communicating state through a colored rim.

## Design

### Rim Highlight Rendering

The rim effect is computed in the fragment shader after ray-sphere intersection, using the already-available surface normal and view direction:

```wgsl
// After computing world_normal and view direction V:
let NdotV = max(dot(world_normal, V), 0.0);
let rim = smoothstep(rim_start, rim_full, 1.0 - NdotV);
final_albedo = mix(base_albedo, rim_color, rim * rim_intensity);
```

Parameters:
- **rim_color**: RGB color of the rim (per-atom, passed via vertex data)
- **rim_intensity**: strength of the rim blend (0.0 = no rim, 1.0 = full rim color at edges). Encoded in the alpha channel of rim_color or as a separate field.
- **rim_start / rim_full**: smoothstep thresholds on `1.0 - NdotV` controlling where the rim fades in and where it reaches full strength. `rim_start` is the inner boundary (closer to sphere center) where the rim begins appearing; `rim_full` is the outer boundary (at the silhouette) where the rim is fully on. These are global constants in the shader (not per-atom) since all states use the same rim shape. Suggested defaults: `rim_start = 0.6, rim_full = 0.9`.

With these defaults, the rim begins appearing at `NdotV = 0.4` (surface angled ~66° from the viewer) and reaches full strength at `NdotV = 0.1` (nearly edge-on). This confines the rim to roughly the outer 25% of the sphere's visible area, preserving element color across the center and most of the lit surface.

When `rim_color` is `(0, 0, 0)` with intensity 0, the rim has no effect — this is the default for normal atoms with no special state.

### Vertex Data Change

Current `AtomImpostorVertex`:
```rust
pub struct AtomImpostorVertex {
    pub center_position: [f32; 3],  // @location(0) Float32x3
    pub quad_offset: [f32; 2],      // @location(1) Float32x2
    pub radius: f32,                // @location(2) Float32
    pub albedo: [f32; 3],           // @location(3) Float32x3
    pub roughness: f32,             // @location(4) Float32
    pub metallic: f32,              // @location(5) Float32
}
```

New `AtomImpostorVertex` — add `rim_color` (RGBA, where A = intensity):
```rust
pub struct AtomImpostorVertex {
    pub center_position: [f32; 3],  // @location(0) Float32x3
    pub quad_offset: [f32; 2],      // @location(1) Float32x2
    pub radius: f32,                // @location(2) Float32
    pub albedo: [f32; 3],           // @location(3) Float32x3
    pub roughness: f32,             // @location(4) Float32
    pub metallic: f32,              // @location(5) Float32
    pub rim_color: [f32; 4],        // @location(6) Float32x4  [R, G, B, intensity]
}
```

This adds 16 bytes per vertex (64 bytes per atom, since 4 vertices per quad). For a 100k-atom model this is 6.4 MB additional GPU memory — negligible.

### Color and State Assignments

All rim colors are applied in the tessellator (`atomic_tessellator.rs`). The atom's **albedo always reflects its element color** — no more color overrides for state. Delete markers and unchanged markers don't have real element identities; their atomic numbers map to appropriate colors in the element color lookup (dark neutral `(0.2, 0.2, 0.2)` for delete markers, light blue `(0.4, 0.6, 0.9)` for unchanged markers), just as unknown atoms (atomic_number=0) already map to gray `(0.5, 0.5, 0.5)`.

| State | Rim Color | Rim Intensity | Notes |
|-------|-----------|---------------|-------|
| Normal | — | 0.0 | No rim |
| **Selected** | Magenta `(1.0, 0.2, 1.0)` | 0.8 | Replaces full magenta override |
| **Frozen** | Ice blue `(0.5, 0.85, 1.0)` | 0.7 | Visible indicator for frozen state |
| **Marked** (measurement) | Yellow `(1.0, 1.0, 0.0)` | 0.8 | Primary measurement target |
| **Secondary Marked** | Blue `(0.0, 0.5, 1.0)` | 0.8 | Secondary measurement target |
| **Delete Marker** | Red `(0.9, 0.1, 0.1)` | 0.8 | Albedo from element lookup (dark neutral) |
| **Unchanged Marker** | — | 0.0 | Ghost atoms in diff view (bond endpoints that didn't change). Albedo from element lookup (light blue). No rim needed. Displayed as "Unknown" in UI. |

Roughness and metallic are **never overridden** by state — they always use element defaults. Only the rim color changes per state. This keeps the atom's albedo recognizable under all lighting conditions.

### Priority Order (Overlapping States)

An atom can have multiple states simultaneously (e.g., selected + frozen, or selected + delete marker). Only **one** rim color is displayed. Priority from highest to lowest:

1. **Marked / Secondary Marked** — transient measurement UI, needs immediate visibility even when atom is also selected
2. **Selected** — user needs to see what they're about to act on
3. **Delete Marker** — structural semantic, important but less transient
4. **Frozen** — persistent state, lowest priority since it's the most "background" information

**Unchanged Marker** is excluded from priority ordering because it has no rim (intensity 0). When no higher-priority state is active, its material overrides apply normally.

The highest-priority active state wins the rim color. There is no per-field merging — one state controls the rim. Roughness, metallic, and albedo are never affected by state priority.

Albedo is not part of the priority system — it is always the element color. Atoms without real element identities (delete markers, unchanged markers) get their albedo from the element color lookup, which maps their special atomic numbers to appropriate colors.

Examples:

- **Selected + frozen**: rim = magenta (from selected). Material = element defaults.
- **Selected + delete marker**: rim = magenta (from selected). Albedo is the delete marker's element color (dark neutral).
- **Delete marker + frozen**: rim = red (from delete marker). Material = element defaults.
- **Marked + frozen**: rim = yellow (from marked). Material = element defaults.

### Bond Highlighting

Bonds do **not** need rim highlights. Bond colors encode bond type (gray, amber, teal, copper) — there is no element identity to preserve. Full color overrides for selection (magenta) and delete markers (red) remain appropriate for bonds.

## Files to Modify

### Rust (renderer)

| File | Change |
|------|--------|
| `rust/src/renderer/atom_impostor_mesh.rs` | Add `rim_color: [f32; 4]` to `AtomImpostorVertex`, update `desc()` with new vertex attribute at `@location(6)`, update `add_atom_quad()` signature |
| `rust/src/renderer/atom_impostor.wgsl` | Add `rim_color` to vertex input/output structs, add rim calculation in `fs_main` after PBR lighting |
| `rust/src/renderer/renderer.rs` | Update pipeline vertex buffer layout if not auto-derived from `desc()` |

### Rust (display/tessellation)

| File | Change |
|------|--------|
| `rust/src/display/atomic_tessellator.rs` | Rewrite `get_atom_color_and_material()` to always return element color as albedo (remove `to_selected_color` override). The element color lookup already handles special atom types (delete markers, unchanged markers) — no albedo override logic needed. Add `get_atom_rim_color()` function implementing the priority table. Update `tessellate_atom_impostor()` to pass rim color. |

### Rust (crystolecule)

| File | Change |
|------|--------|
| `rust/src/crystolecule/atomic_structure/atom.rs` | No change needed — frozen flag already exists at bit 2 |

### Constants

New constants in `atomic_tessellator.rs`:
```rust
// Rim highlight colors
const SELECTED_RIM_COLOR: Vec4 = Vec4::new(1.0, 0.2, 1.0, 0.8);   // Magenta
const FROZEN_RIM_COLOR: Vec4 = Vec4::new(0.5, 0.85, 1.0, 0.7);    // Ice blue
const MARKED_RIM_COLOR: Vec4 = Vec4::new(1.0, 1.0, 0.0, 0.8);     // Yellow
const SECONDARY_MARKED_RIM_COLOR: Vec4 = Vec4::new(0.0, 0.5, 1.0, 0.8); // Blue
const DELETE_MARKER_RIM_COLOR: Vec4 = Vec4::new(0.9, 0.1, 0.1, 0.8);    // Red
const NO_RIM: Vec4 = Vec4::new(0.0, 0.0, 0.0, 0.0);
```

## Triangle Mesh Rendering Path

The rim highlight system is **impostor-only**. The triangle mesh rendering path (`mesh.wgsl`) retains its current behavior:

- **Selected**: full magenta color override
- **Marked**: 3D crosshair geometry (cylinders along X/Y/Z axes)
- **Delete/Unchanged markers**: color overrides
- **Frozen**: no visual (same as current)

Most users use the impostor path, so this is an acceptable inconsistency. If the triangle mesh path needs rim highlights in the future, the same approach applies: add `rim_color` to the mesh vertex struct, add the `dot(N, V)` rim math to `mesh.wgsl`, and pass rim colors from the mesh tessellator.

## Shader Change Detail

### Refactor: extract tone mapping from `calculate_pbr_lighting`

Currently `calculate_pbr_lighting` applies Reinhard tone mapping and gamma correction internally before returning. The rim blend must happen in linear HDR space (before tone mapping), so these steps are extracted into `fs_main`:

**Before (current `calculate_pbr_lighting` ending):**
```wgsl
    var color = light_contribution * (diffuse + specular) + ambient;
    color = color / (color + vec3(1.0)); // Tone mapping
    color = pow(color, vec3(1.0/2.2)); // Gamma correction
    return color;
```

**After (refactored):**
```wgsl
fn calculate_pbr_lighting(...) -> vec3<f32> {
    // ... PBR calculation unchanged ...
    var color = light_contribution * (diffuse + specular) + ambient;
    // Return linear HDR color — tone mapping and gamma applied by caller
    return color;
}
```

This is a behavior-preserving refactor for the non-rim path: `fs_main` applies tone mapping and gamma after calling `calculate_pbr_lighting`, producing identical output for atoms with no rim.

### Rim highlight in `fs_main`

```wgsl
@fragment
fn fs_main(input: AtomImpostorVertexOutput) -> AtomFragmentOutput {
    // ... ray-sphere intersection unchanged ...

    // PBR lighting (returns linear HDR)
    var color = calculate_pbr_lighting(hit_point, world_normal, input.albedo, ...);

    // Rim highlight (in linear HDR space, before tone mapping)
    let V = normalize(camera.camera_position - hit_point);
    let NdotV = max(dot(world_normal, V), 0.0);
    let rim_start = 0.6;
    let rim_full = 0.9;
    let rim_factor = smoothstep(rim_start, rim_full, 1.0 - NdotV);
    let rim_blend = rim_factor * input.rim_color.a;
    color = mix(color, input.rim_color.rgb, rim_blend);

    // Tone mapping and gamma (moved here from calculate_pbr_lighting)
    color = color / (color + vec3(1.0));
    color = pow(color, vec3(1.0 / 2.2));

    var output: AtomFragmentOutput;
    output.depth = depth;
    output.color = vec4<f32>(color, 1.0);
    return output;
}
```

**Why this ordering matters:** Rim colors are specified as sRGB constants (e.g., magenta `(1.0, 0.2, 1.0)`). Blending them into the PBR result in linear HDR space, then tone mapping everything together, ensures the rim and the lit surface respond identically to tone mapping. If the blend happened after tone mapping, rim colors would appear brighter and more saturated than intended because they'd skip the HDR compression step.

### `mesh.wgsl` consistency

The same tone-mapping extraction should be applied to `mesh.wgsl`'s `calculate_pbr_lighting` copy to keep the two shaders consistent, even though `mesh.wgsl` does not use rim highlights. This prevents a future refactor from accidentally re-introducing the problem.

## Implementation Phases

### Phase 1: Infrastructure + Selection + Frozen
- Add `rim_color: [f32; 4]` field to `AtomImpostorVertex`, update `desc()` and `add_atom_quad()`
- Add `rim_color` to shader vertex input/output structs (`atom_impostor.wgsl`)
- Refactor `calculate_pbr_lighting` in both `atom_impostor.wgsl` and `mesh.wgsl`: remove tone mapping and gamma correction from the function, move them to the caller (`fs_main`). This is a behavior-preserving change that enables rim blending in linear HDR space.
- Implement rim calculation in `fs_main` between the PBR call and tone mapping/gamma
- Change `get_atom_color_and_material()` to always return element color for selected atoms (remove magenta override)
- Remove `to_selected_color()` function
- Set magenta rim for selected atoms, ice-blue rim + icy material for frozen atoms
- Implement priority: selected > frozen (if both, magenta rim but icy material)
- All other atoms get `NO_RIM` default

### Phase 2: Marked + Delete markers
- Set yellow/blue rim for marked/secondary-marked atoms (remove color override in `tessellate_atom_impostor()`)
- Delete markers: remove red color override, use element color from lookup (dark neutral), add red rim
- Unchanged markers keep current behavior (element color from lookup gives light blue, roughness 0.7, no rim)
- Full priority chain active: Selected > Marked > Delete Marker > Frozen


## Tuning

The rim `rim_start`, `rim_full`, and per-state intensity values should be tuned visually. Suggested approach:
- Start with the values in this document
- Test with a diamond lattice (many carbon atoms) with some atoms selected and some frozen
- Adjust rim width (rim_start/rim_full) for visual clarity at different zoom levels
- Ensure rim is visible in both ball-and-stick and space-filling modes
- Verify that different rim colors are distinguishable from each other when atoms of different states are adjacent
