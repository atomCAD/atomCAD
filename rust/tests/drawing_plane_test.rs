use rust_lib_flutter_cad::crystolecule::drawing_plane::{
    compute_plane_axes, reduce_to_primitive, gcd, gcd3
};
use rust_lib_flutter_cad::crystolecule::drawing_plane::DrawingPlane;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::DVec3;
use glam::i32::IVec3;

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
    assert_eq!(reduce_to_primitive(IVec3::new(2, 4, 6)), IVec3::new(1, 2, 3));
    assert_eq!(reduce_to_primitive(IVec3::new(0, 3, 6)), IVec3::new(0, 1, 2));
    assert_eq!(reduce_to_primitive(IVec3::new(5, 10, 15)), IVec3::new(1, 2, 3));
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
    let u_real_len = plane.unit_cell.ivec3_lattice_to_real(&plane.u_axis).length();
    let v_real_len = plane.unit_cell.ivec3_lattice_to_real(&plane.v_axis).length();
    assert!((dx.length() - u_real_len).abs() < 1e-8);
    assert!((dy.length() - v_real_len).abs() < 1e-8);

    // Effective unit cell should be plane-local XY (z=0 for a and b).
    assert!((plane.effective_unit_cell.a.z).abs() < 1e-12);
    assert!((plane.effective_unit_cell.b.z).abs() < 1e-12);
}

#[test]
fn test_plane_local_mapping_consistency() {
    // Covers the bug: rect(0,0,1,1) must align with the grid cell for arbitrary Miller indices.
    assert_plane_mapping_consistent(IVec3::new(0, 0, 1));
    assert_plane_mapping_consistent(IVec3::new(0, 1, 1));
    assert_plane_mapping_consistent(IVec3::new(0, 1, 0));
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
