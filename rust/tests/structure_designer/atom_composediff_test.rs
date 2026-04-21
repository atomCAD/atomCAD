use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

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

/// Asserts two AtomicStructures are semantically equal by position-based matching.
fn assert_structures_equal(a: &AtomicStructure, b: &AtomicStructure, tolerance: f64) {
    assert_eq!(
        a.get_num_of_atoms(),
        b.get_num_of_atoms(),
        "atom count mismatch: {} vs {}",
        a.get_num_of_atoms(),
        b.get_num_of_atoms()
    );

    let a_atoms: Vec<_> = a.atoms_values().collect();
    let b_atoms: Vec<_> = b.atoms_values().collect();
    let mut b_matched = vec![false; b_atoms.len()];

    for a_atom in &a_atoms {
        let mut found = false;
        for (j, b_atom) in b_atoms.iter().enumerate() {
            if !b_matched[j]
                && a_atom.position.distance(b_atom.position) < tolerance
                && a_atom.atomic_number == b_atom.atomic_number
            {
                b_matched[j] = true;
                found = true;
                break;
            }
        }
        assert!(
            found,
            "No matching atom found in B for atom at {:?} (Z={})",
            a_atom.position, a_atom.atomic_number
        );
    }

    assert_eq!(
        a.get_num_of_bonds(),
        b.get_num_of_bonds(),
        "bond count mismatch: {} vs {}",
        a.get_num_of_bonds(),
        b.get_num_of_bonds()
    );
}

/// Verifies compose equivalence: sequential application == composed application.
fn assert_compose_equivalence(base: &AtomicStructure, diffs: &[&AtomicStructure], tolerance: f64) {
    // Sequential application
    let mut sequential = base.clone();
    for diff in diffs {
        sequential = atomic_structure_diff::apply_diff(&sequential, diff, tolerance).result;
    }

    // Composed application
    let composed = atomic_structure_diff::compose_diffs(diffs, tolerance).unwrap();
    let composed_result =
        atomic_structure_diff::apply_diff(base, &composed.composed, tolerance).result;

    assert_structures_equal(&sequential, &composed_result, tolerance);
}

// ============================================================================
// Test: atom_composediff_basic_two_diffs
//
// Uses the text format to build a network with atom_edit nodes, then composes
// the diffs via atom_composediff and compares with sequential application.
// ============================================================================

#[test]
fn atom_composediff_basic_two_diffs() {
    // Test compose equivalence at the crystolecule level using the node's core algorithm
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(4.0, 0.0, 0.0));

    // diff1: add N at (6,0,0) and Si at (8,0,0)
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(6.0, 0.0, 0.0));
    diff1.add_atom(14, DVec3::new(8.0, 0.0, 0.0));

    // diff2: delete atom at (4,0,0)
    let mut diff2 = AtomicStructure::new_diff();
    diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(4.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], 0.1);

    // Test via compose_diffs directly
    let composed = atomic_structure_diff::compose_diffs(&[&diff1, &diff2], 0.1).unwrap();
    assert!(composed.composed.is_diff());

    // Apply composed to base
    let composed_result = atomic_structure_diff::apply_diff(&base, &composed.composed, 0.1).result;

    // Should have: C(0), C(2), N(6), Si(8) — original C(4) deleted
    assert_eq!(composed_result.get_num_of_atoms(), 4);
}

// ============================================================================
// Test: atom_composediff_equivalence_with_chained_apply_diff
// ============================================================================

#[test]
fn atom_composediff_equivalence_with_chained_apply_diff() {
    let mut base = AtomicStructure::new();
    let a1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let a2 = base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    base.add_bond(a1, a2, BOND_SINGLE);

    // diff1: add N at (6,0,0)
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(6.0, 0.0, 0.0));

    // diff2: move C from (3,0,0) to (3,1,0)
    let mut diff2 = AtomicStructure::new_diff();
    let moved = diff2.add_atom(6, DVec3::new(3.0, 1.0, 0.0));
    diff2.set_anchor_position(moved, DVec3::new(3.0, 0.0, 0.0));

    // diff3: delete C at origin
    let mut diff3 = AtomicStructure::new_diff();
    diff3.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], 0.1);
}

// ============================================================================
// Test: atom_composediff_single_input
// ============================================================================

#[test]
fn atom_composediff_single_input() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(7, DVec3::new(1.0, 0.0, 0.0));

    let result = atomic_structure_diff::compose_diffs(&[&diff], 0.1).unwrap();

    assert!(result.composed.is_diff());
    assert_eq!(result.composed.get_num_of_atoms(), 1);
    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.atomic_number, 7);
    assert!(atom.position.distance(DVec3::new(1.0, 0.0, 0.0)) < 0.01);
}

// ============================================================================
// Test: atom_composediff_error_non_diff_input
// ============================================================================

#[test]
fn atom_composediff_error_non_diff_input() {
    let network_name = "test_composediff_non_diff";
    let mut designer = setup_designer_with_network(network_name);

    // A normal structure (not a diff) - wire to non-array single pin
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);

    let val_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);

    let compose_id = designer.add_node("atom_composediff", DVec2::new(200.0, 0.0));
    designer.connect_nodes(val_id, 0, compose_id, 0);

    let result = evaluate_raw(&designer, network_name, compose_id);

    // Should be an error (either conversion error or "not a diff" error)
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Expected error for non-diff input"
    );
}

// ============================================================================
// Test: atom_composediff_empty_input
// ============================================================================

#[test]
fn atom_composediff_empty_input() {
    let network_name = "test_composediff_empty";
    let mut designer = setup_designer_with_network(network_name);

    let compose_id = designer.add_node("atom_composediff", DVec2::new(200.0, 0.0));

    let result = evaluate_raw(&designer, network_name, compose_id);

    assert!(
        matches!(result, NetworkResult::Error(_)),
        "Expected error for empty input"
    );
}

// ============================================================================
// Test: atom_composediff_text_format_roundtrip
// ============================================================================

#[test]
fn atom_composediff_text_format_roundtrip() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("atom_composediff").unwrap();

    assert_eq!(node_type.name, "atom_composediff");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 2);
    assert_eq!(node_type.parameters[0].name, "diffs");
    assert_eq!(node_type.parameters[1].name, "tolerance");

    // Verify default data via text properties
    let data = (node_type.node_data_creator)();
    let props = data.get_text_properties();

    let tolerance_prop = props.iter().find(|(k, _)| k == "tolerance").unwrap();
    let error_prop = props.iter().find(|(k, _)| k == "error_on_stale").unwrap();

    assert!(matches!(&tolerance_prop.1, TextValue::Float(v) if (*v - 0.1).abs() < 1e-10));
    assert!(matches!(&error_prop.1, TextValue::Bool(false)));

    // Test set_text_properties roundtrip
    let mut data2 = (node_type.node_data_creator)();
    let mut new_props = std::collections::HashMap::new();
    new_props.insert("tolerance".to_string(), TextValue::Float(0.05));
    new_props.insert("error_on_stale".to_string(), TextValue::Bool(true));
    data2.set_text_properties(&new_props).unwrap();

    let props2 = data2.get_text_properties();
    let tolerance_prop2 = props2.iter().find(|(k, _)| k == "tolerance").unwrap();
    let error_prop2 = props2.iter().find(|(k, _)| k == "error_on_stale").unwrap();
    assert!(matches!(&tolerance_prop2.1, TextValue::Float(v) if (*v - 0.05).abs() < 1e-10));
    assert!(matches!(&error_prop2.1, TextValue::Bool(true)));
}

// ============================================================================
// Test: atom_composediff_composed_diff_is_diff
// ============================================================================

#[test]
fn atom_composediff_composed_diff_is_diff() {
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(1.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    diff2.add_atom(8, DVec3::new(2.0, 0.0, 0.0));

    let result = atomic_structure_diff::compose_diffs(&[&diff1, &diff2], 0.1).unwrap();
    assert!(
        result.composed.is_diff(),
        "Composed result must have is_diff = true"
    );
}

// ============================================================================
// Test: atom_composediff_node_eval_with_text_format
//
// Uses the text format to build and evaluate a network containing
// atom_composediff wired to atom_edit .diff outputs.
// ============================================================================

#[test]
fn atom_composediff_node_eval_with_text_format() {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
    use rust_lib_flutter_cad::structure_designer::text_format::edit_network;

    let registry = NodeTypeRegistry::new();

    let network_type = NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::HasAtoms),
        node_data_creator: || {
            Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
        },
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        public: true,
    };
    let mut network = NodeNetwork::new(network_type);

    // Build network via text format
    // Multi-input pins use array syntax: diffs: [ref1, ref2]
    let text = r#"
base = materialize {}
edit1 = atom_edit { base: base }
edit2 = atom_edit { base: edit1 }
composed = atom_composediff { diffs: [edit1.diff, edit2.diff] }
result = apply_diff { base: base, diff: composed }
"#;

    let edit_result = edit_network(&mut network, &registry, text, true);

    assert!(
        edit_result.errors.is_empty(),
        "Text format errors: {:?}",
        edit_result.errors
    );

    // Verify the atom_composediff node was created
    let compose_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "atom_composediff");
    assert!(compose_node.is_some(), "atom_composediff node should exist");

    // Verify two wires connected to the diffs pin
    let compose_node = compose_node.unwrap();
    assert_eq!(
        compose_node.arguments[0].argument_output_pins.len(),
        2,
        "diffs pin should have 2 wires connected"
    );
}

// ============================================================================
// Test: atom_composediff_node_snapshot
// ============================================================================

#[test]
fn atom_composediff_node_snapshot() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("atom_composediff").unwrap();

    assert_eq!(node_type.name, "atom_composediff");
    assert!(node_type.public);
    assert_eq!(node_type.parameters.len(), 2);

    // Verify parameter types
    assert_eq!(node_type.parameters[0].name, "diffs");
    assert!(
        node_type.parameters[0].data_type.is_array(),
        "diffs parameter should be an array type"
    );
    assert_eq!(node_type.parameters[1].name, "tolerance");

    // Verify output pin is polymorphic (mirrors array element type).
    use rust_lib_flutter_cad::structure_designer::node_type::PinOutputType;
    assert!(matches!(
        &node_type.output_pins[0].data_type,
        PinOutputType::SameAsArrayElements(name) if name == "diffs"
    ));

    // Verify node data defaults
    let data = (node_type.node_data_creator)();
    let subtitle = data.get_subtitle(&std::collections::HashSet::new());
    assert_eq!(subtitle, Some("tol=0.100".to_string()));
}
