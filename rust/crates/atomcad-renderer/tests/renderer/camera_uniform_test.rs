// The camera uniform (doc/design_screen_space_lines.md D3, Phase 1). The
// render target's size and pixel scale are in `render_target_test.rs`.

use atomcad_renderer::camera::Camera;
use atomcad_renderer::renderer::CameraUniform;
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
