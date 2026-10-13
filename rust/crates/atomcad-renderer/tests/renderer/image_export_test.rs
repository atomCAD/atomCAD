//! The size rule of *File > Export viewport image*: images up to the limit
//! pass through, larger ones are scaled down with the aspect ratio kept.

use atomcad_renderer::image_export::{
    IMAGE_EXPORT_MAX_SIZE, IMAGE_EXPORT_SUPERSAMPLING, fit_export_size,
};

#[test]
fn sizes_within_the_limit_are_unchanged() {
    assert_eq!(fit_export_size(1280, 720), (1280, 720, false));
    assert_eq!(fit_export_size(2048, 2048), (2048, 2048, false));
    assert_eq!(fit_export_size(1, 2048), (1, 2048, false));
}

#[test]
fn oversized_images_are_scaled_to_the_limit_keeping_aspect() {
    assert_eq!(fit_export_size(2560, 1440), (2048, 1152, true));
    assert_eq!(fit_export_size(1000, 4096), (500, 2048, true));
    assert_eq!(fit_export_size(4096, 4096), (2048, 2048, true));
}

#[test]
fn degenerate_sides_never_reach_zero() {
    assert_eq!(fit_export_size(0, 0), (1, 1, false));
    assert_eq!(fit_export_size(100_000, 1), (2048, 1, true));
}

/// The supersampled render target must fit wgpu's default 8192 texture limit,
/// or the largest export fails on the GPU instead of being refused here.
#[test]
fn supersampled_target_fits_the_default_texture_limit() {
    assert!(IMAGE_EXPORT_MAX_SIZE * IMAGE_EXPORT_SUPERSAMPLING <= 8192);
}
