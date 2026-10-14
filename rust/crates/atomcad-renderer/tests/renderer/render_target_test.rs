// The render target's size, pixel scale and sample count, and the live
// target's derivation from the quality settings
// (doc/design_viewport_antialiasing.md D1, D8; the size and scale rule is
// doc/design_screen_space_lines.md D2).

use atomcad_renderer::image_export::{
    IMAGE_EXPORT_MAX_SIZE, IMAGE_EXPORT_SUPERSAMPLING, fit_export_size,
};
use atomcad_renderer::render_target::{
    LIVE_SAMPLE_COUNT, RenderQuality, TargetSpec, live_target_spec,
};
use atomcad_renderer::thumbnail::{THUMBNAIL_RENDER_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING};

// ---- TargetSpec -------------------------------------------------------------

#[test]
fn live_viewport_target_is_its_own_size_at_scale_1() {
    let spec = TargetSpec::viewport(1024.0, 768.0, 1.0, 1);
    assert_eq!(
        (
            spec.size.width,
            spec.size.height,
            spec.size.depth_or_array_layers
        ),
        (1024, 768, 1)
    );
    assert_eq!(spec.pixel_scale, 1.0);
}

#[test]
fn an_image_at_supersampling_1_is_its_own_size_at_scale_1() {
    let spec = TargetSpec::image(1024, 768, 1);
    assert_eq!(
        (
            spec.size.width,
            spec.size.height,
            spec.size.depth_or_array_layers
        ),
        (1024, 768, 1)
    );
    assert_eq!(spec.pixel_scale, 1.0);
    assert_eq!(spec.sample_count, 1);
}

#[test]
fn thumbnail_target_is_supersampled_at_its_pixel_scale() {
    let spec = TargetSpec::image(THUMBNAIL_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING);
    assert_eq!(
        (spec.size.width, spec.size.height),
        (THUMBNAIL_RENDER_SIZE, THUMBNAIL_RENDER_SIZE)
    );
    assert_eq!(spec.pixel_scale, 2.0);
    assert_eq!(spec.sample_count, 1);
}

#[test]
fn export_target_is_supersampled_at_its_pixel_scale() {
    let spec = TargetSpec::image(800, 600, IMAGE_EXPORT_SUPERSAMPLING);
    assert_eq!((spec.size.width, spec.size.height), (1600, 1200));
    assert_eq!(spec.pixel_scale, IMAGE_EXPORT_SUPERSAMPLING as f32);
    assert_eq!(spec.sample_count, 1);
}

#[test]
fn clamped_export_target_keeps_the_scale_and_fits_the_texture_limit() {
    // An oversized request is fitted first (as the api does), then the
    // target is the fitted size × the supersampling, at the same scale.
    let (width, height, reduced) = fit_export_size(6000, 3000);
    assert!(reduced);
    assert_eq!((width, height), (IMAGE_EXPORT_MAX_SIZE, 1024));

    let spec = TargetSpec::image(width, height, IMAGE_EXPORT_SUPERSAMPLING);
    assert_eq!((spec.size.width, spec.size.height), (4096, 2048));
    assert_eq!(spec.pixel_scale, 2.0);
    assert_eq!(spec.sample_count, 1);
    // wgpu's default max_texture_dimension_2d.
    assert!(spec.size.width <= 8192 && spec.size.height <= 8192);
}

// ---- live_target_spec -------------------------------------------------------

#[test]
fn render_quality_defaults_to_anti_aliasing_on() {
    assert!(RenderQuality::default().anti_aliasing);
}

#[test]
fn anti_aliasing_on_gives_the_live_target_4_samples() {
    let spec = live_target_spec(
        640.0,
        480.0,
        RenderQuality {
            anti_aliasing: true,
        },
        true,
    );
    assert_eq!(LIVE_SAMPLE_COUNT, 4);
    assert_eq!(spec.sample_count, LIVE_SAMPLE_COUNT);
    assert_eq!((spec.size.width, spec.size.height), (640, 480));
    assert_eq!(spec.pixel_scale, 1.0);
}

#[test]
fn anti_aliasing_off_gives_the_live_target_1_sample() {
    let spec = live_target_spec(
        640.0,
        480.0,
        RenderQuality {
            anti_aliasing: false,
        },
        true,
    );
    assert_eq!(spec.sample_count, 1);
    assert_eq!((spec.size.width, spec.size.height), (640, 480));
}

#[test]
fn anti_aliasing_on_without_msaa_support_gives_1_sample() {
    let spec = live_target_spec(
        640.0,
        480.0,
        RenderQuality {
            anti_aliasing: true,
        },
        false,
    );
    assert_eq!(spec.sample_count, 1);
}

#[test]
fn the_quality_setting_changes_only_the_sample_count() {
    let on = live_target_spec(800.0, 600.0, RenderQuality::default(), true);
    let off = live_target_spec(
        800.0,
        600.0,
        RenderQuality {
            anti_aliasing: false,
        },
        true,
    );
    assert_ne!(on, off);
    assert_eq!(on.size, off.size);
    assert_eq!(on.pixel_scale, off.pixel_scale);
}
