//! Tests for the `get_structure` node (HasStructure -> Structure).

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn evaluate_raw(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

#[test]
fn get_structure_node_type_signature() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("get_structure").unwrap();
    assert_eq!(node_type.parameters.len(), 1);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[0].data_type, DataType::HasStructure);
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(
        node_type.output_pins[0].fixed_type(),
        Some(&DataType::Structure),
    );
}

#[test]
fn get_structure_reads_blueprint_structure() {
    let network_name = "test_get_structure_blueprint";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let get_id = designer.add_node("get_structure", DVec2::new(300.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, get_id, 0);

    let blueprint = match evaluate_raw(&designer, network_name, cuboid_id) {
        NetworkResult::Blueprint(bp) => bp,
        other => panic!(
            "Expected cuboid to produce Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    let result = evaluate_raw(&designer, network_name, get_id);
    let structure = match result {
        NetworkResult::Structure(s) => s,
        NetworkResult::Error(e) => panic!("get_structure returned error: {}", e),
        other => panic!(
            "get_structure expected Structure, got {:?}",
            other.infer_data_type()
        ),
    };

    assert!(
        structure
            .lattice_vecs
            .is_approximately_equal(&blueprint.structure.lattice_vecs),
        "get_structure should preserve the Blueprint's lattice_vecs",
    );
    assert_eq!(
        structure.motif_offset, blueprint.structure.motif_offset,
        "get_structure should preserve motif_offset",
    );
}

#[test]
fn get_structure_reads_crystal_structure() {
    let network_name = "test_get_structure_crystal";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let get_id = designer.add_node("get_structure", DVec2::new(600.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, get_id, 0);

    let crystal = match evaluate_raw(&designer, network_name, mat_id) {
        NetworkResult::Crystal(c) => c,
        NetworkResult::Error(e) => panic!("materialize returned error: {}", e),
        other => panic!(
            "Expected materialize to produce Crystal, got {:?}",
            other.infer_data_type()
        ),
    };

    let result = evaluate_raw(&designer, network_name, get_id);
    let structure = match result {
        NetworkResult::Structure(s) => s,
        NetworkResult::Error(e) => panic!("get_structure returned error: {}", e),
        other => panic!(
            "get_structure expected Structure, got {:?}",
            other.infer_data_type()
        ),
    };

    assert!(
        structure
            .lattice_vecs
            .is_approximately_equal(&crystal.structure.lattice_vecs),
        "get_structure should preserve the Crystal's lattice_vecs",
    );
    assert_eq!(
        structure.motif_offset, crystal.structure.motif_offset,
        "get_structure should preserve motif_offset",
    );
}

/// The static validator must reject non-`HasStructure` sources (e.g. a Motif)
/// on the `input` pin so that no wire can be created in the UI.
#[test]
fn get_structure_validator_rejects_non_has_structure_source() {
    let network_name = "test_get_structure_reject";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let motif_id = designer.add_node("motif", DVec2::ZERO);
    let get_id = designer.add_node("get_structure", DVec2::new(300.0, 0.0));

    assert!(
        !designer.can_connect_nodes(motif_id, 0, get_id, 0),
        "Motif output should not be connectable to get_structure's HasStructure input",
    );
}

/// The static validator must also reject Structure directly (it is not a
/// HasStructure — only Blueprint and Crystal upcast to HasStructure).
#[test]
fn get_structure_validator_rejects_structure_source() {
    let network_name = "test_get_structure_reject_structure";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let structure_id = designer.add_node("structure", DVec2::ZERO);
    let get_id = designer.add_node("get_structure", DVec2::new(300.0, 0.0));

    assert!(
        !designer.can_connect_nodes(structure_id, 0, get_id, 0),
        "Structure output should not be connectable to get_structure's HasStructure input",
    );
}

/// Defense-in-depth: feed a non-`HasStructure` NetworkResult (here, an `int`)
/// into the `input` pin by bypassing the static validator via raw
/// `NodeNetwork::connect_nodes`, and assert eval returns a runtime type error
/// on input 0.
#[test]
fn get_structure_eval_rejects_non_has_structure_input() {
    let network_name = "test_get_structure_eval_reject";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let int_id = designer.add_node("int", DVec2::ZERO);
    let get_id = designer.add_node("get_structure", DVec2::new(300.0, 0.0));

    // Bypass the static validator (which would reject Int -> HasStructure) by
    // calling the underlying NodeNetwork::connect_nodes directly.
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.connect_nodes(int_id, 0, get_id, 0, false);

    let result = evaluate_raw(&designer, network_name, get_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("runtime type error") && msg.contains("0 indexed"),
                "expected runtime type error on input 0, got: {msg}",
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}
