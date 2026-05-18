//! Unit tests for the Phase 4 zones implementation of `map`.
//!
//! These tests exercise `Walker::MapZone` end-to-end against bodies
//! constructed via direct manipulation of the HOF node's owned
//! `NodeNetwork`. The text-format syntax for zones doesn't exist yet, so
//! tests reach into the API to build bodies.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, IncomingWire, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::foreach::ForeachData;
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

/// Drain a freshly evaluated walker against a real designer's registry so
/// per-element body evaluations can see all node types. (Walker::next needs
/// a registry to evaluate body nodes like `expr`.)
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
    panic!("drain exceeded cap of {} elements", cap);
}

fn extract_ints(values: Vec<NetworkResult>) -> Vec<i32> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Int(v) => v,
            NetworkResult::Error(msg) => panic!("expected Int element, got Error: {}", msg),
            other => panic!(
                "expected Int element, got {}",
                other.to_display_string()
            ),
        })
        .collect()
}

/// Add an `expr` node to a body network at a given position.
///
/// Returns the new node's id (in the body's local id space).
fn add_expr_to_body(
    designer: &mut StructureDesigner,
    map_network: &str,
    map_node_id: u64,
    expression: &str,
    parameters: Vec<(String, DataType)>,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(map_network).unwrap();
    let map_node = network.nodes.get_mut(&map_node_id).unwrap();
    let body = map_node.zone_mut().expect("map node missing zone");

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
    // Direct API path bypasses the text-format validator that normally
    // parses the expression — run parse_and_validate explicitly so the
    // expr node has its parsed AST set.
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
            .get_mut(map_network)
            .unwrap()
            .nodes
            .get_mut(&map_node_id)
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

/// Wire the map's inside-facing `element` zone-input pin into one of the body
/// node's argument pins.
fn wire_zone_input_to_body_node(
    designer: &mut StructureDesigner,
    map_network: &str,
    map_node_id: u64,
    body_node_id: u64,
    body_param_index: usize,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(map_network)
        .unwrap();
    let map_node = network.nodes.get_mut(&map_node_id).unwrap();
    let body = map_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: map_node_id,
            source_pin: SourcePin::ZoneInput { pin_index: 0 },
            source_scope_depth: 1,
        });
}

/// Wire an outer-scope node (capture) into one of the body node's argument
/// pins. The map node sits in `map_network`; the source node is in the same
/// network as the map node (depth 1 from the body's perspective).
fn wire_capture_to_body_node(
    designer: &mut StructureDesigner,
    map_network: &str,
    map_node_id: u64,
    body_node_id: u64,
    body_param_index: usize,
    source_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(map_network)
        .unwrap();
    let map_node = network.nodes.get_mut(&map_node_id).unwrap();
    let body = map_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        });
}

/// Wire a body node into the map's `result` zone-output pin (index 0).
fn wire_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    map_network: &str,
    map_node_id: u64,
    body_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(map_network)
        .unwrap();
    let map_node = network.nodes.get_mut(&map_node_id).unwrap();
    if map_node.zone_output_arguments.is_empty() {
        map_node.zone_output_arguments.push(Argument::new());
    }
    map_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Generalized version of `wire_zone_input_to_body_node` that lets the caller
/// pick which zone-input pin to read from. Useful for `fold` (acc=0,
/// element=1) and for nested zones.
fn wire_zone_input_pin_to_body_node(
    designer: &mut StructureDesigner,
    hof_network: &str,
    hof_node_id: u64,
    zone_input_pin: usize,
    body_node_id: u64,
    body_param_index: usize,
    source_scope_depth: u8,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(hof_network)
        .unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    let body = hof_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: hof_node_id,
            source_pin: SourcePin::ZoneInput {
                pin_index: zone_input_pin,
            },
            source_scope_depth,
        });
}

/// Wire an outer-scope node (or even outer-outer) into one of the body
/// node's argument pins with a custom `source_scope_depth`. Used for nested
/// HOFs where the source lives in an ancestor network.
#[allow(dead_code)]
fn wire_capture_to_body_node_at_depth(
    designer: &mut StructureDesigner,
    hof_network: &str,
    hof_node_id: u64,
    body_node_id: u64,
    body_param_index: usize,
    source_node_id: u64,
    source_scope_depth: u8,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(hof_network)
        .unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    let body = hof_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth,
        });
}

/// Add an `expr` node directly into a (nested) body network. Used when the
/// body itself is owned by an HOF whose zone we've already drilled into.
fn add_expr_to_network(
    network: &mut rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork,
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

fn drain_iter_to_ints(designer: &StructureDesigner, result: NetworkResult) -> Vec<i32> {
    extract_ints(drain_iter_with_designer(designer, result))
}

fn evaluate_with_execute(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    execute: bool,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.execute = execute;
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

// ============================================================================
// Tests
// ============================================================================

/// `range(3) → map(zone: element + 1) → ...` — exercises the basic
/// MapZone walker, the per-element push/pop of the zone-input frame, and the
/// body's ZoneInput-wire lookup against the live scope-stack.
#[test]
fn map_zone_trivial_element_plus_one() {
    let mut designer = setup_designer_with_network("main");

    // range(3) — emits Int(0), Int(1), Int(2).
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    // map(input=Int, output=Int) wired to xs=range.
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    // Body: expr "x + 1" with parameter `x: Int`.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );

    // Wire `element` zone-input pin → expr.x (parameter 0).
    wire_zone_input_to_body_node(&mut designer, "main", map_id, expr_id, 0);
    // Wire expr → map's `result` zone-output pin.
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![1, 2, 3]);
}

/// Capture: the body reads `k = int(5)` from the outer network. Verifies
/// capture pre-evaluation at body entry — `k` is evaluated once and the
/// cached value used for every iteration.
#[test]
fn map_zone_capture_outer_constant() {
    let mut designer = setup_designer_with_network("main");

    // k = int(5) in the outer network.
    let k_id = designer.add_node("int", DVec2::new(0.0, -100.0));
    set_node_data(&mut designer, "main", k_id, Box::new(IntData { value: 5 }));

    // range(3) — emits 0, 1, 2.
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    // map(Int → Int).
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    // Body: expr "x + k" with two parameters; x from zone-input, k captured
    // from outer scope.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + k",
        vec![
            ("x".to_string(), DataType::Int),
            ("k".to_string(), DataType::Int),
        ],
    );

    wire_zone_input_to_body_node(&mut designer, "main", map_id, expr_id, 0);
    wire_capture_to_body_node(&mut designer, "main", map_id, expr_id, 1, k_id);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![5, 6, 7]);
}

/// Walker clone independence: cloning the walker (e.g. when fanning to two
/// consumers) must yield walkers that advance independently. Invariant 2 in
/// the design.
#[test]
fn map_zone_walker_clone_advances_independently() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 4,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x * 10",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", map_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    let result = evaluate_node(&designer, "main", map_id);
    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        other => panic!("expected Iterator, got {}", other.to_display_string()),
    };

    let evaluator = NetworkEvaluator::new();
    let registry = &designer.node_type_registry;
    let mut ctx = NetworkEvaluationContext::new();

    // Advance original by 2 — yields 0, 10.
    assert!(matches!(
        walker.next(&evaluator, registry, &mut ctx),
        Some(NetworkResult::Int(0))
    ));
    assert!(matches!(
        walker.next(&evaluator, registry, &mut ctx),
        Some(NetworkResult::Int(10))
    ));

    let mut clone = walker.clone();

    // Drain the clone — should yield 20, 30 (continues from where original
    // was). Clone shares the body Arc but has its own iteration state via
    // the source walker's per-walker `idx`.
    let mut clone_out = Vec::new();
    while let Some(v) = clone.next(&evaluator, registry, &mut ctx) {
        clone_out.push(v);
    }
    let clone_ints = extract_ints(clone_out);
    assert_eq!(clone_ints, vec![20, 30]);

    // Original is independent — it should still yield 20, 30.
    let mut orig_out = Vec::new();
    while let Some(v) = walker.next(&evaluator, registry, &mut ctx) {
        orig_out.push(v);
    }
    let orig_ints = extract_ints(orig_out);
    assert_eq!(orig_ints, vec![20, 30]);
}

/// Body with no incoming wire on the `result` zone-output pin returns an
/// Error at eval time. (Phase 6 will surface this at validation time too.)
#[test]
fn map_zone_empty_body_yields_error_at_eval() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    // No body wiring at all — the map's zone-output pin has zero incoming
    // wires.
    let result = evaluate_node(&designer, "main", map_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("zone-output")
                    || msg.to_lowercase().contains("missing")
                    || msg.to_lowercase().contains("no incoming"),
                "expected an error about a missing zone-output wire, got: {msg}"
            );
        }
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

/// Sanity check that the map node type declares the new zone pin shape.
#[test]
fn map_registers_zone_pins() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("map").unwrap();
    assert_eq!(nt.parameters.len(), 1, "map should have only `xs` externally");
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.zone_input_pins.len(), 1);
    assert_eq!(nt.zone_input_pins[0].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "result");
}

/// MapData::calculate_custom_node_type specializes zone pin types correctly.
#[test]
fn map_calculate_custom_node_type_int_to_float() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("map").unwrap();
    let data = MapData {
        input_type: DataType::Int,
        output_type: DataType::Float,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(custom.parameters.len(), 1);
    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Iterator(Box::new(DataType::Float))
    );

    assert_eq!(custom.zone_input_pins.len(), 1);
    assert_eq!(custom.zone_input_pins[0].name, "element");
    assert_eq!(
        custom.zone_input_pins[0].fixed_type(),
        Some(&DataType::Int)
    );

    assert_eq!(custom.zone_output_pins.len(), 1);
    assert_eq!(custom.zone_output_pins[0].name, "result");
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Float);
}

// ============================================================================
// filter — zone-based predicate
// ============================================================================

/// `range(10) → filter(zone: element % 2 == 0) → drain` yields `[0, 2, 4, 6, 8]`.
#[test]
fn filter_zone_keeps_even_elements() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 10,
        }),
    );

    let filter_id = designer.add_node("filter", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        filter_id,
        Box::new(FilterData {
            element_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, filter_id, 0);

    // Body: expr "x % 2 == 0" with parameter `x: Int`.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        filter_id,
        "x % 2 == 0",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", filter_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", filter_id, expr_id);

    let result = evaluate_node(&designer, "main", filter_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![0, 2, 4, 6, 8]);
}

/// Filter with always-false body produces an empty stream.
#[test]
fn filter_zone_always_false_yields_empty() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 5,
        }),
    );

    let filter_id = designer.add_node("filter", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        filter_id,
        Box::new(FilterData {
            element_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, filter_id, 0);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        filter_id,
        "false",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", filter_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", filter_id, expr_id);

    let result = evaluate_node(&designer, "main", filter_id);
    let elements = drain_iter_with_designer(&designer, result);
    assert_eq!(elements.len(), 0, "expected empty stream, got {} elements", elements.len());
}

/// Filter that captures an outer-scope threshold value.
#[test]
fn filter_zone_capture_outer_threshold() {
    let mut designer = setup_designer_with_network("main");

    let threshold_id = designer.add_node("int", DVec2::new(0.0, -100.0));
    set_node_data(
        &mut designer,
        "main",
        threshold_id,
        Box::new(IntData { value: 3 }),
    );

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 5,
        }),
    );

    let filter_id = designer.add_node("filter", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        filter_id,
        Box::new(FilterData {
            element_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, filter_id, 0);

    // Body: expr "x > threshold" with two params, threshold captured.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        filter_id,
        "x > threshold",
        vec![
            ("x".to_string(), DataType::Int),
            ("threshold".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_to_body_node(&mut designer, "main", filter_id, expr_id, 0);
    wire_capture_to_body_node(&mut designer, "main", filter_id, expr_id, 1, threshold_id);
    wire_body_node_to_zone_output(&mut designer, "main", filter_id, expr_id);

    let result = evaluate_node(&designer, "main", filter_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![4, 5]);
}

#[test]
fn filter_registers_zone_pins() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("filter").unwrap();
    assert_eq!(nt.parameters.len(), 1, "filter should have only `xs` externally");
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.zone_input_pins.len(), 1);
    assert_eq!(nt.zone_input_pins[0].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "keep");
    assert_eq!(nt.zone_output_pins[0].data_type, DataType::Bool);
}

// ============================================================================
// fold — eager zone-based reduction
// ============================================================================

/// `range(1..5) → fold(zone: acc + element, init=0) → 1+2+3+4 = 10`.
#[test]
fn fold_zone_sum_range() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 4,
        }),
    );

    let init_id = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "main", init_id, Box::new(IntData { value: 0 }));

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
    designer.connect_nodes(range_id, 0, fold_id, 0);
    designer.connect_nodes(init_id, 0, fold_id, 1);

    // Body: expr "acc + elem" with two params; acc from zone-input pin 0,
    // elem from pin 1.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + elem",
        vec![
            ("acc".to_string(), DataType::Int),
            ("elem".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, expr_id, 0, 1);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, expr_id, 1, 1);
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, expr_id);

    let result = evaluate_node(&designer, "main", fold_id);
    match result {
        NetworkResult::Int(v) => assert_eq!(v, 10),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

/// `fold` with a captured initial offset: body computes `acc + elem + k`.
#[test]
fn fold_zone_sum_with_captured_offset() {
    let mut designer = setup_designer_with_network("main");

    let k_id = designer.add_node("int", DVec2::new(0.0, -100.0));
    set_node_data(&mut designer, "main", k_id, Box::new(IntData { value: 10 }));

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 3,
        }),
    );

    let init_id = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(&mut designer, "main", init_id, Box::new(IntData { value: 0 }));

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
    designer.connect_nodes(range_id, 0, fold_id, 0);
    designer.connect_nodes(init_id, 0, fold_id, 1);

    // Body: expr "acc + elem + k" — 3 params. k is captured.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + elem + k",
        vec![
            ("acc".to_string(), DataType::Int),
            ("elem".to_string(), DataType::Int),
            ("k".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, expr_id, 0, 1);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, expr_id, 1, 1);
    wire_capture_to_body_node(&mut designer, "main", fold_id, expr_id, 2, k_id);
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, expr_id);

    // Result: 0 + (1+10) = 11, 11 + (2+10) = 23, 23 + (3+10) = 36.
    let result = evaluate_node(&designer, "main", fold_id);
    match result {
        NetworkResult::Int(v) => assert_eq!(v, 36),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

/// fold over an empty stream returns `init` unchanged.
#[test]
fn fold_zone_empty_stream_returns_init() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 0,
        }),
    );

    let init_id = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "main",
        init_id,
        Box::new(IntData { value: 42 }),
    );

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
    designer.connect_nodes(range_id, 0, fold_id, 0);
    designer.connect_nodes(init_id, 0, fold_id, 1);

    // Body never fires, but must still be present and well-formed.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + elem",
        vec![
            ("acc".to_string(), DataType::Int),
            ("elem".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, expr_id, 0, 1);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, expr_id, 1, 1);
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, expr_id);

    let result = evaluate_node(&designer, "main", fold_id);
    match result {
        NetworkResult::Int(v) => assert_eq!(v, 42),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

#[test]
fn fold_registers_zone_pins() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("fold").unwrap();
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "init");
    assert_eq!(nt.zone_input_pins.len(), 2);
    assert_eq!(nt.zone_input_pins[0].name, "acc");
    assert_eq!(nt.zone_input_pins[1].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "new_acc");
}

// ============================================================================
// foreach — Unit-returning side-effect HOF
// ============================================================================

/// Display pass on a `foreach` over a huge range short-circuits via the
/// central skip rule — `eval` never runs.
#[test]
fn foreach_zone_display_pass_short_circuits() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 1_000_000,
        }),
    );

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        foreach_id,
        Box::new(ForeachData {
            input_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    // Body: expr "elem * 2" — wire it through. Never gets called on display.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        foreach_id,
        "elem * 2",
        vec![("elem".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", foreach_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", foreach_id, expr_id);

    let started = std::time::Instant::now();
    let result = evaluate_with_execute(&designer, "main", foreach_id, false);
    let elapsed = started.elapsed();
    assert!(matches!(result, NetworkResult::Unit));
    assert!(
        elapsed.as_millis() < 100,
        "display pass took {:?} — central skip rule did not short-circuit",
        elapsed
    );
}

/// Execute pass drains all elements; body fires but its return value is
/// discarded into Unit.
#[test]
fn foreach_zone_execute_drains_all() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 5,
        }),
    );

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        foreach_id,
        Box::new(ForeachData {
            input_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        foreach_id,
        "elem + 1",
        vec![("elem".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", foreach_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", foreach_id, expr_id);

    let exec = evaluate_with_execute(&designer, "main", foreach_id, true);
    assert!(matches!(exec, NetworkResult::Unit));
}

#[test]
fn foreach_registers_zone_pins() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("foreach").unwrap();
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.zone_input_pins.len(), 1);
    assert_eq!(nt.zone_input_pins[0].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "out");
    assert_eq!(nt.zone_output_pins[0].data_type, DataType::Unit);
}

// ============================================================================
// Nested HOFs: outer fold over [1,2,3] whose body contains an inner fold
// capturing a parent-scope constant.
// ============================================================================

/// Match the worked example in design_zones.md §"Worked example — when do
/// captures fire?": outer fold over a range; the inner fold's body captures
/// a parent-scope constant `K`. Capture pre-evaluation happens at the
/// granularity of inner-fold-eval invocation, which is once per outer
/// iteration.
///
/// Inner fold: range(1..3) summed into `acc + elem * K` ⇒ result depends on
/// K. We assert the final outer-fold accumulator over 3 outer iterations.
#[test]
fn nested_fold_inner_captures_outer_constant() {
    let mut designer = setup_designer_with_network("main");

    // K = int(2) in the outer scope.
    let k_id = designer.add_node("int", DVec2::new(0.0, -200.0));
    set_node_data(&mut designer, "main", k_id, Box::new(IntData { value: 2 }));

    // Outer range(1..3) — emits [1, 2, 3].
    let outer_range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        outer_range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 3,
        }),
    );

    let outer_init_id = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "main",
        outer_init_id,
        Box::new(IntData { value: 0 }),
    );

    // Outer fold: Int element, Int accumulator.
    let outer_fold_id = designer.add_node("fold", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        outer_fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(outer_range_id, 0, outer_fold_id, 0);
    designer.connect_nodes(outer_init_id, 0, outer_fold_id, 1);

    // Inner fold lives inside the outer fold's zone body. Range and init for
    // the inner fold also live in the outer body.
    //
    // We build all the inner-body nodes here imperatively because they need
    // to live inside the outer fold's zone.
    let (inner_range_id, inner_init_id, inner_fold_id) = {
        let registry = &mut designer.node_type_registry;
        let outer_fold_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap();
        let outer_body = outer_fold_node.zone_mut().unwrap();

        // Inner range(1..3): emits [1, 2, 3]. range has 3 declared input
        // pins (start, step, count) — `num_params` here must match.
        let inner_range_id = outer_body.add_node(
            "range",
            DVec2::new(0.0, 0.0),
            3,
            Box::new(RangeData {
                start: 1,
                step: 1,
                count: 3,
            }),
        );

        let inner_init_id = outer_body.add_node(
            "int",
            DVec2::new(0.0, 100.0),
            0,
            Box::new(IntData { value: 0 }),
        );

        let inner_fold_id = outer_body.add_node(
            "fold",
            DVec2::new(200.0, 0.0),
            2,
            Box::new(FoldData {
                element_type: DataType::Int,
                accumulator_type: DataType::Int,
            }),
        );

        (inner_range_id, inner_init_id, inner_fold_id)
    };

    // Repopulate caches for the new body nodes.
    for nid in [inner_range_id, inner_init_id, inner_fold_id] {
        let registry = &mut designer.node_type_registry;
        let body_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
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
            body_node,
            true,
        );
    }

    // Wire inner_range → inner_fold.xs, inner_init → inner_fold.init.
    // These are local wires inside the outer body.
    {
        let outer_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        outer_body.nodes.get_mut(&inner_fold_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_range_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        outer_body.nodes.get_mut(&inner_fold_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_init_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Build the inner fold's body: expr "acc + elem * K" with 3 params.
    // K is captured from the outer scope (depth 2 from the inner body).
    let inner_expr_id = {
        let inner_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();

        add_expr_to_network(
            inner_body,
            "acc + elem * K",
            vec![
                ("acc".to_string(), DataType::Int),
                ("elem".to_string(), DataType::Int),
                ("K".to_string(), DataType::Int),
            ],
        )
    };

    // Repopulate the inner expr's cache.
    {
        let registry = &mut designer.node_type_registry;
        let inner_expr_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_expr_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            inner_expr_node,
            true,
        );
    }

    // Wire the inner expr:
    //  - param 0 (acc) ← inner_fold's zone-input pin 0 (depth 1).
    //  - param 1 (elem) ← inner_fold's zone-input pin 1 (depth 1).
    //  - param 2 (K) ← outer-scope k_id (depth 2 — captures cross the outer
    //    fold's body too).
    //  - inner_fold zone-output pin 0 ← inner_expr.
    {
        let inner_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        inner_body.nodes.get_mut(&inner_expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_fold_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        inner_body.nodes.get_mut(&inner_expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_fold_id,
                source_pin: SourcePin::ZoneInput { pin_index: 1 },
                source_scope_depth: 1,
            });
        // K is two levels up: through inner body → outer body → main.
        inner_body.nodes.get_mut(&inner_expr_id).unwrap().arguments[2]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: k_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 2,
            });
    }

    // Inner fold zone-output: inner_expr → new_acc.
    {
        let inner_fold_node = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_fold_id)
            .unwrap();
        if inner_fold_node.zone_output_arguments.is_empty() {
            inner_fold_node.zone_output_arguments.push(Argument::new());
        }
        inner_fold_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Outer fold's body: an expr that just passes inner_fold's result
    // through, ignoring outer iteration values (the inner fold's value
    // already incorporates K, and we want to assert "inner fold computed
    // correctly per outer iteration"). Outer body: `inner_result + outer_acc`.
    let outer_expr_id = add_expr_to_body(
        &mut designer,
        "main",
        outer_fold_id,
        "outer_acc + inner_result",
        vec![
            ("outer_acc".to_string(), DataType::Int),
            ("inner_result".to_string(), DataType::Int),
        ],
    );

    // Wire outer_expr.outer_acc ← outer fold's zone-input pin 0.
    wire_zone_input_pin_to_body_node(
        &mut designer,
        "main",
        outer_fold_id,
        0,
        outer_expr_id,
        0,
        1,
    );
    // Wire outer_expr.inner_result ← inner_fold (pin 0) — local in the outer
    // body.
    {
        let outer_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        outer_body.nodes.get_mut(&outer_expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_fold_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }
    wire_body_node_to_zone_output(&mut designer, "main", outer_fold_id, outer_expr_id);

    // Expected: inner fold of [1,2,3] with K=2 = 0 + 1*2 + 2*2 + 3*2 = 12.
    // Outer fold over [1,2,3]: each outer iteration adds 12 to outer_acc:
    //   iter 1: 0 + 12 = 12
    //   iter 2: 12 + 12 = 24
    //   iter 3: 24 + 12 = 36
    let result = evaluate_node(&designer, "main", outer_fold_id);
    match result {
        NetworkResult::Int(v) => assert_eq!(v, 36),
        other => panic!("expected Int(36), got {}", other.to_display_string()),
    }
}

// ============================================================================
// Chained pipeline: range → filter → map → drain
// ============================================================================

/// `range(10) → filter(zone: x % 2 == 0) → map(zone: x * 10) → drain` yields
/// `[0, 20, 40, 60, 80]`. Exercises lazy-walker chaining where each HOF's
/// captures are pre-evaluated exactly once during its own `eval`.
#[test]
fn chained_range_filter_map_drain() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 10,
        }),
    );

    let filter_id = designer.add_node("filter", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        filter_id,
        Box::new(FilterData {
            element_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, filter_id, 0);

    let filter_expr = add_expr_to_body(
        &mut designer,
        "main",
        filter_id,
        "x % 2 == 0",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", filter_id, filter_expr, 0);
    wire_body_node_to_zone_output(&mut designer, "main", filter_id, filter_expr);

    let map_id = designer.add_node("map", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(filter_id, 0, map_id, 0);

    let map_expr = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x * 10",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", map_id, map_expr, 0);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, map_expr);

    let result = evaluate_node(&designer, "main", map_id);
    let elements = drain_iter_to_ints(&designer, result);
    assert_eq!(elements, vec![0, 20, 40, 60, 80]);
}

// ============================================================================
// Scope-stack regression: hof_node_id collision between outer fold and
// inner map (zones inhabiting different network scopes share numeric ids
// because `next_node_id` is per-network).
// ============================================================================

/// Force the inner `map`'s node id to collide with the outer `fold`'s id by
/// constructing the inner network with control over its `next_node_id`. Under
/// scope-stack semantics, each outer iteration sees its own `acc` frame even
/// though the inner walker's `next()` repeatedly pushes/pops a frame on the
/// same `hof_id` key. Without scope-stack semantics this test silently
/// produces wrong totals.
///
/// The outer body computes `acc + inner_count` where `inner_count` is the
/// length of `range → map(x → x + acc)` — i.e. the inner map reads outer
/// fold's `acc` via a depth-2 ZoneInput capture, while the inner map itself
/// shares the outer fold's numeric node id.
#[test]
fn nested_fold_with_inner_map_id_collision() {
    let mut designer = setup_designer_with_network("main");

    let outer_range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        outer_range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 3,
        }),
    );
    let outer_init_id = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "main",
        outer_init_id,
        Box::new(IntData { value: 0 }),
    );
    let outer_fold_id = designer.add_node("fold", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        outer_fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(outer_range_id, 0, outer_fold_id, 0);
    designer.connect_nodes(outer_init_id, 0, outer_fold_id, 1);

    // Inside the outer fold's body, build:
    //   inner_range (range 0..2)
    //   inner_map (map; reads element and outer's acc via depth-2 capture)
    //   collect (consume the map into Array[Int])
    //   outer_expr (acc + collect.length())
    use rust_lib_flutter_cad::structure_designer::nodes::array_len::ArrayLenData;
    use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;

    let (inner_range_id, inner_map_id, collect_id, array_len_id, outer_expr_id) = {
        let outer_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();

        // Inner range emits Int(0), Int(1). range has 3 declared input pins.
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

        // Force the inner map's numeric id to equal `outer_fold_id`. The
        // outer body is a fresh empty network whose `next_node_id` starts at
        // 1, so we just set it directly to the target value.
        outer_body.next_node_id = outer_fold_id;

        let inner_map_id = outer_body.add_node(
            "map",
            DVec2::new(200.0, 0.0),
            1,
            Box::new(MapData {
                input_type: DataType::Int,
                output_type: DataType::Int,
            }),
        );
        assert_eq!(
            inner_map_id, outer_fold_id,
            "inner map id should collide with outer fold's id"
        );

        let collect_id = outer_body.add_node(
            "collect",
            DVec2::new(400.0, 0.0),
            3, // iter, limit, offset
            Box::new(CollectData {
                element_type: DataType::Int,
                limit: None,
                offset: 0,
            }),
        );

        let array_len_id = outer_body.add_node(
            "array_len",
            DVec2::new(600.0, 0.0),
            1,
            Box::new(ArrayLenData {
                element_type: DataType::Int,
            }),
        );

        let outer_expr_id = add_expr_to_network(
            outer_body,
            "outer_acc + n",
            vec![
                ("outer_acc".to_string(), DataType::Int),
                ("n".to_string(), DataType::Int),
            ],
        );

        (
            inner_range_id,
            inner_map_id,
            collect_id,
            array_len_id,
            outer_expr_id,
        )
    };

    // Populate caches for every node in the outer body so zones are
    // initialized and custom-type caches are set up.
    for nid in [inner_range_id, inner_map_id, collect_id, array_len_id, outer_expr_id] {
        let registry = &mut designer.node_type_registry;
        let body_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
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
            body_node,
            true,
        );
    }

    // Now wire up the outer body's nodes (inner_map.xs ← inner_range, etc.)
    {
        let outer_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
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
        outer_body.nodes.get_mut(&collect_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_map_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        outer_body.nodes.get_mut(&array_len_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: collect_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        outer_body.nodes.get_mut(&outer_expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: array_len_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Build the inner_map's body: an expr that reads `elem` and outer
    // fold's `acc` (depth-2 ZoneInput capture). Result: `elem + acc`. The
    // inner_map's zone is initialized by the populate call above.
    let inner_expr_id = {
        let inner_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let inner_expr_id = add_expr_to_network(
            inner_body,
            "elem + acc",
            vec![
                ("elem".to_string(), DataType::Int),
                ("acc".to_string(), DataType::Int),
            ],
        );
        inner_body.nodes.get_mut(&inner_expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        // Capture outer fold's `acc` (zone-input pin 0) at depth 2.
        inner_body.nodes.get_mut(&inner_expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: outer_fold_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 2,
            });
        inner_expr_id
    };

    // inner_map zone-output ← inner_expr.
    {
        let inner_map_node = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_map_id)
            .unwrap();
        if inner_map_node.zone_output_arguments.is_empty() {
            inner_map_node.zone_output_arguments.push(Argument::new());
        }
        inner_map_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Populate the inner_expr's custom-type cache (it was added without it).
    {
        let registry = &mut designer.node_type_registry;
        let inner_expr_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer_fold_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&inner_expr_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            inner_expr_node,
            false, // refresh_args=false: preserve our wired args
        );
    }

    // Wire outer fold's body: outer_expr.outer_acc ← zone-input pin 0;
    // outer_expr → zone-output.
    wire_zone_input_pin_to_body_node(
        &mut designer,
        "main",
        outer_fold_id,
        0,
        outer_expr_id,
        0,
        1,
    );
    wire_body_node_to_zone_output(&mut designer, "main", outer_fold_id, outer_expr_id);

    // Expected:
    // Each outer iteration: inner map produces [0+acc, 1+acc] of length 2.
    // The outer body computes outer_acc + 2 each iteration.
    // 0 → 2 → 4 → 6.
    let result = evaluate_node(&designer, "main", outer_fold_id);
    match result {
        NetworkResult::Int(v) => assert_eq!(
            v, 6,
            "scope-stack id-collision regression: each outer iteration must add 2 (inner map yields a 2-element stream)"
        ),
        other => panic!("expected Int(6), got {}", other.to_display_string()),
    }
}

// ============================================================================
// Phase 6 — zone validation rules
// ============================================================================

/// Helper — run validation on the active network and return whether it's
/// valid, along with the collected validation errors.
fn validate_and_get_errors(
    designer: &mut StructureDesigner,
    network_name: &str,
) -> (bool, Vec<String>) {
    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let errors: Vec<String> = network
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    (network.valid, errors)
}

/// Helper — recursively collect every validation error text from the given
/// network and all nested zone bodies. Used when a body-internal wire fails
/// validation but the body's errors live on `body.validation_errors` (not
/// on the top-level network).
fn collect_all_errors(network: &rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork) -> Vec<String> {
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

/// Rule 1: a `map` whose `result` zone-output pin has no incoming wire is
/// invalid after validation, and the validation error mentions the missing
/// zone-output wire.
#[test]
fn validation_rule1_zone_output_pin_missing_wire() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    // No body wiring — the map's `result` zone-output pin has no incoming
    // wire. Validation should report rule 1 violation.
    let (valid, errors) = validate_and_get_errors(&mut designer, "main");
    assert!(!valid, "Network should be invalid (missing zone-output wire)");
    assert!(
        errors.iter().any(|e| e.to_lowercase().contains("zone-output")
            && e.to_lowercase().contains("no incoming wire")),
        "Expected an error about a missing zone-output wire on `result`; got: {:?}",
        errors
    );
}

/// Rule 2: a body wire with `source_scope_depth > 0` referencing a node id
/// that doesn't exist in the ancestor network is invalid.
#[test]
fn validation_rule2_capture_wire_missing_source() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    // Body: expr "x + k" with x from zone-input, k from a capture pointing
    // at a non-existent ancestor node id.
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + k",
        vec![
            ("x".to_string(), DataType::Int),
            ("k".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_to_body_node(&mut designer, "main", map_id, expr_id, 0);
    // Capture wire pointing at a node id that doesn't exist anywhere.
    wire_capture_to_body_node(&mut designer, "main", map_id, expr_id, 1, 99_999);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let all_errors = collect_all_errors(network);
    assert!(!network.valid, "Network with bad capture wire should be invalid");
    assert!(
        all_errors.iter().any(|e| {
            let lower = e.to_lowercase();
            lower.contains("capture wire")
                && (lower.contains("non-existent") || lower.contains("references"))
        }),
        "Expected a capture-wire error about a non-existent source node; got: {:?}",
        all_errors
    );
}

/// Rule 3: a ZoneInput wire whose `pin_index` exceeds the source HOF's
/// declared zone-input pin count is invalid.
#[test]
fn validation_rule3_zone_input_pin_index_out_of_range() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );
    // Wire expr.x to map's ZoneInput with an out-of-range pin index.
    // map declares 1 zone-input pin ("element"), so pin_index 5 is invalid.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let map_node = network.nodes.get_mut(&map_id).unwrap();
        let body = map_node.zone_mut().unwrap();
        let body_node = body.nodes.get_mut(&expr_id).unwrap();
        body_node.arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 5 },
                source_scope_depth: 1,
            });
    }
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let all_errors = collect_all_errors(network);
    assert!(!network.valid, "Network with bad ZoneInput pin_index should be invalid");
    assert!(
        all_errors.iter().any(|e| {
            let lower = e.to_lowercase();
            lower.contains("zoneinput")
                && (lower.contains("out of range") || lower.contains("pin_index"))
        }),
        "Expected a ZoneInput out-of-range error; got: {:?}",
        all_errors
    );
}

/// Rule 3 variant: ZoneInput wire with depth=0 (sibling reference, not
/// allowed — must reference an enclosing HOF, not a sibling).
#[test]
fn validation_rule3_zone_input_depth_zero_rejected() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );
    // Wire expr.x to a ZoneInput with depth 0 — illegal (must be >= 1).
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let map_node = network.nodes.get_mut(&map_id).unwrap();
        let body = map_node.zone_mut().unwrap();
        let body_node = body.nodes.get_mut(&expr_id).unwrap();
        body_node.arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let all_errors = collect_all_errors(network);
    assert!(!network.valid, "ZoneInput at depth=0 must be rejected");
    assert!(
        all_errors.iter().any(|e| {
            let lower = e.to_lowercase();
            lower.contains("zoneinput") && lower.contains("source_scope_depth")
        }),
        "Expected a ZoneInput depth error; got: {:?}",
        all_errors
    );
}

/// A well-formed `map` body should validate cleanly under the new rules.
#[test]
fn validation_well_formed_map_passes() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", map_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    let (valid, errors) = validate_and_get_errors(&mut designer, "main");
    assert!(valid, "Well-formed map body should validate; got errors: {:?}", errors);
}

// ============================================================================
// Phase 6 — repair: zone-input pin type changes disconnect body wires
// ============================================================================

/// When a `map` node's `input_type` changes from `Int` to `Crystal`, body
/// wires that read `ZoneInput { 0 }` into a destination pin declared as
/// `Int` become incompatible — `repair_node_network` should disconnect them.
#[test]
fn repair_disconnects_body_wire_when_zone_input_type_changes() {
    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );

    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
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

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        map_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_to_body_node(&mut designer, "main", map_id, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", map_id, expr_id);

    // Sanity: the body wire we just added is present.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap()
            .nodes
            .get(&map_id)
            .unwrap()
            .zone
            .as_ref()
            .unwrap();
        let body_node = body.nodes.get(&expr_id).unwrap();
        assert_eq!(
            body_node.arguments[0].incoming_wires.len(),
            1,
            "body wire should be present before retyping"
        );
    }

    // Now flip the map's input_type to Crystal — incompatible with Int.
    // Use the same set_node_data path the other tests use; it re-populates
    // the custom-node-type cache so the new zone_input_pins type lands.
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Crystal,
            output_type: DataType::Int,
        }),
    );

    // Trigger repair by calling `repair_node_network` directly via the
    // split-borrow pattern (the same pattern structure_designer uses
    // internally to avoid a double mutable borrow of the registry).
    {
        let mut network = designer
            .node_type_registry
            .node_networks
            .remove("main")
            .unwrap();
        designer.node_type_registry.repair_node_network(&mut network);
        designer
            .node_type_registry
            .node_networks
            .insert("main".to_string(), network);
    }

    // After repair: the ZoneInput wire (Crystal source → Int destination)
    // should be gone.
    let body = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap();
    let body_node = body.nodes.get(&expr_id).unwrap();
    assert!(
        body_node.arguments[0].incoming_wires.is_empty(),
        "body wire should have been disconnected by repair (Crystal→Int incompatible); \
         remaining wires: {:?}",
        body_node.arguments[0].incoming_wires
    );
}
