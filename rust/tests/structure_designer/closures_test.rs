//! Closures tests (Phases 3–4): the `closure` node produces a first-class
//! `Function` value, consumed by all four HOFs (`map`/`filter`/`fold`/
//! `foreach`) via their `f` pin (iteration), by an `apply` node (single-value
//! call), and across a network boundary (a function-factory subnetwork).
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
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, IncomingWire, NodeNetwork, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::foreach::ForeachData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::print::PrintData;
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
    set_node_data(
        designer,
        network,
        id,
        Box::new(RangeData { start, step, count }),
    );
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
    let body = owner_node
        .zone_mut()
        .expect("zone-owning node missing zone");

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
    let expr_id = body.add_node(
        "expr",
        DVec2::new(50.0, 0.0),
        num_params,
        Box::new(expr_data),
    );

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
            param_names: vec![],
            custom_label: None,
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
    assert_eq!(
        extract_ints(drain_iter_with_designer(&designer, r1)),
        vec![0, 10, 20]
    );
    assert_eq!(
        extract_ints(drain_iter_with_designer(&designer, r2)),
        vec![50, 60]
    );
}

/// Capture through a closure: the body captures a parent `k = int(5)`. The
/// captured value is frozen at the `closure` node's eval and reflected in
/// every produced element.
#[test]
fn map_with_closure_capturing_outer_constant() {
    let mut designer = setup_designer_with_network("main");

    let k_id = add_int(&mut designer, "main", 5, -240.0);
    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_map_closure(
        &mut designer,
        "main",
        "x + k",
        "x",
        Some(("k", k_id)),
        -120.0,
    );

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
    let inline_expr = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + 100",
        vec![("x".to_string(), DataType::Int)],
    );
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
            param_names: vec![],
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
    let closure_id = add_int_map_closure(
        &mut designer,
        "main",
        "x + k",
        "x",
        Some(("k", k_id)),
        -120.0,
    );
    let arg_id = add_int(&mut designer, "main", 10, 0.0);

    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
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
            param_names: vec![],
        }),
    );
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element only; f left open

    let result = evaluate_node(&designer, "main", apply_id);
    match result {
        NetworkResult::Error(_) => {}
        other => panic!(
            "expected Error for disconnected f, got {}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Phase 4 — `f` pin on filter / fold / foreach
// ============================================================================

/// Add a `closure` node of the filter-like `(Int) -> Bool` kind with the given
/// predicate body expression (one zone-input parameter `element`). Returns the
/// closure node's id.
fn add_int_filter_closure(
    designer: &mut StructureDesigner,
    network: &str,
    expression: &str,
    element_param: &str,
    y: f64,
) -> u64 {
    let closure_id = designer.add_node("closure", DVec2::new(150.0, y));
    set_node_data(
        designer,
        network,
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Filter,
            type_args: vec![DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );

    // The `element` param is Int; the body expression evaluates to Bool.
    let expr_id = add_expr_to_body(
        designer,
        network,
        closure_id,
        expression,
        vec![(element_param.to_string(), DataType::Int)],
    );

    wire_zone_input_to_body_node(designer, network, closure_id, 0, expr_id, 0);
    wire_body_node_to_zone_output(designer, network, closure_id, expr_id);

    closure_id
}

/// Add a `closure` node of the fold-like `(Int, Int) -> Int` kind. The body
/// expr has params `(acc_param, element_param[, capture_param])`; `acc` is wired
/// from zone-input pin 0, `element` from pin 1, and the optional capture from
/// `capture.1`. Returns the closure node's id.
fn add_int_fold_closure(
    designer: &mut StructureDesigner,
    network: &str,
    expression: &str,
    acc_param: &str,
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
            kind: ClosureKind::Fold,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );

    let mut params = vec![
        (acc_param.to_string(), DataType::Int),
        (element_param.to_string(), DataType::Int),
    ];
    if let Some((cap_name, _)) = capture {
        params.push((cap_name.to_string(), DataType::Int));
    }
    let expr_id = add_expr_to_body(designer, network, closure_id, expression, params);

    // acc zone-input (pin 0) → expr param 0; element zone-input (pin 1) → param 1.
    wire_zone_input_to_body_node(designer, network, closure_id, 0, expr_id, 0);
    wire_zone_input_to_body_node(designer, network, closure_id, 1, expr_id, 1);
    if let Some((_, source_node_id)) = capture {
        wire_capture_to_body_node(designer, network, closure_id, expr_id, 2, source_node_id);
    }
    wire_body_node_to_zone_output(designer, network, closure_id, expr_id);

    closure_id
}

/// Add a built-in node (by type name) into a zone-owning node's body and
/// populate its custom-type cache. Returns the new body node's id.
fn add_node_to_body(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    node_type_name: &str,
    num_params: usize,
    data: Box<dyn NodeData>,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let body = registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap()
        .zone_mut()
        .expect("zone-owning node missing zone");
    let body_node_id = body.add_node(node_type_name, DVec2::new(50.0, 0.0), num_params, data);

    let body_node = registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap()
        .zone_mut()
        .unwrap()
        .nodes
        .get_mut(&body_node_id)
        .unwrap();
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        body_node,
        true,
    );
    body_node_id
}

/// Add a `closure` node of the foreach-like `(Int) -> Unit` kind whose body is a
/// single `print` node (a side effect). The element is unused and the print's
/// `text` defaults to empty; the print's `String` output feeds the closure's
/// `out` (Unit) zone-output — `foreach` discards the value, but the print fires
/// once per element. Returns the closure node's id.
fn add_print_foreach_closure(designer: &mut StructureDesigner, network: &str, y: f64) -> u64 {
    let closure_id = designer.add_node("closure", DVec2::new(150.0, y));
    set_node_data(
        designer,
        network,
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Foreach,
            type_args: vec![DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );

    let print_id = add_node_to_body(
        designer,
        network,
        closure_id,
        "print",
        1,
        Box::new(PrintData::default()),
    );
    wire_body_node_to_zone_output(designer, network, closure_id, print_id);

    closure_id
}

/// Like `evaluate_node`, but lets the caller drive an Execute pass and observe
/// the side-effect print count. Returns `(result, print_buffer_len)`.
fn evaluate_with_execute_capturing_prints(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    execute: bool,
) -> (NetworkResult, usize) {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.execute = execute;
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context);
    (result, context.print_buffer.len())
}

/// Copy of the subnetwork-parameter configuration helper (mirrors the
/// `set_parameter_data` API): sets a parameter node's name and type through
/// `set_node_network_data` so its cached `custom_node_type` (and thus output
/// pin type) is refreshed.
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

/// `filter(f: closure(element % 2 == 0))` over `range(0..6)` keeps the evens.
#[test]
fn filter_with_closure_f_pin_even() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 6, 0.0); // [0,1,2,3,4,5]
    let closure_id = add_int_filter_closure(&mut designer, "main", "x % 2 == 0", "x", -150.0);

    let filter_id = designer.add_node("filter", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        filter_id,
        Box::new(FilterData {
            element_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, filter_id, 0); // xs
    designer.connect_nodes(closure_id, 0, filter_id, 1); // f

    let result = evaluate_node(&designer, "main", filter_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![0, 2, 4]);
}

/// `fold(f: closure((acc, element) -> acc + element), init = 0)` over
/// `range(1..5)` sums to 10.
#[test]
fn fold_with_closure_f_pin_sum() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 1, 1, 4, 0.0); // [1,2,3,4]
    let init_id = add_int(&mut designer, "main", 0, 80.0);
    let closure_id = add_int_fold_closure(
        &mut designer,
        "main",
        "acc + element",
        "acc",
        "element",
        None,
        -150.0,
    );

    let fold_id = designer.add_node("fold", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, fold_id, 0); // xs
    designer.connect_nodes(init_id, 0, fold_id, 1); // init
    designer.connect_nodes(closure_id, 0, fold_id, 2); // f

    let result = evaluate_node(&designer, "main", fold_id);
    assert_eq!(extract_int(result), 10);
}

/// `fold` with a closure whose body captures a constant offset `k = 100`. Over
/// `range(1..4)` with `init = 0`: `(acc + element + 100)` applied 3 times gives
/// `1 + 2 + 3 + 3*100 = 306`. The capture is frozen once (the `closure` node is
/// evaluated once, *outside* the fold) and shared across every iteration — the
/// "capture frozen once" half of the capture-freeze-timing story.
#[test]
fn fold_with_closure_f_pin_captured_offset() {
    let mut designer = setup_designer_with_network("main");

    let k_id = add_int(&mut designer, "main", 100, -260.0);
    let range_id = add_range(&mut designer, "main", 1, 1, 3, 0.0); // [1,2,3]
    let init_id = add_int(&mut designer, "main", 0, 80.0);
    let closure_id = add_int_fold_closure(
        &mut designer,
        "main",
        "acc + element + k",
        "acc",
        "element",
        Some(("k", k_id)),
        -150.0,
    );

    let fold_id = designer.add_node("fold", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, fold_id, 0); // xs
    designer.connect_nodes(init_id, 0, fold_id, 1); // init
    designer.connect_nodes(closure_id, 0, fold_id, 2); // f

    let result = evaluate_node(&designer, "main", fold_id);
    assert_eq!(extract_int(result), 306);
}

/// `foreach(f: closure(... -> Unit))` — the closure body is a `print` (a side
/// effect). Execute gating still works through the `f` pin: a display pass is
/// short-circuited by the central skip rule (the body never runs, `f` is never
/// evaluated), and an Execute pass runs the closure body once per element.
#[test]
fn foreach_with_closure_f_pin_execute_gating() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0); // [0,1,2]
    let closure_id = add_print_foreach_closure(&mut designer, "main", -150.0);

    let foreach_id = designer.add_node("foreach", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        foreach_id,
        Box::new(ForeachData {
            input_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, foreach_id, 0); // xs
    designer.connect_nodes(closure_id, 0, foreach_id, 1); // f

    // Display pass: central skip rule short-circuits foreach. The body's print
    // never fires.
    let (disp_result, disp_prints) =
        evaluate_with_execute_capturing_prints(&designer, "main", foreach_id, false);
    assert!(matches!(disp_result, NetworkResult::Unit));
    assert_eq!(disp_prints, 0, "display pass must not run the foreach body");

    // Execute pass: foreach drains all 3 elements and runs the closure body
    // (the print side effect) once per element.
    let (exec_result, exec_prints) =
        evaluate_with_execute_capturing_prints(&designer, "main", foreach_id, true);
    assert!(matches!(exec_result, NetworkResult::Unit));
    assert_eq!(
        exec_prints, 3,
        "execute pass must run the closure body once per element"
    );
}

/// Function-factory smoke test: a `(k: Int) -> Function` subnetwork whose return
/// is a `closure` capturing `k` and adding it. In the parent, `apply(factory(5),
/// 10)` yields `15` — proving a function value crosses a network boundary *and*
/// is callable via `apply`, using only authorable v1 surface (no `Function`-typed
/// parameter).
#[test]
fn function_factory_returns_closure_applied_in_parent() {
    let mut designer = setup_designer_with_network("factory");

    // factory(k: Int) -> Function: a `k` parameter and a closure capturing it.
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    configure_parameter(&mut designer, "factory", param_id, "k", DataType::Int);

    // Closure body `x + k`, capturing the parameter `k` (depth-1 capture). The
    // closure is the return node ⇒ factory's output type is Function((Int)->Int).
    let closure_id = add_int_map_closure(
        &mut designer,
        "factory",
        "x + k",
        "x",
        Some(("k", param_id)),
        -150.0,
    );
    designer.set_return_node_id(Some(closure_id));
    designer.validate_active_network();

    // Parent "main": apply(factory(5), 10) == 15.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let k_val_id = add_int(&mut designer, "main", 5, -100.0);
    let factory_call_id = designer.add_node("factory", DVec2::new(200.0, 0.0));
    designer.connect_nodes(k_val_id, 0, factory_call_id, 0); // k = 5

    let arg_id = add_int(&mut designer, "main", 10, 100.0);
    let apply_id = designer.add_node("apply", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
        }),
    );
    designer.connect_nodes(factory_call_id, 0, apply_id, 0); // f = factory(5)
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element = 10

    let result = evaluate_node(&designer, "main", apply_id);
    assert_eq!(extract_int(result), 15);
}

// ============================================================================
// Phase 4 — capture-freeze timing and the owner_node_id collision regression
// ============================================================================

/// Copy of `zones_test::add_expr_to_network`: add an `expr` into a raw body
/// `NodeNetwork` (does not populate its custom-type cache — the caller does).
fn add_expr_to_network(
    network: &mut NodeNetwork,
    expression: &str,
    parameters: Vec<(String, DataType)>,
) -> u64 {
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
    network.add_node(
        "expr",
        DVec2::new(0.0, 0.0),
        num_params,
        Box::new(expr_data),
    )
}

/// Capture-freeze timing — the re-freeze ("inside the body") half of the story.
///
/// A `closure` C lives *inside* a `fold` body and captures the fold's `acc`
/// (a depth-2 `ZoneInput` capture, frozen at C's `eval`). An `apply` calls C on
/// the fold's `element`, so `new_acc = element + acc`. Because the `closure`
/// node sits in the body, it is evaluated once *per outer iteration*, re-freezing
/// the *current* `acc` each time.
///
/// fold over `[1, 2, 3]` with `init = 0`:
///   acc=0,e=1 → 1 ; acc=1,e=2 → 3 ; acc=3,e=3 → 6  ⇒ 6.
/// Were the capture frozen only *once* (at the first iteration, acc=0) the body
/// would compute `e + 0` each time ⇒ `1, 2, 3` ⇒ 3. Asserting 6 (not 3) proves
/// the re-freeze. (The "frozen once, shared" half is
/// `fold_with_closure_f_pin_captured_offset`.)
#[test]
fn closure_inside_fold_body_refreezes_capture_per_iteration() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 1, 1, 3, 0.0); // [1,2,3]
    let init_id = add_int(&mut designer, "main", 0, 80.0);
    let fold_id = designer.add_node("fold", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, fold_id, 0); // xs
    designer.connect_nodes(init_id, 0, fold_id, 1); // init

    // Add the `closure` C and the `apply` A into the fold body.
    let (c_id, a_id) = {
        let fold_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let c_id = fold_body.add_node(
            "closure",
            DVec2::new(0.0, 0.0),
            0,
            Box::new(ClosureData {
                kind: ClosureKind::Map,
                type_args: vec![DataType::Int, DataType::Int],
                param_names: vec![],
                custom_label: None,
            }),
        );
        let a_id = fold_body.add_node(
            "apply",
            DVec2::new(200.0, 0.0),
            2,
            Box::new(ApplyData {
                kind: ClosureKind::Map,
                type_args: vec![DataType::Int, DataType::Int],
                param_names: vec![],
            }),
        );
        (c_id, a_id)
    };

    // Populate C and A (refresh_args=true sizes their arg slots; ensures C's
    // empty zone is initialized).
    for nid in [c_id, a_id] {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&nid)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Build C's body: `x + acc`. `x` ← C's element (depth-1 ZoneInput); `acc` ←
    // the fold's `acc` (depth-2 ZoneInput capture, frozen per outer iteration).
    let c_expr_id = {
        let c_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let expr_id = add_expr_to_network(
            c_body,
            "x + acc",
            vec![
                ("x".to_string(), DataType::Int),
                ("acc".to_string(), DataType::Int),
            ],
        );
        c_body.nodes.get_mut(&expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: c_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        c_body.nodes.get_mut(&expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: fold_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 2,
            });
        // C's zone-output ← expr.
        let c_node = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_id)
            .unwrap();
        if c_node.zone_output_arguments.is_empty() {
            c_node.zone_output_arguments.push(Argument::new());
        }
        c_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        expr_id
    };

    // Populate C's body expr (refresh_args=false to preserve the wires above).
    {
        let registry = &mut designer.node_type_registry;
        let expr_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_expr_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            expr_node,
            false,
        );
    }

    // Wire A: f ← C (pin 0). Phase D of function-pin unification means apply
    // has *only* the `f` pin until `f` is wired and the post-pass installs
    // the arg pins. Wire `f` first, then re-run the apply post-pass on the
    // fold body (split-borrow via temporary remove/reinsert of the parent
    // network) to materialise the arg-pin layout from the wired source's
    // flat function type. With the layout in place, wire element ← the
    // fold's `element` (pin 1, depth-1).
    {
        let mut main_network = designer
            .node_type_registry
            .node_networks
            .remove("main")
            .unwrap();
        {
            let fold_body = main_network
                .nodes
                .get_mut(&fold_id)
                .unwrap()
                .zone_mut()
                .unwrap();
            fold_body.nodes.get_mut(&a_id).unwrap().arguments[0]
                .incoming_wires
                .push(IncomingWire {
                    source_node_id: c_id,
                    source_pin: SourcePin::NodeOutput { pin_index: 0 },
                    source_scope_depth: 0,
                });
            designer
                .node_type_registry
                .update_apply_pin_layouts_for_network(fold_body);
            fold_body.nodes.get_mut(&a_id).unwrap().arguments[1]
                .incoming_wires
                .push(IncomingWire {
                    source_node_id: fold_id,
                    source_pin: SourcePin::ZoneInput { pin_index: 1 },
                    source_scope_depth: 1,
                });
        }
        designer
            .node_type_registry
            .node_networks
            .insert("main".to_string(), main_network);
    }

    // fold's zone-output (new_acc) ← A.
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, a_id);

    let result = evaluate_node(&designer, "main", fold_id);
    assert_eq!(
        extract_int(result),
        6,
        "closure capturing the fold's acc must re-freeze per iteration (6, not 3)"
    );
}

/// `owner_node_id` collision regression — the load-bearing Phase 4 test.
///
/// An outer **lazy** `map` (id X) uses its inline body to map each outer element
/// `e` to the stream produced by an inner `map` whose `f` is a `closure` C. C's
/// `owner_node_id` is *forced* to collide with the outer map's id X (a `closure`
/// in a body has its own id space — here we set it explicitly). C's body returns
/// `inner_elem + e`, capturing the outer element `e` at depth 2 (frozen when C is
/// evaluated, once per outer iteration).
///
/// The outer map is drained fully *first* — collecting three inner iterators —
/// and only *then* is each inner iterator drained. So each inner stream's
/// `run_closure_once` (which pushes a frame keyed by C's owner = X) runs *after*
/// the outer map has popped its own X frames: the inner closure body resolves
/// `e` from its **frozen captures**, never from a live X frame. Were that
/// invariant to regress, the colliding inner frame would be read in place of the
/// (gone) outer frame, silently producing wrong results.
///
/// Expected: for `e ∈ [0, 1, 2]`, inner stream `[inner_elem + e for 0,1]` =
/// `[[0,1], [1,2], [2,3]]`.
#[test]
fn owner_node_id_collision_lazy_escaping_closure() {
    let mut designer = setup_designer_with_network("main");

    let outer_range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0); // [0,1,2]
    let outer_map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        outer_map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            // each outer element maps to an Iter[Int] (the inner map's stream).
            output_type: DataType::Iterator(Box::new(DataType::Int)),
        }),
    );
    designer.connect_nodes(outer_range_id, 0, outer_map_id, 0); // xs

    // Build the outer map's body: inner_range, the closure C (id forced to X),
    // and the inner map (f ← C).
    let (inner_range_id, inner_map_id) = {
        let outer_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_map_id)
            .unwrap()
            .zone_mut()
            .unwrap();

        let inner_range_id = outer_body.add_node(
            "range",
            DVec2::new(0.0, 0.0),
            3,
            Box::new(RangeData {
                start: 0,
                step: 1,
                count: 2,
            }),
        );

        // Force the next body id to the outer map's id so the closure collides.
        outer_body.next_node_id = outer_map_id;
        let c_id = outer_body.add_node(
            "closure",
            DVec2::new(150.0, 0.0),
            0,
            Box::new(ClosureData {
                kind: ClosureKind::Map,
                type_args: vec![DataType::Int, DataType::Int],
                param_names: vec![],
                custom_label: None,
            }),
        );
        assert_eq!(
            c_id, outer_map_id,
            "closure's owner id should collide with the outer map's id"
        );

        let inner_map_id = outer_body.add_node(
            "map",
            DVec2::new(300.0, 0.0),
            2,
            Box::new(MapData {
                input_type: DataType::Int,
                output_type: DataType::Int,
            }),
        );

        (inner_range_id, inner_map_id)
    };
    let c_id = outer_map_id; // forced collision

    // Populate every outer-body node (refresh_args=true; inits C's empty zone).
    for nid in [inner_range_id, c_id, inner_map_id] {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&nid)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Wire the outer body: inner_map.xs ← inner_range, inner_map.f ← C.
    {
        let outer_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        outer_body.nodes.get_mut(&inner_map_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_range_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        outer_body.nodes.get_mut(&inner_map_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: c_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Build C's body: `inner_elem + e`. `inner_elem` ← C's element (depth-1,
    // keyed by C.id == X); `e` ← the outer map's element (depth-2 capture, also
    // keyed by X but at a distinct depth — frozen at C's eval).
    let c_expr_id = {
        let c_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let expr_id = add_expr_to_network(
            c_body,
            "inner_elem + e",
            vec![
                ("inner_elem".to_string(), DataType::Int),
                ("e".to_string(), DataType::Int),
            ],
        );
        c_body.nodes.get_mut(&expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: c_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        c_body.nodes.get_mut(&expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: outer_map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 2,
            });
        // C's zone-output ← expr.
        let c_node = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_id)
            .unwrap();
        if c_node.zone_output_arguments.is_empty() {
            c_node.zone_output_arguments.push(Argument::new());
        }
        c_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        expr_id
    };

    // Populate C's body expr (refresh_args=false preserves the wires).
    {
        let registry = &mut designer.node_type_registry;
        let expr_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&c_expr_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            expr_node,
            false,
        );
    }

    // outer map's zone-output (result: Iter[Int]) ← inner_map's stream.
    wire_body_node_to_zone_output(&mut designer, "main", outer_map_id, inner_map_id);

    // Drain the outer map fully first (collecting three inner iterators), then
    // drain each — forcing the inner closures' frames to run after the outer's
    // X frames have all been popped.
    let outer_result = evaluate_node(&designer, "main", outer_map_id);
    let inner_iters = drain_iter_with_designer(&designer, outer_result);
    assert_eq!(
        inner_iters.len(),
        3,
        "outer map should yield 3 inner streams"
    );

    let got: Vec<Vec<i32>> = inner_iters
        .into_iter()
        .map(|it| extract_ints(drain_iter_with_designer(&designer, it)))
        .collect();
    assert_eq!(
        got,
        vec![vec![0, 1], vec![1, 2], vec![2, 3]],
        "escaping inner closures must read the frozen outer element, not a live colliding frame"
    );
}

// ============================================================================
// Phase 5 — validation rules
// ============================================================================

/// Recursively collect every validation-error text from `network` and every
/// nested zone body (body errors live on the body's `validation_errors`).
fn collect_all_errors(network: &NodeNetwork) -> Vec<String> {
    let mut out: Vec<String> = network
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    for node in network.nodes.values() {
        if let Some(body) = node.zone.as_ref() {
            out.extend(collect_all_errors(body));
        }
    }
    out
}

/// Validate the active network and return `(valid, all_error_texts)`.
fn validate_and_collect_errors(
    designer: &mut StructureDesigner,
    network_name: &str,
) -> (bool, Vec<String>) {
    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    (network.valid, collect_all_errors(network))
}

/// Check 1: when an HOF's `f` pin is connected, the inline zone is ignored, so
/// the "every zone-output pin needs an incoming wire" rule is **suspended** —
/// a `map` with `f` wired and an empty inline body must validate cleanly.
#[test]
fn validation_hof_f_connected_suspends_zone_output_rule() {
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
    // The map has NO inline body wired; with `f` connected the empty inline
    // body must not make the network invalid.

    let (valid, errors) = validate_and_collect_errors(&mut designer, "main");
    assert!(
        valid,
        "map with `f` connected and an empty inline body should be valid; got errors: {:?}",
        errors
    );
}

/// Check 2: a `closure` node whose `result` zone-output pin has no incoming
/// wire is invalid — the closure doesn't deliver its result. (The closure has
/// no `f` *input* pin, so the suspension above never applies to it.)
#[test]
fn validation_closure_body_incomplete_rejected() {
    let mut designer = setup_designer_with_network("main");

    let closure_id = designer.add_node("closure", DVec2::new(150.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );
    // No body wiring — the closure's `result` zone-output pin has no incoming
    // wire.

    let (valid, errors) = validate_and_collect_errors(&mut designer, "main");
    assert!(!valid, "a closure with no zone-output wire must be invalid");
    assert!(
        errors.iter().any(|e| {
            let l = e.to_lowercase();
            l.contains("zone-output") && l.contains("no incoming wire")
        }),
        "expected a missing-zone-output-wire error on the closure; got: {:?}",
        errors
    );
}

/// Check 4: unlike an HOF (whose disconnected `f` falls back to the inline
/// body), `apply` has no body, so a disconnected required `f` pin is a
/// validation error attributed to the `apply` node.
#[test]
fn validation_apply_disconnected_f_rejected() {
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
            param_names: vec![],
        }),
    );
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element only; `f` left open

    let (valid, errors) = validate_and_collect_errors(&mut designer, "main");
    assert!(!valid, "apply with a disconnected `f` must be invalid");
    assert!(
        errors.iter().any(|e| {
            let l = e.to_lowercase();
            l.contains("apply") && l.contains("f") && l.contains("not connected")
        }),
        "expected an apply-`f`-not-connected error; got: {:?}",
        errors
    );
}

/// Check 3 (arity): a wrong-arity closure wired into the `f` pin of an
/// exact-arity HOF (filter / fold / foreach) is rejected by the ordinary
/// wire type-compatibility check. A map-kind closure `(Int) -> Int` does not
/// fit `fold`'s `(Int, Int) -> Int` `f` pin.
///
/// Note: `map.f` *no longer* requires exact arity after Currying Phase 4
/// (`doc/design_currying.md` §"HOF auto-partialization"). Higher-arity
/// sources flow into `map.f` via the starts-with rule and the excess
/// parameters become a partial-application tail. The other three HOFs keep
/// exact-arity `f` pins because their output types are constrained.
#[test]
fn validation_wrong_arity_closure_into_f_rejected() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let init_id = add_int(&mut designer, "main", 0, -300.0);
    let map_closure_id = add_int_map_closure(&mut designer, "main", "x + 1", "x", None, -150.0);

    let fold_id = designer.add_node("fold", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, fold_id, 0); // xs
    designer.connect_nodes(init_id, 0, fold_id, 1); // init
    designer.connect_nodes(map_closure_id, 0, fold_id, 2); // f — arity 1 vs expected 2

    let (valid, errors) = validate_and_collect_errors(&mut designer, "main");
    assert!(
        !valid,
        "a wrong-arity closure wired into fold.f must be rejected"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.to_lowercase().contains("data type mismatch")),
        "expected a data-type-mismatch error for the wrong-arity `f` wire; got: {:?}",
        errors
    );
}

/// Check 3 (leaf type): a closure whose return type is incompatible with an
/// exact-arity HOF's expected return is rejected. A map-kind closure
/// `(Int) -> Int` does not fit `filter`'s `(Int) -> Bool` `f` pin.
///
/// Note: after Currying Phase 4, `map.output_type` derives from the wired
/// `f` source, so a `(Int) -> Bool` source into `map.f` is *accepted* (map
/// retypes to `Iter[Bool]`). The leaf-return constraint survives on the
/// three exact-arity HOFs (`filter`/`fold`/`foreach`).
#[test]
fn validation_type_incompatible_closure_into_f_rejected() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 6, 0.0);
    let map_closure_id = add_int_map_closure(&mut designer, "main", "x + 1", "x", None, -150.0);

    let filter_id = designer.add_node("filter", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        filter_id,
        Box::new(
            rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData {
                element_type: DataType::Int,
            },
        ),
    );
    designer.connect_nodes(range_id, 0, filter_id, 0); // xs
    designer.connect_nodes(map_closure_id, 0, filter_id, 1); // f — returns Int, not Bool

    let (valid, errors) = validate_and_collect_errors(&mut designer, "main");
    assert!(
        !valid,
        "a closure with an incompatible return type wired into filter.f must be rejected"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.to_lowercase().contains("data type mismatch")),
        "expected a data-type-mismatch error for the incompatible `f` wire; got: {:?}",
        errors
    );
}

// ============================================================================
// `ClosureKind::Custom` — user-authored function signatures
// ============================================================================

/// `Custom` closure: arity-3 `(Float, Int, Bool) -> Vec3` round-trips through
/// `calculate_custom_node_type` to the expected zone pins / external output.
#[test]
fn custom_kind_closure_calculate_node_type_arity3() {
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::structure_designer::nodes::closure::get_node_type;

    let data = ClosureData {
        kind: ClosureKind::Custom,
        type_args: vec![
            DataType::Float,
            DataType::Int,
            DataType::Bool,
            DataType::Vec3,
        ],
        param_names: vec!["x".into(), "n".into(), "flag".into()],
        custom_label: None,
    };
    let base: NodeType = get_node_type();
    let custom = data
        .calculate_custom_node_type(&base)
        .expect("Custom kind must produce a custom NodeType");

    // External: zero ordinary input pins, one Function output pin.
    assert_eq!(custom.parameters.len(), 0);
    assert_eq!(custom.output_pins.len(), 1);
    assert_eq!(
        custom.output_type(),
        &DataType::Function(FunctionType {
            parameter_types: vec![DataType::Float, DataType::Int, DataType::Bool,],
            output_type: Box::new(DataType::Vec3),
        }),
    );

    // Zone-input pins: one per authored parameter, with authored name + type.
    assert_eq!(custom.zone_input_pins.len(), 3);
    assert_eq!(custom.zone_input_pins[0].name, "x");
    assert_eq!(custom.zone_input_pins[1].name, "n");
    assert_eq!(custom.zone_input_pins[2].name, "flag");
    assert_eq!(
        custom.zone_input_pins[0].fixed_type(),
        Some(&DataType::Float)
    );
    assert_eq!(custom.zone_input_pins[1].fixed_type(), Some(&DataType::Int));
    assert_eq!(
        custom.zone_input_pins[2].fixed_type(),
        Some(&DataType::Bool)
    );

    // Zone-output: a single `result` pin with the authored return type.
    assert_eq!(custom.zone_output_pins.len(), 1);
    assert_eq!(custom.zone_output_pins[0].name, "result");
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Vec3);
}

/// `Custom` closure with arity 1.
#[test]
fn custom_kind_closure_calculate_node_type_arity1() {
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::structure_designer::nodes::closure::get_node_type;

    let data = ClosureData {
        kind: ClosureKind::Custom,
        type_args: vec![DataType::Int, DataType::Float],
        param_names: vec!["only".into()],
        custom_label: None,
    };
    let base: NodeType = get_node_type();
    let custom = data.calculate_custom_node_type(&base).unwrap();

    assert_eq!(custom.zone_input_pins.len(), 1);
    assert_eq!(custom.zone_input_pins[0].name, "only");
    assert_eq!(custom.zone_input_pins[0].fixed_type(), Some(&DataType::Int));
    assert_eq!(custom.zone_output_pins.len(), 1);
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Float);
}

/// `Custom` `apply`: with `f` disconnected the disconnected-`f` default is
/// **only the `f` pin** — no arg pins materialise until `f` is wired (the
/// post-pass derives them from the wired source's flat function type).
/// `ApplyData.kind` / `type_args` / `param_names` stay on disk for `.cnnd`
/// back-compat but are structurally irrelevant here. See
/// `doc/design_function_pin_unification.md` (Phase D, "Apply node UX").
#[test]
fn custom_kind_apply_calculate_node_type_arity2() {
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::structure_designer::nodes::apply::get_node_type;

    let data = ApplyData {
        kind: ClosureKind::Custom,
        type_args: vec![DataType::Int, DataType::Float, DataType::Bool],
        param_names: vec!["lhs".into(), "rhs".into()],
    };
    let base: NodeType = get_node_type();
    let custom = data.calculate_custom_node_type(&base).unwrap();

    // Disconnected-`f` default: only the `f` pin renders.
    assert_eq!(custom.parameters.len(), 1);
    assert_eq!(custom.parameters[0].name, "f");
    assert_eq!(
        custom.parameters[0].data_type,
        DataType::AnyFunction {
            leading_params: vec![],
        },
    );

    // Output is unknown until `f` is wired — `DataType::None` placeholder.
    assert_eq!(custom.output_pins.len(), 1);
    assert_eq!(custom.output_type(), &DataType::None);

    // No zone pins on `apply`.
    assert_eq!(custom.zone_input_pins.len(), 0);
    assert_eq!(custom.zone_output_pins.len(), 0);
}

/// End-to-end Custom-kind: a `closure` of `Custom (Int) -> Int` wired into an
/// `apply` of the same `Custom` shape runs the body once. This exercises the
/// shared `ZoneClosure` payload through Custom-kind nodes.
#[test]
fn custom_kind_apply_runs_custom_closure_once() {
    let mut designer = setup_designer_with_network("main");

    // closure: Custom (Int) -> Int, body x + 1, with element-name "x".
    let closure_id = designer.add_node("closure", DVec2::new(150.0, -120.0));
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec!["x".into()],
            custom_label: None,
        }),
    );
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", closure_id, 0, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", closure_id, expr_id);

    let arg_id = add_int(&mut designer, "main", 10, 0.0);
    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec!["x".into()],
        }),
    );
    designer.connect_nodes(closure_id, 0, apply_id, 0); // f
    designer.connect_nodes(arg_id, 0, apply_id, 1); // x

    let result = evaluate_node(&designer, "main", apply_id);
    assert_eq!(extract_int(result), 11);
}

/// `Custom` closure body wires that no longer make sense after a param is
/// removed are disconnected by `repair_node_network`. We start with a 2-param
/// Custom closure whose body uses both zone-input pins, then drop the second
/// param (arity 2 → 1) and re-validate / repair.
#[test]
fn custom_kind_repair_on_param_remove() {
    use rust_lib_flutter_cad::structure_designer::node_network::SourcePin;

    let mut designer = setup_designer_with_network("main");

    // closure: Custom (Int, Int) -> Int.
    let closure_id = designer.add_node("closure", DVec2::new(150.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int, DataType::Int],
            param_names: vec!["a".into(), "b".into()],
            custom_label: None,
        }),
    );
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "a + b",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_to_body_node(&mut designer, "main", closure_id, 0, expr_id, 0);
    wire_zone_input_to_body_node(&mut designer, "main", closure_id, 1, expr_id, 1);
    wire_body_node_to_zone_output(&mut designer, "main", closure_id, expr_id);

    // Sanity: body has a wire from zone-input pin 1.
    {
        let registry = &designer.node_type_registry;
        let body = registry
            .node_networks
            .get("main")
            .unwrap()
            .nodes
            .get(&closure_id)
            .unwrap()
            .zone
            .as_ref()
            .unwrap();
        let body_expr = body.nodes.get(&expr_id).unwrap();
        assert!(
            body_expr.arguments[1]
                .incoming_wires
                .iter()
                .any(|w| matches!(w.source_pin, SourcePin::ZoneInput { pin_index: 1 }))
        );
    }

    // Drop the second param: arity 2 → 1.
    set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec!["a".into()],
            custom_label: None,
        }),
    );
    // Trigger the repair pass directly. (The production refresh path runs
    // `repair_node_network` on every mutator; these tests build by hand so we
    // invoke it explicitly.)
    {
        let registry = &mut designer.node_type_registry;
        let mut network = registry.node_networks.remove("main").unwrap();
        registry.repair_node_network(&mut network);
        registry.node_networks.insert("main".to_string(), network);
    }

    // Now the body wire from the (gone) zone-input pin 1 must be removed.
    let registry = &designer.node_type_registry;
    let body = registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&closure_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap();
    let body_expr = body.nodes.get(&expr_id).unwrap();
    let has_stale_wire = body_expr.arguments.iter().any(|arg| {
        arg.incoming_wires.iter().any(|w| {
            matches!(
                w.source_pin,
                SourcePin::ZoneInput { pin_index } if pin_index >= 1,
            )
        })
    });
    assert!(
        !has_stale_wire,
        "body wires from removed zone-input pin 1 must be repaired away"
    );
}

/// Serde round-trip: `ClosureData` (and `ApplyData`) with `Custom` kind keep
/// their `param_names` through `serde_json::to_value`/`from_value`.
#[test]
fn custom_kind_cnnd_roundtrip() {
    use serde_json;

    let orig = ClosureData {
        kind: ClosureKind::Custom,
        type_args: vec![DataType::Float, DataType::Int, DataType::Bool],
        param_names: vec!["alpha".into(), "beta".into()],
        custom_label: None,
    };
    let v = serde_json::to_value(&orig).unwrap();
    let back: ClosureData = serde_json::from_value(v).unwrap();
    assert!(matches!(back.kind, ClosureKind::Custom));
    assert_eq!(back.type_args, orig.type_args);
    assert_eq!(back.param_names, orig.param_names);

    let orig_apply = ApplyData {
        kind: ClosureKind::Custom,
        type_args: vec![DataType::Int, DataType::Float],
        param_names: vec!["only".into()],
    };
    let v = serde_json::to_value(&orig_apply).unwrap();
    let back: ApplyData = serde_json::from_value(v).unwrap();
    assert!(matches!(back.kind, ClosureKind::Custom));
    assert_eq!(back.type_args, orig_apply.type_args);
    assert_eq!(back.param_names, orig_apply.param_names);
}

/// `#[serde(default)]` keeps older `.cnnd` files (which lack `param_names`)
/// loadable: a JSON value with only `kind` + `type_args` deserializes to a
/// preset-shape `ClosureData` / `ApplyData` with `param_names == vec![]`.
#[test]
fn cnnd_back_compat_loads_old_closure_data() {
    use serde_json::json;

    // The exact encoding of `DataType` is opaque to this test, so build the
    // type_args sub-value through serde itself rather than spelling it out.
    let closure_type_args = serde_json::to_value(vec![DataType::Int, DataType::Float]).unwrap();
    let old_closure = json!({
        "kind": "Map",
        "type_args": closure_type_args,
    });
    let back: ClosureData = serde_json::from_value(old_closure).unwrap();
    assert!(matches!(back.kind, ClosureKind::Map));
    assert_eq!(back.type_args, vec![DataType::Int, DataType::Float]);
    assert!(back.param_names.is_empty());

    let apply_type_args = serde_json::to_value(vec![DataType::Bool]).unwrap();
    let old_apply = json!({
        "kind": "Filter",
        "type_args": apply_type_args,
    });
    let back: ApplyData = serde_json::from_value(old_apply).unwrap();
    assert!(matches!(back.kind, ClosureKind::Filter));
    assert_eq!(back.type_args, vec![DataType::Bool]);
    assert!(back.param_names.is_empty());
}

/// Helper-method shape: switching a preset's `type_args` into a `Custom`
/// shape via `[params..., return]` reproduces the same param/return types
/// the preset would compute.
#[test]
fn preset_to_custom_data_preservation() {
    // From `Fold` `[A, T]` (return derived = A), the Custom encoding is
    // `[A, T, A]` with names `["acc", "element"]`. Both kinds should yield
    // the same param types and return type when fed their respective args.
    let names = vec!["acc".to_string(), "element".to_string()];
    let preset_args = vec![DataType::Float, DataType::Int];
    let preset_params = ClosureKind::Fold.param_types(&preset_args, &[]);
    let preset_ret = ClosureKind::Fold.return_type(&preset_args, &[]);

    let custom_args = vec![
        preset_params[0].clone(),
        preset_params[1].clone(),
        preset_ret.clone(),
    ];
    let custom_params = ClosureKind::Custom.param_types(&custom_args, &names);
    let custom_ret = ClosureKind::Custom.return_type(&custom_args, &names);

    assert_eq!(preset_params, custom_params);
    assert_eq!(preset_ret, custom_ret);
    assert_eq!(custom_params, vec![DataType::Float, DataType::Int]);
    assert_eq!(custom_ret, DataType::Float);

    // From `Map` `[T, U]`: params `[T]`, return `U` (free), Custom encoding
    // `[T, U]` with name `["element"]`.
    let names = vec!["element".to_string()];
    let preset_args = vec![DataType::Int, DataType::Float];
    let preset_params = ClosureKind::Map.param_types(&preset_args, &[]);
    let preset_ret = ClosureKind::Map.return_type(&preset_args, &[]);
    let custom_args = vec![preset_params[0].clone(), preset_ret.clone()];
    let custom_params = ClosureKind::Custom.param_types(&custom_args, &names);
    let custom_ret = ClosureKind::Custom.return_type(&custom_args, &names);
    assert_eq!(preset_params, custom_params);
    assert_eq!(preset_ret, custom_ret);

    // From `Filter` `[T]` (fixed `Bool` return): Custom encoding `[T, Bool]`.
    let names = vec!["element".to_string()];
    let preset_args = vec![DataType::Int];
    let preset_params = ClosureKind::Filter.param_types(&preset_args, &[]);
    let preset_ret = ClosureKind::Filter.return_type(&preset_args, &[]);
    let custom_args = vec![preset_params[0].clone(), preset_ret.clone()];
    let custom_params = ClosureKind::Custom.param_types(&custom_args, &names);
    let custom_ret = ClosureKind::Custom.return_type(&custom_args, &names);
    assert_eq!(preset_params, custom_params);
    assert_eq!(preset_ret, custom_ret);
    assert_eq!(custom_ret, DataType::Bool);

    // From `Foreach` `[T]` (fixed `Unit` return): Custom encoding `[T, Unit]`.
    let names = vec!["element".to_string()];
    let preset_args = vec![DataType::Int];
    let preset_params = ClosureKind::Foreach.param_types(&preset_args, &[]);
    let preset_ret = ClosureKind::Foreach.return_type(&preset_args, &[]);
    let custom_args = vec![preset_params[0].clone(), preset_ret.clone()];
    let custom_params = ClosureKind::Custom.param_types(&custom_args, &names);
    let custom_ret = ClosureKind::Custom.return_type(&custom_args, &names);
    assert_eq!(preset_params, custom_params);
    assert_eq!(preset_ret, custom_ret);
    assert_eq!(custom_ret, DataType::Unit);
}

// ============================================================================
// Issue #326 - copy/paste of zone-bearing nodes (closure) + apply
// ============================================================================

/// Helper: among a freshly pasted id set, find the one whose node type matches.
fn find_pasted(
    designer: &StructureDesigner,
    network: &str,
    new_ids: &[u64],
    node_type: &str,
) -> u64 {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    new_ids
        .iter()
        .copied()
        .find(|id| net.nodes.get(id).unwrap().node_type_name == node_type)
        .unwrap_or_else(|| panic!("no pasted `{node_type}` node found"))
}

/// Regression for issue #326. Copy/paste of a `closure` (a zone-bearing node)
/// must remap the body's `ZoneInput` iteration-value wire from the OLD closure
/// id to the NEW pasted id. Before the fix the body's `element` wire kept the
/// old id, so evaluating the pasted `apply` (which runs the closure body)
/// panicked with "current_zone_input: no scope-stack entry for HOF id N".
#[test]
fn paste_closure_and_apply_remaps_zone_input_and_evaluates() {
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
            param_names: vec![],
        }),
    );
    designer.connect_nodes(closure_id, 0, apply_id, 0); // f
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element

    // Sanity: the original evaluates fine.
    assert_eq!(extract_int(evaluate_node(&designer, "main", apply_id)), 11);

    // Select everything and round-trip through the clipboard.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_nodes(vec![closure_id, arg_id, apply_id]);
    }
    assert!(designer.copy_selection());
    let new_ids = designer.paste_at_position(DVec2::new(0.0, 400.0));
    assert_eq!(new_ids.len(), 3);

    let pasted_closure = find_pasted(&designer, "main", &new_ids, "closure");
    let pasted_apply = find_pasted(&designer, "main", &new_ids, "apply");

    // The pasted closure's body `element` wire must point at the PASTED closure
    // id, not the original - this is the core of the fix.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let body = network
            .nodes
            .get(&pasted_closure)
            .unwrap()
            .zone
            .as_ref()
            .expect("pasted closure missing body");
        let zone_input_wire = body
            .nodes
            .values()
            .flat_map(|n| n.arguments.iter())
            .flat_map(|a| a.incoming_wires.iter())
            .find(|w| {
                matches!(w.source_pin, SourcePin::ZoneInput { .. }) && w.source_scope_depth == 1
            })
            .expect("body has no depth-1 ZoneInput wire");
        assert_eq!(
            zone_input_wire.source_node_id, pasted_closure,
            "ZoneInput wire was not remapped to the pasted closure id"
        );
        assert_ne!(
            zone_input_wire.source_node_id, closure_id,
            "ZoneInput wire still references the original closure id"
        );
    }

    // Evaluating the pasted apply must not panic and must give the same answer.
    assert_eq!(
        extract_int(evaluate_node(&designer, "main", pasted_apply)),
        11
    );
}

/// The cut path (copy + delete) must also round-trip: cutting the closure +
/// apply and pasting must produce a working, evaluable copy.
#[test]
fn cut_then_paste_closure_and_apply_evaluates() {
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
            param_names: vec![],
        }),
    );
    designer.connect_nodes(closure_id, 0, apply_id, 0);
    designer.connect_nodes(arg_id, 0, apply_id, 1);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_nodes(vec![closure_id, arg_id, apply_id]);
    }
    assert!(designer.cut_selection());

    // Originals gone.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        assert!(!network.nodes.contains_key(&closure_id));
        assert!(!network.nodes.contains_key(&apply_id));
    }

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 400.0));
    assert_eq!(new_ids.len(), 3);
    let pasted_apply = find_pasted(&designer, "main", &new_ids, "apply");
    assert_eq!(
        extract_int(evaluate_node(&designer, "main", pasted_apply)),
        11
    );
}

/// A capture wire (depth-1 `NodeOutput`) into a closure body must also follow
/// the id remap when the captured source node is part of the same paste. The
/// closure captures `k = 5` and adds it; `apply(f, 10)` must still yield `15`
/// after the paste.
#[test]
fn paste_closure_with_capture_remaps_capture_wire() {
    let mut designer = setup_designer_with_network("main");

    let k_id = add_int(&mut designer, "main", 5, -240.0);
    let closure_id = add_int_map_closure(
        &mut designer,
        "main",
        "x + k",
        "x",
        Some(("k", k_id)),
        -120.0,
    );
    let arg_id = add_int(&mut designer, "main", 10, 0.0);

    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
        }),
    );
    designer.connect_nodes(closure_id, 0, apply_id, 0);
    designer.connect_nodes(arg_id, 0, apply_id, 1);

    assert_eq!(extract_int(evaluate_node(&designer, "main", apply_id)), 15);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_nodes(vec![k_id, closure_id, arg_id, apply_id]);
    }
    assert!(designer.copy_selection());
    let new_ids = designer.paste_at_position(DVec2::new(0.0, 400.0));
    assert_eq!(new_ids.len(), 4);

    let pasted_closure = find_pasted(&designer, "main", &new_ids, "closure");
    let pasted_apply = find_pasted(&designer, "main", &new_ids, "apply");

    // The body's depth-1 NodeOutput capture must point at a pasted node, not
    // the original `k`.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let body = network
            .nodes
            .get(&pasted_closure)
            .unwrap()
            .zone
            .as_ref()
            .unwrap();
        let capture_wire = body
            .nodes
            .values()
            .flat_map(|n| n.arguments.iter())
            .flat_map(|a| a.incoming_wires.iter())
            .find(|w| {
                matches!(w.source_pin, SourcePin::NodeOutput { .. }) && w.source_scope_depth == 1
            })
            .expect("body has no depth-1 capture wire");
        assert_ne!(
            capture_wire.source_node_id, k_id,
            "capture wire still references the original k node"
        );
        assert!(
            new_ids.contains(&capture_wire.source_node_id),
            "capture wire must reference a pasted node"
        );
    }

    assert_eq!(
        extract_int(evaluate_node(&designer, "main", pasted_apply)),
        15
    );
}
