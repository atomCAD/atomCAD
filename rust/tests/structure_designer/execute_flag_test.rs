//! Phase 2 tests for the node-execution design (`doc/design_node_execution.md`).
//!
//! Phase 2 lands the eval-time mechanism behind the Execute action — the
//! `execute` field on `NetworkEvaluationContext`, the FunctionEvaluator and
//! Walker context-propagation changes, and the central skip rule for
//! Unit-returning nodes — but no built-in node yet returns Unit, so the rule
//! is dormant for users. These tests register two synthetic test-only node
//! types (`counter_unit` and `mixed_output`) directly into a per-test
//! `NodeTypeRegistry` to exercise the rule end-to-end.

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
        description: "Test-only: increments a counter and returns Unit. Takes one ignored Int input so that this node can also be used as the body of a `map`/`filter`/`fold` (whose `FunctionEvaluator` requires at least one captured arg).".to_string(),
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

// ============================================================================
// FunctionEvaluator propagation — execute flag flows into closure bodies
// ============================================================================

/// Helper that builds a `map` node whose body calls a `counter_unit` node, and
/// drives the resulting `Iter[Unit]` walker. The counter records how many
/// times the body's `eval` actually ran. Returns the counter at end-of-drain.
///
/// We exercise the walker directly (not through `collect`) so the test does
/// not depend on registering custom networks — closure construction is what
/// the FE-propagation property hinges on, and `Walker::map` plus a
/// hand-built `FunctionEvaluator` capture exactly that path.
fn run_map_walker_over_counter(execute: bool) -> usize {
    use rust_lib_flutter_cad::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
    use rust_lib_flutter_cad::structure_designer::evaluator::iterator_walker::Walker;
    use rust_lib_flutter_cad::structure_designer::evaluator::network_result::Closure;
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

    // Build a registry where `counter_unit` is registered as a built-in node.
    // We start from a fresh `StructureDesigner` so all the standard built-in
    // nodes (parameter, value, expr, …) are present too — needed for closure
    // construction below.
    let mut sd = StructureDesigner::new();
    sd.node_type_registry
        .built_in_node_types
        .insert("counter_unit".to_string(), counter_unit_node_type());

    // Build a tiny user-defined network whose body is a single
    // `counter_unit` node. Its function-output pin is what the closure
    // captures.
    let network_name = "body";
    sd.add_node_network(network_name);
    sd.set_active_node_network_name(Some(network_name.to_string()));
    let body_node_id = sd.add_node("counter_unit", DVec2::ZERO);

    let counter = Arc::new(AtomicUsize::new(0));
    let result = with_counter(counter.clone(), || {
        // Build the closure manually: the function-output pin of the body
        // node, with no captured arguments (counter_unit takes none).
        // The closure captures the body node's parameter shape — one arg
        // for `counter_unit`. The walker will overwrite arg 0 per element via
        // `set_argument_value(0, elem)`.
        let closure = Closure {
            node_network_name: network_name.to_string(),
            node_id: body_node_id,
            captured_argument_values: vec![NetworkResult::None],
        };
        let fe = FunctionEvaluator::try_build(closure, &sd.node_type_registry).unwrap();
        let source = Walker::from_array(vec![
            NetworkResult::Int(0),
            NetworkResult::Int(1),
            NetworkResult::Int(2),
        ]);
        let mut walker = Walker::map(source, fe);

        let evaluator = NetworkEvaluator::new();
        let mut ctx = NetworkEvaluationContext::new();
        ctx.execute = execute;

        let mut count = 0;
        while let Some(value) = walker.next(&evaluator, &sd.node_type_registry, &mut ctx) {
            // Sanity: every yielded element should be Unit (the body returns
            // Unit either via central-skip or via direct eval).
            assert!(
                matches!(value, NetworkResult::Unit),
                "expected Unit per element, got {}",
                value.to_display_string()
            );
            count += 1;
        }
        assert_eq!(count, 3, "walker should yield exactly 3 elements");
        counter.load(Ordering::SeqCst)
    });

    result
}

#[test]
fn function_evaluator_propagates_execute_flag_into_closure_body() {
    // Execute pass: body's `eval` should run for every element.
    let count = run_map_walker_over_counter(/*execute=*/ true);
    assert_eq!(
        count, 3,
        "execute=true must run the closure body's eval per element (3 elements ⇒ 3 calls)"
    );
}

#[test]
fn function_evaluator_inherits_display_pass_and_central_rule_skips_body() {
    // Display pass: the central rule must skip the body's `eval` for each
    // element because the body returns Unit. The walker still yields N Unit
    // values — the synthesized result, not real eval calls.
    let count = run_map_walker_over_counter(/*execute=*/ false);
    assert_eq!(
        count, 0,
        "execute=false must skip the body's eval (the central rule applies inside the FE call)"
    );
}

// ============================================================================
// Walker propagation — context flows through nested Map walkers
// ============================================================================

/// Build a `Walker::Map(Walker::Map(source, fe_inner), fe_outer)` and drive
/// it with the given execute flag. Each FE wraps `counter_unit` so the test
/// can observe whether either body's `eval` actually ran. Returns the
/// observed counter at end-of-drain.
///
/// This exercises the "Walker::Map forwards `&mut context` to its enclosed
/// FE" wiring fix: if a chained walker dropped the context, the inner
/// walker's FE call would synthesise a fresh context with the default
/// `execute=false`, causing the central rule to skip the body even on an
/// execute pass.
fn run_chained_map_walker(execute: bool) -> usize {
    use rust_lib_flutter_cad::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
    use rust_lib_flutter_cad::structure_designer::evaluator::iterator_walker::Walker;
    use rust_lib_flutter_cad::structure_designer::evaluator::network_result::Closure;
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

    let mut sd = StructureDesigner::new();
    sd.node_type_registry
        .built_in_node_types
        .insert("counter_unit".to_string(), counter_unit_node_type());

    let inner_network = "inner";
    let outer_network = "outer";
    sd.add_node_network(inner_network);
    sd.set_active_node_network_name(Some(inner_network.to_string()));
    let inner_body_id = sd.add_node("counter_unit", DVec2::ZERO);
    sd.add_node_network(outer_network);
    sd.set_active_node_network_name(Some(outer_network.to_string()));
    let outer_body_id = sd.add_node("counter_unit", DVec2::ZERO);

    let counter = Arc::new(AtomicUsize::new(0));
    with_counter(counter.clone(), || {
        let inner_closure = Closure {
            node_network_name: inner_network.to_string(),
            node_id: inner_body_id,
            captured_argument_values: vec![NetworkResult::None],
        };
        let outer_closure = Closure {
            node_network_name: outer_network.to_string(),
            node_id: outer_body_id,
            captured_argument_values: vec![NetworkResult::None],
        };
        let fe_inner = FunctionEvaluator::try_build(inner_closure, &sd.node_type_registry).unwrap();
        let fe_outer = FunctionEvaluator::try_build(outer_closure, &sd.node_type_registry).unwrap();

        let source = Walker::from_array(vec![NetworkResult::Int(1), NetworkResult::Int(2)]);
        let mid = Walker::map(source, fe_inner);
        let mut walker = Walker::map(mid, fe_outer);

        let evaluator = NetworkEvaluator::new();
        let mut ctx = NetworkEvaluationContext::new();
        ctx.execute = execute;

        let mut yielded = 0;
        while let Some(_v) = walker.next(&evaluator, &sd.node_type_registry, &mut ctx) {
            yielded += 1;
        }
        assert_eq!(yielded, 2, "two-element source should yield two elements");
    });
    counter.load(Ordering::SeqCst)
}

#[test]
fn walker_propagates_execute_through_chained_map_walkers_under_execute() {
    // Each element drives both FE bodies — the inner body once, the outer
    // body once — giving 2 elements * 2 bodies = 4 eval calls.
    let count = run_chained_map_walker(/*execute=*/ true);
    assert_eq!(
        count, 4,
        "execute=true must propagate through both nested Map walkers (2 elements * 2 bodies)"
    );
}

#[test]
fn walker_propagates_display_pass_through_chained_map_walkers() {
    // On a display pass the central rule skips both bodies — neither inner
    // nor outer FE actually invokes eval. Without the `&mut context` wiring
    // through `Walker::next`, the inner walker would have built a fresh
    // context with `execute=false` (the default), but that's the same
    // result we'd see with proper propagation; this test guards against
    // the *opposite* regression — accidentally defaulting the inner context
    // to `execute=true`, which would cause spurious eval calls.
    let count = run_chained_map_walker(/*execute=*/ false);
    assert_eq!(
        count, 0,
        "execute=false must skip both nested bodies via the central rule"
    );
}
