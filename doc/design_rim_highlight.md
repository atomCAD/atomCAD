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
let rim = smoothstep(rim_inner_edge, rim_outer_edge, 1.0 - NdotV);
final_albedo = mix(base_albedo, rim_color, rim * rim_intensity);
```

Parameters:
- **rim_color**: RGB color of the rim (per-atom, passed via vertex data)
- **rim_intensity**: strength of the rim blend (0.0 = no rim, 1.0 = full rim color at edges). Encoded in the alpha channel of rim_color or as a separate field.
- **rim_inner_edge / rim_outer_edge**: smoothstep thresholds controlling rim width. These can be global constants in the shader (not per-atom) since all states use the same rim shape. Suggested defaults: `inner_edge = 0.25, outer_edge = 0.7`.

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

All rim colors are applied in the tessellator (`atomic_tessellator.rs`). The atom's **base albedo always reflects its element color** — no more color overrides for state.

| State | Rim Color | Rim Intensity | Material Override | Notes |
|-------|-----------|---------------|-------------------|-------|
| Normal | — | 0.0 | — | No rim |
| **Selected** | Magenta `(1.0, 0.2, 1.0)` | 0.8 | Roughness 0.15 | Replaces full magenta override |
| **Frozen** | Ice blue `(0.5, 0.85, 1.0)` | 0.7 | Roughness 0.05, metallic 0.6 | Icy/glassy appearance |
| **Marked** (measurement) | Yellow `(1.0, 1.0, 0.0)` | 0.8 | — | Primary measurement target |
| **Secondary Marked** | Blue `(0.0, 0.5, 1.0)` | 0.8 | — | Secondary measurement target |
| **Delete Marker** | Red `(0.9, 0.1, 0.1)` | 0.8 | Roughness 0.5 | Base albedo becomes neutral dark `(0.2, 0.2, 0.2)` instead of red |
| **Unchanged Marker** | — | 0.0 | Roughness 0.7 | Keep current light blue albedo `(0.4, 0.6, 0.9)`, no rim needed (these are ghost atoms) |
| **Unknown Element** | Orange `(1.0, 0.6, 0.0)` | 0.6 | — | Distinguishes from similarly-gray atoms; base albedo stays gray `(0.5, 0.5, 0.5)` |

### Priority Order (Overlapping States)

An atom can have multiple states simultaneously (e.g., selected + frozen, or selected + delete marker). Only **one** rim color is displayed. Priority from highest to lowest:

1. **Selected** — always takes visual priority; user needs to see what they're about to act on
2. **Marked / Secondary Marked** — transient measurement UI, needs immediate visibility
3. **Delete Marker** — structural semantic, important but less transient
4. **Unknown Element** — structural semantic
5. **Frozen** — persistent state, lowest priority since it's the most "background" information

When a higher-priority state provides a rim, lower-priority states are visually suppressed. However, **material overrides stack independently** of rim priority. For example, a selected + frozen atom gets:
- Rim: magenta (from selected, higher priority)
- Material: roughness 0.05, metallic 0.6 (from frozen — material changes are independent)

This means a selected-frozen atom looks glassy/icy with a magenta rim, while a selected-normal atom looks slightly shiny with a magenta rim. The material difference provides a secondary cue even when the rim is "taken" by selection.

### Bond Highlighting

Bonds also currently use full magenta override for selection. The bond impostor shader (`bond_impostor.wgsl`) should receive analogous treatment:

- Selected bonds: keep element/type-based color, add a rim/glow effect along the cylinder silhouette edges
- Delete marker bonds: neutral dark color with red rim

The cylinder rim math is similar: `1.0 - abs(dot(surface_normal, view_dir))` gives silhouette proximity for a cylinder. This is a follow-up task and not required for the initial atom implementation.

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
| `rust/src/display/atomic_tessellator.rs` | Rewrite `get_atom_color_and_material()` to always return element color as albedo (remove `to_selected_color` override). Add `get_atom_rim_color()` function implementing the priority table. Update `tessellate_atom_impostor()` to pass rim color. Update delete marker tessellation to use neutral albedo + red rim. |

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
const UNKNOWN_ELEMENT_RIM_COLOR: Vec4 = Vec4::new(1.0, 0.6, 0.0, 0.6);  // Orange
const NO_RIM: Vec4 = Vec4::new(0.0, 0.0, 0.0, 0.0);
```

## Shader Change Detail

In `atom_impostor.wgsl`, the fragment shader change:

```wgsl
// --- Current end of fs_main ---
// let color = calculate_pbr_lighting(hit_point, world_normal, input.albedo, ...);

// --- New: apply rim highlight after PBR ---
let V = normalize(camera.camera_position - hit_point);
let NdotV = max(dot(world_normal, V), 0.0);

// Rim parameters (global constants)
let rim_inner = 0.25;
let rim_outer = 0.7;
let rim_factor = smoothstep(rim_inner, rim_outer, 1.0 - NdotV);

// Blend rim color into final output (input.rim_color.a is intensity)
let rim_blend = rim_factor * input.rim_color.a;
var final_color = mix(color, input.rim_color.rgb, rim_blend);

// Tone mapping and gamma are already applied in calculate_pbr_lighting,
// so rim color should be in post-tonemap space, OR we apply rim before
// tonemap by moving it inside calculate_pbr_lighting.
```

**Note on tone mapping**: The rim blend should happen _before_ tone mapping and gamma correction for physically correct results. This means either moving the rim logic inside `calculate_pbr_lighting` or extracting tone mapping/gamma to happen after the rim blend. The latter is cleaner.

## Implementation Phases

### Phase 1: Core rim infrastructure
- Add `rim_color` field to `AtomImpostorVertex` and shader
- Implement rim calculation in fragment shader
- Wire through tessellator with `NO_RIM` default for all atoms
- Verify: no visual change (all atoms have zero-intensity rim)

### Phase 2: Selection rim
- Change `get_atom_color_and_material()` to return element color for selected atoms (remove magenta override)
- Set magenta rim for selected atoms in tessellator
- Adjust roughness (keep existing 0.15 for selected)
- Remove `to_selected_color()` function

### Phase 3: Frozen rim
- Set ice-blue rim + icy material (roughness 0.05, metallic 0.6) for frozen atoms
- Priority: selected > frozen (if both, use magenta rim but keep icy material)

### Phase 4: Marked atoms rim
- Set yellow/blue rim for marked/secondary-marked atoms
- Remove the color override in `tessellate_atom_impostor()` for marked states

### Phase 5: Delete markers and unknown elements
- Change delete marker albedo from red to neutral dark, add red rim
- Add orange rim for unknown element atoms (atomic_number not in ATOM_INFO)

### Phase 6 (follow-up): Bond rim highlights
- Extend bond impostor shader with similar rim logic for selected/delete-marker bonds

## Tuning

The rim `inner_edge`, `outer_edge`, and per-state intensity values should be tuned visually. Suggested approach:
- Start with the values in this document
- Test with a diamond lattice (many carbon atoms) with some atoms selected and some frozen
- Adjust rim width (inner/outer edge) for visual clarity at different zoom levels
- Ensure rim is visible in both ball-and-stick and space-filling modes
- Verify that different rim colors are distinguishable from each other when atoms of different states are adjacent
