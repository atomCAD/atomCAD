use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::passivate::PassivateData;
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
    let add_h_id = designer.add_node("passivate", DVec2::new(200.0, 0.0));
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
    let add_h_id = designer.add_node("passivate", DVec2::new(200.0, 0.0));
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
    let add_h_id = designer.add_node("passivate", DVec2::new(200.0, 0.0));
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

    let add_h_id = designer.add_node("passivate", DVec2::ZERO);

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
    let add_h_id = designer.add_node("passivate", DVec2::new(200.0, 0.0));
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

// ============================================================================
// Element property / pin tests (halogen passivation, issue #405 Phase 2)
// ============================================================================

/// Add a `passivate` node with a stored `element` property to the active
/// network (mirrors `add_atomic_value_node`'s direct-construction pattern).
fn add_passivate_node_with_element(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    element: i16,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    // 3 pins: molecule, region, element.
    network.add_node(
        "passivate",
        position,
        3,
        Box::new(PassivateData { element }),
    )
}

/// Add an `Int`-valued `value` node.
fn add_int_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    value: i32,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::Int(value),
    });
    network.add_node("value", position, 0, value_data)
}

/// Like `evaluate_to_atomic` but returns the raw result so error cases can be
/// inspected (D1 rejection surfaces as `NetworkResult::Error`).
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

/// Assert that every terminator atom of `element` sits at `expected_len` from
/// the single heavy (carbon) atom, and count them.
fn assert_terminators(result: &AtomicStructure, element: i16, expected_len: f64) -> usize {
    let carbon_id = *result
        .atom_ids()
        .find(|&&id| result.get_atom(id).unwrap().atomic_number == 6)
        .expect("expected a carbon host");
    let carbon_pos = result.get_atom(carbon_id).unwrap().position;
    let mut count = 0;
    for &id in result.atom_ids() {
        let atom = result.get_atom(id).unwrap();
        if atom.atomic_number == element {
            let dist = (atom.position - carbon_pos).length();
            assert!(
                (dist - expected_len).abs() < 1e-6,
                "{} bond length should be ~{}, got {}",
                element,
                expected_len,
                dist
            );
            count += 1;
        }
    }
    count
}

/// Stored `element = 9` (fluorine): a bare carbon → CF4, all four F at the
/// C–F molecular bond length 1.35 Å, and each terminator carries the
/// passivation flag (D5: the general path flags its terminators).
#[test]
fn test_passivate_fluorine_stored() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let passivate_id =
        add_passivate_node_with_element(&mut designer, network_name, DVec2::new(200.0, 0.0), 9);
    designer.connect_nodes(value_id, 0, passivate_id, 0);

    let result = evaluate_to_atomic(&designer, network_name, passivate_id);

    assert_eq!(result.atom_ids().count(), 5, "Expected 1 C + 4 F");
    let f_count = assert_terminators(&result, 9, 1.35);
    assert_eq!(
        f_count, 4,
        "Expected 4 fluorine terminators, got {}",
        f_count
    );

    // No hydrogens placed.
    let h_count = result
        .atom_ids()
        .filter(|&&id| result.get_atom(id).unwrap().atomic_number == 1)
        .count();
    assert_eq!(h_count, 0, "No hydrogens expected under F passivation");

    // Terminators carry the passivation flag (D5).
    for &id in result.atom_ids() {
        let atom = result.get_atom(id).unwrap();
        if atom.atomic_number == 9 {
            assert!(
                atom.is_hydrogen_passivation(),
                "F terminator should carry the passivation flag"
            );
        }
    }
}

/// Passivation is deterministic: two identical runs place terminators at
/// byte-identical positions (the reproducibility motivation for issue #405).
#[test]
fn test_passivate_fluorine_deterministic() {
    let network_name = "test";

    let run = || {
        let mut designer = setup_designer_with_network(network_name);
        let mut structure = AtomicStructure::new();
        structure.add_atom(6, DVec3::ZERO);
        let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
        let passivate_id =
            add_passivate_node_with_element(&mut designer, network_name, DVec2::new(200.0, 0.0), 9);
        designer.connect_nodes(value_id, 0, passivate_id, 0);
        let result = evaluate_to_atomic(&designer, network_name, passivate_id);
        let mut positions: Vec<[f64; 3]> = result
            .atom_ids()
            .filter(|&&id| result.get_atom(id).unwrap().atomic_number == 9)
            .map(|&id| {
                let p = result.get_atom(id).unwrap().position;
                [p.x, p.y, p.z]
            })
            .collect();
        positions.sort_by(|a, b| a.partial_cmp(b).unwrap());
        positions
    };

    assert_eq!(run(), run(), "F passivation must be deterministic");
}

/// A wired `element` pin (Int 17 = chlorine) overrides the stored default
/// (H) — precedence path from D4.
#[test]
fn test_passivate_element_wired_pin_override() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    // Stored element stays hydrogen (1); the wired pin should win.
    let passivate_id =
        add_passivate_node_with_element(&mut designer, network_name, DVec2::new(200.0, 0.0), 1);
    let int_id = add_int_value_node(&mut designer, network_name, DVec2::new(0.0, 200.0), 17);
    designer.connect_nodes(value_id, 0, passivate_id, 0);
    // element pin is index 2.
    designer.connect_nodes(int_id, 0, passivate_id, 2);

    let result = evaluate_to_atomic(&designer, network_name, passivate_id);

    // C–Cl molecular bond length is 1.77 Å.
    let cl_count = assert_terminators(&result, 17, 1.77);
    assert_eq!(
        cl_count, 4,
        "Expected 4 chlorine terminators, got {}",
        cl_count
    );
    let h_count = result
        .atom_ids()
        .filter(|&&id| result.get_atom(id).unwrap().atomic_number == 1)
        .count();
    assert_eq!(
        h_count, 0,
        "Wired Cl pin should override the stored H default"
    );
}

/// D1: a stored non-monovalent element (oxygen, 8) surfaces a localized eval
/// error naming the allowed set, without panicking.
#[test]
fn test_passivate_invalid_element_stored() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let passivate_id =
        add_passivate_node_with_element(&mut designer, network_name, DVec2::new(200.0, 0.0), 8);
    designer.connect_nodes(value_id, 0, passivate_id, 0);

    let result = evaluate_raw(&designer, network_name, passivate_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("allowed passivant"),
                "error should name the allowed-passivant rule, got: {}",
                msg
            );
        }
        other => panic!(
            "Expected an Error for element 8, got {:?}",
            other.infer_data_type()
        ),
    }
}

/// D1: an invalid element supplied via the wired pin is rejected the same way.
#[test]
fn test_passivate_invalid_element_via_pin() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);

    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let passivate_id =
        add_passivate_node_with_element(&mut designer, network_name, DVec2::new(200.0, 0.0), 1);
    let int_id = add_int_value_node(&mut designer, network_name, DVec2::new(0.0, 200.0), 8);
    designer.connect_nodes(value_id, 0, passivate_id, 0);
    designer.connect_nodes(int_id, 0, passivate_id, 2);

    let result = evaluate_raw(&designer, network_name, passivate_id);
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Invalid element via pin should surface an Error"
    );
}
