//! Tests for *Convert to Closure* (Network → Closure), Phase 1 (top level).
//!
//! See `doc/design_closure_network_conversion.md` (Direction A). The conversion
//! replaces a custom-network instance node `I` (used through its function pin,
//! or unconsumed) with a `closure` node `C` whose inline body copies `I`'s
//! network `N`: wired pins become captures, unwired pins become closure
//! parameters. `C` reuses `I`'s id; consumers of `I`'s `-1` pin are flipped to
//! `C`'s pin `0`.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::CustomNodeData;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, IncomingWire, NodeDisplayState, NodeDisplayType, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
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

/// Add an `expr` node into a zone-owning node's body (an HOF or a `closure`).
/// Returns the new body node's id.
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
    let body = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap()
        .zone_mut()
        .unwrap();
    body.nodes.get_mut(&body_node_id).unwrap().arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: owner_node_id,
            source_pin: SourcePin::ZoneInput {
                pin_index: zone_input_pin,
            },
            source_scope_depth: 1,
        });
}

/// Wire an outer-scope node output (a capture, depth 1) into a body node arg.
fn wire_capture_to_body_node(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    body_node_id: u64,
    body_param_index: usize,
    source_node_id: u64,
) {
    let body = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap()
        .zone_mut()
        .unwrap();
    body.nodes.get_mut(&body_node_id).unwrap().arguments[body_param_index]
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
    let owner_node = designer
        .node_type_registry
        .node_networks
        .get_mut(owner_network)
        .unwrap()
        .nodes
        .get_mut(&owner_node_id)
        .unwrap();
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

/// Add a map-like `(Int) -> Int` `closure` node whose body computes `expression`
/// over `element` (`x`) and optional capture parameters (one expr param per
/// entry in `captures`, wired to the given outer-scope source node id). Returns
/// the closure node's id.
fn add_int_closure(
    designer: &mut StructureDesigner,
    network: &str,
    expression: &str,
    captures: &[(&str, u64)],
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

    let mut params = vec![("x".to_string(), DataType::Int)];
    for (cap_name, _) in captures {
        params.push((cap_name.to_string(), DataType::Int));
    }
    let expr_id = add_expr_to_body(designer, network, closure_id, expression, params);

    wire_zone_input_to_body_node(designer, network, closure_id, 0, expr_id, 0);
    for (i, (_, source_node_id)) in captures.iter().enumerate() {
        wire_capture_to_body_node(
            designer,
            network,
            closure_id,
            expr_id,
            i + 1,
            *source_node_id,
        );
    }
    wire_body_node_to_zone_output(designer, network, closure_id, expr_id);

    closure_id
}

/// Wire `source`'s pin `0` into `dest`'s argument (the way a closure's function
/// value is consumed — e.g. `map.f`).
fn wire_pin0(
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
    net.nodes.get_mut(&dest_node_id).unwrap().arguments[dest_arg_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Read the `map.f` consumer wire's `(source_node_id, source_pin)`.
fn map_f_wire(designer: &StructureDesigner, network: &str, map_id: u64) -> (u64, SourcePin) {
    let w = &designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .arguments[1]
        .incoming_wires[0];
    (w.source_node_id, w.source_pin)
}

/// Collect the `parameter` nodes of network `name` as `(param_index, name,
/// data_type)`, sorted by `param_index`.
fn param_nodes(designer: &StructureDesigner, name: &str) -> Vec<(usize, String, DataType)> {
    let net = designer.node_type_registry.node_networks.get(name).unwrap();
    let mut out: Vec<(usize, String, DataType)> = net
        .nodes
        .values()
        .filter(|n| n.node_type_name == "parameter")
        .map(|n| {
            let pd = n.data.as_any_ref().downcast_ref::<ParameterData>().unwrap();
            (pd.param_index, pd.param_name.clone(), pd.data_type.clone())
        })
        .collect();
    out.sort_by_key(|(i, _, _)| *i);
    out
}

/// The function type carried by the closure node `node_id`'s `ClosureData`.
fn closure_function_type(
    designer: &StructureDesigner,
    network: &str,
    node_id: u64,
) -> FunctionType {
    let cd = closure_data(designer, network, node_id);
    cd.kind.function_type(&cd.type_args, &cd.param_names)
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

/// A `NetworkResult::Int` (the result of evaluating a terminal `fold`).
fn extract_int(result: NetworkResult) -> i32 {
    match result {
        NetworkResult::Int(v) => v,
        NetworkResult::Error(msg) => panic!("expected Int, got Error: {msg}"),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

// ============================================================================
// Closure → Network (Phase 2, top level)
// ============================================================================

/// Basic: a `(Int) -> Int` closure (`x + 1`) with no captures -> a network with
/// one parameter node, return set; an instance replaces the closure; the
/// consumer wire flips `0 -> -1`; evaluation matches before and after.
#[test]
fn extract_basic_one_param_no_capture() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(&mut designer, "main", "x + 1", &[], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_pin0(&mut designer, "main", closure_id, map_id, 1); // map.f <- closure.0

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![1, 2, 3]);

    designer
        .extract_closure_to_network(vec![], closure_id, "inc_net")
        .expect("extraction should succeed");

    // The node became an instance of the new network.
    assert_eq!(node_type_name(&designer, "main", closure_id), "inc_net");
    // One closure parameter node, no captures. (The `Map`-kind closure's
    // zone-input pin is named `element`, so that is the extracted param name.)
    assert_eq!(
        param_nodes(&designer, "inc_net"),
        vec![(0, "element".to_string(), DataType::Int)]
    );
    // The new network has a return node.
    assert!(
        designer
            .node_type_registry
            .node_networks
            .get("inc_net")
            .unwrap()
            .return_node_id
            .is_some()
    );

    // The consumer wire flipped from pin 0 to the function pin -1 (same id).
    assert_eq!(
        map_f_wire(&designer, "main", map_id),
        (closure_id, SourcePin::NodeOutput { pin_index: -1 })
    );

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![1, 2, 3]);
}

/// One capture (`e = 0`): a `(x) -> x + cap` closure capturing an outer constant
/// -> a network with a closure-param pin and a capture pin; the instance's
/// capture pin is wired (depth 0) to the original source; the closure-param pin
/// is left unwired. Evaluation matches.
#[test]
fn extract_one_capture_e0() {
    let mut designer = setup_designer_with_network("main");

    let const_id = add_int(&mut designer, "main", 10, -240.0);
    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(
        &mut designer,
        "main",
        "x + cap",
        &[("cap", const_id)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![10, 11, 12]);

    designer
        .extract_closure_to_network(vec![], closure_id, "addcap")
        .expect("extraction should succeed");

    // Two parameter nodes: the closure param (x) and one capture.
    let params = param_nodes(&designer, "addcap");
    assert_eq!(params.len(), 2, "one closure param + one capture");
    assert_eq!(params[0], (0, "element".to_string(), DataType::Int));
    assert_eq!(params[1].0, 1);
    assert_eq!(params[1].2, DataType::Int);

    // The instance: closure-param pin 0 unwired, capture pin 1 wired to const.
    let inst = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&closure_id)
        .unwrap();
    assert!(
        inst.arguments[0].is_empty(),
        "closure-param pin left unwired"
    );
    let cap_wire = &inst.arguments[1].incoming_wires[0];
    assert_eq!(cap_wire.source_node_id, const_id);
    assert_eq!(cap_wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    assert_eq!(
        cap_wire.source_scope_depth, 0,
        "e = 0 normal same-scope wire"
    );

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![10, 11, 12]);
}

/// Passthrough result wire: a closure whose zone-output reads its own zone-input
/// (`element`) directly. The result wire is a closure-parameter `ZoneInput`, so
/// the network's return node is the parameter node itself; evaluation is the
/// identity over the stream.
#[test]
fn extract_passthrough_result_wire() {
    let mut designer = setup_designer_with_network("main");

    let closure_id = designer.add_node("closure", DVec2::new(150.0, -120.0));
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
    // Wire zone-output directly to the closure's zone-input pin 0 (element).
    {
        let owner = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&closure_id)
            .unwrap();
        owner.zone_output_arguments = vec![Argument {
            incoming_wires: vec![IncomingWire {
                source_node_id: closure_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            }],
        }];
    }

    let range_id = add_range(&mut designer, "main", 5, 1, 3, 0.0); // [5,6,7]
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![5, 6, 7]);

    designer
        .extract_closure_to_network(vec![], closure_id, "idnet")
        .expect("extraction should succeed");

    // The network's return node is its (sole) parameter node.
    let net = designer
        .node_type_registry
        .node_networks
        .get("idnet")
        .unwrap();
    let return_id = net.return_node_id.expect("return node set");
    assert_eq!(
        net.nodes.get(&return_id).unwrap().node_type_name,
        "parameter"
    );
    assert_eq!(param_nodes(&designer, "idnet").len(), 1);

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![5, 6, 7]);
}

/// Same capture referenced twice (both expr params wired to the same source)
/// dedups to a single capture parameter node / a single instance pin.
#[test]
fn extract_same_capture_dedups() {
    let mut designer = setup_designer_with_network("main");

    let const_id = add_int(&mut designer, "main", 10, -240.0);
    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(
        &mut designer,
        "main",
        "x + a + b",
        &[("a", const_id), ("b", const_id)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![20, 21, 22]);

    designer
        .extract_closure_to_network(vec![], closure_id, "addtwice")
        .expect("extraction should succeed");

    // Only two parameters: the closure param + one (deduped) capture.
    assert_eq!(param_nodes(&designer, "addtwice").len(), 2);
    let inst = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&closure_id)
        .unwrap();
    assert_eq!(inst.arguments.len(), 2, "instance has 2 pins");
    assert_eq!(inst.arguments[1].incoming_wires[0].source_node_id, const_id);

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![20, 21, 22]);
}

/// Two captures from distinct sources sharing a base name get de-duplicated
/// parameter-node names (`k_cap`, `k_cap_2`) and two separate instance pins.
#[test]
fn extract_colliding_base_names_dedup() {
    let mut designer = setup_designer_with_network("main");

    let k1 = add_int(&mut designer, "main", 10, -300.0);
    let k2 = add_int(&mut designer, "main", 20, -380.0);
    // Force both capture sources to share the base name "k".
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        net.nodes.get_mut(&k1).unwrap().custom_name = Some("k".to_string());
        net.nodes.get_mut(&k2).unwrap().custom_name = Some("k".to_string());
    }

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(
        &mut designer,
        "main",
        "x + a + b",
        &[("a", k1), ("b", k2)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    let baseline = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(baseline, vec![30, 31, 32]);

    designer
        .extract_closure_to_network(vec![], closure_id, "addk")
        .expect("extraction should succeed");

    let params = param_nodes(&designer, "addk");
    assert_eq!(params.len(), 3, "closure param + two distinct captures");
    let names: Vec<String> = params.iter().map(|(_, n, _)| n.clone()).collect();
    assert_eq!(names[0], "element");
    assert_eq!(names[1], "k_cap");
    assert_eq!(names[2], "k_cap_2");

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![30, 31, 32]);
}

/// A consumer of the closure's pin `0` living inside a sibling HOF body
/// (`source_scope_depth == 1`) is flipped to the function pin `-1` at the same
/// depth -- the recursive consumer walk, not just the host frame. Exercised by
/// calling `redirect_value_consumers` directly: the redirect is the structural
/// rewrite the orchestrator performs, and it must descend into sub-bodies.
#[test]
fn redirect_value_consumers_flips_in_sibling_body_at_depth() {
    use rust_lib_flutter_cad::structure_designer::closure_network_conversion::redirect_value_consumers;

    let mut designer = setup_designer_with_network("main");

    let closure_id = add_int_closure(&mut designer, "main", "x + 1", &[], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    // Same-scope consumer: map.f reads the closure's pin 0.
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    // Sub-body consumer: an `apply` inside map's zone reads pin 0 at depth 1.
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let body = net.nodes.get_mut(&map_id).unwrap().zone_mut().unwrap();
        let body_node_id = body.add_node(
            "apply",
            DVec2::new(50.0, 0.0),
            1,
            Box::new(rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData::default()),
        );
        body.nodes.get_mut(&body_node_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: closure_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }

    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        redirect_value_consumers(net, closure_id);
    }

    // Same-scope consumer flipped 0 -> -1.
    assert_eq!(
        map_f_wire(&designer, "main", map_id),
        (closure_id, SourcePin::NodeOutput { pin_index: -1 })
    );

    // Sub-body consumer flipped 0 -> -1 at the same depth.
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
    assert_eq!(wire.source_node_id, closure_id);
    assert_eq!(wire.source_scope_depth, 1, "still at depth 1");
    assert_eq!(
        wire.source_pin,
        SourcePin::NodeOutput { pin_index: -1 },
        "flipped 0 -> -1 inside the body"
    );
}

// ----------------------------------------------------------------------------
// Reject cases
// ----------------------------------------------------------------------------

/// A non-closure node (a built-in `int`) cannot be extracted.
#[test]
fn extract_rejects_non_closure() {
    let mut designer = setup_designer_with_network("main");
    let int_id = add_int(&mut designer, "main", 5, 0.0);
    let err = designer
        .extract_closure_to_network(vec![], int_id, "net")
        .unwrap_err();
    assert!(err.contains("Only closure nodes"), "got: {err}");
}

/// A closure with no result wire (empty zone-output) is rejected.
#[test]
fn extract_rejects_no_result() {
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
    // Add a body expr but leave the zone-output pin unwired.
    let _expr = add_expr_to_body(
        &mut designer,
        "main",
        closure_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );

    let err = designer
        .extract_closure_to_network(vec![], closure_id, "net")
        .unwrap_err();
    assert!(err.contains("no result"), "got: {err}");
}

/// A result wire drawn from a secondary output pin (`pin_index > 0`) is rejected.
#[test]
fn extract_rejects_secondary_output_pin() {
    let mut designer = setup_designer_with_network("main");
    let closure_id = add_int_closure(&mut designer, "main", "x + 1", &[], 0.0);
    // Rewrite the zone-output wire to read a secondary pin of the body node.
    {
        let owner = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&closure_id)
            .unwrap();
        owner.zone_output_arguments[0].incoming_wires[0].source_pin =
            SourcePin::NodeOutput { pin_index: 1 };
    }

    let err = designer
        .extract_closure_to_network(vec![], closure_id, "net")
        .unwrap_err();
    assert!(err.contains("secondary output pin"), "got: {err}");
}

/// A name already taken is rejected before any mutation.
#[test]
fn extract_rejects_taken_name() {
    let mut designer = setup_designer_with_network("main");
    let closure_id = add_int_closure(&mut designer, "main", "x + 1", &[], 0.0);
    let err = designer
        .extract_closure_to_network(vec![], closure_id, "main")
        .unwrap_err();
    assert!(err.contains("already exists"), "got: {err}");
    // Node is untouched (still a closure).
    assert_eq!(node_type_name(&designer, "main", closure_id), "closure");
}

// ----------------------------------------------------------------------------
// Round-trip + undo
// ----------------------------------------------------------------------------

/// `closure -> network -> closure` (no captures): extract a closure to a
/// network, then convert the resulting instance back to a closure. The
/// reconstructed closure's `function_type` matches the original and it evaluates
/// identically.
#[test]
fn round_trip_closure_network_closure_basic() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(&mut designer, "main", "x + 1", &[], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    let original_ft = closure_function_type(&designer, "main", closure_id);

    designer
        .extract_closure_to_network(vec![], closure_id, "inc_net")
        .expect("extraction should succeed");
    assert_eq!(node_type_name(&designer, "main", closure_id), "inc_net");

    designer
        .convert_instance_to_closure(vec![], closure_id)
        .expect("conversion back should succeed");
    assert_eq!(node_type_name(&designer, "main", closure_id), "closure");

    // Same function type (compare on type, not kind -- Direction A emits Custom).
    assert_eq!(
        closure_function_type(&designer, "main", closure_id),
        original_ft
    );

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![1, 2, 3]);
}

/// `closure -> network -> closure` with a capture: the capture survives the
/// round trip (function type + evaluation match).
#[test]
fn round_trip_closure_network_closure_with_capture() {
    let mut designer = setup_designer_with_network("main");

    let const_id = add_int(&mut designer, "main", 100, -240.0);
    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(
        &mut designer,
        "main",
        "x + cap",
        &[("cap", const_id)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    let original_ft = closure_function_type(&designer, "main", closure_id);

    designer
        .extract_closure_to_network(vec![], closure_id, "addcap")
        .expect("extraction should succeed");
    designer
        .convert_instance_to_closure(vec![], closure_id)
        .expect("conversion back should succeed");

    assert_eq!(node_type_name(&designer, "main", closure_id), "closure");
    assert_eq!(
        closure_function_type(&designer, "main", closure_id),
        original_ft
    );

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![100, 101, 102]);
}

/// Undo/redo round-trip: undo restores the closure and removes the network;
/// redo re-applies the extraction.
#[test]
fn extract_undo_redo_round_trip() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let closure_id = add_int_closure(&mut designer, "main", "x + 1", &[], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_pin0(&mut designer, "main", closure_id, map_id, 1);

    designer
        .extract_closure_to_network(vec![], closure_id, "inc_net")
        .expect("extraction should succeed");
    assert_eq!(node_type_name(&designer, "main", closure_id), "inc_net");
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("inc_net")
    );

    designer.undo();
    assert_eq!(
        node_type_name(&designer, "main", closure_id),
        "closure",
        "undo restores the closure"
    );
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .contains_key("inc_net"),
        "undo removes the extracted network"
    );

    designer.redo();
    assert_eq!(
        node_type_name(&designer, "main", closure_id),
        "inc_net",
        "redo re-applies the extraction"
    );
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("inc_net")
    );
    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", map_id),
    ));
    assert_eq!(after, vec![1, 2, 3]);
}

// ============================================================================
// Phase 3 — body scope + e >= 1 captures
// ============================================================================
//
// All Phase-3 scenarios place the conversion target inside a `fold` body. The
// fold (over `[1,2,3]`, init `0`) applies a body function to each element via an
// inline `apply` whose `f` reads that function value. For *Closure → Network*
// the function is a `closure` C; for *Network → Closure* it is a custom-network
// instance I used through its `-1` pin. The fold's numeric output therefore
// exercises the converted function end-to-end, before and after the rewrite.

/// Populate the custom-node-type cache for a node living inside `fold_id`'s body.
fn populate_fold_body_node(
    designer: &mut StructureDesigner,
    network: &str,
    fold_id: u64,
    node_id: u64,
    refresh_args: bool,
) {
    let registry = &mut designer.node_type_registry;
    let node = registry
        .node_networks
        .get_mut(network)
        .unwrap()
        .nodes
        .get_mut(&fold_id)
        .unwrap()
        .zone_mut()
        .unwrap()
        .nodes
        .get_mut(&node_id)
        .unwrap();
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        refresh_args,
    );
}

/// Add a node into `fold_id`'s body (positioned at `pos_y`) and populate its
/// cache (`refresh_args = true`). Returns the new body node's id.
fn add_node_to_fold_body(
    designer: &mut StructureDesigner,
    network: &str,
    fold_id: u64,
    type_name: &str,
    num_args: usize,
    data: Box<dyn NodeData>,
    pos_y: f64,
) -> u64 {
    let id = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut(network)
            .unwrap()
            .nodes
            .get_mut(&fold_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.add_node(type_name, DVec2::new(0.0, pos_y), num_args, data)
    };
    populate_fold_body_node(designer, network, fold_id, id, true);
    id
}

/// Create a `fold` over `[1,2,3]` with init `0` (Int element + accumulator) in
/// `network`. Returns the fold node id.
fn add_int_fold(designer: &mut StructureDesigner, network: &str) -> u64 {
    let range_id = add_range(designer, network, 1, 1, 3, 0.0); // [1,2,3]
    let init_id = add_int(designer, network, 0, 80.0);
    let fold_id = designer.add_node("fold", DVec2::new(200.0, 0.0));
    set_node_data(
        designer,
        network,
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, fold_id, 0); // xs
    designer.connect_nodes(init_id, 0, fold_id, 1); // init
    fold_id
}

/// Add a Map-kind `(Int) -> Int` `closure` C into `fold_id`'s body whose own
/// body computes `expr` over `x` (= C's `element`) plus one expr param per
/// `capture_names` entry. Wires `x` and the zone-output; leaves the capture arg
/// slots **unwired** (the caller wires them at the depth it needs via
/// [`push_closure_body_capture`]). Returns `(c_id, c_expr_id)`.
fn add_closure_to_fold_body(
    designer: &mut StructureDesigner,
    network: &str,
    fold_id: u64,
    expr: &str,
    capture_names: &[&str],
) -> (u64, u64) {
    let c_id = add_node_to_fold_body(
        designer,
        network,
        fold_id,
        "closure",
        0,
        Box::new(ClosureData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
        0.0,
    );

    // Build C's body: `expr` over `x` + one Int param per capture name.
    let c_expr_id = {
        let c_body = designer
            .node_type_registry
            .node_networks
            .get_mut(network)
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

        let mut expr_params: Vec<ExprParameter> = vec![ExprParameter {
            id: None,
            name: "x".to_string(),
            data_type: DataType::Int,
            data_type_str: None,
        }];
        for name in capture_names {
            expr_params.push(ExprParameter {
                id: None,
                name: name.to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            });
        }
        let num_params = expr_params.len();
        let mut expr_data = ExprData {
            parameters: expr_params,
            expression: expr.to_string(),
            expr: None,
            error: None,
            output_type: None,
        };
        let _ = expr_data.parse_and_validate(0);
        let expr_id = c_body.add_node(
            "expr",
            DVec2::new(50.0, 0.0),
            num_params,
            Box::new(expr_data),
        );

        // x <- C's `element` zone-input pin (depth 1, the immediately enclosing
        // closure).
        c_body.nodes.get_mut(&expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: c_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });

        // C's zone-output (result) <- expr.
        let c_node = designer
            .node_type_registry
            .node_networks
            .get_mut(network)
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

    // Populate C's body expr (refresh_args = false to preserve the wires above).
    {
        let registry = &mut designer.node_type_registry;
        let expr_node = registry
            .node_networks
            .get_mut(network)
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

    (c_id, c_expr_id)
}

/// Push a capture wire into C's body expr at `arg_index` (C lives in `fold_id`'s
/// body). The wire is given as-seen-from-C's-body.
fn push_closure_body_capture(
    designer: &mut StructureDesigner,
    network: &str,
    fold_id: u64,
    c_id: u64,
    c_expr_id: u64,
    arg_index: usize,
    wire: IncomingWire,
) {
    let expr_node = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
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
    expr_node.arguments[arg_index].incoming_wires.push(wire);
}

/// Add an `apply` A into `fold_id`'s body that applies the function value on
/// `(func_id, func_pin)` to the fold's `element`, feeding the fold's `new_acc`.
/// Runs the apply post-pass so A's arg pins materialize, then wires `element`
/// and the zone-output. Returns A's id.
fn add_apply_to_fold_body(
    designer: &mut StructureDesigner,
    network: &str,
    fold_id: u64,
    func_id: u64,
    func_pin: i32,
) -> u64 {
    let a_id = add_node_to_fold_body(
        designer,
        network,
        fold_id,
        "apply",
        1,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![DataType::Int, DataType::Int],
            param_names: vec![],
        }),
        100.0,
    );

    // Wire A.f, run the apply post-pass to install the arg pins from the wired
    // source's function type, then wire A.element. Split-borrow via a temporary
    // remove/reinsert of the host network.
    {
        let mut net = designer
            .node_type_registry
            .node_networks
            .remove(network)
            .unwrap();
        {
            let body = net.nodes.get_mut(&fold_id).unwrap().zone_mut().unwrap();
            body.nodes.get_mut(&a_id).unwrap().arguments[0]
                .incoming_wires
                .push(IncomingWire {
                    source_node_id: func_id,
                    source_pin: SourcePin::NodeOutput {
                        pin_index: func_pin,
                    },
                    source_scope_depth: 0,
                });
            designer
                .node_type_registry
                .update_apply_pin_layouts_for_network(body);
            body.nodes.get_mut(&a_id).unwrap().arguments[1]
                .incoming_wires
                .push(IncomingWire {
                    source_node_id: fold_id,
                    source_pin: SourcePin::ZoneInput { pin_index: 1 }, // element
                    source_scope_depth: 1,
                });
        }
        designer
            .node_type_registry
            .node_networks
            .insert(network.to_string(), net);
    }

    // fold's zone-output (new_acc) <- A.
    wire_body_node_to_zone_output(designer, network, fold_id, a_id);
    a_id
}

/// The type name of a node living inside `fold_id`'s body.
fn fold_body_node_type(
    designer: &StructureDesigner,
    network: &str,
    fold_id: u64,
    node_id: u64,
) -> String {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&fold_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .node_type_name
        .clone()
}

/// A wire on node `node_id` (argument `arg_index`) inside `fold_id`'s body.
fn fold_body_instance_wire(
    designer: &StructureDesigner,
    network: &str,
    fold_id: u64,
    node_id: u64,
    arg_index: usize,
) -> IncomingWire {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&fold_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .arguments[arg_index]
        .incoming_wires[0]
        .clone()
}

// ----------------------------------------------------------------------------
// Closure → Network, inside a fold body
// ----------------------------------------------------------------------------

/// `e = 0` capture inside a body: the closure captures a constant living in the
/// **same** fold body. After extraction the instance's capture pin is a normal
/// same-scope wire (depth 0) to that constant; the fold's value is unchanged;
/// undo restores the closure and removes the network.
#[test]
fn extract_in_body_capture_e0() {
    let mut designer = setup_designer_with_network("main");
    let fold_id = add_int_fold(&mut designer, "main");

    // A constant living inside the fold body (host scope of the closure).
    let body_const = add_node_to_fold_body(
        &mut designer,
        "main",
        fold_id,
        "int",
        0,
        Box::new(IntData { value: 5 }),
        200.0,
    );

    let (c_id, c_expr_id) =
        add_closure_to_fold_body(&mut designer, "main", fold_id, "x + cap", &["cap"]);
    // cap <- body const (from C's body, the fold-body const is 1 frame up → e=0).
    push_closure_body_capture(
        &mut designer,
        "main",
        fold_id,
        c_id,
        c_expr_id,
        1,
        IncomingWire {
            source_node_id: body_const,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        },
    );
    add_apply_to_fold_body(&mut designer, "main", fold_id, c_id, 0);
    designer.validate_active_network();

    // Baseline: new_acc = element + 5; fold over [1,2,3] → 3 + 5 = 8.
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 8);

    designer
        .extract_closure_to_network(vec![fold_id], c_id, "addbody")
        .expect("extraction should succeed");

    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, c_id),
        "addbody"
    );
    // Instance capture pin (index 1) is a normal same-scope wire (e = 0).
    let cap = fold_body_instance_wire(&designer, "main", fold_id, c_id, 1);
    assert_eq!(cap.source_node_id, body_const);
    assert_eq!(cap.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    assert_eq!(
        cap.source_scope_depth, 0,
        "e = 0 capture is a same-scope wire"
    );

    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 8);

    // Body undo: closure restored, network removed.
    designer.undo();
    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, c_id),
        "closure"
    );
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .contains_key("addbody"),
        "undo removes the extracted network"
    );
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 8);

    designer.redo();
    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, c_id),
        "addbody"
    );
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 8);
}

/// `e >= 1` `NodeOutput` capture: a closure in the fold body captures a
/// **top-level** constant (one frame above the host body). After extraction the
/// instance's capture wire has `source_scope_depth == 1`; the fold's value is
/// unchanged.
#[test]
fn extract_in_body_capture_e1_node_output() {
    let mut designer = setup_designer_with_network("main");
    // Top-level constant captured from within the fold body's closure.
    let top_const = add_int(&mut designer, "main", 100, -200.0);
    let fold_id = add_int_fold(&mut designer, "main");

    let (c_id, c_expr_id) =
        add_closure_to_fold_body(&mut designer, "main", fold_id, "x + cap", &["cap"]);
    // cap <- top-level const. From C's body: C-body → fold-body → main = depth 2.
    push_closure_body_capture(
        &mut designer,
        "main",
        fold_id,
        c_id,
        c_expr_id,
        1,
        IncomingWire {
            source_node_id: top_const,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 2,
        },
    );
    add_apply_to_fold_body(&mut designer, "main", fold_id, c_id, 0);
    designer.validate_active_network();

    // Baseline: new_acc = element + 100; fold over [1,2,3] → 3 + 100 = 103.
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 103);

    designer
        .extract_closure_to_network(vec![fold_id], c_id, "addtop")
        .expect("extraction should succeed");

    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, c_id),
        "addtop"
    );
    // Instance capture wire reaches the top-level const at depth e = 1.
    let cap = fold_body_instance_wire(&designer, "main", fold_id, c_id, 1);
    assert_eq!(cap.source_node_id, top_const);
    assert_eq!(cap.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    assert_eq!(
        cap.source_scope_depth, 1,
        "e = 1 capture wire on the instance"
    );

    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 103);
}

/// `e >= 1` `ZoneInput` capture: a closure in the fold body captures the fold's
/// own `acc` iteration value. This is the subtlest case — the captured value
/// must re-freeze per outer iteration. After extraction the instance carries a
/// `ZoneInput` capture wire at depth `e = 1`, and the fold still evaluates to the
/// running-sum result (not a frozen-once result).
#[test]
fn extract_in_body_capture_e1_zone_input() {
    let mut designer = setup_designer_with_network("main");
    let fold_id = add_int_fold(&mut designer, "main");

    let (c_id, c_expr_id) =
        add_closure_to_fold_body(&mut designer, "main", fold_id, "x + acc", &["acc"]);
    // acc <- the fold's `acc` zone-input pin (index 0). From C's body the fold
    // node is depth 2 (C-body → fold-body → main, where the fold node lives).
    push_closure_body_capture(
        &mut designer,
        "main",
        fold_id,
        c_id,
        c_expr_id,
        1,
        IncomingWire {
            source_node_id: fold_id,
            source_pin: SourcePin::ZoneInput { pin_index: 0 }, // acc
            source_scope_depth: 2,
        },
    );
    add_apply_to_fold_body(&mut designer, "main", fold_id, c_id, 0);
    designer.validate_active_network();

    // Baseline: new_acc = element + acc; fold over [1,2,3] from 0 → 1,3,6.
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 6);

    designer
        .extract_closure_to_network(vec![fold_id], c_id, "addacc")
        .expect("extraction should succeed");

    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, c_id),
        "addacc"
    );
    // Instance capture wire is a `ZoneInput` reference at depth e = 1.
    let cap = fold_body_instance_wire(&designer, "main", fold_id, c_id, 1);
    assert_eq!(cap.source_node_id, fold_id);
    assert_eq!(cap.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(
        cap.source_scope_depth, 1,
        "ZoneInput capture at depth e = 1"
    );

    // The per-iteration value must still be read live (re-frozen per outer
    // iteration), so the running sum is preserved.
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 6);
}

/// `closure → network → closure` round trip with an `e >= 1` `ZoneInput`
/// capture, inside a fold body. After extracting then converting back, the fold
/// evaluates identically and the reconstructed body node is a closure again.
#[test]
fn round_trip_in_body_zone_input_capture() {
    let mut designer = setup_designer_with_network("main");
    let fold_id = add_int_fold(&mut designer, "main");

    let (c_id, c_expr_id) =
        add_closure_to_fold_body(&mut designer, "main", fold_id, "x + acc", &["acc"]);
    push_closure_body_capture(
        &mut designer,
        "main",
        fold_id,
        c_id,
        c_expr_id,
        1,
        IncomingWire {
            source_node_id: fold_id,
            source_pin: SourcePin::ZoneInput { pin_index: 0 },
            source_scope_depth: 2,
        },
    );
    add_apply_to_fold_body(&mut designer, "main", fold_id, c_id, 0);
    designer.validate_active_network();
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 6);

    designer
        .extract_closure_to_network(vec![fold_id], c_id, "addacc")
        .expect("extraction should succeed");
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 6);

    designer
        .convert_instance_to_closure(vec![fold_id], c_id)
        .expect("conversion back should succeed");

    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, c_id),
        "closure"
    );
    assert_eq!(
        extract_int(evaluate_node(&designer, "main", fold_id)),
        6,
        "round trip preserves the per-iteration fold result"
    );
}

// ----------------------------------------------------------------------------
// Network → Closure, inside a fold body
// ----------------------------------------------------------------------------

/// Convert a custom-network instance used as a function inside a fold body. The
/// body node becomes a `closure`, the `apply` consumer flips `-1 → 0`, the fold
/// value is unchanged, and a body undo/redo round-trips.
#[test]
fn convert_in_body_basic_and_undo() {
    let mut designer = setup_designer_with_network("main");
    build_expr_network(&mut designer, "inc", &[("x", DataType::Int)], "x + 1", true);

    let fold_id = add_int_fold(&mut designer, "main");

    // An instance of `inc` inside the fold body, consumed through its `-1` pin.
    let inst_id = add_node_to_fold_body(
        &mut designer,
        "main",
        fold_id,
        "inc",
        1,
        Box::new(CustomNodeData::default()),
        0.0,
    );
    let a_id = add_apply_to_fold_body(&mut designer, "main", fold_id, inst_id, -1);
    designer.validate_active_network();

    // Baseline: inc.-1 = (Int) -> Int = x + 1; new_acc = element + 1; → 3 + 1 = 4.
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 4);

    designer
        .convert_instance_to_closure(vec![fold_id], inst_id)
        .expect("conversion should succeed");

    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, inst_id),
        "closure"
    );
    // The apply's `f` wire flipped from the function pin (-1) to pin 0.
    let f_wire = fold_body_instance_wire(&designer, "main", fold_id, a_id, 0);
    assert_eq!(f_wire.source_node_id, inst_id);
    assert_eq!(f_wire.source_pin, SourcePin::NodeOutput { pin_index: 0 });
    assert_eq!(f_wire.source_scope_depth, 0);

    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 4);

    // Body undo restores the instance; redo re-applies the conversion.
    designer.undo();
    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, inst_id),
        "inc"
    );
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 4);

    designer.redo();
    assert_eq!(
        fold_body_node_type(&designer, "main", fold_id, inst_id),
        "closure"
    );
    assert_eq!(extract_int(evaluate_node(&designer, "main", fold_id)), 4);
}
