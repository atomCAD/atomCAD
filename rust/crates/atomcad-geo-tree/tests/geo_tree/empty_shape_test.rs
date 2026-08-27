//! The `Empty2D` / `Empty3D` primitives.
//!
//! Emptiness lives in the tree rather than being discovered numerically by a
//! boolean that happens to cancel — see `empty_intersection_test.rs` for the
//! failure mode that motivated it. What matters is that the sentinel SDF
//! (`f64::MAX`) composes correctly through every operator with no special
//! cases: union identity, intersection annihilator, difference no-op.

use atomcad_geo_tree::GeoNode;
use atomcad_geo_tree::implicit_geometry::{ImplicitGeometry2D, ImplicitGeometry3D};
use glam::f64::{DVec2, DVec3};

const PROBES_3D: [DVec3; 4] = [
    DVec3::ZERO,
    DVec3::new(0.5, 0.0, 0.0),
    DVec3::new(-3.0, 7.5, 2.0),
    DVec3::new(1e6, 1e6, 1e6),
];

const PROBES_2D: [DVec2; 4] = [
    DVec2::ZERO,
    DVec2::new(0.5, 0.0),
    DVec2::new(-3.0, 7.5),
    DVec2::new(1e6, 1e6),
];

fn sphere() -> GeoNode {
    GeoNode::sphere(DVec3::ZERO, 1.0)
}

fn circle() -> GeoNode {
    GeoNode::circle(DVec2::ZERO, 1.0)
}

// ---- identity ----

#[test]
fn empty_reports_its_dimension() {
    assert!(GeoNode::empty_3d().is3d());
    assert!(!GeoNode::empty_3d().is2d());
    assert!(GeoNode::empty_2d().is2d());
    assert!(!GeoNode::empty_2d().is3d());
}

#[test]
fn empty_shapes_are_flagged_and_distinct() {
    assert!(GeoNode::empty_3d().is_empty_shape());
    assert!(GeoNode::empty_2d().is_empty_shape());
    assert!(!sphere().is_empty_shape());
    assert!(!circle().is_empty_shape());

    // Distinct hashes, or the CSG cache would serve a mesh for a sketch.
    assert_ne!(GeoNode::empty_3d().hash(), GeoNode::empty_2d().hash());
}

// ---- SDF ----

#[test]
fn empty_contains_no_point() {
    for p in PROBES_3D {
        assert_eq!(GeoNode::empty_3d().implicit_eval_3d(&p), f64::MAX);
    }
    for p in PROBES_2D {
        assert_eq!(GeoNode::empty_2d().implicit_eval_2d(&p), f64::MAX);
    }
}

#[test]
fn empty_gradient_is_zero_not_nan() {
    // (MAX - MAX)/eps, so finite differences must degrade to a zero vector
    // rather than a NaN that would poison a normal downstream.
    let (gradient, value) = GeoNode::empty_3d().get_gradient(&DVec3::ZERO);
    assert!(gradient.is_finite(), "gradient was {gradient:?}");
    assert_eq!(gradient, DVec3::ZERO);
    assert_eq!(value, f64::MAX);
}

// ---- boolean algebra, 3D ----

#[test]
fn empty_is_the_union_identity_3d() {
    for p in PROBES_3D {
        let expected = sphere().implicit_eval_3d(&p);
        assert_eq!(
            GeoNode::union_3d(vec![sphere(), GeoNode::empty_3d()]).implicit_eval_3d(&p),
            expected
        );
        assert_eq!(
            GeoNode::union_3d(vec![GeoNode::empty_3d(), sphere()]).implicit_eval_3d(&p),
            expected
        );
    }
}

#[test]
fn empty_annihilates_an_intersection_3d() {
    for p in PROBES_3D {
        assert_eq!(
            GeoNode::intersection_3d(vec![sphere(), GeoNode::empty_3d()]).implicit_eval_3d(&p),
            f64::MAX
        );
        assert_eq!(
            GeoNode::intersection_3d(vec![GeoNode::empty_3d(), sphere()]).implicit_eval_3d(&p),
            f64::MAX
        );
    }
}

#[test]
fn subtracting_empty_changes_nothing_3d() {
    for p in PROBES_3D {
        assert_eq!(
            GeoNode::difference_3d(Box::new(sphere()), Box::new(GeoNode::empty_3d()))
                .implicit_eval_3d(&p),
            sphere().implicit_eval_3d(&p)
        );
    }
}

// ---- boolean algebra, 2D ----

#[test]
fn empty_is_the_union_identity_2d() {
    for p in PROBES_2D {
        assert_eq!(
            GeoNode::union_2d(vec![circle(), GeoNode::empty_2d()]).implicit_eval_2d(&p),
            circle().implicit_eval_2d(&p)
        );
    }
}

#[test]
fn empty_annihilates_an_intersection_2d() {
    for p in PROBES_2D {
        assert_eq!(
            GeoNode::intersection_2d(vec![circle(), GeoNode::empty_2d()]).implicit_eval_2d(&p),
            f64::MAX
        );
    }
}

#[test]
fn subtracting_empty_changes_nothing_2d() {
    for p in PROBES_2D {
        assert_eq!(
            GeoNode::difference_2d(Box::new(circle()), Box::new(GeoNode::empty_2d()))
                .implicit_eval_2d(&p),
            circle().implicit_eval_2d(&p)
        );
    }
}

// ---- CSG ----

#[test]
fn empty_converts_to_a_geometry_free_mesh_and_sketch() {
    let mesh = GeoNode::empty_3d().to_csg_mesh().expect("mesh conversion");
    assert_eq!(mesh.polygons.len(), 0);

    let sketch = GeoNode::empty_2d()
        .to_csg_sketch()
        .expect("sketch conversion");
    assert_eq!(sketch.geometry.0.len(), 0);
}

#[test]
fn csg_booleans_agree_with_the_sdf() {
    let sphere_polys = sphere().to_csg_mesh().expect("sphere").polygons.len();
    assert!(sphere_polys > 0);

    let cases = [
        (
            "union first",
            GeoNode::union_3d(vec![GeoNode::empty_3d(), sphere()]),
            sphere_polys,
        ),
        (
            "union last",
            GeoNode::union_3d(vec![sphere(), GeoNode::empty_3d()]),
            sphere_polys,
        ),
        (
            "intersect first",
            GeoNode::intersection_3d(vec![GeoNode::empty_3d(), sphere()]),
            0,
        ),
        (
            "intersect last",
            GeoNode::intersection_3d(vec![sphere(), GeoNode::empty_3d()]),
            0,
        ),
        (
            "difference base",
            GeoNode::difference_3d(Box::new(GeoNode::empty_3d()), Box::new(sphere())),
            0,
        ),
        (
            "difference sub",
            GeoNode::difference_3d(Box::new(sphere()), Box::new(GeoNode::empty_3d())),
            sphere_polys,
        ),
    ];

    for (label, node, expected) in cases {
        assert_eq!(
            node.to_csg_mesh().expect("mesh conversion").polygons.len(),
            expected,
            "{label}"
        );
    }
}

/// `Sketch::intersection` always emits a `MultiPolygon` entry, empty or not, so
/// emptiness in 2D has to be read from the polygons rather than the collection.
#[test]
fn csg_2d_booleans_agree_with_the_sdf() {
    fn polygon_count(node: &GeoNode) -> usize {
        node.to_csg_sketch()
            .expect("sketch conversion")
            .geometry
            .0
            .iter()
            .map(|g| match g {
                geo::Geometry::Polygon(_) => 1,
                geo::Geometry::MultiPolygon(mp) => mp.0.len(),
                _ => 0,
            })
            .sum()
    }

    let circle_polys = polygon_count(&circle());
    assert!(circle_polys > 0);

    assert_eq!(
        polygon_count(&GeoNode::union_2d(vec![GeoNode::empty_2d(), circle()])),
        circle_polys,
        "union identity"
    );
    assert_eq!(
        polygon_count(&GeoNode::intersection_2d(vec![
            GeoNode::empty_2d(),
            circle()
        ])),
        0,
        "intersection annihilator, empty first"
    );
    assert_eq!(
        polygon_count(&GeoNode::intersection_2d(vec![
            circle(),
            GeoNode::empty_2d()
        ])),
        0,
        "intersection annihilator, empty last"
    );
    assert_eq!(
        polygon_count(&GeoNode::difference_2d(
            Box::new(circle()),
            Box::new(GeoNode::empty_2d())
        )),
        circle_polys,
        "difference no-op"
    );
    assert_eq!(
        polygon_count(&GeoNode::difference_2d(
            Box::new(GeoNode::empty_2d()),
            Box::new(circle())
        )),
        0,
        "nothing minus something is nothing"
    );
}

#[test]
fn empty_prints_as_itself() {
    assert_eq!(GeoNode::empty_3d().to_string().trim(), "Empty3D");
    assert_eq!(GeoNode::empty_2d().to_string().trim(), "Empty2D");
}
