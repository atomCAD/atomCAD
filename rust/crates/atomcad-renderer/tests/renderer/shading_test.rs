// The Rust twin of the shared lighting module (doc/design_depth_perception.md
// Testing layer 3, Phase 2): the D3 calibration constraint, the hemisphere
// ambient, the rim's screen-space width and (Phase 3) the depth cue. The GPU harness's twin-agreement
// test is what ties the twin to `lighting.wgsl`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::shading::{
    RIM_WIDTH_PX, apply_depth_cue, depth_cue_fraction, light_surface, luminance, rim_weight,
    to_display, view_vector,
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
    ShadingUniform::new(
        camera,
        None,
        [0, 0, 0],
        &settings,
        ShadingPass::Content,
        None,
    )
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

// ---- The depth cue (D5, Phase 3) -------------------------------------------

fn cued(camera: &Camera, strength: f32, background: [u8; 3]) -> ShadingUniform {
    let settings = ShadingSettings {
        depth_cue_strength: strength,
        ..ShadingSettings::default()
    };
    let bounds = (DVec3::splat(-5.0), DVec3::splat(5.0));
    ShadingUniform::new(
        camera,
        Some(bounds),
        background,
        &settings,
        ShadingPass::Content,
        None,
    )
}

/// The point of the content farthest from the eye: the corner of the cube
/// opposite the nearest one, which lies less than a span behind it.
fn deepest_point(camera: &Camera) -> DVec3 {
    let back = (camera.target - camera.eye).normalize();
    DVec3::new(5.0, 5.0, 5.0) * back.signum()
}

/// D5: at strength 1 the far end of the span is exactly the background, so
/// fogged content converges on the pixel the pass clears to; the nearest
/// content keeps its colour; and strength 0 changes nothing.
#[test]
fn the_depth_cue_mixes_towards_the_background_in_display_space() {
    let camera = camera();
    let colour = Vec3::new(0.8, 0.3, 0.1);
    let full = cued(&camera, 1.0, [240, 240, 240]);
    // A span behind the front: past the content, full strength.
    let forward = (camera.target - camera.eye).normalize();
    let beyond = camera.eye + forward * (full.depth_cue_front + full.depth_cue_span) as f64 * 1.01;
    let fogged = apply_depth_cue(&full, &camera, colour, beyond);
    assert!((fogged - Vec3::splat(240.0 / 255.0)).abs().max_element() < 1e-6);

    let nearest = -deepest_point(&camera);
    assert_eq!(apply_depth_cue(&full, &camera, colour, nearest), colour);

    let off = cued(&camera, 0.0, [240, 240, 240]);
    assert_eq!(apply_depth_cue(&off, &camera, colour, beyond), colour);

    // In between, on a dark background: part of the way, towards it.
    let half = cued(&camera, 0.5, [20, 20, 20]);
    let back = apply_depth_cue(&half, &camera, colour, deepest_point(&camera));
    let f = 0.5 * depth_cue_fraction(&half, &camera, deepest_point(&camera));
    assert!(f > 0.2 && f <= 0.5, "f {f}");
    let background = Vec3::splat(20.0 / 255.0);
    assert!((back - colour.lerp(background, f)).abs().max_element() < 1e-6);
    assert!((back - background).length() < (colour - background).length());
}

// ---- Baked ambient occlusion (Phase 4, D7, D10) ----

use atomcad_renderer::shading::{
    AO_FLOOR, AO_KEY, FULLY_EXPOSED, ao_factors, ao_visibility, decode_exposure,
    light_surface_exposed,
};
use glam::f32::Vec4;

/// D10: a terrace atom's packed value, `E = ½` and `b = ¼·up` (so `4b = up`),
/// gives the closed-form visibility `(1 + cos θ)/2` at every tilt.
#[test]
fn a_half_space_sky_has_the_closed_form_visibility() {
    let terrace = Vec4::new(0.0, 0.0, 1.0, 0.5);
    for degrees in [0.0f32, 30.0, 60.0, 90.0, 135.0, 180.0] {
        let (s, c) = degrees.to_radians().sin_cos();
        let v = ao_visibility(terrace, Vec3::new(s, 0.0, c));
        assert!(
            (v - (1.0 + c) / 2.0).abs() < 1e-6,
            "{degrees}°: {v} against {}",
            (1.0 + c) / 2.0
        );
    }
}

/// D7: the mapping leaves an open surface alone, keeps `AO_FLOOR` of the
/// indirect light on a buried one, takes only `AO_KEY` of that from the key,
/// and does nothing at all at strength 0.
#[test]
fn the_ao_mapping_spans_floor_to_one_and_is_off_at_strength_0() {
    let n = Vec3::Z;
    assert_eq!(ao_factors(1.0, AO_KEY, FULLY_EXPOSED, n), (1.0, 1.0));
    let buried = Vec4::new(0.0, 0.0, 0.0, 0.0);
    let (ambient, key) = ao_factors(1.0, AO_KEY, buried, n);
    assert!((ambient - AO_FLOOR).abs() < 1e-6, "{ambient}");
    assert!(
        (key - (1.0 - (1.0 - AO_FLOOR) * AO_KEY)).abs() < 1e-6,
        "{key}"
    );
    assert_eq!(ao_factors(0.0, AO_KEY, buried, n), (1.0, 1.0));
    // Half strength is half way.
    let (half, _) = ao_factors(0.5, AO_KEY, buried, n);
    assert!((half - (1.0 + AO_FLOOR) / 2.0).abs() < 1e-6, "{half}");
}

/// A buried surface is darker than an open one under both presets, and the
/// fully exposed value reproduces `light_surface` exactly.
#[test]
fn burial_darkens_and_full_exposure_changes_nothing() {
    let camera = camera();
    let v = view_vector(&camera, camera.target);
    for preset in [LightingPreset::Headlight, LightingPreset::Studio] {
        let mut shading = uniform(&camera, preset);
        shading.ao_strength = 1.0;
        let albedo = Vec3::new(0.6, 0.5, 0.4);
        let open = light_surface(&shading, v, v, albedo, 0.5, 0.0);
        let exposed = light_surface_exposed(&shading, v, v, albedo, 0.5, 0.0, FULLY_EXPOSED, 1.0);
        assert_eq!(open, exposed);
        let buried = light_surface_exposed(&shading, v, v, albedo, 0.5, 0.0, Vec4::ZERO, 1.0);
        assert!(luminance(buried) < 0.9 * luminance(open), "{preset:?}");
    }
}

/// `Snorm8x4` decoding: 127 is 1, −127 and −128 are −1.
#[test]
fn snorm_decoding_matches_the_gpu() {
    assert_eq!(decode_exposure([0, 0, 0, 127]), FULLY_EXPOSED);
    assert_eq!(
        decode_exposure([-127, -128, 127, 0]),
        Vec4::new(-1.0, -1.0, 1.0, 0.0)
    );
}

// ---- Shadows (doc/design_shadows.md D1, Testing layer 3) -------------------

use atomcad_renderer::shading::key_visibility;

/// D1: at strength 0 the key is whole whatever the lookup says, so every
/// image drawn with shadows off is today's.
#[test]
fn key_visibility_is_one_at_strength_zero() {
    for v in [0.0, 0.25, 0.5, 1.0] {
        assert_eq!(key_visibility(0.0, v), 1.0);
    }
    assert_eq!(key_visibility(1.0, 1.0), 1.0);
    assert_eq!(key_visibility(1.0, 0.0), 0.0);
    assert!((key_visibility(0.7, 0.0) - 0.3).abs() < 1e-6);
}

/// D1: a full shadow takes the key's direct term and nothing else: the fill
/// and the hemisphere stay, so a shadowed surface reads as out of the sun,
/// not black.
#[test]
fn a_full_shadow_takes_only_the_key() {
    let camera = camera();
    let v = view_vector(&camera, camera.target);
    let n = (v + Vec3::new(0.3, 0.5, 0.0)).normalize();
    let albedo = Vec3::new(0.6, 0.5, 0.4);
    let studio = uniform(&camera, LightingPreset::Studio);
    let shadowed = light_surface_exposed(&studio, v, n, albedo, 0.5, 0.0, FULLY_EXPOSED, 0.0);
    let no_key = ShadingUniform {
        key_intensity: 0.0,
        ..studio
    };
    let expected = light_surface(&no_key, v, n, albedo, 0.5, 0.0);
    assert!(
        (shadowed - expected).length() < 1e-6,
        "{shadowed} vs {expected}"
    );
    let lit = light_surface(&studio, v, n, albedo, 0.5, 0.0);
    assert!(luminance(shadowed) < 0.8 * luminance(lit));
    assert!(luminance(shadowed) > 0.0);
}
