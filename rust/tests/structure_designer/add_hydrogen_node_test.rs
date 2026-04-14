use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
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

// ============================================================================
// Tests
// ============================================================================

/// Bare carbon atom -> CH4 with 5 atoms and 4 bonds
#[test]
fn test_add_hydrogen_node_bare_carbon() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Create a structure with a single carbon atom at origin
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO); // Carbon

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let add_h_id = designer.add_node("add_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, add_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, add_h_id);

    // 1 Carbon + 4 Hydrogens = 5 atoms
    let atom_count = result.atom_ids().count();
    assert_eq!(
        atom_count, 5,
        "Expected 5 atoms (1 C + 4 H), got {}",
        atom_count
    );

    // Count hydrogens
    let h_count = result
        .atom_ids()
        .filter(|&&id| result.get_atom(id).unwrap().atomic_number == 1)
        .count();
    assert_eq!(h_count, 4, "Expected 4 hydrogen atoms, got {}", h_count);

    // Each hydrogen should be bonded (verify bond count on carbon)
    let carbon_id = *result
        .atom_ids()
        .find(|&&id| result.get_atom(id).unwrap().atomic_number == 6)
        .unwrap();
    let carbon = result.get_atom(carbon_id).unwrap();
    let carbon_bonds: Vec<_> = carbon
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .collect();
    assert_eq!(
        carbon_bonds.len(),
        4,
        "Carbon should have 4 bonds, got {}",
        carbon_bonds.len()
    );

    // Verify C-H bond lengths are ~1.09 A
    let carbon_pos = carbon.position;
    for &h_id in result.atom_ids() {
        let atom = result.get_atom(h_id).unwrap();
        if atom.atomic_number == 1 {
            let dist = (atom.position - carbon_pos).length();
            assert!(
                (dist - 1.09).abs() < 0.01,
                "C-H bond length should be ~1.09 A, got {}",
                dist
            );
        }
    }
}

/// Already-saturated structure should pass through unchanged
#[test]
fn test_add_hydrogen_node_saturated_structure() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Build a water molecule (O with 2 H already bonded - saturated)
    let mut structure = AtomicStructure::new();
    let o_id = structure.add_atom(8, DVec3::ZERO);
    let h1_id = structure.add_atom(1, DVec3::new(0.757, 0.586, 0.0));
    let h2_id = structure.add_atom(1, DVec3::new(-0.757, 0.586, 0.0));
    structure.add_bond(o_id, h1_id, BOND_SINGLE);
    structure.add_bond(o_id, h2_id, BOND_SINGLE);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let add_h_id = designer.add_node("add_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, add_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, add_h_id);

    // Water is already saturated: O has 2 bonds = max for sp3 oxygen
    // Should still have exactly 3 atoms
    let atom_count = result.atom_ids().count();
    assert_eq!(
        atom_count, 3,
        "Saturated water should remain 3 atoms, got {}",
        atom_count
    );
}

/// Empty structure should produce empty output
#[test]
fn test_add_hydrogen_node_empty_structure() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let structure = AtomicStructure::new();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let add_h_id = designer.add_node("add_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, add_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, add_h_id);

    let atom_count = result.atom_ids().count();
    assert_eq!(
        atom_count, 0,
        "Empty structure should remain empty, got {} atoms",
        atom_count
    );
}

/// Disconnected add_hydrogen node should return an error
#[test]
fn test_add_hydrogen_node_no_input() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let add_h_id = designer.add_node("add_hydrogen", DVec2::ZERO);

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];

    let result = evaluator.evaluate(&network_stack, add_h_id, 0, registry, false, &mut context);

    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Disconnected add_hydrogen should return an error"
    );
}

/// Nitrogen with one bond -> should add 2 H's (sp3, max 3 bonds)
#[test]
fn test_add_hydrogen_node_nitrogen() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    let n_id = structure.add_atom(7, DVec3::ZERO); // Nitrogen
    let c_id = structure.add_atom(6, DVec3::new(1.47, 0.0, 0.0)); // Carbon neighbor
    structure.add_bond(n_id, c_id, BOND_SINGLE);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let add_h_id = designer.add_node("add_hydrogen", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, add_h_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, add_h_id);

    // N has 1 bond, max 3 -> needs 2 H. C has 1 bond, max 4 -> needs 3 H.
    // Total: 2 (original) + 2 (N-H) + 3 (C-H) = 7
    let atom_count = result.atom_ids().count();
    assert_eq!(
        atom_count, 7,
        "Expected 7 atoms (N + C + 5H), got {}",
        atom_count
    );

    let h_count = result
        .atom_ids()
        .filter(|&&id| result.get_atom(id).unwrap().atomic_number == 1)
        .count();
    assert_eq!(h_count, 5, "Expected 5 hydrogen atoms, got {}", h_count);
}
