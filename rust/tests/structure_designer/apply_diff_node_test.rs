use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
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

/// Adds a value node containing an AtomicStructure directly to the network.
/// Returns the node_id. This bypasses StructureDesigner::add_node to avoid
/// any custom_node_type_cache side effects.
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

/// Evaluates a node and extracts the resulting AtomicStructure.
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

/// Evaluates a node and returns the raw NetworkResult.
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

/// Creates a base structure with 4 carbon atoms in a line along X.
fn create_base_line_structure() -> AtomicStructure {
    let mut base = AtomicStructure::new();
    let a1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let a2 = base.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let a3 = base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let a4 = base.add_atom(6, DVec3::new(4.5, 0.0, 0.0));
    base.add_bond(a1, a2, BOND_SINGLE);
    base.add_bond(a2, a3, BOND_SINGLE);
    base.add_bond(a3, a4, BOND_SINGLE);
    base
}

// ============================================================================
// Test: apply_diff_node_basic
// ============================================================================

#[test]
fn apply_diff_node_basic() {
    let network_name = "test_apply_diff_basic";
    let mut designer = setup_designer_with_network(network_name);

    // Create base: 4 carbons in a line
    let base = create_base_line_structure();

    // Create diff: delete atom at (1.5, 0, 0) and add a new silicon at (1.5, 1.0, 0)
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    diff.add_atom(14, DVec3::new(1.5, 1.0, 0.0));

    // Add value nodes directly into the network
    let base_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let diff_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::new(0.0, 100.0), diff);

    // Add apply_diff node
    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 50.0));

    // Connect: base -> apply_diff pin 0, diff -> apply_diff pin 1
    designer.connect_nodes(base_node_id, 0, apply_diff_id, 0);
    designer.connect_nodes(diff_node_id, 0, apply_diff_id, 1);

    // Evaluate
    let result = evaluate_to_atomic(&designer, network_name, apply_diff_id);

    // Base had 4 atoms, diff deletes 1 and adds 1 → result should have 4 atoms
    assert_eq!(
        result.get_num_of_atoms(),
        4,
        "Should have 4 atoms (4 - 1 + 1)"
    );

    // The deleted atom at (1.5, 0, 0) should be gone
    let has_deleted_pos = result
        .atoms_values()
        .any(|a| a.position.distance(DVec3::new(1.5, 0.0, 0.0)) < 0.01);
    assert!(
        !has_deleted_pos,
        "Atom at (1.5, 0, 0) should have been deleted"
    );

    // The new silicon at (1.5, 1.0, 0) should exist
    let has_new_si = result
        .atoms_values()
        .any(|a| a.atomic_number == 14 && a.position.distance(DVec3::new(1.5, 1.0, 0.0)) < 0.01);
    assert!(has_new_si, "New silicon atom at (1.5, 1.0, 0) should exist");
}

// ============================================================================
// Test: apply_diff_node_with_moved_diff (integration with Phase 1)
// ============================================================================

#[test]
fn apply_diff_node_with_moved_diff() {
    let network_name = "test_apply_diff_moved";
    let mut designer = setup_designer_with_network(network_name);

    // Create base: atoms at (0,0,0), (5,0,0), (10,0,0)
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(10.0, 0.0, 0.0));

    // Create diff: delete marker at (0,0,0)
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));

    let base_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let diff_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::new(0.0, 100.0), diff);

    // Add atom_move and apply_diff nodes
    let atom_move_id = designer.add_node("free_move", DVec2::new(100.0, 100.0));
    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 50.0));

    // Wire: diff -> atom_move -> apply_diff pin 1
    designer.connect_nodes(diff_node_id, 0, atom_move_id, 0);
    designer.connect_nodes(atom_move_id, 0, apply_diff_id, 1);
    // Wire: base -> apply_diff pin 0
    designer.connect_nodes(base_node_id, 0, apply_diff_id, 0);

    // Set atom_move translation to (5, 0, 0)
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        let move_node = network.nodes.get_mut(&atom_move_id).unwrap();
        use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
        let mut props = std::collections::HashMap::new();
        props.insert(
            "translation".to_string(),
            TextValue::Vec3(DVec3::new(5.0, 0.0, 0.0)),
        );
        move_node.data.set_text_properties(&props).unwrap();
    }

    // Evaluate
    let result = evaluate_to_atomic(&designer, network_name, apply_diff_id);

    // The delete marker was at (0,0,0) and moved by (5,0,0) → now at (5,0,0).
    // So the atom at (5,0,0) should be deleted, not the one at (0,0,0).
    assert_eq!(
        result.get_num_of_atoms(),
        2,
        "Should have 2 atoms (3 - 1 deleted)"
    );

    // Atom at (0,0,0) should still exist (delete marker was moved away)
    let has_origin = result
        .atoms_values()
        .any(|a| a.position.distance(DVec3::ZERO) < 0.01);
    assert!(
        has_origin,
        "Atom at origin should still exist (delete marker moved to (5,0,0))"
    );

    // Atom at (5,0,0) should be deleted (delete marker moved here)
    let has_five = result
        .atoms_values()
        .any(|a| a.position.distance(DVec3::new(5.0, 0.0, 0.0)) < 0.01);
    assert!(
        !has_five,
        "Atom at (5,0,0) should be deleted by the moved delete marker"
    );

    // Atom at (10,0,0) should still exist
    let has_ten = result
        .atoms_values()
        .any(|a| a.position.distance(DVec3::new(10.0, 0.0, 0.0)) < 0.01);
    assert!(has_ten, "Atom at (10,0,0) should still exist");
}

// ============================================================================
// Test: apply_diff_node_rejects_non_diff
// ============================================================================

#[test]
fn apply_diff_node_rejects_non_diff() {
    let network_name = "test_apply_diff_reject";
    let mut designer = setup_designer_with_network(network_name);

    // Both inputs are normal structures (is_diff = false)
    let base = AtomicStructure::new();
    let not_a_diff = AtomicStructure::new();

    let base_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let diff_node_id = add_atomic_value_node(
        &mut designer,
        network_name,
        DVec2::new(0.0, 100.0),
        not_a_diff,
    );

    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 50.0));
    designer.connect_nodes(base_node_id, 0, apply_diff_id, 0);
    designer.connect_nodes(diff_node_id, 0, apply_diff_id, 1);

    let result = evaluate_raw(&designer, network_name, apply_diff_id);

    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("is_diff = false"),
                "Error should mention is_diff, got: {}",
                msg
            );
        }
        _ => panic!("Expected NetworkResult::Error for non-diff input"),
    }
}

// ============================================================================
// Test: apply_diff_node_error_on_stale
// ============================================================================

#[test]
fn apply_diff_node_error_on_stale() {
    let network_name = "test_apply_diff_stale";
    let mut designer = setup_designer_with_network(network_name);

    // Base: single atom at origin
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // Diff: anchored atom pointing to a position that doesn't exist in base
    let mut diff = AtomicStructure::new_diff();
    let diff_atom = diff.add_atom(6, DVec3::new(100.0, 0.0, 0.0));
    diff.set_anchor_position(diff_atom, DVec3::new(50.0, 50.0, 50.0));

    let base_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let diff_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::new(0.0, 100.0), diff);

    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 50.0));
    designer.connect_nodes(base_node_id, 0, apply_diff_id, 0);
    designer.connect_nodes(diff_node_id, 0, apply_diff_id, 1);

    // Set error_on_stale = true
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        let node = network.nodes.get_mut(&apply_diff_id).unwrap();
        use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
        let mut props = std::collections::HashMap::new();
        props.insert("error_on_stale".to_string(), TextValue::Bool(true));
        node.data.set_text_properties(&props).unwrap();
    }

    let result = evaluate_raw(&designer, network_name, apply_diff_id);

    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("stale") || msg.contains("orphaned"),
                "Error should mention stale/orphaned entries, got: {}",
                msg
            );
        }
        _ => panic!("Expected NetworkResult::Error for stale diff entries"),
    }
}

// ============================================================================
// Test: apply_diff_node_modification (move atom via anchor)
// ============================================================================

#[test]
fn apply_diff_node_modification() {
    let network_name = "test_apply_diff_mod";
    let mut designer = setup_designer_with_network(network_name);

    // Base: carbon at (0,0,0) and carbon at (3,0,0)
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));

    // Diff: modify the atom at (0,0,0) — move to (0,0,2) and change to silicon
    let mut diff = AtomicStructure::new_diff();
    let moved_atom = diff.add_atom(14, DVec3::new(0.0, 0.0, 2.0));
    diff.set_anchor_position(moved_atom, DVec3::new(0.0, 0.0, 0.0));

    let base_node_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let diff_node_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::new(0.0, 100.0), diff);

    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 50.0));
    designer.connect_nodes(base_node_id, 0, apply_diff_id, 0);
    designer.connect_nodes(diff_node_id, 0, apply_diff_id, 1);

    let result = evaluate_to_atomic(&designer, network_name, apply_diff_id);

    assert_eq!(result.get_num_of_atoms(), 2, "Should still have 2 atoms");

    // Should have silicon at (0,0,2)
    let has_si = result
        .atoms_values()
        .any(|a| a.atomic_number == 14 && a.position.distance(DVec3::new(0.0, 0.0, 2.0)) < 0.01);
    assert!(has_si, "Modified atom should be silicon at (0, 0, 2)");

    // Should have carbon at (3,0,0) unchanged
    let has_c = result
        .atoms_values()
        .any(|a| a.atomic_number == 6 && a.position.distance(DVec3::new(3.0, 0.0, 0.0)) < 0.01);
    assert!(has_c, "Unmodified carbon at (3,0,0) should remain");

    // Should NOT have carbon at (0,0,0) (it was replaced)
    let has_old = result
        .atoms_values()
        .any(|a| a.atomic_number == 6 && a.position.distance(DVec3::ZERO) < 0.01);
    assert!(!has_old, "Original carbon at (0,0,0) should be replaced");
}

// ============================================================================
// Test: apply_diff_node_snapshot
// ============================================================================

#[test]
fn apply_diff_node_snapshot() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("apply_diff").unwrap();

    // Verify the node type is registered with correct properties
    assert_eq!(node_type.name, "apply_diff");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 3);
    assert_eq!(node_type.parameters[0].name, "base");
    assert_eq!(node_type.parameters[1].name, "diff");
    assert_eq!(node_type.parameters[2].name, "tolerance");

    // Verify default data: create a node and check text properties
    let data = (node_type.node_data_creator)();
    let props = data.get_text_properties();
    let tolerance_prop = props.iter().find(|(k, _)| k == "tolerance").unwrap();
    let error_prop = props.iter().find(|(k, _)| k == "error_on_stale").unwrap();

    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
    assert!(matches!(&tolerance_prop.1, TextValue::Float(v) if (*v - 0.1).abs() < 1e-10));
    assert!(matches!(&error_prop.1, TextValue::Bool(false)));
}
