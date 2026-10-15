// ============================================================================
// Shared lighting (doc/design_depth_perception.md D1)
//
// Not a shader of its own: `pipelines::compose_lit_shader` inserts this file
// into each lit shader (`atom_impostor.wgsl`, `bond_impostor.wgsl`,
// `transparent_impostor.wgsl`, `mesh.wgsl`) at the host's
// `// ---- lighting ----` marker, after the host's `CameraUniform` and
// `ModelUniform` declarations. It reads `camera` but does not declare it.
// ============================================================================

const PI: f32 = 3.14159265359;

// The per-pass shading uniform (D2). Twin: `shading_uniform::ShadingUniform`,
// whose offsets `shader_test.rs` checks against this struct in every lit
// shader. Directions are world space; the two light directions are the
// direction the light travels, as `CameraUniform.head_light_dir`.
struct ShadingUniform {
  key_light_dir: vec3<f32>,
  key_intensity: f32,
  fill_light_dir: vec3<f32>,
  fill_intensity: f32,
  hemisphere_up: vec3<f32>,  // The sky side of the hemisphere ambient.
  sky: f32,
  background: vec3<f32>,     // The pass's clear colour, display space (D5).
  ground: f32,
  depth_cue_front: f32,      // View depth where the depth cue starts (D5).
  depth_cue_span: f32,       // Depth over which it reaches full strength; > 0.
  depth_cue_strength: f32,
  ao_strength: f32,          // D7, Phase 4.
};
@group(0) @binding(1)
var<uniform> shading: ShadingUniform;

// The exposure of a surface nothing occludes, packed as `(4b, E)` (D10): what
// a caller without per-atom exposure passes to `light_surface`.
const FULLY_EXPOSED: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 1.0);

// ============================================================================
// Camera helpers
// ============================================================================

// Points from the scene toward the eye (right-handed view space +Z),
// extracted from the view matrix rows (the rotation part of a look-at matrix
// stores the camera axes as rows).
fn camera_backward() -> vec3<f32> {
    return vec3<f32>(camera.view_matrix[0][2], camera.view_matrix[1][2], camera.view_matrix[2][2]);
}

// Direction from a surface point toward the viewer. In orthographic mode all
// view rays are parallel, so the eye position must not be used per-point.
fn camera_view_vector(world_position: vec3<f32>) -> vec3<f32> {
    if camera.is_orthographic > 0.5 {
        return camera_backward();
    }
    return normalize(camera.camera_position - world_position);
}

// ============================================================================
// PBR helpers
// ============================================================================

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

fn distribution_ggx(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    let denom = (NdotH2 * (a2 - 1.0) + 1.0);
    return num / (PI * denom * denom);
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

fn geometry_smith(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, roughness: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx2 = geometry_schlick_ggx(NdotV, roughness);
    let ggx1 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

// ============================================================================
// The rig (D3)
// ============================================================================

// The linear (HDR) colour of a surface point under the pass's rig: the key
// light (diffuse + specular), the diffuse-only fill light and the hemisphere
// ambient. `exposure` is the packed `(4b, E)` of D10, `FULLY_EXPOSED` for a
// surface without per-atom exposure. Twin: `shading::light_surface`.
fn light_surface(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
    exposure: vec4<f32>,
) -> vec3<f32> {
    let V = camera_view_vector(world_position);
    let N = normalize(normal);
    let L = normalize(-shading.key_light_dir);
    let H = normalize(V + L);

    // Base reflectivity for metallic/non-metallic materials
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);

    // Fresnel term
    let F = fresnel_schlick(max(dot(H, V), 0.0), f0);

    // Normal Distribution Function (NDF)
    let D = distribution_ggx(N, H, roughness);

    // Geometry term
    let G = geometry_smith(N, V, L, roughness);

    // BRDF
    let numerator = D * G * F;
    let denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.001;
    let specular = numerator / denominator;

    // Diffuse term (Lambertian reflection)
    let k_d = (vec3<f32>(1.0) - F) * (1.0 - metallic);
    let diffuse = k_d * albedo / PI;

    // What burial takes from the indirect light and from the key (D7). Both
    // stay 1 until Phase 4 maps `exposure`.
    let ambient_factor = 1.0;
    let key_factor = 1.0;

    let NdotL = max(dot(N, L), 0.0);
    let key_contribution = vec3<f32>(shading.key_intensity * key_factor) * NdotL;

    // Hemisphere ambient: ground below, sky above, around the camera's up.
    // Written as `ground + (sky − ground)·t` rather than `mix`, so a preset
    // with sky = ground gets exactly that value at every normal.
    let t = 0.5 + 0.5 * dot(N, shading.hemisphere_up);
    let ambient_light = (shading.ground + (shading.sky - shading.ground) * t) * ambient_factor;

    // Diffuse ambient term (non-metallic contribution)
    let ambient_diffuse = vec3<f32>(ambient_light) * albedo;

    // Specular ambient term (reflective contribution)
    let ambient_specular = vec3<f32>(ambient_light) * fresnel_schlick(max(dot(V, N), 0.0), f0);

    // Blend ambient terms based on the material's metallic property
    let ambient = mix(ambient_diffuse, ambient_specular, metallic);

    // Fill light: diffuse only, so a sphere keeps one specular highlight.
    let L_fill = normalize(-shading.fill_light_dir);
    let fill = vec3<f32>(shading.fill_intensity * ambient_factor) * max(dot(N, L_fill), 0.0) * diffuse;

    return key_contribution * (diffuse + specular) + ambient + fill;
}

// Linear HDR colour to display colour: Reinhard tone mapping, then gamma.
// Twin: `shading::to_display`.
fn to_display(linear: vec3<f32>) -> vec3<f32> {
    let mapped = linear / (linear + vec3<f32>(1.0));
    return pow(mapped, vec3<f32>(1.0 / 2.2));
}

// ============================================================================
// The depth cue (D5)
// ============================================================================

// A display colour fogged towards the pass's background with its view depth:
// not at all at `depth_cue_front` (the nearest content), by
// `depth_cue_strength` at a span's depth behind it. Mixed in display space, so
// at full strength the result is exactly the background pixel. Applied after
// `to_display` and before `apply_rim`. Twin: `shading::apply_depth_cue`.
fn apply_depth_cue(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    // The gadget pass, no content, the cue off: leave the colour untouched,
    // not merely mixed by 0.
    if shading.depth_cue_strength <= 0.0 {
        return color;
    }
    let depth = -(camera.view_matrix * vec4<f32>(world_position, 1.0)).z;
    let f = shading.depth_cue_strength
        * clamp((depth - shading.depth_cue_front) / shading.depth_cue_span, 0.0, 1.0);
    return mix(color, shading.background, f);
}

// ============================================================================
// The rim band (D4)
// ============================================================================

// The rim band's width in output pixels. Twin: `shading::RIM_WIDTH_PX`.
const RIM_WIDTH_PX: f32 = 3.0;

// How much of the rim colour a sphere's pixel takes: 1 within the band along
// the silhouette, 0 inside it, with a one-render-pixel soft inner edge.
// `distance` is how far the view ray passes outside the silhouette in world
// units (`sphere_ray_distance`, negative inside), `pixel_world` the world
// size of a render pixel at the sphere. The band is `RIM_WIDTH_PX` output
// pixels wide, but at most 0.35 of the sphere's radius on screen, so a
// sphere a few pixels across is not all rim. Twin: `shading::rim_weight`.
fn rim_weight(distance: f32, pixel_world: f32, radius: f32) -> f32 {
    let inside_px = -distance / pixel_world;
    let width_px = min(RIM_WIDTH_PX * camera.pixel_scale, 0.35 * radius / pixel_world);
    return 1.0 - smoothstep(width_px - 0.5, width_px + 0.5, inside_px);
}

// The world size of a render pixel where the derivatives were taken: the
// mean of the two screen axes' steps.
fn pixel_world_size(dwx: vec3<f32>, dwy: vec3<f32>) -> f32 {
    return 0.5 * (length(dwx) + length(dwy));
}

// The rim colour over a display colour, applied last: after `to_display` and
// after the depth cue, so the rim is neither lit nor fogged (D4).
fn apply_rim(color: vec3<f32>, rim_color: vec3<f32>, weight: f32) -> vec3<f32> {
    return mix(color, rim_color, weight);
}
