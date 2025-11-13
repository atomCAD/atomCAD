const PI: f32 = 3.14159265359;

struct CameraUniform {
  view_proj: mat4x4<f32>,
  view_matrix: mat4x4<f32>,
  proj_matrix: mat4x4<f32>,
  camera_position: vec3<f32>,
  head_light_dir: vec3<f32>,
  is_orthographic: f32,      // 1.0 = orthographic, 0.0 = perspective
  ortho_half_height: f32,    // Half height for orthographic projection (for zoom level)
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
    let V = normalize(camera.camera_position - world_position);
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
// Atom Impostor Shaders
// ============================================================================

@vertex
fn vs_main(input: AtomImpostorVertexInput) -> AtomImpostorVertexOutput {
    var output: AtomImpostorVertexOutput;
    
    // Transform atom center to world space
    let world_center = (model.model_matrix * vec4<f32>(input.center_position, 1.0)).xyz;
    
    // Extract camera right and up vectors from view matrix
    // The view_proj matrix is view * projection, so we need to extract from the view part
    // We can get the inverse view matrix vectors by using the transpose approach
    // For now, let's calculate them manually from camera position
    let view_dir = normalize(camera.camera_position - world_center);
    
    // Use the same robust method as before but simpler
    let world_up = vec3<f32>(0.0, 1.0, 0.0);
    let right = normalize(cross(world_up, view_dir));
    let up = normalize(cross(view_dir, right));
    
    // Create camera-facing billboard quad
    let world_offset = (input.quad_offset.x * right + input.quad_offset.y * up) * input.radius;
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
    
    return output;
}

struct AtomFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

@fragment
fn fs_main(input: AtomImpostorVertexOutput) -> AtomFragmentOutput {
    // Early discard for fragments outside the circle (following reference)
    let dist_sq = dot(input.quad_uv, input.quad_uv);
    if dist_sq > 1.0 {
        discard;
    }

    // Ray-sphere intersection using view space for better precision (following reference)
    let ray_origin = camera.camera_position;
    let ray_dir = normalize(input.world_position - ray_origin);

    // Optimized ray-sphere intersection (following reference)
    let oc = input.world_center - ray_origin;
    let tca = dot(oc, ray_dir);

    // Early out if sphere is behind camera (following reference)
    if tca < 0.0 {
        discard;
    }

    let d2 = dot(oc, oc) - tca * tca;
    let radius2 = input.radius * input.radius;

    if d2 > radius2 {
        discard;
    }

    // Calculate intersection (following reference)
    let thc = sqrt(radius2 - d2);
    let t0 = tca - thc;
    let t1 = tca + thc;

    // Use the nearest positive intersection (following reference)
    let t = select(t1, t0, t0 > 0.0);
    if t < 0.0 {
        discard;
    }

    // Calculate the sphere surface z-offset (following reference)
    let z_normalized = sqrt(1.0 - dist_sq);
    let z_offset = z_normalized * input.radius;

    // Adjust clip position using projection matrix (following reference)
    let proj_z_col = camera.proj_matrix[2];
    let center_clip = camera.view_proj * vec4<f32>(input.world_center, 1.0);
    let adjusted_clip_pos = center_clip + proj_z_col * z_offset;
    let depth = adjusted_clip_pos.z / adjusted_clip_pos.w;

    // Calculate hit point and normal (following reference)
    let hit_point = ray_origin + t * ray_dir;
    let world_normal = normalize(hit_point - input.world_center);

    // Calculate PBR lighting using the shared function
    let color = calculate_pbr_lighting(
        hit_point,
        world_normal,
        input.albedo,
        input.roughness,
        input.metallic
    );

    var output: AtomFragmentOutput;
    output.depth = depth;
    output.color = vec4<f32>(color, 1.0);
    return output;
}
