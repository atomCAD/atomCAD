//! Phase 6 of `doc/design_error_management.md` (D9 + the chain-hygiene
//! violations) — three related guarantees:
//!
//! 1. **Inner-cause survival across array inputs.** A consumer that unwraps an
//!    `Array` input and dispatches per element used to replace a failing
//!    element's error with its own "all inputs must be X" prose, destroying the
//!    root cause. The shared scanner
//!    `network_result::first_array_element_error` now forwards the element's
//!    own text (naming the index) before any type dispatch.
//! 2. **The third error channel is folded in.** `motif` / `motif_sub` /
//!    `materialize` keep a parse failure of their stored definition string on
//!    their node data. It used to reach the user only as a node badge —
//!    invisible to the panel list and the F8 cycle. `NodeData::get_data_error`
//!    now surfaces it as a validation error on every validate pass.
//! 3. **The severity sweep.** Rules that were demoted to `warning` only
//!    because the runtime already localized the failure are **blocking** now
//!    that blocking costs a cone rather than the network — so D8's dedupe shows
//!    one entry where an amber+red pair used to appear for one fact.

use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIErrorSource;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    NetworkResult, first_array_element_error,
};
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::materialize::MaterializeData;
use rust_lib_flutter_cad::structure_designer::nodes::motif::MotifData;
use rust_lib_flutter_cad::structure_designer::nodes::motif_sub::MotifSubData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::cell::RefCell;
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

fn full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn validate_and_refresh(designer: &mut StructureDesigner) {
    designer.validate_active_network();
    full_refresh(designer);
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn error_text(result: NetworkResult) -> String {
    match result {
        NetworkResult::Error(msg) => msg,
        other => panic!("expected an Error, got {}", other.to_display_string()),
    }
}

/// Validation errors of the active network attributed to `node_id`.
fn node_validation_errors(designer: &StructureDesigner, node_id: u64) -> Vec<(String, bool)> {
    designer
        .get_active_node_network()
        .unwrap()
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(node_id))
        .map(|e| (e.error_text.clone(), e.blocking))
        .collect()
}

// ============================================================================
// 1. Inner-cause survival across array inputs
// ============================================================================

/// The shared scanner: the first failing element wins, its own text is kept
/// verbatim inside the wrap, and the index is named. A clean array yields
/// `None` so the caller's normal path is untouched.
#[test]
fn first_array_element_error_keeps_inner_text_and_index() {
    let clean = [NetworkResult::Int(1), NetworkResult::Int(2)];
    assert!(first_array_element_error("shapes", &clean).is_none());

    let dirty = [
        NetworkResult::Int(1),
        NetworkResult::Error("root cause here".to_string()),
        NetworkResult::Error("a later, unreported failure".to_string()),
    ];
    let text = error_text(first_array_element_error("shapes", &dirty).expect("an element failed"));
    assert!(
        text.contains("root cause here"),
        "the element's own text must survive: {}",
        text
    );
    assert!(text.contains("shapes"), "the input pin is named: {}", text);
    assert!(text.contains("element 1"), "the index is named: {}", text);
    assert!(
        !text.contains("a later"),
        "only the first failing element is reported: {}",
        text
    );
}

/// End-to-end: a failing `union` feeds a `sequence`, whose array feeds an outer
/// `union`. Before Phase 6 the outer node reported "All inputs must be geometry
/// objects" — a type complaint that is simply false and hides the real problem.
#[test]
fn union_forwards_a_failing_array_element_instead_of_a_type_complaint() {
    let mut designer = setup_designer_with_network("main");

    // A `union` with no `shapes` wire fails at runtime with a clean localized
    // "shapes input is missing" — and its output type is a *fixed* Blueprint,
    // so no blocking validation rule fires and the node really is evaluated.
    let failing_id = designer.add_node("union", DVec2::new(0.0, 0.0));

    let seq_id = designer.add_node("sequence", DVec2::new(200.0, 0.0));
    designer.set_node_network_data_scoped(
        &[],
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::Blueprint,
            input_count: 1,
        }),
    );
    designer.connect_nodes(failing_id, 0, seq_id, 0);

    let outer_id = designer.add_node("union", DVec2::new(400.0, 0.0));
    designer.connect_nodes(seq_id, 0, outer_id, 0);

    designer.validate_active_network();

    let inner = error_text(evaluate_node(&designer, "main", failing_id));
    let outer = error_text(evaluate_node(&designer, "main", outer_id));
    assert!(
        outer.contains(&inner),
        "the outer union must carry the inner cause `{}`; got `{}`",
        inner,
        outer
    );
    assert!(
        !outer.contains("All inputs must be"),
        "the ad-hoc type complaint must not replace the root cause: {}",
        outer
    );
}

// ============================================================================
// 2. The third error channel — stored-data parse errors
// ============================================================================

/// `motif_sub`'s parse error becomes a **non-blocking** validation entry: its
/// `eval` ignores `self.error` and still emits a usable motif, so the node's
/// output stays useful.
#[test]
fn motif_sub_parse_error_is_a_non_blocking_validation_entry() {
    let mut designer = setup_designer_with_network("main");
    let id = designer.add_node("motif_sub", DVec2::ZERO);
    let mut data = MotifSubData {
        parameter_element_value_definition: "PRIMARY C EXTRA TOKENS".to_string(),
        error: None,
        parameter_element_values: HashMap::new(),
        available_parameters: RefCell::new(Vec::new()),
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], id, Box::new(data));

    designer.validate_active_network();

    let errors = node_validation_errors(&designer, id);
    assert_eq!(errors.len(), 1, "exactly one entry; got {:?}", errors);
    assert!(errors[0].0.contains("Parameter element parse error"));
    assert!(!errors[0].1, "motif_sub's parse error is advisory");
}

/// Same for `materialize` (the other definition-string node whose `eval`
/// no-ops on unparsed data).
#[test]
fn materialize_parse_error_is_a_non_blocking_validation_entry() {
    let mut designer = setup_designer_with_network("main");
    let id = designer.add_node("materialize", DVec2::ZERO);
    let mut data = MaterializeData {
        parameter_element_value_definition: "PRIMARY C EXTRA TOKENS".to_string(),
        hydrogen_passivation: false,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        invert_phase: false,
        passivation_element: 1,
        error: None,
        parameter_element_values: HashMap::new(),
        available_parameters: RefCell::new(Vec::new()),
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], id, Box::new(data));

    designer.validate_active_network();

    let errors = node_validation_errors(&designer, id);
    assert_eq!(errors.len(), 1, "exactly one entry; got {:?}", errors);
    assert!(errors[0].0.contains("Parameter element parse error"));
    assert!(!errors[0].1, "materialize's parse error is advisory");
}

/// `motif` is the one of the three whose `eval` *does* return the parse error
/// as its output — the node has no motif to emit — so by the new litmus ("is
/// the node's output still useful?") its entry is **blocking**. The payoff is
/// D8's dedupe: the badge/panel show the sentence exactly once rather than
/// twice (validation row + identical eval row).
#[test]
fn motif_parse_error_is_blocking_and_shows_exactly_one_entry() {
    let mut designer = setup_designer_with_network("main");
    let id = designer.add_node("motif", DVec2::ZERO);
    let mut data = MotifData {
        definition: "SITE".to_string(),
        name: None,
        error: None,
        motif: None,
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], id, Box::new(data));
    designer.set_node_display(id, true);

    validate_and_refresh(&mut designer);

    let errors = node_validation_errors(&designer, id);
    assert_eq!(errors.len(), 1, "exactly one entry; got {:?}", errors);
    assert!(errors[0].0.contains("Motif parse error"));
    assert!(errors[0].1, "motif's parse error blocks (no motif to emit)");

    // Cone-scoped: the network stays usable.
    assert!(designer.get_active_node_network().unwrap().valid);

    // Exactly one row on the unified panel list — the eval entry is the
    // deduped synthesized vehicle, not a second sentence.
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let rows: Vec<_> = networks
        .iter()
        .find(|n| n.name == "main")
        .expect("main is listed")
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(id))
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "one fact, one row; got {:?}",
        rows.iter()
            .map(|e| (&e.error_text, e.source))
            .collect::<Vec<_>>()
    );
    assert_eq!(rows[0].source, APIErrorSource::Validation);
}

// ============================================================================
// 3. The severity sweep
// ============================================================================

/// The sweep's headline case: an independent HOF whose zone-output pin has no
/// incoming wire now carries a **blocking** entry, its `eval` is skipped
/// (skip-and-synthesize), the downstream chain carries that same text, and the
/// panel shows exactly one entry for the node — where it previously showed an
/// amber validation row plus a red eval row for the same fact.
#[test]
fn unwired_zone_output_is_blocking_and_deduped_and_chains_downstream() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    designer.set_node_network_data_scoped(
        &[],
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    designer.set_node_network_data_scoped(
        &[],
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);

    // A downstream consumer of the poisoned map.
    let collect_id = designer.add_node("collect", DVec2::new(400.0, 0.0));
    designer.connect_nodes(map_id, 0, collect_id, 0);

    designer.set_node_display(map_id, true);
    designer.set_node_display(collect_id, true);
    validate_and_refresh(&mut designer);

    let errors = node_validation_errors(&designer, map_id);
    assert_eq!(errors.len(), 1, "one entry on the map; got {:?}", errors);
    let (text, blocking) = &errors[0];
    assert!(text.contains("Zone-output pin"), "got: {}", text);
    assert!(blocking, "the sweep promoted this rule to blocking");

    // Cone-scoped: the network stays usable and the independent `range` still
    // evaluates.
    assert!(designer.get_active_node_network().unwrap().valid);
    assert!(matches!(
        evaluate_node(&designer, "main", range_id),
        NetworkResult::Iterator(_)
    ));

    // Skip-and-synthesize: the map's output *is* the validation text (its
    // `eval`, which would have produced `build_inline_closure`'s own message,
    // was never entered), and the downstream consumer chains it.
    assert_eq!(error_text(evaluate_node(&designer, "main", map_id)), *text);
    let downstream = error_text(evaluate_node(&designer, "main", collect_id));
    assert!(
        downstream.contains(text.as_str()),
        "the downstream chain must carry the root text `{}`; got `{}`",
        text,
        downstream
    );

    // D8 dedupe: one fact, one panel row (previously amber + red).
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let rows: Vec<_> = networks
        .iter()
        .find(|n| n.name == "main")
        .expect("main is listed")
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(map_id))
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "one fact, one row; got {:?}",
        rows.iter()
            .map(|e| (&e.error_text, e.source))
            .collect::<Vec<_>>()
    );
    assert_eq!(rows[0].source, APIErrorSource::Validation);
}

/// An `apply` with its required `f` pin unwired is the sweep's second member:
/// blocking, cone-scoped, and it does not flip the network invalid.
#[test]
fn apply_without_f_is_blocking_but_cone_scoped() {
    let mut designer = setup_designer_with_network("main");
    let apply_id = designer.add_node("apply", DVec2::ZERO);

    designer.validate_active_network();

    let errors = node_validation_errors(&designer, apply_id);
    let entry = errors
        .iter()
        .find(|(text, _)| text.contains("`f`"))
        .unwrap_or_else(|| panic!("expected the f-pin rule; got {:?}", errors));
    assert!(
        entry.1,
        "the sweep promoted the apply f-pin rule to blocking"
    );
    assert!(
        designer.get_active_node_network().unwrap().valid,
        "node-attributed blocking must not flip `valid`"
    );
}
