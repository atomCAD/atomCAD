//! Phase 3 closures tests: the `closure` node produces a first-class
//! `Function` value, consumed two ways — by a `map` via its new `f` pin
//! (iteration) and by an `apply` node (single-value call).
//!
//! Like `zones_test.rs`, these build bodies by direct manipulation of the
//! zone-owning node's owned `NodeNetwork` (no text-format syntax for closures
//! yet). The helpers are intentionally close to the `zones_test.rs` ones but
//! generalised: a "zone-owning node" is either an HOF *or* a `closure` node.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{Argument, IncomingWire, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
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

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
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

/// Drain a freshly evaluated walker against a real designer's registry so
/// per-element body evaluations can see all node types.
fn drain_iter_with_designer(
    designer: &StructureDesigner,
    result: NetworkResult,
) -> Vec<NetworkResult> {
    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        other => panic!(
            "expected NetworkResult::Iterator, got {}",
            other.to_display_string()
        ),
    };
    let evaluator = NetworkEvaluator::new();
    let registry = &designer.node_type_registry;
    let mut context = NetworkEvaluationContext::new();
    let mut out = Vec::new();
    let cap = 4096;
    while out.len() < cap {
        match walker.next(&evaluator, registry, &mut context) {
            None => return out,
            Some(v) => out.push(v),
        }
    }
    panic!("drain exceeded cap of {cap} elements");
}

fn extract_ints(values: Vec<NetworkResult>) -> Vec<i32> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Int(v) => v,
            NetworkResult::Error(msg) => panic!("expected Int element, got Error: {msg}"),
            other => panic!("expected Int element, got {}", other.to_display_string()),
        })
        .collect()
}

fn extract_int(result: NetworkResult) -> i32 {
    match result {
        NetworkResult::Int(v) => v,
        NetworkResult::Error(msg) => panic!("expected Int, got Error: {msg}"),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

/// Add an `int` constant node to the active network.
fn add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    let id = designer.add_node("int", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

/// Add a `range(start, step, count)` node to the active network.
fn add_range(
    designer: &mut StructureDesigner,
    network: &str,
    start: i32,
    step: i32,
    count: i32,
    y: f64,
) -> u64 {
    let id = designer.add_node("range", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(RangeData { start, step, count }));
    id
}

/// Add an `expr` node into a zone-owning node's body. Works for any
/// zone-bearing node type (an HOF or a `closure` node). Returns the new body
/// node's id (in the body's local id space).
fn add_expr_to_body(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    expression: &str,
    parameters: Vec<(String, DataType)>,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(owner_network).unwrap();
    let owner_node = network.nodes.get_mut(&owner_node_id).unwrap();
    let body = owner_node.zone_mut().expect("zone-owning node missing zone");

    let expr_params: Vec<ExprParameter> = parameters
        .into_iter()
        .map(|(name, dt)| ExprParameter {
            id: None,
            name,
            data_type: dt,
            data_type_str: None,
        })
        .collect();
    let num_params = expr_params.len();
    let mut expr_data = ExprData {
        parameters: expr_params,
        expression: expression.to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = expr_data.parse_and_validate(0);
    let expr_id = body.add_node("expr", DVec2::new(50.0, 0.0), num_params, Box::new(expr_data));

    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        registry
            .node_networks
            .get_mut(owner_network)
            .unwrap()
            .nodes
            .get_mut(&owner_node_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&expr_id)
            .unwrap(),
        true,
    );

    expr_id
}

/// Wire the owner's `zone_input_pin` (e.g. `element`) into a body node arg.
fn wire_zone_input_to_body_node(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    zone_input_pin: usize,
    body_node_id: u64,
    body_param_index: usize,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap();
    let owner_node = network.nodes.get_mut(&owner_node_id).unwrap();
    let body = owner_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: owner_node_id,
            source_pin: SourcePin::ZoneInput {
                pin_index: zone_input_pin,
            },
            source_scope_depth: 1,
        });
}

/// Wire an outer-scope node output (a capture) into a body node arg (depth 1).
fn wire_capture_to_body_node(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    body_node_id: u64,
    body_param_index: usize,
    source_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap();
    let owner_node = network.nodes.get_mut(&owner_node_id).unwrap();
    let body = owner_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        });
}

/// Wire a body node into the owner's `result` zone-output pin (index 0).
fn wire_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    body_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap();
    let owner_node = network.nodes.get_mut(&owner_node_id).unwrap();
    if owner_node.zone_output_arguments.is_empty() {
        owner_node.zone_output_arguments.push(Argument::new());
    }
    owner_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Add a `closure` node of the map-like `(Int) -> Int` kind with the given
/// body expression, optionally capturing one outer constant. Returns the
/// closure node's id.
///
/// `capture_source` — when `Some((param_name, source_node_id))`, the body's
/// expr gets a second parameter wired from that outer node as a capture.
fn add_int_map_closure(
    designer: &mut StructureDesigner,
    network: &str,
    expression: &str,
    element_param: &str,
    capture: Option<(&str, u64)>,
    y: f64,
) -> u64 {
    let closure_id = designer.add_node("closure", DVec2::new(150.0, y));
    set_node_data(
        designer,
        network,
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
        }),
    );

    let mut params = vec![(element_param.to_string(), DataType::Int)];
    if let Some((cap_name, _)) = capture {
        params.push((cap_name.to_string(), DataType::Int));
    }
    let expr_id = add_expr_to_body(designer, network, closure_id, expression, params);

    // element zone-input → expr param 0.
    wire_zone_input_to_body_node(designer, network, closure_id, 0, expr_id, 0);
    if let Some((_, source_node_id)) = capture {
        wire_capture_to_body_node(designer, network, closure_id, expr_id, 1, source_node_id);
    }
    wire_body_node_to_zone_output(designer, network, closure_id, expr_id);

    closure_id
}

// ============================================================================
// Tests
// ============================================================================

/// `range(3) → map(f: closure(element + 1))` yields `[1, 2, 3]`. The first
/// end-to-end function value: a `closure` produces it, `map` consumes it via
/// the new `f` pin.
#[test]
fn map_with_closure_f_pin_element_plus_one() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_map_closure(&mut designer, "main", "x + 1", "x", None, -120.0);

    let map_id = designer.add_node("map", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    designer.connect_nodes(closure_id, 0, map_id, 1); // f

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![1, 2, 3]);
}

/// Closure reuse: one `closure` value wired into two independent `map`s. Both
/// evaluate correctly; neither starves the other (each consumer re-evaluates
/// the closure and gets its own walker).
#[test]
fn closure_reused_by_two_maps() {
    let mut designer = setup_designer_with_network("main");

    let closure_id = add_int_map_closure(&mut designer, "main", "x * 10", "x", None, -150.0);

    // map 1 over range(3): [0,10,20]
    let range1 = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let map1 = designer.add_node("map", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map1,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range1, 0, map1, 0);
    designer.connect_nodes(closure_id, 0, map1, 1);

    // map 2 over range(start=5, step=1, count=2): [50,60]
    let range2 = add_range(&mut designer, "main", 5, 1, 2, 150.0);
    let map2 = designer.add_node("map", DVec2::new(350.0, 150.0));
    set_node_data(
        &mut designer,
        "main",
        map2,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range2, 0, map2, 0);
    designer.connect_nodes(closure_id, 0, map2, 1);

    let r1 = evaluate_node(&designer, "main", map1);
    let r2 = evaluate_node(&designer, "main", map2);
    assert_eq!(extract_ints(drain_iter_with_designer(&designer, r1)), vec![0, 10, 20]);
    assert_eq!(extract_ints(drain_iter_with_designer(&designer, r2)), vec![50, 60]);
}

/// Capture through a closure: the body captures a parent `k = int(5)`. The
/// captured value is frozen at the `closure` node's eval and reflected in
/// every produced element.
#[test]
fn map_with_closure_capturing_outer_constant() {
    let mut designer = setup_designer_with_network("main");

    let k_id = add_int(&mut designer, "main", 5, -240.0);
    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id =
        add_int_map_closure(&mut designer, "main", "x + k", "x", Some(("k", k_id)), -120.0);

    let map_id = designer.add_node("map", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);
    designer.connect_nodes(closure_id, 0, map_id, 1);

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![5, 6, 7]);
}

/// `f` connected ⇒ the HOF's own inline zone body is ignored. The map node is
/// given an inline body that would add 100; wiring a closure that adds 1 into
/// `f` must win.
#[test]
fn f_pin_overrides_inline_zone_body() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);

    let map_id = designer.add_node("map", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);

    // Give the map a *different* inline body: x + 100. If `f` is honored this
    // body is never run.
    let inline_expr =
        add_expr_to_body(&mut designer, "main", map_id, "x + 100", vec![("x".to_string(), DataType::Int)]);
    wire_zone_input_to_body_node(&mut designer, "main", map_id, 0, inline_expr, 0);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, inline_expr);

    // Closure that adds 1, wired into `f`.
    let closure_id = add_int_map_closure(&mut designer, "main", "x + 1", "x", None, -150.0);
    designer.connect_nodes(closure_id, 0, map_id, 1);

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![1, 2, 3], "f should win over the inline body");
}

/// Direct single-value call: `apply(f: closure(element + 1), 10)` yields `11`.
/// No iterator is involved — this is what makes a `Function` a callable value.
#[test]
fn apply_calls_closure_once() {
    let mut designer = setup_designer_with_network("main");

    let closure_id = add_int_map_closure(&mut designer, "main", "x + 1", "x", None, -120.0);
    let arg_id = add_int(&mut designer, "main", 10, 0.0);

    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
        }),
    );
    designer.connect_nodes(closure_id, 0, apply_id, 0); // f
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element

    let result = evaluate_node(&designer, "main", apply_id);
    assert_eq!(extract_int(result), 11);
}

/// `apply` honours a closure's frozen capture: the closure captures `k = 5`
/// and adds it; `apply(f, 10)` yields `15`.
#[test]
fn apply_honors_closure_capture() {
    let mut designer = setup_designer_with_network("main");

    let k_id = add_int(&mut designer, "main", 5, -240.0);
    let closure_id =
        add_int_map_closure(&mut designer, "main", "x + k", "x", Some(("k", k_id)), -120.0);
    let arg_id = add_int(&mut designer, "main", 10, 0.0);

    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
        }),
    );
    designer.connect_nodes(closure_id, 0, apply_id, 0); // f
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element

    let result = evaluate_node(&designer, "main", apply_id);
    assert_eq!(extract_int(result), 15);
}

/// A disconnected `f` on `apply` is an evaluation error (it has no inline body
/// to fall back to).
#[test]
fn apply_with_disconnected_f_errors() {
    let mut designer = setup_designer_with_network("main");

    let arg_id = add_int(&mut designer, "main", 10, 0.0);
    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
        }),
    );
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element only; f left open

    let result = evaluate_node(&designer, "main", apply_id);
    match result {
        NetworkResult::Error(_) => {}
        other => panic!("expected Error for disconnected f, got {}", other.to_display_string()),
    }
}
