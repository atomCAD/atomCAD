//! Tests for the navigation-up-axis *resolution helpers* (issue #349, Phase 2).
//! See `doc/design_view_up_axis.md`.
//!
//! These were cut out of `tests/renderer/camera_test.rs` when
//! `atomcad-renderer` was extracted (`doc/design_rust_crate_split.md`, Phase 3):
//! they exercise `api::common_api` against `crystolecule` types and touch no
//! `Camera` at all, so they cannot live in a crate that sits below `api`. The
//! camera math itself moved with the crate.

use atomcad_crystolecule::drawing_plane::DrawingPlane;
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::api::common_api::{
    drawing_plane_up, resolve_lattice_direction_up, resolve_miller_plane_up,
};

fn vec_approx_eq(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
}

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
