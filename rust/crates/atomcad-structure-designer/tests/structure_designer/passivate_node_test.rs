use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::nodes::passivate::PassivateData;
use atomcad_structure_designer::nodes::value::add_value_node;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};

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
    let value = NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    });
    add_value_node(network, position, value)
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
    let network_stack = vec![NetworkStackElement::root(network)];

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
    let network_stack = vec![NetworkStackElement::root(network)];

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
        Box::new(PassivateData {
            element,
            ..PassivateData::default()
        }),
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
    add_value_node(network, position, NetworkResult::Int(value))
}

/// Like `evaluate_to_atomic` but returns the raw result so error cases can be
/// inspected (D1 rejection surfaces as `NetworkResult::Error`).
fn evaluate_raw(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement::root(network)];
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

// ============================================================================
// skip_clashes (doc/design_chemisorb_mirrors_and_passivation.md §5.2)
// ============================================================================

use atomcad_crystolecule::hydrogen_passivation::DEFAULT_PASSIVATION_CLASH_FACTOR;
use atomcad_structure_designer::nodes::passivate::PassivateEvalCache;

/// A bare carbon whose (1,1,1) slot points at a methane carbon 2.3 Å away:
/// that H would sit 1.21 Å from it, inside 0.7 of the H–C sum; the other
/// three slots are clear.
fn carbon_facing_methane() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    s.add_atom(6, DVec3::ZERO);
    let centre = DVec3::ONE.normalize() * 2.3;
    let methane = s.add_atom(6, centre);
    for d in [
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(-1.0, -1.0, 1.0),
        DVec3::new(-1.0, 1.0, -1.0),
        DVec3::new(1.0, -1.0, -1.0),
    ] {
        let h = s.add_atom(1, centre + d.normalize() * 1.09);
        s.add_bond(methane, h, BOND_SINGLE);
    }
    s
}

fn add_passivate_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    data: PassivateData,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("passivate", position, 3, Box::new(data))
}

/// Evaluates `data` on `structure` and returns the result with the eval-cache
/// message.
fn passivate_with(structure: AtomicStructure, data: PassivateData) -> (NetworkResult, String) {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, structure);
    let id = add_passivate_node(&mut designer, network_name, DVec2::new(200.0, 0.0), data);
    designer.connect_nodes(value_id, 0, id, 0);

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement::root(network)];
    let result = evaluator.evaluate(&network_stack, id, 0, registry, false, &mut context);
    let message = context
        .selected_node_eval_cache
        .as_ref()
        .and_then(|cache| cache.downcast_ref::<PassivateEvalCache>())
        .map(|cache| cache.message.clone())
        .unwrap_or_default();
    (result, message)
}

fn hydrogens(result: &NetworkResult) -> usize {
    match result {
        NetworkResult::Molecule(m) => m
            .atoms
            .atoms_values()
            .filter(|a| a.atomic_number == 1)
            .count(),
        NetworkResult::Error(e) => panic!("unexpected error: {e}"),
        _ => panic!("expected a molecule"),
    }
}

#[test]
fn skip_clashes_leaves_a_clashing_valence_open() {
    let on = PassivateData {
        skip_clashes: true,
        ..PassivateData::default()
    };
    let (result, message) = passivate_with(carbon_facing_methane(), on);
    assert_eq!(hydrogens(&result), 4 + 3);
    assert_eq!(message, "Added 3 H atoms, skipped 1 (steric clash)");

    let (result, message) = passivate_with(carbon_facing_methane(), PassivateData::default());
    assert_eq!(hydrogens(&result), 4 + 4);
    assert_eq!(message, "Added 4 H atoms");
}

#[test]
fn a_non_positive_clash_factor_is_an_error_only_when_used() {
    for factor in [0.0, -0.5, f64::NAN] {
        let data = PassivateData {
            skip_clashes: true,
            clash_factor: factor,
            ..PassivateData::default()
        };
        match passivate_with(carbon_facing_methane(), data).0 {
            NetworkResult::Error(msg) => assert!(msg.contains("clash_factor"), "{msg}"),
            other => panic!("expected an error, got {:?}", other.infer_data_type()),
        }
        // With the option off nothing reads the factor.
        let data = PassivateData {
            skip_clashes: false,
            clash_factor: factor,
            ..PassivateData::default()
        };
        assert_eq!(
            hydrogens(&passivate_with(carbon_facing_methane(), data).0),
            8
        );
    }
}

#[test]
fn old_payloads_load_with_the_option_off() {
    for json in ["{}", "{\"element\":1}"] {
        let data: PassivateData = serde_json::from_str(json).unwrap();
        assert_eq!(data.element, 1);
        assert!(!data.skip_clashes);
        assert_eq!(data.clash_factor, DEFAULT_PASSIVATION_CLASH_FACTOR);
    }
    // And an untouched node writes exactly the old payload.
    assert_eq!(
        serde_json::to_string(&PassivateData::default()).unwrap(),
        "{\"element\":1}"
    );
}

fn author_and_serialize(source: &str) -> String {
    use atomcad_structure_designer::data_type::DataType;
    use atomcad_structure_designer::node_network::NodeNetwork;
    use atomcad_structure_designer::node_type::{NodeType, NodeTypeCategory, OutputPinDefinition};
    use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
    use atomcad_structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();
    let mut network = NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Molecule),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(atomcad_structure_designer::node_data::NoData {}),
        node_data_saver: atomcad_structure_designer::node_type::no_data_saver,
        node_data_loader: atomcad_structure_designer::node_type::no_data_loader,
    });
    let result = edit_network(&mut network, &registry, source, true);
    assert!(result.success, "edit should succeed: {:?}", result.errors);
    serialize_network(&network, &registry, Some("test"))
}

#[test]
fn the_clash_properties_round_trip_through_the_text_format() {
    const ON: &str = "p = passivate { element: 1, skip_clashes: true, clash_factor: 0.55 }";
    let serialized = author_and_serialize(&format!("{ON}\n"));
    assert!(serialized.contains(ON), "got:\n{serialized}");
    assert_eq!(serialized, author_and_serialize(&serialized));

    // Off: neither property is written.
    let serialized = author_and_serialize("p = passivate { element: 9 }\n");
    assert!(
        serialized.contains("p = passivate { element: 9 }"),
        "got:\n{serialized}"
    );
    assert!(!serialized.contains("skip_clashes") && !serialized.contains("clash_factor"));

    // Turning it on without a factor writes the default.
    let serialized = author_and_serialize("p = passivate { skip_clashes: true }\n");
    assert!(
        serialized.contains("p = passivate { element: 1, skip_clashes: true, clash_factor: 0.7 }"),
        "got:\n{serialized}"
    );
}

#[test]
fn the_clash_properties_round_trip_through_cnnd() {
    use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
    use atomcad_structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use std::collections::HashMap;
    use tempfile::tempdir;

    let save_and_load = |data: PassivateData| -> (String, PassivateData) {
        let network_name = "test";
        let mut designer = setup_designer_with_network(network_name);
        add_passivate_node(&mut designer, network_name, DVec2::ZERO, data);
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("passivate.cnnd");
        save_node_networks_to_file(
            &mut designer.node_type_registry,
            &path,
            false,
            &HashMap::new(),
        )
        .expect("save");
        let json = std::fs::read_to_string(&path).unwrap();
        let mut registry = NodeTypeRegistry::new();
        load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("load");
        let (_, node) = registry.node_networks[network_name]
            .nodes
            .iter()
            .find(|(_, n)| n.node_type_name == "passivate")
            .unwrap();
        let loaded = node
            .data
            .as_any_ref()
            .downcast_ref::<PassivateData>()
            .unwrap()
            .clone();
        (json, loaded)
    };

    let (json, loaded) = save_and_load(PassivateData {
        element: 1,
        skip_clashes: true,
        clash_factor: 0.55,
    });
    assert!(json.contains("skip_clashes") && json.contains("clash_factor"));
    assert!(loaded.skip_clashes);
    assert_eq!(loaded.clash_factor, 0.55);

    let (json, loaded) = save_and_load(PassivateData::default());
    assert!(!json.contains("skip_clashes") && !json.contains("clash_factor"));
    assert!(!loaded.skip_clashes);
    assert_eq!(loaded.clash_factor, DEFAULT_PASSIVATION_CLASH_FACTOR);
}
