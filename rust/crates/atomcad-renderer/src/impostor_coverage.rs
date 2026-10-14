//! Rust twins of the per-sample coverage arithmetic in the impostor shaders
//! (`doc/design_viewport_antialiasing.md` D4, D6).
//!
//! `atom_impostor.wgsl`, `bond_impostor.wgsl` and both branches of
//! `transparent_impostor.wgsl` decide coverage per sample: one hit-only ray
//! per sample position, the result written as `@builtin(sample_mask)`. WGSL
//! has no includes, so each shader carries its own copy of that arithmetic;
//! these functions repeat it line for line so it can be unit-tested on the
//! CPU. Each WGSL copy names its twin here, and a change to one must be made
//! to the others. A twin can drift from the WGSL silently, so the GPU harness
//! (`tests/renderer_gpu.rs`) is the cross-check.
//!
//! The refined frame (D5) builds one ray per sample from the same pieces,
//! plus [`target_pixel_of`] and [`per_sample_offset`].

use glam::{Mat4, Vec2, Vec3};

/// WebGPU's standard 4-sample pattern, in pixels from the pixel's top-left
/// corner (y down). Sample `i` is bit `i` of the sample mask.
pub const SAMPLE_POSITIONS_4X: [[f32; 2]; 4] = [
    [0.375, 0.125],
    [0.875, 0.375],
    [0.125, 0.625],
    [0.625, 0.875],
];

/// [`SAMPLE_POSITIONS_4X`] relative to the pixel centre. Twin of
/// `SAMPLE_OFFSETS_4X` in the impostor shaders.
const SAMPLE_OFFSETS_4X: [[f32; 2]; 4] = [
    [-0.125, -0.375],
    [0.375, -0.125],
    [-0.375, 0.125],
    [0.125, 0.375],
];

/// The single sample of a 1-sample target is the pixel centre.
const SAMPLE_OFFSETS_1X: [[f32; 2]; 1] = [[0.0, 0.0]];

/// The largest distance of a 4× sample from the pixel centre, in pixels
/// (`|(0.375, 0.125)|` ≈ 0.395, rounded up). Twin of `SAMPLE_FOOTPRINT_PX`.
pub const SAMPLE_FOOTPRINT_PX: f32 = 0.4;

/// The sample positions of a `sample_count`-sample target, relative to the
/// pixel centre, in pixels (y down). Only 1 and 4 exist (D1).
pub fn sample_offsets(sample_count: u32) -> &'static [[f32; 2]] {
    match sample_count {
        1 => &SAMPLE_OFFSETS_1X,
        4 => &SAMPLE_OFFSETS_4X,
        _ => panic!("no sample pattern for {sample_count} samples"),
    }
}

/// The point on the (planar) billboard quad under a sample: the interpolated
/// `world_position` plus the screen-space derivatives (`dwx` per +1 px in x,
/// `dwy` per +1 px in y, downwards) times the sample's offset. Twin of
/// `sample_point` in the impostor shaders.
pub fn sample_point(world_position: Vec3, dwx: Vec3, dwy: Vec3, offset: [f32; 2]) -> Vec3 {
    world_position + dwx * offset[0] + dwy * offset[1]
}

/// The limit on `|quad_uv|` beyond which no sample of the fragment can be
/// inside the atom quad's inscribed circle: 1 widened by the sample
/// footprint, converted to `quad_uv` units with its derivatives. Exactly 1
/// for one sample, so the single-sample early-out is today's. Twin of
/// `quad_uv_limit` in the atom shaders.
pub fn quad_uv_limit(duv_dx: Vec2, duv_dy: Vec2, sample_count: u32) -> f32 {
    if sample_count > 1 {
        1.0 + SAMPLE_FOOTPRINT_PX * (duv_dx.length_squared() + duv_dy.length_squared()).sqrt()
    } else {
        1.0
    }
}

/// The nearest non-negative `t` at which the ray `origin + t·dir` (`dir` a
/// unit vector) meets the sphere, or `None`. Twin of `sphere_hit` in the
/// shaders, which returns a negative `t` for a miss; the same three
/// rejections as the shading code: sphere behind the origin, ray past the
/// sphere, both intersections behind the origin.
pub fn sphere_hit(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let oc = centre - origin;
    let tca = oc.dot(dir);
    if tca < 0.0 {
        return None;
    }
    let d2 = oc.dot(oc) - tca * tca;
    let radius2 = radius * radius;
    if d2 > radius2 {
        return None;
    }
    let thc = (radius2 - d2).sqrt();
    let t0 = tca - thc;
    let t1 = tca + thc;
    let t = if t0 > 0.0 { t0 } else { t1 };
    if t < 0.0 { None } else { Some(t) }
}

/// The nearest non-negative `t` at which the ray meets the open cylinder
/// from `start` to `end` (no caps: the bond is clipped flat at both ends),
/// or `None`. Twin of `cylinder_hit` in the shaders.
pub fn cylinder_hit(origin: Vec3, dir: Vec3, start: Vec3, end: Vec3, radius: f32) -> Option<f32> {
    let axis = (end - start).normalize();
    let length = (end - start).length();
    let oc = origin - start;
    let ray_perp = dir - dir.dot(axis) * axis;
    let oc_perp = oc - oc.dot(axis) * axis;
    let a = ray_perp.dot(ray_perp);
    let b = 2.0 * oc_perp.dot(ray_perp);
    let c = oc_perp.dot(oc_perp) - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let sqrt_discriminant = discriminant.sqrt();
    let t1 = (-b - sqrt_discriminant) / (2.0 * a);
    let t2 = (-b + sqrt_discriminant) / (2.0 * a);
    let t = if t1 < 0.0 { t2 } else { t1 };
    if t < 0.0 {
        return None;
    }
    let projection = (origin + t * dir - start).dot(axis);
    if projection < 0.0 || projection > length {
        return None;
    }
    Some(t)
}

/// The largest `|x| + |y|` of a 4× sample offset: every one of them is 0.5.
pub const SAMPLE_OFFSET_L1: f32 = 0.5;

/// How far, in world units, a sample ray can pass from the pixel-centre ray.
/// A sample's quad point is `dwx·o.x + dwy·o.y` from the centre's, at most
/// [`SAMPLE_OFFSET_L1`] times the larger of the two pixel steps. Moving the
/// quad point by `δ` moves the ray by at most `|δ|` where it passes the
/// impostor (an atom's billboard faces the eye, so no point of it is nearer
/// the eye than the atom's centre); 1.25 to spare, for the bond's billboard,
/// which only faces the eye as far as its axis allows. Twin of
/// `sample_reach` in the impostor shaders.
pub fn sample_reach(dwx: Vec3, dwy: Vec3) -> f32 {
    1.25 * SAMPLE_OFFSET_L1 * dwx.length_squared().max(dwy.length_squared()).sqrt()
}

/// How far the ray, as a line, passes outside the sphere's silhouette:
/// negative inside it, `f32`-huge for a sphere behind the ray's origin. A
/// pixel-centre ray beyond [`sample_reach`] on either side lets the shader
/// skip the per-sample tests: every sample misses, or every sample hits.
/// Twin of `sphere_ray_distance` in the shaders.
pub fn sphere_ray_distance(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> f32 {
    let oc = centre - origin;
    let tca = oc.dot(dir);
    if tca < 0.0 {
        return 3.0e38;
    }
    (oc.dot(oc) - tca * tca).max(0.0).sqrt() - radius
}

/// How far the ray, as a line, passes outside the infinite cylinder through
/// `start` and `end`: negative inside it, and for a ray along the axis. A
/// pixel-centre ray beyond [`sample_reach`] lets the shader discard without
/// the per-sample tests. Twin of `cylinder_ray_distance` in the shaders.
pub fn cylinder_ray_distance(origin: Vec3, dir: Vec3, start: Vec3, end: Vec3, radius: f32) -> f32 {
    let axis = (end - start).normalize();
    let oc = origin - start;
    let ray_perp = dir - dir.dot(axis) * axis;
    let oc_perp = oc - oc.dot(axis) * axis;
    let a = ray_perp.dot(ray_perp);
    if a < 1.0e-12 {
        return -radius;
    }
    let b = oc_perp.dot(ray_perp);
    (oc_perp.dot(oc_perp) - b * b / a).max(0.0).sqrt() - radius
}

/// The sample mask of per-sample hits: bit `i` set when sample `i` hit.
pub fn coverage_mask(hits: impl Iterator<Item = bool>) -> u32 {
    hits.enumerate()
        .filter(|(_, hit)| *hit)
        .fold(0, |mask, (i, _)| mask | (1 << i))
}

/// Where `world_position` lies on a render target of `viewport_size_px`
/// pixels, in pixels from the top-left corner, y down. Twin of
/// `target_pixel_of` in the impostor shaders' per-sample block (D5).
pub fn target_pixel_of(view_proj: Mat4, viewport_size_px: Vec2, world_position: Vec3) -> Vec2 {
    let clip = view_proj * world_position.extend(1.0);
    let ndc = clip.truncate().truncate() / clip.w;
    Vec2::new(
        (ndc.x * 0.5 + 0.5) * viewport_size_px.x,
        (0.5 - ndc.y * 0.5) * viewport_size_px.y,
    )
}

/// The refined frame's sample offset (D5): from `interpolated_px`, where the
/// fragment's `world_position` was interpolated (measured with
/// [`target_pixel_of`]), to 4× sample `index` of the pixel at `frag_xy`. With
/// interpolation at the pixel centre it is the sample's standard offset;
/// with interpolation at the sample itself it is zero. Twin of
/// `per_sample_offset` in the impostor shaders.
pub fn per_sample_offset(frag_xy: Vec2, interpolated_px: Vec2, index: usize) -> [f32; 2] {
    let [x, y] = SAMPLE_OFFSETS_4X[index];
    let sample = frag_xy.floor() + Vec2::splat(0.5) + Vec2::new(x, y);
    (sample - interpolated_px).to_array()
}
