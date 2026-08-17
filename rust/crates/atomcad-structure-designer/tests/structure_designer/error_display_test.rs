//! Phase 1 of `doc/design_error_management.md` (D8) — the eval-error
//! suppression gate in node badge assembly.
//!
//! Before this phase, `build_node_view` appended a node's evaluation error
//! only when the **whole network's** `validation_errors` list was empty — one
//! validation error anywhere (even a non-blocking warning on an unrelated
//! node) hid every runtime-error badge in the network. The fix makes the
//! suppression per-node and blocking-only: a node's eval error is dropped iff
//! *that node* carries a **blocking** validation error (where the eval entry
//! would merely repeat the same fact).
//!
//! The badge-assembly logic is the pure helper
//! `node_network::collect_node_error_messages` (`build_node_view` itself is
//! unreachable from tests — it needs a `CADInstance`, which owns a
//! GPU-initialised `Renderer`). The unit tests pin the gate matrix on the
//! helper directly; the integration tests drive a real `StructureDesigner`
//! through validate + refresh and feed the resulting live state
//! (`validation_errors` + the scene's `get_node_error`) through the same
//! helper the production code calls.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::{ValidationError, collect_node_error_messages};
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::map::MapData;
use atomcad_structure_designer::nodes::motif_sub::MotifSubData;
use atomcad_structure_designer::nodes::range::RangeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
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

/// Add a top-level `expr` node with one unwired `Int` parameter — the
/// canonical "clean runtime error" node: its output type is `Fixed` (no
/// blocking polymorphic-resolution rule fires) and evaluating it with the
/// parameter unwired yields a localized `input_missing_error`.
fn add_expr_with_unwired_param(designer: &mut StructureDesigner) -> u64 {
    let expr_id = designer.add_node_scoped(&[], "expr", DVec2::new(0.0, 200.0), None);
    let mut data = ExprData {
        parameters: vec![ExprParameter {
            id: None,
            name: "x".to_string(),
            data_type: DataType::Int,
            data_type_str: None,
        }],
        expression: "x + 1".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], expr_id, Box::new(data));
    expr_id
}

/// Add a `motif_sub` whose stored parameter-element definition does not parse.
/// Since Phase 6 this is the canonical *non-blocking* validation error
/// ("Parameter element parse error: …", attributed to the node,
/// `blocking == false`): `motif_sub::eval` ignores `self.error` entirely and
/// still emits a usable motif, so the node's output stays useful. Its required
/// `motif` input is left unwired, so the node *also* fails at runtime for an
/// independent reason — the warning + eval-error pair.
///
/// (The former fixture here — a `map` with an empty inline body — became
/// *blocking* in the D9 severity sweep, so it no longer exercises the
/// warning arm of the gate.)
fn add_motif_sub_with_bad_definition(designer: &mut StructureDesigner) -> u64 {
    let id = designer.add_node("motif_sub", DVec2::new(0.0, 0.0));
    let mut data = MotifSubData {
        parameter_element_value_definition: "PRIMARY C EXTRA TOKENS".to_string(),
        error: None,
        parameter_element_values: HashMap::new(),
        available_parameters: RefCell::new(Vec::new()),
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], id, Box::new(data));
    id
}

/// Add `range -> map` where the map's inline body is left empty — its
/// `result` zone-output pin has no incoming wire, a **blocking** validation
/// error on the map node since the D9 severity sweep.
#[allow(dead_code)]
fn add_map_with_empty_body(designer: &mut StructureDesigner) -> u64 {
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
    map_id
}

fn validation_errors_of(designer: &StructureDesigner) -> Vec<ValidationError> {
    designer
        .get_active_node_network()
        .unwrap()
        .validation_errors
        .clone()
}

// ============================================================================
// Unit tests — the gate matrix on the pure helper
// ============================================================================

#[test]
fn no_errors_yields_no_messages() {
    assert!(collect_node_error_messages(&[], 1, None).is_empty());
}

#[test]
fn eval_error_alone_is_shown() {
    let messages = collect_node_error_messages(&[], 1, Some("boom".to_string()));
    assert_eq!(messages, vec!["boom".to_string()]);
}

/// The Phase-1 headline: a validation error on *another* node no longer
/// suppresses this node's eval error (the old gate keyed on the whole
/// network's `validation_errors.is_empty()`).
#[test]
fn validation_error_on_other_node_does_not_suppress_eval_error() {
    let errors = [
        ValidationError::warning("warning on node 1".to_string(), Some(1)),
        ValidationError::new("blocking on node 2".to_string(), Some(2)),
        ValidationError::new("network-level".to_string(), None),
    ];
    let messages = collect_node_error_messages(&errors, 3, Some("runtime boom".to_string()));
    assert_eq!(messages, vec!["runtime boom".to_string()]);
}

/// A *warning* on the node itself never suppresses the eval error — warnings
/// evaluate, and the runtime failure can be a different, more specific fact.
#[test]
fn own_warning_and_eval_error_are_both_shown() {
    let errors = [ValidationError::warning("advisory".to_string(), Some(1))];
    let messages = collect_node_error_messages(&errors, 1, Some("runtime boom".to_string()));
    assert_eq!(
        messages,
        vec!["advisory".to_string(), "runtime boom".to_string()]
    );
}

/// A *blocking* validation error on the node suppresses the eval entry — a
/// predicate check, not a text comparison (the texts here are unrelated).
#[test]
fn own_blocking_error_suppresses_eval_error() {
    let errors = [ValidationError::new("type mismatch".to_string(), Some(1))];
    let messages = collect_node_error_messages(&errors, 1, Some("runtime boom".to_string()));
    assert_eq!(messages, vec!["type mismatch".to_string()]);
}

/// Multiple validation entries on one node all surface, and one blocking
/// entry among them is enough to drop the eval entry.
#[test]
fn mixed_entries_accumulate_and_one_blocking_suppresses() {
    let errors = [
        ValidationError::warning("advisory".to_string(), Some(1)),
        ValidationError::new("hard error".to_string(), Some(1)),
        ValidationError::warning("other node".to_string(), Some(2)),
    ];
    let messages = collect_node_error_messages(&errors, 1, Some("runtime boom".to_string()));
    assert_eq!(
        messages,
        vec!["advisory".to_string(), "hard error".to_string()]
    );
}

/// An unattributed (`node_id == None`) error never lands on any node's badge
/// and never suppresses anything.
#[test]
fn unattributed_error_neither_shows_nor_suppresses() {
    let errors = [ValidationError::new("network-level".to_string(), None)];
    let messages = collect_node_error_messages(&errors, 1, Some("runtime boom".to_string()));
    assert_eq!(messages, vec!["runtime boom".to_string()]);
}

// ============================================================================
// Integration tests — real validate + refresh state through the same helper
// ============================================================================

/// Design-doc Phase 1 test 1: node A (a `motif_sub` with an unparseable
/// definition) carries a validation *warning*, node B (an `expr` with an
/// unwired parameter) fails at runtime → B's badge shows its runtime error.
/// Under the old network-wide gate the warning anywhere in the network hid
/// B's eval error.
#[test]
fn runtime_error_badge_survives_warning_elsewhere_in_network() {
    let mut designer = setup_designer_with_network("main");

    let warned_id = add_motif_sub_with_bad_definition(&mut designer);
    let expr_id = add_expr_with_unwired_param(&mut designer);

    // Only the runtime-error node is displayed (and therefore evaluated).
    designer.set_node_display(warned_id, false);
    designer.set_node_display(expr_id, true);

    designer.validate_active_network();

    let errors = validation_errors_of(&designer);
    let warning = errors
        .iter()
        .find(|e| e.node_id == Some(warned_id))
        .expect("an unparseable definition must produce a validation error on the node");
    assert!(
        !warning.blocking,
        "the motif_sub parse-error rule must be non-blocking; got: {}",
        warning.error_text
    );
    assert!(
        designer.get_active_node_network().unwrap().valid,
        "a warning must not flip the network invalid"
    );

    full_refresh(&mut designer);

    let eval_error = designer
        .last_generated_structure_designer_scene
        .get_node_error(&[], expr_id);
    assert!(
        eval_error.is_some(),
        "the displayed expr with an unwired parameter must record a runtime error"
    );

    // The gate under test: with a non-empty `validation_errors` list, the old
    // whole-network check suppressed this badge; the per-node check keeps it.
    let messages = collect_node_error_messages(&errors, expr_id, eval_error.clone());
    assert_eq!(
        messages,
        vec![eval_error.unwrap()],
        "expr's badge must show exactly its runtime error"
    );
}

/// Design-doc Phase 1 test 2: one node with a warning *and* its own runtime
/// error shows both messages (the warning must not mask the more specific
/// runtime failure).
#[test]
fn warning_and_own_runtime_error_are_both_shown() {
    let mut designer = setup_designer_with_network("main");

    // Displayed → evaluated → the required `motif` input is unwired, so the
    // node fails at runtime for a reason *independent* of the advisory parse
    // warning. That independence is the point: the warning must not mask the
    // more specific runtime failure.
    let node_id = add_motif_sub_with_bad_definition(&mut designer);
    designer.set_node_display(node_id, true);

    designer.validate_active_network();
    full_refresh(&mut designer);

    let errors = validation_errors_of(&designer);
    let warning_text = errors
        .iter()
        .find(|e| e.node_id == Some(node_id) && !e.blocking)
        .expect("motif_sub must carry its non-blocking parse error")
        .error_text
        .clone();
    let eval_error = designer
        .last_generated_structure_designer_scene
        .get_node_error(&[], node_id)
        .expect("the displayed motif_sub with an unwired motif input must record a runtime error");

    let messages = collect_node_error_messages(&errors, node_id, Some(eval_error.clone()));
    assert_eq!(
        messages,
        vec![warning_text, eval_error],
        "warning and runtime error must both appear, warning first"
    );
}

/// Design-doc Phase 1 test 3: a node with a *blocking* validation error shows
/// only the validation text. Since Phase 3 (cone-scoped blocking) the network
/// stays `valid` and the poisoned node carries a *synthesized* eval entry
/// (the skip-and-synthesize propagation vehicle, equal to the validation
/// text); the D8 dedupe drops that entry from the badge, so the displayed
/// message list is still exactly the validation text.
#[test]
fn blocking_error_shows_only_validation_text() {
    let mut designer = setup_designer_with_network("main");

    // A bare `relax`: its `SameAsInput` output pin cannot resolve with the
    // input unwired → blocking validation error attributed to the node.
    let relax_id = designer.add_node("relax", DVec2::new(0.0, 0.0));
    designer.set_node_display(relax_id, true);

    designer.validate_active_network();

    let errors = validation_errors_of(&designer);
    let blocking = errors
        .iter()
        .find(|e| e.node_id == Some(relax_id))
        .expect("bare relax must produce a validation error on the relax node");
    assert!(
        blocking.blocking,
        "the unresolved-output rule is blocking at this phase; got: {}",
        blocking.error_text
    );
    assert!(
        designer.get_active_node_network().unwrap().valid,
        "cone-scoped blocking: a node-attributed error must not flip `valid`"
    );

    full_refresh(&mut designer);

    let eval_error = designer
        .last_generated_structure_designer_scene
        .get_node_error(&[], relax_id);
    assert_eq!(
        eval_error.as_deref(),
        Some(blocking.error_text.as_str()),
        "the poisoned node carries the synthesized eval entry (its validation text)"
    );

    let messages = collect_node_error_messages(&errors, relax_id, eval_error);
    assert_eq!(
        messages,
        vec![blocking.error_text.clone()],
        "only the validation text appears, no duplicated sentence"
    );
}
