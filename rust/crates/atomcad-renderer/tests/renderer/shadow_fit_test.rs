// The shadow fit and its uniform (doc/design_shadows.md D5, D8, Phase 1):
// the light frame, the depth range, the footprint, the stabilisation, and
// `ShadowUniform::new`. Its WGSL twin is checked in `shader_test.rs`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::render_target::TargetSpec;
use atomcad_renderer::renderer::CameraUniform;
use atomcad_renderer::shading_uniform::{
    LightingPreset, ShadingPass, ShadingSettings, ShadingUniform,
};
use atomcad_renderer::shadow_fit::{
    MAX_CASCADES, SHADOW_BLEND_FRACTION, SHADOW_MAP_SIZE, ShadowCascade, ShadowSetup,
    ShadowUniform, light_uniform, shadow_filter_radius, shadow_setup,
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
