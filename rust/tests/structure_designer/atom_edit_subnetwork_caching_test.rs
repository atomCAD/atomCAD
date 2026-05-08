//! Regression test for the atom_edit input-cache cross-talk bug.
//!
//! `AtomEditData::cached_input` was read by `eval` as a perf shortcut. That
//! shortcut is unsound when the atom_edit lives inside a subnetwork:
//! - `refresh_partial`'s dependency walker only invalidates caches in the
//!   *active* network, never reaching nodes nested inside subnetworks.
//! - Subnetwork bodies are stored once in the registry, so a single
//!   `cached_input` is shared across every call site of that subnetwork —
//!   there is no correct value to cache.
//!
//! The test exercises the second failure mode (cross-call-site cross-talk)
//! because it triggers in a single pass with no mutation between evaluations,
//! so the failure is unambiguously about the cache and not about
//! invalidation timing.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

/// Configure a parameter node's `data_type` and `param_name`. Goes through
/// `set_node_network_data` rather than mutating `ParameterData` in place,
/// because the parameter's cached `custom_node_type` (which drives its
/// output pin type) is only refreshed via that path. Mirrors the production
/// `set_parameter_data` API call.
fn configure_parameter(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    name: &str,
    data_type: DataType,
) {
    designer.set_active_node_network_name(Some(network_name.to_string()));
    let existing_param_id = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&node_id))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<ParameterData>())
        .and_then(|p| p.param_id);
    let new_data = Box::new(ParameterData {
        param_id: existing_param_id,
        param_index: 0,
        param_name: name.to_string(),
        data_type,
        sort_order: 0,
        data_type_str: None,
        error: None,
    });
    designer.set_node_network_data(node_id, new_data);
}

/// Add a `value` node holding a `Molecule` payload. Used to feed distinct
/// atomic structures into two call sites of the same subnetwork.
fn add_molecule_value_node(
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
        value: NetworkResult::Molecule(MoleculeData {
            atoms: structure,
            geo_tree_root: None,
        }),
    });
    network.add_node("value", DVec2::ZERO, 0, value_data)
}

/// Evaluate a single node's pin 0 with a fresh evaluator+context. The
/// `cached_input` field that this test targets lives on `AtomEditData`
/// (i.e. the registry-stored Node), so it persists across separate
/// `evaluator`/`context` instances.
fn evaluate_pin0(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

fn atomic_numbers(result: &NetworkResult) -> Vec<i16> {
    let atoms = match result {
        NetworkResult::Molecule(m) => &m.atoms,
        NetworkResult::Crystal(c) => &c.atoms,
        other => panic!(
            "expected Molecule/Crystal, got {}",
            other.to_display_string()
        ),
    };
    atoms.atoms_values().map(|a| a.atomic_number).collect()
}

/// Two call sites of the same subnetwork (containing an `atom_edit`) feeding
/// distinct molecules must produce distinct outputs in a single evaluation
/// pass. Pre-fix: both sites collapsed to whichever ran first, because the
/// atom_edit's `cached_input` was registry-shared and read by `eval`.
#[test]
fn atom_edit_in_subnetwork_does_not_leak_input_across_call_sites() {
    let mut designer = StructureDesigner::new();

    // ----- Subnetwork "Inner": parameter -> atom_edit -> return ----------
    designer.add_node_network("Inner");
    designer.set_active_node_network_name(Some("Inner".to_string()));

    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    configure_parameter(&mut designer, "Inner", param_id, "mol", DataType::Molecule);

    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(100.0, 0.0));
    // Wire parameter (output pin 0) -> atom_edit (input pin 0 = `molecule`).
    designer.connect_nodes(param_id, 0, atom_edit_id, 0);
    designer.set_return_node_id(Some(atom_edit_id));
    designer.validate_active_network();

    // ----- Outer "main": two value nodes, two Inner call sites -----------
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Two distinguishable molecules: a single C atom vs. a single O atom.
    let mut mol_a = AtomicStructure::new();
    mol_a.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let mut mol_b = AtomicStructure::new();
    mol_b.add_atom(8, DVec3::new(0.0, 0.0, 0.0));

    let value_a_id = add_molecule_value_node(&mut designer, "main", mol_a);
    let value_b_id = add_molecule_value_node(&mut designer, "main", mol_b);

    let call_a_id = designer.add_node("Inner", DVec2::new(200.0, 0.0));
    let call_b_id = designer.add_node("Inner", DVec2::new(200.0, 200.0));

    designer.connect_nodes(value_a_id, 0, call_a_id, 0);
    designer.connect_nodes(value_b_id, 0, call_b_id, 0);
    // Note: we do not validate `main`. The `value` node declares output
    // `DataType::None`, which the validator rejects when wiring into a typed
    // input pin, but the runtime evaluator flows the concrete payload through
    // unchanged (same trick `atom_composediff_test.rs` uses). Inner *must*
    // stay valid because the evaluator short-circuits on invalid subnetworks.
    let inner = designer
        .node_type_registry
        .node_networks
        .get("Inner")
        .unwrap();
    assert!(
        inner.valid,
        "Inner subnetwork must be valid: {:?}",
        inner
            .validation_errors
            .iter()
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );

    // ----- Evaluate both call sites; each must reflect its own input -----
    let result_a = evaluate_pin0(&designer, "main", call_a_id);
    let result_b = evaluate_pin0(&designer, "main", call_b_id);

    assert_eq!(
        atomic_numbers(&result_a),
        vec![6],
        "call_a should return its own carbon atom"
    );
    assert_eq!(
        atomic_numbers(&result_b),
        vec![8],
        "call_b should return its own oxygen atom — pre-fix this leaked the carbon \
         from call_a via atom_edit's registry-shared cached_input"
    );
}
