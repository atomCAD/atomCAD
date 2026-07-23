//! Phase 1 tests for the `zip_with` node (`doc/design_zip_with.md`,
//! issue #382): the core node + `ZipZone` walker over the inline-body path,
//! plus the wired-`f` evaluation path.
//!
//! Network construction mirrors `zones_test.rs` (direct manipulation of the
//! HOF node's owned body network — mind the per-body `next_node_id` /
//! `num_params` gotchas documented there). Walker-level `ZipZone` tests live
//! in `iterator_walker_test.rs`.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, ArgumentKind, IncomingWire, SourcePin, Wire,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::nodes::zip_with::{ZipWithData, ZipWithLane};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers (mirroring zones_test.rs)
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
        is_zone_body: false,
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
            other => panic!("expected Int element, got {}", other.to_display_string()),
        })
        .collect()
}

fn extract_floats(values: Vec<NetworkResult>) -> Vec<f64> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Float(v) => v,
            NetworkResult::Error(msg) => panic!("expected Float element, got Error: {}", msg),
            other => panic!("expected Float element, got {}", other.to_display_string()),
        })
        .collect()
}

/// `ZipWithData` with `n` lanes of the given types (ids 1..=n) and the given
/// output type — the shape the property panel would author.
fn zip_data(lane_types: Vec<DataType>, output_type: DataType) -> ZipWithData {
    let n = lane_types.len() as u64;
    ZipWithData {
        lanes: lane_types
            .into_iter()
            .enumerate()
            .map(|(i, data_type)| ZipWithLane {
                id: Some(i as u64 + 1),
                data_type,
            })
            .collect(),
        output_type,
        next_lane_id: n + 1,
    }
}

fn add_range(
    designer: &mut StructureDesigner,
    network_name: &str,
    start: i32,
    step: i32,
    count: i32,
    x: f64,
) -> u64 {
    let id = designer.add_node("range", DVec2::new(x, 0.0));
    set_node_data(
        designer,
        network_name,
        id,
        Box::new(RangeData { start, step, count }),
    );
    id
}

fn add_zip(
    designer: &mut StructureDesigner,
    network_name: &str,
    lane_types: Vec<DataType>,
    output_type: DataType,
    x: f64,
) -> u64 {
    let id = designer.add_node("zip_with", DVec2::new(x, 0.0));
    set_node_data(
        designer,
        network_name,
        id,
        Box::new(zip_data(lane_types, output_type)),
    );
    id
}

/// Add an `expr` node to a top-level HOF node's body network.
fn add_expr_to_body(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    expression: &str,
    parameters: Vec<(String, DataType)>,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    let body = hof_node.zone_mut().expect("HOF node missing zone");

    let expr_id = add_expr_to_network(body, expression, parameters);

    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        registry
            .node_networks
            .get_mut(network_name)
            .unwrap()
            .nodes
            .get_mut(&hof_node_id)
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

/// Add an `expr` node directly into a (possibly nested) body network.
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
        DVec2::new(50.0, 0.0),
        num_params,
        Box::new(expr_data),
    )
}

/// Wire an HOF's inside-facing zone-input pin into a body node's argument pin.
fn wire_zone_input_pin_to_body_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    zone_input_pin: usize,
    body_node_id: u64,
    body_param_index: usize,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
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
            source_scope_depth: 1,
        });
}

/// Wire an outer-scope node (capture) into a body node's argument pin.
fn wire_capture_to_body_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    body_node_id: u64,
    body_param_index: usize,
    source_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    let body = hof_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        });
}

/// Wire a body node into the HOF's first zone-output pin.
fn wire_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    network_name: &str,
    hof_node_id: u64,
    body_node_id: u64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let hof_node = network.nodes.get_mut(&hof_node_id).unwrap();
    if hof_node.zone_output_arguments.is_empty() {
        hof_node.zone_output_arguments.push(Argument::new());
    }
    hof_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Build a 2-lane Int zip whose body sums the two elements. Returns
/// (zip_id, range1_id, range2_id); ranges left unwired when `counts` is None
/// per lane.
fn build_sum_zip(
    designer: &mut StructureDesigner,
    network_name: &str,
    range1: Option<(i32, i32, i32)>,
    range2: Option<(i32, i32, i32)>,
) -> u64 {
    let zip_id = add_zip(
        designer,
        network_name,
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    if let Some((start, step, count)) = range1 {
        let r1 = add_range(designer, network_name, start, step, count, 0.0);
        designer.connect_nodes(r1, 0, zip_id, 0);
    }
    if let Some((start, step, count)) = range2 {
        let r2 = add_range(designer, network_name, start, step, count, 200.0);
        designer.connect_nodes(r2, 0, zip_id, 1);
    }

    let expr_id = add_expr_to_body(
        designer,
        network_name,
        zip_id,
        "a + b",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(designer, network_name, zip_id, 0, expr_id, 0);
    wire_zone_input_pin_to_body_node(designer, network_name, zip_id, 1, expr_id, 1);
    wire_body_node_to_zone_output(designer, network_name, zip_id, expr_id);

    zip_id
}

// ============================================================================
// Registration / node-type shape
// ============================================================================

#[test]
fn zip_with_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("zip_with")
        .expect("zip_with should be registered");

    // Default: two Float lanes + the trailing optional `f`.
    assert_eq!(nt.parameters.len(), 3);
    assert_eq!(nt.parameters[0].name, "xs1");
    assert_eq!(nt.parameters[1].name, "xs2");
    assert_eq!(nt.parameters[2].name, "f");
    assert_eq!(
        nt.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Float))
    );
    assert!(nt.has_zone());
    assert_eq!(nt.zone_input_pins.len(), 2);
    assert_eq!(nt.zone_input_pins[0].name, "element1");
    assert_eq!(nt.zone_input_pins[1].name, "element2");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "result");
    assert_eq!(
        *nt.output_type(),
        DataType::Iterator(Box::new(DataType::Float))
    );
}

#[test]
fn zip_with_custom_node_type_tracks_lane_list() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("zip_with").unwrap();
    let data = zip_data(
        vec![DataType::Int, DataType::Float, DataType::Vec3],
        DataType::Vec3,
    );
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(custom.parameters.len(), 4);
    assert_eq!(custom.parameters[0].name, "xs1");
    assert_eq!(custom.parameters[1].name, "xs2");
    assert_eq!(custom.parameters[2].name, "xs3");
    assert_eq!(custom.parameters[3].name, "f");
    // Hidden stable lane ids ride on the external parameters.
    assert_eq!(custom.parameters[0].id, Some(1));
    assert_eq!(custom.parameters[1].id, Some(2));
    assert_eq!(custom.parameters[2].id, Some(3));
    assert_eq!(custom.parameters[3].id, None);
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Iterator(Box::new(DataType::Float))
    );
    // `f` accepts any function whose parameter list starts with the lane types.
    assert_eq!(
        custom.parameters[3].data_type,
        DataType::AnyFunction {
            leading_params: vec![DataType::Int, DataType::Float, DataType::Vec3],
        }
    );

    assert_eq!(custom.zone_input_pins.len(), 3);
    assert_eq!(custom.zone_input_pins[0].name, "element1");
    assert_eq!(custom.zone_input_pins[2].name, "element3");
    assert_eq!(
        custom.zone_input_pins[1].fixed_type(),
        Some(&DataType::Float)
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Iterator(Box::new(DataType::Vec3))
    );
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Vec3);
}

#[test]
fn zip_with_subtitle_renders_shape() {
    let data = zip_data(vec![DataType::Int, DataType::Vec3], DataType::Float);
    let subtitle = data.get_subtitle(&std::collections::HashSet::new());
    assert_eq!(subtitle.as_deref(), Some("(Int, Vec3) → Float"));
}

// ============================================================================
// Inline-body evaluation
// ============================================================================

/// `range(3) ⊗ range(10,10,3)` with body `a + b` — the basic two-lane path.
#[test]
fn zip_two_lanes_sums_elementwise() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 3)), Some((10, 10, 3)));
    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![10, 21, 32]);
}

/// Shortest input ends the stream (3 ⊗ 5 → 3).
#[test]
fn zip_truncates_to_shortest_input() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 3)), Some((10, 10, 5)));
    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![10, 21, 32]);
}

/// Empty lane → empty output.
#[test]
fn zip_empty_lane_yields_empty_stream() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 0)), Some((10, 10, 5)));
    let result = evaluate_node(&designer, "main", zip_id);
    let elements = drain_iter_with_designer(&designer, result);
    assert!(elements.is_empty());
}

/// Three lanes: `element1 * element2 + element3` — variadic pins end-to-end.
#[test]
fn zip_three_lanes_variadic_body() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int, DataType::Int],
        DataType::Int,
        600.0,
    );
    let r1 = add_range(&mut designer, "main", 1, 1, 3, 0.0); // [1,2,3]
    let r2 = add_range(&mut designer, "main", 10, 10, 3, 200.0); // [10,20,30]
    let r3 = add_range(&mut designer, "main", 100, 100, 3, 400.0); // [100,200,300]
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);
    designer.connect_nodes(r3, 0, zip_id, 2);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "a * b + c",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
            ("c".to_string(), DataType::Int),
        ],
    );
    for pin in 0..3 {
        wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, pin, expr_id, pin);
    }
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![110, 240, 390]);
}

/// One lane is legal (the degenerate case equals `map`).
#[test]
fn zip_one_lane_degenerates_to_map() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int],
        DataType::Int,
        400.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    designer.connect_nodes(r1, 0, zip_id, 0);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "x + 1",
        vec![("x".to_string(), DataType::Int)],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 0, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![1, 2, 3]);
}

/// Mixed lane types (`Iter[Int]` ⊗ `Iter[Vec3]`): per-lane types flow to the
/// correct zone-input pins.
#[test]
fn zip_mixed_lane_types() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Vec3],
        DataType::Float,
        800.0,
    );

    let r1 = add_range(&mut designer, "main", 1, 1, 3, 0.0); // [1,2,3]
    designer.connect_nodes(r1, 0, zip_id, 0);

    // A 2-element Array[Vec3] via `sequence` fed by two vec3 nodes; the wire
    // converts Array[Vec3] → Iter[Vec3].
    let v1 = designer.add_node("vec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        "main",
        v1,
        Box::new(Vec3Data {
            value: DVec3::new(10.0, 0.0, 0.0),
        }),
    );
    let v2 = designer.add_node("vec3", DVec2::new(0.0, 400.0));
    set_node_data(
        &mut designer,
        "main",
        v2,
        Box::new(Vec3Data {
            value: DVec3::new(20.0, 0.0, 0.0),
        }),
    );
    let seq = designer.add_node("sequence", DVec2::new(200.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        seq,
        Box::new(SequenceData {
            element_type: DataType::Vec3,
            input_count: 2,
        }),
    );
    designer.connect_nodes(v1, 0, seq, 0);
    designer.connect_nodes(v2, 0, seq, 1);
    designer.connect_nodes(seq, 0, zip_id, 1);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "v.x + n",
        vec![
            ("n".to_string(), DataType::Int),
            ("v".to_string(), DataType::Vec3),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 0, expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 1, expr_id, 1);
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_floats(drain_iter_with_designer(&designer, result));
    // Truncated to the 2-element Vec3 lane: [10+1, 20+2].
    assert_eq!(elements, vec![11.0, 22.0]);
}

// ============================================================================
// Captures
// ============================================================================

/// Body references an outer constant (depth-1 capture): frozen once, correct
/// per element.
#[test]
fn zip_body_capture_of_outer_constant() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let r2 = add_range(&mut designer, "main", 0, 10, 3, 200.0);
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);

    let k_id = designer.add_node("int", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        k_id,
        Box::new(IntData { value: 1000 }),
    );

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "a + b + k",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
            ("k".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 0, expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 1, expr_id, 1);
    wire_capture_to_body_node(&mut designer, "main", zip_id, expr_id, 2, k_id);
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![1000, 1011, 1022]);
}

/// A `zip_with` nested inside a `map` body, whose own body deep-captures the
/// outer map's `element` (a `ZoneInput` reference at depth 2). Exercises the
/// eager-vs-lazy stack discipline the zip inherits: the deep capture is frozen
/// at the zip's per-outer-element `eval` against the live outer frame.
#[test]
fn zip_nested_in_map_body_with_deep_capture() {
    let mut designer = setup_designer_with_network("main");

    // Outer: range(2) → map(Int → Array[Int]).
    let outer_range_id = add_range(&mut designer, "main", 0, 1, 2, 0.0); // [0,1]
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Array(Box::new(DataType::Int)),
        }),
    );
    designer.connect_nodes(outer_range_id, 0, map_id, 0);

    // Map body: two ranges → zip_with → collect.
    let (r1_id, r2_id, zip_id, collect_id) = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let r1_id = body.add_node(
            "range",
            DVec2::new(0.0, 0.0),
            3,
            Box::new(RangeData {
                start: 1,
                step: 1,
                count: 3,
            }),
        ); // [1,2,3]
        let r2_id = body.add_node(
            "range",
            DVec2::new(0.0, 100.0),
            3,
            Box::new(RangeData {
                start: 10,
                step: 10,
                count: 2,
            }),
        ); // [10,20]
        let zip_id = body.add_node(
            "zip_with",
            DVec2::new(200.0, 0.0),
            3,
            Box::new(zip_data(vec![DataType::Int, DataType::Int], DataType::Int)),
        );
        let collect_id = body.add_node(
            "collect",
            DVec2::new(400.0, 0.0),
            2,
            Box::new(CollectData {
                element_type: DataType::Int,
                limit: None,
                offset: 0,
            }),
        );
        (r1_id, r2_id, zip_id, collect_id)
    };

    // Populate every body node's custom-type cache.
    for nid in [r1_id, r2_id, zip_id, collect_id] {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
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

    // Wire the map body: zip.xs1 ← r1, zip.xs2 ← r2, collect.xs ← zip; the
    // map's zone-output ← collect.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.nodes.get_mut(&zip_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: r1_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        body.nodes.get_mut(&zip_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: r2_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        body.nodes.get_mut(&collect_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: zip_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let map_node = network.nodes.get_mut(&map_id).unwrap();
        if map_node.zone_output_arguments.is_empty() {
            map_node.zone_output_arguments.push(Argument::new());
        }
        map_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: collect_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Zip body: expr `a + b + e` where `e` deep-captures the outer map's
    // element (ZoneInput pin 0 of map_id at depth 2 from the zip body).
    let expr_id = {
        let zip_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let expr_id = add_expr_to_network(
            zip_body,
            "a + b + e",
            vec![
                ("a".to_string(), DataType::Int),
                ("b".to_string(), DataType::Int),
                ("e".to_string(), DataType::Int),
            ],
        );
        zip_body.nodes.get_mut(&expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: zip_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        zip_body.nodes.get_mut(&expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: zip_id,
                source_pin: SourcePin::ZoneInput { pin_index: 1 },
                source_scope_depth: 1,
            });
        zip_body.nodes.get_mut(&expr_id).unwrap().arguments[2]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 2,
            });
        expr_id
    };

    // Populate the zip-body expr's cache.
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&expr_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Zip's zone-output ← expr.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let zip_node = body.nodes.get_mut(&zip_id).unwrap();
        if zip_node.zone_output_arguments.is_empty() {
            zip_node.zone_output_arguments.push(Argument::new());
        }
        zip_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Per outer element e ∈ [0,1]: zip [1,2,3] ⊗ [10,20] with a+b+e →
    // [11+e, 22+e], collected into an array.
    let result = evaluate_node(&designer, "main", map_id);
    let outer: Vec<NetworkResult> = drain_iter_with_designer(&designer, result);
    assert_eq!(outer.len(), 2);
    let arrays: Vec<Vec<i32>> = outer
        .into_iter()
        .map(|r| match r {
            NetworkResult::Array(items) => extract_ints(items),
            other => panic!("expected Array element, got {}", other.to_display_string()),
        })
        .collect();
    assert_eq!(arrays, vec![vec![11, 22], vec![12, 23]]);
}

// ============================================================================
// Wire conversions
// ============================================================================

/// An `Array[Int]` wired to an `Iter[Float]` lane converts (eager wrap +
/// element conversion at the wire layer).
#[test]
fn zip_array_int_to_iter_float_lane_converts() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Float],
        DataType::Float,
        600.0,
    );

    // range → collect gives a materialized Array[Int].
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0); // [0,1,2]
    let collect_id = designer.add_node("collect", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        collect_id,
        Box::new(CollectData {
            element_type: DataType::Int,
            limit: None,
            offset: 0,
        }),
    );
    designer.connect_nodes(r1, 0, collect_id, 0);
    designer.connect_nodes(collect_id, 0, zip_id, 0);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "x + 0.5",
        vec![("x".to_string(), DataType::Float)],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 0, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_floats(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![0.5, 1.5, 2.5]);
}

/// An `Iter[Int]` wired to an `Iter[Float]` lane converts lazily (the wire
/// wraps the source in `WalkerKind::Convert`).
#[test]
fn zip_iter_int_to_iter_float_lane_converts_lazily() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Float],
        DataType::Float,
        400.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0); // Iter[Int]
    designer.connect_nodes(r1, 0, zip_id, 0);

    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "x + 0.25",
        vec![("x".to_string(), DataType::Float)],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 0, expr_id, 0);
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_floats(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![0.25, 1.25, 2.25]);
}

/// A scalar-fed lane broadcasts to a 1-element stream and ends the zip after
/// one element (silently allowed by design — constants belong in captures).
#[test]
fn zip_scalar_broadcast_lane_yields_single_element() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", None, Some((10, 10, 5)));
    let k_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "main", k_id, Box::new(IntData { value: 7 }));
    designer.connect_nodes(k_id, 0, zip_id, 0);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![17]);
}

// ============================================================================
// Errors
// ============================================================================

/// An unwired lane is a per-node eval error (`xs2 input is missing`).
#[test]
fn zip_unwired_lane_is_missing_input_error() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 3)), None);
    let result = evaluate_node(&designer, "main", zip_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(msg.contains("xs2"), "unexpected error message: {}", msg)
        }
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

/// A malformed body (no zone-output wire) is a single construction-time error,
/// not a per-element one.
#[test]
fn zip_malformed_body_single_construction_error() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let r2 = add_range(&mut designer, "main", 0, 1, 3, 200.0);
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);
    // No body, no zone-output wire.

    let result = evaluate_node(&designer, "main", zip_id);
    match result {
        NetworkResult::Error(msg) => assert!(
            msg.contains("zip_with") && msg.contains("zone-output"),
            "unexpected error message: {}",
            msg
        ),
        other => panic!("expected Error, got {}", other.to_display_string()),
    }
}

// ============================================================================
// Wired `f` (evaluation path only — layout derivation is Phase 2)
// ============================================================================

/// A `closure` node (`Custom`, 2 params) wired into `f` drives the zip; the
/// (empty) inline body is ignored.
#[test]
fn zip_wired_f_closure_drives_evaluation() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        600.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0); // [0,1,2]
    let r2 = add_range(&mut designer, "main", 5, 1, 3, 200.0); // [5,6,7]
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);

    // closure C: (a: Int, b: Int) -> Int = a*10 + b.
    let c_id = designer.add_node("closure", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        c_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int, DataType::Int],
            param_names: vec!["a".to_string(), "b".to_string()],
            custom_label: None,
        }),
    );
    let c_expr_id = add_expr_to_body(
        &mut designer,
        "main",
        c_id,
        "a * 10 + b",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", c_id, 0, c_expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", c_id, 1, c_expr_id, 1);
    wire_body_node_to_zone_output(&mut designer, "main", c_id, c_expr_id);

    // Wire C into the zip's `f` (pin index 2 = after the two lanes).
    designer.connect_nodes(c_id, 0, zip_id, 2);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![5, 16, 27]);
}

/// Auto-partialization: a 3-param closure on a 2-lane zip yields `Function`
/// elements with one remaining parameter.
#[test]
fn zip_wired_f_excess_arity_partializes() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        600.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 2, 0.0);
    let r2 = add_range(&mut designer, "main", 0, 1, 2, 200.0);
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);

    let c_id = designer.add_node("closure", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        c_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int, DataType::Int, DataType::Int],
            param_names: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            custom_label: None,
        }),
    );
    let c_expr_id = add_expr_to_body(
        &mut designer,
        "main",
        c_id,
        "a + b + c",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
            ("c".to_string(), DataType::Int),
        ],
    );
    for pin in 0..3 {
        wire_zone_input_pin_to_body_node(&mut designer, "main", c_id, pin, c_expr_id, pin);
    }
    wire_body_node_to_zone_output(&mut designer, "main", c_id, c_expr_id);

    designer.connect_nodes(c_id, 0, zip_id, 2);

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = drain_iter_with_designer(&designer, result);
    assert_eq!(elements.len(), 2);
    for elem in &elements {
        match elem {
            NetworkResult::Function(zc) => {
                assert_eq!(zc.param_types.len(), 1);
                assert_eq!(zc.pre_supplied_args.len(), 2);
            }
            other => panic!(
                "expected Function element, got {}",
                other.to_display_string()
            ),
        }
    }
}

// ============================================================================
// Laziness
// ============================================================================

/// A zip over an effectively unbounded lane and a 3-element lane, drained by a
/// `fold`, terminates immediately: only `min(len)` frames are ever pulled. (If
/// any layer materialized the long lane, this test would hang.)
#[test]
fn zip_is_lazy_pulls_only_min_len() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(
        &mut designer,
        "main",
        Some((0, 1, 1_000_000_000)),
        Some((1, 1, 3)), // [1,2,3]
    );

    // fold(zip, 0) with body `acc + x`.
    let fold_id = designer.add_node("fold", DVec2::new(800.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        fold_id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    let init_id = designer.add_node("int", DVec2::new(600.0, 200.0));
    set_node_data(
        &mut designer,
        "main",
        init_id,
        Box::new(IntData { value: 0 }),
    );
    designer.connect_nodes(zip_id, 0, fold_id, 0);
    designer.connect_nodes(init_id, 0, fold_id, 1);

    let fold_expr_id = add_expr_to_body(
        &mut designer,
        "main",
        fold_id,
        "acc + x",
        vec![
            ("acc".to_string(), DataType::Int),
            ("x".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 0, fold_expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", fold_id, 1, fold_expr_id, 1);
    wire_body_node_to_zone_output(&mut designer, "main", fold_id, fold_expr_id);

    let result = evaluate_node(&designer, "main", fold_id);
    // zip: [0+1, 1+2, 2+3] = [1,3,5]; fold sum = 9.
    match result {
        NetworkResult::Int(v) => assert_eq!(v, 9),
        other => panic!("expected Int(9), got {}", other.to_display_string()),
    }
}

// ============================================================================
// Data-level: lane ids, text properties, healing
// ============================================================================

/// The positional text merge keeps position-stable ids: retype preserves the
/// id, growth mints from `next_lane_id`, shrink drops the tail; an empty lane
/// list is rejected.
#[test]
fn zip_set_text_properties_positional_id_merge() {
    let mut data = zip_data(vec![DataType::Int, DataType::Float], DataType::Int);

    // Retype lane 2 + grow by one: ids [1,2] survive, the new lane mints 3.
    let mut props = HashMap::new();
    props.insert(
        "lane_types".to_string(),
        TextValue::Array(vec![
            TextValue::DataType(DataType::Int),
            TextValue::DataType(DataType::Vec3),
            TextValue::DataType(DataType::Float),
        ]),
    );
    data.set_text_properties(&props).unwrap();
    assert_eq!(data.lanes.len(), 3);
    assert_eq!(data.lanes[0].id, Some(1));
    assert_eq!(data.lanes[1].id, Some(2));
    assert_eq!(data.lanes[1].data_type, DataType::Vec3);
    assert_eq!(data.lanes[2].id, Some(3));
    assert_eq!(data.next_lane_id, 4);

    // Shrink to one lane: tail dropped, surviving lane keeps its id.
    let mut props = HashMap::new();
    props.insert(
        "lane_types".to_string(),
        TextValue::Array(vec![TextValue::DataType(DataType::Int)]),
    );
    data.set_text_properties(&props).unwrap();
    assert_eq!(data.lanes.len(), 1);
    assert_eq!(data.lanes[0].id, Some(1));
    // Consumed ids are not recycled: growing again mints 4, not 2.
    let mut props = HashMap::new();
    props.insert(
        "lane_types".to_string(),
        TextValue::Array(vec![
            TextValue::DataType(DataType::Int),
            TextValue::DataType(DataType::Int),
        ]),
    );
    data.set_text_properties(&props).unwrap();
    assert_eq!(data.lanes[1].id, Some(4));

    // Empty lane list is rejected and leaves the node unchanged.
    let mut props = HashMap::new();
    props.insert("lane_types".to_string(), TextValue::Array(vec![]));
    assert!(data.set_text_properties(&props).is_err());
    assert_eq!(data.lanes.len(), 2);
}

/// Loader healing: id-less lanes get fresh distinct ids and the counter is
/// advanced past every existing id (the `next_param_id` regression shape).
#[test]
fn zip_heal_lane_ids_mints_missing_ids() {
    let mut data = ZipWithData {
        lanes: vec![
            ZipWithLane {
                id: None,
                data_type: DataType::Int,
            },
            ZipWithLane {
                id: Some(5),
                data_type: DataType::Float,
            },
            ZipWithLane {
                id: None,
                data_type: DataType::Int,
            },
        ],
        output_type: DataType::Int,
        next_lane_id: 0, // missing in a hand-authored file
    };
    data.heal_lane_ids();

    let ids: Vec<u64> = data.lanes.iter().map(|l| l.id.unwrap()).collect();
    assert_eq!(ids.len(), 3);
    // All distinct, and none collides with the pre-existing id 5.
    let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
    assert_eq!(unique.len(), 3);
    assert!(ids.contains(&5));
    assert!(data.next_lane_id > *ids.iter().max().unwrap());

    // Idempotent.
    let before = ids.clone();
    data.heal_lane_ids();
    let after: Vec<u64> = data.lanes.iter().map(|l| l.id.unwrap()).collect();
    assert_eq!(before, after);
}

// ============================================================================
// Phase 2: `f`-derivation post-pass + validation polish
// (`doc/design_zip_with.md` §Phase 2)
// ============================================================================

/// The *resolved* output type of `node_id`'s pin 0 — what downstream
/// consumers see, including the post-pass derivation (mirrors
/// `currying_test.rs::phase4_output_type`).
fn resolved_output_type(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> DataType {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    designer
        .node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_type()
        .clone()
}

/// Add a bodiless `closure` node of `Custom` kind with the given signature —
/// sufficient for layout/type derivation tests (the declared output pin type
/// doesn't need a populated body).
fn add_custom_closure(
    designer: &mut StructureDesigner,
    network_name: &str,
    param_types: Vec<DataType>,
    return_type: DataType,
    y: f64,
) -> u64 {
    let c_id = designer.add_node("closure", DVec2::new(0.0, y));
    let n = param_types.len();
    set_node_data(
        designer,
        network_name,
        c_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: param_types.into_iter().chain([return_type]).collect(),
            param_names: (0..n).map(|i| format!("p{}", i)).collect(),
            custom_label: None,
        }),
    );
    c_id
}

/// Disconnect the wire `src.0 → dst.arg_index` by selecting and deleting it
/// (same path the UI takes; re-validates, so the post-pass reruns).
fn disconnect_wire(
    designer: &mut StructureDesigner,
    network_name: &str,
    src: u64,
    dst: u64,
    dst_arg_index: usize,
) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    net.selected_wires.clear();
    net.selected_wires.push(Wire {
        source_node_id: src,
        source_pin: SourcePin::NodeOutput { pin_index: 0 },
        source_scope_depth: 0,
        destination_node_id: dst,
        destination_argument_index: dst_arg_index,
        destination_argument_kind: ArgumentKind::External,
    });
    designer.delete_selected();
}

/// Exact-arity wired `f` derives the output pin type; the stored
/// `output_type` stays untouched; disconnecting `f` restores `Iter[stored]`.
#[test]
fn zip_phase2_wired_f_derives_output_and_disconnect_restores() {
    let mut designer = setup_designer_with_network("main");
    // Stored output_type = Bool, so the derived type is distinguishable.
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Bool,
        400.0,
    );
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Vec3,
        200.0,
    );

    assert!(
        designer.can_connect_nodes(c_id, 0, zip_id, 2),
        "an exact-arity (Int, Int) -> Vec3 source must be accepted on [Int, Int] lanes"
    );
    designer.connect_nodes(c_id, 0, zip_id, 2);

    assert_eq!(
        resolved_output_type(&designer, "main", zip_id),
        DataType::Iterator(Box::new(DataType::Vec3)),
        "wired f must derive the output pin to Iter[Vec3]"
    );

    // The stored ZipWithData is untouched — only the derived layout changed.
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let data = net
            .nodes
            .get(&zip_id)
            .unwrap()
            .data
            .as_any_ref()
            .downcast_ref::<ZipWithData>()
            .unwrap();
        assert_eq!(data.output_type, DataType::Bool);
    }

    disconnect_wire(&mut designer, "main", c_id, zip_id, 2);
    assert_eq!(
        resolved_output_type(&designer, "main", zip_id),
        DataType::Iterator(Box::new(DataType::Bool)),
        "disconnecting f must restore the stored output_type"
    );
}

/// Excess-arity wired `f` derives `Iter[Function(tail → R)]` — the
/// auto-partialization tail.
#[test]
fn zip_phase2_excess_arity_derives_partial_tail() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int, DataType::Float],
        DataType::Int,
        200.0,
    );

    assert!(
        designer.can_connect_nodes(c_id, 0, zip_id, 2),
        "a (Int, Int, Float) -> Int source starts with the lane types and must be accepted"
    );
    designer.connect_nodes(c_id, 0, zip_id, 2);

    assert_eq!(
        resolved_output_type(&designer, "main", zip_id),
        DataType::Iterator(Box::new(DataType::Function(FunctionType::new(
            vec![DataType::Float],
            DataType::Int,
        )))),
        "excess arity must derive Iter[Function((Float) -> Int)]"
    );
}

/// Pinning test: a value-convertible-but-unequal prefix — `(Float, Int)` on
/// `[Int, Int]` lanes — is admitted by the wire-level pairwise-convertibility
/// rule but does NOT derive (the post-pass requires elementwise equality,
/// mirroring map), so the output pin keeps the stored type.
#[test]
fn zip_phase2_convertible_but_unequal_prefix_keeps_stored_output() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Bool,
        400.0,
    );
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Float, DataType::Int],
        DataType::Int,
        200.0,
    );

    assert!(
        designer.can_connect_nodes(c_id, 0, zip_id, 2),
        "pairwise-convertible prefix (Float ~ Int) passes the wire rule (pinning)"
    );
    designer.connect_nodes(c_id, 0, zip_id, 2);

    assert_eq!(
        resolved_output_type(&designer, "main", zip_id),
        DataType::Iterator(Box::new(DataType::Bool)),
        "non-equal prefix must not derive; the stored output_type stays (pinning)"
    );
}

/// A source whose prefix mismatches the lane types outright is rejected at
/// connect time.
#[test]
fn zip_phase2_mismatched_prefix_rejected_at_connect() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Bool, DataType::Int],
        DataType::Int,
        200.0,
    );

    assert!(
        !designer.can_connect_nodes(c_id, 0, zip_id, 2),
        "a (Bool, Int) -> Int source must be rejected on [Int, Int] lanes"
    );
}

/// Zone rule 1 ("zone-output pin needs a wire") is suspended when `f` is
/// connected — `function_input_pin_connected` locates the trailing pin by
/// name + function shape, so the network stays valid with an empty inline
/// body, and evaluation uses the wired closure (pinning test).
#[test]
fn zip_phase2_f_wired_empty_body_is_valid_and_evaluates() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        600.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0); // [0,1,2]
    let r2 = add_range(&mut designer, "main", 5, 1, 3, 200.0); // [5,6,7]
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);

    // Closure with a real body: (a, b) -> a * 10 + b.
    let c_id = designer.add_node("closure", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        c_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int, DataType::Int],
            param_names: vec!["a".to_string(), "b".to_string()],
            custom_label: None,
        }),
    );
    let c_expr_id = add_expr_to_body(
        &mut designer,
        "main",
        c_id,
        "a * 10 + b",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", c_id, 0, c_expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", c_id, 1, c_expr_id, 1);
    wire_body_node_to_zone_output(&mut designer, "main", c_id, c_expr_id);
    designer.connect_nodes(c_id, 0, zip_id, 2);

    designer.validate_active_network();
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        assert!(net.valid, "network must stay valid with an empty zip body");
        assert!(
            net.validation_errors
                .iter()
                .all(|e| e.node_id != Some(zip_id)),
            "no validation error may be attributed to the zip while f is wired \
             (rule-1 suspension must cover the trailing f pin); got: {:?}",
            net.validation_errors
                .iter()
                .filter(|e| e.node_id == Some(zip_id))
                .map(|e| e.error_text.clone())
                .collect::<Vec<_>>()
        );
    }

    let result = evaluate_node(&designer, "main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![5, 16, 27]);
}

/// Two consecutive validate passes leave the derived custom type identical —
/// guards the recompute-every-node discipline (idempotence).
#[test]
fn zip_phase2_post_pass_idempotent() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int, DataType::Float],
        DataType::Vec3,
        200.0,
    );
    designer.connect_nodes(c_id, 0, zip_id, 2);

    designer.validate_active_network();
    let snapshot = {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        net.nodes
            .get(&zip_id)
            .unwrap()
            .custom_node_type
            .clone()
            .expect("zip must carry a custom node type")
    };

    designer.validate_active_network();
    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let after = net
        .nodes
        .get(&zip_id)
        .unwrap()
        .custom_node_type
        .as_ref()
        .expect("zip must still carry a custom node type");

    // `Parameter` has no Debug impl, so compare with plain equality.
    assert!(
        after.parameters == snapshot.parameters,
        "parameters must be unchanged by a second validate"
    );
    assert_eq!(after.output_pins, snapshot.output_pins);
    assert_eq!(after.zone_input_pins, snapshot.zone_input_pins);
    assert!(
        after.zone_output_pins == snapshot.zone_output_pins,
        "zone_output_pins must be unchanged by a second validate"
    );
}

/// A body-internal `zip_with` whose `f` is a cross-scope capture of an outer
/// closure derives its layout too — the scoped-recursion path of the
/// post-pass.
#[test]
fn zip_phase2_body_internal_zip_cross_scope_f_derives() {
    let mut designer = setup_designer_with_network("main");

    // Top level: a closure (Int, Int) -> Vec3 and a map whose body hosts the
    // zip.
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Vec3,
        -200.0,
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

    // Body-internal zip (stored output_type Float — distinguishable from the
    // derived Vec3).
    let zip_id = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.add_node(
            "zip_with",
            DVec2::new(0.0, 0.0),
            3,
            Box::new(zip_data(
                vec![DataType::Int, DataType::Int],
                DataType::Float,
            )),
        )
    };
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Capture the outer closure into the body zip's `f` (pin 2, depth 1).
    wire_capture_to_body_node(&mut designer, "main", map_id, zip_id, 2, c_id);

    designer.validate_active_network();

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let body = net.nodes.get(&map_id).unwrap().zone.as_ref().unwrap();
    let out_ty = body
        .nodes
        .get(&zip_id)
        .unwrap()
        .custom_node_type
        .as_ref()
        .expect("body zip must carry a custom node type")
        .output_type()
        .clone();
    assert_eq!(
        out_ty,
        DataType::Iterator(Box::new(DataType::Vec3)),
        "a cross-scope f capture must derive the body zip's output type"
    );
}

/// The drag hint for the `f` pin exposes the concrete `(T_1..T_N) ->
/// output_type` signature (the declared `AnyFunction` omits the return type);
/// lane pins expose no hint.
#[test]
fn zip_phase2_drag_hint_exposes_concrete_signature() {
    let data = zip_data(vec![DataType::Int, DataType::Vec3], DataType::Float);
    assert_eq!(data.drag_hint_for_input_pin(0), None);
    assert_eq!(data.drag_hint_for_input_pin(1), None);
    assert_eq!(
        data.drag_hint_for_input_pin(2),
        Some(DataType::Function(FunctionType::new(
            vec![DataType::Int, DataType::Vec3],
            DataType::Float,
        )))
    );
}

/// Mirror of `function_pin_test.rs::configure_parameter`: set a parameter
/// node's name and type, preserving an already-minted `param_id`.
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

/// `.cnnd` round-trip where the zip's `f`-source is another network's
/// function pin — the load-order bug class (`apply`'s under-derived layout):
/// every wire must survive the load positionally, and after validation the
/// output type derives and the zip evaluates correctly.
#[test]
fn zip_phase2_cnnd_roundtrip_preserves_wires_and_derives() {
    let mut designer = setup_designer_with_network("mynet");

    // mynet(a: Int, b: Int) -> Int = a * 10 + b, used via its function pin.
    let pa = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    configure_parameter(&mut designer, "mynet", pa, "a", DataType::Int);
    let pb = designer.add_node("parameter", DVec2::new(0.0, 150.0));
    configure_parameter(&mut designer, "mynet", pb, "b", DataType::Int);
    let expr_id = {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("mynet").unwrap();
        let expr_id = add_expr_to_network(
            network,
            "a * 10 + b",
            vec![
                ("a".to_string(), DataType::Int),
                ("b".to_string(), DataType::Int),
            ],
        );
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            registry
                .node_networks
                .get_mut("mynet")
                .unwrap()
                .nodes
                .get_mut(&expr_id)
                .unwrap(),
            true,
        );
        expr_id
    };
    designer.connect_nodes(pa, 0, expr_id, 0);
    designer.connect_nodes(pb, 0, expr_id, 1);
    designer.set_return_node_id(Some(expr_id));
    designer.validate_active_network();

    // Main: two ranges into a 2×Int zip whose f is mynet's function pin.
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    let zip_id = add_zip(
        &mut designer,
        "Main",
        vec![DataType::Int, DataType::Int],
        DataType::Float,
        600.0,
    );
    let r1 = add_range(&mut designer, "Main", 0, 1, 3, 0.0); // [0,1,2]
    let r2 = add_range(&mut designer, "Main", 5, 1, 3, 200.0); // [5,6,7]
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);
    let inst = designer.add_node("mynet", DVec2::new(0.0, 400.0));
    designer.connect_nodes(inst, -1, zip_id, 2); // mynet.fn → f

    // Pre-save sanity: the function-pin source derives the output type.
    assert_eq!(
        resolved_output_type(&designer, "Main", zip_id),
        DataType::Iterator(Box::new(DataType::Int)),
        "pre-save: the function-pin f source must derive Iter[Int]"
    );

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("zip_with_f_load_order.cnnd");
    let path_str = path.to_str().unwrap();
    designer.save_node_networks_as(path_str).unwrap();

    let mut loaded = StructureDesigner::new();
    loaded
        .load_node_networks(path_str)
        .unwrap_or_else(|e| panic!("fixture failed to load: {}", e));

    // Every wire survives the load positionally (the apply bug class).
    {
        let main = loaded
            .node_type_registry
            .node_networks
            .get("Main")
            .expect("Main network");
        let zip_node = main.nodes.get(&zip_id).unwrap();
        assert_eq!(zip_node.arguments.len(), 3, "xs1 + xs2 + f pins");
        for (i, arg) in zip_node.arguments.iter().enumerate() {
            assert_eq!(
                arg.incoming_wires.len(),
                1,
                "the wire on zip pin {} must survive the load",
                i
            );
        }
    }

    // After the standard post-load validation the output type derives and
    // evaluation runs the function-pin closure per frame.
    loaded.set_active_node_network_name(Some("Main".to_string()));
    loaded.validate_active_network();
    assert_eq!(
        resolved_output_type(&loaded, "Main", zip_id),
        DataType::Iterator(Box::new(DataType::Int)),
        "post-load: the derived output type must be restored"
    );
    let result = evaluate_node(&loaded, "Main", zip_id);
    let elements = extract_ints(drain_iter_with_designer(&loaded, result));
    assert_eq!(elements, vec![5, 16, 27]);
}

// ============================================================================
// Phase 3: lane editing — add / remove / retype + repair + undo
// (`doc/design_zip_with.md` §Phase 3)
// ============================================================================

use rust_lib_flutter_cad::structure_designer::node_data::DragDirection;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;
use serde_json::Value;

/// A 3×Int-lane zip whose body computes `element1 + element3` (lane 2 is wired
/// externally but unused in the body — the shape whose evaluation result is
/// unchanged by removing lane 2). Returns (zip_id, r1, r2, r3, expr_id).
fn build_three_lane_ac_zip(designer: &mut StructureDesigner) -> (u64, u64, u64, u64, u64) {
    let zip_id = add_zip(
        designer,
        "main",
        vec![DataType::Int, DataType::Int, DataType::Int],
        DataType::Int,
        600.0,
    );
    let r1 = add_range(designer, "main", 1, 1, 3, 0.0); // [1,2,3]
    let r2 = add_range(designer, "main", 7, 1, 3, 200.0); // [7,8,9]
    let r3 = add_range(designer, "main", 100, 100, 3, 400.0); // [100,200,300]
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);
    designer.connect_nodes(r3, 0, zip_id, 2);

    let expr_id = add_expr_to_body(
        designer,
        "main",
        zip_id,
        "a + c",
        vec![
            ("a".to_string(), DataType::Int),
            ("c".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(designer, "main", zip_id, 0, expr_id, 0);
    wire_zone_input_pin_to_body_node(designer, "main", zip_id, 2, expr_id, 1);
    wire_body_node_to_zone_output(designer, "main", zip_id, expr_id);
    (zip_id, r1, r2, r3, expr_id)
}

/// A 3×Int-lane zip whose body is `range → map → collect`, where the `map`'s
/// own body reads the zip's `element1` and `element3` as **depth-2** deep
/// captures. The map node is the first node added to the zip body, so its
/// body-local id numerically equals the zip's top-level id — the collision
/// that pins the depth+id matching of the remap. Returns
/// (zip_id, map_id, inner_expr_id).
fn build_zip_with_nested_map_deep_captures(designer: &mut StructureDesigner) -> (u64, u64, u64) {
    let zip_id = designer.add_node("zip_with", DVec2::new(600.0, 0.0));
    set_node_data(
        designer,
        "main",
        zip_id,
        Box::new(zip_data(
            vec![DataType::Int, DataType::Int, DataType::Int],
            DataType::Array(Box::new(DataType::Int)),
        )),
    );

    // Zip body: map (added FIRST so its body-local id collides with zip_id),
    // range, collect.
    let (map_id, rng_id, collect_id) = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let map_id = body.add_node(
            "map",
            DVec2::new(200.0, 0.0),
            2,
            Box::new(MapData {
                input_type: DataType::Int,
                output_type: DataType::Int,
            }),
        );
        let rng_id = body.add_node(
            "range",
            DVec2::new(0.0, 0.0),
            3,
            Box::new(RangeData {
                start: 0,
                step: 1,
                count: 2,
            }),
        ); // [0,1]
        let collect_id = body.add_node(
            "collect",
            DVec2::new(400.0, 0.0),
            2,
            Box::new(CollectData {
                element_type: DataType::Int,
                limit: None,
                offset: 0,
            }),
        );
        (map_id, rng_id, collect_id)
    };
    assert_eq!(
        map_id, zip_id,
        "test precondition: the body map's id must numerically collide with the zip's id"
    );

    // Populate body nodes' custom-type caches (also initializes the map's
    // zone state).
    for nid in [map_id, rng_id, collect_id] {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&zip_id)
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

    // Zip-body wiring: map.xs ← range, collect.xs ← map, zip.result ← collect.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.nodes.get_mut(&map_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: rng_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        body.nodes.get_mut(&collect_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let zip_node = network.nodes.get_mut(&zip_id).unwrap();
        if zip_node.zone_output_arguments.is_empty() {
            zip_node.zone_output_arguments.push(Argument::new());
        }
        zip_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: collect_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Map body: expr `x + a + c` — x = the map's own element (depth 1),
    // a/c = the zip's element1/element3 (depth 2 deep captures).
    let inner_expr_id = {
        let map_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let inner_expr_id = add_expr_to_network(
            map_body,
            "x + a + c",
            vec![
                ("x".to_string(), DataType::Int),
                ("a".to_string(), DataType::Int),
                ("c".to_string(), DataType::Int),
            ],
        );
        let expr = map_body.nodes.get_mut(&inner_expr_id).unwrap();
        expr.arguments[0].incoming_wires.push(IncomingWire {
            source_node_id: map_id,
            source_pin: SourcePin::ZoneInput { pin_index: 0 },
            source_scope_depth: 1,
        });
        expr.arguments[1].incoming_wires.push(IncomingWire {
            source_node_id: zip_id,
            source_pin: SourcePin::ZoneInput { pin_index: 0 },
            source_scope_depth: 2,
        });
        expr.arguments[2].incoming_wires.push(IncomingWire {
            source_node_id: zip_id,
            source_pin: SourcePin::ZoneInput { pin_index: 2 },
            source_scope_depth: 2,
        });
        inner_expr_id
    };
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&map_id)
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
            node,
            true,
        );
    }

    // Map's body-return ← expr.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let map_node = body.nodes.get_mut(&map_id).unwrap();
        if map_node.zone_output_arguments.is_empty() {
            map_node.zone_output_arguments.push(Argument::new());
        }
        map_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: inner_expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // External lanes.
    let r1 = add_range(designer, "main", 1, 1, 2, 0.0); // [1,2]
    let r2 = add_range(designer, "main", 5, 1, 2, 200.0); // [5,6]
    let r3 = add_range(designer, "main", 100, 100, 2, 400.0); // [100,200]
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);
    designer.connect_nodes(r3, 0, zip_id, 2);

    (zip_id, map_id, inner_expr_id)
}

/// Drain the zip's `Iter[Array[Int]]` output into a Vec<Vec<i32>>.
fn drain_int_arrays(designer: &StructureDesigner, result: NetworkResult) -> Vec<Vec<i32>> {
    drain_iter_with_designer(designer, result)
        .into_iter()
        .map(|r| match r {
            NetworkResult::Array(items) => extract_ints(items),
            other => panic!("expected Array element, got {}", other.to_display_string()),
        })
        .collect()
}

/// The (source_node_id, zone-input pin_index, depth) triple of the single wire
/// on the given body-node argument; panics on a different wire shape.
fn zone_input_wire_of(arg: &Argument) -> (u64, usize, u8) {
    assert_eq!(arg.incoming_wires.len(), 1, "expected exactly one wire");
    let w = &arg.incoming_wires[0];
    match w.source_pin {
        SourcePin::ZoneInput { pin_index } => (w.source_node_id, pin_index, w.source_scope_depth),
        SourcePin::NodeOutput { .. } => panic!("expected a ZoneInput wire"),
    }
}

/// Normalized whole-network JSON for undo/redo state comparison — sorts the
/// HashMap-derived arrays (`nodes`, `displayed_node_ids`,
/// `displayed_output_pins`) the way `undo_test.rs::normalize_json` does.
fn network_json(designer: &mut StructureDesigner, name: &str) -> Value {
    let snapshot = designer
        .snapshot_network(name)
        .expect("network must snapshot");
    let mut value = serde_json::to_value(&snapshot).unwrap();
    normalize_network_json(&mut value);
    value
}

fn normalize_network_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if key == "displayed_node_ids" || key == "displayed_output_pins" {
                    if let Value::Array(arr) = val {
                        arr.sort_by(|a, b| {
                            let id_a = a
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let id_b = b
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            id_a.cmp(&id_b)
                        });
                    }
                    normalize_network_json(val);
                } else if key == "nodes" {
                    if let Value::Array(arr) = val {
                        arr.sort_by(|a, b| {
                            let id_a = a
                                .as_object()
                                .and_then(|o| o.get("id"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let id_b = b
                                .as_object()
                                .and_then(|o| o.get("id"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            id_a.cmp(&id_b)
                        });
                    }
                    normalize_network_json(val);
                } else {
                    normalize_network_json(val);
                }
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                normalize_network_json(val);
            }
        }
        _ => {}
    }
}

/// Design test 1 — remove the middle lane of 3 (id-accurate path): the removed
/// lane's external wire is dropped, the later lane's wire survives on the
/// renumbered pin (old `xs3` → new `xs2`, same id), body wires to the removed
/// `element2` index are remapped (here: the `element3` wire decrements to
/// index 1), the network re-validates without manual fixes, and the
/// **evaluation result** is unchanged for the surviving lanes.
#[test]
fn zip_phase3_remove_middle_lane_preserves_surviving_wires() {
    let mut designer = setup_designer_with_network("main");
    let (zip_id, r1, r2, r3, expr_id) = build_three_lane_ac_zip(&mut designer);

    let before = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", zip_id),
    ));
    assert_eq!(before, vec![101, 202, 303]);

    designer
        .remove_zip_with_lane(&[], zip_id, 1)
        .expect("middle-lane removal must succeed");

    {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let zip_node = net.nodes.get(&zip_id).unwrap();
        let ct = zip_node.custom_node_type.as_ref().unwrap();
        assert_eq!(ct.parameters.len(), 3, "xs1 + xs2 + f");
        assert_eq!(ct.parameters[0].name, "xs1");
        assert_eq!(ct.parameters[1].name, "xs2");
        assert_eq!(
            ct.parameters[1].id,
            Some(3),
            "the renumbered xs2 must carry the old lane 3's id"
        );

        // External wires: the removed lane's wire is gone, the later lane's
        // wire followed its id onto the renumbered pin.
        assert_eq!(zip_node.arguments[0].get_node_id(), Some(r1));
        assert_eq!(
            zip_node.arguments[1].get_node_id(),
            Some(r3),
            "old xs3's wire must survive on the renumbered xs2"
        );
        assert!(
            !zip_node.arguments.iter().any(|a| a.has_source(r2)),
            "the removed lane's external wire must be dropped"
        );

        // Body wires: `a` untouched at index 0, `c` decremented 2 → 1.
        let body = zip_node.zone.as_ref().unwrap();
        let expr = body.nodes.get(&expr_id).unwrap();
        assert_eq!(zone_input_wire_of(&expr.arguments[0]), (zip_id, 0, 1));
        assert_eq!(
            zone_input_wire_of(&expr.arguments[1]),
            (zip_id, 1, 1),
            "the element3 body wire must be decremented to index 1"
        );

        assert!(
            net.valid,
            "the network must re-validate without manual fixes; errors: {:?}",
            net.validation_errors
                .iter()
                .map(|e| e.error_text.clone())
                .collect::<Vec<_>>()
        );
    }

    let after = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", zip_id),
    ));
    assert_eq!(
        after, before,
        "evaluation must be unchanged for the surviving lanes"
    );
}

/// Design test 2 — nested-body remap with an id collision: a `map` inside the
/// zip body (whose body-local id numerically equals the zip's id) deep-reads
/// the zip's `element1` and `element3` at depth 2. Removing lane 2 decrements
/// the depth-2 `element3` wire to index 1, leaves the depth-2 `element1` wire
/// untouched, and leaves the map's **own** depth-1 `element` wire untouched —
/// a match on `source_node_id` alone would corrupt it. Asserted by evaluation
/// result, not just structure.
#[test]
fn zip_phase3_nested_body_remap_with_id_collision() {
    let mut designer = setup_designer_with_network("main");
    let (zip_id, map_id, inner_expr_id) = build_zip_with_nested_map_deep_captures(&mut designer);

    let before = drain_int_arrays(&designer, evaluate_node(&designer, "main", zip_id));
    assert_eq!(before, vec![vec![101, 102], vec![202, 203]]);

    designer
        .remove_zip_with_lane(&[], zip_id, 1)
        .expect("removal must succeed");

    {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let map_body = net
            .nodes
            .get(&zip_id)
            .unwrap()
            .zone
            .as_ref()
            .unwrap()
            .nodes
            .get(&map_id)
            .unwrap()
            .zone
            .as_ref()
            .unwrap();
        let expr = map_body.nodes.get(&inner_expr_id).unwrap();
        assert_eq!(
            zone_input_wire_of(&expr.arguments[0]),
            (map_id, 0, 1),
            "the map's own depth-1 element wire must be untouched despite the id collision"
        );
        assert_eq!(
            zone_input_wire_of(&expr.arguments[1]),
            (zip_id, 0, 2),
            "the depth-2 element1 wire must be untouched"
        );
        assert_eq!(
            zone_input_wire_of(&expr.arguments[2]),
            (zip_id, 1, 2),
            "the depth-2 element3 wire must be decremented to index 1"
        );
    }

    let after = drain_int_arrays(&designer, evaluate_node(&designer, "main", zip_id));
    assert_eq!(after, before, "evaluation must be unchanged");
}

/// Design test 3 — remove the last lane: no renumbering, only the dropped
/// lane's wires (external + body) disappear.
#[test]
fn zip_phase3_remove_last_lane_drops_only_its_wires() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int, DataType::Int],
        DataType::Int,
        600.0,
    );
    let r1 = add_range(&mut designer, "main", 1, 1, 3, 0.0);
    let r2 = add_range(&mut designer, "main", 10, 10, 3, 200.0);
    let r3 = add_range(&mut designer, "main", 100, 100, 3, 400.0);
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);
    designer.connect_nodes(r3, 0, zip_id, 2);
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "a + b + c",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
            ("c".to_string(), DataType::Int),
        ],
    );
    for pin in 0..3 {
        wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, pin, expr_id, pin);
    }
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    designer
        .remove_zip_with_lane(&[], zip_id, 2)
        .expect("last-lane removal must succeed");

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let zip_node = net.nodes.get(&zip_id).unwrap();
    let ct = zip_node.custom_node_type.as_ref().unwrap();
    assert_eq!(ct.parameters.len(), 3, "xs1 + xs2 + f");
    assert_eq!(ct.parameters[0].id, Some(1));
    assert_eq!(ct.parameters[1].id, Some(2));
    assert_eq!(zip_node.arguments[0].get_node_id(), Some(r1));
    assert_eq!(zip_node.arguments[1].get_node_id(), Some(r2));
    assert!(
        !zip_node.arguments.iter().any(|a| a.has_source(r3)),
        "the dropped lane's external wire must be gone"
    );

    let body = zip_node.zone.as_ref().unwrap();
    let expr = body.nodes.get(&expr_id).unwrap();
    assert_eq!(zone_input_wire_of(&expr.arguments[0]), (zip_id, 0, 1));
    assert_eq!(zone_input_wire_of(&expr.arguments[1]), (zip_id, 1, 1));
    assert!(
        expr.arguments[2].is_empty(),
        "the body wire to the dropped element3 must be disconnected"
    );
}

/// Design test 4 — retype a lane `Int → Crystal`: the now-incompatible body
/// wire is disconnected (`repair_zone_body`), the compatible one is kept, the
/// lane keeps its id, and the external wire stays for the usual wire-type
/// revalidation to flag.
#[test]
fn zip_phase3_retype_lane_drops_incompatible_body_wires() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 3)), Some((10, 10, 3)));

    designer
        .set_zip_with_lanes(&[], zip_id, vec![DataType::Int, DataType::Crystal])
        .expect("retype must succeed");

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let zip_node = net.nodes.get(&zip_id).unwrap();

    // Retype preserves lane identity.
    let data = zip_node
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap();
    assert_eq!(data.lanes[1].id, Some(2));
    assert_eq!(data.lanes[1].data_type, DataType::Crystal);

    // Body: the Crystal element2 → Int expr param wire is dropped; the
    // untouched Int lane's wire is kept.
    let body = zip_node.zone.as_ref().unwrap();
    let expr = body
        .nodes
        .values()
        .find(|n| n.node_type_name == "expr")
        .expect("body expr");
    assert_eq!(zone_input_wire_of(&expr.arguments[0]), (zip_id, 0, 1));
    assert!(
        expr.arguments[1].is_empty(),
        "the retype-incompatible body wire must be disconnected by repair"
    );

    // The external Iter[Int] → Iter[Crystal] wire stays and validation flags
    // it (the usual wire-type revalidation).
    assert_eq!(zip_node.arguments[1].len(), 1);
    assert!(
        !net.valid,
        "the incompatible external wire must flag the network invalid"
    );
}

/// Design test 5 — add a lane: the new pins appear unwired, existing wires
/// are untouched, and the fresh id is minted from `next_lane_id`, never
/// recycling a consumed id (the `next_param_id` regression shape): remove the
/// highest-id lane, grow back, and the removed id must not reappear.
#[test]
fn zip_phase3_add_lane_never_recycles_ids() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 3)), Some((10, 10, 3)));

    // Remove the highest-id lane (index 1, id 2)…
    designer.remove_zip_with_lane(&[], zip_id, 1).unwrap();
    // …then grow back to two lanes.
    designer
        .set_zip_with_lanes(&[], zip_id, vec![DataType::Int, DataType::Int])
        .unwrap();

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let zip_node = net.nodes.get(&zip_id).unwrap();
    let data = zip_node
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap();
    assert_eq!(data.lanes[0].id, Some(1));
    assert_eq!(
        data.lanes[1].id,
        Some(3),
        "the new lane must mint id 3 from the counter, not recycle the removed id 2"
    );
    assert_eq!(data.next_lane_id, 4);

    // Existing wire untouched, new pin unwired.
    assert_eq!(zip_node.arguments[0].len(), 1);
    assert!(zip_node.arguments[1].is_empty(), "new xs2 must be unwired");
    let ct = zip_node.custom_node_type.as_ref().unwrap();
    assert_eq!(ct.parameters[1].id, Some(3));
}

/// Design test 6 — positional text merge: a `lane_types` shrink through
/// `set_text_properties` (the `edit_network` incremental path) preserves
/// position-stable ids AND **disconnects** a nested depth-2 wire to a dropped
/// tail index (not just flags it red).
#[test]
fn zip_phase3_text_merge_shrink_disconnects_nested_wires() {
    let mut designer = setup_designer_with_network("main");
    let (zip_id, map_id, inner_expr_id) = build_zip_with_nested_map_deep_captures(&mut designer);

    // Bind the node to a text-format name.
    designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .nodes
        .get_mut(&zip_id)
        .unwrap()
        .custom_name = Some("z".to_string());

    // Incremental text edit shrinking the lane list 3 → 2 (a tail drop — the
    // text format has no way to say "remove lane 2 specifically").
    let result = {
        let mut network = designer
            .node_type_registry
            .node_networks
            .remove("main")
            .unwrap();
        let result = edit_network(
            &mut network,
            &designer.node_type_registry,
            "z = zip_with { lane_types: [Int, Int] }",
            false,
        );
        designer
            .node_type_registry
            .node_networks
            .insert("main".to_string(), network);
        result
    };
    assert!(
        result.success,
        "text edit must succeed: {:?}",
        result.errors
    );

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let zip_node = net.nodes.get(&zip_id).unwrap();
    let data = zip_node
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap();
    assert_eq!(data.lanes.len(), 2);
    assert_eq!(data.lanes[0].id, Some(1), "position-stable id preserved");
    assert_eq!(data.lanes[1].id, Some(2), "position-stable id preserved");

    let map_body = zip_node
        .zone
        .as_ref()
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap();
    let expr = map_body.nodes.get(&inner_expr_id).unwrap();
    assert_eq!(
        zone_input_wire_of(&expr.arguments[0]),
        (map_id, 0, 1),
        "the map's own element wire must be untouched"
    );
    assert_eq!(
        zone_input_wire_of(&expr.arguments[1]),
        (zip_id, 0, 2),
        "the depth-2 wire to a surviving index must be untouched (no decrement on a tail drop)"
    );
    assert!(
        expr.arguments[2].is_empty(),
        "the nested depth-2 wire to the dropped tail index must be disconnected, not just flagged"
    );
}

/// Design test 7 — minimum arity enforced: `remove_zip_with_lane` on a 1-lane
/// node and an empty lane list through `set_zip_with_lanes` both return an
/// error and leave the node unchanged.
#[test]
fn zip_phase3_minimum_arity_enforced() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int],
        DataType::Int,
        400.0,
    );

    assert!(designer.remove_zip_with_lane(&[], zip_id, 0).is_err());
    assert!(designer.set_zip_with_lanes(&[], zip_id, vec![]).is_err());
    // Out-of-range index is also rejected cleanly.
    assert!(designer.remove_zip_with_lane(&[], zip_id, 5).is_err());

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let data = net
        .nodes
        .get(&zip_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap();
    assert_eq!(data.lanes.len(), 1);
    assert_eq!(data.lanes[0].id, Some(1));
}

/// Design test 8 (pinning) — a bare scalar wired to a lane produces no
/// validation error or warning: the implicit `S → Iter[T]` broadcast is
/// silently allowed by design (the 1-element evaluation behavior is pinned by
/// `zip_scalar_broadcast_lane_yields_single_element`).
#[test]
fn zip_phase3_scalar_broadcast_no_validation_error() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", None, Some((10, 10, 5)));
    let k_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "main", k_id, Box::new(IntData { value: 7 }));
    designer.connect_nodes(k_id, 0, zip_id, 0);

    designer.validate_active_network();
    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(net.valid);
    assert!(
        net.validation_errors
            .iter()
            .all(|e| e.node_id != Some(zip_id)),
        "a scalar-fed lane must produce no error or warning on the zip; got: {:?}",
        net.validation_errors
            .iter()
            .filter(|e| e.node_id == Some(zip_id))
            .map(|e| e.error_text.clone())
            .collect::<Vec<_>>()
    );
}

/// Phase 3 deliverable — `adapt_for_drag_source` peels the drag source's
/// element type into lane 1 and `output_type`, leaving lane 2 at its Float
/// default.
#[test]
fn zip_phase3_adapt_for_drag_source_peels_element_type() {
    let registry = NodeTypeRegistry::new();
    let data = ZipWithData::default();

    let adapted = data
        .adapt_for_drag_source(
            &DataType::Iterator(Box::new(DataType::Vec3)),
            DragDirection::FromOutput,
            &registry,
        )
        .expect("an Iter source must adapt");
    let adapted = adapted.as_any_ref().downcast_ref::<ZipWithData>().unwrap();
    assert_eq!(adapted.lanes[0].data_type, DataType::Vec3);
    assert_eq!(adapted.lanes[1].data_type, DataType::Float, "lane 2 stays");
    assert_eq!(adapted.output_type, DataType::Vec3);
}

/// Design test 10a — undo/redo of an id-accurate middle-lane removal with
/// wires attached in the immediate body: the whole-network JSON state
/// (including `next_lane_id`, the removed lane's external wire, and the
/// remapped body wires) is restored exactly.
#[test]
fn zip_phase3_undo_redo_remove_middle_lane() {
    let mut designer = setup_designer_with_network("main");
    let (zip_id, _r1, _r2, _r3, _expr_id) = build_three_lane_ac_zip(&mut designer);
    designer.validate_active_network();

    let before = network_json(&mut designer, "main");
    designer.remove_zip_with_lane(&[], zip_id, 1).unwrap();
    let after = network_json(&mut designer, "main");
    assert_ne!(before, after);

    assert!(designer.undo(), "undo must report a command");
    assert_eq!(
        network_json(&mut designer, "main"),
        before,
        "undo must restore the exact whole-network state"
    );
    let restored = extract_ints(drain_iter_with_designer(
        &designer,
        evaluate_node(&designer, "main", zip_id),
    ));
    assert_eq!(
        restored,
        vec![101, 202, 303],
        "undone network must evaluate"
    );

    assert!(designer.redo(), "redo must report a command");
    assert_eq!(
        network_json(&mut designer, "main"),
        after,
        "redo must restore the exact post-edit state"
    );
}

/// Design test 10b — undo/redo of a removal whose body remap reaches a
/// nested body (the depth-2 wires of `zip_phase3_nested_body_remap…`).
#[test]
fn zip_phase3_undo_redo_nested_body_removal() {
    let mut designer = setup_designer_with_network("main");
    let (zip_id, _map_id, _inner_expr_id) = build_zip_with_nested_map_deep_captures(&mut designer);
    designer.validate_active_network();

    let before = network_json(&mut designer, "main");
    designer.remove_zip_with_lane(&[], zip_id, 1).unwrap();
    let after = network_json(&mut designer, "main");
    assert_ne!(before, after);

    assert!(designer.undo());
    assert_eq!(network_json(&mut designer, "main"), before);
    let restored = drain_int_arrays(&designer, evaluate_node(&designer, "main", zip_id));
    assert_eq!(restored, vec![vec![101, 102], vec![202, 203]]);

    assert!(designer.redo());
    assert_eq!(network_json(&mut designer, "main"), after);
}

/// Design test 10c — undo/redo of a whole-list edit that retypes AND shrinks
/// (dropping a body wire the node-data snapshot could never restore).
#[test]
fn zip_phase3_undo_redo_retype_and_shrink() {
    let mut designer = setup_designer_with_network("main");
    let (zip_id, _r1, _r2, _r3, _expr_id) = build_three_lane_ac_zip(&mut designer);
    designer.validate_active_network();

    let before = network_json(&mut designer, "main");
    designer
        .set_zip_with_lanes(&[], zip_id, vec![DataType::Int, DataType::Float])
        .unwrap();
    let after = network_json(&mut designer, "main");
    assert_ne!(before, after);

    assert!(designer.undo());
    assert_eq!(network_json(&mut designer, "main"), before);
    assert!(designer.redo());
    assert_eq!(network_json(&mut designer, "main"), after);

    // A no-op whole-list edit pushes nothing: after undoing, re-setting the
    // identical lane list must not truncate the redo tail (a pushed command
    // would).
    designer.undo();
    designer
        .set_zip_with_lanes(
            &[],
            zip_id,
            vec![DataType::Int, DataType::Int, DataType::Int],
        )
        .unwrap();
    assert!(
        designer.redo(),
        "a no-op lane edit must not have pushed a command (the redo tail survives)"
    );
    assert_eq!(network_json(&mut designer, "main"), after);
}

// ============================================================================
// Phase 4: text format + serialization round-trips
// (`doc/design_zip_with.md` §Phase 4)
//
// `zip_with` uses `generic_node_data_saver` + a loader that heals persisted
// lane-id state. These tests drive that saver/loader through the same
// serializable form the `.cnnd` file path uses (`node_network_to_serializable`
// → JSON → `serializable_to_node_network`), covering a populated inline body,
// captures, a wired `f`, a body-internal (nested) zip, and hand-authored files
// with missing lane ids. (Text-format parse/serialize round-trips live in
// `text_format_test.rs::zip_with_text_format_tests`; zone bodies are not part
// of the text format, only the JSON form.)
// ============================================================================

use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, node_network_to_serializable, serializable_to_node_network,
};

/// Round-trip a network through the serializable (`.cnnd`) form —
/// saver → JSON string → loader (`serializable_to_node_network`, which invokes
/// each node's `node_data_loader`, i.e. `zip_with_node_data_loader` + id
/// healing) → saver again — and assert the normalized JSON is byte-identical.
/// Exercises the recursive zone-body serialization for any HOF present.
fn assert_cnnd_roundtrip(designer: &mut StructureDesigner, name: &str) {
    let before = network_json(designer, name);

    let snap = designer.snapshot_network(name).expect("snapshot network");
    let json = serde_json::to_string(&snap).unwrap();
    let reloaded: SerializableNodeNetwork = serde_json::from_str(&json).unwrap();

    let built_in = &designer.node_type_registry.built_in_node_types;
    let mut net2 =
        serializable_to_node_network(&reloaded, built_in, None).expect("load network back");
    let snap2 = node_network_to_serializable(&mut net2, built_in, None).unwrap();
    let mut after = serde_json::to_value(&snap2).unwrap();
    normalize_network_json(&mut after);

    assert_eq!(
        before, after,
        "cnnd roundtrip changed the network JSON for '{}'",
        name
    );
}

/// Design test 4 (top-level): a `zip_with` with a populated inline body, a
/// depth-1 capture, and a wired `f` round-trips exactly.
#[test]
fn zip_phase4_cnnd_roundtrip_body_capture_and_wired_f() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        400.0,
    );
    let r1 = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let r2 = add_range(&mut designer, "main", 0, 10, 3, 200.0);
    designer.connect_nodes(r1, 0, zip_id, 0);
    designer.connect_nodes(r2, 0, zip_id, 1);

    // Inline body `a + b + k` where k is a depth-1 capture of an outer int.
    let k_id = designer.add_node("int", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "main",
        k_id,
        Box::new(IntData { value: 1000 }),
    );
    let expr_id = add_expr_to_body(
        &mut designer,
        "main",
        zip_id,
        "a + b + k",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
            ("k".to_string(), DataType::Int),
        ],
    );
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 0, expr_id, 0);
    wire_zone_input_pin_to_body_node(&mut designer, "main", zip_id, 1, expr_id, 1);
    wire_capture_to_body_node(&mut designer, "main", zip_id, expr_id, 2, k_id);
    wire_body_node_to_zone_output(&mut designer, "main", zip_id, expr_id);

    // Also wire a (bodiless) closure into `f` so the wired-`f` argument is part
    // of the serialized shape (the closure wins at eval; here we only round-trip
    // the structure).
    let c_id = add_custom_closure(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        500.0,
    );
    designer.connect_nodes(c_id, 0, zip_id, 2);

    assert_cnnd_roundtrip(&mut designer, "main");
}

/// Design test 4 (nested): a body-internal `zip_with` (inside a `map` zone),
/// with its own inline body, survives the roundtrip — a zone inside a zone.
#[test]
fn zip_phase4_cnnd_roundtrip_nested_body_internal_zip() {
    let mut designer = setup_designer_with_network("main");
    let outer = add_range(&mut designer, "main", 0, 1, 2, 0.0);
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
    designer.connect_nodes(outer, 0, map_id, 0);

    // Map body: two ranges + a body-internal zip (2 lanes).
    let zip_id = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let r1_id = body.add_node(
            "range",
            DVec2::new(0.0, 0.0),
            3,
            Box::new(RangeData {
                start: 1,
                step: 1,
                count: 3,
            }),
        );
        let r2_id = body.add_node(
            "range",
            DVec2::new(0.0, 100.0),
            3,
            Box::new(RangeData {
                start: 10,
                step: 10,
                count: 2,
            }),
        );
        let zip_id = body.add_node(
            "zip_with",
            DVec2::new(200.0, 0.0),
            3,
            Box::new(zip_data(vec![DataType::Int, DataType::Int], DataType::Int)),
        );
        // xs1 <- r1, xs2 <- r2 (intra-body wires).
        body.nodes.get_mut(&zip_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: r1_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        body.nodes.get_mut(&zip_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: r2_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        zip_id
    };

    // Populate the body-internal zip's cache — this initializes its inline zone
    // body (`ensure_zone_init`), which the roundtrip must then preserve.
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // The body-internal zip's own inline body: expr `a + b`, element pins +
    // zone-output wired.
    {
        let zip_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let expr_id = add_expr_to_network(
            zip_body,
            "a + b",
            vec![
                ("a".to_string(), DataType::Int),
                ("b".to_string(), DataType::Int),
            ],
        );
        zip_body.nodes.get_mut(&expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: zip_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        zip_body.nodes.get_mut(&expr_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: zip_id,
                source_pin: SourcePin::ZoneInput { pin_index: 1 },
                source_scope_depth: 1,
            });
        let zip_node = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&zip_id)
            .unwrap();
        if zip_node.zone_output_arguments.is_empty() {
            zip_node.zone_output_arguments.push(Argument::new());
        }
        zip_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    assert_cnnd_roundtrip(&mut designer, "main");
}

/// Set every lane's `id` to null and zero `next_lane_id` on the zip node's
/// serialized `data` blob — the shape a hand-authored `.cnnd` file has.
fn strip_zip_lane_ids(network_json: &mut Value, zip_id: u64) {
    let nodes = network_json
        .get_mut("nodes")
        .and_then(|v| v.as_array_mut())
        .expect("nodes array");
    for node in nodes {
        if node.get("id").and_then(|v| v.as_u64()) == Some(zip_id) {
            let data = node.get_mut("data").expect("node data");
            data["next_lane_id"] = Value::from(0u64);
            for lane in data
                .get_mut("lanes")
                .and_then(|v| v.as_array_mut())
                .expect("lanes array")
            {
                lane["id"] = Value::Null;
            }
        }
    }
}

/// Design test 6 — healing: a hand-authored file with lanes missing `id` and a
/// zero `next_lane_id` loads with fresh distinct lane ids and a consistent
/// counter, and the external lane wires (positional, id-independent) survive.
#[test]
fn zip_phase4_load_heals_missing_lane_ids() {
    let mut designer = setup_designer_with_network("main");
    let zip_id = build_sum_zip(&mut designer, "main", Some((0, 1, 3)), Some((10, 10, 3)));

    let snap = designer.snapshot_network("main").unwrap();
    let mut json = serde_json::to_value(&snap).unwrap();
    strip_zip_lane_ids(&mut json, zip_id);

    let reloaded: SerializableNodeNetwork = serde_json::from_value(json).unwrap();
    let built_in = &designer.node_type_registry.built_in_node_types;
    let net2 = serializable_to_node_network(&reloaded, built_in, None).expect("load heals");

    let z_node = net2.nodes.get(&zip_id).unwrap();
    let data = z_node
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap();
    // Every lane got a fresh id, all distinct, and the counter is past them.
    let ids: Vec<u64> = data
        .lanes
        .iter()
        .map(|l| l.id.expect("healed id"))
        .collect();
    let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "healed lane ids must be distinct");
    assert!(
        data.next_lane_id > *ids.iter().max().unwrap(),
        "next_lane_id must be past every healed id"
    );
    // The external lane wires are positional and independent of ids, so they
    // survive the id-less load.
    assert!(
        !z_node.arguments[0].is_empty() && !z_node.arguments[1].is_empty(),
        "external lane wires must survive an id-less load"
    );
}

// ============================================================================
// Phase 5 — API-facing scoped setter / removal (body-internal node)
// ============================================================================

/// Stored lane types of a `zip_with` nested inside a `map` body (mirrors what
/// `get_zip_with_data` reads over the API, resolved through the map's scope).
fn body_zip_lane_types(
    designer: &StructureDesigner,
    network_name: &str,
    map_id: u64,
    zip_id: u64,
) -> Vec<DataType> {
    designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .zone
        .as_ref()
        .unwrap()
        .nodes
        .get(&zip_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap()
        .lane_types()
}

/// Stored lane types of a top-level `zip_with`.
fn top_level_zip_lane_types(
    designer: &StructureDesigner,
    network_name: &str,
    zip_id: u64,
) -> Vec<DataType> {
    designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap()
        .nodes
        .get(&zip_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ZipWithData>()
        .unwrap()
        .lane_types()
}

/// The API layer drives `set_zip_with_data` / `remove_zip_with_lane` with a
/// `scope_path`, so a body-internal `zip_with` must be addressed through its
/// containing HOF's scope — never by bare id (the property-panel-wrong-node bug
/// class, `rust/AGENTS.md`). A decoy top-level `zip_with` must be left untouched
/// by the scoped edits, proving the scope routes into the map's body.
#[test]
fn zip_phase5_scoped_setter_and_removal_target_body_node() {
    let mut designer = setup_designer_with_network("main");

    // Decoy top-level 2-lane zip — must never be touched by a scoped edit.
    let decoy_id = add_zip(
        &mut designer,
        "main",
        vec![DataType::Int, DataType::Int],
        DataType::Int,
        0.0,
    );

    // A `map` whose body holds the zip we actually edit.
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

    let body_zip_id = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.add_node(
            "zip_with",
            DVec2::new(100.0, 0.0),
            3,
            Box::new(zip_data(vec![DataType::Int, DataType::Int], DataType::Int)),
        )
    };
    // Initialize the body zip's custom-type cache + inline zone.
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&body_zip_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Grow the body zip to 3 lanes through the scoped combined setter.
    designer
        .set_zip_with_data(
            &[map_id],
            body_zip_id,
            vec![DataType::Int, DataType::Int, DataType::Int],
            DataType::Int,
        )
        .expect("scoped set must succeed");
    assert_eq!(
        body_zip_lane_types(&designer, "main", map_id, body_zip_id).len(),
        3,
        "the body-internal zip must gain a lane"
    );
    assert_eq!(
        top_level_zip_lane_types(&designer, "main", decoy_id).len(),
        2,
        "the decoy top-level zip must be untouched by a scoped edit"
    );

    // Remove the middle lane through the scoped id-accurate removal.
    designer
        .remove_zip_with_lane(&[map_id], body_zip_id, 1)
        .expect("scoped removal must succeed");
    assert_eq!(
        body_zip_lane_types(&designer, "main", map_id, body_zip_id).len(),
        2,
        "the body-internal zip must lose the removed lane"
    );
    assert_eq!(
        top_level_zip_lane_types(&designer, "main", decoy_id).len(),
        2,
        "the decoy stays at 2 lanes after a scoped removal"
    );
}
