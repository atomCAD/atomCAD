// The per-pass shading uniform (doc/design_depth_perception.md D2, D3,
// Phases 1-3). Its WGSL twin is checked in `shader_test.rs`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::renderer::CameraUniform;
use atomcad_renderer::shading::depth_cue_fraction;
use atomcad_renderer::shading_uniform::{
    HEADLIGHT_AMBIENT, HEADLIGHT_KEY_INTENSITY, LightingPreset, STUDIO_FILL_INTENSITY,
    STUDIO_GROUND, STUDIO_KEY_INTENSITY, STUDIO_SKY, ShadingPass, ShadingSettings, ShadingUniform,
    depth_cue_range,
};
use glam::f32::Vec3;
use glam::f64::{DQuat, DVec3};
use std::mem::offset_of;

const BOUNDS: Option<(DVec3, DVec3)> =
    Some((DVec3::new(-5.0, -5.0, -5.0), DVec3::new(5.0, 5.0, 5.0)));

fn content(camera: &Camera, settings: &ShadingSettings) -> ShadingUniform {
    ShadingUniform::new(
        camera,
        BOUNDS,
        [30, 30, 30],
        settings,
        ShadingPass::Content,
        None,
    )
}

/// `doc/design_shadows.md` D8: 96 bytes since the shadows, the two new
/// fields at 80 and 84 and the struct rounded up to its 16-byte alignment.
#[test]
fn shading_uniform_is_96_bytes_with_the_shadow_fields() {
    assert_eq!(std::mem::size_of::<ShadingUniform>(), 96);
    // Each vec3 is followed by the f32 that fills its 16-byte slot.
    assert_eq!(offset_of!(ShadingUniform, key_light_dir), 0);
    assert_eq!(offset_of!(ShadingUniform, key_intensity), 12);
    assert_eq!(offset_of!(ShadingUniform, fill_light_dir), 16);
    assert_eq!(offset_of!(ShadingUniform, fill_intensity), 28);
    assert_eq!(offset_of!(ShadingUniform, hemisphere_up), 32);
    assert_eq!(offset_of!(ShadingUniform, sky), 44);
    assert_eq!(offset_of!(ShadingUniform, background), 48);
    assert_eq!(offset_of!(ShadingUniform, ground), 60);
    assert_eq!(offset_of!(ShadingUniform, depth_cue_front), 64);
    assert_eq!(offset_of!(ShadingUniform, depth_cue_span), 68);
    assert_eq!(offset_of!(ShadingUniform, depth_cue_strength), 72);
    assert_eq!(offset_of!(ShadingUniform, ao_strength), 76);
    assert_eq!(offset_of!(ShadingUniform, shadow_strength), 80);
    assert_eq!(offset_of!(ShadingUniform, ao_key), 84);
}

/// D2: the renderer's own default is today's look, so every test that sets
/// nothing renders the old image.
#[test]
fn the_default_settings_are_the_neutral_headlight() {
    let settings = ShadingSettings::default();
    assert_eq!(settings.preset, LightingPreset::Headlight);
    assert_eq!(settings.depth_cue_strength, 0.0);
    assert_eq!(settings.ao_strength, 0.0);
}

/// D3: the *Headlight* preset is the old rig: the key along
/// `CameraUniform::head_light_dir` at 2.0, no fill, and an ambient of 0.2
/// from every side. These values are what makes Phase 1 pixel-neutral.
#[test]
fn the_headlight_preset_is_the_old_rig() {
    let mut camera = Camera::default_pose(1.5);
    for eye in [
        DVec3::new(0.0, -30.0, 10.0),
        DVec3::new(12.0, 4.0, -7.0),
        DVec3::new(-3.0, 20.0, 25.0),
    ] {
        camera.eye = eye;
        camera.orthonormalize_up();
        let uniform = content(&camera, &ShadingSettings::default());
        let head_light_dir = CameraUniform::new(&camera, (100, 100), 1.0).head_light_dir;
        assert_eq!(uniform.key_light_dir, head_light_dir, "eye {eye}");
        assert_eq!(uniform.key_intensity, HEADLIGHT_KEY_INTENSITY);
        assert_eq!(uniform.key_intensity, 2.0);
        assert_eq!(uniform.fill_intensity, 0.0);
        assert_eq!(uniform.sky, HEADLIGHT_AMBIENT);
        assert_eq!(uniform.ground, HEADLIGHT_AMBIENT);
        assert_eq!(uniform.sky, 0.2);
        // The shader normalizes the fill direction even when the fill is
        // off, so it must be a unit vector, never zero.
        let fill = glam::Vec3::from(uniform.fill_light_dir);
        assert!((fill.length() - 1.0).abs() < 1e-6, "fill {fill}");
    }
}

/// The hemisphere's up is the camera's up axis: a unit vector at right
/// angles to the view direction, on the side of `camera.up`.
#[test]
fn the_hemisphere_up_is_the_cameras_up_axis() {
    let mut camera = Camera::default_pose(1.0);
    camera.eye = DVec3::new(8.0, -6.0, 3.0);
    camera.up = DVec3::Z;
    let up = glam::Vec3::from(content(&camera, &ShadingSettings::default()).hemisphere_up);
    let forward = (camera.target - camera.eye).normalize().as_vec3();
    assert!((up.length() - 1.0).abs() < 1e-6);
    assert!(up.dot(forward).abs() < 1e-6);
    assert!(up.z > 0.9);
}

/// D2, D5: the gadget pass never has a depth cue, and neither has a scene
/// without content; the content pass carries the setting.
#[test]
fn only_the_content_pass_with_content_has_a_depth_cue() {
    let camera = Camera::default_pose(1.0);
    let settings = ShadingSettings {
        depth_cue_strength: 0.5,
        ao_strength: 0.75,
        ..ShadingSettings::default()
    };
    let new = |bounds, pass| ShadingUniform::new(&camera, bounds, [0, 0, 0], &settings, pass, None);

    assert_eq!(new(BOUNDS, ShadingPass::Content).depth_cue_strength, 0.5);
    assert_eq!(new(BOUNDS, ShadingPass::Gadget).depth_cue_strength, 0.0);
    assert_eq!(new(None, ShadingPass::Content).depth_cue_strength, 0.0);
    assert_eq!(new(None, ShadingPass::Gadget).depth_cue_strength, 0.0);

    // Apart from the cue, the two passes are lit alike.
    let content = new(BOUNDS, ShadingPass::Content);
    let gadget = new(BOUNDS, ShadingPass::Gadget);
    assert_eq!(
        ShadingUniform {
            depth_cue_strength: 0.0,
            ..content
        },
        gadget
    );
    assert_eq!(content.ao_strength, 0.75);
}

/// D5: the background is the pass's clear colour, 0–1 per channel, the
/// same conversion `draw_to_target` clears with.
#[test]
fn the_background_is_the_clear_colour() {
    let camera = Camera::default_pose(1.0);
    let uniform = ShadingUniform::new(
        &camera,
        BOUNDS,
        [0, 128, 255],
        &ShadingSettings::default(),
        ShadingPass::Content,
        None,
    );
    assert_eq!(uniform.background, [0.0, 128.0 / 255.0, 1.0]);
}

// ---- The *Studio* rig (D3, Phase 2) ----------------------------------------

const STUDIO: ShadingSettings = ShadingSettings {
    preset: LightingPreset::Studio,
    depth_cue_strength: 0.0,
    ao_strength: 0.0,
    shadow_strength: 0.0,
};

/// `direction` in the camera's frame: (right, up, forward).
fn in_camera_space(camera: &Camera, direction: [f32; 3]) -> Vec3 {
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    let d = Vec3::from(direction).as_dvec3();
    DVec3::new(d.dot(right), d.dot(up), d.dot(forward)).as_vec3()
}

fn oblique_camera() -> Camera {
    let mut camera = Camera::default_pose(1.5);
    camera.eye = DVec3::new(8.0, -12.0, 6.0);
    camera.target = DVec3::new(1.0, 2.0, -1.0);
    camera.up = DVec3::Z;
    camera.orthonormalize_up();
    camera
}

/// D3: the *Studio* key comes from the upper left of the view direction and
/// the fill from the lower right. A light's travel direction is the reverse:
/// the key travels down and to the right, the fill up and to the left, and
/// both travel away from the eye. Both are unit vectors.
#[test]
fn the_studio_key_is_upper_left_and_the_fill_lower_right() {
    let camera = oblique_camera();
    let uniform = content(&camera, &STUDIO);
    let key = in_camera_space(&camera, uniform.key_light_dir);
    let fill = in_camera_space(&camera, uniform.fill_light_dir);
    assert!((key.length() - 1.0).abs() < 1e-5, "key {key}");
    assert!((fill.length() - 1.0).abs() < 1e-5, "fill {fill}");
    // Travelling right (from the left), down (from above), away from the eye.
    assert!(key.x > 0.2 && key.y < -0.4 && key.z > 0.5, "key {key}");
    // Travelling left (from the right), up (from below), away from the eye.
    assert!(fill.x < -0.4 && fill.y > 0.2 && fill.z > 0.5, "fill {fill}");
    // 35° above the view axis, 30° to its side: sin 35° of it is vertical.
    assert!((key.y + 35f32.to_radians().sin()).abs() < 1e-5, "key {key}");
    assert!(
        (fill.y - 20f32.to_radians().sin()).abs() < 1e-5,
        "fill {fill}"
    );

    assert_eq!(uniform.key_intensity, STUDIO_KEY_INTENSITY);
    assert_eq!(uniform.fill_intensity, STUDIO_FILL_INTENSITY);
    assert_eq!(uniform.sky, STUDIO_SKY);
    assert_eq!(uniform.ground, STUDIO_GROUND);
    assert!(uniform.sky > uniform.ground);
}

/// D3: the rig is camera-relative. Orbiting the camera 90° about its up
/// axis turns both lights in world space and leaves them unchanged in camera
/// space, so the shading stays put while the user orbits.
#[test]
fn the_studio_lights_turn_with_the_camera() {
    let camera = oblique_camera();
    let mut orbited = camera.clone();
    let rotation = DQuat::from_axis_angle(camera.up, 90f64.to_radians());
    orbited.eye = camera.target + rotation * (camera.eye - camera.target);
    orbited.up = rotation * camera.up;

    let before = content(&camera, &STUDIO);
    let after = content(&orbited, &STUDIO);
    for (name, a, b) in [
        ("key", before.key_light_dir, after.key_light_dir),
        ("fill", before.fill_light_dir, after.fill_light_dir),
    ] {
        let (a_cam, b_cam) = (in_camera_space(&camera, a), in_camera_space(&orbited, b));
        assert!(a_cam.abs_diff_eq(b_cam, 1e-5), "{name}: {a_cam} vs {b_cam}");
        // It did turn in world space.
        assert!(
            !Vec3::from(a).abs_diff_eq(Vec3::from(b), 0.1),
            "{name} did not move"
        );
    }
}

// ---- The depth cue (D5, Phase 3) -------------------------------------------

const CUE: ShadingSettings = ShadingSettings {
    preset: LightingPreset::Headlight,
    depth_cue_strength: 0.5,
    ao_strength: 0.0,
    shadow_strength: 0.0,
};

fn looking_at(eye: DVec3, target: DVec3, orthographic: bool) -> Camera {
    let mut camera = Camera::default_pose(1.5);
    camera.eye = eye;
    camera.target = target;
    camera.up = if (eye - target).normalize().dot(DVec3::Z).abs() > 0.99 {
        DVec3::Y
    } else {
        DVec3::Z
    };
    camera.orthographic = orthographic;
    camera.orthonormalize_up();
    camera
}

fn cue(camera: &Camera, bounds: (DVec3, DVec3)) -> ShadingUniform {
    ShadingUniform::new(
        camera,
        Some(bounds),
        [0, 0, 0],
        &CUE,
        ShadingPass::Content,
        None,
    )
}

/// The cue's `f` at a point: strength times the fraction of the span.
fn fog_at(camera: &Camera, uniform: &ShadingUniform, point: DVec3) -> f32 {
    uniform.depth_cue_strength * depth_cue_fraction(uniform, camera, point)
}

const CUBE: (DVec3, DVec3) = (DVec3::new(-5.0, -5.0, -5.0), DVec3::new(5.0, 5.0, 5.0));

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

/// D5: `front` is the depth of the AABB's support point along the view
/// axis and `span` the bounding sphere's diameter, the same in both
/// projections.
#[test]
fn the_cue_starts_at_the_nearest_point_and_spans_the_bounding_sphere() {
    let diameter = 2.0 * 75f64.sqrt();
    for orthographic in [false, true] {
        // Straight down the z axis: the top face is 45 Å away.
        let camera = looking_at(DVec3::new(0.0, 0.0, 50.0), DVec3::ZERO, orthographic);
        let (front, span) = depth_cue_range(&camera, CUBE).unwrap();
        assert!((front - 45.0).abs() < 1e-9, "front {front}");
        assert!((span - diameter).abs() < 1e-9, "span {span}");
        let uniform = cue(&camera, CUBE);
        assert!((uniform.depth_cue_front - 45.0).abs() < 1e-5);
        assert!((uniform.depth_cue_span as f64 - diameter).abs() < 1e-5);

        // Along the body diagonal the nearest point is a corner, 5√3 in
        // front of the centre.
        let eye = DVec3::splat(30.0);
        let camera = looking_at(eye, DVec3::ZERO, orthographic);
        let (front, span) = depth_cue_range(&camera, CUBE).unwrap();
        assert!(
            (front - (eye.length() - 75f64.sqrt())).abs() < 1e-9,
            "front {front}"
        );
        assert!((span - diameter).abs() < 1e-9);
    }
}

/// D5: a camera inside the content starts the cue at the near plane, not
/// behind the eye.
#[test]
fn the_front_is_clamped_at_the_near_plane() {
    let camera = looking_at(
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(0.0, 0.0, -10.0),
        false,
    );
    let (front, _) = depth_cue_range(&camera, CUBE).unwrap();
    assert_eq!(front, camera.znear);
}

/// D5: no content, or content without extent, has no cue, and its span stays
/// positive so the shader's division is defined.
#[test]
fn without_content_there_is_no_cue() {
    let camera = looking_at(DVec3::new(0.0, 0.0, 50.0), DVec3::ZERO, false);
    let none = ShadingUniform::new(&camera, None, [0, 0, 0], &CUE, ShadingPass::Content, None);
    assert_eq!(none.depth_cue_strength, 0.0);
    assert!(none.depth_cue_span > 0.0);
    let point = (DVec3::ONE, DVec3::ONE);
    assert_eq!(depth_cue_range(&camera, point), None);
    assert_eq!(cue(&camera, point).depth_cue_strength, 0.0);
    assert!(cue(&camera, point).depth_cue_span > 0.0);
}

/// D5: orbiting the AABB's centre leaves the span alone and moves `front`
/// continuously: by at most R·Δθ per step, since the centre's depth stays
/// put and the support function is R-Lipschitz in the view axis.
#[test]
fn orbiting_keeps_the_span_and_moves_the_front_continuously() {
    let bounds = (DVec3::new(-20.0, -8.0, -3.0), DVec3::new(30.0, 12.0, 5.0));
    let centre = 0.5 * (bounds.0 + bounds.1);
    let r = 0.5 * (bounds.1 - bounds.0).length();
    let step = 1f64.to_radians();
    let start = DVec3::new(70.0, -40.0, 35.0);
    let mut previous: Option<(f64, f64)> = None;
    for i in 0..=360 {
        let rotation =
            DQuat::from_axis_angle(DVec3::new(0.3, 0.2, 1.0).normalize(), i as f64 * step);
        let camera = looking_at(centre + rotation * start, centre, false);
        let (front, span) = depth_cue_range(&camera, bounds).unwrap();
        assert!((span - 2.0 * r).abs() < 1e-9, "span {span} at {i}°");
        if let Some((previous_front, _)) = previous {
            assert!(
                (front - previous_front).abs() <= r * step + 1e-9,
                "front jumps from {previous_front} to {front} at {i}°"
            );
        }
        previous = Some((front, span));
    }
}

/// D5: a 100 × 100 × 10 Å slab seen face-on. Its top face is not fogged at
/// all and its bottom face barely, because the cue starts at the nearest
/// content. Anchored at the bounding sphere's front, every atom would carry a
/// uniform 25 % wash.
#[test]
fn a_slab_seen_face_on_is_fogged_by_its_thickness_only() {
    let slab = (DVec3::new(-50.0, -50.0, -5.0), DVec3::new(50.0, 50.0, 5.0));
    for orthographic in [false, true] {
        let camera = looking_at(DVec3::new(0.0, 0.0, 120.0), DVec3::ZERO, orthographic);
        let uniform = cue(&camera, slab);
        for x in [-50.0, 0.0, 50.0] {
            let top = fog_at(&camera, &uniform, DVec3::new(x, x, 5.0));
            let bottom = fog_at(&camera, &uniform, DVec3::new(x, -x, -5.0));
            assert_eq!(top, 0.0, "top at {x}");
            assert!(bottom > 0.0 && bottom < 0.05, "bottom {bottom} at {x}");
        }
    }
}

/// D5: whichever way a cube is seen, its nearest corner is unfogged and its
/// farthest is fogged.
#[test]
fn a_cubes_nearest_corner_is_unfogged_at_every_orientation() {
    for eye in [
        DVec3::new(0.0, 0.0, 40.0),
        DVec3::new(30.0, 0.0, 10.0),
        DVec3::new(-20.0, 25.0, 15.0),
        DVec3::new(17.0, -23.0, -31.0),
        DVec3::splat(-25.0),
    ] {
        let camera = looking_at(eye, DVec3::new(1.0, -2.0, 0.5), false);
        let uniform = cue(&camera, CUBE);
        let forward = (camera.target - camera.eye).normalize();
        let depth = |p: &DVec3| (*p - camera.eye).dot(forward);
        let all = corners(CUBE);
        let nearest = all
            .iter()
            .min_by(|a, b| depth(a).total_cmp(&depth(b)))
            .unwrap();
        let farthest = all
            .iter()
            .max_by(|a, b| depth(a).total_cmp(&depth(b)))
            .unwrap();
        assert!(fog_at(&camera, &uniform, *nearest) < 1e-5, "eye {eye}");
        assert!(fog_at(&camera, &uniform, *farthest) > 0.1, "eye {eye}");
    }
}

// ---- Shadows (doc/design_shadows.md D8, Phase 1) ---------------------------

use atomcad_renderer::render_target::TargetSpec;
use atomcad_renderer::shading::{AO_KEY, AO_KEY_WITH_SHADOWS};
use atomcad_renderer::shadow_fit::shadow_setup;

/// D8: the content pass of a draw with a shadow setup carries the
/// settings' strength; the gadget pass and a draw without a setup carry 0.
/// (`ao_key` follows the strength drawn, see the D10 test below.)
#[test]
fn only_the_content_pass_with_a_shadow_setup_has_a_shadow_strength() {
    let camera = looking_at(DVec3::new(0.0, -60.0, 30.0), DVec3::ZERO, false);
    let settings = ShadingSettings {
        shadow_strength: 0.7,
        ..STUDIO
    };
    let setup =
        shadow_setup(&camera, BOUNDS, &settings, &TargetSpec::image(64, 64, 1)).expect("a setup");
    let new =
        |pass, shadow| ShadingUniform::new(&camera, BOUNDS, [0, 0, 0], &settings, pass, shadow);
    assert_eq!(new(ShadingPass::Content, Some(&setup)).shadow_strength, 0.7);
    assert_eq!(new(ShadingPass::Gadget, Some(&setup)).shadow_strength, 0.0);
    assert_eq!(new(ShadingPass::Content, None).shadow_strength, 0.0);
    assert_eq!(new(ShadingPass::Gadget, None).shadow_strength, 0.0);
    // Without shadows nothing else changes but AO's share of the key.
    let without = ShadingUniform {
        shadow_strength: 0.0,
        ao_key: AO_KEY,
        ..new(ShadingPass::Content, Some(&setup))
    };
    assert_eq!(without, new(ShadingPass::Content, None));
}

/// D10: AO cedes the key to the shadows by the strength the uniform carries,
/// not the setting's: exactly `AO_KEY` wherever no shadow is drawn (no setup,
/// the gadget pass, strength 0) and `AO_KEY_WITH_SHADOWS` at strength 1.
#[test]
fn ao_cedes_the_key_to_the_shadows_actually_drawn() {
    let camera = looking_at(DVec3::new(0.0, -60.0, 30.0), DVec3::ZERO, false);
    let target = TargetSpec::image(64, 64, 1);
    let with = |strength| ShadingSettings {
        shadow_strength: strength,
        ..STUDIO
    };
    let uniform = |settings: &ShadingSettings, pass| {
        let setup = shadow_setup(&camera, BOUNDS, settings, &target);
        ShadingUniform::new(&camera, BOUNDS, [0, 0, 0], settings, pass, setup.as_ref())
    };
    assert_eq!(uniform(&with(0.0), ShadingPass::Content).ao_key, AO_KEY);
    assert_eq!(
        uniform(&with(1.0), ShadingPass::Content).ao_key,
        AO_KEY_WITH_SHADOWS
    );
    let mid = uniform(&with(0.7), ShadingPass::Content).ao_key;
    assert!((mid - (0.3 * AO_KEY + 0.7 * AO_KEY_WITH_SHADOWS)).abs() < 1e-6);
    // The gadget pass draws no shadow, so AO keeps its share there.
    assert_eq!(uniform(&with(1.0), ShadingPass::Gadget).ao_key, AO_KEY);

    // A strength setting without a setup (Headlight, no casters) draws no
    // shadow, so AO keeps its full share of the key.
    let settings = with(0.7);
    for pass in [ShadingPass::Content, ShadingPass::Gadget] {
        let none = ShadingUniform::new(&camera, BOUNDS, [0, 0, 0], &settings, pass, None);
        assert_eq!(none.ao_key, AO_KEY);
    }
    let headlight = ShadingSettings {
        preset: LightingPreset::Headlight,
        ..settings
    };
    assert_eq!(
        uniform(&headlight, ShadingPass::Content).ao_key,
        AO_KEY,
        "Headlight"
    );
    let no_casters = ShadingUniform::new(
        &camera,
        BOUNDS,
        [0, 0, 0],
        &settings,
        ShadingPass::Content,
        shadow_setup(&camera, None, &settings, &target).as_ref(),
    );
    assert_eq!(no_casters.ao_key, AO_KEY, "no casters");
}
