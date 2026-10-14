// The render target's size, pixel scale and sample count, and the live
// target's derivation from the quality settings
// (doc/design_viewport_antialiasing.md D1, D8; the size and scale rule is
// doc/design_screen_space_lines.md D2).

use atomcad_renderer::image_export::{
    IMAGE_EXPORT_MAX_SIZE, IMAGE_EXPORT_SUPERSAMPLING, fit_export_size,
};
use atomcad_renderer::render_target::{
    LIVE_SAMPLE_COUNT, RenderQuality, TargetSpec, ViewportRequest, live_target_spec,
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

#[test]
fn viewport_target_rounds_the_device_size_at_fractional_ratios() {
    // 1.25: 801 × 1.25 = 1001.25 → 1001; 601 × 1.25 = 751.25 → 751.
    let spec = TargetSpec::viewport(801.0, 601.0, 1.25, 1);
    assert_eq!((spec.size.width, spec.size.height), (1001, 751));
    assert_eq!(spec.pixel_scale, 1.25);
    // 1.5: 333 × 1.5 = 499.5 → 500 (half away from zero); 201 × 1.5 = 301.5 → 302.
    let spec = TargetSpec::viewport(333.0, 201.0, 1.5, 1);
    assert_eq!((spec.size.width, spec.size.height), (500, 302));
    assert_eq!(spec.pixel_scale, 1.5);
    // 2: exact.
    let spec = TargetSpec::viewport(640.0, 480.0, 2.0, 4);
    assert_eq!((spec.size.width, spec.size.height), (1280, 960));
    assert_eq!(spec.pixel_scale, 2.0);
    assert_eq!(spec.sample_count, 4);
}

#[test]
fn viewport_target_takes_untruncated_logical_sizes() {
    // Flutter lays out fractional logical sizes; the device size is rounded
    // once, after the multiplication, not truncated before it.
    let spec = TargetSpec::viewport(640.6, 480.4, 1.5, 1);
    assert_eq!((spec.size.width, spec.size.height), (961, 721));
}

#[test]
fn viewport_target_is_never_below_1x1() {
    for ratio in [1.0, 1.25, 1.5, 2.0] {
        let spec = TargetSpec::viewport(0.0, 0.2, ratio, 1);
        assert_eq!((spec.size.width, spec.size.height), (1, 1), "ratio {ratio}");
        assert_eq!(spec.pixel_scale, ratio as f32);
    }
}

// ---- live_target_spec -------------------------------------------------------

#[test]
fn render_quality_defaults_to_both_settings_on() {
    assert!(RenderQuality::default().anti_aliasing);
    assert!(RenderQuality::default().full_display_resolution);
}

#[test]
fn anti_aliasing_on_gives_the_live_target_4_samples() {
    let spec = live_target_spec(
        640.0,
        480.0,
        1.0,
        RenderQuality {
            anti_aliasing: true,
            full_display_resolution: true,
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
        1.0,
        RenderQuality {
            anti_aliasing: false,
            full_display_resolution: true,
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
        1.0,
        RenderQuality {
            anti_aliasing: true,
            full_display_resolution: true,
        },
        false,
    );
    assert_eq!(spec.sample_count, 1);
}

#[test]
fn the_quality_setting_changes_only_the_sample_count() {
    let on = live_target_spec(800.0, 600.0, 1.0, RenderQuality::default(), true);
    let off = live_target_spec(
        800.0,
        600.0,
        1.0,
        RenderQuality {
            anti_aliasing: false,
            full_display_resolution: true,
        },
        true,
    );
    assert_ne!(on, off);
    assert_eq!(on.size, off.size);
    assert_eq!(on.pixel_scale, off.pixel_scale);
}

fn quality(anti_aliasing: bool, full_display_resolution: bool) -> RenderQuality {
    RenderQuality {
        anti_aliasing,
        full_display_resolution,
    }
}

#[test]
fn live_target_at_ratio_2_under_all_four_quality_combinations() {
    let cases = [
        // (anti_aliasing, full_display_resolution) → (size, scale, samples)
        ((true, true), ((1280, 960), 2.0, LIVE_SAMPLE_COUNT)),
        ((true, false), ((640, 480), 1.0, LIVE_SAMPLE_COUNT)),
        ((false, true), ((1280, 960), 2.0, 1)),
        ((false, false), ((640, 480), 1.0, 1)),
    ];
    for ((aa, full), ((width, height), scale, samples)) in cases {
        let spec = live_target_spec(640.0, 480.0, 2.0, quality(aa, full), true);
        assert_eq!(
            (spec.size.width, spec.size.height),
            (width, height),
            "aa {aa}, full {full}"
        );
        assert_eq!(spec.pixel_scale, scale, "aa {aa}, full {full}");
        assert_eq!(spec.sample_count, samples, "aa {aa}, full {full}");
    }
}

#[test]
fn full_display_resolution_off_is_the_logical_size_at_any_ratio() {
    for ratio in [1.0, 1.25, 1.5, 2.0] {
        let spec = live_target_spec(801.0, 601.0, ratio, quality(true, false), true);
        assert_eq!((spec.size.width, spec.size.height), (801, 601));
        assert_eq!(spec.pixel_scale, 1.0);
    }
}

#[test]
fn a_nonsensical_ratio_falls_back_to_1() {
    for ratio in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        let spec = live_target_spec(640.0, 480.0, ratio, RenderQuality::default(), true);
        assert_eq!((spec.size.width, spec.size.height), (640, 480), "{ratio}");
        assert_eq!(spec.pixel_scale, 1.0, "{ratio}");
    }
}

#[test]
fn a_viewport_request_derives_the_live_target_spec() {
    let request = ViewportRequest {
        logical_width: 640.0,
        logical_height: 480.0,
        device_pixel_ratio: 1.5,
    };
    assert_eq!(
        request.target_spec(RenderQuality::default(), true),
        live_target_spec(640.0, 480.0, 1.5, RenderQuality::default(), true)
    );
}
