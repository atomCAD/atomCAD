//! Tests for `GeoNode::point_invert` — the improper isometry `p ↦ 2·center − p`
//! backing the `structure_invert` node.
//!
//! The SDF side is exact (inversion is an isometry, so the child's distance
//! values carry over unchanged). The CSG side is the risky part: csgrs's
//! `Mesh::transform` does not reverse polygon winding under a det = −1 matrix,
//! so `point_invert_to_csg` flips each polygon afterwards — these tests pin
//! that the resulting mesh is *not* inside-out (positive signed volume, plane
//! normals agreeing with winding and vertex normals).

use glam::f64::DVec3;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::geo_tree::implicit_geometry::ImplicitGeometry3D;

const EPS: f64 = 1e-9;

#[test]
fn inverted_sphere_sdf_equals_mirrored_sphere() {
    // Inverting a sphere centered at s through c yields the sphere centered at
    // 2c − s (same radius). Compare SDFs at scattered sample points.
    let s = DVec3::new(3.0, -1.0, 2.0);
    let c = DVec3::new(1.0, 1.0, 1.0);
    let r = 2.5;

    let inverted = GeoNode::point_invert(c, Box::new(GeoNode::sphere(s, r)));
    let expected = GeoNode::sphere(2.0 * c - s, r);

    let samples = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(-1.0, 3.0, 0.5),
        DVec3::new(-1.0, 3.0, 0.0), // the mirrored center: deep inside
        DVec3::new(10.0, -5.0, 7.0),
        DVec3::new(1.5, 2.5, -0.5),
    ];
    for p in &samples {
        assert!(
            (inverted.implicit_eval_3d(p) - expected.implicit_eval_3d(p)).abs() < EPS,
            "SDF mismatch at {:?}",
            p
        );
    }
}

#[test]
fn inverted_half_space_sdf_equals_mirrored_half_space() {
    // A half space is asymmetric under inversion: normal flips, anchor mirrors.
    let n = DVec3::new(0.0, 0.0, 1.0);
    let q = DVec3::new(0.0, 0.0, 2.0);
    let c = DVec3::new(0.0, 0.0, 0.0);

    let inverted = GeoNode::point_invert(c, Box::new(GeoNode::half_space(n, q)));
    let expected = GeoNode::half_space(-n, 2.0 * c - q);

    let samples = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 2.0, -5.0),
        DVec3::new(-3.0, 0.5, 4.0),
    ];
    for p in &samples {
        assert!(
            (inverted.implicit_eval_3d(p) - expected.implicit_eval_3d(p)).abs() < EPS,
            "SDF mismatch at {:?}",
            p
        );
    }
}

#[test]
fn double_inversion_is_identity() {
    let shape = GeoNode::difference_3d(
        Box::new(GeoNode::sphere(DVec3::new(1.0, 0.0, 0.0), 3.0)),
        Box::new(GeoNode::sphere(DVec3::new(2.5, 0.5, 0.0), 1.5)),
    );
    let c = DVec3::new(0.7, -1.2, 0.4);
    let twice = GeoNode::point_invert(
        c,
        Box::new(GeoNode::point_invert(c, Box::new(shape.clone()))),
    );

    let samples = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 0.0, 2.9),
        DVec3::new(2.5, 0.5, 0.1),
        DVec3::new(-4.0, 2.0, 1.0),
    ];
    for p in &samples {
        assert!(
            (twice.implicit_eval_3d(p) - shape.implicit_eval_3d(p)).abs() < EPS,
            "SDF mismatch at {:?}",
            p
        );
    }
}

#[test]
fn batch_eval_matches_single_eval() {
    use rust_lib_flutter_cad::geo_tree::implicit_geometry::BATCH_SIZE;

    let shape = GeoNode::point_invert(
        DVec3::new(1.0, 2.0, 3.0),
        Box::new(GeoNode::sphere(DVec3::new(-1.0, 0.5, 2.0), 2.0)),
    );

    let mut points = [DVec3::ZERO; BATCH_SIZE];
    for (i, p) in points.iter_mut().enumerate() {
        let t = i as f64 * 0.1;
        *p = DVec3::new(t.sin() * 5.0, t.cos() * 5.0, t * 0.05 - 2.0);
    }
    let mut results = [0.0; BATCH_SIZE];
    shape.implicit_eval_3d_batch(&points, &mut results);

    for i in 0..BATCH_SIZE {
        assert!(
            (results[i] - shape.implicit_eval_3d(&points[i])).abs() < EPS,
            "batch/single mismatch at index {}",
            i
        );
    }
}

#[test]
fn hash_depends_on_center_and_child() {
    let child = || Box::new(GeoNode::sphere(DVec3::new(1.0, 0.0, 0.0), 2.0));
    let a = GeoNode::point_invert(DVec3::ZERO, child());
    let b = GeoNode::point_invert(DVec3::ZERO, child());
    let c = GeoNode::point_invert(DVec3::new(0.5, 0.0, 0.0), child());
    let d = GeoNode::point_invert(
        DVec3::ZERO,
        Box::new(GeoNode::sphere(DVec3::new(1.0, 0.0, 0.0), 3.0)),
    );

    assert_eq!(a.hash(), b.hash());
    assert_ne!(a.hash(), c.hash());
    assert_ne!(a.hash(), d.hash());
    assert_ne!(a.hash(), child().hash());
}

/// Signed volume of a CSG mesh via the divergence theorem over fan-triangulated
/// polygons. Positive iff the winding is consistently outward.
fn signed_volume(mesh: &rust_lib_flutter_cad::geo_tree::csg_types::CSGMesh) -> f64 {
    let mut vol = 0.0;
    for poly in &mesh.polygons {
        for tri in poly.triangulate() {
            let v0 = DVec3::new(tri[0].pos.x, tri[0].pos.y, tri[0].pos.z);
            let v1 = DVec3::new(tri[1].pos.x, tri[1].pos.y, tri[1].pos.z);
            let v2 = DVec3::new(tri[2].pos.x, tri[2].pos.y, tri[2].pos.z);
            vol += v0.dot(v1.cross(v2)) / 6.0;
        }
    }
    vol
}

#[test]
fn inverted_csg_mesh_is_not_inside_out() {
    // An asymmetric solid (sphere minus off-center bite). If the winding fix in
    // point_invert_to_csg were missing, the signed volume would come out
    // negative and the plane normals would disagree with the winding.
    let shape = GeoNode::difference_3d(
        Box::new(GeoNode::sphere(DVec3::new(1.0, 0.0, 0.0), 3.0)),
        Box::new(GeoNode::sphere(DVec3::new(3.0, 1.0, 0.5), 1.5)),
    );
    let original = shape.to_csg_mesh().expect("original mesh");
    let inverted = GeoNode::point_invert(DVec3::new(0.5, -0.5, 1.0), Box::new(shape))
        .to_csg_mesh()
        .expect("inverted mesh");

    let vol_original = signed_volume(&original);
    let vol_inverted = signed_volume(&inverted);

    assert!(vol_original > 0.0);
    assert!(
        vol_inverted > 0.0,
        "inverted mesh is inside-out (signed volume {})",
        vol_inverted
    );
    // Inversion is an isometry: volume is preserved exactly (up to float noise).
    assert!(
        (vol_inverted - vol_original).abs() < 1e-6 * vol_original.abs(),
        "volume changed: {} vs {}",
        vol_original,
        vol_inverted
    );

    // Winding and plane normal must agree per polygon — these are the two
    // things downstream consumers read: the display pipeline derives face
    // orientation from winding (vertex normals are ignored, see
    // `display/csg_to_poly_mesh.rs`) and csgrs BSP booleans classify against
    // `plane`. Per-vertex normals are deliberately NOT asserted: csgrs's own
    // difference output already violates vertex-normal/plane agreement on
    // over half its vertices, so it is not an invariant of the pipeline.
    for poly in &inverted.polygons {
        let plane_normal = poly.plane.normal();
        let plane_n =
            DVec3::new(plane_normal.x, plane_normal.y, plane_normal.z).normalize_or_zero();
        if plane_n.length() < 0.5 {
            continue; // fully degenerate polygon (zero-length plane normal)
        }
        for tri in poly.triangulate() {
            let v0 = DVec3::new(tri[0].pos.x, tri[0].pos.y, tri[0].pos.z);
            let v1 = DVec3::new(tri[1].pos.x, tri[1].pos.y, tri[1].pos.z);
            let v2 = DVec3::new(tri[2].pos.x, tri[2].pos.y, tri[2].pos.z);
            let winding_normal = (v1 - v0).cross(v2 - v0);
            if winding_normal.length() < 1e-12 {
                continue; // degenerate sliver
            }
            // ≥ −ε rather than > 0: BSP splitting leaves the occasional
            // zero-area sliver whose fan-triangulated normal is numerically
            // perpendicular (dot ≈ ±0.0) — present in the original mesh too.
            assert!(
                winding_normal.normalize().dot(plane_n) > -1e-6,
                "winding disagrees with plane normal"
            );
        }
    }
}
