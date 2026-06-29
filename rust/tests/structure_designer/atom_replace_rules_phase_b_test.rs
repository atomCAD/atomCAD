//! Phase B tests for `atom_replace` programmatic rules input. See
//! `doc/design_atom_replace_rules_input.md` §Phase B. Phase B adds the
//! optional `rules: Array[Record(Named("ElementMapping"))]` input pin to
//! `atom_replace`. When the pin is wired the rules supplied by upstream
//! entirely replace `AtomReplaceData.replacements`; when unwired the stored
//! list still drives eval (no behavioral change from pre-Phase-B).
//!
//! What we verify here:
//! - Pin signature: `atom_replace` now has two parameters (`molecule`,
//!   `rules`); `rules` is marked optional via `get_parameter_metadata`.
//! - Eval semantics: stored vs. wired vs. empty-wired vs. error paths.
//! - Validation: out-of-range targets and missing-field records become
//!   `NetworkResult::Error` at eval time.
//! - Subtitle: dropped when `rules` is connected.
//! - Custom-network passthrough: a Crystal-in / Crystal-out shape with a
//!   wired `rules` pin still preserves the Crystal variant.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::atom_replace::AtomReplaceData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let data = Box::new(ValueData { value });
    network.add_node("value", DVec2::ZERO, 0, data)
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

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
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

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    match evaluate(designer, network_name, node_id) {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        other => panic!("Expected Atomic result, got {:?}", other.infer_data_type()),
    }
}

/// Build a simple linear molecule: 2 C atoms + 1 O atom.
fn carbon_oxygen_structure() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    let o = s.add_atom(8, DVec3::new(3.0, 0.0, 0.0));
    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, o, BOND_SINGLE);
    s
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

fn rule_record(from: i32, to: i32) -> NetworkResult {
    NetworkResult::record(vec![
        ("from".to_string(), NetworkResult::Int(from)),
        ("to".to_string(), NetworkResult::Int(to)),
    ])
}

fn rules_array(rules: Vec<(i32, i32)>) -> NetworkResult {
    NetworkResult::Array(rules.into_iter().map(|(f, t)| rule_record(f, t)).collect())
}

// ============================================================================
// Pin signature
// ============================================================================

#[test]
fn atom_replace_has_rules_pin() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("atom_replace")
        .expect("atom_replace registered");
    // molecule, rules, region (region added by design_blueprint_region_atom_edits.md Phase A1)
    assert_eq!(nt.parameters.len(), 3);
    assert_eq!(nt.parameters[0].name, "molecule");
    assert_eq!(nt.parameters[1].name, "rules");
    let expected_rules = DataType::Array(Box::new(DataType::Record(RecordType::Named(
        "ElementMapping".to_string(),
    ))));
    assert_eq!(nt.parameters[1].data_type, expected_rules);
    assert_eq!(nt.parameters[2].name, "region");
    assert_eq!(nt.parameters[2].data_type, DataType::Blueprint);
}

#[test]
fn atom_replace_rules_pin_is_optional() {
    let data = AtomReplaceData::default();
    let metadata = data.get_parameter_metadata();
    assert_eq!(metadata.get("molecule"), Some(&(true, None)));
    assert_eq!(metadata.get("rules"), Some(&(false, None)));
}

// ============================================================================
// Eval semantics: stored vs. wired
// ============================================================================

/// Disconnected pin → stored `replacements` drive output (pre-Phase-B parity).
#[test]
fn atom_replace_disconnected_uses_stored() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    // Stored: C → Si.
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    designer.connect_nodes(value_id, 0, replace_id, 0);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    let mut elements: Vec<i16> = result.iter_atoms().map(|(_, a)| a.atomic_number).collect();
    elements.sort();
    assert_eq!(elements, vec![8, 14, 14], "C→Si applied from stored list");
}

/// Connected pin with a wired `Array[Record(ElementMapping)]` value → those
/// rules drive the output; stored `replacements` are not consulted.
#[test]
fn atom_replace_wired_overrides_stored() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    // Stored says C→Si but wired pin says O→S. Wired wins.
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14)]);
    let rules_id = add_value_node(&mut designer, net, rules_array(vec![(8, 16)]));
    designer.connect_nodes(mol_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    let mut elements: Vec<i16> = result.iter_atoms().map(|(_, a)| a.atomic_number).collect();
    elements.sort();
    // Carbons untouched (stored rule ignored), oxygen → sulfur.
    assert_eq!(elements, vec![6, 6, 16]);
}

/// Wired empty array → input passes through unchanged regardless of stored
/// replacements.
#[test]
fn atom_replace_wired_empty_passes_through() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    // Stored would have replaced everything; wired empty array overrides.
    let replace_id = add_replace_node(&mut designer, net, vec![(6, 14), (8, 16)]);
    let rules_id = add_value_node(&mut designer, net, NetworkResult::Array(vec![]));
    designer.connect_nodes(mol_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    let mut elements: Vec<i16> = result.iter_atoms().map(|(_, a)| a.atomic_number).collect();
    elements.sort();
    assert_eq!(elements, vec![6, 6, 8], "Empty wired rules → no change");
}

/// Wired rule with `to == 0` → matching atoms deleted; bonds cleaned up.
#[test]
fn atom_replace_wired_delete_target() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![]);
    // Wire: O → delete (atomic_number 0).
    let rules_id = add_value_node(&mut designer, net, rules_array(vec![(8, 0)]));
    designer.connect_nodes(mol_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    let result = evaluate_to_atomic(&designer, net, replace_id);
    assert_eq!(result.atom_ids().count(), 2, "Oxygen should be deleted");
    assert_eq!(
        result.get_num_of_bonds(),
        1,
        "C-O bond removed, C-C bond remains"
    );
    for (_, a) in result.iter_atoms() {
        assert_eq!(a.atomic_number, 6);
    }
}

// ============================================================================
// Validation: error paths
// ============================================================================

/// Wired rule with target outside `0..=118` → eval-time error.
#[test]
fn atom_replace_out_of_range_target_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![]);
    let rules_id = add_value_node(&mut designer, net, rules_array(vec![(6, 999)]));
    designer.connect_nodes(mol_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    let result = evaluate(&designer, net, replace_id);
    let NetworkResult::Error(msg) = result else {
        panic!("expected Error, got {:?}", result.infer_data_type());
    };
    assert!(
        msg.contains("out of range") && msg.contains("999"),
        "error message should mention out-of-range value: {}",
        msg
    );
}

/// Wired record missing the `from` field at runtime → eval-time error. The
/// type system enforces field presence at pin connect time, so this branch
/// is the defensive fallback for runtime values constructed via paths that
/// bypass static checking (e.g. raw value nodes in tests).
#[test]
fn atom_replace_missing_from_field_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let mol_id = add_value_node(
        &mut designer,
        net,
        molecule_value(carbon_oxygen_structure()),
    );
    let replace_id = add_replace_node(&mut designer, net, vec![]);
    // Hand-build a record that omits `from`.
    let bad_record = NetworkResult::record(vec![("to".to_string(), NetworkResult::Int(14))]);
    let rules_id = add_value_node(&mut designer, net, NetworkResult::Array(vec![bad_record]));
    designer.connect_nodes(mol_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    let result = evaluate(&designer, net, replace_id);
    let NetworkResult::Error(msg) = result else {
        panic!("expected Error, got {:?}", result.infer_data_type());
    };
    assert!(
        msg.contains("'from'") || msg.contains("from"),
        "error message should mention missing 'from' field: {}",
        msg
    );
}

// ============================================================================
// Crystal-variant preservation (custom-network passthrough)
// ============================================================================

/// Crystal in → Crystal out. The `SameAsInput("molecule")` output pin
/// preserves the concrete variant even when the rules come from the wired
/// pin.
#[test]
fn atom_replace_wired_preserves_crystal_variant() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let crystal = NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: carbon_oxygen_structure(),
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    });
    let mol_id = add_value_node(&mut designer, net, crystal);
    let replace_id = add_replace_node(&mut designer, net, vec![]);
    let rules_id = add_value_node(&mut designer, net, rules_array(vec![(6, 14)]));
    designer.connect_nodes(mol_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    let result = evaluate(&designer, net, replace_id);
    match result {
        NetworkResult::Crystal(c) => {
            let mut elements: Vec<i16> =
                c.atoms.iter_atoms().map(|(_, a)| a.atomic_number).collect();
            elements.sort();
            assert_eq!(elements, vec![8, 14, 14]);
        }
        other => panic!(
            "Expected Crystal variant, got {:?}",
            other.infer_data_type()
        ),
    }
}

// ============================================================================
// Subtitle behavior
// ============================================================================

/// When the `rules` pin is in the connected set, `get_subtitle` returns None
/// (project convention — wired-pin overrides drop the property summary).
#[test]
fn atom_replace_subtitle_dropped_when_rules_connected() {
    let mut connected = std::collections::HashSet::new();
    connected.insert("rules".to_string());

    let data = AtomReplaceData {
        replacements: vec![(6, 14), (8, 16)],
    };
    assert_eq!(data.get_subtitle(&connected), None);

    // Sanity: with `rules` not connected, subtitle is the rule summary.
    let no_pins = std::collections::HashSet::new();
    assert_eq!(data.get_subtitle(&no_pins), Some("C→Si, O→S".to_string()));
}
