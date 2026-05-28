//! Function-pin tests (design_function_pins.md, Phase 1): a node's function pin
//! (`output_pin_index == -1`) produces a real `NetworkResult::Function` value,
//! synthesized on demand from the node viewed as a function of *all* its inputs,
//! and consumed by the four HOFs and `apply` exactly like a `closure` node's
//! output.
//!
//! Like `closures_test.rs` / `zones_test.rs`, these construct the function-pin
//! wire programmatically (no UI / connection gating — that lands in Phase 2),
//! by pushing an `IncomingWire { source_pin: NodeOutput { pin_index: -1 } }`
//! straight onto the consumer's `f` argument.

use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::{
    GeometryVisualization, GeometryVisualizationPreferences,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network::{
    IncomingWire, NodeDisplayType, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData;
use rust_lib_flutter_cad::structure_designer::nodes::array_len::ArrayLenData;
use rust_lib_flutter_cad::structure_designer::nodes::closure::ClosureKind;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::NodeOutput;

// ============================================================================
// Helpers (mirrors of the closures_test.rs set, trimmed to what's needed here)
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

/// Evaluate a node's pin 0.
fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    evaluate_node_pin(designer, network_name, node_id, 0)
}

/// Evaluate an arbitrary output pin of a node (used to exercise `-1` directly).
fn evaluate_node_pin(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    pin: i32,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, pin, registry, false, &mut context)
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

fn add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    let id = designer.add_node("int", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

/// Add a `range(start, step, count)` node and return its id.
fn add_range(
    designer: &mut StructureDesigner,
    network: &str,
    start: i32,
    step: i32,
    count: i32,
    y: f64,
) -> u64 {
    use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
    let id = designer.add_node("range", DVec2::new(0.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(RangeData { start, step, count }),
    );
    id
}

/// Add a top-level `expr` node with the given free parameters and return its id.
/// The expr's function type is `(param types) -> (parsed output type)`.
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

/// Wire `source_node`'s function pin (`-1`) into `dest_node`'s argument at
/// `dest_arg_index`. Constructed straight onto the argument's wire list to
/// bypass connection gating (which is Phase 2).
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

/// Add a `map` node configured for `input_type -> output_type` and return its id.
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

/// Mirror of `closures_test::configure_parameter`: set a parameter node's name
/// and type so its cached `custom_node_type` (and output pin type) refresh.
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

// ============================================================================
// Tests
// ============================================================================

/// `range(3) → map(f: <expr "x+1">.fn) → drain` yields `[1, 2, 3]`. The headline
/// case: a one-input `expr`'s function pin drives `map` via the `f` pin, with no
/// `closure` node in sight.
#[test]
fn map_function_pin_expr_increment() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", expr_id, map_id, 1); // f ← expr.fn

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![1, 2, 3]);
}

/// A single-input *built-in* (not `expr`) through the function pin. `array_len`
/// is `(Array[Int]) -> Int`; mapping its function pin over `[[1,2,3], [4,5]]`
/// (an `Array[Array[Int]]` literal, broadcast to `Iter[Array[Int]]`) yields the
/// per-element lengths `[3, 2]`. Exercises the built-in eval path of a
/// function-pin body that is not an `expr`.
#[test]
fn map_function_pin_single_input_builtin_array_len() {
    let mut designer = setup_designer_with_network("main");

    // An expr producing the source `Array[Array[Int]]`. (No free params.)
    let arrays_id = add_expr(&mut designer, "main", "[[1, 2, 3], [4, 5]]", vec![], 0.0);

    let len_id = designer.add_node("array_len", DVec2::new(150.0, -120.0));
    set_node_data(
        &mut designer,
        "main",
        len_id,
        Box::new(ArrayLenData {
            element_type: DataType::Int,
        }),
    );

    // map: Array[Int] -> Int, source Array[Array[Int]] broadcasts to the iter.
    let map_id = add_map(
        &mut designer,
        "main",
        DataType::Array(Box::new(DataType::Int)),
        DataType::Int,
        0.0,
    );
    designer.connect_nodes(arrays_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", len_id, map_id, 1); // f ← array_len.fn

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![3, 2]);
}

/// `apply(f: <expr "x*2">.fn, 10)` yields `20` — a single-shot call, no iterator.
#[test]
fn apply_function_pin_expr_double() {
    let mut designer = setup_designer_with_network("main");

    let expr_id = add_expr(&mut designer, "main", "x * 2", vec![("x", DataType::Int)], -120.0);
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
    // Phase D: apply renders only the `f` pin until `f` is wired (the post-
    // pass derives arg pins from the wired source's flat function type).
    // Go through `connect_nodes` for `f` so validation runs and installs the
    // arg pins; otherwise `connect_nodes(arg_id, 0, apply_id, 1)` would
    // early-return on "param index out of bounds".
    designer.connect_nodes(expr_id, -1, apply_id, 0); // f ← expr.fn
    designer.connect_nodes(arg_id, 0, apply_id, 1); // element

    let result = evaluate_node(&designer, "main", apply_id);
    assert_eq!(extract_int(result), 20);
}

/// `fold(f: <expr "a + b">.fn, init = 0)` over `range(1..5)` sums to 10.
///
/// Param order is the body node's input-pin order: input 0 → `acc`, input 1 →
/// `element` (the consumer pushes `[acc, element]`). The expr's params are
/// `(a, b)`, so `a = acc` and `b = element`; `a + b == acc + element`.
#[test]
fn fold_function_pin_expr_sum() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 1, 1, 4, 0.0); // [1,2,3,4]
    let init_id = add_int(&mut designer, "main", 0, 80.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "a + b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
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
    wire_function_pin(&mut designer, "main", expr_id, fold_id, 2); // f

    let result = evaluate_node(&designer, "main", fold_id);
    assert_eq!(extract_int(result), 10);
}

/// A one-parameter custom subnetwork's function pin into `map.f`, evaluated via
/// the recursive custom-node path. The subnetwork `sphere_maker(r: Int) ->
/// Blueprint` wraps a `sphere` whose radius is the parameter; mapping its
/// function pin over `range(1..4)` produces three Blueprints with no error.
#[test]
fn map_function_pin_custom_subnetwork_yields_blueprints() {
    let mut designer = setup_designer_with_network("sphere_maker");

    // sphere_maker(r: Int) -> Blueprint
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    configure_parameter(&mut designer, "sphere_maker", param_id, "r", DataType::Int);

    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(param_id, 0, sphere_id, 1); // r → sphere.radius (pin 1)
    designer.set_return_node_id(Some(sphere_id));
    designer.validate_active_network();

    // Parent network "main": map(f: sphere_maker.fn) over range of radii.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let range_id = add_range(&mut designer, "main", 1, 1, 3, 0.0); // radii [1,2,3]
    let maker_id = designer.add_node("sphere_maker", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Blueprint, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", maker_id, map_id, 1); // f ← sphere_maker.fn

    let result = evaluate_node(&designer, "main", map_id);
    let elements = drain_iter_with_designer(&designer, result);
    assert_eq!(elements.len(), 3, "expected three Blueprints");
    for el in &elements {
        match el {
            NetworkResult::Blueprint(_) => {}
            NetworkResult::Error(msg) => panic!("element evaluated to Error: {msg}"),
            other => panic!("expected Blueprint, got {}", other.to_display_string()),
        }
    }
}

/// Clone-independence: two drains of the *same* `map(f: …)` walker advance
/// independently (the embedded `ZoneClosure` is `Arc`-shared, so cloning the
/// walker doesn't alias its stream). Both produce the full `[1, 2, 3]`.
#[test]
fn map_function_pin_independent_walkers() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_function_pin(&mut designer, "main", expr_id, map_id, 1);

    let walker = match evaluate_node(&designer, "main", map_id) {
        NetworkResult::Iterator(w) => w,
        other => panic!("expected iterator, got {}", other.to_display_string()),
    };
    let first = extract_ints(drain_iter_with_designer(
        &designer,
        NetworkResult::Iterator(walker.clone()),
    ));
    let second = extract_ints(drain_iter_with_designer(
        &designer,
        NetworkResult::Iterator(walker.clone()),
    ));
    assert_eq!(first, vec![1, 2, 3]);
    assert_eq!(second, vec![1, 2, 3], "the clone must drain independently");
}

/// A zero-input node's function pin is an error at synthesis: a `() -> T`
/// function matches no consumer. Evaluating an `int` node's `-1` pin yields
/// `Error`.
#[test]
fn function_pin_zero_input_node_errors() {
    let mut designer = setup_designer_with_network("main");

    let int_id = add_int(&mut designer, "main", 42, 0.0);
    let result = evaluate_node_pin(&designer, "main", int_id, -1);
    match result {
        NetworkResult::Error(_) => {}
        other => panic!(
            "expected Error for a zero-input node's function pin, got {}",
            other.to_display_string()
        ),
    }
}

/// A polymorphic-output node's function pin is rejected at synthesis: its
/// return type is unresolved (`SameAsInput` reads as `DataType::None`).
/// `free_move`'s output is `single_same_as("input")`, so its `-1` pin yields
/// `Error`.
#[test]
fn function_pin_polymorphic_output_node_errors() {
    let mut designer = setup_designer_with_network("main");

    let mv_id = designer.add_node("free_move", DVec2::new(0.0, 0.0));
    let result = evaluate_node_pin(&designer, "main", mv_id, -1);
    match result {
        NetworkResult::Error(_) => {}
        other => panic!(
            "expected Error for a polymorphic-output node's function pin, got {}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Phase 2 — connection gating, validation, display suppression
// (design_function_pins.md §"Validation & connection gating" / §"Display in
//  function mode")
// ============================================================================

/// Build a `fold(Int, Int)` node (`f` at arg index 2, typed `(Int,Int) -> Int`)
/// and return its id.
fn add_fold_int(designer: &mut StructureDesigner, network: &str, y: f64) -> u64 {
    let id = designer.add_node("fold", DVec2::new(350.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    id
}

/// Validate the active network and return `(valid, error_texts)` for the named
/// top-level network. The function-mode rule attributes its error to the
/// offending top-level node, so its text lands on `network.validation_errors`.
fn validate_and_errors(designer: &mut StructureDesigner, network: &str) -> (bool, Vec<String>) {
    designer.validate_active_network();
    read_validity(designer, network)
}

/// Read the *stored* validity + error texts of a network **without** triggering
/// a fresh validation pass. Used to assert that a mutation method re-validated
/// on its own (rather than leaving a stale error behind).
fn read_validity(designer: &StructureDesigner, network: &str) -> (bool, Vec<String>) {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let errors = net
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    (net.valid, errors)
}

/// Clear every incoming wire on `node_id`'s argument at `arg_index`. Used to
/// disconnect a wire directly (no disconnect API needed in tests).
fn clear_argument(designer: &mut StructureDesigner, network: &str, node_id: u64, arg_index: usize) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    net.nodes
        .get_mut(&node_id)
        .unwrap()
        .arguments[arg_index]
        .clear();
}

/// Remove `node_id` from `network` (drops it and any wires stored on its
/// arguments, including a function-pin wire it consumed).
fn remove_node(designer: &mut StructureDesigner, network: &str, node_id: u64) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    net.nodes.remove(&node_id);
}

/// Run `generate_scene` for `node_id` in `network` and return its pin-0
/// `NodeOutput`. Uses `SurfaceSplatting` so a Blueprint result reliably yields a
/// (non-`None`) `SurfacePointCloud` regardless of sampling density.
fn scene_output(designer: &StructureDesigner, network: &str, node_id: u64) -> NodeOutput {
    let registry = &designer.node_type_registry;
    let mut evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let prefs = GeometryVisualizationPreferences {
        geometry_visualization: GeometryVisualization::SurfaceSplatting,
        ..Default::default()
    };
    evaluator
        .generate_scene(
            network,
            node_id,
            NodeDisplayType::Normal,
            registry,
            &prefs,
            &mut context,
        )
        .output
}

/// `can_connect_nodes` accepts/rejects function-pin sources by structural type
/// match: `(Int)->Int` fits `map.f` but not (by arity) a 2-ary `fold.f`, and
/// `(Int,Int)->Int` fits `fold.f` but not `map.f`.
#[test]
fn can_connect_function_pin_type_match() {
    let mut designer = setup_designer_with_network("main");

    let expr1 = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -200.0);
    let expr2 = add_expr(
        &mut designer,
        "main",
        "a + b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
        -80.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    let fold_id = add_fold_int(&mut designer, "main", 200.0);

    // (Int) -> Int  ✓ map.f (arg 1)   ✗ fold.f (arg 2, arity 2)
    assert!(designer.can_connect_nodes(expr1, -1, map_id, 1));
    assert!(!designer.can_connect_nodes(expr1, -1, fold_id, 2));

    // (Int,Int) -> Int  ✓ map.f (auto-partial via Phase 4 starts-with rule;
    // the excess Int param becomes a partial-application tail and map
    // retypes to `Iter[Function((Int,), Int)]`)   ✓ fold.f (exact arity)
    assert!(designer.can_connect_nodes(expr2, -1, map_id, 1));
    assert!(designer.can_connect_nodes(expr2, -1, fold_id, 2));
}

/// Function-mode mutual exclusion is enforced in both directions at connect
/// time: a function-pin source with a wired input is rejected, and an input
/// pin on a node whose function pin is already consumed is rejected.
#[test]
fn can_connect_function_mode_mutual_exclusion() {
    let mut designer = setup_designer_with_network("main");

    let expr1 = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    let arg_int = add_int(&mut designer, "main", 5, 120.0);

    // Source side: with no wired input the function pin connects; once an input
    // pin is wired, dragging the function pin is rejected (it would be a dead
    // wire — every input is a parameter).
    assert!(designer.can_connect_nodes(expr1, -1, map_id, 1));
    designer.connect_nodes(arg_int, 0, expr1, 0); // wire x
    assert!(
        !designer.can_connect_nodes(expr1, -1, map_id, 1),
        "function pin must be unwireable while an input is connected"
    );

    // Destination side: a fresh node accepts an input wire; once its function
    // pin is consumed, the input pin no longer accepts a wire.
    let expr2 = add_expr(&mut designer, "main", "y + 1", vec![("y", DataType::Int)], 240.0);
    assert!(designer.can_connect_nodes(arg_int, 0, expr2, 0));
    wire_function_pin(&mut designer, "main", expr2, map_id, 1); // expr2.fn → map.f
    assert!(
        !designer.can_connect_nodes(arg_int, 0, expr2, 0),
        "an input pin must be unwireable while the function pin is consumed"
    );
}

/// A stored function-pin wire that bypasses the drag gate (`.cnnd` load /
/// text edit) is caught by validation when the function-pinned node also has a
/// wired input; disconnecting the input clears the error.
#[test]
fn validation_function_mode_both_wired() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
    let arg_int = add_int(&mut designer, "main", 7, 120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", expr_id, map_id, 1); // expr.fn → map.f
    designer.connect_nodes(arg_int, 0, expr_id, 0); // dead wire: expr.x

    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(!valid, "function pin + wired input must be invalid");
    assert!(
        errors.iter().any(|e| e.contains("used as a function value")),
        "expected the function-mode error, got {errors:?}"
    );

    // Disconnecting the dead input wire clears the violation.
    clear_argument(&mut designer, "main", expr_id, 0);
    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        valid,
        "clearing the input wire should restore validity, got {errors:?}"
    );
    assert!(!errors.iter().any(|e| e.contains("used as a function value")));
}

/// A matched function-pin wire validates clean (and an HOF driven by it with an
/// empty inline body is fine — rule-1 suspension); an arity/type-mismatched one
/// surfaces a wire type error via `get_function_type()` resolution.
#[test]
fn validation_function_pin_type_match_and_mismatch() {
    // Matched: (Int)->Int into map.f, empty body — valid.
    {
        let mut designer = setup_designer_with_network("main");
        let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
        let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
        let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
        designer.connect_nodes(range_id, 0, map_id, 0);
        wire_function_pin(&mut designer, "main", expr_id, map_id, 1);

        let (valid, errors) = validate_and_errors(&mut designer, "main");
        assert!(valid, "matched function pin should validate clean: {errors:?}");
    }

    // Mismatched: (Bool,Int)->Int into map.f (expects starts-with [Int]) —
    // invalid. The first param doesn't match the element_type, so neither the
    // standard structural check nor the Phase 4 starts-with rule admits the
    // wire. (A `(Int,Int)->Int` source would *now* be accepted as auto-partial
    // — that is the headline Phase 4 change.)
    {
        let mut designer = setup_designer_with_network("main");
        let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
        let expr_id = add_expr(
            &mut designer,
            "main",
            "if a then b else 0",
            vec![("a", DataType::Bool), ("b", DataType::Int)],
            -120.0,
        );
        let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
        designer.connect_nodes(range_id, 0, map_id, 0);
        wire_function_pin(&mut designer, "main", expr_id, map_id, 1);

        let (valid, errors) = validate_and_errors(&mut designer, "main");
        assert!(
            !valid,
            "element-type-mismatched function pin must be invalid"
        );
        assert!(
            errors.iter().any(|e| e.contains("mismatch")),
            "expected a wire type-mismatch error, got {errors:?}"
        );
    }
}

/// `apply`'s required `f` pin is still enforced after the function-pin work:
/// an `apply` with a disconnected `f` is invalid (closures rule 4 untouched).
#[test]
fn validation_apply_requires_f_unchanged() {
    let mut designer = setup_designer_with_network("main");
    let apply_id = designer.add_node("apply", DVec2::new(0.0, 0.0));
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

    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(!valid, "apply with disconnected f must be invalid");
    assert!(
        errors.iter().any(|e| e.contains("apply")),
        "expected the apply-requires-f error, got {errors:?}"
    );
}

/// A node whose function pin is consumed emits no scene output even though it
/// has a displayable (Blueprint) pin 0 — the scene-skip overrides the display
/// path. Removing the consuming wire restores its output.
#[test]
fn scene_skip_function_mode_node() {
    // const_sphere(unused: Int) -> Blueprint: an arity-1 custom network whose
    // body ignores its parameter, so it both fits `map.f` and renders standalone.
    let mut designer = setup_designer_with_network("const_sphere");
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    configure_parameter(&mut designer, "const_sphere", param_id, "unused", DataType::Int);
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.set_return_node_id(Some(sphere_id));
    designer.validate_active_network();

    // main: range → map.xs; const_sphere.fn → map.f.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let range_id = add_range(&mut designer, "main", 1, 1, 3, 0.0);
    let maker_id = designer.add_node("const_sphere", DVec2::new(150.0, -120.0));
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Blueprint, 0.0);
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_function_pin(&mut designer, "main", maker_id, map_id, 1);
    designer.validate_active_network();

    // Display the maker's pin 0 explicitly, then confirm it still renders
    // nothing because its function pin is consumed (skip overrides display).
    designer.set_node_display(maker_id, true);
    let out = scene_output(&designer, "main", maker_id);
    assert!(
        matches!(out, NodeOutput::None),
        "a function-mode node must emit no scene output"
    );

    // Remove the consumer; the maker is no longer in function mode and renders
    // its Blueprint again.
    remove_node(&mut designer, "main", map_id);
    designer.validate_active_network();
    let out = scene_output(&designer, "main", maker_id);
    assert!(
        !matches!(out, NodeOutput::None),
        "removing the consumer must restore the node's scene output"
    );
}

/// Regression: connecting a function-pin source into a *top-level* HOF's `f`
/// pin must re-validate so the "zone-output pin has no incoming wire" error the
/// HOF carried while its inline body was empty is cleared.
///
/// `StructureDesigner::connect_nodes` (the top-level entry point the UI's
/// `connect_nodes`/`connect_nodes_scoped(empty path)` resolves to) marks a
/// `Partial` refresh, which re-evaluates but does **not** re-validate. The
/// scoped/body connect path calls `validate_active_network()` explicitly for
/// exactly this reason; the top-level path was missing it for function wires, so
/// a `map` driven by a freshly-wired `f` pin kept showing a stale zone-output
/// error in the editor (while its body correctly collapsed, since collapse *is*
/// recomputed every view build). This reads the *stored* validity (no explicit
/// re-validate) to prove `connect_nodes` cleared it on its own.
#[test]
fn connecting_f_pin_revalidates_and_clears_stale_zone_output_error() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs

    // Before `f`: the empty inline body trips the zone-output rule.
    designer.validate_active_network();
    let (valid_before, errors_before) = read_validity(&designer, "main");
    assert!(
        !valid_before,
        "an empty-body map should be invalid before `f` is wired; got {errors_before:?}"
    );
    assert!(
        errors_before.iter().any(|e| {
            let l = e.to_lowercase();
            l.contains("zone-output") && l.contains("no incoming wire")
        }),
        "expected the zone-output rule error before `f`; got {errors_before:?}"
    );

    // Connect the function pin through the same entry point the UI uses. No
    // explicit `validate_active_network()` afterward — the method must do it.
    designer.connect_nodes(expr_id, -1, map_id, 1); // f ← expr.fn

    let (valid_after, errors_after) = read_validity(&designer, "main");
    assert!(
        valid_after,
        "connecting `f` must re-validate and clear the stale zone-output error; got {errors_after:?}"
    );
}

/// Mirror of the connect case: *disconnecting* the `f` wire from a top-level HOF
/// with an empty inline body must re-validate so the zone-output rule fires
/// again. The wire-deletion branch of `delete_selected` skips validation for
/// ordinary value wires, but a function wire (here, a `-1` source feeding `f`)
/// requests a re-validate so the network doesn't stay wrongly marked valid.
#[test]
fn disconnecting_f_pin_revalidates_and_restores_zone_output_error() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(&mut designer, "main", "x + 1", vec![("x", DataType::Int)], -120.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    designer.connect_nodes(expr_id, -1, map_id, 1); // f ← expr.fn (now valid)

    let (valid_wired, errors_wired) = read_validity(&designer, "main");
    assert!(
        valid_wired,
        "map with `f` wired and an empty body should be valid; got {errors_wired:?}"
    );

    // Select and delete the `f` wire through the same entry point the UI uses.
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        assert!(
            net.select_wire(expr_id, -1, map_id, 1),
            "failed to select the f wire"
        );
    }
    designer.delete_selected();

    let (valid_after, errors_after) = read_validity(&designer, "main");
    assert!(
        !valid_after,
        "disconnecting `f` must re-validate and restore the zone-output error"
    );
    assert!(
        errors_after.iter().any(|e| {
            let l = e.to_lowercase();
            l.contains("zone-output") && l.contains("no incoming wire")
        }),
        "expected the zone-output rule error after disconnecting `f`; got {errors_after:?}"
    );
}
