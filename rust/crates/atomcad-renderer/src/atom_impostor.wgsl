const PI: f32 = 3.14159265359;

struct CameraUniform {
  view_proj: mat4x4<f32>,
  view_matrix: mat4x4<f32>,
  proj_matrix: mat4x4<f32>,
  camera_position: vec3<f32>,
  head_light_dir: vec3<f32>,
  is_orthographic: f32,      // 1.0 = orthographic, 0.0 = perspective
  ortho_half_height: f32,    // Half height for orthographic projection (for zoom level)
  viewport_size_px: vec2<f32>, // Render-target size in render pixels
  pixel_scale: f32,          // Render-target pixels per output pixel
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct ModelUniform {
  model_matrix: mat4x4<f32>,
  normal_matrix: mat4x4<f32>,  // For transforming normals
};
@group(1) @binding(0)
var<uniform> model: ModelUniform;

struct AtomImpostorVertexInput {
    @location(0) center_position: vec3<f32>,
    @location(1) quad_offset: vec2<f32>,
    @location(2) radius: f32,
    @location(3) albedo: vec3<f32>,
    @location(4) roughness: f32,
    @location(5) metallic: f32,
    @location(6) rim_color: vec4<f32>,
}

struct AtomImpostorVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_center: vec3<f32>,
    @location(1) world_position: vec3<f32>,  // World position of the quad vertex
    @location(2) radius: f32,
    @location(3) albedo: vec3<f32>,
    @location(4) roughness: f32,
    @location(5) metallic: f32,
    @location(6) quad_uv: vec2<f32>, // For ray-casting (-1 to 1 range)
    @location(7) rim_color: vec4<f32>,
}

// ============================================================================
// Camera Helper Functions
// ============================================================================

// Camera basis vectors in world space, extracted from the view matrix rows
// (the rotation part of a look-at matrix stores the camera axes as rows).
fn camera_right() -> vec3<f32> {
    return vec3<f32>(camera.view_matrix[0][0], camera.view_matrix[1][0], camera.view_matrix[2][0]);
}

fn camera_up() -> vec3<f32> {
    return vec3<f32>(camera.view_matrix[0][1], camera.view_matrix[1][1], camera.view_matrix[2][1]);
}

// Points from the scene toward the eye (right-handed view space +Z).
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
// PBR Helper Functions (copied from mesh.wgsl)
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

fn calculate_pbr_lighting(
    world_position: vec3<f32>,
    normal: vec3<f32>, 
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32
) -> vec3<f32> {
    let N = normalize(normal);
    let V = camera_view_vector(world_position);
    let L = normalize(-camera.head_light_dir);
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

    let NdotL = max(dot(N, L), 0.0);
    let light_color = vec3<f32>(2.0);
    let light_contribution = light_color * NdotL;

    // Diffuse ambient term (non-metallic contribution)
    let ambient_light_color: vec3<f32> = vec3<f32>(0.2); // Ambient light color
    let ambient_diffuse = ambient_light_color * albedo;

    // Specular ambient term (reflective contribution)
    let ambient_specular = ambient_light_color * fresnel_schlick(max(dot(V, N), 0.0), f0);

    // Blend ambient terms based on the material's metallic property
    let ambient = mix(ambient_diffuse, ambient_specular, metallic);

    var color = light_contribution * (diffuse + specular) + ambient;

    // Return linear HDR color — tone mapping and gamma applied by caller
    return color;
}

// ============================================================================
// Atom Impostor Shaders
// ============================================================================

@vertex
fn vs_main(input: AtomImpostorVertexInput) -> AtomImpostorVertexOutput {
    var output: AtomImpostorVertexOutput;
    
    // Transform atom center to world space
    let world_center = (model.model_matrix * vec4<f32>(input.center_position, 1.0)).xyz;
    
    var right: vec3<f32>;
    var up: vec3<f32>;
    if camera.is_orthographic > 0.5 {
        // Parallel projection: every quad must face the shared view direction.
        // A per-atom eye-facing billboard tilts off-axis quads and breaks the
        // ray-cast coverage. The view matrix rows give a non-degenerate basis.
        right = camera_right();
        up = camera_up();
    } else {
        let view_dir = normalize(camera.camera_position - world_center);
        let world_up = vec3<f32>(0.0, 1.0, 0.0);
        right = normalize(cross(world_up, view_dir));
        up = normalize(cross(view_dir, right));
    }
    
    // Create camera-facing billboard quad
    // Pad quad beyond sphere radius so edge fragments (where rim is strongest)
    // are not clipped by the quad boundary, especially under perspective projection.
    let quad_padding = 1.15;
    let world_offset = (input.quad_offset.x * right + input.quad_offset.y * up) * input.radius * quad_padding;
    let quad_world_pos = world_center + world_offset;
    
    // Transform to clip space
    output.clip_position = camera.view_proj * vec4<f32>(quad_world_pos, 1.0);
    
    // Pass through data needed for fragment shader
    output.world_center = world_center;
    output.world_position = quad_world_pos;  // World position of the quad vertex
    output.radius = input.radius;
    output.albedo = input.albedo;
    output.roughness = input.roughness;
    output.metallic = input.metallic;
    output.quad_uv = input.quad_offset; // -1 to 1 range for ray-casting
    output.rim_color = input.rim_color;

    return output;
}

// ============================================================================
// Per-sample coverage (doc/design_viewport_antialiasing.md D4)
//
// Shading runs once per pixel, coverage is decided per sample: one hit-only
// ray per sample position, written as `@builtin(sample_mask)`. Rust twins in
// `impostor_coverage.rs`; this block is repeated in `bond_impostor.wgsl` and
// `transparent_impostor.wgsl`, and a change to one is made to all four.
// ============================================================================

// The render target's sample count (1 or 4), set per pipeline by
// `PipelineSet::new`.
override SAMPLE_COUNT: u32 = 1u;

// Largest distance of a 4x sample from the pixel centre, in pixels.
// Twin: `SAMPLE_FOOTPRINT_PX`.
const SAMPLE_FOOTPRINT_PX: f32 = 0.4;

// WebGPU's standard 4x sample positions relative to the pixel centre, in
// pixels, y down; sample i is bit i of the sample mask.
// Twin: `sample_offsets(4)`.
fn sample_offset_4x(i: u32) -> vec2<f32> {
    var offsets = array<vec2<f32>, 4>(
        vec2<f32>(-0.125, -0.375),
        vec2<f32>(0.375, -0.125),
        vec2<f32>(-0.375, 0.125),
        vec2<f32>(0.125, 0.375),
    );
    return offsets[i];
}

// The point on the billboard quad under a sample. Twin: `sample_point`.
fn sample_point(world_position: vec3<f32>, dwx: vec3<f32>, dwy: vec3<f32>, offset: vec2<f32>) -> vec3<f32> {
    return world_position + dwx * offset.x + dwy * offset.y;
}

// The early-out limit on |quad_uv|, widened by the sample footprint so no
// covered sample is lost; exactly 1 for one sample. Twin: `quad_uv_limit`.
fn quad_uv_limit(duv_dx: vec2<f32>, duv_dy: vec2<f32>) -> f32 {
    if SAMPLE_COUNT > 1u {
        return 1.0 + SAMPLE_FOOTPRINT_PX * sqrt(dot(duv_dx, duv_dx) + dot(duv_dy, duv_dy));
    }
    return 1.0;
}

// How far, in world units, a sample ray can pass from the pixel-centre ray.
// Every 4x offset has |x| + |y| = 0.5, so a sample's quad point is at most
// half the larger pixel step from the centre's, and the ray moves by at most
// that where it passes the impostor; 1.25 to spare. Twin: `sample_reach`.
fn sample_reach(dwx: vec3<f32>, dwy: vec3<f32>) -> f32 {
    return 1.25 * 0.5 * sqrt(max(dot(dwx, dwx), dot(dwy, dwy)));
}

struct Ray {
    origin: vec3<f32>,
    dir: vec3<f32>,
}

// The view ray through the quad point `p`.
fn view_ray(p: vec3<f32>, radius: f32) -> Ray {
    var ray: Ray;
    if camera.is_orthographic > 0.5 {
        // Parallel rays. The quad plane passes through the sphere center, so
        // start the ray one diameter behind the fragment to keep both
        // intersections at positive t for the selection logic below.
        ray.dir = -camera_backward();
        ray.origin = p - ray.dir * (radius * 2.0);
    } else {
        ray.origin = camera.camera_position;
        ray.dir = normalize(p - ray.origin);
    }
    return ray;
}

// The nearest non-negative t at which the ray meets the sphere, negative on a
// miss. Twin: `sphere_hit`.
fn sphere_hit(origin: vec3<f32>, dir: vec3<f32>, center: vec3<f32>, radius: f32) -> f32 {
    let oc = center - origin;
    let tca = dot(oc, dir);
    // Sphere behind the ray origin.
    if tca < 0.0 {
        return -1.0;
    }
    let d2 = dot(oc, oc) - tca * tca;
    let radius2 = radius * radius;
    if d2 > radius2 {
        return -1.0;
    }
    let thc = sqrt(radius2 - d2);
    let t0 = tca - thc;
    let t1 = tca + thc;
    // Use the nearest positive intersection.
    let t = select(t1, t0, t0 > 0.0);
    if t < 0.0 {
        return -1.0;
    }
    return t;
}

// How far the ray, as a line, passes outside the sphere's silhouette:
// negative inside it. Huge for a sphere behind the ray's origin. Lets a
// fragment far from the silhouette skip the per-sample tests.
// Twin: `sphere_ray_distance`.
fn sphere_ray_distance(origin: vec3<f32>, dir: vec3<f32>, center: vec3<f32>, radius: f32) -> f32 {
    let oc = center - origin;
    let tca = dot(oc, dir);
    if tca < 0.0 {
        return 3.0e38;
    }
    return sqrt(max(dot(oc, oc) - tca * tca, 0.0)) - radius;
}

struct AtomFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @builtin(sample_mask) sample_mask: u32,
    @location(0) color: vec4<f32>,
}

@fragment
fn fs_main(input: AtomImpostorVertexOutput) -> AtomFragmentOutput {
    // Derivatives first: they must be taken in uniform control flow, before
    // anything can discard.
    let dwx = dpdxFine(input.world_position);
    let dwy = dpdyFine(input.world_position);
    let duv_dx = dpdxFine(input.quad_uv);
    let duv_dy = dpdyFine(input.quad_uv);

    // Early discard for fragments outside the padded quad circle.
    // quad_uv ranges from -1..1 across the padded quad, so the sphere edge
    // is at dist_sq = 1/quad_padding^2. Discard beyond the quad circle.
    let uv_limit = quad_uv_limit(duv_dx, duv_dy);
    let dist_sq = dot(input.quad_uv, input.quad_uv);
    if dist_sq > uv_limit * uv_limit {
        discard;
    }

    // Coverage: one hit test per sample. The shading ray is the pixel
    // centre's if it hits, otherwise the first sample's that does.
    var ray = view_ray(input.world_position, input.radius);
    var t = sphere_hit(ray.origin, ray.dir, input.world_center, input.radius);
    var mask = 0u;
    if SAMPLE_COUNT == 1u {
        mask = select(0u, 1u, t >= 0.0);
    } else {
        // Far outside the silhouette every sample misses; far inside it every
        // sample hits. Only fragments near it test each sample.
        let reach = sample_reach(dwx, dwy);
        let distance = sphere_ray_distance(ray.origin, ray.dir, input.world_center, input.radius);
        if distance > reach {
            discard;
        }
        if distance < -reach && t >= 0.0 {
            mask = 0xFu;
        }
        let all_hit = mask != 0u;
        for (var i = 0u; i < 4u && !all_hit; i++) {
            let p = sample_point(input.world_position, dwx, dwy, sample_offset_4x(i));
            let sample_ray = view_ray(p, input.radius);
            let sample_t = sphere_hit(sample_ray.origin, sample_ray.dir, input.world_center, input.radius);
            if sample_t >= 0.0 {
                mask |= 1u << i;
                if t < 0.0 {
                    ray = sample_ray;
                    t = sample_t;
                }
            }
        }
    }
    if mask == 0u {
        discard;
    }

    // Calculate hit point and normal (following reference)
    let hit_point = ray.origin + t * ray.dir;
    let world_normal = normalize(hit_point - input.world_center);

    // Compute depth from the actual hit point
    let hit_clip = camera.view_proj * vec4<f32>(hit_point, 1.0);
    let depth = hit_clip.z / hit_clip.w;

    // Rim highlight: blend rim color into albedo before PBR so the rim is lit
    let V = camera_view_vector(hit_point);
    let NdotV = max(dot(world_normal, V), 0.0);
    let rim_start = 0.18;
    let rim_full = 0.22;
    let rim_factor = smoothstep(rim_start, rim_full, 1.0 - NdotV);
    let rim_blend = rim_factor * input.rim_color.a;
    let rim_albedo = mix(input.albedo, input.rim_color.rgb, rim_blend);

    // PBR lighting (returns linear HDR)
    var color = calculate_pbr_lighting(
        hit_point,
        world_normal,
        rim_albedo,
        input.roughness,
        input.metallic
    );

    // Tone mapping and gamma correction
    color = color / (color + vec3(1.0));
    color = pow(color, vec3(1.0 / 2.2));

    var output: AtomFragmentOutput;
    output.depth = depth;
    output.sample_mask = mask;
    output.color = vec4<f32>(color, 1.0);
    return output;
}
