// Unified transparent impostor shader (x-ray) — Phase 4 of
// `doc/design_xray_node.md`.
//
// One pipeline draws the single merged transparent mesh containing both ghost
// atoms (spheres) and ghost bonds (cylinders). Each vertex carries a `kind`
// field (0 = atom, 1 = bond) that selects the ray-cast branch. `kind` is
// constant across a quad, so the branch is uniform per primitive. Both branches
// write `@builtin(frag_depth)` from the true ray-hit point, so ghost impostors
// depth-test exactly against opaque geometry (which draws first with depth
// writes on). The fragment outputs `vec4(color, alpha)`; the pipeline blends
// with `ALPHA_BLENDING` and depth writes off.
//
// The atom branch is the sphere ray-cast + PBR shading from
// `atom_impostor.wgsl`; the bond branch is the cylinder ray-cast + PBR shading
// from `bond_impostor.wgsl`. Shared camera helpers are copied along.

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

struct TransparentImpostorVertexInput {
    @location(0) kind: u32,             // 0 = atom (sphere), 1 = bond (cylinder)
    @location(1) position_a: vec3<f32>, // atom: center; bond: start
    @location(2) position_b: vec3<f32>, // atom: unused;  bond: end
    @location(3) quad_offset: vec2<f32>,
    @location(4) radius: f32,
    @location(5) color: vec3<f32>,
    @location(6) alpha: f32,
    @location(7) roughness: f32,        // atom branch only
    @location(8) metallic: f32,         // atom branch only
    @location(9) rim_color: vec4<f32>,  // atom branch only
}

struct TransparentImpostorVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) kind: u32,
    @location(1) world_a: vec3<f32>,        // atom: center; bond: start
    @location(2) world_b: vec3<f32>,        // bond: end (atom: unused)
    @location(3) world_position: vec3<f32>, // world position of this quad vertex
    @location(4) radius: f32,
    @location(5) color: vec3<f32>,
    @location(6) quad_uv: vec2<f32>,        // -1..1 range for ray-casting
    @location(7) rim_color: vec4<f32>,
    @location(8) roughness: f32,
    @location(9) metallic: f32,
    @location(10) alpha: f32,
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
// PBR Helper Functions (shared by both branches)
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
    let ambient_light_color: vec3<f32> = vec3<f32>(0.2);
    let ambient_diffuse = ambient_light_color * albedo;

    // Specular ambient term (reflective contribution)
    let ambient_specular = ambient_light_color * fresnel_schlick(max(dot(V, N), 0.0), f0);

    // Blend ambient terms based on the material's metallic property
    let ambient = mix(ambient_diffuse, ambient_specular, metallic);

    var color = light_contribution * (diffuse + specular) + ambient;

    // Tone mapping and gamma correction
    color = color / (color + vec3(1.0));
    color = pow(color, vec3(1.0 / 2.2));

    return color;
}

// ============================================================================
// Vertex stage
// ============================================================================

@vertex
fn vs_main(input: TransparentImpostorVertexInput) -> TransparentImpostorVertexOutput {
    var output: TransparentImpostorVertexOutput;

    output.kind = input.kind;
    output.radius = input.radius;
    output.color = input.color;
    output.quad_uv = input.quad_offset;
    output.rim_color = input.rim_color;
    output.roughness = input.roughness;
    output.metallic = input.metallic;
    output.alpha = input.alpha;

    if input.kind == 0u {
        // ----- Atom (sphere) billboard: mirrors atom_impostor.wgsl -----
        let world_center = (model.model_matrix * vec4<f32>(input.position_a, 1.0)).xyz;

        var right: vec3<f32>;
        var up: vec3<f32>;
        if camera.is_orthographic > 0.5 {
            right = camera_right();
            up = camera_up();
        } else {
            let view_dir = normalize(camera.camera_position - world_center);
            let world_up = vec3<f32>(0.0, 1.0, 0.0);
            right = normalize(cross(world_up, view_dir));
            up = normalize(cross(view_dir, right));
        }

        // Pad quad beyond sphere radius so rim fragments are not clipped.
        let quad_padding = 1.15;
        let world_offset = (input.quad_offset.x * right + input.quad_offset.y * up)
            * input.radius * quad_padding;
        let quad_world_pos = world_center + world_offset;

        output.clip_position = camera.view_proj * vec4<f32>(quad_world_pos, 1.0);
        output.world_a = world_center;
        output.world_b = vec3<f32>(0.0, 0.0, 0.0);
        output.world_position = quad_world_pos;
    } else {
        // ----- Bond (cylinder) billboard: mirrors bond_impostor.wgsl -----
        let world_start = (model.model_matrix * vec4<f32>(input.position_a, 1.0)).xyz;
        let world_end = (model.model_matrix * vec4<f32>(input.position_b, 1.0)).xyz;

        let bond_center = (world_start + world_end) * 0.5;
        let bond_vector = world_end - world_start;
        let bond_length = length(bond_vector);
        let bond_dir = bond_vector / bond_length;

        var camera_forward: vec3<f32>;
        if camera.is_orthographic > 0.5 {
            camera_forward = camera_backward();
        } else {
            camera_forward = normalize(camera.camera_position - bond_center);
        }

        let right = normalize(cross(bond_dir, camera_forward));

        let quad_width = input.radius * 2.5;
        let quad_height = bond_length * 1.1;

        let quad_world_pos = bond_center
            + input.quad_offset.x * right * quad_width
            + input.quad_offset.y * bond_dir * quad_height * 0.5;

        output.clip_position = camera.view_proj * vec4<f32>(quad_world_pos, 1.0);
        output.world_a = world_start;
        output.world_b = world_end;
        output.world_position = quad_world_pos;
    }

    return output;
}

// ============================================================================
// Fragment stage
// ============================================================================

struct TransparentFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

// Sphere ray-cast + shading (atom branch).
fn shade_atom(input: TransparentImpostorVertexOutput) -> TransparentFragmentOutput {
    // Early discard for fragments outside the padded quad circle.
    let dist_sq = dot(input.quad_uv, input.quad_uv);
    if dist_sq > 1.0 {
        discard;
    }

    var ray_origin: vec3<f32>;
    var ray_dir: vec3<f32>;
    if camera.is_orthographic > 0.5 {
        ray_dir = -camera_backward();
        ray_origin = input.world_position - ray_dir * (input.radius * 2.0);
    } else {
        ray_origin = camera.camera_position;
        ray_dir = normalize(input.world_position - ray_origin);
    }

    let oc = input.world_a - ray_origin;
    let tca = dot(oc, ray_dir);
    if tca < 0.0 {
        discard;
    }

    let d2 = dot(oc, oc) - tca * tca;
    let radius2 = input.radius * input.radius;
    if d2 > radius2 {
        discard;
    }

    let thc = sqrt(radius2 - d2);
    let t0 = tca - thc;
    let t1 = tca + thc;
    let t = select(t1, t0, t0 > 0.0);
    if t < 0.0 {
        discard;
    }

    let hit_point = ray_origin + t * ray_dir;
    let world_normal = normalize(hit_point - input.world_a);

    let hit_clip = camera.view_proj * vec4<f32>(hit_point, 1.0);
    let depth = hit_clip.z / hit_clip.w;

    // Rim highlight: blend rim color into albedo before PBR so the rim is lit.
    let V = camera_view_vector(hit_point);
    let NdotV = max(dot(world_normal, V), 0.0);
    let rim_start = 0.18;
    let rim_full = 0.22;
    let rim_factor = smoothstep(rim_start, rim_full, 1.0 - NdotV);
    let rim_blend = rim_factor * input.rim_color.a;
    let rim_albedo = mix(input.color, input.rim_color.rgb, rim_blend);

    let color = calculate_pbr_lighting(
        hit_point,
        world_normal,
        rim_albedo,
        input.roughness,
        input.metallic
    );

    var output: TransparentFragmentOutput;
    output.depth = depth;
    output.color = vec4<f32>(color, input.alpha);
    return output;
}

// Cylinder ray-cast + shading (bond branch).
fn shade_bond(input: TransparentImpostorVertexOutput) -> TransparentFragmentOutput {
    var ray_origin: vec3<f32>;
    var ray_dir: vec3<f32>;
    if camera.is_orthographic > 0.5 {
        ray_dir = -camera_backward();
        ray_origin = input.world_position - ray_dir * (input.radius * 2.0);
    } else {
        ray_origin = camera.camera_position;
        ray_dir = normalize(input.world_position - ray_origin);
    }

    let cylinder_start = input.world_a;
    let cylinder_end = input.world_b;
    let cylinder_axis = normalize(cylinder_end - cylinder_start);
    let cylinder_length = length(cylinder_end - cylinder_start);
    let cylinder_radius = input.radius;

    let oc = ray_origin - cylinder_start;
    let ray_perp = ray_dir - dot(ray_dir, cylinder_axis) * cylinder_axis;
    let oc_perp = oc - dot(oc, cylinder_axis) * cylinder_axis;

    let a = dot(ray_perp, ray_perp);
    let b = 2.0 * dot(oc_perp, ray_perp);
    let c = dot(oc_perp, oc_perp) - cylinder_radius * cylinder_radius;

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        discard;
    }

    let sqrt_discriminant = sqrt(discriminant);
    let t1 = (-b - sqrt_discriminant) / (2.0 * a);
    let t2 = (-b + sqrt_discriminant) / (2.0 * a);

    var t = t1;
    if t1 < 0.0 {
        t = t2;
    }
    if t < 0.0 {
        discard;
    }

    let hit_point = ray_origin + t * ray_dir;
    let hit_to_start = hit_point - cylinder_start;
    let projection_length = dot(hit_to_start, cylinder_axis);
    if projection_length < 0.0 || projection_length > cylinder_length {
        discard;
    }

    let axis_point = cylinder_start + projection_length * cylinder_axis;
    let surface_normal = normalize(hit_point - axis_point);

    let view_pos = camera.view_matrix * vec4<f32>(hit_point, 1.0);
    let clip_pos = camera.proj_matrix * view_pos;
    let depth = clip_pos.z / clip_pos.w;

    let color = calculate_pbr_lighting(
        hit_point,
        surface_normal,
        input.color,
        0.5, // roughness - moderate for bonds
        0.0  // metallic - bonds are non-metallic
    );

    var output: TransparentFragmentOutput;
    output.depth = depth;
    output.color = vec4<f32>(color, input.alpha);
    return output;
}

@fragment
fn fs_main(input: TransparentImpostorVertexOutput) -> TransparentFragmentOutput {
    if input.kind == 0u {
        return shade_atom(input);
    }
    return shade_bond(input);
}
