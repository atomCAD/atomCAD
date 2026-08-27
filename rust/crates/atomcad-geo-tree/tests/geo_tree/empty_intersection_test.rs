//! An intersection that collapses to nothing must *stay* nothing.
//!
//! csgrs has no representation for the empty set: a `Node` built from zero
//! polygons has no splitting plane, and `Node::clip_polygons` passes its input
//! through untouched in that case. An empty operand therefore behaves like the
//! universe, which used to make `intersect(half_space, opposite_half_space,
//! sphere)` render as the bare sphere — the two half-spaces cancelled first,
//! and the empty accumulator then swallowed nothing at all.

use atomcad_geo_tree::GeoNode;
use glam::f64::DVec3;

fn polygon_count(node: &GeoNode) -> usize {
    node.to_csg_mesh().expect("mesh conversion").polygons.len()
}

/// Two opposing half-spaces whose solid sides face away from each other.
/// `gap` is the distance between the two planes.
fn disjoint_half_spaces(gap: f64) -> (GeoNode, GeoNode) {
    (
        GeoNode::half_space(DVec3::Z, DVec3::new(0.0, 0.0, -0.5 * gap)),
        GeoNode::half_space(-DVec3::Z, DVec3::new(0.0, 0.0, 0.5 * gap)),
    )
}

#[test]
fn opposing_half_spaces_intersect_to_nothing() {
    for gap in [0.0, 1.0] {
        let (up, down) = disjoint_half_spaces(gap);
        assert_eq!(
            polygon_count(&GeoNode::intersection_3d(vec![up, down])),
            0,
            "opposing half-spaces with gap {gap} should bound no volume"
        );
    }
}

#[test]
fn empty_intersection_is_not_resurrected_by_later_operands() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    for gap in [0.0, 1.0] {
        let (up, down) = disjoint_half_spaces(gap);
        assert_eq!(
            polygon_count(&GeoNode::intersection_3d(vec![up, down, sphere.clone()])),
            0,
            "sphere must not survive an already-empty intersection (gap {gap})"
        );
    }
}

#[test]
fn empty_operand_empties_a_non_empty_accumulator() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    let (up, down) = disjoint_half_spaces(1.0);

    // Same operands, sphere first: the result must not depend on the order.
    assert_eq!(
        polygon_count(&GeoNode::intersection_3d(vec![sphere.clone(), up, down])),
        0
    );
}

#[test]
fn difference_from_an_empty_base_stays_empty() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    let (up, down) = disjoint_half_spaces(1.0);
    let empty = GeoNode::intersection_3d(vec![up, down]);

    assert_eq!(
        polygon_count(&GeoNode::difference_3d(Box::new(empty), Box::new(sphere))),
        0
    );
}

#[test]
fn union_with_an_empty_operand_keeps_the_other() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    let (up, down) = disjoint_half_spaces(1.0);
    let empty = GeoNode::intersection_3d(vec![up, down]);

    let sphere_polys = polygon_count(&sphere);
    assert_eq!(
        polygon_count(&GeoNode::union_3d(vec![empty, sphere])),
        sphere_polys
    );
}
