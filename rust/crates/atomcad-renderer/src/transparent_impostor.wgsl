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
// from `bond_impostor.wgsl`. Shared camera helpers are copied along; the
// lighting is `lighting.wgsl`'s, as in every lit shader.

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

// ---- lighting ----
// `lighting.wgsl` is inserted here (`pipelines::compose_lit_shader`):
// `camera_backward`, `camera_view_vector`, `light_surface`, `to_display`.

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
// Per-sample coverage (doc/design_viewport_antialiasing.md D4)
//
// Shading runs once per pixel, coverage is decided per sample: one hit-only
// ray per sample position, written as `@builtin(sample_mask)`. A ghost is
// blended into each covered sample and the resolve averages, which is correct
// coverage for a blended surface too. Rust twins in `impostor_coverage.rs`;
// this block is repeated in `atom_impostor.wgsl` and `bond_impostor.wgsl`, and
// a change to one is made to all four.
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

// The view ray through the quad point `p`. The quad plane contains the
// sphere center / cylinder axis, so in orthographic the ray starts one
// diameter behind the fragment to keep intersections at positive t.
fn view_ray(p: vec3<f32>, radius: f32) -> Ray {
    var ray: Ray;
    if camera.is_orthographic > 0.5 {
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
    let t = select(t1, t0, t0 > 0.0);
    if t < 0.0 {
        return -1.0;
    }
    return t;
}

// The nearest non-negative t at which the ray meets the open cylinder (clipped
// flat at both ends), negative on a miss. Twin: `cylinder_hit`.
fn cylinder_hit(origin: vec3<f32>, dir: vec3<f32>, start: vec3<f32>, end: vec3<f32>, radius: f32) -> f32 {
    let cylinder_axis = normalize(end - start);
    let cylinder_length = length(end - start);

    let oc = origin - start;
    let ray_perp = dir - dot(dir, cylinder_axis) * cylinder_axis;
    let oc_perp = oc - dot(oc, cylinder_axis) * cylinder_axis;

    let a = dot(ray_perp, ray_perp);
    let b = 2.0 * dot(oc_perp, ray_perp);
    let c = dot(oc_perp, oc_perp) - radius * radius;

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return -1.0;
    }

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

    let projection_length = dot(origin + t * dir - start, cylinder_axis);
    if projection_length < 0.0 || projection_length > cylinder_length {
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

// ============================================================================
// Fragment stage
// ============================================================================

struct TransparentFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @builtin(sample_mask) sample_mask: u32,
    @location(0) color: vec4<f32>,
}

// Screen-space derivatives of the fragment inputs, taken in `fs_main` before
// the branch on `kind` and before anything can discard.
struct Derivatives {
    dwx: vec3<f32>,
    dwy: vec3<f32>,
    duv_dx: vec2<f32>,
    duv_dy: vec2<f32>,
}

// Sphere ray-cast + shading (atom branch).
fn shade_atom(input: TransparentImpostorVertexOutput, d: Derivatives) -> TransparentFragmentOutput {
    // Early discard for fragments outside the padded quad circle.
    let uv_limit = quad_uv_limit(d.duv_dx, d.duv_dy);
    let dist_sq = dot(input.quad_uv, input.quad_uv);
    if dist_sq > uv_limit * uv_limit {
        discard;
    }

    // Coverage: one hit test per sample. The shading ray is the pixel
    // centre's if it hits, otherwise the first sample's that does.
    var ray = view_ray(input.world_position, input.radius);
    var t = sphere_hit(ray.origin, ray.dir, input.world_a, input.radius);
    var mask = 0u;
    if SAMPLE_COUNT == 1u {
        mask = select(0u, 1u, t >= 0.0);
    } else {
        // Far outside the silhouette every sample misses; far inside it every
        // sample hits. Only fragments near it test each sample.
        let reach = sample_reach(d.dwx, d.dwy);
        let distance = sphere_ray_distance(ray.origin, ray.dir, input.world_a, input.radius);
        if distance > reach {
            discard;
        }
        if distance < -reach && t >= 0.0 {
            mask = 0xFu;
        }
        let all_hit = mask != 0u;
        for (var i = 0u; i < 4u && !all_hit; i++) {
            let p = sample_point(input.world_position, d.dwx, d.dwy, sample_offset_4x(i));
            let sample_ray = view_ray(p, input.radius);
            let sample_t = sphere_hit(sample_ray.origin, sample_ray.dir, input.world_a, input.radius);
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

    let shaded = shade_atom_hit(input, ray, t);
    var output: TransparentFragmentOutput;
    output.depth = shaded.depth;
    output.sample_mask = mask;
    output.color = shaded.color;
    return output;
}

// The colour and depth of a surface hit. Shared by `fs_main` and
// `fs_main_per_sample` (D5), so the two cannot disagree about the surface.
struct Shaded {
    color: vec4<f32>,
    depth: f32,
}

// Shading of the ghost sphere where `ray` meets it at `t`.
fn shade_atom_hit(input: TransparentImpostorVertexOutput, ray: Ray, t: f32) -> Shaded {
    let hit_point = ray.origin + t * ray.dir;
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

    let color = to_display(light_surface(
        hit_point,
        world_normal,
        rim_albedo,
        input.roughness,
        input.metallic,
        FULLY_EXPOSED,
    ));

    var shaded: Shaded;
    shaded.depth = depth;
    shaded.color = vec4<f32>(color, input.alpha);
    return shaded;
}

// Cylinder ray-cast + shading (bond branch).
fn shade_bond(input: TransparentImpostorVertexOutput, d: Derivatives) -> TransparentFragmentOutput {
    let cylinder_start = input.world_a;
    let cylinder_end = input.world_b;

    // Coverage: one hit test per sample, as in the atom branch.
    var ray = view_ray(input.world_position, input.radius);
    var t = cylinder_hit(ray.origin, ray.dir, cylinder_start, cylinder_end, input.radius);
    var mask = 0u;
    if SAMPLE_COUNT == 1u {
        mask = select(0u, 1u, t >= 0.0);
    } else {
        // Far outside the cylinder every sample misses. Only fragments near
        // its sides test each sample.
        let reach = sample_reach(d.dwx, d.dwy);
        if cylinder_ray_distance(ray.origin, ray.dir, cylinder_start, cylinder_end, input.radius) > reach {
            discard;
        }
        for (var i = 0u; i < 4u; i++) {
            let p = sample_point(input.world_position, d.dwx, d.dwy, sample_offset_4x(i));
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

    let shaded = shade_bond_hit(input, ray, t);
    var output: TransparentFragmentOutput;
    output.depth = shaded.depth;
    output.sample_mask = mask;
    output.color = shaded.color;
    return output;
}

// Shading of the ghost cylinder where `ray` meets it at `t`.
fn shade_bond_hit(input: TransparentImpostorVertexOutput, ray: Ray, t: f32) -> Shaded {
    let cylinder_start = input.world_a;
    let cylinder_axis = normalize(input.world_b - cylinder_start);

    let hit_point = ray.origin + t * ray.dir;
    let projection_length = dot(hit_point - cylinder_start, cylinder_axis);
    let axis_point = cylinder_start + projection_length * cylinder_axis;
    let surface_normal = normalize(hit_point - axis_point);

    let view_pos = camera.view_matrix * vec4<f32>(hit_point, 1.0);
    let clip_pos = camera.proj_matrix * view_pos;
    let depth = clip_pos.z / clip_pos.w;

    let color = to_display(light_surface(
        hit_point,
        surface_normal,
        input.color,
        0.5, // roughness - moderate for bonds
        0.0, // metallic - bonds are non-metallic
        FULLY_EXPOSED,
    ));

    var shaded: Shaded;
    shaded.depth = depth;
    shaded.color = vec4<f32>(color, input.alpha);
    return shaded;
}

@fragment
fn fs_main(input: TransparentImpostorVertexOutput) -> TransparentFragmentOutput {
    // Derivatives first: they must be taken in uniform control flow, and the
    // branch on `kind` (a varying) is not uniform as far as WGSL knows.
    var d: Derivatives;
    d.dwx = dpdxFine(input.world_position);
    d.dwy = dpdyFine(input.world_position);
    d.duv_dx = dpdxFine(input.quad_uv);
    d.duv_dy = dpdyFine(input.quad_uv);
    if input.kind == 0u {
        return shade_atom(input, d);
    }
    return shade_bond(input, d);
}

// ---- Per-sample shading (D5) ----
// Everything from this marker to the end of the file is the refined frame's
// entry point and is cut off (`pipelines::without_per_sample_shading`) on an
// adapter without `MULTISAMPLED_SHADING`, where a module using
// `@builtin(sample_index)` would not even compile. The block is repeated in
// `atom_impostor.wgsl` and `bond_impostor.wgsl`.

// Where `world_position` lies on the render target, in pixels, y down.
fn target_pixel_of(world_position: vec3<f32>) -> vec2<f32> {
    let clip = camera.view_proj * vec4<f32>(world_position, 1.0);
    let ndc = clip.xy / clip.w;
    return vec2<f32>(
        (ndc.x * 0.5 + 0.5) * camera.viewport_size_px.x,
        (0.5 - ndc.y * 0.5) * camera.viewport_size_px.y,
    );
}

// The offset, in pixels, from where `world_position` was interpolated to
// sample `index` of the pixel at `frag_xy`. WGSL interpolates at the pixel
// centre, which makes this the sample's standard offset; a backend running
// the shader per sample may interpolate at the sample instead, which makes it
// about zero. Measured rather than assumed, the sample ray is right either
// way. Twin: `per_sample_offset`.
fn per_sample_offset(frag_xy: vec2<f32>, world_position: vec3<f32>, index: u32) -> vec2<f32> {
    let sample_px = floor(frag_xy) + vec2<f32>(0.5) + sample_offset_4x(index);
    return sample_px - target_pixel_of(world_position);
}

struct PerSampleFragmentOutput {
    @builtin(frag_depth) depth: f32,
    @location(0) color: vec4<f32>,
}

// The refined frame (D5): taking `sample_index` runs this once per sample,
// each with its own ray, hit, colour and depth, so where two ghosts meet is
// resolved per sample. Each sample blends the ghost at its own alpha and the
// resolve averages them. No sample mask: an invocation covers its sample.
@fragment
fn fs_main_per_sample(
    input: TransparentImpostorVertexOutput,
    @builtin(sample_index) sample_index: u32,
) -> PerSampleFragmentOutput {
    // Derivatives first, before the branch on `kind` and anything that can
    // discard.
    let dwx = dpdxFine(input.world_position);
    let dwy = dpdyFine(input.world_position);

    let offset = per_sample_offset(input.clip_position.xy, input.world_position, sample_index);
    let ray = view_ray(sample_point(input.world_position, dwx, dwy, offset), input.radius);
    var t: f32;
    if input.kind == 0u {
        t = sphere_hit(ray.origin, ray.dir, input.world_a, input.radius);
    } else {
        t = cylinder_hit(ray.origin, ray.dir, input.world_a, input.world_b, input.radius);
    }
    if t < 0.0 {
        discard;
    }

    var shaded: Shaded;
    if input.kind == 0u {
        shaded = shade_atom_hit(input, ray, t);
    } else {
        shaded = shade_bond_hit(input, ray, t);
    }
    var output: PerSampleFragmentOutput;
    output.depth = shaded.depth;
    output.color = shaded.color;
    return output;
}
