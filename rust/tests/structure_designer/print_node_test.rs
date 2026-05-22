//! Phase 4 tests for the node-execution design (`doc/design_node_execution.md`).
//!
//! Phase 4 lands the `print` node + Console panel plumbing. The `print` node
//! returns `String` (passthrough) and as a side effect appends an entry to
//! `context.print_buffer`, which the orchestrator drains into
//! `StructureDesigner.print_log`. A bool `execute_only` flag gates whether the
//! side effect fires only under `context.execute == true` (Execute pass) or on
//! every evaluation including normal display passes.
//!
//! Because `print` returns `String` — not `Unit` — the central skip rule
//! does **not** apply to it: `eval` runs on every pass that reaches this node.
//! These tests exercise both gating modes and the passthrough output.

use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::print::PrintData;
use rust_lib_flutter_cad::structure_designer::nodes::string::StringData;
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

/// Build a `string` node holding `text`, return its node id.
fn add_string_literal(
    designer: &mut StructureDesigner,
    network_name: &str,
    pos: DVec2,
    text: &str,
) -> u64 {
    let id = designer.add_node("string", pos);
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let data = network
        .get_node_network_data_mut(id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<StringData>()
        .unwrap();
    data.value = text.to_string();
    id
}

fn set_print_execute_only(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    execute_only: bool,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let data = network
        .get_node_network_data_mut(node_id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<PrintData>()
        .unwrap();
    data.execute_only = execute_only;
}

/// Run a single eval pass on a node, returning the result and the print
/// entries pushed during that pass. Mirrors the `with_eval_context` pattern
/// without going through `StructureDesigner` so tests can assert on the
/// per-pass buffer slice directly.
fn evaluate_and_capture_prints(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    execute: bool,
) -> (NetworkResult, Vec<String>) {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.execute = execute;
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context);
    let texts = context
        .print_buffer
        .iter()
        .map(|e| e.text.clone())
        .collect();
    (result, texts)
}

// ============================================================================
// Registration & defaults
// ============================================================================

#[test]
fn print_default_execute_only_is_false() {
    let data = PrintData::default();
    assert!(!data.execute_only);
}

#[test]
fn print_is_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("print")
        .expect("print should be registered");
    assert_eq!(nt.name, "print");
    assert!(nt.public);
    assert_eq!(nt.category, NodeTypeCategory::MathAndProgramming);
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "text");
    assert_eq!(nt.parameters[0].data_type, DataType::String);
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::String);
}

// ============================================================================
// execute_only=false: fires on every pass (display + execute)
// ============================================================================

#[test]
fn print_with_execute_only_false_appends_on_display_pass() {
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "hello");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);
    set_print_execute_only(&mut designer, "main", p_id, false);

    let (result, texts) = evaluate_and_capture_prints(&designer, "main", p_id, false);
    assert!(
        matches!(&result, NetworkResult::String(s) if s == "hello"),
        "passthrough should preserve the input string (got {})",
        result.to_display_string()
    );
    assert_eq!(
        texts,
        vec!["hello".to_string()],
        "execute_only=false should fire on a display pass"
    );
}

#[test]
fn print_with_execute_only_false_appends_on_execute_pass() {
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "hi");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);
    set_print_execute_only(&mut designer, "main", p_id, false);

    let (result, texts) = evaluate_and_capture_prints(&designer, "main", p_id, true);
    assert!(matches!(&result, NetworkResult::String(s) if s == "hi"));
    assert_eq!(texts, vec!["hi".to_string()]);
}

// ============================================================================
// execute_only=true: gated to execute passes only
// ============================================================================

#[test]
fn print_with_execute_only_true_skips_display_pass_buffer_push() {
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "secret");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);
    set_print_execute_only(&mut designer, "main", p_id, true);

    let (result, texts) = evaluate_and_capture_prints(&designer, "main", p_id, false);
    // Output pin is String, not Unit — central skip rule does NOT apply.
    // `eval` still runs and passthroughs the input. Per-node check inside
    // `eval` is what suppresses the buffer push.
    assert!(
        matches!(&result, NetworkResult::String(s) if s == "secret"),
        "passthrough must still occur even when execute_only suppresses the side effect"
    );
    assert!(
        texts.is_empty(),
        "execute_only=true must NOT push on a display pass; saw {:?}",
        texts
    );
}

#[test]
fn print_with_execute_only_true_appends_on_execute_pass() {
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "fire");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);
    set_print_execute_only(&mut designer, "main", p_id, true);

    let (result, texts) = evaluate_and_capture_prints(&designer, "main", p_id, true);
    assert!(matches!(&result, NetworkResult::String(s) if s == "fire"));
    assert_eq!(texts, vec!["fire".to_string()]);
}

// ============================================================================
// StructureDesigner.print_log integration via with_eval_context
// ============================================================================

#[test]
fn structure_designer_print_log_aggregates_across_passes() {
    // The orchestrator drains `context.print_buffer` into
    // `StructureDesigner.print_log` in `with_eval_context`. Two consecutive
    // execute passes should accumulate two entries; `take_print_log` then
    // returns both and leaves the log empty.
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "a");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);
    // execute_only=false so display + execute both push.

    let r1 = designer.execute_node("main", &[],p_id).expect("execute_node");
    assert!(r1.ok);
    assert_eq!(r1.logs.len(), 1, "first execute should report 1 entry");
    assert_eq!(r1.logs[0].text, "a");
    assert_eq!(r1.logs[0].node_id, p_id);
    assert!(r1.logs[0].from_execute);

    let r2 = designer.execute_node("main", &[],p_id).expect("execute_node");
    assert!(r2.ok);
    assert_eq!(
        r2.logs.len(),
        1,
        "second execute should report only THIS pass's entry, not the prior one"
    );

    // Drain everything: the log retains both entries from both passes (the
    // pass_start slice trick keeps them in print_log; APIExecuteResult.logs
    // sees only its own pass).
    let drained = designer.take_print_log();
    assert_eq!(drained.len(), 2);

    // After draining, the log is empty.
    let drained_again = designer.take_print_log();
    assert!(drained_again.is_empty());
}

#[test]
fn structure_designer_clear_print_log_empties_buffer() {
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "x");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);

    let _ = designer.execute_node("main", &[],p_id).expect("execute_node");
    assert_eq!(designer.print_log.len(), 1);

    designer.clear_print_log();
    assert!(designer.print_log.is_empty());
}

#[test]
fn print_log_entry_carries_network_name_and_node_label() {
    let mut designer = setup_designer_with_network("main");
    let s_id = add_string_literal(&mut designer, "main", DVec2::new(-200.0, 0.0), "labelled");
    let p_id = designer.add_node("print", DVec2::ZERO);
    designer.connect_nodes(s_id, 0, p_id, 0);

    let r = designer.execute_node("main", &[],p_id).expect("execute_node");
    assert!(r.ok);
    assert_eq!(r.logs.len(), 1);
    let entry = &r.logs[0];
    assert_eq!(entry.network_name, "main");
    assert_eq!(entry.text, "labelled");
    // The default custom_name for a node added via add_node is its type name
    // followed by an instance number (`print1`, etc.). The exact suffix
    // depends on how many `print` nodes have been added; assert prefix only.
    assert!(
        entry.node_label.starts_with("print"),
        "expected node_label to start with 'print', got '{}'",
        entry.node_label
    );
}
