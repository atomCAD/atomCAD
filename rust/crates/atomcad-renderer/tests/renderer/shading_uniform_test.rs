// The per-pass shading uniform (doc/design_depth_perception.md D2, D3,
// Phase 1). Its WGSL twin is checked in `shader_test.rs`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::renderer::CameraUniform;
use atomcad_renderer::shading_uniform::{
    HEADLIGHT_AMBIENT, HEADLIGHT_KEY_INTENSITY, LightingPreset, ShadingPass, ShadingSettings,
    ShadingUniform,
};
use glam::f64::DVec3;
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
