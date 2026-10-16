// The shadow fit and its uniform (doc/design_shadows.md D5, D6, D8): the
// light frame, the depth range, the footprint, the stabilisation, the
// cascade criterion and splits, and `ShadowUniform::new`. Its WGSL twin is
// checked in `shader_test.rs`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::render_target::TargetSpec;
use atomcad_renderer::renderer::CameraUniform;
use atomcad_renderer::shading_uniform::{
    LightingPreset, ShadingPass, ShadingSettings, ShadingUniform,
};
use atomcad_renderer::shadow_fit::{
    MAX_CASCADES, SHADOW_BLEND_FRACTION, SHADOW_MAP_SIZE, SHADOW_SOFTNESS_A,
    SHADOW_TEXELS_PER_PIXEL, ShadowCascade, ShadowSetup, ShadowUniform, light_uniform,
    shadow_filter_radius, shadow_setup,
};
use glam::f64::{DQuat, DVec3};
use std::mem::offset_of;

const SHADOWS: ShadingSettings = ShadingSettings {
    preset: LightingPreset::Studio,
    depth_cue_strength: 0.0,
    ao_strength: 0.0,
    shadow_strength: 0.7,
};

/// A 20 Å block around the origin.
const BLOCK: (DVec3, DVec3) = (
    DVec3::new(-10.0, -10.0, -10.0),
    DVec3::new(10.0, 10.0, 10.0),
);

fn target() -> TargetSpec {
    TargetSpec::image(320, 200, 1)
}

fn looking_at(eye: DVec3, target: DVec3, orthographic: bool) -> Camera {
    let mut camera = Camera::default_pose(1.6);
    camera.eye = eye;
    camera.target = target;
    camera.up = DVec3::Z;
    camera.orthographic = orthographic;
    camera.ortho_half_height = 20.0;
    camera.orthonormalize_up();
    camera
}

fn setup_for(camera: &Camera, bounds: (DVec3, DVec3)) -> ShadowSetup {
    shadow_setup(camera, Some(bounds), &SHADOWS, &target()).expect("a shadow setup")
}

/// `direction` in the camera's frame: (right, up, forward).
fn in_camera_space(camera: &Camera, direction: DVec3) -> DVec3 {
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    DVec3::new(
        direction.dot(right),
        direction.dot(up),
        direction.dot(forward),
    )
}

/// The camera orbited about `centre` by `angle` round `axis`.
fn orbited(camera: &Camera, centre: DVec3, axis: DVec3, angle: f64) -> Camera {
    let rotation = DQuat::from_axis_angle(axis.normalize(), angle);
    let mut orbited = camera.clone();
    orbited.eye = centre + rotation * (camera.eye - centre);
    orbited.target = centre + rotation * (camera.target - centre);
    orbited.up = rotation * camera.up;
    orbited.orthonormalize_up();
    orbited
}

fn orientations() -> Vec<(DVec3, f64)> {
    vec![
        (DVec3::Z, 0.0),
        (DVec3::Z, 0.7),
        (DVec3::Z, 2.1),
        (DVec3::X, 0.4),
        (DVec3::X, -0.9),
        (DVec3::new(1.0, 1.0, 0.0), 1.3),
        (DVec3::new(-1.0, 2.0, 0.5), 2.6),
        (DVec3::new(0.3, -1.0, 1.0), -2.2),
    ]
}

/// D5: the light frame is a fixed rotation of view space, so orbiting
/// leaves `L` and `u` unchanged in camera space; `u ⊥ L`, both unit.
#[test]
fn the_light_frame_is_camera_relative() {
    let base = looking_at(DVec3::new(0.0, -80.0, 30.0), DVec3::ZERO, false);
    let reference = setup_for(&base, BLOCK);
    let l0 = in_camera_space(&base, reference.light_dir);
    let u0 = in_camera_space(&base, reference.light_up);
    for (axis, angle) in orientations() {
        let camera = orbited(&base, DVec3::ZERO, axis, angle);
        let setup = setup_for(&camera, BLOCK);
        assert!((setup.light_dir.length() - 1.0).abs() < 1e-12);
        assert!((setup.light_up.length() - 1.0).abs() < 1e-12);
        assert!(setup.light_dir.dot(setup.light_up).abs() < 1e-12);
        let l = in_camera_space(&camera, setup.light_dir);
        let u = in_camera_space(&camera, setup.light_up);
        assert!((l - l0).length() < 1e-9, "{axis} {angle}: L {l} vs {l0}");
        assert!((u - u0).length() < 1e-9, "{axis} {angle}: u {u} vs {u0}");
    }
}

/// D5: the map is built along *the* key the receiver shades with.
#[test]
fn the_light_direction_is_the_shading_uniforms_key() {
    let base = looking_at(DVec3::new(15.0, -60.0, 40.0), DVec3::ZERO, false);
    for (axis, angle) in orientations() {
        for orthographic in [false, true] {
            let mut camera = orbited(&base, DVec3::ZERO, axis, angle);
            camera.orthographic = orthographic;
            let setup = setup_for(&camera, BLOCK);
            let uniform = ShadingUniform::new(
                &camera,
                Some(BLOCK),
                [0, 0, 0],
                &SHADOWS,
                ShadingPass::Content,
                Some(&setup),
            );
            assert_eq!(
                setup.light_dir.as_vec3().to_array(),
                uniform.key_light_dir,
                "{axis} {angle}"
            );
        }
    }
}

fn corners(bounds: (DVec3, DVec3)) -> Vec<DVec3> {
    let (min, max) = bounds;
    (0..8)
        .map(|i| {
            DVec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            )
        })
        .collect()
}

/// D5: every caster is inside the depth range, on screen or not.
#[test]
fn the_depth_range_contains_every_caster_corner() {
    let bounds = (DVec3::new(-30.0, -5.0, -2.0), DVec3::new(40.0, 12.0, 9.0));
    let base = looking_at(
        DVec3::new(0.0, -20.0, 15.0),
        DVec3::new(0.0, 0.0, 3.0),
        false,
    );
    for (axis, angle) in orientations() {
        let camera = orbited(&base, DVec3::new(0.0, 0.0, 3.0), axis, angle);
        let Some(setup) = shadow_setup(&camera, Some(bounds), &SHADOWS, &target()) else {
            continue;
        };
        let cascade = setup.cascades[0];
        for corner in corners(bounds) {
            let z = setup.to_light_space(corner).z;
            assert!(
                z >= cascade.z_near_l,
                "{axis} {angle}: {z} before the range"
            );
            assert!(
                z <= cascade.z_near_l + cascade.depth_range,
                "{axis} {angle}: {z} beyond the range"
            );
        }
    }
}

/// A fixed-seed xorshift, so the random points are the same every run.
struct Random(u64);

impl Random {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn in_ball(&mut self, centre: DVec3, radius: f64) -> DVec3 {
        loop {
            let p = DVec3::new(self.unit(), self.unit(), self.unit()) * 2.0 - DVec3::ONE;
            if p.length_squared() <= 1.0 {
                return centre + p * radius;
            }
        }
    }
}

/// Whether `p` is in `camera`'s view, laterally.
fn in_view(camera: &Camera, p: DVec3) -> bool {
    let ndc = camera.build_view_projection_matrix().project_point3(p);
    ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0
}

fn view_depth(camera: &Camera, p: DVec3) -> f64 {
    (p - camera.eye).dot((camera.target - camera.eye).normalize())
}

/// A cube centred on the origin whose bounding sphere's diameter is just
/// under a quarter-octave step (2^5.25 Å): the quantisation leaves almost no
/// slack, so only the fit's own margin keeps the sphere's edge inside the
/// square.
fn cube_just_under_a_step() -> (DVec3, DVec3) {
    let half = (2f64.powf(5.25) - 0.1) / (2.0 * 3f64.sqrt());
    (DVec3::splat(-half), DVec3::splat(half))
}

/// D5 step 4: no filter tap of a receiver in the visible slice leaves the
/// square. Random points of the caster sphere (its surface, where the
/// footprint is tightest, in the full views) that the camera sees lie inside the square shrunk
/// by the margin (the filter radius, the normal offset and one texel), in
/// both projections, in full views and in close-ups. Mutation: the fit
/// without its margin fails the full views.
#[test]
fn visible_points_of_the_caster_sphere_project_inside_the_square_with_margin() {
    let slab = (DVec3::new(-40.0, -40.0, -5.0), DVec3::new(40.0, 40.0, 5.0));
    let cube = cube_just_under_a_step();
    let views = [
        (
            cube,
            looking_at(DVec3::new(0.0, -300.0, 150.0), DVec3::ZERO, false),
        ),
        (
            cube,
            looking_at(DVec3::new(0.0, -300.0, 150.0), DVec3::ZERO, true),
        ),
        (
            slab,
            looking_at(
                DVec3::new(10.0, -25.0, 12.0),
                DVec3::new(5.0, 0.0, 0.0),
                false,
            ),
        ),
        (
            slab,
            looking_at(
                DVec3::new(-60.0, -40.0, 50.0),
                DVec3::new(-20.0, 10.0, 0.0),
                true,
            ),
        ),
    ];
    let mut random = Random(0x5eed_1234_abcd_ef01);
    for (v, (bounds, camera)) in views.iter().enumerate() {
        let (centre, radius) = (
            0.5 * (bounds.0 + bounds.1),
            0.5 * (bounds.1 - bounds.0).length(),
        );
        let setup = setup_for(camera, *bounds);
        let cascade = setup.cascades[0];
        let t = cascade.texel_world;
        let inner = 0.5 * cascade.side - (shadow_filter_radius(t) + t + t);
        let mut checked = 0;
        for _ in 0..1000 {
            // The full views' footprint is the sphere's shadow, tightest at
            // its surface; a close-up sees only the inside of the sphere.
            let in_ball = random.in_ball(DVec3::ZERO, 1.0);
            let p = centre + radius * if v < 2 { in_ball.normalize() } else { in_ball };
            let d = view_depth(camera, p);
            if !in_view(camera, p) || d < cascade.split_near || d > cascade.split_far {
                continue;
            }
            let l = setup.to_light_space(p).truncate();
            let offset = (l - cascade.centre_l).abs();
            assert!(
                offset.max_element() <= inner,
                "view {v}: {p} is {offset} from the centre, the square's inner half side is {inner}"
            );
            checked += 1;
        }
        assert!(checked > 50, "view {v}: only {checked} points in view");
    }
}

/// D5: the side is a quarter-octave step, at least the footprint with its
/// margin. In full view the footprint is the sphere's shadow, `2R`.
#[test]
fn the_side_is_a_quarter_octave_step_at_least_the_footprint() {
    for (bounds, camera) in [
        (
            BLOCK,
            looking_at(DVec3::new(0.0, -200.0, 80.0), DVec3::ZERO, false),
        ),
        (
            (DVec3::splat(-1.0), DVec3::splat(1.0)),
            looking_at(DVec3::new(0.0, -20.0, 8.0), DVec3::ZERO, true),
        ),
        (
            cube_just_under_a_step(),
            looking_at(DVec3::new(0.0, -300.0, 150.0), DVec3::ZERO, false),
        ),
        (
            (
                DVec3::new(-300.0, -300.0, 0.0),
                DVec3::new(300.0, 300.0, 5.0),
            ),
            looking_at(DVec3::new(0.0, -900.0, 600.0), DVec3::ZERO, false),
        ),
    ] {
        let cascade = setup_for(&camera, bounds).cascades[0];
        let steps = 4.0 * cascade.side.log2();
        assert!(
            (steps - steps.round()).abs() < 1e-9,
            "side {}",
            cascade.side
        );
        let t = cascade.texel_world;
        assert!((t - cascade.side / SHADOW_MAP_SIZE as f64).abs() < 1e-12);
        let footprint = (bounds.1 - bounds.0).length();
        let margin = shadow_filter_radius(t) + 2.0 * t;
        assert!(
            cascade.side >= footprint + 2.0 * margin,
            "side {} for a footprint of {footprint}",
            cascade.side
        );
        // Quantising rounds up by at most one step.
        assert!(cascade.side < (footprint + 2.0 * margin) * 2f64.powf(0.25) * 1.01);
    }
}

/// D5: a pan moves the square by whole texels, so casters stay on their
/// texels. A close-up whose footprint is the view's, not the sphere's, so
/// the square does follow the pan.
#[test]
fn a_pan_moves_the_square_by_whole_texels() {
    let bounds = (
        DVec3::new(-200.0, -200.0, -5.0),
        DVec3::new(200.0, 200.0, 5.0),
    );
    for orthographic in [false, true] {
        let base = looking_at(DVec3::new(0.0, -30.0, 20.0), DVec3::ZERO, orthographic);
        let first = setup_for(&base, bounds).cascades[0];
        let t = first.texel_world;
        let pan_dir = (base.target - base.eye).cross(base.up).normalize();
        let mut previous = first;
        let mut moved = 0;
        for step in 1..=12 {
            let shift = pan_dir * (0.3 * t * step as f64) + base.up * (0.17 * t * step as f64);
            let mut camera = base.clone();
            camera.eye += shift;
            camera.target += shift;
            let cascade = setup_for(&camera, bounds).cascades[0];
            assert_eq!(cascade.side, first.side, "the pan changed the side");
            let delta = (cascade.centre_l - previous.centre_l) / t;
            for axis in [delta.x, delta.y] {
                assert!(
                    (axis - axis.round()).abs() < 1e-6,
                    "step {step}: moved by {delta} texels"
                );
                assert!(axis.round().abs() <= 1.0, "step {step}: moved by {delta}");
                moved += (axis.round() != 0.0) as u32;
            }
            previous = cascade;
        }
        assert!(
            moved > 0,
            "orthographic {orthographic}: the square never moved"
        );
    }
}

/// D5: with the whole caster sphere in view, orbiting about its centre
/// leaves the side unchanged: the footprint is the sphere's shadow.
#[test]
fn orbiting_with_the_casters_in_view_keeps_the_side() {
    let centre = DVec3::new(3.0, -2.0, 1.0);
    let bounds = (
        centre - DVec3::new(8.0, 5.0, 3.0),
        centre + DVec3::new(8.0, 5.0, 3.0),
    );
    for orthographic in [false, true] {
        let base = looking_at(centre + DVec3::new(0.0, -120.0, 50.0), centre, orthographic);
        let side = setup_for(&base, bounds).cascades[0].side;
        for (axis, angle) in orientations() {
            let camera = orbited(&base, centre, axis, angle);
            let cascade = setup_for(&camera, bounds).cascades[0];
            assert_eq!(
                cascade.side, side,
                "{axis} {angle}, orthographic {orthographic}"
            );
        }
    }
}

/// D5: `None` when no shadow is drawn.
#[test]
fn no_setup_without_strength_studio_casters_or_casters_in_front() {
    let camera = looking_at(DVec3::new(0.0, -80.0, 30.0), DVec3::ZERO, false);
    let target = target();
    assert!(shadow_setup(&camera, Some(BLOCK), &SHADOWS, &target).is_some());
    let off = ShadingSettings {
        shadow_strength: 0.0,
        ..SHADOWS
    };
    assert_eq!(shadow_setup(&camera, Some(BLOCK), &off, &target), None);
    let headlight = ShadingSettings {
        preset: LightingPreset::Headlight,
        ..SHADOWS
    };
    assert_eq!(
        shadow_setup(&camera, Some(BLOCK), &headlight, &target),
        None
    );
    assert_eq!(shadow_setup(&camera, None, &SHADOWS, &target), None);
    // Every caster behind the camera.
    let behind = (
        DVec3::new(-5.0, -200.0, 60.0),
        DVec3::new(5.0, -150.0, 80.0),
    );
    assert_eq!(shadow_setup(&camera, Some(behind), &SHADOWS, &target), None);
}

// ---- ShadowUniform (D8) ----------------------------------------------------

#[test]
fn the_shadow_uniform_has_the_wgsl_layout() {
    assert_eq!(std::mem::size_of::<ShadowCascade>(), 80);
    assert_eq!(offset_of!(ShadowCascade, world_to_clip), 0);
    assert_eq!(offset_of!(ShadowCascade, split_far), 64);
    assert_eq!(offset_of!(ShadowCascade, texel_world), 68);
    assert_eq!(offset_of!(ShadowCascade, filter_radius_uv), 72);
    assert_eq!(offset_of!(ShadowCascade, depth_range), 76);
    assert_eq!(std::mem::size_of::<ShadowUniform>(), 336);
    assert_eq!(offset_of!(ShadowUniform, cascades), 0);
    assert_eq!(offset_of!(ShadowUniform, count), 320);
    assert_eq!(offset_of!(ShadowUniform, blend_fraction), 324);
    assert_eq!(offset_of!(ShadowUniform, debug_tint), 328);
    assert_eq!(offset_of!(ShadowUniform, front), 332);
}

/// D8: the receiver projects with exactly the matrix the caster pass draws
/// with, and the derived fields come from the fit.
#[test]
fn the_shadow_uniform_carries_the_casters_matrix_and_the_fit() {
    let camera = looking_at(DVec3::new(10.0, -60.0, 40.0), DVec3::ZERO, false);
    let setup = setup_for(&camera, BLOCK);
    let uniform = ShadowUniform::new(Some(&setup), false);
    assert_eq!(uniform.count, 1);
    assert_eq!(uniform.blend_fraction, SHADOW_BLEND_FRACTION as f32);
    assert_eq!(uniform.debug_tint, 0);
    let cascade = setup.cascades[0];
    assert_eq!(uniform.front, cascade.split_near as f32);
    let gpu = uniform.cascades[0];
    let caster = CameraUniform::new(
        &setup.light_camera(0),
        (SHADOW_MAP_SIZE, SHADOW_MAP_SIZE),
        1.0,
    );
    assert_eq!(gpu.world_to_clip, caster.view_proj);
    assert_eq!(gpu.world_to_clip, light_uniform(&setup, 0).view_proj);
    assert_eq!(gpu.split_far, cascade.split_far as f32);
    assert_eq!(gpu.texel_world, cascade.texel_world as f32);
    assert_eq!(gpu.depth_range, cascade.depth_range as f32);
    assert_eq!(
        gpu.filter_radius_uv,
        (shadow_filter_radius(cascade.texel_world) / cascade.side) as f32
    );
    for unused in &uniform.cascades[1..MAX_CASCADES] {
        assert_eq!(*unused, bytemuck::Zeroable::zeroed());
    }
    assert!(ShadowUniform::new(Some(&setup), true).debug_tint != 0);

    let none = ShadowUniform::new(None, false);
    assert_eq!(none.count, 0);
    for slot in &none.cascades {
        assert_eq!(*slot, bytemuck::Zeroable::zeroed());
    }
}

/// D5: the light camera maps the square to the whole map and the depth
/// range to [0, 1], as the receiver's arithmetic assumes (uv from clip x/y
/// with w = 1, depth from clip z).
#[test]
fn the_light_camera_maps_the_square_and_the_depth_range_to_the_map() {
    let camera = looking_at(DVec3::new(10.0, -60.0, 40.0), DVec3::ZERO, true);
    let setup = setup_for(&camera, BLOCK);
    let cascade = setup.cascades[0];
    let view_proj = setup.light_camera(0).build_view_projection_matrix();
    let world = |x: f64, y: f64, z: f64| {
        setup.light_right() * (cascade.centre_l.x + x)
            + setup.light_up * (cascade.centre_l.y + y)
            + setup.light_dir * (cascade.z_near_l + z)
    };
    let half = 0.5 * cascade.side;
    let d = cascade.depth_range;
    for (x, y, z, expected) in [
        (0.0, 0.0, 0.0, DVec3::new(0.0, 0.0, 0.0)),
        (half, 0.0, d, DVec3::new(1.0, 0.0, 1.0)),
        (0.0, half, 0.5 * d, DVec3::new(0.0, 1.0, 0.5)),
        (-half, -half, 0.25 * d, DVec3::new(-1.0, -1.0, 0.25)),
    ] {
        let clip = view_proj * world(x, y, z).extend(1.0);
        assert!((clip.w - 1.0).abs() < 1e-12, "orthographic, w = 1");
        assert!(
            (clip.truncate() - expected).length() < 1e-9,
            "({x}, {y}, {z}) -> {clip}"
        );
    }
}

// ---- Adaptive cascades (D6, Phase 2) ---------------------------------------

/// A 1 000 × 1 000 × 20 Å slab, its top face at z = 10.
const SLAB: (DVec3, DVec3) = (
    DVec3::new(-500.0, -500.0, -10.0),
    DVec3::new(500.0, 500.0, 10.0),
);

/// A perspective close-up on [`SLAB`] at a grazing angle: the eye 15 Å above
/// its top face, near its edge, looking across it.
fn grazing_slab_camera() -> Camera {
    looking_at(
        DVec3::new(0.0, -480.0, 25.0),
        DVec3::new(0.0, -380.0, 10.0),
        false,
    )
}

/// The support of `bounds` along `camera`'s view axis, clamped to its near
/// and far planes: what the splits must cover (D6). Computed here from the
/// corners, not with the fit's helper.
fn view_extent(camera: &Camera, bounds: (DVec3, DVec3)) -> (f64, f64) {
    let depths: Vec<f64> = corners(bounds)
        .iter()
        .map(|&c| view_depth(camera, c))
        .collect();
    let front = depths.iter().copied().fold(f64::INFINITY, f64::min);
    let back = depths.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (front.max(camera.znear), back.min(camera.zfar))
}

/// The world size of an output pixel at view depth `depth` (D6).
fn output_pixel(camera: &Camera, target: &TargetSpec, depth: f64) -> f64 {
    let output_height = target.size.height as f64 / target.pixel_scale as f64;
    let view_height = if camera.orthographic {
        2.0 * camera.ortho_half_height
    } else {
        2.0 * depth * (0.5 * camera.fovy).tan()
    };
    view_height / output_height
}

/// D6's criterion, written out: `κ` output pixels at view depth `depth`,
/// but never below the floor.
fn texel_limit(camera: &Camera, target: &TargetSpec, depth: f64) -> f64 {
    (SHADOW_TEXELS_PER_PIXEL * output_pixel(camera, target, depth)).max(SHADOW_SOFTNESS_A / 1.5)
}

/// The splits are contiguous, start at the casters' front and end at their
/// back.
fn assert_splits_cover(setup: &ShadowSetup, front: f64, back: f64, what: &str) {
    let cascades = &setup.cascades;
    assert!(
        (cascades[0].split_near - front).abs() < 1e-9,
        "{what}: starts at {}",
        cascades[0].split_near
    );
    for pair in cascades.windows(2) {
        assert_eq!(
            pair[0].split_far, pair[1].split_near,
            "{what}: a gap between splits"
        );
    }
    for cascade in cascades {
        assert!(
            cascade.split_far > cascade.split_near,
            "{what}: an empty cascade"
        );
    }
    let last = cascades.last().unwrap();
    assert!(
        (last.split_far - back).abs() < 1e-9,
        "{what}: ends at {}",
        last.split_far
    );
}

/// D6: a compact block in full view needs one cascade, in both projections
/// and for a 2048-pixel export of the same view.
#[test]
fn a_compact_block_in_full_view_has_one_cascade() {
    for orthographic in [false, true] {
        let camera = looking_at(DVec3::new(20.0, -90.0, 50.0), DVec3::ZERO, orthographic);
        for target in [target(), TargetSpec::image(2048, 1280, 1)] {
            let setup = shadow_setup(&camera, Some(BLOCK), &SHADOWS, &target).unwrap();
            assert_eq!(
                setup.cascades.len(),
                1,
                "orthographic {orthographic}, {} px",
                target.size.width
            );
        }
    }
}

/// D6: a perspective close-up on a large slab at a grazing angle gets more
/// than one cascade and at most four; the splits are contiguous and cover
/// the casters' view-depth extent, and every cascade but the last meets the
/// criterion where it starts.
#[test]
fn a_grazing_close_up_on_a_large_slab_has_several_cascades_that_meet_the_criterion() {
    let camera = grazing_slab_camera();
    for target in [target(), TargetSpec::image(2048, 1280, 1)] {
        let what = format!("{} px", target.size.width);
        let setup = shadow_setup(&camera, Some(SLAB), &SHADOWS, &target).unwrap();
        let count = setup.cascades.len();
        assert!(
            (2..=MAX_CASCADES).contains(&count),
            "{what}: {count} cascades"
        );
        let (front, back) = view_extent(&camera, SLAB);
        assert_splits_cover(&setup, front, back, &what);
        for cascade in &setup.cascades[..count - 1] {
            let limit = texel_limit(&camera, &target, cascade.split_near);
            assert!(
                cascade.texel_world <= limit,
                "{what}: texel {} over the limit {limit} at {}",
                cascade.texel_world,
                cascade.split_near
            );
        }
        // Coarser with depth.
        for pair in setup.cascades.windows(2) {
            assert!(pair[1].texel_world > pair[0].texel_world, "{what}");
        }
    }
}

/// D6: the floor. On a close-up of a 100 Å block (its bounding sphere 173 Å
/// across, so one cascade at the floor's texel holds all of it), `κ·p(a)` is
/// far below `t_floor` and one cascade is enough. Mutation: without the
/// floor, the near slice must resolve output pixels a few hundredths of an
/// Å wide and the count rises.
#[test]
fn the_texel_floor_keeps_a_close_up_on_a_mid_sized_block_to_one_cascade() {
    let block = (DVec3::splat(-50.0), DVec3::splat(50.0));
    for orthographic in [false, true] {
        let mut camera = looking_at(
            DVec3::new(5.0, -58.0, 20.0),
            DVec3::new(0.0, 0.0, 10.0),
            orthographic,
        );
        camera.ortho_half_height = 2.0;
        let target = target();
        let setup = shadow_setup(&camera, Some(block), &SHADOWS, &target).unwrap();
        let near = setup.cascades[0].split_near;
        let pixel = SHADOW_TEXELS_PER_PIXEL * output_pixel(&camera, &target, near);
        assert!(
            pixel < 0.5 * SHADOW_SOFTNESS_A / 1.5,
            "orthographic {orthographic}: κ·p(a) = {pixel}, not a close-up"
        );
        assert_eq!(
            setup.cascades.len(),
            1,
            "orthographic {orthographic}: {:?}",
            setup.cascades
        );
    }
}

/// The rod of the end-on views: 12 × 12 × 800 Å, its end face at z = 0.
const ROD: (DVec3, DVec3) = (DVec3::new(-6.0, -6.0, -800.0), DVec3::new(6.0, 6.0, 0.0));

/// An orthographic view of [`ROD`] end-on, `half_height` Å high.
fn rod_end_on(half_height: f64) -> Camera {
    let mut camera = looking_at(DVec3::new(0.0, 0.0, 40.0), DVec3::ZERO, true);
    camera.up = DVec3::Y;
    camera.orthonormalize_up();
    camera.ortho_half_height = half_height;
    camera
}

/// D5 step 0, D7: receivers in each cascade's blend band are inside the
/// *next* cascade's square, margin included, so the blend never reads a
/// cascade outside its square. Random points (fixed seed) of the band,
/// inside the view and the casters' sphere, on the grazing slab and on the
/// rod end-on at three zooms. On the rod the footprint's near end is the
/// square's edge, so a band left out of the fit sticks out of it; on the
/// slab the far end dominates. Mutation: fit cascade `k` over `[a, b]`
/// instead of `[a', b]`.
#[test]
fn the_blend_band_is_inside_the_next_cascades_square() {
    let views = [
        ("grazing slab", SLAB, grazing_slab_camera()),
        ("rod 5", ROD, rod_end_on(5.0)),
        ("rod 8", ROD, rod_end_on(8.0)),
        ("rod 12", ROD, rod_end_on(12.0)),
    ];
    let mut random = Random(0x0b1e_7d00_5eed_cafe);
    for (name, bounds, camera) in views {
        let setup = setup_for(&camera, bounds);
        assert!(setup.cascades.len() >= 2, "{name}: one cascade");
        let (centre, radius) = (
            0.5 * (bounds.0 + bounds.1),
            0.5 * (bounds.1 - bounds.0).length(),
        );
        let view_proj_inverse = camera.build_view_projection_matrix().inverse();
        for k in 0..setup.cascades.len() - 1 {
            let (this, next) = (setup.cascades[k], setup.cascades[k + 1]);
            let band = SHADOW_BLEND_FRACTION * (this.split_far - this.split_near);
            let t = next.texel_world;
            let inner = 0.5 * next.side - (shadow_filter_radius(t) + t + t);
            let mut checked = 0;
            for _ in 0..1000 {
                // A random point of the view at a random depth of the band:
                // along the view ray through a random point of the screen.
                let depth = this.split_far - band * random.unit();
                let (x, y) = (2.0 * random.unit() - 1.0, 2.0 * random.unit() - 1.0);
                let near = view_proj_inverse.project_point3(DVec3::new(x, y, 0.0));
                let far = view_proj_inverse.project_point3(DVec3::new(x, y, 1.0));
                let ray = far - near;
                let along = (depth - view_depth(&camera, near))
                    / (view_depth(&camera, far) - view_depth(&camera, near));
                let p = near + ray * along;
                if (p - centre).length() > radius {
                    continue;
                }
                let offset = (setup.to_light_space(p).truncate() - next.centre_l).abs();
                assert!(
                    offset.max_element() <= inner,
                    "{name}, cascade {k}: {p} at depth {depth} is {offset} from cascade {}'s centre, its inner half side is {inner}",
                    k + 1
                );
                checked += 1;
            }
            assert!(
                checked > 100,
                "{name}, cascade {k}: only {checked} points checked"
            );
        }
    }
}

/// D6: an orthographic rod seen end-on spreads along the light by its whole
/// depth, so one map would be coarse at its end face: more than one
/// cascade, and cascade 0, which serves the end face, meets the criterion.
#[test]
fn an_orthographic_rod_end_on_has_several_cascades() {
    let rod = ROD;
    let camera = rod_end_on(8.0);
    let target = target();
    let setup = shadow_setup(&camera, Some(rod), &SHADOWS, &target).unwrap();
    let count = setup.cascades.len();
    assert!((2..=MAX_CASCADES).contains(&count), "{count} cascades");
    let (front, back) = view_extent(&camera, rod);
    assert_splits_cover(&setup, front, back, "rod");
    let first = setup.cascades[0];
    assert!(first.texel_world <= texel_limit(&camera, &target, first.split_near));
}

/// D6: the even-share rule. On an output wider than `κ·N` = 4 096 pixels,
/// zoomed out so that `κ·p(a)` is above the floor, the view's own width at
/// any depth spans more than `κ·N` output pixels and no slice, however thin,
/// meets the criterion: each cascade takes an even share of what is left,
/// so the four are equally deep. The slab is far wider than the view, so
/// the casters' square never cuts the footprint (which would make a slice
/// acceptable).
#[test]
fn an_output_wider_than_the_map_shares_the_depth_evenly() {
    let slab = (
        DVec3::new(-20000.0, -3000.0, -10.0),
        DVec3::new(20000.0, 3000.0, 10.0),
    );
    // Outside the slab, so its front is about 100 Å away.
    let mut camera = looking_at(
        DVec3::new(0.0, -3100.0, 60.0),
        DVec3::new(0.0, -2800.0, 0.0),
        false,
    );
    camera.aspect = 16.0;
    let target = TargetSpec::image(8192, 512, 1);
    let setup = shadow_setup(&camera, Some(slab), &SHADOWS, &target).unwrap();
    let (front, back) = view_extent(&camera, slab);
    assert!(texel_limit(&camera, &target, front) > SHADOW_SOFTNESS_A / 1.5);
    assert_eq!(setup.cascades.len(), MAX_CASCADES, "{:?}", setup.cascades);
    assert_splits_cover(&setup, front, back, "wide");
    let share = (back - front) / MAX_CASCADES as f64;
    for (k, cascade) in setup.cascades.iter().enumerate() {
        let depth = cascade.split_far - cascade.split_near;
        assert!(
            (depth - share).abs() < 1e-6 * share,
            "cascade {k}: {depth} deep, the share is {share}"
        );
    }
}

/// D6: the count changes only when the view genuinely changes. Ten small
/// orbit steps about the grazing slab's centre keep it.
#[test]
fn the_cascade_count_is_stable_under_a_small_orbit() {
    let base = grazing_slab_camera();
    let count = setup_for(&base, SLAB).cascades.len();
    for step in 1..=10 {
        let camera = orbited(&base, DVec3::ZERO, DVec3::Z, 0.002 * step as f64);
        assert_eq!(
            setup_for(&camera, SLAB).cascades.len(),
            count,
            "step {step}"
        );
    }
}

/// D8: with several cascades, each slot carries its own light camera's
/// matrix and fit, and the slots past the count stay zeroed.
#[test]
fn the_shadow_uniform_carries_every_cascade() {
    let setup = setup_for(&grazing_slab_camera(), SLAB);
    let uniform = ShadowUniform::new(Some(&setup), false);
    let count = setup.cascades.len();
    assert!(count >= 2);
    assert_eq!(uniform.count as usize, count);
    assert_eq!(uniform.front, setup.cascades[0].split_near as f32);
    for (k, cascade) in setup.cascades.iter().enumerate() {
        let gpu = uniform.cascades[k];
        assert_eq!(gpu.world_to_clip, light_uniform(&setup, k).view_proj, "{k}");
        assert_eq!(gpu.split_far, cascade.split_far as f32, "{k}");
        assert_eq!(gpu.texel_world, cascade.texel_world as f32, "{k}");
        assert_eq!(gpu.depth_range, cascade.depth_range as f32, "{k}");
    }
    for unused in &uniform.cascades[count..] {
        assert_eq!(*unused, bytemuck::Zeroable::zeroed());
    }
}
