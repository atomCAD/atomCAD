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
    let V = normalize(camera.camera_position - world_position);
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
    
    // Calculate camera forward direction (from bond center to camera)
    let camera_forward = normalize(camera.camera_position - bond_center);
    
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

struct BondFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

@fragment
fn fs_main(input: BondImpostorVertexOutput) -> BondFragmentOutput {
    // EXACT reference shader approach - follow their implementation exactly
    // Calculate local bond axis coordinates
    let p = input.quad_uv;

    // Calculate signed distance to capsule (EXACT reference values)
    let capsule_dist = sd_capsule(p, vec2<f32>(0.0, -1.0), vec2<f32>(0.0, 1.0), 0.5);

    // Discard fragments outside the capsule with a small antialiasing border
    if capsule_dist > 0.0 {
        discard;
    }

    // Calculate closest point to camera ray on the bond axis using an approximation
    // We use the UV y-coordinate (-1 to 1) to interpolate between atom centers
    let t = (input.quad_uv.y + 1.0) * 0.5; // Map from [-1,1] to [0,1]
    let closest_point = mix(input.world_start, input.world_end, t);

    // Calculate point on surface of the bond cylinder
    // (cylinder radius * normalized distance from center)
    let radial_offset = -capsule_dist * input.radius;

    // We need to calculate appropriate depth adjustment based on cylinder surface
    // Use view.clip_from_view to properly project the depth (EXACT reference approach)
    let view_pos = camera.view_matrix * vec4<f32>(input.world_position, 1.0);
    var adjusted_view_pos = view_pos;
    adjusted_view_pos.z += radial_offset * 0.5; // Adjust depth by scaled radial offset

    let clip_pos = camera.proj_matrix * adjusted_view_pos;
    let depth = clip_pos.z / clip_pos.w;

    // Calculate lighting - simple gradient from center outward (EXACT reference approach)
    let edge_factor = smoothstep(-0.1, 0.0, capsule_dist);
    let light_factor = 1.0 - edge_factor * 0.3;

    // Apply lighting to bond color (EXACT reference approach)
    var output: BondFragmentOutput;
    output.depth = depth;
    output.color = vec4<f32>(input.color * light_factor, 1.0);
    return output;
}
