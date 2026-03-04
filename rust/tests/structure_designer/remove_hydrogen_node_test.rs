use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

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
        value: NetworkResult::Atomic(structure),
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
        NetworkResult::Atomic(s) => s,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        _ => panic!("Expected Atomic result, got unexpected type"),
    }
}

fn evaluate_to_result(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> NetworkResult {
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

// ============================================================================
// Tests
// ============================================================================

/// Methane (1 C + 4 H) -> remove_hydrogen -> 1 C, 0 H, 0 bonds
#[test]
fn test_remove_hydrogen_node_methane() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Build methane: C at origin + 4 H bonded to it
    let mut structure = AtomicStructure::new();
    let c_id = structure.add_atom(6, DVec3::ZERO);
    let h1_id = structure.add_atom(1, DVec3::new(0.63, 0.63, 0.63));
    let h2_id = structure.add_atom(1, DVec3::new(-0.63, -0.63, 0.63));
    let h3_id = structure.add_atom(1, DVec3::new(-0.63, 0.63, -0.63));
    let h4_id = structure.add_atom(1, DVec3::new(0.63, -0.63, -0.63));
    structure.add_bond(c_id, h1_id, BOND_SINGLE);
    structure.add_bond(c_id, h2_id, BOND_SINGLE);
    structure.add_bond(c_id, h3_id, BOND_SINGLE);
    structure.add_bond(c_id, h4_id, BOND_SINGLE);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let remove_h_id = designer.add_node("remove_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, remove_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, remove_h_id);

    // Should have 1 carbon, 0 hydrogens
    let atom_count = result.atom_ids().count();
    assert_eq!(atom_count, 1, "Expected 1 atom (C only), got {}", atom_count);

    let carbon = result
        .atom_ids()
        .find(|&&id| result.get_atom(id).unwrap().atomic_number == 6)
        .expect("Carbon should remain");
    let carbon_atom = result.get_atom(*carbon).unwrap();
    let bond_count = carbon_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .count();
    assert_eq!(bond_count, 0, "Carbon should have 0 bonds, got {}", bond_count);
}

/// Empty structure -> empty output
#[test]
fn test_remove_hydrogen_node_empty_structure() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let structure = AtomicStructure::new();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let remove_h_id = designer.add_node("remove_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, remove_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, remove_h_id);

    let atom_count = result.atom_ids().count();
    assert_eq!(
        atom_count, 0,
        "Empty structure should remain empty, got {} atoms",
        atom_count
    );
}

/// Disconnected node should return an error
#[test]
fn test_remove_hydrogen_node_no_input() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let remove_h_id = designer.add_node("remove_hydrogen", DVec2::ZERO);

    let result = evaluate_to_result(&designer, network_name, remove_h_id);

    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Disconnected remove_hydrogen should return an error"
    );
}

/// Ethane (2 C + 6 H) -> remove H -> 2 C with 1 C-C bond
#[test]
fn test_remove_hydrogen_node_ethane() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    let c1_id = structure.add_atom(6, DVec3::ZERO);
    let c2_id = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure.add_bond(c1_id, c2_id, BOND_SINGLE);

    // Add 3 H to each C
    let h_positions_c1 = [
        DVec3::new(-0.51, 0.89, 0.0),
        DVec3::new(-0.51, -0.45, 0.77),
        DVec3::new(-0.51, -0.45, -0.77),
    ];
    let h_positions_c2 = [
        DVec3::new(2.05, 0.89, 0.0),
        DVec3::new(2.05, -0.45, 0.77),
        DVec3::new(2.05, -0.45, -0.77),
    ];

    for pos in &h_positions_c1 {
        let h_id = structure.add_atom(1, *pos);
        structure.add_bond(c1_id, h_id, BOND_SINGLE);
    }
    for pos in &h_positions_c2 {
        let h_id = structure.add_atom(1, *pos);
        structure.add_bond(c2_id, h_id, BOND_SINGLE);
    }

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let remove_h_id = designer.add_node("remove_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, remove_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, remove_h_id);

    // 2 carbons remain
    let atom_count = result.atom_ids().count();
    assert_eq!(atom_count, 2, "Expected 2 atoms (2 C), got {}", atom_count);

    // C-C bond intact
    for &id in result.atom_ids() {
        let atom = result.get_atom(id).unwrap();
        assert_eq!(atom.atomic_number, 6, "All remaining atoms should be carbon");
        let bond_count = atom.bonds.iter().filter(|b| !b.is_delete_marker()).count();
        assert_eq!(bond_count, 1, "Each carbon should have 1 bond (C-C), got {}", bond_count);
    }
}

/// Structure with no H atoms passes through unchanged
#[test]
fn test_remove_hydrogen_node_no_hydrogens() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Two carbons bonded together, no H
    let mut structure = AtomicStructure::new();
    let c1_id = structure.add_atom(6, DVec3::ZERO);
    let c2_id = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure.add_bond(c1_id, c2_id, BOND_SINGLE);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let remove_h_id = designer.add_node("remove_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, remove_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, remove_h_id);

    let atom_count = result.atom_ids().count();
    assert_eq!(atom_count, 2, "Expected 2 atoms unchanged, got {}", atom_count);
}

/// Round-trip: bare C -> add_hydrogen -> remove_hydrogen -> 1 C, 0 bonds
#[test]
fn test_remove_hydrogen_node_roundtrip() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO); // Bare carbon

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let add_h_id = designer.add_node("add_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, add_h_id, 0);

    let remove_h_id = designer.add_node("remove_hydrogen", DVec2::new(400.0, 0.0));
    designer.connect_nodes(add_h_id, 0, remove_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, remove_h_id);

    // Should be back to just 1 carbon
    let atom_count = result.atom_ids().count();
    assert_eq!(atom_count, 1, "Round-trip should yield 1 atom, got {}", atom_count);

    let carbon = result
        .atom_ids()
        .find(|&&id| result.get_atom(id).unwrap().atomic_number == 6)
        .expect("Carbon should remain after round-trip");
    let bond_count = result
        .get_atom(*carbon)
        .unwrap()
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .count();
    assert_eq!(bond_count, 0, "Carbon should have 0 bonds after round-trip, got {}", bond_count);
}
