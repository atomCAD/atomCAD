// The Rust twin of the shared lighting module (doc/design_depth_perception.md
// Testing layer 3, Phase 2): the D3 calibration constraint, the hemisphere
// ambient and the rim's screen-space width. The GPU harness's twin-agreement
// test is what ties the twin to `lighting.wgsl`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::shading::{
    RIM_WIDTH_PX, light_surface, luminance, rim_weight, to_display, view_vector,
};
use atomcad_renderer::shading_uniform::{
    LightingPreset, ShadingPass, ShadingSettings, ShadingUniform,
};
use glam::f32::Vec3;
use glam::f64::DVec3;

fn uniform(camera: &Camera, preset: LightingPreset) -> ShadingUniform {
    let settings = ShadingSettings {
        preset,
        ..ShadingSettings::default()
    };
    ShadingUniform::new(camera, None, [0, 0, 0], &settings, ShadingPass::Content)
}

fn camera() -> Camera {
    let mut camera = Camera::default_pose(1.5);
    camera.eye = DVec3::new(6.0, -14.0, 9.0);
    camera.target = DVec3::ZERO;
    camera.up = DVec3::Z;
    camera.orthonormalize_up();
    camera
}

/// The linear colour of a white, roughness 0.5, non-metallic surface facing
/// the viewer, the reference surface of the D3 calibration constraint.
fn front_facing_white(camera: &Camera, preset: LightingPreset) -> Vec3 {
    let v = view_vector(camera, camera.target);
    light_surface(&uniform(camera, preset), v, v, Vec3::ONE, 0.5, 0.0)
}

/// D3: the studio rig must not shift the palette. A white surface facing the
/// viewer comes out within ±5 % of the *Headlight* luminance before tone
/// mapping, in both projections.
#[test]
fn studio_keeps_a_front_facing_surface_within_5_percent_of_headlight() {
    for orthographic in [false, true] {
        let mut camera = camera();
        camera.orthographic = orthographic;
        let headlight = luminance(front_facing_white(&camera, LightingPreset::Headlight));
        let studio = luminance(front_facing_white(&camera, LightingPreset::Studio));
        let ratio = studio / headlight;
        println!(
            "calibration (orthographic {orthographic}): headlight {headlight}, studio {studio}, ratio {ratio}"
        );
        assert!(
            (0.95..=1.05).contains(&ratio),
            "Studio is {ratio} × Headlight (orthographic {orthographic})"
        );
    }
}

/// The ambient part of a uniform only: key and fill off.
fn ambient_only(camera: &Camera, preset: LightingPreset) -> ShadingUniform {
    ShadingUniform {
        key_intensity: 0.0,
        fill_intensity: 0.0,
        ..uniform(camera, preset)
    }
}

/// D3: the *Studio* hemisphere is brighter from above than from below, so
/// the unlit side of a sphere still shows which way it faces.
#[test]
fn the_studio_hemisphere_is_brighter_from_the_sky() {
    let camera = camera();
    let shading = ambient_only(&camera, LightingPreset::Studio);
    let up = Vec3::from(shading.hemisphere_up);
    let v = view_vector(&camera, camera.target);
    let lit = |n: Vec3| luminance(light_surface(&shading, v, n, Vec3::ONE, 0.5, 0.0));
    assert!(lit(up) > lit(-up) * 1.5, "{} vs {}", lit(up), lit(-up));
}

/// D3: with sky = ground (*Headlight*) the hemisphere is the old constant
/// ambient, exactly, at every normal.
#[test]
fn with_sky_equal_to_ground_the_hemisphere_is_constant() {
    let camera = camera();
    let shading = ambient_only(&camera, LightingPreset::Headlight);
    // Facing the viewer, so the specular-ambient blend (metallic 0: none)
    // and the normal's direction play no part but the hemisphere's.
    let v = view_vector(&camera, camera.target);
    let reference = light_surface(&shading, v, v, Vec3::ONE, 0.5, 0.0);
    assert_eq!(reference, Vec3::splat(0.2));
    let up = Vec3::from(shading.hemisphere_up);
    for n in [up, -up, (up + v).normalize(), (v - up).normalize()] {
        let lit = light_surface(&shading, v, n, Vec3::ONE, 0.5, 0.0);
        assert_eq!(lit, Vec3::splat(0.2), "normal {n}");
    }
}

/// `to_display` is Reinhard then gamma 2.2: 0 stays 0, 1 maps to 0.5^(1/2.2).
#[test]
fn to_display_is_reinhard_then_gamma() {
    assert_eq!(to_display(Vec3::ZERO), Vec3::ZERO);
    let one = to_display(Vec3::ONE);
    assert!((one.x - 0.5f32.powf(1.0 / 2.2)).abs() < 1e-6, "{one}");
}

/// D4: the rim is `RIM_WIDTH_PX` output pixels wide, whatever the sphere's
/// size, and twice as many render pixels on a 2× target.
#[test]
fn the_rim_is_a_fixed_number_of_pixels_wide() {
    // A sphere of radius 100 render pixels: 1 world unit per pixel.
    let (radius, pixel_world) = (100.0, 1.0);
    let at = |inside_px: f32, pixel_scale: f32| {
        rim_weight(-inside_px * pixel_world, pixel_world, radius, pixel_scale)
    };
    assert_eq!(at(0.0, 1.0), 1.0);
    assert_eq!(at(RIM_WIDTH_PX - 0.5, 1.0), 1.0);
    assert!((at(RIM_WIDTH_PX, 1.0) - 0.5).abs() < 1e-6);
    assert_eq!(at(RIM_WIDTH_PX + 0.5, 1.0), 0.0);
    assert_eq!(at(50.0, 1.0), 0.0);
    // Outside the silhouette (a sample ray's hit at the very edge): full.
    assert_eq!(at(-0.3, 1.0), 1.0);
    // A 2× supersampled target: the same output width, so twice the render
    // pixels.
    assert_eq!(at(2.0 * RIM_WIDTH_PX - 0.5, 2.0), 1.0);
    assert!((at(2.0 * RIM_WIDTH_PX, 2.0) - 0.5).abs() < 1e-6);
}

/// D4: on a sphere a few pixels across the band is at most 0.35 of its
/// radius, so the sphere is not all rim.
#[test]
fn a_small_spheres_rim_is_at_most_a_third_of_its_radius() {
    // Radius 4 render pixels: the band is 1.4 px, not 3.
    let (radius, pixel_world) = (4.0, 1.0);
    let at = |inside_px: f32| rim_weight(-inside_px, pixel_world, radius, 1.0);
    assert_eq!(at(0.5), 1.0);
    assert!((at(1.4) - 0.5).abs() < 1e-6);
    assert_eq!(at(2.5), 0.0);
}
