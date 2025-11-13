const PI: f32 = 3.14159265359;

struct CameraUniform {
  view_proj: mat4x4<f32>,
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
    @location(1) radius: f32,
    @location(2) albedo: vec3<f32>,
    @location(3) roughness: f32,
    @location(4) metallic: f32,
    @location(5) quad_uv: vec2<f32>, // For ray-casting (-1 to 1 range)
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
    
    // Transform center to clip space to get the base position
    let center_clip = camera.view_proj * vec4<f32>(world_center, 1.0);
    
    // Calculate screen-space radius (perspective-correct scaling)
    // Use the projection matrix's Y-scale component for consistent sizing
    let screen_radius = input.radius * camera.view_proj[1][1] / center_clip.w;
    
    // Expand the quad based on the offset and screen-space radius
    output.clip_position = center_clip + vec4<f32>(input.quad_offset * screen_radius, 0.0, 0.0);
    
    // Pass through data needed for fragment shader
    output.world_center = world_center;
    output.radius = input.radius;
    output.albedo = input.albedo;
    output.roughness = input.roughness;
    output.metallic = input.metallic;
    output.quad_uv = input.quad_offset; // -1 to 1 range for ray-casting
    
    return output;
}

@fragment
fn fs_main(input: AtomImpostorVertexOutput) -> @location(0) vec4<f32> {
    // Ray-sphere intersection using quad UV coordinates
    let uv_length_sq = dot(input.quad_uv, input.quad_uv);
    
    // Discard fragments outside the sphere
    if (uv_length_sq > 1.0) {
        discard;
    }
    
    // Calculate the Z component of the sphere surface normal
    // For a unit sphere: x² + y² + z² = 1, so z = sqrt(1 - x² - y²)
    let z = sqrt(1.0 - uv_length_sq);
    
    // The surface normal in sphere-local space (pointing outward)
    let local_normal = vec3<f32>(input.quad_uv.x, input.quad_uv.y, z);
    
    // Transform normal to world space (sphere is axis-aligned, so no rotation needed)
    let world_normal = normalize(local_normal);
    
    // Calculate the actual world position of this fragment on the sphere surface
    let surface_offset = local_normal * input.radius;
    let world_position = input.world_center + surface_offset;
    
    // Calculate PBR lighting using the shared function
    let color = calculate_pbr_lighting(
        world_position,
        world_normal,
        input.albedo,
        input.roughness,
        input.metallic
    );
    
    return vec4<f32>(color, 1.0);
}
