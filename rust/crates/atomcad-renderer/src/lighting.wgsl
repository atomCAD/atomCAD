// ============================================================================
// Shared lighting (doc/design_depth_perception.md D1)
//
// Not a shader of its own: `pipelines::compose_lit_shader` inserts this file
// into each lit shader (`atom_impostor.wgsl`, `bond_impostor.wgsl`,
// `transparent_impostor.wgsl`, `mesh.wgsl`) at the host's
// `// ---- lighting ----` marker, after the host's `CameraUniform` and
// `ModelUniform` declarations. It reads `camera` but does not declare it.
// It declares group 2, the shadow map (doc/design_shadows.md D8).
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
  ao_strength: f32,          // Baked ambient occlusion, 0 = off (D7).
  shadow_strength: f32,      // Shadows, 0 = none (doc/design_shadows.md D1).
  ao_key: f32,               // AO's share of the key (shadows D10).
};
@group(0) @binding(1)
var<uniform> shading: ShadingUniform;

// The key light's shadow map (doc/design_shadows.md D8). Twins:
// `shadow_fit::ShadowCascade`, `shadow_fit::ShadowUniform`, whose offsets
// `shader_test.rs` checks against these structs.
struct ShadowCascade {
  world_to_clip: mat4x4<f32>,  // The light camera's view_proj.
  split_far: f32,              // View depth where the cascade ends (D6).
  texel_world: f32,            // t, in Å.
  filter_radius_uv: f32,       // The PCF radius over the square's side.
  depth_range: f32,            // The light camera's zfar − znear, in Å.
};
struct ShadowUniform {
  cascades: array<ShadowCascade, 4>,
  count: u32,
  blend_fraction: f32,
  debug_tint: u32,             // Colour each receiver by its cascade (D9).
  front: f32,                  // View depth where cascade 0 starts.
};
@group(2) @binding(0)
var<uniform> shadow: ShadowUniform;
@group(2) @binding(1)
var shadow_map: texture_depth_2d_array;
@group(2) @binding(2)
var shadow_sampler: sampler_comparison;

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
// Baked ambient occlusion (D7, D10)
// ============================================================================

// The mapping from visibility to light. Twins: `shading::AO_FLOOR`,
// `AO_OPEN`. The key light's share of the darkening is `shading.ao_key`.
const AO_FLOOR: f32 = 0.25; // What a fully buried surface keeps.
const AO_OPEN: f32 = 0.6;   // Visibility above which nothing is taken.

// The cosine-weighted visible fraction of the sky around the normal `N`, from
// the packed `(4b, E)` (D10): `E + 2·dot(b, N)`, with `4b` as delivered.
// Twin: `shading::ao_visibility`.
fn ao_visibility(exposure: vec4<f32>, N: vec3<f32>) -> f32 {
    return clamp(exposure.w + 0.5 * dot(exposure.xyz, N), 0.0, 1.0);
}

// The factors burial applies to the indirect light (x: fill and hemisphere)
// and to the key (y). Both exactly 1 when AO is off, whatever the exposure.
// Twin: `shading::ao_factors`.
fn ao_factors(exposure: vec4<f32>, N: vec3<f32>) -> vec2<f32> {
    if shading.ao_strength <= 0.0 {
        return vec2<f32>(1.0, 1.0);
    }
    let ao = AO_FLOOR + (1.0 - AO_FLOOR) * smoothstep(0.0, AO_OPEN, ao_visibility(exposure, N));
    return vec2<f32>(
        mix(1.0, ao, shading.ao_strength),
        mix(1.0, ao, shading.ao_strength * shading.ao_key),
    );
}

// ============================================================================
// Shadows (doc/design_shadows.md D7)
// ============================================================================

// The bias, in texels of the cascade (D7). Starting values, bounded by the
// GPU harness's acne and contact detectors.
const NORMAL_OFFSET_TEXELS: f32 = 1.0; // At most `shadow_fit`'s margin allows.
const SLOPE_BIAS_TEXELS: f32 = 0.71;
const CONST_BIAS_TEXELS: f32 = 0.5;
// The largest receiver slope the residual bias follows, as tan θ.
const SHADOW_MAX_TAN: f32 = 4.0;
// The largest slope the receiver plane follows, as tan θ (89.4°). Not the
// residual's 4: a convex receiver's tangent plane is nearer the light than
// the surface at every tap only while it has the surface's true slope, and
// a plane clamped at 76° rises slower than a sphere near its terminator and
// shadows it (the GPU harness's acne detector). This clamp only keeps the
// arithmetic finite; the key contributes about 1 % there.
const RECEIVER_PLANE_MAX_TAN: f32 = 100.0;

// The PCF taps: a fixed 16-point Poisson disk of radius 1, centred.
fn shadow_tap(i: u32) -> vec2<f32> {
    var taps = array<vec2<f32>, 16>(
        vec2<f32>(-0.2992, -0.4867),
        vec2<f32>(0.9289, 0.3619),
        vec2<f32>(-0.2741, 0.9356),
        vec2<f32>(0.7561, -0.5880),
        vec2<f32>(-0.9193, -0.0445),
        vec2<f32>(0.0006, 0.2579),
        vec2<f32>(0.3935, 0.7820),
        vec2<f32>(-0.8005, 0.5882),
        vec2<f32>(0.2486, -0.8653),
        vec2<f32>(0.3185, -0.1971),
        vec2<f32>(0.9736, -0.1383),
        vec2<f32>(-0.3670, 0.4965),
        vec2<f32>(-0.4291, 0.0610),
        vec2<f32>(-0.1780, -0.9514),
        vec2<f32>(0.4993, 0.2611),
        vec2<f32>(-0.8498, -0.4836),
    );
    return taps[i];
}

// The view depth of `p`, as the fit measures its splits (D6).
fn shadow_view_depth(p: vec3<f32>) -> f32 {
    return -(camera.view_matrix * vec4<f32>(p, 1.0)).z;
}

// The cascade serving view depth `depth` (D7): the first whose slice reaches
// it; beyond the last split, the last cascade.
fn shadow_cascade(depth: f32) -> u32 {
    var k = 0u;
    while k + 1u < shadow.count && depth > shadow.cascades[k].split_far {
        k += 1u;
    }
    return k;
}

// The view depth where cascade `k` starts: the previous one's end.
fn shadow_split_near(k: u32) -> f32 {
    if k == 0u {
        return shadow.front;
    }
    return shadow.cascades[k - 1u].split_far;
}

// 1 where the key light reaches `p`, 0 in full shadow. `geo_normal` is the
// surface's geometric normal (D7), not a shading normal. Uses only
// `textureSampleCompareLevel`, so it is valid after a `discard`.
fn shadow_visibility(p: vec3<f32>, geo_normal: vec3<f32>) -> f32 {
    // Shadows off, the gadget pass, no casters: nothing is sampled.
    if shading.shadow_strength <= 0.0 {
        return 1.0;
    }
    let n = normalize(geo_normal);
    let cos_theta = dot(n, normalize(-shading.key_light_dir));
    // Facing away from the key, which contributes nothing there anyway.
    if cos_theta <= 0.0 {
        return 1.0;
    }
    let depth = shadow_view_depth(p);
    let k = shadow_cascade(depth);
    let v = cascade_visibility(k, p, n, cos_theta);
    if k + 1u >= shadow.count {
        return v;
    }
    // The blend band: the last `blend_fraction` of the cascade's slice is
    // mixed into the next cascade, so a split never shows as a seam. The
    // next cascade is fitted back over this band (D5 step 0), so `p` is
    // inside both squares.
    let split_far = shadow.cascades[k].split_far;
    let band = shadow.blend_fraction * (split_far - shadow_split_near(k));
    let w = (depth - (split_far - band)) / max(band, 1.0e-6);
    if w <= 0.0 {
        return v;
    }
    return mix(v, cascade_visibility(k + 1u, p, n, cos_theta), min(w, 1.0));
}

// The debug tint's colours (D9): cascade 0 red, 1 green, 2 blue, 3 yellow.
fn shadow_cascade_tint(k: u32) -> vec3<f32> {
    var tints = array<vec3<f32>, 4>(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0),
        vec3<f32>(1.0, 1.0, 0.0),
    );
    return tints[min(k, 3u)];
}

// A linear colour at `p` mixed half-way to its cascade's tint when the debug
// tint is on and the pass draws shadows (D9); otherwise unchanged. The only
// way to see where the splits fall.
fn apply_cascade_tint(color: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    if shadow.debug_tint == 0u || shading.shadow_strength <= 0.0 || shadow.count == 0u {
        return color;
    }
    return mix(color, shadow_cascade_tint(shadow_cascade(shadow_view_depth(p))), 0.5);
}

// `shadow_visibility` in cascade `k`, for a receiver at `p` with the unit
// geometric normal `n` at `cos_theta` to the light.
fn cascade_visibility(k: u32, p: vec3<f32>, n: vec3<f32>, cos_theta: f32) -> f32 {
    let cascade = shadow.cascades[k];
    let m = cascade.world_to_clip;
    let t = cascade.texel_world;
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let tan_theta = min(sin_theta / cos_theta, SHADOW_MAX_TAN);

    // 1. The normal offset: off the surface by about a texel, nothing where
    // it faces the light.
    let q = p + n * (NORMAL_OFFSET_TEXELS * t * sin_theta);
    // The light camera is orthographic, so w = 1.
    let clip = (m * vec4<f32>(q, 1.0)).xyz;
    let uv = vec2<f32>(0.5 + 0.5 * clip.x, 0.5 - 0.5 * clip.y);
    // Outside the cascade's square nothing casts (D7). The sampler clamps,
    // so without this test the border texels would repeat out to infinity.
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return 1.0;
    }

    // 2. The receiver's tangent plane, as depth per unit uv. The matrix's
    // first two rows are the light's x and y axes scaled by 2/s.
    let row_x = vec3<f32>(m[0][0], m[1][0], m[2][0]);
    let row_y = vec3<f32>(m[0][1], m[1][1], m[2][1]);
    let side = 2.0 / length(row_x);
    // Moving the lookup by (du, dv) moves it by du·s along x_L and by −dv·s
    // along y_L (v runs down); on the plane, depth along L then changes by
    // (n_x·Δx + n_y·Δy) / cos θ.
    let slope = vec2<f32>(dot(n, row_x), -dot(n, row_y)) * (0.5 * side) / cos_theta;
    let slope_tan = length(slope);
    let clamped = slope * min(1.0, RECEIVER_PLANE_MAX_TAN / max(slope_tan, 1.0e-6));
    let depth_per_uv = clamped * side / cascade.depth_range;

    // 3. The residual: the comparison's 2×2 footprint.
    let bias = (SLOPE_BIAS_TEXELS * tan_theta + CONST_BIAS_TEXELS) * t / cascade.depth_range;

    // Fixed-softness PCF: the disk's radius is in Å, not texels.
    var lit = 0.0;
    for (var i = 0u; i < 16u; i++) {
        let offset = shadow_tap(i) * cascade.filter_radius_uv;
        let reference = clip.z + dot(depth_per_uv, offset) - bias;
        lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, k, reference);
    }
    return lit / 16.0;
}

// The share of the key light that reaches a surface whose shadow lookup gave
// `v` (D1): exactly 1 at strength 0. Twin: `shading::key_visibility`.
fn key_visibility(v: f32) -> f32 {
    return 1.0 - shading.shadow_strength * (1.0 - v);
}

// ============================================================================
// The rig (D3)
// ============================================================================

// The linear (HDR) colour of a surface point under the pass's rig: the key
// light (diffuse + specular), the diffuse-only fill light and the hemisphere
// ambient. `exposure` is the packed `(4b, E)` of D10, `FULLY_EXPOSED` for a
// surface without per-atom exposure. `key_visibility` is the share of the key
// that reaches the surface (doc/design_shadows.md D1): `key_visibility(…)` of
// its shadow lookup, or 1. With the cascade debug tint on, the result is
// tinted by its cascade (`apply_cascade_tint`, which has no twin). Twin:
// `shading::light_surface_exposed`.
fn light_surface(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
    exposure: vec4<f32>,
    key_visibility: f32,
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

    // What burial takes from the indirect light and from the key (D7).
    let factors = ao_factors(exposure, N);
    let ambient_factor = factors.x;
    let key_factor = factors.y;

    let NdotL = max(dot(N, L), 0.0);
    let key_contribution = vec3<f32>(shading.key_intensity * key_factor * key_visibility) * NdotL;

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

    return apply_cascade_tint(key_contribution * (diffuse + specular) + ambient + fill, world_position);
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
