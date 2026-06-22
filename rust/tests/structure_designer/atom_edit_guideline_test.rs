//! Phase 1 tests for the atom placement guideline geometry (issue #368).
//!
//! Pure geometry + the `Guideline` value type — no `AtomEditData` interaction.
//! See `doc/atom_edit/design_atom_guidelines.md`.

use glam::f64::DVec3;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    Guideline, GuidelineError,
};

const EPS: f64 = 1e-9;

// =============================================================================
// from_three_atoms — circumcenter + normal
// =============================================================================

#[test]
fn test_three_atoms_equilateral_circumcenter() {
    // Equilateral triangle in the z=0 plane.
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(1.0, 0.0, 0.0);
    let c = DVec3::new(0.5, 3.0_f64.sqrt() / 2.0, 0.0);

    let (origin, dir) = Guideline::from_three_atoms(a, b, c).unwrap();

    // Centroid == circumcenter for an equilateral triangle.
    let centroid = (a + b + c) / 3.0;
    assert!((origin - centroid).length() < 1e-9);

    // Equidistant from all three vertices.
    let ra = (origin - a).length();
    let rb = (origin - b).length();
    let rc = (origin - c).length();
    assert!((ra - rb).abs() < 1e-9);
    assert!((ra - rc).abs() < 1e-9);

    // Direction is unit and perpendicular to both edges.
    assert!((dir.length() - 1.0).abs() < 1e-9);
    assert!(dir.dot(b - a).abs() < 1e-9);
    assert!(dir.dot(c - a).abs() < 1e-9);
}

#[test]
fn test_three_atoms_right_triangle_circumcenter() {
    // Right triangle: hypotenuse from a to c; circumcenter is the hypotenuse midpoint.
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(4.0, 0.0, 0.0);
    let c = DVec3::new(4.0, 3.0, 0.0);

    let (origin, dir) = Guideline::from_three_atoms(a, b, c).unwrap();

    let hyp_mid = (a + c) * 0.5;
    assert!((origin - hyp_mid).length() < 1e-9);

    let ra = (origin - a).length();
    let rb = (origin - b).length();
    let rc = (origin - c).length();
    assert!((ra - rb).abs() < 1e-9);
    assert!((ra - rc).abs() < 1e-9);

    // Normal of a triangle in z=0 is ±z.
    assert!((dir.length() - 1.0).abs() < 1e-9);
    assert!(dir.x.abs() < 1e-9 && dir.y.abs() < 1e-9);
    assert!((dir.z.abs() - 1.0).abs() < 1e-9);
}

#[test]
fn test_three_atoms_normal_sign_follows_selection_order() {
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(1.0, 0.0, 0.0);
    let c = DVec3::new(0.0, 1.0, 0.0);

    let (_, dir_abc) = Guideline::from_three_atoms(a, b, c).unwrap();
    // Swapping two atoms flips the normal (cross product anticommutes).
    let (_, dir_acb) = Guideline::from_three_atoms(a, c, b).unwrap();

    assert!((dir_abc + dir_acb).length() < 1e-9);
}

#[test]
fn test_three_atoms_exact_collinear_rejected() {
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(1.0, 0.0, 0.0);
    let c = DVec3::new(2.0, 0.0, 0.0);
    assert_eq!(
        Guideline::from_three_atoms(a, b, c),
        Err(GuidelineError::Collinear)
    );
}

#[test]
fn test_three_atoms_near_collinear_rejected() {
    // Well-separated points with a tiny bend → finite but huge circumradius.
    // An exact area==0 test would NOT catch this; the circumradius cap does.
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(10.0, 0.0, 0.0);
    let c = DVec3::new(20.0, 1.0e-7, 0.0);
    assert_eq!(
        Guideline::from_three_atoms(a, b, c),
        Err(GuidelineError::Collinear)
    );
}

// =============================================================================
// from_two_atoms — midpoint + direction
// =============================================================================

#[test]
fn test_two_atoms_midpoint_and_direction() {
    let a = DVec3::new(1.0, 2.0, 3.0);
    let b = DVec3::new(1.0, 2.0, 7.0);

    let (origin, dir) = Guideline::from_two_atoms(a, b).unwrap();

    assert!((origin - DVec3::new(1.0, 2.0, 5.0)).length() < 1e-9);
    assert!((dir - DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
}

#[test]
fn test_two_atoms_direction_sign_flips_with_order() {
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(0.0, 0.0, 4.0);

    let (origin_ab, dir_ab) = Guideline::from_two_atoms(a, b).unwrap();
    let (origin_ba, dir_ba) = Guideline::from_two_atoms(b, a).unwrap();

    // Same midpoint, opposite direction.
    assert!((origin_ab - origin_ba).length() < 1e-9);
    assert!((dir_ab + dir_ba).length() < 1e-9);

    // The sign flip is observable as a sign flip in `t`: an off-origin point
    // projects to +d under one order and -d under the other.
    let g_ab = Guideline::new(origin_ab, dir_ab);
    let g_ba = Guideline::new(origin_ba, dir_ba);
    let p = DVec3::new(0.0, 0.0, 3.0); // 1 Å past the midpoint toward b
    let (t_ab, _) = g_ab.decompose(p);
    let (t_ba, _) = g_ba.decompose(p);
    assert!((t_ab + t_ba).abs() < 1e-9);
    assert!(t_ab > 0.0);
}

#[test]
fn test_two_atoms_coincident_rejected() {
    let a = DVec3::new(1.0, 1.0, 1.0);
    let b = DVec3::new(1.0, 1.0, 1.0);
    assert_eq!(
        Guideline::from_two_atoms(a, b),
        Err(GuidelineError::Coincident)
    );
}

#[test]
fn test_two_atoms_near_coincident_rejected() {
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(1.0e-9, 0.0, 0.0);
    assert_eq!(
        Guideline::from_two_atoms(a, b),
        Err(GuidelineError::Coincident)
    );
}

// =============================================================================
// from_one_atom — origin + normalized direction
// =============================================================================

#[test]
fn test_one_atom_origin_and_normalized_direction() {
    let p = DVec3::new(2.0, -1.0, 5.0);
    let dir = DVec3::new(0.0, 0.0, 3.0); // non-unit

    let (origin, unit_dir) = Guideline::from_one_atom(p, dir).unwrap();

    assert!((origin - p).length() < 1e-9);
    assert!((unit_dir.length() - 1.0).abs() < 1e-9);
    assert!((unit_dir - DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
}

#[test]
fn test_one_atom_zero_direction_rejected() {
    let p = DVec3::new(1.0, 2.0, 3.0);
    assert_eq!(
        Guideline::from_one_atom(p, DVec3::ZERO),
        Err(GuidelineError::ZeroDirection)
    );
}

#[test]
fn test_one_atom_near_zero_direction_rejected() {
    let p = DVec3::new(1.0, 2.0, 3.0);
    let tiny = DVec3::new(1.0e-9, 0.0, 0.0);
    assert_eq!(
        Guideline::from_one_atom(p, tiny),
        Err(GuidelineError::ZeroDirection)
    );
}

// =============================================================================
// decompose / point_at round-trip
// =============================================================================

#[test]
fn test_point_at_decompose_roundtrip_on_line() {
    let g = Guideline::new(
        DVec3::new(1.0, 2.0, 3.0),
        DVec3::new(1.0, 1.0, 0.0).normalize(),
    );

    for &t in &[-3.5, 0.0, 2.0, 10.25] {
        let p = g.point_at(t);
        let (rec_t, offset) = g.decompose(p);
        assert!((rec_t - t).abs() < EPS, "t round-trip failed for {t}");
        assert!(offset.length() < EPS, "on-line offset should be ~0");
    }
}

#[test]
fn test_decompose_offline_point() {
    // Line is the x-axis through origin.
    let g = Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));

    // Point 2 along, 5 off (in +y).
    let point = DVec3::new(2.0, 5.0, 0.0);
    let (t, offset) = g.decompose(point);

    assert!((t - 2.0).abs() < EPS);
    // Offset is perpendicular, length == distance from the line.
    assert!((offset - DVec3::new(0.0, 5.0, 0.0)).length() < EPS);
    assert!((offset.length() - 5.0).abs() < EPS);
    assert!(offset.dot(g.direction).abs() < EPS);

    // point == point_at(t) + offset.
    assert!((g.point_at(t) + offset - point).length() < EPS);
}

// =============================================================================
// closest_t_to_ray
// =============================================================================

#[test]
fn test_closest_t_to_ray_crossing() {
    // Guideline: x-axis through origin. Ray comes down the +z toward (3,0,0).
    let g = Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));
    let ray_origin = DVec3::new(3.0, 0.0, 10.0);
    let ray_dir = DVec3::new(0.0, 0.0, -1.0);

    let t = g.closest_t_to_ray(ray_origin, ray_dir).unwrap();
    assert!((t - 3.0).abs() < EPS);
    // The foot is the point on the guideline closest to the ray.
    assert!((g.point_at(t) - DVec3::new(3.0, 0.0, 0.0)).length() < EPS);
}

#[test]
fn test_closest_t_to_ray_skew() {
    // Guideline along x; ray along y offset in +z at x=1.5. Closest point on the
    // guideline is its foot of the common perpendicular: x = 1.5.
    let g = Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));
    let ray_origin = DVec3::new(1.5, -10.0, 2.0);
    let ray_dir = DVec3::new(0.0, 1.0, 0.0);

    let t = g.closest_t_to_ray(ray_origin, ray_dir).unwrap();
    assert!((t - 1.5).abs() < EPS);
}

#[test]
fn test_closest_t_to_ray_parallel_returns_none() {
    let g = Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));
    // Ray parallel to the guideline (offset in +y).
    let ray_origin = DVec3::new(0.0, 4.0, 0.0);
    let ray_dir = DVec3::new(2.0, 0.0, 0.0); // non-unit, still parallel
    assert_eq!(g.closest_t_to_ray(ray_origin, ray_dir), None);
}

#[test]
fn test_closest_t_to_ray_non_unit_ray_dir() {
    // closest_t must be robust to a non-normalized ray direction.
    let g = Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));
    let ray_origin = DVec3::new(3.0, 0.0, 10.0);
    let ray_dir = DVec3::new(0.0, 0.0, -7.0); // non-unit

    let t = g.closest_t_to_ray(ray_origin, ray_dir).unwrap();
    assert!((t - 3.0).abs() < EPS);
}
