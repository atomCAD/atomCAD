//! Tests for the navigation-up-axis camera math (issue #349, Phases 1 & 2).
//! See `doc/design_view_up_axis.md`.

use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::api::common_api::{
    drawing_plane_up, resolve_lattice_direction_up, resolve_miller_plane_up,
};
use rust_lib_flutter_cad::crystolecule::drawing_plane::DrawingPlane;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::renderer::camera::{Camera, CameraCanonicalView};

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

// --- Phase 2: axis resolution helpers -----------------------------------

/// A hexagonal cell built from explicit basis vectors so the hand-computed
/// expectations below are convention-independent: `a` along +X, `b` at 120°
/// in the XY plane, `c` along +Z (length 2).
fn hexagonal_cell() -> UnitCellStruct {
    UnitCellStruct::new(
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(-0.5, 3.0_f64.sqrt() / 2.0, 0.0),
        DVec3::new(0.0, 0.0, 2.0),
    )
}

#[test]
fn plane_and_direction_differ_on_non_cubic_cell() {
    let cell = hexagonal_cell();
    let idx = IVec3::new(1, 1, 1);

    let plane = resolve_miller_plane_up(&cell, idx).unwrap();
    let dir = resolve_lattice_direction_up(&cell, idx).unwrap();

    // D2: for a non-cubic lattice the (hkl) plane normal and the [hkl] lattice
    // direction are genuinely different vectors.
    assert!((plane - dir).length() > 0.1);

    // Each matches its hand-computed value.
    // Plane normal ∝ (b×c + c×a + a×b)/V = (1, √3, 0.5), normalized.
    assert!(vec_approx_eq(
        plane,
        DVec3::new(
            0.485_071_250_072_665_8,
            0.840_168_050_416_805_9,
            0.242_535_625_036_332_9
        ),
    ));
    // Lattice direction = a + b + c = (0.5, √3/2, 2), normalized.
    assert!(vec_approx_eq(
        dir,
        DVec3::new(
            0.223_606_797_749_979,
            0.387_298_334_620_741_7,
            0.894_427_190_999_915_9
        ),
    ));
}

#[test]
fn plane_and_direction_coincide_on_cubic_cell() {
    // The contrast that makes D2's separation meaningful: on a cubic cell the
    // (111) plane normal and the [111] direction are the same vector.
    let cell = UnitCellStruct::cubic_diamond();
    let idx = IVec3::new(1, 1, 1);

    let plane = resolve_miller_plane_up(&cell, idx).unwrap();
    let dir = resolve_lattice_direction_up(&cell, idx).unwrap();

    assert!(vec_approx_eq(plane, dir));
    assert!(vec_approx_eq(plane, DVec3::new(1.0, 1.0, 1.0).normalize()));
}

#[test]
fn resolve_zero_indices_error() {
    let cell = hexagonal_cell();
    assert!(resolve_miller_plane_up(&cell, IVec3::ZERO).is_err());
    assert!(resolve_lattice_direction_up(&cell, IVec3::ZERO).is_err());
}

#[test]
fn drawing_plane_up_is_reciprocal_normal_and_labelled() {
    let cell = hexagonal_cell();
    let idx = IVec3::new(1, 1, 1);
    let plane = DrawingPlane::new(cell.clone(), idx, IVec3::ZERO, 0, 1).unwrap();

    let (up, label) = drawing_plane_up(&plane);

    // The plane's up is the reciprocal-space normal (matches the plane helper),
    // *not* the [111] lattice direction.
    assert!(vec_approx_eq(
        up,
        resolve_miller_plane_up(&cell, idx).unwrap()
    ));
    assert!((up - resolve_lattice_direction_up(&cell, idx).unwrap()).length() > 0.1);
    assert_eq!(label, "(1 1 1)");
}
