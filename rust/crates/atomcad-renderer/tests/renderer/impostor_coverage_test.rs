// Per-sample impostor coverage: the Rust twins of the shaders' arithmetic
// (doc/design_viewport_antialiasing.md D4, D6; Phase 2).

use atomcad_renderer::impostor_coverage::{
    SAMPLE_FOOTPRINT_PX, SAMPLE_OFFSET_L1, SAMPLE_POSITIONS_4X, coverage_mask, cylinder_hit,
    cylinder_ray_distance, per_sample_offset, quad_uv_limit, sample_offsets, sample_point,
    sample_reach, sphere_hit, sphere_ray_distance, target_pixel_of,
};
use glam::{Mat4, Vec2, Vec3};

/// Orthographic, looking down -Z, one world unit per pixel: a sample's ray
/// starts above its quad point and points down. `dwy` is -Y because pixel
/// rows grow downwards.
const DWX: Vec3 = Vec3::X;
const DWY: Vec3 = Vec3::NEG_Y;
const DOWN: Vec3 = Vec3::NEG_Z;

/// The hits of the four 4× sample rays of the pixel whose centre is above
/// `pixel_centre`, against `hit`.
fn sample_hits(pixel_centre: Vec3, hit: impl Fn(Vec3, Vec3) -> bool) -> u32 {
    coverage_mask(sample_offsets(4).iter().map(|offset| {
        let p = sample_point(pixel_centre, DWX, DWY, *offset);
        hit(p + Vec3::Z * 10.0, DOWN)
    }))
}

#[test]
fn the_4x_positions_are_webgpus_standard_pattern() {
    assert_eq!(
        SAMPLE_POSITIONS_4X,
        [
            [0.375, 0.125],
            [0.875, 0.375],
            [0.125, 0.625],
            [0.625, 0.875]
        ]
    );
}

#[test]
fn the_4x_offsets_are_the_positions_relative_to_the_centre_and_sum_to_zero() {
    let offsets = sample_offsets(4);
    assert_eq!(offsets.len(), 4);
    for (offset, position) in offsets.iter().zip(SAMPLE_POSITIONS_4X) {
        assert_eq!(offset[0], position[0] - 0.5);
        assert_eq!(offset[1], position[1] - 0.5);
    }
    let sum = offsets
        .iter()
        .fold(Vec2::ZERO, |sum, o| sum + Vec2::from(*o));
    assert_eq!(sum, Vec2::ZERO);
}

#[test]
fn the_footprint_bounds_every_4x_offset() {
    for offset in sample_offsets(4) {
        assert!(Vec2::from(*offset).length() <= SAMPLE_FOOTPRINT_PX);
    }
}

/// `sample_reach` rests on this: no 4× sample is further than half a pixel
/// step from the centre along x and y together.
#[test]
fn every_4x_offset_has_an_l1_length_of_one_half() {
    for offset in sample_offsets(4) {
        assert_eq!(offset[0].abs() + offset[1].abs(), SAMPLE_OFFSET_L1);
    }
}

#[test]
fn the_single_sample_is_the_pixel_centre() {
    assert_eq!(sample_offsets(1), &[[0.0, 0.0]]);
}

#[test]
fn sample_point_with_orthogonal_derivatives_lands_on_the_sample_positions() {
    let centre = Vec3::new(10.0, 20.0, 3.0);
    let dwx = Vec3::new(0.5, 0.0, 0.0);
    let dwy = Vec3::new(0.0, 0.0, -2.0);
    let points: Vec<Vec3> = sample_offsets(4)
        .iter()
        .map(|o| sample_point(centre, dwx, dwy, *o))
        .collect();
    assert_eq!(
        points,
        vec![
            Vec3::new(10.0 - 0.0625, 20.0, 3.0 + 0.75),
            Vec3::new(10.0 + 0.1875, 20.0, 3.0 + 0.25),
            Vec3::new(10.0 - 0.1875, 20.0, 3.0 - 0.25),
            Vec3::new(10.0 + 0.0625, 20.0, 3.0 - 0.75),
        ]
    );
    // The single sample is the interpolated position itself.
    assert_eq!(sample_point(centre, dwx, dwy, sample_offsets(1)[0]), centre);
}

#[test]
fn a_sphere_covering_the_pixel_sets_every_bit_and_one_beside_it_none() {
    let sphere = |centre: Vec3| move |o: Vec3, d: Vec3| sphere_hit(o, d, centre, 2.0).is_some();
    assert_eq!(sample_hits(Vec3::ZERO, sphere(Vec3::ZERO)), 0b1111);
    assert_eq!(
        sample_hits(Vec3::ZERO, sphere(Vec3::new(10.0, 0.0, 0.0))),
        0
    );
}

/// The property D4 rests on: two edges meeting at the pixel centre pick
/// complementary samples, so neighbouring impostors leave no seam.
#[test]
fn two_half_planes_meeting_at_the_pixel_centre_give_complementary_masks() {
    for degrees in (0..360).step_by(5) {
        let normal = Vec2::from_angle((degrees as f32 + 2.5).to_radians());
        let side = |sign: f32| move |o: Vec3, _: Vec3| sign * normal.dot(Vec2::new(o.x, o.y)) > 0.0;
        let a = sample_hits(Vec3::ZERO, side(1.0));
        let b = sample_hits(Vec3::ZERO, side(-1.0));
        assert_eq!(a | b, 0b1111, "at {degrees}°: {a:04b} | {b:04b}");
        assert_eq!(a & b, 0, "at {degrees}°: {a:04b} & {b:04b}");
    }
}

/// The same with the real hit test: two huge spheres whose silhouettes meet
/// at the pixel centre, one to its left and one to its right.
#[test]
fn two_sphere_silhouettes_meeting_at_the_pixel_centre_give_complementary_masks() {
    let radius = 1000.0;
    let left = sample_hits(Vec3::ZERO, |o, d| {
        sphere_hit(o, d, Vec3::new(-radius, 0.0, 0.0), radius).is_some()
    });
    let right = sample_hits(Vec3::ZERO, |o, d| {
        sphere_hit(o, d, Vec3::new(radius, 0.0, 0.0), radius).is_some()
    });
    assert_eq!(left, 0b0101, "samples 0 and 2 are left of the centre");
    assert_eq!(right, 0b1010);
}

#[test]
fn the_quad_uv_limit_is_exactly_one_for_one_sample_and_wider_for_four() {
    let duv_dx = Vec2::new(0.1, 0.0);
    let duv_dy = Vec2::new(0.0, -0.1);
    assert_eq!(quad_uv_limit(duv_dx, duv_dy, 1), 1.0);
    let limit = quad_uv_limit(duv_dx, duv_dy, 4);
    // Every sample of a fragment at |uv| = limit - ε reaches back inside
    // |uv| ≤ 1; the widening is the footprint in uv units.
    let expected = 1.0 + SAMPLE_FOOTPRINT_PX * 0.1 * 2f32.sqrt();
    assert!((limit - expected).abs() < 1e-6, "{limit} vs {expected}");
    for offset in sample_offsets(4) {
        let duv = duv_dx * offset[0] + duv_dy * offset[1];
        assert!(duv.length() <= limit - 1.0, "{offset:?}");
    }
}

#[test]
fn coverage_mask_sets_bit_i_for_sample_i() {
    assert_eq!(coverage_mask([false; 4].into_iter()), 0);
    assert_eq!(coverage_mask([true; 4].into_iter()), 0b1111);
    assert_eq!(
        coverage_mask([true, false, false, true].into_iter()),
        0b1001
    );
    assert_eq!(coverage_mask([false, true].into_iter()), 0b10);
}

/// A small deterministic generator, so the agreement tests sweep many rays
/// without a dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32) * 2.0 - 1.0
    }

    fn vec3(&mut self) -> Vec3 {
        Vec3::new(self.next(), self.next(), self.next())
    }
}

/// The nearest non-negative root of the sphere's quadratic, solved directly:
/// what the shading code's intersection computes, written independently.
fn reference_sphere(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - centre;
    let b = oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let roots = [-b - disc.sqrt(), -b + disc.sqrt()];
    roots.into_iter().find(|t| *t >= 0.0)
}

#[test]
fn sphere_hit_agrees_with_the_quadratic_on_hits() {
    let mut rng = Lcg(1);
    let mut hits = 0;
    for _ in 0..2000 {
        let centre = rng.vec3() * 2.0;
        let radius = 0.5 + rng.next().abs();
        let origin = Vec3::new(0.0, 0.0, 12.0) + rng.vec3() * 0.5;
        let target = rng.vec3() * 3.0;
        let dir = (target - origin).normalize();
        let got = sphere_hit(origin, dir, centre, radius);
        let expected = reference_sphere(origin, dir, centre, radius);
        match (got, expected) {
            (Some(t), Some(e)) => {
                hits += 1;
                assert!((t - e).abs() < 1e-3, "t {t} vs {e}");
                let point = origin + t * dir;
                assert!(((point - centre).length() - radius).abs() < 1e-3);
            }
            (None, None) => {}
            // A ray grazing the sphere within rounding may go either way.
            (got, expected) => {
                let grazing = (centre - origin).cross(dir).length() - radius;
                assert!(grazing.abs() < 1e-4, "{got:?} vs {expected:?}");
            }
        }
    }
    assert!(hits > 200, "too few hits to mean anything: {hits}");
}

/// The cylinder's hit, solved from `|(p - start) × axis| = r` and clipped to
/// the segment: independent of the twin's perpendicular-projection form.
fn reference_cylinder(origin: Vec3, dir: Vec3, start: Vec3, end: Vec3, r: f32) -> Option<f32> {
    let axis = (end - start).normalize();
    let length = (end - start).length();
    let u = (origin - start).cross(axis);
    let v = dir.cross(axis);
    let a = v.dot(v);
    let b = 2.0 * u.dot(v);
    let c = u.dot(u) - r * r;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t1 = (-b - disc.sqrt()) / (2.0 * a);
    let t2 = (-b + disc.sqrt()) / (2.0 * a);
    let t = if t1 < 0.0 { t2 } else { t1 };
    if t < 0.0 {
        return None;
    }
    let along = (origin + t * dir - start).dot(axis);
    (0.0..=length).contains(&along).then_some(t)
}

#[test]
fn cylinder_hit_agrees_with_the_cross_product_form_on_hits() {
    let mut rng = Lcg(2);
    let mut hits = 0;
    for _ in 0..2000 {
        let start = rng.vec3() * 2.0;
        let end = start + rng.vec3().normalize() * (1.0 + rng.next().abs() * 2.0);
        let radius = 0.2 + rng.next().abs() * 0.3;
        let origin = Vec3::new(0.0, 0.0, 12.0) + rng.vec3() * 0.5;
        // Aimed near the bond (a little past each end, a little off its
        // side), so hits, misses and clip-plane cases all occur.
        let along = 0.5 + rng.next() * 0.6;
        let target = start.lerp(end, along) + rng.vec3() * radius * 1.5;
        let dir = (target - origin).normalize();
        let got = cylinder_hit(origin, dir, start, end, radius);
        let expected = reference_cylinder(origin, dir, start, end, radius);
        match (got, expected) {
            (Some(t), Some(e)) => {
                hits += 1;
                assert!((t - e).abs() < 1e-3, "t {t} vs {e}");
            }
            (None, None) => {}
            (got, expected) => {
                // Grazing the side or crossing a clip plane within rounding.
                let t = got.or(expected).unwrap();
                let point = origin + t * dir;
                let axis = (end - start).normalize();
                let along = (point - start).dot(axis);
                let to_clip = along.abs().min((along - (end - start).length()).abs());
                let to_side = ((point - start).cross(axis).length() - radius).abs();
                assert!(to_clip < 1e-3 || to_side < 1e-3, "{got:?} vs {expected:?}");
            }
        }
    }
    assert!(hits > 500, "too few hits to mean anything: {hits}");
}

/// The bond is clipped flat at both ends: a ray that meets the infinite
/// cylinder beyond `end` misses, one just inside hits.
#[test]
fn cylinder_hit_clips_at_both_ends() {
    let start = Vec3::new(-1.0, 0.0, 0.0);
    let end = Vec3::new(1.0, 0.0, 0.0);
    let ray = |x: f32| cylinder_hit(Vec3::new(x, 0.0, 10.0), DOWN, start, end, 0.25);
    let t = ray(0.0).expect("the middle hits");
    assert!((t - 9.75).abs() < 1e-5, "{t}");
    assert!(ray(0.99).is_some());
    assert!(ray(-0.99).is_some());
    assert!(ray(1.01).is_none());
    assert!(ray(-1.01).is_none());
    // Off to the side.
    assert!(cylinder_hit(Vec3::new(0.0, 0.3, 10.0), DOWN, start, end, 0.25).is_none());
}

/// A sphere behind the ray's origin is a miss, not a hit at negative `t`.
#[test]
fn sphere_hit_rejects_a_sphere_behind_the_origin() {
    assert!(sphere_hit(Vec3::ZERO, DOWN, Vec3::new(0.0, 0.0, 5.0), 1.0).is_none());
    // Origin inside the sphere: the far intersection.
    let t = sphere_hit(Vec3::ZERO, DOWN, Vec3::ZERO, 1.0).unwrap();
    assert!((t - 1.0).abs() < 1e-6);
}

// ---- The early-outs: skipping the per-sample tests far from an edge --------

/// A pinhole camera at `eye` looking at the origin, `focal` pixels from its
/// image plane: what the rasteriser does to a billboard, without the GPU.
struct Pinhole {
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    focal: f32,
}

impl Pinhole {
    fn new(eye: Vec3, focal: f32) -> Self {
        let forward = (-eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        Self {
            eye,
            forward,
            right,
            up,
            focal,
        }
    }

    /// Where the ray through pixel `(u, v)` (from the image centre, y down)
    /// meets the plane through `point` with normal `normal`.
    fn on_plane(&self, u: f32, v: f32, point: Vec3, normal: Vec3) -> Vec3 {
        let dir = (self.forward * self.focal + self.right * u - self.up * v).normalize();
        let t = (point - self.eye).dot(normal) / dir.dot(normal);
        self.eye + dir * t
    }

    /// The quad point under pixel `(u, v)` and its fine derivatives, as the
    /// fragment shader sees them.
    fn fragment(&self, u: f32, v: f32, point: Vec3, normal: Vec3) -> (Vec3, Vec3, Vec3) {
        let p = self.on_plane(u, v, point, normal);
        let dwx = self.on_plane(u + 1.0, v, point, normal) - p;
        let dwy = self.on_plane(u, v + 1.0, point, normal) - p;
        (p, dwx, dwy)
    }

    /// The sample rays of the fragment at `p`, in perspective.
    fn sample_rays(&self, p: Vec3, dwx: Vec3, dwy: Vec3) -> Vec<(Vec3, Vec3)> {
        sample_offsets(4)
            .iter()
            .map(|o| {
                (
                    self.eye,
                    (sample_point(p, dwx, dwy, *o) - self.eye).normalize(),
                )
            })
            .collect()
    }
}

/// The shaders discard when the pixel-centre ray passes further outside the
/// sphere than `sample_reach`, and take every sample when it passes further
/// inside: in both cases every sample must agree.
#[test]
fn the_sphere_early_outs_never_change_the_mask() {
    let mut rng = Lcg(3);
    let (mut outside, mut inside) = (0, 0);
    for _ in 0..200 {
        let camera = Pinhole::new(rng.vec3() * 2.0 + Vec3::new(0.0, 0.0, 15.0), 400.0);
        let centre = rng.vec3() * 3.0;
        let radius = 0.1 + rng.next().abs();
        let normal = (camera.eye - centre).normalize();
        // Pixels on a ring around the sphere's projected silhouette.
        let projected = |w: Vec3| {
            let d = w - camera.eye;
            let z = d.dot(camera.forward);
            (
                d.dot(camera.right) / z * camera.focal,
                -d.dot(camera.up) / z * camera.focal,
            )
        };
        let (cu, cv) = projected(centre);
        let r_px = radius / (centre - camera.eye).length() * camera.focal;
        for k in 0..64 {
            let angle = k as f32 * 0.1;
            let rho = r_px * (1.0 + rng.next() * 0.2) + rng.next() * 2.0;
            let (u, v) = (cu + rho * angle.cos(), cv + rho * angle.sin());
            let (p, dwx, dwy) = camera.fragment(u, v, centre, normal);
            let dir = (p - camera.eye).normalize();
            let distance = sphere_ray_distance(camera.eye, dir, centre, radius);
            let reach = sample_reach(dwx, dwy);
            let hits: Vec<bool> = camera
                .sample_rays(p, dwx, dwy)
                .iter()
                .map(|(o, d)| sphere_hit(*o, *d, centre, radius).is_some())
                .collect();
            if distance > reach {
                outside += 1;
                assert!(hits.iter().all(|h| !h), "discarded a covered sample");
            } else if distance < -reach && sphere_hit(camera.eye, dir, centre, radius).is_some() {
                inside += 1;
                assert!(hits.iter().all(|h| *h), "took an uncovered sample");
            }
        }
    }
    assert!(
        outside > 1000 && inside > 1000,
        "{outside} outside, {inside} inside"
    );
}

/// The bond shader discards when the pixel-centre ray passes further outside
/// the cylinder than `sample_reach`: then every sample must miss.
#[test]
fn the_cylinder_early_out_never_drops_a_covered_sample() {
    let mut rng = Lcg(4);
    let mut outside = 0;
    for _ in 0..200 {
        let camera = Pinhole::new(rng.vec3() * 2.0 + Vec3::new(0.0, 0.0, 15.0), 400.0);
        let start = rng.vec3() * 2.0;
        let end = start + rng.vec3().normalize() * (1.0 + rng.next().abs() * 2.0);
        let radius = 0.05 + rng.next().abs() * 0.3;
        // The bond's billboard: contains the axis, faces the eye as far as
        // it can (bond_impostor.wgsl `vs_main`).
        let axis = (end - start).normalize();
        let centre = (start + end) * 0.5;
        let right = axis.cross((camera.eye - centre).normalize()).normalize();
        let normal = right.cross(axis);
        for _ in 0..64 {
            let along = centre + axis * rng.next() * (end - start).length() * 0.55;
            let point = along + right * rng.next() * radius * 2.5;
            let d = point - camera.eye;
            let z = d.dot(camera.forward);
            let (u, v) = (
                (d.dot(camera.right) / z * camera.focal).round(),
                (-d.dot(camera.up) / z * camera.focal).round(),
            );
            let (p, dwx, dwy) = camera.fragment(u, v, centre, normal);
            let dir = (p - camera.eye).normalize();
            if cylinder_ray_distance(camera.eye, dir, start, end, radius) > sample_reach(dwx, dwy) {
                outside += 1;
                for (o, d) in camera.sample_rays(p, dwx, dwy) {
                    assert!(
                        cylinder_hit(o, d, start, end, radius).is_none(),
                        "discarded a covered sample"
                    );
                }
            }
        }
    }
    assert!(outside > 1000, "{outside} outside");
}

#[test]
fn the_ray_distances_are_signed_distances_to_the_silhouette() {
    // Straight down, 0.5 beside a sphere of radius 2, and 0.5 inside it.
    assert!(
        (sphere_ray_distance(Vec3::new(2.5, 0.0, 10.0), DOWN, Vec3::ZERO, 2.0) - 0.5).abs() < 1e-5
    );
    assert!(
        (sphere_ray_distance(Vec3::new(1.5, 0.0, 10.0), DOWN, Vec3::ZERO, 2.0) + 0.5).abs() < 1e-5
    );
    // A sphere behind the origin is never near.
    assert!(sphere_ray_distance(Vec3::ZERO, DOWN, Vec3::new(0.0, 0.0, 5.0), 1.0) > 1e30);
    // Beside a cylinder along X, and along its axis (undecided: negative).
    let (start, end) = (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
    assert!(
        (cylinder_ray_distance(Vec3::new(0.0, 1.0, 10.0), DOWN, start, end, 0.25) - 0.75).abs()
            < 1e-5
    );
    assert!(cylinder_ray_distance(Vec3::new(-5.0, 0.0, 0.0), Vec3::X, start, end, 0.25) < 0.0);
    // The reach is 1.25 × half the larger pixel step.
    let reach = sample_reach(Vec3::new(0.3, 0.0, 0.0), Vec3::new(0.0, 0.4, 0.0));
    assert!((reach - 1.25 * 0.5 * 0.4).abs() < 1e-6);
}

// ---- The refined frame's sample rays (D5, Phase 3) --------------------------

/// The point of the plane z = 0 under render pixel position `px` (y down)
/// of a `size` target seen through `view_proj`.
fn plane_point_under(view_proj: Mat4, size: Vec2, px: Vec2) -> Vec3 {
    let ndc = Vec2::new(px.x / size.x * 2.0 - 1.0, 1.0 - px.y / size.y * 2.0);
    let inverse = view_proj.inverse();
    let near = inverse.project_point3(ndc.extend(0.0));
    let far = inverse.project_point3(ndc.extend(1.0));
    near + (far - near) * (-near.z / (far.z - near.z))
}

#[test]
fn target_pixel_of_maps_world_to_pixels_with_y_down() {
    // One world unit per pixel, origin at the centre of a 100 x 50 target.
    let view_proj = Mat4::orthographic_rh(-50.0, 50.0, -25.0, 25.0, -10.0, 10.0);
    let size = Vec2::new(100.0, 50.0);
    assert_eq!(
        target_pixel_of(view_proj, size, Vec3::ZERO),
        Vec2::new(50.0, 25.0)
    );
    let px = target_pixel_of(view_proj, size, Vec3::new(10.0, 5.0, 3.0));
    assert!((px - Vec2::new(60.0, 20.0)).length() < 1e-4, "{px}");
}

/// The offset is the standard one when the input was interpolated at the
/// pixel centre, and zero when it was interpolated at the sample itself, so
/// the sample ray is right under either interpolation rule.
#[test]
fn the_per_sample_offset_is_measured_from_where_the_input_was_interpolated() {
    let pixel = Vec2::new(10.0, 20.0);
    for (i, &[x, y]) in sample_offsets(4).iter().enumerate() {
        let centre = pixel + Vec2::splat(0.5);
        assert_eq!(per_sample_offset(centre, centre, i), [x, y], "sample {i}");

        let at_sample = pixel + Vec2::from(SAMPLE_POSITIONS_4X[i]);
        assert_eq!(
            per_sample_offset(at_sample, at_sample, i),
            [0.0, 0.0],
            "sample {i}"
        );
    }
}

/// End to end in perspective: from a point interpolated at the pixel centre
/// or at the sample, `sample_point` with the measured offset lands on the
/// plane point under the sample (to first order, within a hundredth of a
/// pixel).
#[test]
fn a_sample_point_lands_under_its_sample_in_perspective() {
    let size = Vec2::new(320.0, 200.0);
    let view = Mat4::look_at_rh(Vec3::new(3.0, -4.0, 20.0), Vec3::ZERO, Vec3::Y);
    let proj = Mat4::perspective_rh(0.8, size.x / size.y, 0.1, 100.0);
    let view_proj = proj * view;

    let pixel = Vec2::new(201.0, 77.0);
    for (i, position) in SAMPLE_POSITIONS_4X.iter().enumerate() {
        let sample_px = pixel + Vec2::from(*position);
        for interpolated_px in [pixel + Vec2::splat(0.5), sample_px] {
            let world = plane_point_under(view_proj, size, interpolated_px);
            let dwx = plane_point_under(view_proj, size, interpolated_px + Vec2::X) - world;
            let dwy = plane_point_under(view_proj, size, interpolated_px + Vec2::Y) - world;
            let measured = target_pixel_of(view_proj, size, world);
            let offset = per_sample_offset(interpolated_px, measured, i);
            let p = sample_point(world, dwx, dwy, offset);
            let landed = target_pixel_of(view_proj, size, p);
            assert!(
                (landed - sample_px).length() < 0.01,
                "sample {i} from {interpolated_px}: landed at {landed}, want {sample_px}"
            );
        }
    }
}
