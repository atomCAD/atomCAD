use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::drawing_plane::DrawingPlane;
use rust_lib_flutter_cad::crystolecule::drawing_plane::{
    collinear, compute_plane_axes, derive_miller, gcd, gcd3, in_plane, reduce_to_primitive,
};
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;

#[test]
fn test_compute_plane_axes_001() {
    // (001) plane - normal along Z
    let m = IVec3::new(0, 0, 1);
    let (u, v) = compute_plane_axes(&m).unwrap();

    // Axes should be in XY plane
    assert_eq!(u.z, 0);
    assert_eq!(v.z, 0);

    // Should be perpendicular to normal
    assert_eq!(u.dot(m), 0);
    assert_eq!(v.dot(m), 0);

    // Should be non-zero and non-collinear
    assert_ne!(u, IVec3::ZERO);
    assert_ne!(v, IVec3::ZERO);
    let cross = u.as_dvec3().cross(v.as_dvec3());
    assert!(cross.length() > 0.1);
}

fn expected_in_plane_reference_directions(normal: DVec3) -> (DVec3, DVec3) {
    let x_world = DVec3::new(1.0, 0.0, 0.0);
    let y_world = DVec3::new(0.0, 1.0, 0.0);

    let x_proj = x_world - normal * x_world.dot(normal);
    let y_proj = y_world - normal * y_world.dot(normal);

    let ref_u = if x_proj.length_squared() > 1e-12 {
        x_proj.normalize()
    } else if y_proj.length_squared() > 1e-12 {
        y_proj.normalize()
    } else {
        x_world
    };

    let mut ref_v = if y_proj.length_squared() > 1e-12 {
        y_proj.normalize()
    } else {
        normal.cross(ref_u)
    };

    // Avoid degeneracy: if ref_v is nearly parallel to ref_u, fall back to n×ref_u.
    if ref_v.length_squared() < 1e-12 || ref_v.dot(ref_u).abs() > 0.999 {
        ref_v = normal.cross(ref_u);
    }
    if ref_v.length_squared() < 1e-12 {
        ref_v = y_world;
    }

    let ref_v_ortho = ref_v - ref_u * ref_v.dot(ref_u);
    let mut ref_v_final = if ref_v_ortho.length_squared() > 1e-12 {
        ref_v_ortho.normalize()
    } else {
        ref_v.normalize()
    };

    // Match the drawing-plane convention: (u×v)·n > 0.
    if ref_u.cross(ref_v_final).dot(normal) < 0.0 {
        ref_v_final = -ref_v_final;
    }

    (ref_u, ref_v_final)
}

fn assert_axes_match_expected_direction(m: IVec3) {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::new(unit_cell, m, IVec3::ZERO, 0, 1).unwrap();

    assert_eq!(plane.u_axis.dot(m), 0);
    assert_eq!(plane.v_axis.dot(m), 0);

    let plane_props = plane
        .unit_cell
        .ivec3_miller_index_to_plane_props(&plane.miller_index)
        .unwrap();
    let n = plane_props.normal;

    let u_real = plane.unit_cell.ivec3_lattice_to_real(&plane.u_axis);
    let v_real = plane.unit_cell.ivec3_lattice_to_real(&plane.v_axis);

    let u_dir = u_real.normalize();
    let v_ortho = v_real - u_dir * v_real.dot(u_dir);
    let v_dir = v_ortho.normalize();

    // Right-handedness: (u×v)·n > 0
    assert!(u_dir.cross(v_dir).dot(n) > 0.0);

    let abs_m = m.abs();
    let is_111_family = abs_m.x != 0 && abs_m.x == abs_m.y && abs_m.y == abs_m.z;
    if is_111_family {
        return;
    }

    let (ref_u, ref_v) = expected_in_plane_reference_directions(n);

    // Mirror the production scoring logic: v is compared against the component of ref_v
    // orthogonal to the chosen u direction.
    let v_ref_ortho = ref_v - u_dir * ref_v.dot(u_dir);
    let v_ref_dir = if v_ref_ortho.length_squared() > 1e-12 {
        v_ref_ortho.normalize()
    } else {
        ref_v
    };

    // The chosen axes are discrete (integer lattice vectors), so alignment is approximate.
    // We require a strong positive alignment with the expected reference directions.
    assert!(
        u_dir.dot(ref_u) > 0.70,
        "u axis direction not aligned enough for miller={:?}",
        m
    );
    assert!(
        v_dir.dot(v_ref_dir) > 0.70,
        "v axis direction not aligned enough for miller={:?}",
        m
    );
}

#[test]
fn test_compute_plane_axes_100() {
    // (100) plane - normal along X
    let m = IVec3::new(1, 0, 0);
    let (u, v) = compute_plane_axes(&m).unwrap();

    // Axes should be in YZ plane
    assert_eq!(u.x, 0);
    assert_eq!(v.x, 0);

    // Should be perpendicular to normal
    assert_eq!(u.dot(m), 0);
    assert_eq!(v.dot(m), 0);
}

#[test]
fn test_compute_plane_axes_111() {
    // (111) plane - diagonal
    let m = IVec3::new(1, 1, 1);
    let (u, v) = compute_plane_axes(&m).unwrap();

    // Should be perpendicular to normal
    assert_eq!(u.dot(m), 0);
    assert_eq!(v.dot(m), 0);

    // Should be primitive (GCD = 1)
    assert_eq!(gcd3(u.x.abs(), u.y.abs(), u.z.abs()), 1);
    assert_eq!(gcd3(v.x.abs(), v.y.abs(), v.z.abs()), 1);
}

#[test]
fn test_reduce_to_primitive() {
    assert_eq!(
        reduce_to_primitive(IVec3::new(2, 4, 6)),
        IVec3::new(1, 2, 3)
    );
    assert_eq!(
        reduce_to_primitive(IVec3::new(0, 3, 6)),
        IVec3::new(0, 1, 2)
    );
    assert_eq!(
        reduce_to_primitive(IVec3::new(5, 10, 15)),
        IVec3::new(1, 2, 3)
    );
    assert_eq!(reduce_to_primitive(IVec3::ZERO), IVec3::ZERO);
}

#[test]
fn test_gcd() {
    assert_eq!(gcd(12, 8), 4);
    assert_eq!(gcd(17, 5), 1);
    assert_eq!(gcd(100, 50), 50);
}

fn assert_plane_mapping_consistent(m: IVec3) {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::new(unit_cell, m, IVec3::ZERO, 0, 1).unwrap();

    let plane_props = plane
        .unit_cell
        .ivec3_miller_index_to_plane_props(&plane.miller_index)
        .unwrap();
    let normal = plane_props.normal;

    let p00 = plane.lattice_2d_to_world_3d(&glam::i32::IVec2::new(0, 0));
    let p10 = plane.lattice_2d_to_world_3d(&glam::i32::IVec2::new(1, 0));
    let p01 = plane.lattice_2d_to_world_3d(&glam::i32::IVec2::new(0, 1));

    let dx = p10 - p00;
    let dy = p01 - p00;

    // Displacements should lie in the plane.
    assert!(normal.dot(dx).abs() < 1e-8);
    assert!(normal.dot(dy).abs() < 1e-8);

    // A single lattice step must match the real-space length of the corresponding
    // in-plane lattice axis.
    let u_real_len = plane
        .unit_cell
        .ivec3_lattice_to_real(&plane.u_axis)
        .length();
    let v_real_len = plane
        .unit_cell
        .ivec3_lattice_to_real(&plane.v_axis)
        .length();
    assert!((dx.length() - u_real_len).abs() < 1e-8);
    assert!((dy.length() - v_real_len).abs() < 1e-8);

    // Effective unit cell should be plane-local XY (z=0 for a and b).
    assert!((plane.effective_unit_cell.a.z).abs() < 1e-12);
    assert!((plane.effective_unit_cell.b.z).abs() < 1e-12);
}

fn assert_preferred_basis_angle_cos(m: IVec3, expected_cos: f64, tol: f64) {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::new(unit_cell, m, IVec3::ZERO, 0, 1).unwrap();

    assert_eq!(plane.u_axis.dot(m), 0);
    assert_eq!(plane.v_axis.dot(m), 0);

    let plane_props = plane
        .unit_cell
        .ivec3_miller_index_to_plane_props(&plane.miller_index)
        .unwrap();
    let n = plane_props.normal;

    let u_real = plane.unit_cell.ivec3_lattice_to_real(&plane.u_axis);
    let v_real = plane.unit_cell.ivec3_lattice_to_real(&plane.v_axis);

    let u_dir = u_real.normalize();
    let v_dir = v_real.normalize();

    assert!(u_dir.cross(v_dir).dot(n) > 0.0);

    let cos = u_dir.dot(v_dir);
    assert!(
        (cos - expected_cos).abs() <= tol,
        "unexpected basis angle for miller={:?}: cos={}, expected≈{}",
        m,
        cos,
        expected_cos
    );
}

#[test]
fn test_plane_local_mapping_consistency() {
    // Covers the bug: rect(0,0,1,1) must align with the grid cell for arbitrary Miller indices.
    assert_plane_mapping_consistent(IVec3::new(0, 0, 1));
    assert_plane_mapping_consistent(IVec3::new(0, 1, 1));
    assert_plane_mapping_consistent(IVec3::new(0, 1, 0));
    assert_plane_mapping_consistent(IVec3::new(1, 0, 0));
    assert_plane_mapping_consistent(IVec3::new(1, 1, 0));
    assert_plane_mapping_consistent(IVec3::new(1, 0, 1));
    assert_plane_mapping_consistent(IVec3::new(1, 2, 3));
    assert_plane_mapping_consistent(IVec3::new(2, 1, 0));
}

#[test]
fn test_preferred_plane_axes_expected_directions() {
    let cases = [
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
        IVec3::new(0, 1, 1),
        IVec3::new(0, 1, -1),
        IVec3::new(0, -1, 1),
        IVec3::new(0, -1, -1),
        IVec3::new(0, 1, 0),
        IVec3::new(0, -1, 0),
        IVec3::new(1, 1, 1),
        IVec3::new(1, 1, -1),
        IVec3::new(1, -1, 1),
        IVec3::new(-1, 1, 1),
        IVec3::new(-1, -1, 1),
    ];

    for m in cases {
        assert_axes_match_expected_direction(m);
    }
}

#[test]
fn test_preferred_plane_axes_111_family_prefers_120_degrees() {
    // For cubic lattices, {111} planes have 3-fold symmetry in-plane.
    // Prefer the conventional obtuse (120°) basis rather than the acute (60°) one.
    // cos(120°) = -0.5
    let cases = [
        IVec3::new(1, 1, 1),
        IVec3::new(1, 1, -1),
        IVec3::new(1, -1, 1),
        IVec3::new(-1, 1, 1),
        IVec3::new(-1, -1, 1),
        IVec3::new(-1, 1, -1),
        IVec3::new(1, -1, -1),
        IVec3::new(-1, -1, -1),
    ];

    for m in cases {
        assert_preferred_basis_angle_cos(m, -0.5, 1e-6);
    }
}

#[test]
fn test_preferred_plane_axes_common_planes_prefer_90_degrees() {
    // For common non-{111} planes in a cubic cell, the preferred basis should be close to orthogonal.
    // cos(90°) = 0
    let cases = [
        IVec3::new(0, 0, 1),
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(1, 1, 0),
        IVec3::new(1, 0, 1),
        IVec3::new(0, 1, 1),
    ];

    for m in cases {
        assert_preferred_basis_angle_cos(m, 0.0, 1e-6);
    }
}

// ---------------------------------------------------------------------------
// Explicit in-plane axes (`from_spec` cases A–D)
// See doc/design_drawing_plane_explicit_axes.md
// ---------------------------------------------------------------------------

// --- Geometry helpers ---

#[test]
fn test_in_plane_weiss_zone_law() {
    // (0,0,1) plane: any [u v 0] direction lies in it; anything with a z step does not.
    assert!(in_plane(&IVec3::new(0, 0, 1), &IVec3::new(1, 0, 0)));
    assert!(in_plane(&IVec3::new(0, 0, 1), &IVec3::new(3, -7, 0)));
    assert!(!in_plane(&IVec3::new(0, 0, 1), &IVec3::new(0, 0, 1)));
    // (1,1,1) plane: [1,-1,0] satisfies 1-1+0 = 0.
    assert!(in_plane(&IVec3::new(1, 1, 1), &IVec3::new(1, -1, 0)));
    assert!(!in_plane(&IVec3::new(1, 1, 1), &IVec3::new(1, 1, 0)));
}

#[test]
fn test_collinear_uses_integer_cross() {
    assert!(collinear(&IVec3::new(1, 0, 0), &IVec3::new(2, 0, 0)));
    assert!(collinear(&IVec3::new(1, 2, 3), &IVec3::new(-2, -4, -6)));
    assert!(!collinear(&IVec3::new(1, 0, 0), &IVec3::new(0, 1, 0)));
}

#[test]
fn test_derive_miller_reduces_and_errors() {
    // Primitive case.
    assert_eq!(
        derive_miller(&IVec3::new(1, 0, 0), &IVec3::new(0, 1, 0)).unwrap(),
        IVec3::new(0, 0, 1)
    );
    // Non-primitive cross is reduced to lowest terms.
    assert_eq!(
        derive_miller(&IVec3::new(2, 0, 0), &IVec3::new(0, 2, 0)).unwrap(),
        IVec3::new(0, 0, 1)
    );
    // Sign is preserved (so handedness holds by construction).
    assert_eq!(
        derive_miller(&IVec3::new(0, 1, 0), &IVec3::new(1, 0, 0)).unwrap(),
        IVec3::new(0, 0, -1)
    );
    // Parallel directions are degenerate.
    assert!(derive_miller(&IVec3::new(1, 0, 0), &IVec3::new(2, 0, 0)).is_err());
}

// --- Case A: Miller index only ---

#[test]
fn test_from_spec_case_a_matches_new() {
    // from_spec with only a Miller index must reproduce DrawingPlane::new exactly.
    let unit_cell = UnitCellStruct::cubic_diamond();
    let m = IVec3::new(0, 0, 1);

    let via_new = DrawingPlane::new(unit_cell.clone(), m, IVec3::ZERO, 0, 1).unwrap();
    let via_spec =
        DrawingPlane::from_spec(unit_cell, Some(m), None, None, IVec3::ZERO, 0, 1).unwrap();

    assert_eq!(via_new.miller_index, via_spec.miller_index);
    assert_eq!(via_new.u_axis, via_spec.u_axis);
    assert_eq!(via_new.v_axis, via_spec.v_axis);
    // For (001) cubic the auto basis is the canonical X/Y pair.
    assert_eq!(via_spec.u_axis, IVec3::new(1, 0, 0));
    assert_eq!(via_spec.v_axis, IVec3::new(0, 1, 0));
}

// --- Case B: Miller index + u ---

#[test]
fn test_from_spec_case_b_u_collinear_with_first_auto_axis_picks_other() {
    // u = [1,0,0] is collinear with the first auto axis ([1,0,0]); the second axis
    // must fall back to the other auto axis ([0,1,0]).
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(1, 0, 0)),
        None,
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    assert_eq!(plane.u_axis, IVec3::new(1, 0, 0));
    assert_eq!(plane.v_axis, IVec3::new(0, 1, 0));
    assert_right_handed(&plane);
}

#[test]
fn test_from_spec_case_b_u_non_collinear_reuses_first_auto_axis_and_flips() {
    // u = [0,1,0] is non-collinear with the first auto axis [1,0,0], so the second
    // axis is [1,0,0] — but (u × [1,0,0])·n < 0, so it is flipped to [-1,0,0].
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(0, 1, 0)),
        None,
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    assert_eq!(plane.u_axis, IVec3::new(0, 1, 0));
    assert_eq!(plane.v_axis, IVec3::new(-1, 0, 0));
    assert_right_handed(&plane);
}

#[test]
fn test_from_spec_case_b_u_not_in_plane_errors() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let result = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(0, 0, 1)), // along the normal, not in the plane
        None,
        IVec3::ZERO,
        0,
        1,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Weiss"));
}

// --- Case C: Miller index + u + v ---

#[test]
fn test_from_spec_case_c_axes_honored_verbatim() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(1, 0, 0)),
        Some(IVec3::new(0, 1, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    assert_eq!(plane.u_axis, IVec3::new(1, 0, 0));
    assert_eq!(plane.v_axis, IVec3::new(0, 1, 0));
}

#[test]
fn test_from_spec_case_c_left_handed_pair_accepted_unchanged() {
    // Decision 6: case C performs no handedness flip. A left-handed (u, v) is kept.
    let unit_cell = UnitCellStruct::cubic_diamond();
    let plane = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(0, 1, 0)),
        Some(IVec3::new(1, 0, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    // No flip: v stays exactly as given.
    assert_eq!(plane.u_axis, IVec3::new(0, 1, 0));
    assert_eq!(plane.v_axis, IVec3::new(1, 0, 0));

    // The pair really is left-handed: (u × v) · n < 0.
    let n = plane
        .unit_cell
        .ivec3_miller_index_to_plane_props(&plane.miller_index)
        .unwrap()
        .normal;
    let u_real = plane.unit_cell.ivec3_lattice_to_real(&plane.u_axis);
    let v_real = plane.unit_cell.ivec3_lattice_to_real(&plane.v_axis);
    assert!(u_real.cross(v_real).dot(n) < 0.0);
}

#[test]
fn test_from_spec_case_c_weiss_violation_errors() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let result = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(1, 0, 0)),
        Some(IVec3::new(0, 0, 1)), // v not in plane
        IVec3::ZERO,
        0,
        1,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Weiss"));
}

#[test]
fn test_from_spec_case_c_collinear_axes_error() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let result = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(1, 0, 0)),
        Some(IVec3::new(2, 0, 0)), // collinear with u
        IVec3::ZERO,
        0,
        1,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("collinear"));
}

// --- Case D: u + v, no Miller index ---

#[test]
fn test_from_spec_case_d_derives_and_reduces_miller() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    // Non-primitive axes still derive a primitive Miller index.
    let plane = DrawingPlane::from_spec(
        unit_cell,
        None,
        Some(IVec3::new(2, 0, 0)),
        Some(IVec3::new(0, 2, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    assert_eq!(plane.miller_index, IVec3::new(0, 0, 1));
    // Axes are honored verbatim (magnitudes preserved).
    assert_eq!(plane.u_axis, IVec3::new(2, 0, 0));
    assert_eq!(plane.v_axis, IVec3::new(0, 2, 0));
}

#[test]
fn test_from_spec_case_d_right_handed_without_flip() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    // A pair whose cross product points along -z derives m = (0,0,-1); the basis is
    // right-handed by construction with no flip applied.
    let plane = DrawingPlane::from_spec(
        unit_cell,
        None,
        Some(IVec3::new(0, 1, 0)),
        Some(IVec3::new(1, 0, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    assert_eq!(plane.miller_index, IVec3::new(0, 0, -1));
    assert_eq!(plane.u_axis, IVec3::new(0, 1, 0));
    assert_eq!(plane.v_axis, IVec3::new(1, 0, 0));
    assert_right_handed(&plane);
}

#[test]
fn test_from_spec_case_d_parallel_axes_error() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let result = DrawingPlane::from_spec(
        unit_cell,
        None,
        Some(IVec3::new(1, 0, 0)),
        Some(IVec3::new(2, 0, 0)),
        IVec3::ZERO,
        0,
        1,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("parallel"));
}

// --- Error combinations ---

#[test]
fn test_from_spec_v_only_with_miller_errors() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let result = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        None,
        Some(IVec3::new(0, 1, 0)),
        IVec3::ZERO,
        0,
        1,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("specify `u`"));
}

#[test]
fn test_from_spec_underspecified_combos_error() {
    let unit_cell = UnitCellStruct::cubic_diamond();

    // u only.
    let r1 = DrawingPlane::from_spec(
        unit_cell.clone(),
        None,
        Some(IVec3::new(1, 0, 0)),
        None,
        IVec3::ZERO,
        0,
        1,
    );
    assert!(r1.unwrap_err().contains("under-specified"));

    // v only.
    let r2 = DrawingPlane::from_spec(
        unit_cell.clone(),
        None,
        None,
        Some(IVec3::new(0, 1, 0)),
        IVec3::ZERO,
        0,
        1,
    );
    assert!(r2.unwrap_err().contains("under-specified"));

    // nothing.
    let r3 = DrawingPlane::from_spec(unit_cell, None, None, None, IVec3::ZERO, 0, 1);
    assert!(r3.unwrap_err().contains("unspecified"));
}

// --- Magnitude preservation ---

#[test]
fn test_from_spec_preserves_u_magnitude() {
    let unit_cell = UnitCellStruct::cubic_diamond();

    let plane_1 = DrawingPlane::from_spec(
        unit_cell.clone(),
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(1, 0, 0)),
        Some(IVec3::new(0, 1, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    let plane_2 = DrawingPlane::from_spec(
        unit_cell,
        Some(IVec3::new(0, 0, 1)),
        Some(IVec3::new(2, 0, 0)),
        Some(IVec3::new(0, 1, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    // u = [2,0,0] gives a 2× period along the effective cell's a axis vs u = [1,0,0].
    let len_1 = plane_1.effective_unit_cell.a.length();
    let len_2 = plane_2.effective_unit_cell.a.length();
    assert!((len_2 - 2.0 * len_1).abs() < 1e-9);
}

// --- is_compatible with explicit axes ---

#[test]
fn test_is_compatible_distinguishes_explicit_axes() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    let m = IVec3::new(0, 0, 1);

    // Case A auto axes.
    let plane_auto =
        DrawingPlane::from_spec(unit_cell.clone(), Some(m), None, None, IVec3::ZERO, 0, 1).unwrap();

    // Case C with explicit, rotated axes — same Miller index, different in-plane frame.
    let plane_explicit = DrawingPlane::from_spec(
        unit_cell.clone(),
        Some(m),
        Some(IVec3::new(1, 1, 0)),
        Some(IVec3::new(-1, 1, 0)),
        IVec3::ZERO,
        0,
        1,
    )
    .unwrap();

    assert_eq!(plane_auto.miller_index, plane_explicit.miller_index);
    assert!(
        !plane_auto.is_compatible(&plane_explicit),
        "planes with different in-plane axes must be incompatible"
    );

    // Two case-A planes with the same Miller index remain compatible (regression).
    let plane_auto2 =
        DrawingPlane::from_spec(unit_cell, Some(m), None, None, IVec3::ZERO, 0, 1).unwrap();
    assert!(plane_auto.is_compatible(&plane_auto2));
}

// --- shared assertions ---

fn assert_right_handed(plane: &DrawingPlane) {
    let n = plane
        .unit_cell
        .ivec3_miller_index_to_plane_props(&plane.miller_index)
        .unwrap()
        .normal;
    let u_real = plane.unit_cell.ivec3_lattice_to_real(&plane.u_axis);
    let v_real = plane.unit_cell.ivec3_lattice_to_real(&plane.v_axis);
    assert!(
        u_real.cross(v_real).dot(n) > 0.0,
        "expected right-handed basis for {:?}",
        plane.miller_index
    );
}
