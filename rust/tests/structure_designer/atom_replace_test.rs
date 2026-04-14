use glam::IVec2;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::nodes::atom_replace::AtomReplaceData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers
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
    structure: AtomicStructure,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::Molecule(MoleculeData { atoms: structure, geo_tree_root: None }),
    });
    network.add_node("value", DVec2::ZERO, 0, value_data)
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

fn add_replace_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    replacements: Vec<(i16, i16)>,
) -> u64 {
    let replace_id = designer.add_node("atom_replace", DVec2::new(200.0, 0.0));
    let data = Box::new(AtomReplaceData { replacements });
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(replace_id, data);
    replace_id
}

/// Build a simple structure: 2 C atoms + 1 O atom, all bonded in a line
fn carbon_oxygen_structure() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO); // Carbon
    let c2 = s.add_atom(6, DVec3::new(1.54, 0.0, 0.0)); // Carbon
    let o = s.add_atom(8, DVec3::new(3.0, 0.0, 0.0)); // Oxygen
    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, o, BOND_SINGLE);
    s
}

// ============================================================================
// Tests
// ============================================================================

/// No replacement rules → structure passes through unchanged
#[test]
fn atom_replace_empty_rules() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure();
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    let replace_id = add_replace_node(&mut designer, net, vec![]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 3);
    assert_eq!(result.get_num_of_bonds(), 2);
    // All elements unchanged
    for (_id, atom) in result.iter_atoms() {
        assert!(atom.atomic_number == 6 || atom.atomic_number == 8);
    }
}

/// Single rule: replace C(6) → Si(14)
#[test]
fn atom_replace_single_rule() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure();
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]); // C→Si
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 3);

    let mut si_count = 0;
    let mut o_count = 0;
    for (_id, atom) in result.iter_atoms() {
        match atom.atomic_number {
            14 => si_count += 1,
            8 => o_count += 1,
            other => panic!("Unexpected element: {}", other),
        }
    }
    assert_eq!(si_count, 2, "Both carbons should become silicon");
    assert_eq!(o_count, 1, "Oxygen should be unchanged");
}

/// Multiple simultaneous replacement rules
#[test]
fn atom_replace_multiple_rules() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure();
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    // C→Si, O→S
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14), (8, 16)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 3);

    let mut si_count = 0;
    let mut s_count = 0;
    for (_id, atom) in result.iter_atoms() {
        match atom.atomic_number {
            14 => si_count += 1,
            16 => s_count += 1,
            other => panic!("Unexpected element: {}", other),
        }
    }
    assert_eq!(si_count, 2);
    assert_eq!(s_count, 1);
}

/// Rules for elements not present → silently ignored, no error
#[test]
fn atom_replace_no_matching_atoms() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure();
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    // Replace nitrogen (not in structure) with phosphorus
    let replace_id = add_replace_node(&mut designer, net, vec![(7, 15)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 3);
    // Everything unchanged
    let mut c_count = 0;
    let mut o_count = 0;
    for (_id, atom) in result.iter_atoms() {
        match atom.atomic_number {
            6 => c_count += 1,
            8 => o_count += 1,
            other => panic!("Unexpected element: {}", other),
        }
    }
    assert_eq!(c_count, 2);
    assert_eq!(o_count, 1);
}

/// Bonds remain intact after element replacement
#[test]
fn atom_replace_preserves_bonds() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure();
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(
        result.get_num_of_bonds(),
        2,
        "Bonds should be preserved after replacement"
    );
}

/// Atom positions unchanged after replacement
#[test]
fn atom_replace_preserves_positions() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let mut structure = AtomicStructure::new();
    let pos1 = DVec3::new(1.0, 2.0, 3.0);
    let pos2 = DVec3::new(4.0, 5.0, 6.0);
    structure.add_atom(6, pos1);
    structure.add_atom(6, pos2);

    let value_id = add_atomic_value_node(&mut designer, net, structure);
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    let positions: Vec<DVec3> = result.iter_atoms().map(|(_, a)| a.position).collect();
    assert!(positions.contains(&pos1));
    assert!(positions.contains(&pos2));
}

/// Target atomic number 0 → delete matched atoms and their bonds
#[test]
fn atom_replace_delete_target() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure(); // 2C + 1O, C-C bond + C-O bond
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    // Delete all oxygen atoms
    let replace_id = add_replace_node(&mut designer, net, vec![(8, 0)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 2, "Oxygen should be deleted");
    assert_eq!(
        result.get_num_of_bonds(),
        1,
        "C-O bond should be removed, C-C bond remains"
    );
    for (_id, atom) in result.iter_atoms() {
        assert_eq!(atom.atomic_number, 6, "Only carbons should remain");
    }
}

/// Mix of element replacements and deletions in one rule set
#[test]
fn atom_replace_delete_mixed() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let structure = carbon_oxygen_structure(); // 2C + 1O
    let value_id = add_atomic_value_node(&mut designer, net, structure);
    // C→Si, O→delete
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14), (8, 0)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(
        result.atom_ids().count(),
        2,
        "Oxygen deleted, two Si remain"
    );
    for (_id, atom) in result.iter_atoms() {
        assert_eq!(atom.atomic_number, 14, "All remaining should be silicon");
    }
    assert_eq!(result.get_num_of_bonds(), 1, "Only Si-Si bond remains");
}

/// Atoms with atomic_number 0 (delete marker) or -1 (unchanged marker) are never replaced
#[test]
fn atom_replace_skip_markers() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let mut structure = AtomicStructure::new();
    structure.add_atom(0, DVec3::ZERO); // delete marker
    structure.add_atom(-1, DVec3::new(1.0, 0.0, 0.0)); // unchanged marker
    structure.add_atom(6, DVec3::new(2.0, 0.0, 0.0)); // normal carbon

    let value_id = add_atomic_value_node(&mut designer, net, structure);
    // Rule maps 0→1 and -1→1 — should not affect markers
    let replace_id = add_replace_node(&mut designer, net, vec![(0, 1), (-1, 1), (6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    let mut elements: Vec<i16> = result.iter_atoms().map(|(_, a)| a.atomic_number).collect();
    elements.sort();
    assert_eq!(
        elements,
        vec![-1, 0, 14],
        "Markers should be unchanged, carbon becomes silicon"
    );
}

/// Text properties roundtrip: get then set
#[test]
fn atom_replace_text_properties_roundtrip() {
    // Empty replacements → no properties
    let data = AtomReplaceData::default();
    let props = data.get_text_properties();
    assert!(
        props.is_empty(),
        "Default should produce no text properties"
    );

    // Non-empty replacements
    let data = AtomReplaceData {
        replacements: vec![(6, 14), (8, 16)],
    };
    let props = data.get_text_properties();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].0, "replacements");
    if let TextValue::Array(items) = &props[0].1 {
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], TextValue::IVec2(IVec2::new(6, 14)));
        assert_eq!(items[1], TextValue::IVec2(IVec2::new(8, 16)));
    } else {
        panic!("Expected Array");
    }

    // Roundtrip: set then get
    let mut data2 = AtomReplaceData::default();
    let mut map = HashMap::new();
    map.insert(
        "replacements".to_string(),
        TextValue::Array(vec![
            TextValue::IVec2(IVec2::new(6, 14)),
            TextValue::IVec2(IVec2::new(8, 16)),
        ]),
    );
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.replacements, vec![(6, 14), (8, 16)]);
}

/// Subtitle formatting
#[test]
fn atom_replace_subtitle() {
    let connected = std::collections::HashSet::new();

    // Empty
    let data = AtomReplaceData {
        replacements: vec![],
    };
    assert_eq!(
        data.get_subtitle(&connected),
        Some("(no replacements)".to_string())
    );

    // Single rule: C→Si
    let data = AtomReplaceData {
        replacements: vec![(6, 14)],
    };
    assert_eq!(data.get_subtitle(&connected), Some("C→Si".to_string()));

    // Two rules
    let data = AtomReplaceData {
        replacements: vec![(6, 14), (8, 16)],
    };
    assert_eq!(data.get_subtitle(&connected), Some("C→Si, O→S".to_string()));

    // Three rules
    let data = AtomReplaceData {
        replacements: vec![(6, 14), (8, 16), (1, 3)],
    };
    assert_eq!(
        data.get_subtitle(&connected),
        Some("C→Si, O→S, H→Li".to_string())
    );

    // Four rules → truncated
    let data = AtomReplaceData {
        replacements: vec![(6, 14), (8, 16), (1, 3), (7, 15)],
    };
    assert_eq!(
        data.get_subtitle(&connected),
        Some("C→Si, O→S, H→Li, … (+1 more)".to_string())
    );

    // Delete rule
    let data = AtomReplaceData {
        replacements: vec![(1, 0)],
    };
    assert_eq!(data.get_subtitle(&connected), Some("H→(del)".to_string()));
}

/// .cnnd roundtrip: serialize then deserialize AtomReplaceData
#[test]
fn atom_replace_cnnd_roundtrip() {
    let data = AtomReplaceData {
        replacements: vec![(6, 14), (8, 0), (1, 3)],
    };
    let json = serde_json::to_string(&data).unwrap();
    let loaded: AtomReplaceData = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.replacements, data.replacements);
}
