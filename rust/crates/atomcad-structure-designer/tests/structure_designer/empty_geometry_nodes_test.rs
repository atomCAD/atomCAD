//! The `empty` / `empty_2d` nodes — geometry that bounds nothing.
//!
//! These replace the "empty volume hack" (intersecting two opposing
//! `half_space`s and relying on the result cancelling). The geo-tree-level
//! algebra is covered in `atomcad-geo-tree`'s `empty_shape_test.rs`; what
//! matters here is the node wiring: that the nodes emit the right result kind,
//! that they cooperate with the boolean nodes' structure / drawing-plane
//! compatibility checks, and that `materialize` over an emptied region produces
//! no atoms.

use atomcad_geo_tree::implicit_geometry::ImplicitGeometry2D;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, GeometrySummary2D, NetworkResult,
};
use atomcad_structure_designer::nodes::free_sphere::FreeSphereData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("t");
    designer.set_active_node_network_name(Some("t".to_string()));
    designer
}

fn evaluate(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("t").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn blueprint(result: NetworkResult) -> BlueprintData {
    match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("expected Blueprint, got Error: {e}"),
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    }
}

fn geometry_2d(result: NetworkResult) -> GeometrySummary2D {
    match result {
        NetworkResult::Geometry2D(g) => g,
        NetworkResult::Error(e) => panic!("expected Geometry2D, got Error: {e}"),
        other => panic!("expected Geometry2D, got {:?}", other.infer_data_type()),
    }
}

fn crystal(result: NetworkResult) -> CrystalData {
    match result {
        NetworkResult::Crystal(c) => c,
        NetworkResult::Error(e) => panic!("expected Crystal, got Error: {e}"),
        other => panic!("expected Crystal, got {:?}", other.infer_data_type()),
    }
}

fn add_sphere(designer: &mut StructureDesigner, radius: f64) -> u64 {
    let id = designer.add_node("free_sphere", DVec2::ZERO);
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("t")
        .unwrap();
    network.set_node_network_data(
        id,
        Box::new(FreeSphereData {
            center: DVec3::ZERO,
            radius,
        }),
    );
    id
}

// ============================================================================
// empty (3D)
// ============================================================================

#[test]
fn empty_evaluates_to_an_empty_blueprint() {
    let mut designer = setup();
    let id = designer.add_node("empty", DVec2::ZERO);

    let bp = blueprint(evaluate(&designer, id));
    assert!(bp.geo_tree_root.is_empty_shape());
    // An empty region imposes no orientation, so it must not taint alignment.
    assert_eq!(bp.alignment, Alignment::Aligned);
    assert_eq!(bp.alignment_reason, None);
}

#[test]
fn intersecting_with_empty_yields_nothing() {
    let mut designer = setup();
    let sphere = add_sphere(&mut designer, 5.0);
    let empty = designer.add_node("empty", DVec2::new(0.0, 200.0));
    let isect = designer.add_node("intersect", DVec2::new(300.0, 0.0));
    designer.connect_nodes(sphere, 0, isect, 0);
    designer.connect_nodes(empty, 0, isect, 0);

    let mat = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(isect, 0, mat, 0);

    assert_eq!(
        crystal(evaluate(&designer, mat)).atoms.get_num_of_atoms(),
        0
    );
}

/// The wire order must not matter — the pre-`Empty` hack broke precisely
/// because an emptied CSG accumulator resurrected whatever came after it.
#[test]
fn intersecting_with_empty_yields_nothing_whichever_side_it_is_on() {
    let mut designer = setup();
    let empty = designer.add_node("empty", DVec2::ZERO);
    let sphere = add_sphere(&mut designer, 5.0);
    let isect = designer.add_node("intersect", DVec2::new(300.0, 0.0));
    designer.connect_nodes(empty, 0, isect, 0);
    designer.connect_nodes(sphere, 0, isect, 0);

    let mat = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(isect, 0, mat, 0);

    assert_eq!(
        crystal(evaluate(&designer, mat)).atoms.get_num_of_atoms(),
        0
    );
}

#[test]
fn union_with_empty_keeps_the_other_shape() {
    let mut designer = setup();
    let sphere = add_sphere(&mut designer, 5.0);
    let empty = designer.add_node("empty", DVec2::new(0.0, 200.0));
    let union = designer.add_node("union", DVec2::new(300.0, 0.0));
    designer.connect_nodes(sphere, 0, union, 0);
    designer.connect_nodes(empty, 0, union, 0);

    let mat = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(union, 0, mat, 0);
    let with_empty = crystal(evaluate(&designer, mat)).atoms.get_num_of_atoms();

    // Baseline: the same sphere materialized on its own.
    let mut plain = setup();
    let sphere2 = add_sphere(&mut plain, 5.0);
    let mat2 = plain.add_node("materialize", DVec2::new(300.0, 0.0));
    plain.connect_nodes(sphere2, 0, mat2, 0);
    let alone = crystal(evaluate(&plain, mat2)).atoms.get_num_of_atoms();

    assert!(alone > 0, "the baseline sphere should carve atoms");
    assert_eq!(with_empty, alone);
}

#[test]
fn subtracting_empty_keeps_the_base() {
    let mut designer = setup();
    let sphere = add_sphere(&mut designer, 5.0);
    let empty = designer.add_node("empty", DVec2::new(0.0, 200.0));
    let diff = designer.add_node("diff", DVec2::new(300.0, 0.0));
    designer.connect_nodes(sphere, 0, diff, 0);
    designer.connect_nodes(empty, 0, diff, 1);

    let mat = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(diff, 0, mat, 0);

    assert!(crystal(evaluate(&designer, mat)).atoms.get_num_of_atoms() > 0);
}

/// `empty` carries the same optional `structure` pin as every other Blueprint
/// primitive, so on its default it is diamond-compatible and combines with
/// diamond shapes without tripping the structure check.
#[test]
fn empty_defaults_to_a_diamond_compatible_structure() {
    let mut designer = setup();
    let sphere = add_sphere(&mut designer, 5.0);
    let empty = designer.add_node("empty", DVec2::new(0.0, 200.0));
    let union = designer.add_node("union", DVec2::new(300.0, 0.0));
    designer.connect_nodes(sphere, 0, union, 0);
    designer.connect_nodes(empty, 0, union, 0);

    // A structure mismatch would surface as an Error rather than a Blueprint.
    blueprint(evaluate(&designer, union));
}

// ============================================================================
// empty_2d
// ============================================================================

#[test]
fn empty_2d_evaluates_to_an_empty_geometry() {
    let mut designer = setup();
    let id = designer.add_node("empty_2d", DVec2::ZERO);

    let geo = geometry_2d(evaluate(&designer, id));
    assert!(geo.geo_tree_root.is_empty_shape());
    assert_eq!(geo.frame_transform.translation, DVec2::ZERO);
}

#[test]
fn intersecting_2d_with_empty_yields_nothing() {
    let mut designer = setup();
    let circle = designer.add_node("circle", DVec2::ZERO);
    let empty = designer.add_node("empty_2d", DVec2::new(0.0, 200.0));
    let isect = designer.add_node("intersect_2d", DVec2::new(300.0, 0.0));
    designer.connect_nodes(circle, 0, isect, 0);
    designer.connect_nodes(empty, 0, isect, 0);

    // Compatible drawing planes (both defaulted), so this must be a geometry,
    // not a mismatch error — and its shape must contain no point. Asserted on
    // the SDF rather than the sketch: `Sketch::intersection` always emits a
    // `MultiPolygon` entry, empty or not, so a collection length proves nothing.
    let geo = geometry_2d(evaluate(&designer, isect));
    for p in [DVec2::ZERO, DVec2::new(0.5, 0.5), DVec2::new(-1.0, 0.25)] {
        assert_eq!(
            geo.geo_tree_root.implicit_eval_2d(&p),
            f64::MAX,
            "point {p:?} should be outside the emptied intersection"
        );
    }
}

/// `empty_2d → extrude → materialize` carves no atoms.
#[test]
fn extruding_empty_2d_yields_no_atoms() {
    let mut designer = setup();
    let empty = designer.add_node("empty_2d", DVec2::ZERO);
    let extrude = designer.add_node("extrude", DVec2::new(300.0, 0.0));
    designer.connect_nodes(empty, 0, extrude, 0);
    let mat = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(extrude, 0, mat, 0);

    assert_eq!(
        crystal(evaluate(&designer, mat)).atoms.get_num_of_atoms(),
        0
    );
}
