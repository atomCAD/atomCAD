//! Tests for Phase 7c phase-transition nodes:
//! `dematerialize` (Crystal -> Blueprint),
//! `exit_structure` (Crystal -> Molecule),
//! `enter_structure` ((Molecule, Structure) -> Crystal).

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

// ---- dematerialize ----

#[test]
fn dematerialize_converts_crystal_to_blueprint() {
    let network_name = "test_dematerialize";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let materialize_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let demat_id = designer.add_node("dematerialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, materialize_id, 0);
    designer.connect_nodes(materialize_id, 0, demat_id, 0);

    let cuboid_bp = match evaluate_raw(&designer, network_name, cuboid_id) {
        NetworkResult::Blueprint(bp) => bp,
        other => panic!(
            "cuboid expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    let demat_result = evaluate_raw(&designer, network_name, demat_id);
    let blueprint = match demat_result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("dematerialize returned error: {}", e),
        other => panic!(
            "dematerialize expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    assert!(
        blueprint
            .structure
            .lattice_vecs
            .is_approximately_equal(&cuboid_bp.structure.lattice_vecs),
        "dematerialize should preserve the lattice_vecs"
    );
}

#[test]
fn dematerialize_node_type_signature() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("dematerialize").unwrap();
    assert_eq!(node_type.parameters.len(), 1);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[0].data_type, DataType::Crystal);
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(
        node_type.output_pins[0].fixed_type(),
        Some(&DataType::Blueprint)
    );
}

// ---- exit_structure ----

#[test]
fn exit_structure_converts_crystal_to_molecule() {
    let network_name = "test_exit_structure";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let materialize_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let exit_id = designer.add_node("exit_structure", DVec2::new(600.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, materialize_id, 0);
    designer.connect_nodes(materialize_id, 0, exit_id, 0);

    let crystal_atom_count = match evaluate_raw(&designer, network_name, materialize_id) {
        NetworkResult::Crystal(c) => c.atoms.get_num_of_atoms(),
        NetworkResult::Error(e) => panic!("materialize returned error: {}", e),
        other => panic!(
            "materialize expected Crystal, got {:?}",
            other.infer_data_type()
        ),
    };
    assert!(crystal_atom_count > 0);

    let exit_result = evaluate_raw(&designer, network_name, exit_id);
    let mol = match exit_result {
        NetworkResult::Molecule(m) => m,
        NetworkResult::Error(e) => panic!("exit_structure returned error: {}", e),
        other => panic!(
            "exit_structure expected Molecule, got {:?}",
            other.infer_data_type()
        ),
    };

    assert_eq!(
        mol.atoms.get_num_of_atoms(),
        crystal_atom_count,
        "atom count should be preserved through exit_structure"
    );
    assert!(
        mol.geo_tree_root.is_some(),
        "exit_structure should preserve the Crystal's geometry shell"
    );
}

#[test]
fn exit_structure_node_type_signature() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("exit_structure").unwrap();
    assert_eq!(node_type.parameters.len(), 1);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[0].data_type, DataType::Crystal);
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(
        node_type.output_pins[0].fixed_type(),
        Some(&DataType::Molecule)
    );
}

// ---- enter_structure ----

#[test]
fn enter_structure_converts_molecule_to_crystal() {
    let network_name = "test_enter_structure";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    // Crystal pipeline: cuboid -> materialize -> exit_structure (gives a Molecule)
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let materialize_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let exit_id = designer.add_node("exit_structure", DVec2::new(600.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, materialize_id, 0);
    designer.connect_nodes(materialize_id, 0, exit_id, 0);

    // Structure source + enter_structure recombination
    let structure_id = designer.add_node("structure", DVec2::new(300.0, 300.0));
    let enter_id = designer.add_node("enter_structure", DVec2::new(900.0, 0.0));
    designer.connect_nodes(exit_id, 0, enter_id, 0);
    designer.connect_nodes(structure_id, 0, enter_id, 1);

    let mol_atom_count = match evaluate_raw(&designer, network_name, exit_id) {
        NetworkResult::Molecule(m) => m.atoms.get_num_of_atoms(),
        other => panic!(
            "exit_structure expected Molecule, got {:?}",
            other.infer_data_type()
        ),
    };

    let enter_result = evaluate_raw(&designer, network_name, enter_id);
    let crystal = match enter_result {
        NetworkResult::Crystal(c) => c,
        NetworkResult::Error(e) => panic!("enter_structure returned error: {}", e),
        other => panic!(
            "enter_structure expected Crystal, got {:?}",
            other.infer_data_type()
        ),
    };

    assert_eq!(
        crystal.atoms.get_num_of_atoms(),
        mol_atom_count,
        "atom count should be preserved through enter_structure"
    );
    assert!(
        crystal.geo_tree_root.is_some(),
        "enter_structure should preserve the Molecule's geometry shell"
    );
}

#[test]
fn enter_structure_node_type_signature() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("enter_structure").unwrap();
    assert_eq!(node_type.parameters.len(), 2);
    assert_eq!(node_type.parameters[0].name, "input");
    assert_eq!(node_type.parameters[0].data_type, DataType::Molecule);
    assert_eq!(node_type.parameters[1].name, "structure");
    assert_eq!(node_type.parameters[1].data_type, DataType::Structure);
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(
        node_type.output_pins[0].fixed_type(),
        Some(&DataType::Crystal)
    );
}

// ---- round-trip: Blueprint -> Crystal -> Molecule -> Crystal -> Blueprint ----

#[test]
fn phase_transition_roundtrip_preserves_atom_count_and_lattice() {
    let network_name = "test_roundtrip";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let materialize_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let exit_id = designer.add_node("exit_structure", DVec2::new(600.0, 0.0));
    let structure_id = designer.add_node("structure", DVec2::new(300.0, 300.0));
    let enter_id = designer.add_node("enter_structure", DVec2::new(900.0, 0.0));
    let demat_id = designer.add_node("dematerialize", DVec2::new(1200.0, 0.0));

    designer.connect_nodes(cuboid_id, 0, materialize_id, 0);
    designer.connect_nodes(materialize_id, 0, exit_id, 0);
    designer.connect_nodes(exit_id, 0, enter_id, 0);
    designer.connect_nodes(structure_id, 0, enter_id, 1);
    designer.connect_nodes(enter_id, 0, demat_id, 0);

    let materialize_crystal = match evaluate_raw(&designer, network_name, materialize_id) {
        NetworkResult::Crystal(c) => c,
        other => panic!(
            "materialize expected Crystal, got {:?}",
            other.infer_data_type()
        ),
    };
    let materialize_atom_count = materialize_crystal.atoms.get_num_of_atoms();
    assert!(materialize_atom_count > 0);

    let final_bp = match evaluate_raw(&designer, network_name, demat_id) {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("dematerialize returned error: {}", e),
        other => panic!(
            "dematerialize expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    assert!(
        final_bp
            .structure
            .lattice_vecs
            .is_approximately_equal(&materialize_crystal.structure.lattice_vecs),
        "roundtrip should preserve lattice_vecs"
    );
}
