// The camera uniform and the render target's size and pixel scale
// (doc/design_screen_space_lines.md D2, D3, Phase 1).

use atomcad_renderer::camera::Camera;
use atomcad_renderer::image_export::{
    IMAGE_EXPORT_MAX_SIZE, IMAGE_EXPORT_SUPERSAMPLING, fit_export_size,
};
use atomcad_renderer::renderer::{CameraUniform, RenderTarget};
use atomcad_renderer::thumbnail::{THUMBNAIL_RENDER_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING};
use std::mem::offset_of;

// ---- CameraUniform ----------------------------------------------------------

#[test]
fn camera_uniform_is_256_bytes() {
    assert_eq!(std::mem::size_of::<CameraUniform>(), 256);
}

#[test]
fn camera_uniform_new_fields_sit_at_their_wgsl_offsets() {
    // The fields before them are unchanged by the extension.
    assert_eq!(offset_of!(CameraUniform, camera_position), 192);
    assert_eq!(offset_of!(CameraUniform, head_light_dir), 208);
    assert_eq!(offset_of!(CameraUniform, is_orthographic), 220);
    assert_eq!(offset_of!(CameraUniform, ortho_half_height), 224);
    // vec2<f32> aligns to 8, so 228 is padding.
    assert_eq!(offset_of!(CameraUniform, viewport_size_px), 232);
    assert_eq!(offset_of!(CameraUniform, pixel_scale), 240);
}

#[test]
fn camera_uniform_new_carries_the_target_size_and_scale() {
    let camera = Camera::default_pose(640.0 / 480.0);
    let uniform = CameraUniform::new(&camera, (1280, 960), 2.0);
    assert_eq!(uniform.viewport_size_px, [1280.0, 960.0]);
    assert_eq!(uniform.pixel_scale, 2.0);

    let uniform = CameraUniform::new(&camera, (640, 480), 1.0);
    assert_eq!(uniform.viewport_size_px, [640.0, 480.0]);
    assert_eq!(uniform.pixel_scale, 1.0);
}

#[test]
fn camera_uniform_new_fills_the_camera_fields_from_the_camera() {
    let mut camera = Camera::default_pose(1.5);
    camera.orthographic = true;
    camera.ortho_half_height = 7.5;
    let uniform = CameraUniform::new(&camera, (300, 200), 1.0);

    let view = camera.build_view_matrix().as_mat4();
    let proj = camera.build_projection_matrix().as_mat4();
    assert_eq!(uniform.view_matrix, view.to_cols_array_2d());
    assert_eq!(uniform.proj_matrix, proj.to_cols_array_2d());
    assert_eq!(uniform.view_proj, (proj * view).to_cols_array_2d());
    assert_eq!(uniform.camera_position, camera.eye.as_vec3().to_array());
    assert_eq!(uniform.is_orthographic, 1.0);
    assert_eq!(uniform.ortho_half_height, 7.5);

    camera.orthographic = false;
    assert_eq!(
        CameraUniform::new(&camera, (300, 200), 1.0).is_orthographic,
        0.0
    );
}

#[test]
fn camera_uniform_padding_is_zero() {
    let uniform = CameraUniform::new(&Camera::default_pose(1.0), (10, 10), 1.0);
    assert_eq!(uniform._padding0, 0);
    assert_eq!(uniform._padding1, 0);
    assert_eq!(uniform._padding2, [0, 0, 0]);
}

// ---- RenderTarget::dims -----------------------------------------------------

#[test]
fn live_viewport_target_is_its_own_size_at_scale_1() {
    let (size, scale) = RenderTarget::dims(1024, 768, 1);
    assert_eq!(
        (size.width, size.height, size.depth_or_array_layers),
        (1024, 768, 1)
    );
    assert_eq!(scale, 1.0);
}

#[test]
fn thumbnail_target_is_supersampled_at_its_pixel_scale() {
    let (size, scale) = RenderTarget::dims(THUMBNAIL_SIZE, THUMBNAIL_SIZE, THUMBNAIL_SUPERSAMPLING);
    assert_eq!(
        (size.width, size.height),
        (THUMBNAIL_RENDER_SIZE, THUMBNAIL_RENDER_SIZE)
    );
    assert_eq!(scale, 2.0);
}

#[test]
fn export_target_is_supersampled_at_its_pixel_scale() {
    let (size, scale) = RenderTarget::dims(800, 600, IMAGE_EXPORT_SUPERSAMPLING);
    assert_eq!((size.width, size.height), (1600, 1200));
    assert_eq!(scale, IMAGE_EXPORT_SUPERSAMPLING as f32);
}

#[test]
fn clamped_export_target_keeps_the_scale_and_fits_the_texture_limit() {
    // An oversized request is fitted first (as the api does), then the
    // target is the fitted size × the supersampling, at the same scale.
    let (width, height, reduced) = fit_export_size(6000, 3000);
    assert!(reduced);
    assert_eq!((width, height), (IMAGE_EXPORT_MAX_SIZE, 1024));

    let (size, scale) = RenderTarget::dims(width, height, IMAGE_EXPORT_SUPERSAMPLING);
    assert_eq!((size.width, size.height), (4096, 2048));
    assert_eq!(scale, 2.0);
    // wgpu's default max_texture_dimension_2d.
    assert!(size.width <= 8192 && size.height <= 8192);
}
