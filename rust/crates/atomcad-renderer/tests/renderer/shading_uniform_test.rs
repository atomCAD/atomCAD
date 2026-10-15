// The per-pass shading uniform (doc/design_depth_perception.md D2, D3,
// Phases 1 and 2). Its WGSL twin is checked in `shader_test.rs`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::renderer::CameraUniform;
use atomcad_renderer::shading_uniform::{
    HEADLIGHT_AMBIENT, HEADLIGHT_KEY_INTENSITY, LightingPreset, STUDIO_FILL_INTENSITY,
    STUDIO_GROUND, STUDIO_KEY_INTENSITY, STUDIO_SKY, ShadingPass, ShadingSettings, ShadingUniform,
};
use glam::f32::Vec3;
use glam::f64::{DQuat, DVec3};
use std::mem::offset_of;

const BOUNDS: Option<(DVec3, DVec3)> =
    Some((DVec3::new(-5.0, -5.0, -5.0), DVec3::new(5.0, 5.0, 5.0)));

fn content(camera: &Camera, settings: &ShadingSettings) -> ShadingUniform {
    ShadingUniform::new(camera, BOUNDS, [30, 30, 30], settings, ShadingPass::Content)
}

#[test]
fn shading_uniform_is_80_bytes_without_padding() {
    assert_eq!(std::mem::size_of::<ShadingUniform>(), 80);
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
    let new = |bounds, pass| ShadingUniform::new(&camera, bounds, [0, 0, 0], &settings, pass);

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
    );
    assert_eq!(uniform.background, [0.0, 128.0 / 255.0, 1.0]);
}

// ---- The *Studio* rig (D3, Phase 2) ----------------------------------------

const STUDIO: ShadingSettings = ShadingSettings {
    preset: LightingPreset::Studio,
    depth_cue_strength: 0.0,
    ao_strength: 0.0,
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
