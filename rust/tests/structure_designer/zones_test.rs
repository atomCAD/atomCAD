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
