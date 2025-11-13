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
    // Ray-cylinder intersection approach (mathematically correct)
    
    // Step 1: Set up ray
    let ray_origin = camera.camera_position;
    let ray_dir = normalize(input.world_position - ray_origin);
    
    // Step 2: Define cylinder parameters
    let cylinder_start = input.world_start;
    let cylinder_end = input.world_end;
    let cylinder_axis = normalize(cylinder_end - cylinder_start);
    let cylinder_length = length(cylinder_end - cylinder_start);
    let cylinder_radius = input.radius;
    
    // Step 3: Ray-infinite cylinder intersection
    // Vector from ray origin to cylinder start
    let oc = ray_origin - cylinder_start;
    
    // Project ray direction and oc onto plane perpendicular to cylinder axis
    let ray_perp = ray_dir - dot(ray_dir, cylinder_axis) * cylinder_axis;
    let oc_perp = oc - dot(oc, cylinder_axis) * cylinder_axis;
    
    // Solve quadratic equation: at² + bt + c = 0
    let a = dot(ray_perp, ray_perp);
    let b = 2.0 * dot(oc_perp, ray_perp);
    let c = dot(oc_perp, oc_perp) - cylinder_radius * cylinder_radius;
    
    let discriminant = b * b - 4.0 * a * c;
    
    // No intersection if discriminant is negative
    if discriminant < 0.0 {
        discard;
    }
    
    // Step 4: Find intersection points
    let sqrt_discriminant = sqrt(discriminant);
    let t1 = (-b - sqrt_discriminant) / (2.0 * a);
    let t2 = (-b + sqrt_discriminant) / (2.0 * a);
    
    // Choose the nearest positive intersection
    var t = t1;
    if t1 < 0.0 {
        t = t2;
    }
    if t < 0.0 {
        discard;
    }
    
    // Step 5: Calculate hit point and check cylinder bounds
    let hit_point = ray_origin + t * ray_dir;
    let hit_to_start = hit_point - cylinder_start;
    let projection_length = dot(hit_to_start, cylinder_axis);
    
    // Check if hit point is within cylinder length
    if projection_length < 0.0 || projection_length > cylinder_length {
        discard;
    }
    
    // Step 6: Calculate surface normal
    let axis_point = cylinder_start + projection_length * cylinder_axis;
    let surface_normal = normalize(hit_point - axis_point);
    
    // Step 7: Calculate depth
    let view_pos = camera.view_matrix * vec4<f32>(hit_point, 1.0);
    let clip_pos = camera.proj_matrix * view_pos;
    let depth = clip_pos.z / clip_pos.w;
    
    // Step 8: Simple lighting (can be upgraded to PBR later)
    let light_dir = normalize(-camera.head_light_dir);
    let light_intensity = max(dot(surface_normal, light_dir), 0.2); // Ambient minimum
    
    var output: BondFragmentOutput;
    output.depth = depth;
    output.color = vec4<f32>(input.color * light_intensity, 1.0);
    return output;
}
