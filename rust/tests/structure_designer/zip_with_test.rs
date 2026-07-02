//! Phase 1 tests for the `zip_with` node (`doc/design_zip_with.md`,
//! issue #382): the core node + `ZipZone` walker over the inline-body path,
//! plus the wired-`f` evaluation path.
//!
//! Network construction mirrors `zones_test.rs` (direct manipulation of the
//! HOF node's owned body network — mind the per-body `next_node_id` /
//! `num_params` gotchas documented there). Walker-level `ZipZone` tests live
//! in `iterator_walker_test.rs`.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{Argument, IncomingWire, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
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
