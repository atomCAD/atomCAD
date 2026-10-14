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

struct BondImpostorVertexInput {
    @location(0) start_position: vec3<f32>,
    @location(1) end_position: vec3<f32>,
    @location(2) quad_offset: vec2<f32>,
    @location(3) radius: f32,
    @location(4) color: vec3<f32>,
}

struct BondImpostorVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_start: vec3<f32>,
    @location(1) world_end: vec3<f32>,
    @location(2) radius: f32,
    @location(3) color: vec3<f32>,
    @location(4) quad_uv: vec2<f32>, // For ray-casting
    @location(5) world_position: vec3<f32>, // World position of this fragment
}

// ============================================================================
// Camera Helper Functions
// ============================================================================

// Points from the scene toward the eye (right-handed view space +Z),
// extracted from the view matrix rows.
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
// Math Helper Functions
// ============================================================================

fn length_squared(v: vec3<f32>) -> f32 {
    return dot(v, v);
}

// ============================================================================
// Ray-Cylinder Intersection
// ============================================================================

struct CylinderIntersection {
    hit: bool,
    distance: f32,
    normal: vec3<f32>,
    position: vec3<f32>,
}

fn ray_cylinder_intersect(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
    cylinder_start: vec3<f32>,
    cylinder_end: vec3<f32>,
    radius: f32
) -> CylinderIntersection {
    var result: CylinderIntersection;
    result.hit = false;
    
    let cylinder_axis = cylinder_end - cylinder_start;
    let cylinder_length = length(cylinder_axis);
    let cylinder_dir = cylinder_axis / cylinder_length;
    
    // Vector from cylinder start to ray origin
    let oc = ray_origin - cylinder_start;
    
    // Project ray direction and origin-to-start onto cylinder axis
    let ray_dot_axis = dot(ray_dir, cylinder_dir);
    let oc_dot_axis = dot(oc, cylinder_dir);
    
    // Components perpendicular to cylinder axis
    let ray_perp = ray_dir - ray_dot_axis * cylinder_dir;
    let oc_perp = oc - oc_dot_axis * cylinder_dir;
    
    // Solve quadratic equation for infinite cylinder intersection
    let a = dot(ray_perp, ray_perp);
    let b = 2.0 * dot(oc_perp, ray_perp);
    let c = dot(oc_perp, oc_perp) - radius * radius;
    
    let discriminant = b * b - 4.0 * a * c;
    
    if (discriminant < 0.0) {
        return result; // No intersection
    }
    
    let sqrt_discriminant = sqrt(discriminant);
    let t1 = (-b - sqrt_discriminant) / (2.0 * a);
    let t2 = (-b + sqrt_discriminant) / (2.0 * a);
    
    // Choose the closest positive intersection
    var t = t1;
    if (t1 < 0.0) {
        t = t2;
    }
    if (t < 0.0) {
        return result; // Both intersections behind ray origin
    }
    
    // Calculate intersection point
    let intersection_point = ray_origin + t * ray_dir;
    
    // Check if intersection is within cylinder length
    let intersection_to_start = intersection_point - cylinder_start;
    let projection_length = dot(intersection_to_start, cylinder_dir);
    
    if (projection_length < 0.0 || projection_length > cylinder_length) {
        return result; // Intersection outside cylinder caps
    }
    
    // Calculate surface normal (perpendicular to cylinder axis)
    let closest_point_on_axis = cylinder_start + projection_length * cylinder_dir;
    let surface_normal = normalize(intersection_point - closest_point_on_axis);
    
    result.hit = true;
    result.distance = t;
    result.position = intersection_point;
    result.normal = surface_normal;
    
    return result;
}

// ============================================================================
// Simple Lighting (since bonds typically use simpler shading)
// ============================================================================

fn calculate_bond_lighting(
    world_position: vec3<f32>,
    normal: vec3<f32>, 
    color: vec3<f32>
) -> vec3<f32> {
    let N = normalize(normal);
    let V = camera_view_vector(world_position);
    let L = normalize(-camera.head_light_dir);

    // Simple Lambertian diffuse + ambient
    let NdotL = max(dot(N, L), 0.0);
    let diffuse = color * NdotL;
    
    // Simple specular highlight
    let R = reflect(-L, N);
    let VdotR = max(dot(V, R), 0.0);
    let specular = vec3<f32>(0.3) * pow(VdotR, 32.0);
    
    // Ambient lighting
    let ambient = color * 0.2;
    
    var final_color = ambient + diffuse * 0.8 + specular;
    
    // Simple tone mapping and gamma correction
    final_color = final_color / (final_color + vec3(1.0));
    final_color = pow(final_color, vec3(1.0/2.2));
    
    return final_color;
}

// ============================================================================
// PBR Helper Functions (copied from atom_impostor.wgsl)
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
	
    color = color / (color + vec3(1.0)); // Tone mapping
    color = pow(color, vec3(1.0/2.2)); // Gamma correction

    return color;
}

// ============================================================================
// Bond Impostor Shaders
// ============================================================================

@vertex
fn vs_main(input: BondImpostorVertexInput) -> BondImpostorVertexOutput {
    var output: BondImpostorVertexOutput;
    
    // Transform bond endpoints to world space
    let world_start = (model.model_matrix * vec4<f32>(input.start_position, 1.0)).xyz;
    let world_end = (model.model_matrix * vec4<f32>(input.end_position, 1.0)).xyz;
    
    // Calculate bond center and direction
    let bond_center = (world_start + world_end) * 0.5;
    let bond_vector = world_end - world_start;
    let bond_length = length(bond_vector);
    let bond_dir = bond_vector / bond_length;
    
    // Calculate camera forward direction (from bond center to camera).
    // Orthographic projection has no per-bond eye direction; use the shared one.
    var camera_forward: vec3<f32>;
    if camera.is_orthographic > 0.5 {
        camera_forward = camera_backward();
    } else {
        camera_forward = normalize(camera.camera_position - bond_center);
    }
    
    // Create billboard vectors - right is perpendicular to both bond and camera direction
    let right = normalize(cross(bond_dir, camera_forward));
    // Bond direction is used as the "up" axis for the quad
    
    // Calculate the quad size following the reference approach
    let quad_width = input.radius * 2.5; // Extra width for proper coverage
    let quad_height = bond_length * 1.1; // Extra height for coverage
    
    // Calculate world position of this quad vertex
    // x offset uses right vector, y offset uses bond direction
    let quad_world_pos = bond_center + 
                        input.quad_offset.x * right * quad_width +
                        input.quad_offset.y * bond_dir * quad_height * 0.5;
    
    // Transform to clip space
    output.clip_position = camera.view_proj * vec4<f32>(quad_world_pos, 1.0);
    
    // Pass through data for fragment shader
    output.world_start = world_start;
    output.world_end = world_end;
    output.radius = input.radius;
    output.color = input.color;
    output.quad_uv = input.quad_offset;
    output.world_position = quad_world_pos;
    
    return output;
}

// Signed distance function for 2D capsule (cylinder with rounded ends)
fn sd_capsule(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

// ============================================================================
// Per-sample coverage (doc/design_viewport_antialiasing.md D4)
//
// Shading runs once per pixel, coverage is decided per sample: one hit-only
// ray per sample position, written as `@builtin(sample_mask)`. Rust twins in
// `impostor_coverage.rs`; this block is repeated in `atom_impostor.wgsl` and
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
        // Parallel rays. The quad plane contains the cylinder axis, so start
        // the ray one diameter behind the fragment to keep intersections at
        // positive t for the selection logic below.
        ray.dir = -camera_backward();
        ray.origin = p - ray.dir * (radius * 2.0);
    } else {
        ray.origin = camera.camera_position;
        ray.dir = normalize(p - ray.origin);
    }
    return ray;
}

// The nearest non-negative t at which the ray meets the open cylinder (clipped
// flat at both ends), negative on a miss. Twin: `cylinder_hit`.
fn cylinder_hit(origin: vec3<f32>, dir: vec3<f32>, start: vec3<f32>, end: vec3<f32>, radius: f32) -> f32 {
    let cylinder_axis = normalize(end - start);
    let cylinder_length = length(end - start);

    // Vector from ray origin to cylinder start
    let oc = origin - start;

    // Project ray direction and oc onto plane perpendicular to cylinder axis
    let ray_perp = dir - dot(dir, cylinder_axis) * cylinder_axis;
    let oc_perp = oc - dot(oc, cylinder_axis) * cylinder_axis;

    // Solve quadratic equation: at² + bt + c = 0
    let a = dot(ray_perp, ray_perp);
    let b = 2.0 * dot(oc_perp, ray_perp);
    let c = dot(oc_perp, oc_perp) - radius * radius;

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return -1.0;
    }

    // Choose the nearest positive intersection
    let sqrt_discriminant = sqrt(discriminant);
    let t1 = (-b - sqrt_discriminant) / (2.0 * a);
    let t2 = (-b + sqrt_discriminant) / (2.0 * a);
    var t = t1;
    if t1 < 0.0 {
        t = t2;
    }
    if t < 0.0 {
        return -1.0;
    }

    // Check if hit point is within cylinder length
    let projection_length = dot(origin + t * dir - start, cylinder_axis);
    if projection_length < 0.0 || projection_length > cylinder_length {
        return -1.0;
    }
    return t;
}

// How far the ray, as a line, passes outside the infinite cylinder: negative
// inside it, and for a ray along the axis. Lets a fragment far from the
// bond's sides skip the per-sample tests. Twin: `cylinder_ray_distance`.
fn cylinder_ray_distance(origin: vec3<f32>, dir: vec3<f32>, start: vec3<f32>, end: vec3<f32>, radius: f32) -> f32 {
    let cylinder_axis = normalize(end - start);
    let oc = origin - start;
    let ray_perp = dir - dot(dir, cylinder_axis) * cylinder_axis;
    let oc_perp = oc - dot(oc, cylinder_axis) * cylinder_axis;
    let a = dot(ray_perp, ray_perp);
    if a < 1.0e-12 {
        return -radius;
    }
    let b = dot(oc_perp, ray_perp);
    return sqrt(max(dot(oc_perp, oc_perp) - b * b / a, 0.0)) - radius;
}

struct BondFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @builtin(sample_mask) sample_mask: u32,
    @location(0) color: vec4<f32>,
}

@fragment
fn fs_main(input: BondImpostorVertexOutput) -> BondFragmentOutput {
    // Derivatives first: they must be taken in uniform control flow, before
    // anything can discard.
    let dwx = dpdxFine(input.world_position);
    let dwy = dpdyFine(input.world_position);

    let cylinder_start = input.world_start;
    let cylinder_end = input.world_end;
    let cylinder_axis = normalize(cylinder_end - cylinder_start);

    // Coverage: one hit test per sample. The shading ray is the pixel
    // centre's if it hits, otherwise the first sample's that does.
    var ray = view_ray(input.world_position, input.radius);
    var t = cylinder_hit(ray.origin, ray.dir, cylinder_start, cylinder_end, input.radius);
    var mask = 0u;
    if SAMPLE_COUNT == 1u {
        mask = select(0u, 1u, t >= 0.0);
    } else {
        // Far outside the cylinder every sample misses. Only fragments near
        // its sides test each sample.
        let reach = sample_reach(dwx, dwy);
        if cylinder_ray_distance(ray.origin, ray.dir, cylinder_start, cylinder_end, input.radius) > reach {
            discard;
        }
        for (var i = 0u; i < 4u; i++) {
            let p = sample_point(input.world_position, dwx, dwy, sample_offset_4x(i));
            let sample_ray = view_ray(p, input.radius);
            let sample_t = cylinder_hit(sample_ray.origin, sample_ray.dir, cylinder_start, cylinder_end, input.radius);
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

    // Hit point and surface normal
    let hit_point = ray.origin + t * ray.dir;
    let projection_length = dot(hit_point - cylinder_start, cylinder_axis);
    let axis_point = cylinder_start + projection_length * cylinder_axis;
    let surface_normal = normalize(hit_point - axis_point);

    // Depth from the hit point
    let view_pos = camera.view_matrix * vec4<f32>(hit_point, 1.0);
    let clip_pos = camera.proj_matrix * view_pos;
    let depth = clip_pos.z / clip_pos.w;

    // PBR lighting using the correct surface normal
    let color = calculate_pbr_lighting(
        hit_point,
        surface_normal,
        input.color,
        0.5,  // roughness - moderate for bonds
        0.0   // metallic - bonds are non-metallic
    );

    var output: BondFragmentOutput;
    output.depth = depth;
    output.sample_mask = mask;
    output.color = vec4<f32>(color, 1.0);
    return output;
}
