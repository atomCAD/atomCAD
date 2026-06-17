//! Regression tests for the `evaluate_arg` argument/parameter desync crash.
//!
//! Background (mechadense's non-reproducible `index out of bounds: the len is 0
//! but the index is 0` panic): a node's `arguments` vector is supposed to carry
//! one slot per declared parameter, but it can transiently be *shorter* than the
//! parameter list. The slots are grown to match by `repair_network_arguments`,
//! which runs only inside `validate_network`; the load-time `repair_node_network`
//! grows only top-level nodes, and switching the active network used to mark a
//! refresh **without** validating. So `generate_scene` could evaluate a node
//! with `parameters.len() > arguments.len()` — most easily an `expr`, whose
//! `eval` iterates its own `self.parameters` and calls `evaluate_arg` with a
//! `parameter_index` past the end of `arguments` — and panic.
//!
//! Two independent defenses, one test each:
//! 1. `evaluate_arg` bounds-guards the index and returns "unconnected" instead of
//!    panicking.
//! 2. `set_active_node_network_name` validates (and thereby repairs) the newly
//!    active network before the caller's refresh evaluates it.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Add an `expr` node with a single `x: Int` parameter and body `x`.
fn add_expr_x(designer: &mut StructureDesigner, network: &str) -> u64 {
    let mut expr_data = ExprData {
        parameters: vec![ExprParameter {
            id: Some(1),
            name: "x".to_string(),
            data_type: DataType::Int,
            data_type_str: None,
        }],
        expression: "x".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = expr_data.parse_and_validate(0);

    let id = designer.add_node("expr", DVec2::new(150.0, 0.0));

    // Install the data + refresh the per-node custom-node-type cache (mirrors the
    // `set_node_data` helper used across the structure_designer tests).
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(network).unwrap();
    let node = net.nodes.get_mut(&id).unwrap();
    node.data = Box::new(expr_data) as Box<dyn NodeData>;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
    id
}

/// Forcibly empty a node's `arguments` to reproduce the desync state a freshly
/// loaded (not-yet-validated) network can carry.
fn clear_arguments(designer: &mut StructureDesigner, network: &str, node_id: u64) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    net.nodes.get_mut(&node_id).unwrap().arguments.clear();
}

fn argument_count(designer: &StructureDesigner, network: &str, node_id: u64) -> usize {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .arguments
        .len()
}

/// Defense #1: evaluating an `expr` node whose `arguments` is shorter than its
/// `parameters` must NOT panic — `evaluate_arg` returns "unconnected", so the
/// node reports a clean input-missing error instead of crashing the whole pass.
#[test]
fn evaluate_arg_does_not_panic_on_undersized_arguments() {
    let mut designer = setup_designer_with_network("main");
    let expr_id = add_expr_x(&mut designer, "main");

    // Simulate the desynced-load state: parameter `x` exists, no argument slot.
    clear_arguments(&mut designer, "main", expr_id);
    assert_eq!(argument_count(&designer, "main", expr_id), 0);

    // Evaluate the node directly (this is the path that used to panic via
    // ExprData::eval -> evaluate_arg -> node.arguments[0]).
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("main").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];

    let result = evaluator.evaluate(&network_stack, expr_id, 0, registry, false, &mut context);

    // The unconnected input surfaces as an Error, not a panic.
    match result {
        NetworkResult::Error(_) => {}
        other => panic!(
            "expected a clean input-missing Error, got {}",
            other.to_display_string()
        ),
    }
}

/// Defense #2: re-activating the network runs validate/repair, which grows the
/// `arguments` back to one slot per parameter — so the network is correct on the
/// first frame the user sees after switching to it.
#[test]
fn set_active_network_repairs_undersized_arguments() {
    let mut designer = setup_designer_with_network("main");
    let expr_id = add_expr_x(&mut designer, "main");

    clear_arguments(&mut designer, "main", expr_id);
    assert_eq!(argument_count(&designer, "main", expr_id), 0);

    // Switching to the network (the user clicking it in the tree/list) must
    // leave it repaired before any refresh evaluates it.
    designer.set_active_node_network_name(Some("main".to_string()));

    assert_eq!(
        argument_count(&designer, "main", expr_id),
        1,
        "activating the network should have grown `arguments` to match the \
         expr node's single parameter"
    );
}
