//! The gadget pick context is computed from the viewport's *logical* height,
//! whatever the device pixel ratio (`doc/design_viewport_antialiasing.md` D7):
//! the gizmo's minimum grab radius is authored in logical pixels, so on a
//! 200 % display it must not shrink to half its size. The function takes
//! only the logical height, so the device height cannot enter; that the api
//! passes the logical one (`Renderer::get_viewport_logical_size`) is checked
//! in the GPU harness.

use atomcad_renderer::camera::Camera;
use rust_lib_flutter_cad::api::api_common::gadget_pick_context;

fn camera(orthographic: bool) -> Camera {
    let mut camera = Camera::default_pose(1.6);
    camera.fovy = std::f64::consts::FRAC_PI_2;
    camera.orthographic = orthographic;
    camera.ortho_half_height = 25.0;
    camera
}

#[test]
fn world_per_pixel_is_that_of_the_logical_height() {
    // fovy = 90°: tan(45°) = 1, so a 500 px tall viewport is 2 / 500 world
    // units per pixel at unit distance.
    let context = gadget_pick_context(&camera(false), 500.0);
    assert!((context.perspective_world_per_pixel - 2.0 / 500.0).abs() < 1e-12);
    assert!((context.ortho_world_per_pixel - 50.0 / 500.0).abs() < 1e-12);
    assert!(!context.orthographic);
    assert_eq!(context.eye, camera(false).eye);
}

#[test]
fn a_degenerate_height_is_clamped_to_one_pixel() {
    let context = gadget_pick_context(&camera(false), 0.0);
    assert!(context.perspective_world_per_pixel.is_finite());
    assert!((context.perspective_world_per_pixel - 2.0).abs() < 1e-12);
}
