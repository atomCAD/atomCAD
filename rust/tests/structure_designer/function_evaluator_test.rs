//! Regression tests for `FunctionEvaluator` (the engine that runs closures
//! captured from a node's function-output pin).
//!
//! Scenario: feeding the function output of a *dynamic-parameter* node
//! (`expr`, whose base `NodeType` declares `parameters: vec![]` because the
//! real parameter list is supplied via `calculate_custom_node_type`) into the
//! `f` pin of a `map` node used to panic at evaluation time:
//!
//!   thread '...' panicked at node_type_registry.rs:626:
//!   index out of bounds: the len is 0 but the index is 0
//!
//! Root cause: `FunctionEvaluator::new` clones the function node's data into a
//! throw-away network but never repopulates the `custom_node_type` cache on
//! the cloned node, so `get_node_type_for_node` falls back to the empty base
//! parameter list.

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn edit_designer_network(
    designer: &mut StructureDesigner,
    network_name: &str,
    code: &str,
    replace: bool,
) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .unwrap();
    let result = edit_network(&mut network, &designer.node_type_registry, code, replace);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
    designer.validate_active_network();
    result
}

fn find_node_id(designer: &StructureDesigner, network_name: &str, node_type_name: &str) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let (id, _) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == node_type_name)
        .unwrap_or_else(|| panic!("expected a `{}` node in `{}`", node_type_name, network_name));
    *id
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
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

#[test]
fn test_map_with_expr_function_evaluates_without_panic() {
    let mut designer = setup_designer_with_network("main");

    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 5 }
            f = expr {
                expression: "x * 2 + 1",
                parameters: [{ name: "x", data_type: Int }]
            }
            m = map { input_type: Int, output_type: Int, xs: r, f: @f }
        "#,
        true,
    );
    assert!(
        result.success,
        "edit_network should succeed: {:?}",
        result.errors
    );

    let map_id = find_node_id(&designer, "main", "map");

    // Phase 4: `map` produces `Iter[T]`, not `Array[T]`. Drain the walker to
    // collect the values for the assertion.
    let result = evaluate_node(&designer, "main", map_id);
    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        other => panic!(
            "expected Iterator result from map, got {}",
            other.to_display_string()
        ),
    };
    let registry = &designer.node_type_registry;
    let evaluator = NetworkEvaluator::new();
    let mut values: Vec<i32> = Vec::new();
    loop {
        match walker.next(&evaluator, registry, &mut NetworkEvaluationContext::new()) {
            None => break,
            Some(NetworkResult::Int(v)) => values.push(v),
            Some(NetworkResult::Error(e)) => panic!("map walker yielded Error: {}", e),
            Some(other) => panic!(
                "expected Int element from map, got {}",
                other.to_display_string()
            ),
        }
    }
    assert_eq!(values, vec![1, 3, 5, 7, 9], "expected i * 2 + 1 over 0..5");
}
