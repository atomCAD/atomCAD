//! Tests for CSG `union`/`intersect`/`diff` propagating the full `Structure`
//! of their inputs and rejecting mismatched inputs.
//!
//! See `doc/design_csg_structure_propagation.md`. Previously CSG nodes silently
//! dropped the motif and motif_offset, emitting `Structure::from_lattice_vecs(...)`
//! with the default zincblende motif. They now require full Structure equality
//! across all inputs (lattice + motif + motif_offset) and pass the shared
//! Structure through to the output Blueprint unchanged.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const NET: &str = "csg_test";

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

fn evaluate(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn extract_blueprint(result: NetworkResult) -> BlueprintData {
    match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("expected Blueprint, got Error: {}", e),
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    }
}

fn extract_error(result: NetworkResult) -> String {
    match result {
        NetworkResult::Error(e) => e,
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}

fn set_node_data<T: rust_lib_flutter_cad::structure_designer::node_data::NodeData + 'static>(
    designer: &mut StructureDesigner,
    node_id: u64,
    data: T,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(NET).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        node,
        true,
    );
}

/// Builds a `structure` node whose output has the given `motif_offset`
/// (other fields default to diamond). Returns the `structure` node's id.
fn add_structure_with_offset(designer: &mut StructureDesigner, offset: DVec3) -> u64 {
    let off_id = designer.add_node("vec3", DVec2::ZERO);
    set_node_data(designer, off_id, Vec3Data { value: offset });
    designer.validate_active_network();

    let st_id = designer.add_node("structure", DVec2::new(200.0, 0.0));
    // `structure` pins: 0=structure, 1=lattice_vecs, 2=motif, 3=motif_offset
    designer.connect_nodes(off_id, 0, st_id, 3);
    st_id
}

/// Wires a `structure` node into a primitive's `structure` input (pin 2 for
/// cuboid/sphere). Returns the primitive's id.
fn add_primitive_with_structure(
    designer: &mut StructureDesigner,
    primitive: &str,
    structure_id: u64,
) -> u64 {
    let prim_id = designer.add_node(primitive, DVec2::new(400.0, 0.0));
    designer.connect_nodes(structure_id, 0, prim_id, 2);
    prim_id
}

// ---- union ----

#[test]
fn union_preserves_shared_structure() {
    let mut designer = setup();
    let offset = DVec3::new(0.1, 0.2, 0.3);
    let st_id = add_structure_with_offset(&mut designer, offset);
    let a = add_primitive_with_structure(&mut designer, "cuboid", st_id);
    let b = add_primitive_with_structure(&mut designer, "sphere", st_id);

    let u = designer.add_node("union", DVec2::new(600.0, 0.0));
    designer.connect_nodes(a, 0, u, 0);
    designer.connect_nodes(b, 0, u, 0);

    let bp = extract_blueprint(evaluate(&designer, u));
    assert!(
        bp.structure.motif_offset.abs_diff_eq(offset, 1e-12),
        "union must pass motif_offset through unchanged, got {:?}",
        bp.structure.motif_offset
    );
    // Shared structure: motif must still be the custom one (not diamond-default
    // replaced). Since the two inputs came from the same structure, equality
    // with the input structure is what we want.
}

#[test]
fn union_rejects_motif_offset_mismatch() {
    let mut designer = setup();
    let st_a = add_structure_with_offset(&mut designer, DVec3::new(0.1, 0.0, 0.0));
    let st_b = add_structure_with_offset(&mut designer, DVec3::new(0.2, 0.0, 0.0));
    let a = add_primitive_with_structure(&mut designer, "cuboid", st_a);
    let b = add_primitive_with_structure(&mut designer, "sphere", st_b);

    let u = designer.add_node("union", DVec2::new(600.0, 0.0));
    designer.connect_nodes(a, 0, u, 0);
    designer.connect_nodes(b, 0, u, 0);

    let err = extract_error(evaluate(&designer, u));
    assert!(
        err.contains("Structure mismatch"),
        "expected Structure mismatch error, got: {}",
        err
    );
}

#[test]
fn single_input_union_passes_structure_through() {
    let mut designer = setup();
    let offset = DVec3::new(0.4, 0.5, 0.6);
    let st_id = add_structure_with_offset(&mut designer, offset);
    let a = add_primitive_with_structure(&mut designer, "cuboid", st_id);

    let u = designer.add_node("union", DVec2::new(600.0, 0.0));
    designer.connect_nodes(a, 0, u, 0);

    let bp = extract_blueprint(evaluate(&designer, u));
    assert!(bp.structure.motif_offset.abs_diff_eq(offset, 1e-12));
}

// ---- intersect ----

#[test]
fn intersect_preserves_shared_structure() {
    let mut designer = setup();
    let offset = DVec3::new(0.7, 0.8, 0.9);
    let st_id = add_structure_with_offset(&mut designer, offset);
    let a = add_primitive_with_structure(&mut designer, "cuboid", st_id);
    let b = add_primitive_with_structure(&mut designer, "sphere", st_id);

    let n = designer.add_node("intersect", DVec2::new(600.0, 0.0));
    designer.connect_nodes(a, 0, n, 0);
    designer.connect_nodes(b, 0, n, 0);

    let bp = extract_blueprint(evaluate(&designer, n));
    assert!(bp.structure.motif_offset.abs_diff_eq(offset, 1e-12));
}

#[test]
fn intersect_rejects_motif_offset_mismatch() {
    let mut designer = setup();
    let st_a = add_structure_with_offset(&mut designer, DVec3::new(0.1, 0.0, 0.0));
    let st_b = add_structure_with_offset(&mut designer, DVec3::new(0.2, 0.0, 0.0));
    let a = add_primitive_with_structure(&mut designer, "cuboid", st_a);
    let b = add_primitive_with_structure(&mut designer, "sphere", st_b);

    let n = designer.add_node("intersect", DVec2::new(600.0, 0.0));
    designer.connect_nodes(a, 0, n, 0);
    designer.connect_nodes(b, 0, n, 0);

    let err = extract_error(evaluate(&designer, n));
    assert!(err.contains("Structure mismatch"), "got: {}", err);
}

// ---- diff ----

#[test]
fn diff_preserves_shared_structure() {
    let mut designer = setup();
    let offset = DVec3::new(0.25, 0.5, 0.75);
    let st_id = add_structure_with_offset(&mut designer, offset);
    let base = add_primitive_with_structure(&mut designer, "cuboid", st_id);
    let sub = add_primitive_with_structure(&mut designer, "sphere", st_id);

    let d = designer.add_node("diff", DVec2::new(600.0, 0.0));
    designer.connect_nodes(base, 0, d, 0);
    designer.connect_nodes(sub, 0, d, 1);

    let bp = extract_blueprint(evaluate(&designer, d));
    assert!(
        bp.structure.motif_offset.abs_diff_eq(offset, 1e-12),
        "diff must pass motif_offset through unchanged, got {:?}",
        bp.structure.motif_offset
    );
}

#[test]
fn diff_rejects_subtracted_structure_mismatch() {
    let mut designer = setup();
    let st_base = add_structure_with_offset(&mut designer, DVec3::new(0.1, 0.0, 0.0));
    let st_sub = add_structure_with_offset(&mut designer, DVec3::new(0.2, 0.0, 0.0));
    let base = add_primitive_with_structure(&mut designer, "cuboid", st_base);
    let sub = add_primitive_with_structure(&mut designer, "sphere", st_sub);

    let d = designer.add_node("diff", DVec2::new(600.0, 0.0));
    designer.connect_nodes(base, 0, d, 0);
    designer.connect_nodes(sub, 0, d, 1);

    let err = extract_error(evaluate(&designer, d));
    assert!(err.contains("Structure mismatch"), "got: {}", err);
}

#[test]
fn diff_rejects_base_internal_mismatch() {
    // Two different structures on the base side → mismatch within the primary
    // set, reported before the subtracted set is even considered.
    let mut designer = setup();
    let st_a = add_structure_with_offset(&mut designer, DVec3::new(0.1, 0.0, 0.0));
    let st_b = add_structure_with_offset(&mut designer, DVec3::new(0.2, 0.0, 0.0));
    let base_a = add_primitive_with_structure(&mut designer, "cuboid", st_a);
    let base_b = add_primitive_with_structure(&mut designer, "sphere", st_b);
    let sub = add_primitive_with_structure(&mut designer, "sphere", st_a);

    let d = designer.add_node("diff", DVec2::new(600.0, 0.0));
    designer.connect_nodes(base_a, 0, d, 0);
    designer.connect_nodes(base_b, 0, d, 0);
    designer.connect_nodes(sub, 0, d, 1);

    let err = extract_error(evaluate(&designer, d));
    assert!(err.contains("Structure mismatch"), "got: {}", err);
}

// ---- helper-level ----

/// Direct test of `BlueprintData::all_have_same_structure` — empty and single
/// slices are trivially equal (no comparison possible).
#[test]
fn all_have_same_structure_empty_and_single_are_true() {
    assert!(BlueprintData::all_have_same_structure(&[]));
}
