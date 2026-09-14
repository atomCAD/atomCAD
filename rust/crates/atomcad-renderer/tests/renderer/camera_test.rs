//! Tests for the navigation-up-axis camera math (issue #349, Phase 1).
//! See `doc/design_view_up_axis.md`.
//!
//! The Phase 2 axis-resolution helpers (`resolve_miller_plane_up` and friends)
//! are `api`-level and cannot be tested from this crate, which sits below both
//! `api` and `crystolecule`; those tests live in
//! `rust/tests/renderer_api/camera_axis_resolution_test.rs`.

use atomcad_renderer::camera::{Camera, CameraCanonicalView};
use glam::f64::DVec3;

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn vec_approx_eq(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
}

/// A camera looking at the origin with the default `+Z` nav axis.
fn test_camera() -> Camera {
    Camera {
        eye: DVec3::new(0.0, -30.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::new(0.0, 0.32, 0.95).normalize(),
        aspect: 1.0,
        fovy: std::f64::consts::PI * 0.15,
        znear: 1.5,
        zfar: 2400.0,
        orthographic: false,
        ortho_half_height: 10.0,
        pivot_point: DVec3::ZERO,
        nav_up: DVec3::Z,
        nav_up_label: "Z".to_string(),
    }
}

fn forward(cam: &Camera) -> DVec3 {
    (cam.target - cam.eye).normalize()
}

// --- realign_up_to_nav_axis ---------------------------------------------

#[test]
fn realign_generic_case() {
    let mut cam = test_camera();
    cam.eye = DVec3::new(10.0, -20.0, 5.0);
    cam.target = DVec3::ZERO;
    let f = forward(&cam);
    cam.nav_up = DVec3::new(1.0, 1.0, 1.0).normalize();

    cam.realign_up_to_nav_axis();

    // Forward is untouched.
    assert!(vec_approx_eq(forward(&cam), f));
    // Result up is a unit vector perpendicular to forward.
    assert!(approx_eq(cam.up.length(), 1.0));
    assert!(approx_eq(cam.up.dot(f), 0.0));
    // Coplanar with {forward, nav_up}: up · (forward × nav_up) == 0.
    assert!(approx_eq(cam.up.dot(f.cross(cam.nav_up)), 0.0));
    // Positive dot with nav_up (turntable invariant up·axis > 0).
    assert!(cam.up.dot(cam.nav_up) > 0.0);
}

#[test]
fn realign_already_aligned_is_noop() {
    let mut cam = test_camera();
    // Forward along +Y; with nav_up = +Z the aligned up is exactly +Z.
    cam.eye = DVec3::new(0.0, -30.0, 0.0);
    cam.target = DVec3::ZERO;
    cam.nav_up = DVec3::Z;
    cam.up = DVec3::Z;

    cam.realign_up_to_nav_axis();

    assert!(vec_approx_eq(cam.up, DVec3::Z));
}

#[test]
fn realign_underside_case_flips_to_positive_dot() {
    let mut cam = test_camera();
    cam.eye = DVec3::new(0.0, -30.0, 0.0);
    cam.target = DVec3::ZERO;
    cam.nav_up = DVec3::Z;
    // Current up points *against* the nav axis (viewing from the underside).
    cam.up = DVec3::new(0.0, 0.0, -1.0);

    cam.realign_up_to_nav_axis();

    // Result still satisfies up·nav_up > 0 (deliberate sign choice, D3).
    assert!(cam.up.dot(cam.nav_up) > 0.0);
    assert!(vec_approx_eq(cam.up, DVec3::Z));
}

#[test]
fn realign_degenerate_forward_parallel_axis_keeps_up() {
    let mut cam = test_camera();
    // Forward along +Z, nav_up along +Z → projection ≈ 0 → up unchanged.
    cam.eye = DVec3::new(0.0, 0.0, -30.0);
    cam.target = DVec3::ZERO;
    cam.nav_up = DVec3::Z;
    let original_up = DVec3::new(0.0, 1.0, 0.0);
    cam.up = original_up;

    cam.realign_up_to_nav_axis();

    assert!(vec_approx_eq(cam.up, original_up));
}

// --- nav_frame ----------------------------------------------------------

#[test]
fn nav_frame_identity_for_plus_z() {
    let cam = test_camera();
    let frame = cam.nav_frame();
    assert!(vec_approx_eq(frame * DVec3::X, DVec3::X));
    assert!(vec_approx_eq(frame * DVec3::Y, DVec3::Y));
    assert!(vec_approx_eq(frame * DVec3::Z, DVec3::Z));
}

#[test]
fn nav_frame_minus_z_is_180_about_y() {
    let mut cam = test_camera();
    cam.nav_up = DVec3::new(0.0, 0.0, -1.0);
    let frame = cam.nav_frame();
    // Z' == nav_up, and X flips (180° about Y), Y stays.
    assert!(vec_approx_eq(frame * DVec3::Z, DVec3::new(0.0, 0.0, -1.0)));
    assert!(vec_approx_eq(frame * DVec3::X, DVec3::new(-1.0, 0.0, 0.0)));
    assert!(vec_approx_eq(frame * DVec3::Y, DVec3::Y));
}

#[test]
fn nav_frame_plus_y_uses_z_fallback() {
    let mut cam = test_camera();
    cam.nav_up = DVec3::Y;
    let frame = cam.nav_frame();
    // Z' == nav_up.
    assert!(vec_approx_eq(frame * DVec3::Z, DVec3::Y));
    // The Y' fallback projected world +Z, so Y' == world +Z.
    assert!(vec_approx_eq(frame * DVec3::Y, DVec3::Z));
}

#[test]
fn nav_frame_front_has_largest_world_y_of_side_views() {
    let mut cam = test_camera();
    cam.nav_up = DVec3::new(0.0, 0.3, 1.0).normalize();

    let mut side_view_dir = |view: CameraCanonicalView| {
        cam.set_canonical_view(view);
        forward(&cam)
    };

    let front_y = side_view_dir(CameraCanonicalView::Front).y;
    let back_y = side_view_dir(CameraCanonicalView::Back).y;
    let left_y = side_view_dir(CameraCanonicalView::Left).y;
    let right_y = side_view_dir(CameraCanonicalView::Right).y;

    assert!(front_y > back_y);
    assert!(front_y > left_y);
    assert!(front_y > right_y);
}

// --- reset_nav_up -------------------------------------------------------

#[test]
fn reset_nav_up_restores_z_and_realigns() {
    let mut cam = test_camera();
    cam.eye = DVec3::new(0.0, -30.0, 0.0);
    cam.target = DVec3::ZERO;
    cam.nav_up = DVec3::new(1.0, 0.0, 0.0);
    cam.nav_up_label = "X".to_string();
    cam.up = DVec3::new(1.0, 0.0, 0.0);

    cam.reset_nav_up();

    assert!(vec_approx_eq(cam.nav_up, DVec3::Z));
    assert_eq!(cam.nav_up_label, "Z");
    // up re-aligned to +Z projected ⊥ forward (forward = +Y here → +Z).
    assert!(vec_approx_eq(cam.up, DVec3::Z));
}

// --- canonical views under a tilted nav_up ------------------------------

#[test]
fn top_looks_along_negative_nav_up() {
    let mut cam = test_camera();
    cam.nav_up = DVec3::new(0.2, 0.3, 1.0).normalize();
    cam.set_canonical_view(CameraCanonicalView::Top);
    assert!(vec_approx_eq(forward(&cam), -cam.nav_up));
}

#[test]
fn canonical_round_trip_default_axis() {
    let views = [
        CameraCanonicalView::Top,
        CameraCanonicalView::Bottom,
        CameraCanonicalView::Front,
        CameraCanonicalView::Back,
        CameraCanonicalView::Left,
        CameraCanonicalView::Right,
    ];
    let mut cam = test_camera();
    for v in views {
        cam.set_canonical_view(v);
        assert_eq!(cam.get_canonical_view(), v);
    }
}

#[test]
fn canonical_round_trip_tilted_axis() {
    let views = [
        CameraCanonicalView::Top,
        CameraCanonicalView::Bottom,
        CameraCanonicalView::Front,
        CameraCanonicalView::Back,
        CameraCanonicalView::Left,
        CameraCanonicalView::Right,
    ];
    let mut cam = test_camera();
    cam.nav_up = DVec3::new(0.2, 0.3, 1.0).normalize();
    for v in views {
        cam.set_canonical_view(v);
        assert_eq!(cam.get_canonical_view(), v);
    }
}

// ---------------------------------------------------------------------------
// `orthonormalize_up` — the stored-pose invariant the Flutter viewport relies
// on. `build_view_matrix` orthonormalizes internally, so nothing here shows up
// in a render; what it protects is `getCameraTransform` on the Flutter side,
// which builds every pick ray and every world→screen projection from `up` as
// stored. A world axis left in `up` scaled `right` by `cos φ` and sheared `up`
// along the view direction, so hover and clicks hit the wrong atom.
// ---------------------------------------------------------------------------

/// The invariant every reader of a pose is entitled to assume.
fn assert_canonical(cam: &Camera) {
    let forward = (cam.target - cam.eye).normalize();
    assert!(
        approx_eq(cam.up.length(), 1.0),
        "up must be a unit vector, got {}",
        cam.up.length()
    );
    assert!(
        cam.up.dot(forward).abs() < 1e-9,
        "up must be perpendicular to forward, got dot {}",
        cam.up.dot(forward)
    );
}

#[test]
fn orthonormalize_up_projects_a_world_axis_onto_the_view_plane() {
    // The pose `atomcad-cli camera --eye ... --up 0,0,1` writes: looking down
    // at 45°, with world +Z still in `up`.
    let mut cam = test_camera();
    cam.eye = DVec3::new(0.0, -30.0, 30.0);
    cam.target = DVec3::ZERO;
    cam.up = DVec3::Z;

    cam.orthonormalize_up();

    assert_canonical(&cam);
    // Looking down the +Y/-Z diagonal, screen-up tilts back over the shoulder:
    // world +Z projected onto the view plane is (0, 1, 1)/sqrt(2).
    let s = std::f64::consts::FRAC_1_SQRT_2;
    assert!(vec_approx_eq(cam.up, DVec3::new(0.0, s, s)), "{:?}", cam.up);
}

#[test]
fn orthonormalize_up_is_idempotent_and_leaves_a_canonical_pose_alone() {
    let mut cam = test_camera();
    // `test_camera`'s up is the old hand-rounded (0, 0.32, 0.95), 0.2° off
    // perpendicular — start from the exact one, or this measures that rounding
    // instead of idempotence.
    cam.up = DVec3::new(0.0, 1.0, 3.0).normalize();
    let before = cam.up;
    cam.orthonormalize_up();
    assert!(
        vec_approx_eq(cam.up, before),
        "{:?} vs {:?}",
        cam.up,
        before
    );
    cam.orthonormalize_up();
    assert!(vec_approx_eq(cam.up, before));
}

#[test]
fn orthonormalize_up_normalizes_a_scaled_up() {
    let mut cam = test_camera();
    cam.eye = DVec3::new(0.0, -30.0, 0.0);
    cam.target = DVec3::ZERO;
    cam.up = DVec3::Z * 7.5;

    cam.orthonormalize_up();

    assert_canonical(&cam);
    assert!(vec_approx_eq(cam.up, DVec3::Z));
}

#[test]
fn orthonormalize_up_falls_back_to_the_nav_axis_when_up_is_the_view_direction() {
    // Looking along +Y with `up` left pointing the same way: the roll the
    // caller asked for carries no information, so the turntable axis wins.
    let mut cam = test_camera();
    cam.eye = DVec3::new(0.0, -30.0, 0.0);
    cam.target = DVec3::ZERO;
    cam.up = DVec3::Y;

    cam.orthonormalize_up();

    assert_canonical(&cam);
    assert!(vec_approx_eq(cam.up, DVec3::Z), "{:?}", cam.up);
}

#[test]
fn orthonormalize_up_stays_finite_looking_along_the_nav_axis() {
    // Straight down with `up` at +Z and `nav_up` at +Z: every roll is equally
    // valid, but the basis must not collapse onto the view axis or picking
    // dies outright.
    let mut cam = test_camera();
    cam.eye = DVec3::new(0.0, 0.0, 50.0);
    cam.target = DVec3::ZERO;
    cam.up = DVec3::Z;
    cam.nav_up = DVec3::Z;

    cam.orthonormalize_up();

    assert_canonical(&cam);
    assert!(cam.up.is_finite());
}

#[test]
fn orthonormalize_up_leaves_a_degenerate_pose_for_its_owner() {
    // eye == target: there is no view direction, so there is nothing to be
    // perpendicular to and the call must not produce a NaN.
    let mut cam = test_camera();
    cam.eye = DVec3::ZERO;
    cam.target = DVec3::ZERO;
    cam.up = DVec3::Z;

    cam.orthonormalize_up();

    assert!(vec_approx_eq(cam.up, DVec3::Z));
}

#[test]
fn canonical_views_are_already_canonical() {
    // `set_canonical_view` writes its own perpendicular up; this pins that it
    // does, so the invariant does not quietly depend on a later call.
    let mut cam = test_camera();
    for v in [
        CameraCanonicalView::Top,
        CameraCanonicalView::Bottom,
        CameraCanonicalView::Front,
        CameraCanonicalView::Back,
        CameraCanonicalView::Left,
        CameraCanonicalView::Right,
    ] {
        cam.set_canonical_view(v);
        assert_canonical(&cam);
    }
}
