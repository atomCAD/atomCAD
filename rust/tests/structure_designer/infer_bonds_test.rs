use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::infer_bonds::InferBondsData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helper Functions
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_atomic_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    structure: AtomicStructure,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::Molecule(MoleculeData {
            atoms: structure,
            geo_tree_root: None,
        }),
    });
    network.add_node("value", position, 0, value_data)
}

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];

    let result = evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context);

    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        _ => panic!("Expected Atomic result, got unexpected type"),
    }
}

/// Create two carbon atoms at bonding distance (~1.54 A)
fn two_carbons_at_bonding_distance() -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);
    structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure
}

// ============================================================================
// Tests
// ============================================================================

/// Basic inference: two atoms at bonding distance should get a bond
#[test]
fn test_infer_bonds_basic() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let structure = two_carbons_at_bonding_distance();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let infer_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, infer_id);

    assert_eq!(result.atom_ids().count(), 2);
    assert_eq!(result.get_num_of_bonds(), 1, "Should infer one bond");
}

/// Low tolerance produces no bond, high tolerance produces bond
#[test]
fn test_infer_bonds_tolerance_effect() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Two carbons at ~1.54 A. Covalent radius of C is ~0.77 A.
    // Sum = 1.54 A. At tolerance 0.9: max = 1.386 A (no bond).
    // At tolerance 1.15: max = 1.771 A (bond).
    let structure = two_carbons_at_bonding_distance();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);

    // Low tolerance node
    let infer_low_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer_low_id, 0);

    // Set low tolerance via set_node_network_data
    {
        let data = Box::new(InferBondsData {
            additive: false,
            bond_tolerance: 0.9,
        });
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(infer_low_id, data);
    }

    let result_low = evaluate_to_atomic(&designer, network_name, infer_low_id);
    assert_eq!(
        result_low.get_num_of_bonds(),
        0,
        "Low tolerance should produce no bonds"
    );

    // High tolerance node (default 1.15)
    let infer_high_id = designer.add_node("infer_bonds", DVec2::new(200.0, 100.0));
    designer.connect_nodes(value_id, 0, infer_high_id, 0);

    let result_high = evaluate_to_atomic(&designer, network_name, infer_high_id);
    assert_eq!(
        result_high.get_num_of_bonds(),
        1,
        "Default tolerance should produce one bond"
    );
}

/// Non-additive mode is idempotent: applying twice gives same result
#[test]
fn test_infer_bonds_idempotent_non_additive() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Structure with existing bond
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::ZERO);
    let id2 = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure.add_bond(id1, id2, BOND_SINGLE);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);

    // First infer_bonds
    let infer1_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer1_id, 0);

    // Second infer_bonds chained
    let infer2_id = designer.add_node("infer_bonds", DVec2::new(400.0, 0.0));
    designer.connect_nodes(infer1_id, 0, infer2_id, 0);

    let result1 = evaluate_to_atomic(&designer, network_name, infer1_id);
    let result2 = evaluate_to_atomic(&designer, network_name, infer2_id);

    assert_eq!(result1.get_num_of_bonds(), result2.get_num_of_bonds());
    assert_eq!(result1.atom_ids().count(), result2.atom_ids().count());
}

/// Additive mode preserves existing bonds and adds new ones
#[test]
fn test_infer_bonds_additive_mode() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Three atoms: A-B bonded, B-C at bonding distance but not bonded
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(6, DVec3::ZERO);
    let b = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    let _c = structure.add_atom(6, DVec3::new(3.08, 0.0, 0.0));
    structure.add_bond(a, b, BOND_SINGLE);
    assert_eq!(structure.get_num_of_bonds(), 1);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let infer_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer_id, 0);

    // Set additive mode
    {
        let data = Box::new(InferBondsData {
            additive: true,
            bond_tolerance: 1.15,
        });
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(infer_id, data);
    }

    let result = evaluate_to_atomic(&designer, network_name, infer_id);

    // Should have 2 bonds: original A-B + inferred B-C
    assert_eq!(
        result.get_num_of_bonds(),
        2,
        "Additive mode should preserve existing bond and add new one"
    );
}

/// Additive mode does not duplicate existing bonds
#[test]
fn test_infer_bonds_additive_no_duplicate() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Two atoms already bonded at bonding distance
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(6, DVec3::ZERO);
    let b = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure.add_bond(a, b, BOND_SINGLE);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let infer_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer_id, 0);

    // Set additive
    {
        let data = Box::new(InferBondsData {
            additive: true,
            bond_tolerance: 1.15,
        });
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(infer_id, data);
    }

    let result = evaluate_to_atomic(&designer, network_name, infer_id);

    assert_eq!(
        result.get_num_of_bonds(),
        1,
        "Additive mode should not duplicate existing bonds"
    );
}

/// Non-additive mode clears existing bonds before re-inferring
#[test]
fn test_infer_bonds_non_additive_clears_bonds() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Two atoms far apart with an existing bond (manually added)
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(6, DVec3::ZERO);
    let b = structure.add_atom(6, DVec3::new(10.0, 0.0, 0.0));
    structure.add_bond(a, b, BOND_SINGLE);
    assert_eq!(structure.get_num_of_bonds(), 1);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let infer_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, infer_id);

    // Non-additive (default) clears bonds, then infers. Atoms too far apart = no bonds.
    assert_eq!(
        result.get_num_of_bonds(),
        0,
        "Non-additive should clear existing bonds; atoms too far for inference"
    );
}

/// Empty structure: no atoms in, no atoms out, no error
#[test]
fn test_infer_bonds_empty_structure() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let structure = AtomicStructure::new();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let infer_id = designer.add_node("infer_bonds", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, infer_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, infer_id);
    assert_eq!(result.atom_ids().count(), 0);
    assert_eq!(result.get_num_of_bonds(), 0);
}

/// Text properties: defaults produce empty, non-defaults roundtrip
#[test]
fn test_infer_bonds_text_properties_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;

    // Default values: get_text_properties should be empty
    let data = InferBondsData::default();
    let props = data.get_text_properties();
    assert!(
        props.is_empty(),
        "Default values should produce no text properties"
    );

    // Non-default values
    let data = InferBondsData {
        additive: true,
        bond_tolerance: 1.30,
    };
    let props = data.get_text_properties();
    assert_eq!(props.len(), 2);
    assert_eq!(props[0].0, "additive");
    assert_eq!(props[0].1, TextValue::Bool(true));
    assert_eq!(props[1].0, "bond_tolerance");
    if let TextValue::Float(f) = props[1].1 {
        assert!((f - 1.30).abs() < 1e-10);
    } else {
        panic!("Expected Float");
    }

    // Roundtrip: set_text_properties then get
    let mut data2 = InferBondsData::default();
    let mut map = HashMap::new();
    map.insert("additive".to_string(), TextValue::Bool(true));
    map.insert("bond_tolerance".to_string(), TextValue::Float(1.30));
    data2.set_text_properties(&map).unwrap();
    assert!(data2.additive);
    assert!((data2.bond_tolerance - 1.30).abs() < 1e-10);
}

/// Node snapshot test
#[test]
fn test_infer_bonds_node_snapshot() {
    use rust_lib_flutter_cad::structure_designer::nodes::infer_bonds::get_node_type;

    let node_type = get_node_type();
    assert_eq!(node_type.name, "infer_bonds");
    assert_eq!(node_type.parameters.len(), 3);
    assert_eq!(node_type.parameters[0].name, "molecule");
    assert_eq!(node_type.parameters[1].name, "additive");
    assert_eq!(node_type.parameters[2].name, "bond_tolerance");
    assert!(node_type.public);
}
