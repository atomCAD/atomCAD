//! Tests for *Convert to Closure* (Network → Closure), Phase 1 (top level).
//!
//! See `doc/design_closure_network_conversion.md` (Direction A). The conversion
//! replaces a custom-network instance node `I` (used through its function pin,
//! or unconsumed) with a `closure` node `C` whose inline body copies `I`'s
//! network `N`: wired pins become captures, unwired pins become closure
//! parameters. `C` reuses `I`'s id; consumers of `I`'s `-1` pin are flipped to
//! `C`'s pin `0`.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{
    IncomingWire, NodeDisplayState, NodeDisplayType, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
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

fn add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    let id = designer.add_node("int", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

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

fn add_expr(
    designer: &mut StructureDesigner,
    network: &str,
    expression: &str,
    parameters: Vec<(&str, DataType)>,
    y: f64,
) -> u64 {
    let expr_params: Vec<ExprParameter> = parameters
        .into_iter()
        .map(|(name, dt)| ExprParameter {
            id: None,
            name: name.to_string(),
            data_type: dt,
            data_type_str: None,
        })
        .collect();
    let mut expr_data = ExprData {
        parameters: expr_params,
        expression: expression.to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = expr_data.parse_and_validate(0);
    let id = designer.add_node("expr", DVec2::new(150.0, y));
    set_node_data(designer, network, id, Box::new(expr_data));
    id
}

fn add_map(
    designer: &mut StructureDesigner,
    network: &str,
    input_type: DataType,
    output_type: DataType,
    y: f64,
) -> u64 {
    let id = designer.add_node("map", DVec2::new(350.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(MapData {
            input_type,
            output_type,
        }),
    );
    id
}

/// Wire `source_node`'s function pin (`-1`) into `dest_node`'s argument.
fn wire_function_pin(
    designer: &mut StructureDesigner,
    network: &str,
    source_node_id: u64,
    dest_node_id: u64,
    dest_arg_index: usize,
) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    let dest = net.nodes.get_mut(&dest_node_id).unwrap();
    dest.arguments[dest_arg_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: -1 },
            source_scope_depth: 0,
        });
}

/// Build and register a custom network `name` whose interface is `params`
/// (name, type) and whose single `expr` return node computes `expression` over
/// those params (each parameter node wired into the expr at its index). Leaves
/// `main` active on return (assumes the caller set it up first).
fn build_expr_network(
    designer: &mut StructureDesigner,
    name: &str,
    params: &[(&str, DataType)],
    expression: &str,
    set_return: bool,
) {
    designer.add_node_network(name);
    designer.set_active_node_network_name(Some(name.to_string()));

    let mut param_ids = Vec::new();
    for (i, (pname, ptype)) in params.iter().enumerate() {
        let pid = designer.add_node("parameter", DVec2::new(0.0, i as f64 * 80.0));
        designer.set_node_network_data(
            pid,
            Box::new(ParameterData {
                param_id: None,
                param_index: i,
                param_name: pname.to_string(),
                data_type: ptype.clone(),
                sort_order: i as i32,
                data_type_str: None,
                error: None,
            }),
        );
        param_ids.push(pid);
    }

    let expr_id = add_expr(designer, name, expression, params.to_vec(), 0.0);
    for (i, pid) in param_ids.iter().enumerate() {
        designer.connect_nodes(*pid, 0, expr_id, i);
    }
    if set_return {
        designer.set_return_node_id(Some(expr_id));
    }
    designer.validate_active_network();

    designer.set_active_node_network_name(Some("main".to_string()));
}

/// The `closure` node's `ClosureData` for `node_id` in `network`.
fn closure_data<'a>(
    designer: &'a StructureDesigner,
    network: &str,
    node_id: u64,
) -> &'a ClosureData {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("node is not a closure")
}

fn node_type_name(designer: &StructureDesigner, network: &str, node_id: u64) -> String {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .node_type_name
        .clone()
}

// ============================================================================
// Network → Closure
// ============================================================================

/// Basic: an instance of a 1-param network, pin unwired → a `Custom` closure
/// with one named zone-input pin; the downstream `map.f` yields the same stream
/// before and after conversion; the consumer wire flips `-1 → 0`.
#[test]
fn convert_basic_one_param_unwired() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let inst_id = designer.add_node("inc", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", inst_id, map_id, 1); // map.f ← inc.fn

    // Baseline: instance-as-function over [0,1,2] → [1,2,3].
    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![1, 2, 3]);

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

    // The node became a closure with one named parameter `x`.
    assert_eq!(node_type_name(&designer, "main", inst_id), "closure");
    let cd = closure_data(&designer, "main", inst_id);
    assert_eq!(cd.kind, ClosureKind::Custom);
    assert_eq!(cd.param_names, vec!["x".to_string()]);
    assert_eq!(cd.type_args, vec![DataType::Int, DataType::Int]);

    // The consumer wire flipped from the function pin to pin 0.
    let map_wire = &designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .arguments[1]
        .incoming_wires[0];
    assert_eq!(map_wire.source_node_id, inst_id);
    assert_eq!(map_wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });

    // Same stream after conversion.
    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![1, 2, 3]);
}

/// Mixed pins: a 2-param network (`a + b`) with pin `a` wired to a constant and
/// pin `b` unwired → closure with one zone-input param (`b`) and one capture
/// wire (depth 1) for `a`. Evaluation matches.
#[test]
fn convert_mixed_wired_and_unwired() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(
        &mut designer,
        "add2",
        &[("a", DataType::Int), ("b", DataType::Int)],
        "a + b",
        true,
    );

    let const_id = add_int(&mut designer, "main", 10, -240.0);
    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let inst_id = designer.add_node("add2", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(const_id, 0, inst_id, 0); // a ← const 10 (capture)
    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", inst_id, map_id, 1); // map.f ← add2.fn

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![10, 11, 12]);

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

    // Closure has a single zone-input parameter `b`.
    let cd = closure_data(&designer, "main", inst_id);
    assert_eq!(cd.param_names, vec!["b".to_string()]);
    assert_eq!(cd.type_args, vec![DataType::Int, DataType::Int]);

    // Body: the expr's arg 0 (`a`) is a capture wire (depth 1) onto the const;
    // arg 1 (`b`) is a ZoneInput wire to the closure (depth 1).
    let body = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&inst_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap();
    let expr_node = body
        .nodes
        .values()
        .find(|n| n.node_type_name == "expr")
        .expect("body should contain the expr");
    let cap_wire = &expr_node.arguments[0].incoming_wires[0];
    assert_eq!(cap_wire.source_scope_depth, 1, "capture wire at depth 1");
    assert_eq!(cap_wire.source_node_id, const_id);
    assert_eq!(cap_wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    let param_wire = &expr_node.arguments[1].incoming_wires[0];
    assert_eq!(param_wire.source_scope_depth, 1);
    assert_eq!(param_wire.source_node_id, inst_id);
    assert_eq!(param_wire.source_pin, SourcePin::ZoneInput { pin_index: 0 });

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![10, 11, 12]);
}

/// Multi-param, all unwired → a `Custom` closure preserving parameter
/// names/order. (`a - b` over a fold-shaped 2-arg call, but here exercised via
/// `apply`-free structural assertion + a single-arg-free eval is awkward, so we
/// only assert the shape.)
#[test]
fn convert_multi_param_all_unwired() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(
        &mut designer,
        "sub3",
        &[
            ("a", DataType::Int),
            ("b", DataType::Int),
            ("c", DataType::Int),
        ],
        "a - b - c",
        true,
    );

    let inst_id = designer.add_node("sub3", DVec2::new(150.0, 0.0));
    // Unconsumed instance — allowed by the gate.
    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

    let cd = closure_data(&designer, "main", inst_id);
    assert_eq!(cd.kind, ClosureKind::Custom);
    assert_eq!(
        cd.param_names,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(
        cd.type_args,
        vec![DataType::Int, DataType::Int, DataType::Int, DataType::Int]
    );
}

/// Passthrough return, unwired pin: the network just forwards its parameter
/// (`return` node *is* the parameter). The closure's zone-output wire becomes a
/// `ZoneInput` at depth 1, and the function value is the identity over the
/// stream.
#[test]
fn convert_passthrough_return_unwired() {
    let mut designer = setup_designer_with_network("main");

    // Build an `id` network whose return node is its parameter node directly.
    designer.add_node_network("idnet");
    designer.set_active_node_network_name(Some("idnet".to_string()));
    let pid = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        pid,
        Box::new(ParameterData {
            param_id: None,
            param_index: 0,
            param_name: "x".to_string(),
            data_type: DataType::Int,
            sort_order: 0,
            data_type_str: None,
            error: None,
        }),
    );
    designer.set_return_node_id(Some(pid));
    designer.validate_active_network();
    designer.set_active_node_network_name(Some("main".to_string()));

    let range_id = add_range(&mut designer, "main", 5, 1, 3, 0.0); // [5,6,7]
    let inst_id = designer.add_node("idnet", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_function_pin(&mut designer, "main", inst_id, map_id, 1);

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![5, 6, 7]);

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

    // The zone-output wire reads the closure's ZoneInput param 0 (depth 1).
    let closure_node = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&inst_id)
        .unwrap();
    let zo_wire = &closure_node.zone_output_arguments[0].incoming_wires[0];
    assert_eq!(zo_wire.source_node_id, inst_id);
    assert_eq!(zo_wire.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(zo_wire.source_scope_depth, 1);

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![5, 6, 7]);
}

/// Unconsumed instance with pin 0 displayed: the conversion succeeds and the
/// stale displayed-pin entry is cleared (a `Function`-valued pin renders
/// nothing).
#[test]
fn convert_unconsumed_clears_display() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let inst_id = designer.add_node("inc", DVec2::new(150.0, 0.0));
    // Mark the instance's pin 0 displayed.
    designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .displayed_nodes
        .insert(
            inst_id,
            NodeDisplayState::with_type(NodeDisplayType::Normal),
        );

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

    assert_eq!(node_type_name(&designer, "main", inst_id), "closure");
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap()
            .displayed_nodes
            .contains_key(&inst_id),
        "stale display state should be cleared"
    );
}

/// Two consumers of the instance's function pin (two `map.f` sinks) both flip
/// `-1 → 0`.
#[test]
fn convert_two_consumers_both_flip() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let range_a = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let range_b = add_range(&mut designer, "main", 10, 1, 2, 200.0);
    let inst_id = designer.add_node("inc", DVec2::new(150.0, -120.0));
    let map_a = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    let map_b = add_map(&mut designer, "main", DataType::Int, DataType::Int, 200.0);

    designer.connect_nodes(range_a, 0, map_a, 0);
    designer.connect_nodes(range_b, 0, map_b, 0);
    wire_function_pin(&mut designer, "main", inst_id, map_a, 1);
    wire_function_pin(&mut designer, "main", inst_id, map_b, 1);

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

    for map_id in [map_a, map_b] {
        let wire = &designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap()
            .nodes
            .get(&map_id)
            .unwrap()
            .arguments[1]
            .incoming_wires[0];
        assert_eq!(wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    }

    assert_eq!(
        extract_ints(drain_iter_with_designer(
            &designer,
            evaluate_node(&designer, "main", map_a)
        )),
        vec![1, 2, 3]
    );
    assert_eq!(
        extract_ints(drain_iter_with_designer(
            &designer,
            evaluate_node(&designer, "main", map_b)
        )),
        vec![11, 12]
    );
}

/// A consumer of the instance's `-1` pin living inside a sibling HOF body
/// (`source_scope_depth == 1`) is flipped to pin `0` at the same depth — the
/// recursive consumer walk, not just the host frame.
#[test]
fn convert_consumer_in_sibling_body_flips_at_depth() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let inst_id = designer.add_node("inc", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);

    // Inject a body node into map's zone and wire its arg to the instance's
    // function pin at depth 1 (a capture of the function value into the body).
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let map_node = net.nodes.get_mut(&map_id).unwrap();
        let body = map_node.zone_mut().unwrap();
        let body_node_id = body.add_node(
            "apply",
            DVec2::new(50.0, 0.0),
            1,
            Box::new(rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData::default()),
        );
        body.nodes.get_mut(&body_node_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inst_id,
                source_pin: SourcePin::NodeOutput { pin_index: -1 },
                source_scope_depth: 1,
            });
    }

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");

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
    let apply_node = body
        .nodes
        .values()
        .find(|n| n.node_type_name == "apply")
        .unwrap();
    let wire = &apply_node.arguments[0].incoming_wires[0];
    assert_eq!(wire.source_node_id, inst_id);
    assert_eq!(wire.source_scope_depth, 1, "still at depth 1");
    assert_eq!(
        wire.source_pin,
        SourcePin::NodeOutput { pin_index: 0 },
        "flipped -1 → 0 inside the body"
    );
}

// ----------------------------------------------------------------------------
// Reject cases
// ----------------------------------------------------------------------------

/// A non-custom node (a built-in `int`) cannot be converted.
#[test]
fn convert_rejects_non_custom_node() {
    let mut designer = setup_designer_with_network("main");
    let int_id = add_int(&mut designer, "main", 5, 0.0);
    let err = designer
        .convert_instance_to_closure(vec![], int_id)
        .unwrap_err();
    assert!(err.contains("custom node instance"), "got: {err}");
}

/// An instance used as a *value* (a normal-output consumer) is rejected.
#[test]
fn convert_rejects_value_consumer() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let src = add_int(&mut designer, "main", 7, 0.0);
    let inst_id = designer.add_node("inc", DVec2::new(150.0, 0.0));
    designer.connect_nodes(src, 0, inst_id, 0); // x ← 7
    // Consume the instance's normal output pin 0 as a value.
    let sink = add_map(&mut designer, "main", DataType::Int, DataType::Int, 200.0);
    // Wire instance pin 0 into the map's xs (broadcast scalar → iter).
    designer.connect_nodes(inst_id, 0, sink, 0);

    let err = designer
        .convert_instance_to_closure(vec![], inst_id)
        .unwrap_err();
    assert!(err.contains("used as a value"), "got: {err}");
}

/// A custom network with no return node cannot deliver a closure result.
#[test]
fn convert_rejects_no_return_node() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(
        &mut designer,
        "noret",
        &[("x", DataType::Int)],
        "x + 1",
        false, // do not set a return node
    );

    let inst_id = designer.add_node("noret", DVec2::new(150.0, 0.0));
    let err = designer
        .convert_instance_to_closure(vec![], inst_id)
        .unwrap_err();
    assert!(err.contains("no return node"), "got: {err}");
}

/// Undo/redo round-trip: the host network is byte-identical after undo, and the
/// conversion re-applies on redo.
#[test]
fn convert_undo_redo_round_trip() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let inst_id = designer.add_node("inc", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_function_pin(&mut designer, "main", inst_id, map_id, 1);

    assert_eq!(node_type_name(&designer, "main", inst_id), "inc");

    designer
        .convert_instance_to_closure(vec![], inst_id)
        .expect("conversion should succeed");
    assert_eq!(node_type_name(&designer, "main", inst_id), "closure");

    designer.undo();
    assert_eq!(
        node_type_name(&designer, "main", inst_id),
        "inc",
        "undo restores the instance"
    );

    designer.redo();
    assert_eq!(
        node_type_name(&designer, "main", inst_id),
        "closure",
        "redo re-applies the conversion"
    );
    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![1, 2, 3]);
}
