//! Phase 2 tests for the node-execution design (`doc/design_node_execution.md`).
//!
//! Phase 2 lands the eval-time mechanism behind the Execute action — the
//! `execute` field on `NetworkEvaluationContext`, the Walker / zone-closure
//! context-propagation, and the central skip rule for Unit-returning nodes —
//! but no built-in node yet returns Unit, so the rule is dormant for users.
//! These tests register two synthetic test-only node types (`counter_unit` and
//! `mixed_output`) directly into a per-test `NodeTypeRegistry` to exercise the
//! rule end-to-end. (The FE-driven walker-propagation tests that once lived
//! here were removed in closures Phase 2 along with `FunctionEvaluator`.)

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::{EvalOutput, NodeData};
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::{
    NodeType, OutputPinDefinition, no_data_loader, no_data_saver,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

// ============================================================================
// Test fixtures
// ============================================================================

/// Per-test counter shared between the test thread and the synthetic node's
/// `eval`. The node increments the counter on each `eval` invocation, so a
/// passing display-pass test asserts the counter stayed at 0 (the central
/// skip rule short-circuited eval).
type Counter = Arc<AtomicUsize>;

thread_local! {
    /// The active counter for the current test. `CounterUnitNodeData::eval`
    /// reads this on each invocation so the node `clone_box`es safely (its
    /// own data carries no Arc, which keeps it `Serialize`-shaped consistent
    /// with how production `NodeData` impls are built). Each test sets this
    /// at the start of the test body and clears it at the end.
    static ACTIVE_COUNTER: std::cell::RefCell<Option<Counter>> = const { std::cell::RefCell::new(None) };
}

fn set_active_counter(counter: Counter) {
    ACTIVE_COUNTER.with(|c| *c.borrow_mut() = Some(counter));
}

fn clear_active_counter() {
    ACTIVE_COUNTER.with(|c| *c.borrow_mut() = None);
}

fn bump_active_counter() {
    ACTIVE_COUNTER.with(|c| {
        if let Some(counter) = &*c.borrow() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
}

/// Node that returns `Unit` and increments the active counter on every eval.
/// All output pins are `Unit`, so the central skip rule applies on display
/// passes.
#[derive(Debug)]
struct CounterUnitNodeData;

impl NodeData for CounterUnitNodeData {
    fn provide_gadget(
        &self,
        _structure_designer: &rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner,
    ) -> Option<
        Box<dyn rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget>,
    > {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        bump_active_counter();
        EvalOutput::single(NetworkResult::Unit)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(CounterUnitNodeData)
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }
}

fn counter_unit_node_type() -> NodeType {
    use rust_lib_flutter_cad::structure_designer::node_type::Parameter;
    NodeType {
        name: "counter_unit".to_string(),
        description: "Test-only: increments a counter and returns Unit. Takes one ignored Int input so that this node can also be used as the body of a `map`/`filter`/`fold`.".to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![Parameter {
            id: None,
            name: "elem".to_string(),
            data_type: DataType::Int,
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::Unit),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: false,
        node_data_creator: || Box::new(CounterUnitNodeData),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    }
}

/// Node with two output pins: pin 0 is `Float(7.0)`, pin 1 is `Unit`. Used to
/// guard the central rule's "all output pins must be Unit" precondition — the
/// rule must NOT skip this node because pin 0 carries data the downstream
/// graph would need.
#[derive(Debug)]
struct MixedOutputNodeData;

impl NodeData for MixedOutputNodeData {
    fn provide_gadget(
        &self,
        _structure_designer: &rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner,
    ) -> Option<
        Box<dyn rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget>,
    > {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        bump_active_counter();
        EvalOutput::multi(vec![NetworkResult::Float(7.0), NetworkResult::Unit])
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(MixedOutputNodeData)
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }
}

fn mixed_output_node_type() -> NodeType {
    NodeType {
        name: "mixed_output".to_string(),
        description: "Test-only: returns (Float(7.0), Unit). Guards the central skip rule's all-Unit precondition.".to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![],
        output_pins: vec![
            OutputPinDefinition::fixed("value", DataType::Float),
            OutputPinDefinition::fixed("done", DataType::Unit),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: false,
        node_data_creator: || Box::new(MixedOutputNodeData),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    }
}

fn build_test_registry() -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    registry
        .built_in_node_types
        .insert("counter_unit".to_string(), counter_unit_node_type());
    registry
        .built_in_node_types
        .insert("mixed_output".to_string(), mixed_output_node_type());
    registry
}

/// Build a top-level network containing a single instance of
/// `node_type_name` (no inputs wired). Returns the network plus the node id.
fn make_single_node_network(
    registry: &NodeTypeRegistry,
    node_type_name: &str,
    arg_count: usize,
) -> (NodeNetwork, u64) {
    let mut network = NodeNetwork::new(NodeType {
        name: "test_network".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::None),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
        },
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    });
    let node_id = network.add_node(
        node_type_name,
        DVec2::ZERO,
        arg_count,
        registry
            .built_in_node_types
            .get(node_type_name)
            .unwrap()
            .node_data_creator
            .clone()(),
    );
    (network, node_id)
}

/// Helper to set the active counter for the duration of a closure and clear
/// it on return. Equivalent to a thread-local scope guard.
fn with_counter<R>(counter: Counter, f: impl FnOnce() -> R) -> R {
    set_active_counter(counter);
    let r = f();
    clear_active_counter();
    r
}

// ============================================================================
// Central skip rule — all-Unit
// ============================================================================

#[test]
fn central_skip_rule_skips_eval_on_display_pass_when_all_pins_are_unit() {
    let registry = build_test_registry();
    let (network, node_id) = make_single_node_network(&registry, "counter_unit", 1);

    let evaluator = NetworkEvaluator::new();
    let counter = Arc::new(AtomicUsize::new(0));

    with_counter(counter.clone(), || {
        let mut ctx = NetworkEvaluationContext::new();
        // execute = false (the default) — display pass.
        let stack = vec![NetworkStackElement {
            is_zone_body: false,
            node_network: &network,
            node_id: 0,
        }];
        let result = evaluator.evaluate(&stack, node_id, 0, &registry, false, &mut ctx);
        assert!(matches!(result, NetworkResult::Unit));
    });

    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "central skip rule should have prevented eval from running"
    );
}

#[test]
fn central_skip_rule_runs_eval_on_execute_pass_when_all_pins_are_unit() {
    let registry = build_test_registry();
    let (network, node_id) = make_single_node_network(&registry, "counter_unit", 1);

    let evaluator = NetworkEvaluator::new();
    let counter = Arc::new(AtomicUsize::new(0));

    with_counter(counter.clone(), || {
        let mut ctx = NetworkEvaluationContext::new();
        ctx.execute = true;
        let stack = vec![NetworkStackElement {
            is_zone_body: false,
            node_network: &network,
            node_id: 0,
        }];
        let result = evaluator.evaluate(&stack, node_id, 0, &registry, false, &mut ctx);
        assert!(matches!(result, NetworkResult::Unit));
    });

    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "execute pass must invoke eval exactly once"
    );
}

#[test]
fn central_skip_rule_via_evaluate_all_outputs_skips_on_display_pass() {
    let registry = build_test_registry();
    let (network, node_id) = make_single_node_network(&registry, "counter_unit", 1);

    let evaluator = NetworkEvaluator::new();
    let counter = Arc::new(AtomicUsize::new(0));

    with_counter(counter.clone(), || {
        let mut ctx = NetworkEvaluationContext::new();
        let stack = vec![NetworkStackElement {
            is_zone_body: false,
            node_network: &network,
            node_id: 0,
        }];
        let output = evaluator.evaluate_all_outputs(&stack, node_id, &registry, false, &mut ctx);
        assert_eq!(output.results.len(), 1);
        assert!(matches!(output.results[0], NetworkResult::Unit));
    });

    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

// ============================================================================
// Central skip rule — mixed-output guard
// ============================================================================

#[test]
fn central_skip_rule_does_not_skip_mixed_output_node_on_display_pass() {
    let registry = build_test_registry();
    let (network, node_id) = make_single_node_network(&registry, "mixed_output", 0);

    let evaluator = NetworkEvaluator::new();
    let counter = Arc::new(AtomicUsize::new(0));

    with_counter(counter.clone(), || {
        let mut ctx = NetworkEvaluationContext::new();
        let stack = vec![NetworkStackElement {
            is_zone_body: false,
            node_network: &network,
            node_id: 0,
        }];
        // Pin 0 is Float(7.0); the central rule must not apply because pin
        // 1's Unit type is not enough to make this an all-Unit node.
        let pin0 = evaluator.evaluate(&stack, node_id, 0, &registry, false, &mut ctx);
        let pin1 = evaluator.evaluate(&stack, node_id, 1, &registry, false, &mut ctx);
        assert!(
            matches!(pin0, NetworkResult::Float(v) if (v - 7.0).abs() < 1e-12),
            "pin 0 should carry Float(7.0)"
        );
        assert!(matches!(pin1, NetworkResult::Unit));
    });

    // `evaluate` is called twice; each call invokes eval once.
    assert!(
        counter.load(Ordering::SeqCst) >= 1,
        "mixed-output node must run eval on display passes"
    );
}
