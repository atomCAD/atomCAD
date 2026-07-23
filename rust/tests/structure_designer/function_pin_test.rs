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
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
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
        is_zone_body: false,
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
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + 1",
        vec![("x", DataType::Int)],
        -120.0,
    );
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

    let expr_id = add_expr(
        &mut designer,
        "main",
        "x * 2",
        vec![("x", DataType::Int)],
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
    let map_id = add_map(
        &mut designer,
        "main",
        DataType::Int,
        DataType::Blueprint,
        0.0,
    );

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
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + 1",
        vec![("x", DataType::Int)],
        -120.0,
    );
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

/// A zero-input node's function pin is a legal `() -> R` thunk (the degenerate
/// all-captured case, with zero captures). Re-grounded for
/// `doc/design_node_function_pin_captures.md`: the old `param_types.is_empty()`
/// rejection is gone. Evaluating an `int(42)` node's `-1` pin yields a
/// `Function` value; forcing it via `apply` (with no args) returns `42`.
#[test]
fn function_pin_zero_input_node_is_thunk() {
    let mut designer = setup_designer_with_network("main");

    let int_id = add_int(&mut designer, "main", 42, 0.0);
    let result = evaluate_node_pin(&designer, "main", int_id, -1);
    let zc = match result {
        NetworkResult::Function(zc) => zc,
        other => panic!(
            "expected a () -> Int thunk for a zero-input node's function pin, got {}",
            other.to_display_string()
        ),
    };
    assert!(
        zc.param_types.is_empty(),
        "a zero-input node's function pin is a nullary thunk"
    );
    assert_eq!(zc.return_type, DataType::Int);
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

    let expr1 = add_expr(
        &mut designer,
        "main",
        "x + 1",
        vec![("x", DataType::Int)],
        -200.0,
    );
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

/// Re-grounded for `doc/design_node_function_pin_captures.md`: the function-mode
/// mutual-exclusion gates are gone. A function-pin source with a wired input is
/// now legal (the wired input is a *capture*), and an input pin on a node whose
/// function pin is already consumed is now legal (it adds a capture). The
/// connection is gated only by the wiring-aware type match.
#[test]
fn can_connect_function_pin_with_captures_is_allowed() {
    let mut designer = setup_designer_with_network("main");

    // expr2 keeps a free Int param even after one input is captured, so its
    // wiring-aware `-1` type stays `(Int) -> Int` and fits `map.f`.
    let expr1 = add_expr(
        &mut designer,
        "main",
        "x + c",
        vec![("x", DataType::Int), ("c", DataType::Int)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    let arg_int = add_int(&mut designer, "main", 5, 120.0);

    // Source side: the function pin connects with no captures; wiring `c` (a
    // capture) leaves `(Int) -> Int` (x is still a parameter), still wireable.
    assert!(designer.can_connect_nodes(expr1, -1, map_id, 1));
    designer.connect_nodes(arg_int, 0, expr1, 1); // capture c
    assert!(
        designer.can_connect_nodes(expr1, -1, map_id, 1),
        "function pin with one capture and a remaining (Int) param must still fit map.f"
    );

    // Destination side: an input pin still accepts a wire after the function
    // pin is consumed (it becomes a capture).
    let expr2 = add_expr(
        &mut designer,
        "main",
        "y + k",
        vec![("y", DataType::Int), ("k", DataType::Int)],
        240.0,
    );
    assert!(designer.can_connect_nodes(arg_int, 0, expr2, 1));
    wire_function_pin(&mut designer, "main", expr2, map_id, 1); // expr2.fn → map.f
    assert!(
        designer.can_connect_nodes(arg_int, 0, expr2, 1),
        "an input pin must remain wireable (as a capture) while the function pin is consumed"
    );
}

/// Re-grounded for `doc/design_node_function_pin_captures.md`: a wired input on
/// a function-consumed node is a legal *capture*, not a dead wire. With a
/// two-param `x + c` expr feeding `map.f`, capturing `c` leaves a `(Int) -> Int`
/// function that validates clean. Capturing *both* inputs makes a `() -> Int`
/// thunk that no longer fits `map.f` — but that surfaces as an ordinary
/// `AnyFunction` type mismatch, not the old function-mode message.
#[test]
fn validation_function_pin_captures_and_thunk_mismatch() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + c",
        vec![("x", DataType::Int), ("c", DataType::Int)],
        -120.0,
    );
    let cap_int = add_int(&mut designer, "main", 7, 120.0);
    let x_int = add_int(&mut designer, "main", 9, 200.0);
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    wire_function_pin(&mut designer, "main", expr_id, map_id, 1); // expr.fn → map.f
    designer.connect_nodes(cap_int, 0, expr_id, 1); // capture c → (Int) -> Int

    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        valid,
        "one capture leaves a (Int) -> Int that fits map.f; got {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("used as a function value"))
    );

    // Capture the second input too → `() -> Int` thunk, which doesn't fit
    // `map.f` (needs a leading Int param). Surfaces as a type mismatch.
    designer.connect_nodes(x_int, 0, expr_id, 0); // capture x → () -> Int
    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(!valid, "an all-captured thunk does not fit map.f");
    assert!(
        errors.iter().any(|e| e.contains("mismatch")),
        "expected a wire type-mismatch error, got {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("used as a function value")),
        "the old function-mode error must be gone, got {errors:?}"
    );
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
        let expr_id = add_expr(
            &mut designer,
            "main",
            "x + 1",
            vec![("x", DataType::Int)],
            -120.0,
        );
        let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
        designer.connect_nodes(range_id, 0, map_id, 0);
        wire_function_pin(&mut designer, "main", expr_id, map_id, 1);

        let (valid, errors) = validate_and_errors(&mut designer, "main");
        assert!(
            valid,
            "matched function pin should validate clean: {errors:?}"
        );
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
/// an `apply` with a disconnected `f` still surfaces the rule-4 error. The
/// error is now *non-blocking* (the runtime localizes it into a clean
/// `NetworkResult::Error`), so the network stays valid; the discriminator is
/// the error text, not the `valid` flag.
#[test]
fn validation_apply_requires_f_still_reported() {
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
    assert!(
        valid,
        "apply with disconnected f is non-blocking, so the network stays valid; got {errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("apply")),
        "expected the apply-requires-f error (badge), got {errors:?}"
    );
}

/// A node whose function pin is consumed still emits scene output when its
/// pin-0 eye is on: function mode no longer suppresses display
/// (`doc/design_function_pin_roles.md` §"Display relaxation"). Removing the
/// consuming wire changes nothing — it was already rendering.
#[test]
fn scene_function_mode_node_displays_when_shown() {
    // const_sphere(unused: Int) -> Blueprint: an arity-1 custom network whose
    // body ignores its parameter, so it both fits `map.f` and renders standalone.
    let mut designer = setup_designer_with_network("const_sphere");
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    configure_parameter(
        &mut designer,
        "const_sphere",
        param_id,
        "unused",
        DataType::Int,
    );
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.set_return_node_id(Some(sphere_id));
    designer.validate_active_network();

    // main: range → map.xs; const_sphere.fn → map.f.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let range_id = add_range(&mut designer, "main", 1, 1, 3, 0.0);
    let maker_id = designer.add_node("const_sphere", DVec2::new(150.0, -120.0));
    let map_id = add_map(
        &mut designer,
        "main",
        DataType::Int,
        DataType::Blueprint,
        0.0,
    );
    designer.connect_nodes(range_id, 0, map_id, 0);
    wire_function_pin(&mut designer, "main", maker_id, map_id, 1);
    designer.validate_active_network();

    // Display the maker's pin 0 explicitly: it renders its Blueprint even
    // though its function pin is consumed.
    designer.set_node_display(maker_id, true);
    let out = scene_output(&designer, "main", maker_id);
    assert!(
        !matches!(out, NodeOutput::None),
        "a displayed function-mode node must emit its pin-0 scene output"
    );

    // Removing the consumer leaves it rendering — display never depended on
    // function mode.
    remove_node(&mut designer, "main", map_id);
    designer.validate_active_network();
    let out = scene_output(&designer, "main", maker_id);
    assert!(
        !matches!(out, NodeOutput::None),
        "the node keeps rendering once the consumer is gone"
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
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + 1",
        vec![("x", DataType::Int)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs

    // Before `f`: the empty inline body trips the zone-output rule. The error
    // is non-blocking (the network stays valid), so the discriminating signal
    // is the *error text*, not the validity flag.
    designer.validate_active_network();
    let (valid_before, errors_before) = read_validity(&designer, "main");
    assert!(
        valid_before,
        "the zone-output error is non-blocking, so the network stays valid \
         before `f` is wired; got {errors_before:?}"
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
        "map with `f` wired and an empty body must be valid; got {errors_after:?}"
    );
    // The discriminating check that `connect_nodes` actually re-validated: the
    // zone-output error must be gone now that the wired `f` suspends the rule.
    // Before the fix the stale error lingered in the editor.
    assert!(
        !errors_after.iter().any(|e| {
            let l = e.to_lowercase();
            l.contains("zone-output") && l.contains("no incoming wire")
        }),
        "connecting `f` must clear the stale zone-output error; got {errors_after:?}"
    );
}

/// Mirror of the connect case: *disconnecting* the `f` wire from a top-level HOF
/// with an empty inline body must re-validate so the zone-output rule fires
/// again. The wire-deletion branch of `delete_selected` skips validation for
/// ordinary value wires, but a function wire (here, a `-1` source feeding `f`)
/// requests a re-validate so the zone-output error (badge) is restored on the
/// HOF — the error is non-blocking, so the discriminator is its text, not the
/// `valid` flag.
#[test]
fn disconnecting_f_pin_revalidates_and_restores_zone_output_error() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + 1",
        vec![("x", DataType::Int)],
        -120.0,
    );
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

    // Disconnecting `f` must re-validate and restore the (non-blocking)
    // zone-output error. Validity stays true — the discriminating signal is the
    // restored error text.
    let (valid_after, errors_after) = read_validity(&designer, "main");
    assert!(
        valid_after,
        "the restored zone-output error is non-blocking, so validity stays true; \
         got {errors_after:?}"
    );
    assert!(
        errors_after.iter().any(|e| {
            let l = e.to_lowercase();
            l.contains("zone-output") && l.contains("no incoming wire")
        }),
        "expected the zone-output rule error restored after disconnecting `f`; got {errors_after:?}"
    );
}

// ============================================================================
// Captures on the function pin (doc/design_node_function_pin_captures.md, Phase 1)
// ============================================================================

/// Resolve `map_id`'s pin-0 output type in `network` (the type the
/// `update_map_pin_layouts_for_network` post-pass installs from the wired `f`
/// source). Used to assert the wire-state propagation flips.
fn map_output_type(designer: &StructureDesigner, network: &str, map_id: u64) -> DataType {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let node = net.nodes.get(&map_id).unwrap();
    designer
        .node_type_registry
        .resolve_output_type(node, net, 0)
        .expect("map pin-0 output type should resolve")
}

/// The headline: a two-param `x + c` expr with `c` **wired** (a capture) and `x`
/// **unwired** (a parameter). Its `-1` pin resolves to `(Int) -> Int` with `c`
/// frozen; mapping it over `range(0,1,3) = [0,1,2]` with `c = 10` yields
/// `[10, 11, 12]`.
#[test]
fn map_function_pin_with_capture_freezes_value() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0); // [0,1,2]
    let cap_id = add_int(&mut designer, "main", 10, 120.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + c",
        vec![("x", DataType::Int), ("c", DataType::Int)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    designer.connect_nodes(cap_id, 0, expr_id, 1); // capture c = 10
    wire_function_pin(&mut designer, "main", expr_id, map_id, 1); // f ← expr.fn

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(elements, vec![10, 11, 12]);
}

/// Capture-freeze timing under an outer `fold`: a `-1`-sourced function whose
/// capture reads the *outer* iteration value re-freezes per outer step. The
/// inner `map` body uses `add_outer(x) = x + acc` where `acc` is the outer
/// fold's running accumulator (a capture reaching past the inner map into the
/// fold). Summing the inner results across `fold` therefore mirrors an inline
/// body — proving the capture isn't frozen once-and-stale.
///
/// This is exercised more directly elsewhere (zones_test capture-timing cases);
/// here we keep a lean check that a capture sourced from a sibling constant
/// reflects that constant's value at `-1`-eval, not some earlier default.
#[test]
fn function_pin_capture_reflects_source_value() {
    let mut designer = setup_designer_with_network("main");

    // c is produced by an expr (so it's a real evaluated source, not a literal
    // baked into the body): c = 2 * 5 = 10.
    let c_id = add_expr(&mut designer, "main", "2 * 5", vec![], 120.0);
    let range_id = add_range(&mut designer, "main", 1, 1, 3, 0.0); // [1,2,3]
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x * c",
        vec![("x", DataType::Int), ("c", DataType::Int)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    designer.connect_nodes(c_id, 0, expr_id, 1); // capture c = 10
    wire_function_pin(&mut designer, "main", expr_id, map_id, 1); // f ← expr.fn

    let result = evaluate_node(&designer, "main", map_id);
    let elements = extract_ints(drain_iter_with_designer(&designer, result));
    assert_eq!(
        elements,
        vec![10, 20, 30],
        "capture must equal the evaluated source value"
    );
}

/// All-wired source → `() -> R` thunk, forced via `apply` (no args) returns `R`.
/// `a * b` with both `a` and `b` captured (6 and 7) is a `() -> Int` thunk;
/// `apply(f)` runs the body and returns `42`.
#[test]
fn apply_function_pin_all_captured_thunk_returns_value() {
    let mut designer = setup_designer_with_network("main");

    let a_id = add_int(&mut designer, "main", 6, -80.0);
    let b_id = add_int(&mut designer, "main", 7, 80.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "a * b",
        vec![("a", DataType::Int), ("b", DataType::Int)],
        -200.0,
    );
    designer.connect_nodes(a_id, 0, expr_id, 0); // capture a
    designer.connect_nodes(b_id, 0, expr_id, 1); // capture b

    let apply_id = designer.add_node("apply", DVec2::new(350.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int],
            param_names: vec![],
        }),
    );
    designer.connect_nodes(expr_id, -1, apply_id, 0); // f ← expr.fn (() -> Int)

    let result = evaluate_node(&designer, "main", apply_id);
    assert_eq!(extract_int(result), 42);
}

/// Wire-state propagation (guards the new revalidation triggers). With
/// `expr.-1 → map.f` and a two-param `x + c` expr, wiring a capture on `c` flips
/// `map`'s resolved output `Iter[Function((Int,),Int)]` → `Iter[Int]` with **no**
/// edit at `map`; deleting that wire flips it back. Without the
/// `function_pin_consumed`-keyed triggers in `connect_nodes` / `delete_selected`,
/// the partial refresh would leave `map`'s derived type stale.
#[test]
fn function_pin_capture_propagates_to_consumer_type() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let cap_id = add_int(&mut designer, "main", 10, 120.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + c",
        vec![("x", DataType::Int), ("c", DataType::Int)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    designer.connect_nodes(expr_id, -1, map_id, 1); // f ← expr.fn ((Int,Int)->Int)

    let iter_int = DataType::Iterator(Box::new(DataType::Int));

    // Both expr inputs unwired → `(Int,Int)->Int` → map auto-partializes to a
    // stream of partials (not `Iter[Int]`).
    let before = map_output_type(&designer, "main", map_id);
    assert_ne!(
        before, iter_int,
        "with no capture map should produce a partial stream, not Iter[Int]; got {before:?}"
    );

    // Wire a capture on `c` → expr.-1 becomes `(Int)->Int` → map output flips to
    // `Iter[Int]`, with no edit at map. (Tests the connect-time trigger.)
    designer.connect_nodes(cap_id, 0, expr_id, 1);
    let after_capture = map_output_type(&designer, "main", map_id);
    assert_eq!(
        after_capture, iter_int,
        "wiring a capture must re-derive map's output to Iter[Int]; got {after_capture:?}"
    );

    // Delete the capture wire → expr.-1 reverts to `(Int,Int)->Int` → map flips
    // back. (Tests the delete-time trigger.)
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        assert!(
            net.select_wire(cap_id, 0, expr_id, 1),
            "failed to select the capture wire"
        );
    }
    designer.delete_selected();
    let after_delete = map_output_type(&designer, "main", map_id);
    assert_ne!(
        after_delete, iter_int,
        "deleting the capture must revert map's output off Iter[Int]; got {after_delete:?}"
    );
    // And it is specifically the partial-stream shape again.
    assert_eq!(
        after_delete,
        DataType::Iterator(Box::new(DataType::Function(FunctionType::new(
            vec![DataType::Int],
            DataType::Int
        )))),
        "deleting the capture should restore the partial-stream output"
    );
}

/// Undo/redo of a capture-wire edit must re-derive the consumer's type too —
/// `ConnectWireCommand` uses a `NodeDataChanged` refresh, which normally skips
/// validation; the `function_pin_consumed`-keyed branch in
/// `apply_undo_refresh_mode` covers it. Wiring a capture flips `map` to
/// `Iter[Int]`; **undo** must flip it back, **redo** must flip it forward again.
#[test]
fn function_pin_capture_propagation_survives_undo_redo() {
    let mut designer = setup_designer_with_network("main");

    let range_id = add_range(&mut designer, "main", 0, 1, 3, 0.0);
    let cap_id = add_int(&mut designer, "main", 10, 120.0);
    let expr_id = add_expr(
        &mut designer,
        "main",
        "x + c",
        vec![("x", DataType::Int), ("c", DataType::Int)],
        -120.0,
    );
    let map_id = add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    designer.connect_nodes(range_id, 0, map_id, 0); // xs
    designer.connect_nodes(expr_id, -1, map_id, 1); // f ← expr.fn

    let iter_int = DataType::Iterator(Box::new(DataType::Int));

    // Wire the capture (pushes a ConnectWireCommand) → map flips to Iter[Int].
    designer.connect_nodes(cap_id, 0, expr_id, 1);
    assert_eq!(map_output_type(&designer, "main", map_id), iter_int);

    // Undo: the capture is removed → map must revert off Iter[Int].
    assert!(designer.undo(), "undo should report success");
    assert_ne!(
        map_output_type(&designer, "main", map_id),
        iter_int,
        "undo of the capture must re-derive map's output off Iter[Int]"
    );

    // Redo: the capture is re-added → map must flip back to Iter[Int].
    assert!(designer.redo(), "redo should report success");
    assert_eq!(
        map_output_type(&designer, "main", map_id),
        iter_int,
        "redo of the capture must re-derive map's output back to Iter[Int]"
    );
}

// ============================================================================
// Function pin roles — Phase 1 backend core
// (doc/design_function_pin_roles.md, issue #408)
// ============================================================================

use glam::i32::IVec3;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api::build_function_pin_role_views;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIFunctionPinDisposition, APIFunctionPinRole,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::{
    NodeDisplayPolicy, NodeDisplayPreferences, StructureDesignerPreferences,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::node_network::{
    FunctionPinDisposition, FunctionPinRole, function_pin_dispositions,
};
use rust_lib_flutter_cad::structure_designer::nodes::array_concat::ArrayConcatData;
use rust_lib_flutter_cad::structure_designer::nodes::cuboid::CuboidData;
use rust_lib_flutter_cad::structure_designer::nodes::structure_move::{
    StructureMoveData, StructureMoveEvalCache,
};

use crate::structure_equivalence::assert_structures_equivalent;

/// Set `pin_index`'s role directly on the node (bypasses the designer's
/// setter/undo path, for tests that only care about the resulting partition).
fn set_role_raw(
    designer: &mut StructureDesigner,
    network: &str,
    node_id: u64,
    pin_index: usize,
    role: FunctionPinRole,
) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    let node = net.nodes.get_mut(&node_id).unwrap();
    node.function_pin_roles.insert(pin_index, role);
}

fn clear_role_raw(designer: &mut StructureDesigner, network: &str, node_id: u64, pin_index: usize) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    net.nodes
        .get_mut(&node_id)
        .unwrap()
        .function_pin_roles
        .remove(&pin_index);
}

/// Clear every incoming wire on one input pin (used to re-point a preview wire).
fn clear_pin_wires(designer: &mut StructureDesigner, network: &str, node_id: u64, pin: usize) {
    designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap()
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .arguments[pin]
        .clear();
}

fn roles_of(
    designer: &StructureDesigner,
    network: &str,
    node_id: u64,
) -> std::collections::BTreeMap<usize, FunctionPinRole> {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .function_pin_roles
        .clone()
}

fn dispositions_of(
    designer: &StructureDesigner,
    network: &str,
    node_id: u64,
) -> Vec<FunctionPinDisposition> {
    let registry = &designer.node_type_registry;
    let net = registry.node_networks.get(network).unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    let node_type = registry.get_node_type_for_node(node).unwrap();
    function_pin_dispositions(node, node_type)
}

/// The `-1` pin's resolved `DataType` for a top-level node.
fn function_pin_type(
    designer: &StructureDesigner,
    network: &str,
    node_id: u64,
) -> Option<DataType> {
    let registry = &designer.node_type_registry;
    let net = registry.node_networks.get(network).unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    registry.resolve_output_type(node, net, -1)
}

/// A `cuboid` node with the given extent (a Blueprint source with real content).
fn add_cuboid(designer: &mut StructureDesigner, network: &str, extent: i32, y: f64) -> u64 {
    let id = designer.add_node("cuboid", DVec2::new(0.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(CuboidData {
            min_corner: IVec3::ZERO,
            extent: IVec3::splat(extent),
            subdivision: 1,
        }),
    );
    id
}

/// `materialize(cuboid(extent))` — a self-contained `Crystal` source. Its output
/// pin is `Fixed(Crystal)`, so it also serves as a static type witness.
fn add_crystal_source(designer: &mut StructureDesigner, network: &str, extent: i32, y: f64) -> u64 {
    let cuboid_id = add_cuboid(designer, network, extent, y);
    let mat_id = designer.add_node("materialize", DVec2::new(150.0, y));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0); // shape
    mat_id
}

/// A `structure_move` node with the given stored translation. Pins: 0 `input`
/// (HasStructure, required), 1 `translation` (IVec3), 2 `subdivision` (Int).
fn add_structure_move(
    designer: &mut StructureDesigner,
    network: &str,
    translation: IVec3,
    y: f64,
) -> u64 {
    let id = designer.add_node("structure_move", DVec2::new(300.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(StructureMoveData {
            translation,
            lattice_subdivision: IVec3::ONE,
        }),
    );
    id
}

/// Mark `structure_move`'s `input` Delayed and its property pins Supplied —
/// the issue's configuration.
fn configure_move_as_delayed_input(designer: &mut StructureDesigner, network: &str, mv_id: u64) {
    set_role_raw(designer, network, mv_id, 0, FunctionPinRole::Delayed);
    set_role_raw(designer, network, mv_id, 1, FunctionPinRole::Supplied);
    set_role_raw(designer, network, mv_id, 2, FunctionPinRole::Supplied);
    set_role_raw(designer, network, mv_id, 3, FunctionPinRole::Supplied);
}

/// An `apply` node whose `f` is wired to `source_id`'s function pin, with
/// `arg_id`'s pin 0 as its single argument. Goes through `connect_nodes` so the
/// arg-pin layout post-pass runs (see `apply_function_pin_expr_double`).
fn add_apply_of_function_pin(
    designer: &mut StructureDesigner,
    network: &str,
    source_id: u64,
    arg_id: u64,
    y: f64,
) -> u64 {
    let apply_id = designer.add_node("apply", DVec2::new(500.0, y));
    set_node_data(
        designer,
        network,
        apply_id,
        Box::new(ApplyData {
            kind: ClosureKind::Map,
            type_args: vec![],
            param_names: vec![],
        }),
    );
    designer.connect_nodes(source_id, -1, apply_id, 0); // f
    designer.connect_nodes(arg_id, 0, apply_id, 1); // arg0
    apply_id
}

fn extract_atoms(result: NetworkResult) -> AtomicStructure {
    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(msg) => panic!("expected atoms, got Error: {msg}"),
        other => panic!("expected atoms, got {}", other.to_display_string()),
    }
}

// --- Partition ---------------------------------------------------------------

/// Every role × wired/unwired combination maps onto the disposition table in
/// `doc/design_function_pin_roles.md` §"Semantics".
#[test]
fn roles_partition_table_covers_every_combination() {
    let mut designer = setup_designer_with_network("main");
    let crystal_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let int_id = add_int(&mut designer, "main", 3, 200.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -120.0);

    // All unwired, all Auto → every pin is a parameter.
    assert_eq!(
        dispositions_of(&designer, "main", mv_id),
        vec![FunctionPinDisposition::Parameter; 4]
    );

    // Auto + wired → capture-wire.
    designer.connect_nodes(crystal_id, 0, mv_id, 0);
    designer.connect_nodes(int_id, 0, mv_id, 2);
    assert_eq!(
        dispositions_of(&designer, "main", mv_id),
        vec![
            FunctionPinDisposition::CaptureWire,
            FunctionPinDisposition::Parameter,
            FunctionPinDisposition::CaptureWire,
            FunctionPinDisposition::Parameter,
        ]
    );

    // Delayed + wired → still a parameter (the wire is preview-only).
    // Supplied + unwired → capture-stored.
    // Supplied + wired → capture-wire (identical to Auto + wired).
    set_role_raw(&mut designer, "main", mv_id, 0, FunctionPinRole::Delayed);
    set_role_raw(&mut designer, "main", mv_id, 1, FunctionPinRole::Supplied);
    set_role_raw(&mut designer, "main", mv_id, 2, FunctionPinRole::Supplied);
    set_role_raw(&mut designer, "main", mv_id, 3, FunctionPinRole::Supplied);
    assert_eq!(
        dispositions_of(&designer, "main", mv_id),
        vec![
            FunctionPinDisposition::Parameter,
            FunctionPinDisposition::CaptureStored,
            FunctionPinDisposition::CaptureWire,
            FunctionPinDisposition::CaptureStored,
        ]
    );

    // Delayed + unwired → parameter (same as Auto + unwired).
    set_role_raw(&mut designer, "main", mv_id, 1, FunctionPinRole::Delayed);
    assert_eq!(
        dispositions_of(&designer, "main", mv_id)[1],
        FunctionPinDisposition::Parameter
    );
}

/// A node with no parameters left is a legal `() -> R` thunk, and the Supplied
/// pins' stored data is baked into it.
#[test]
fn roles_all_supplied_is_thunk() {
    let mut designer = setup_designer_with_network("main");
    let crystal_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -120.0);
    // `input` is required, so capture it by wire; the property-backed pins
    // are Supplied from stored data.
    designer.connect_nodes(crystal_id, 0, mv_id, 0);
    set_role_raw(&mut designer, "main", mv_id, 1, FunctionPinRole::Supplied);
    set_role_raw(&mut designer, "main", mv_id, 2, FunctionPinRole::Supplied);
    set_role_raw(&mut designer, "main", mv_id, 3, FunctionPinRole::Supplied);

    let zc = match evaluate_node_pin(&designer, "main", mv_id, -1) {
        NetworkResult::Function(zc) => zc,
        other => panic!("expected a thunk, got {}", other.to_display_string()),
    };
    assert!(
        zc.param_types.is_empty(),
        "a fully captured/supplied node's function pin is a nullary thunk"
    );
    assert_eq!(zc.return_type, DataType::Crystal);
}

/// A **multi-wire** (array) pin: the role applies to the whole pin, never per
/// wire. Auto/Supplied capture all wires; Delayed drops all of them and takes
/// the declared `Array[T]` pin type (no witness for a multi-wire pin).
#[test]
fn roles_multi_wire_pin_applies_to_whole_pin() {
    let mut designer = setup_designer_with_network("main");

    // `array_concat(arrays: Array[Array[Int]])` — a genuine multi-wire pin.
    let a1 = add_expr(&mut designer, "main", "[1, 2]", vec![], 0.0);
    let a2 = add_expr(&mut designer, "main", "[3]", vec![], 80.0);
    let cat_id = designer.add_node("array_concat", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        cat_id,
        Box::new(ArrayConcatData {
            element_type: DataType::Int,
        }),
    );
    designer.connect_nodes(a1, 0, cat_id, 0);
    designer.connect_nodes(a2, 0, cat_id, 0);

    let wire_count = |d: &StructureDesigner| {
        d.node_type_registry
            .node_networks
            .get("main")
            .unwrap()
            .nodes
            .get(&cat_id)
            .unwrap()
            .arguments[0]
            .len()
    };
    assert_eq!(
        wire_count(&designer),
        2,
        "precondition: two wires on the `a` pin"
    );

    let body_arg_of = |d: &StructureDesigner| {
        let zc = match evaluate_node_pin(d, "main", cat_id, -1) {
            NetworkResult::Function(zc) => zc,
            other => panic!("expected Function, got {}", other.to_display_string()),
        };
        let arg = zc.body.nodes.values().next().unwrap().arguments[0].clone();
        (zc.param_types.clone(), arg)
    };

    // `b` is left unwired throughout, so it stays a plain parameter and every
    // assertion below is about the multi-wired `a` pin (index 0).
    //
    // Auto + wired → one capture-wire disposition for the pin as a whole; both
    // of its wires are captured, so `a` contributes no parameter.
    assert_eq!(
        dispositions_of(&designer, "main", cat_id),
        vec![
            FunctionPinDisposition::CaptureWire,
            FunctionPinDisposition::Parameter,
        ]
    );
    let (params, body_arg) = body_arg_of(&designer);
    assert_eq!(
        params,
        vec![DataType::Array(Box::new(DataType::Int))],
        "only `b` remains a parameter"
    );
    assert_eq!(
        body_arg.len(),
        2,
        "Auto+wired captures ALL of the pin's wires"
    );

    // Supplied + wired → identical to Auto + wired.
    set_role_raw(&mut designer, "main", cat_id, 0, FunctionPinRole::Supplied);
    assert_eq!(
        dispositions_of(&designer, "main", cat_id)[0],
        FunctionPinDisposition::CaptureWire
    );
    let (_, body_arg) = body_arg_of(&designer);
    assert_eq!(body_arg.len(), 2, "Supplied+wired captures ALL wires too");

    // Delayed + wired → a parameter, all wires dropped from the body, and the
    // parameter type is the *declared* `Array[Int]` pin type (no witness).
    set_role_raw(&mut designer, "main", cat_id, 0, FunctionPinRole::Delayed);
    assert_eq!(
        dispositions_of(&designer, "main", cat_id)[0],
        FunctionPinDisposition::Parameter
    );
    let (params, body_arg) = body_arg_of(&designer);
    assert_eq!(
        params,
        vec![
            DataType::Array(Box::new(DataType::Int)),
            DataType::Array(Box::new(DataType::Int)),
        ],
        "the Delayed `a` pin takes its DECLARED Array[Int] type (no witness)"
    );
    assert_eq!(
        body_arg.len(),
        1,
        "a Delayed pin's preview wires are all replaced by the parameter"
    );
    assert!(
        matches!(
            body_arg.incoming_wires[0].source_pin,
            SourcePin::ZoneInput { pin_index: 0 }
        ),
        "a Delayed multi-wire pin reads from the parameter frame, not its wires"
    );
}

// --- Typing (the preview-wire witness) --------------------------------------

/// The issue's exact case: `structure_move` with `input` Delayed + previewed
/// from a `Crystal` source and the other pins Supplied types as
/// `(Crystal) -> Crystal`.
#[test]
fn roles_witness_types_structure_move_as_crystal_to_crystal() {
    let mut designer = setup_designer_with_network("main");
    let preview_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -120.0);

    // Before roles: all pins unwired ⇒ arity 3 and an unresolvable
    // `same_as_input` return ⇒ the `-1` pin has no type at all.
    assert_eq!(
        function_pin_type(&designer, "main", mv_id),
        None,
        "an unwitnessed structure_move has no resolvable function type"
    );

    designer.connect_nodes(preview_id, 0, mv_id, 0); // preview wire
    configure_move_as_delayed_input(&mut designer, "main", mv_id);

    assert_eq!(
        function_pin_type(&designer, "main", mv_id),
        Some(DataType::Function(FunctionType::new(
            vec![DataType::Crystal],
            DataType::Crystal
        ))),
        "the preview wire witnesses both the parameter and the same_as_input return"
    );
}

/// The witness falls back to the **declared** pin type when the preview
/// source's own type doesn't resolve.
#[test]
fn roles_witness_falls_back_to_declared_pin_type() {
    let mut designer = setup_designer_with_network("main");

    // `free_move`'s output is `same_as_input` with its input unwired, so its
    // pin 0 does not resolve — an unresolvable preview source.
    let unresolved_id = designer.add_node("free_move", DVec2::new(0.0, 0.0));

    // `materialize`'s pin 0 is `Fixed(Crystal)`, so its return resolves
    // regardless of what feeds `shape`; that isolates the *parameter*
    // resolution, which must fall back to the declared pin type.
    let mat_id = designer.add_node("materialize", DVec2::new(300.0, 200.0));
    let mat_pins = designer
        .node_type_registry
        .get_node_type("materialize")
        .unwrap()
        .parameters
        .len();
    designer.connect_nodes(unresolved_id, 0, mat_id, 0);
    set_role_raw(&mut designer, "main", mat_id, 0, FunctionPinRole::Delayed);
    for pin in 1..mat_pins {
        set_role_raw(
            &mut designer,
            "main",
            mat_id,
            pin,
            FunctionPinRole::Supplied,
        );
    }

    assert_eq!(
        function_pin_type(&designer, "main", mat_id),
        Some(DataType::Function(FunctionType::new(
            vec![DataType::Blueprint], // the declared `shape` pin type
            DataType::Crystal
        ))),
        "an unresolvable preview source falls back to the declared pin type"
    );
}

/// The witness is **transitive**: retyping the preview wire's *upstream*
/// re-derives the `-1` type on the next resolve.
#[test]
fn roles_witness_updates_transitively() {
    let mut designer = setup_designer_with_network("main");

    // `free_move(input: HasFreeLinOps)` previewed through `exit_structure`
    // (`Crystal -> Molecule`).
    let crystal_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let exit_id = designer.add_node("exit_structure", DVec2::new(300.0, 60.0));
    designer.connect_nodes(crystal_id, 0, exit_id, 0);

    let mv_id = designer.add_node("free_move", DVec2::new(450.0, -120.0));
    let mv_pins = designer
        .node_type_registry
        .get_node_type("free_move")
        .unwrap()
        .parameters
        .len();
    designer.connect_nodes(exit_id, 0, mv_id, 0);
    set_role_raw(&mut designer, "main", mv_id, 0, FunctionPinRole::Delayed);
    for pin in 1..mv_pins {
        set_role_raw(&mut designer, "main", mv_id, pin, FunctionPinRole::Supplied);
    }

    assert_eq!(
        function_pin_type(&designer, "main", mv_id),
        Some(DataType::Function(FunctionType::new(
            vec![DataType::Molecule],
            DataType::Molecule
        ))),
        "witnessed through exit_structure → Molecule"
    );

    // Swap the *upstream* of the previewed pin: route the same crystal through
    // `dematerialize` (`Crystal -> Blueprint`) instead.
    let demat_id = designer.add_node("dematerialize", DVec2::new(300.0, 300.0));
    designer.connect_nodes(crystal_id, 0, demat_id, 0);
    clear_pin_wires(&mut designer, "main", mv_id, 0);
    designer.connect_nodes(demat_id, 0, mv_id, 0);

    assert_eq!(
        function_pin_type(&designer, "main", mv_id),
        Some(DataType::Function(FunctionType::new(
            vec![DataType::Blueprint],
            DataType::Blueprint
        ))),
        "the -1 type follows the preview wire's upstream chain"
    );
}

// --- End-to-end: the issue's workflow ---------------------------------------

/// The full issue #408 case: a `structure_move` marked `Delayed` on `input`
/// (with a preview wire) and `Supplied` on the rest is invoked via `apply` — the
/// output is moved by the node's **stored** translation, and the preview wire is
/// ignored in favour of the caller's argument.
#[test]
fn roles_end_to_end_structure_move_applies_stored_translation() {
    let mut designer = setup_designer_with_network("main");

    // Two *different* crystals so "which one came out" is observable: the
    // preview source (extent 3) and the actual argument (extent 1).
    let preview_id = add_crystal_source(&mut designer, "main", 3, 0.0);
    let arg_id = add_crystal_source(&mut designer, "main", 1, 300.0);

    let translation = IVec3::new(2, 0, 0);
    let mv_id = add_structure_move(&mut designer, "main", translation, -160.0);
    designer.connect_nodes(preview_id, 0, mv_id, 0); // preview wire
    configure_move_as_delayed_input(&mut designer, "main", mv_id);

    let apply_id = add_apply_of_function_pin(&mut designer, "main", mv_id, arg_id, 400.0);
    let applied = extract_atoms(evaluate_node(&designer, "main", apply_id));

    // Reference: the same stored translation applied to the argument crystal
    // directly by an ordinary (non-function) structure_move.
    let ref_mv = add_structure_move(&mut designer, "main", translation, 600.0);
    designer.connect_nodes(arg_id, 0, ref_mv, 0);
    let expected = extract_atoms(evaluate_node(&designer, "main", ref_mv));
    assert_structures_equivalent(&applied, &expected, 1e-9);

    // And it is *not* the preview crystal: the two differ in size, so a leaked
    // preview wire would show up as a different atom count.
    let preview_atoms = extract_atoms(evaluate_node(&designer, "main", preview_id));
    assert_ne!(
        applied.atoms_values().count(),
        preview_atoms.atoms_values().count(),
        "the preview wire must be ignored at invocation"
    );
}

/// Editing the stored data of a `Supplied` pin (what a gizmo drag does) is
/// reflected by the `-1` consumer on the next evaluation — the closure is
/// rebuilt per consumer eval, never cached across the edit.
#[test]
fn roles_supplied_stored_value_is_fresh_after_edit() {
    let mut designer = setup_designer_with_network("main");
    let preview_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let arg_id = add_crystal_source(&mut designer, "main", 1, 300.0);

    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -160.0);
    designer.connect_nodes(preview_id, 0, mv_id, 0);
    configure_move_as_delayed_input(&mut designer, "main", mv_id);
    let apply_id = add_apply_of_function_pin(&mut designer, "main", mv_id, arg_id, 400.0);

    let before = extract_atoms(evaluate_node(&designer, "main", apply_id));

    // Simulate a gizmo drag: rewrite the stored translation.
    set_node_data(
        &mut designer,
        "main",
        mv_id,
        Box::new(StructureMoveData {
            translation: IVec3::new(5, 0, 0),
            lattice_subdivision: IVec3::ONE,
        }),
    );

    let after = extract_atoms(evaluate_node(&designer, "main", apply_id));
    let ref_mv = add_structure_move(&mut designer, "main", IVec3::new(5, 0, 0), 600.0);
    designer.connect_nodes(arg_id, 0, ref_mv, 0);
    let expected = extract_atoms(evaluate_node(&designer, "main", ref_mv));

    assert_structures_equivalent(&after, &expected, 1e-9);
    let first_x = |s: &AtomicStructure| s.atoms_values().next().unwrap().position.x;
    assert_ne!(
        first_x(&before),
        first_x(&after),
        "the new stored translation must reach the consumer"
    );
}

/// An **erroring preview source** does not poison the function: the wire is
/// dropped from the body, so the `-1` closure still builds and invokes, while
/// pin 0 shows the error normally.
#[test]
fn roles_erroring_preview_source_does_not_poison_the_function() {
    let mut designer = setup_designer_with_network("main");

    // An `enter_structure` with both inputs unwired evaluates to an Error at
    // pin 0, but its output pin is statically `Fixed(Crystal)` — a valid type
    // witness whose *value* is broken.
    let broken_id = designer.add_node("enter_structure", DVec2::new(0.0, 0.0));
    match evaluate_node(&designer, "main", broken_id) {
        NetworkResult::Error(_) => {}
        other => panic!(
            "precondition: the preview source must error at pin 0, got {}",
            other.to_display_string()
        ),
    }

    let arg_id = add_crystal_source(&mut designer, "main", 1, 300.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(2, 0, 0), -160.0);
    designer.connect_nodes(broken_id, 0, mv_id, 0);
    configure_move_as_delayed_input(&mut designer, "main", mv_id);

    // Pin 0 of the function node itself errors (it evaluates the broken wire)…
    match evaluate_node(&designer, "main", mv_id) {
        NetworkResult::Error(_) => {}
        other => panic!(
            "expected the previewed node's pin 0 to error, got {}",
            other.to_display_string()
        ),
    }

    // …but the function value builds and invokes fine.
    let apply_id = add_apply_of_function_pin(&mut designer, "main", mv_id, arg_id, 400.0);
    let applied = extract_atoms(evaluate_node(&designer, "main", apply_id));
    let ref_mv = add_structure_move(&mut designer, "main", IVec3::new(2, 0, 0), 600.0);
    designer.connect_nodes(arg_id, 0, ref_mv, 0);
    assert_structures_equivalent(
        &applied,
        &extract_atoms(evaluate_node(&designer, "main", ref_mv)),
        1e-9,
    );
}

// --- Connection gating ------------------------------------------------------

/// Before roles, a `structure_move.-1` is rejected by a `(Crystal) -> Crystal`
/// consumer (no resolvable type); after the Supplied/Delayed+preview setup it is
/// accepted. Reverting the role re-flags the existing wire on revalidate.
#[test]
fn roles_connection_gating_and_revert() {
    let mut designer = setup_designer_with_network("main");
    let preview_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -160.0);

    // A `map` over `Iter[Crystal]` wants `(Crystal) -> Crystal` at `f`.
    let map_id = add_map(
        &mut designer,
        "main",
        DataType::Crystal,
        DataType::Crystal,
        400.0,
    );

    assert!(
        !designer.can_connect_nodes(mv_id, -1, map_id, 1),
        "an unwitnessed structure_move.-1 has no type and must be rejected"
    );

    designer.connect_nodes(preview_id, 0, mv_id, 0);
    configure_move_as_delayed_input(&mut designer, "main", mv_id);

    assert!(
        designer.can_connect_nodes(mv_id, -1, map_id, 1),
        "Delayed+preview + Supplied makes structure_move a (Crystal) -> Crystal function"
    );
    designer.connect_nodes(mv_id, -1, map_id, 1);
    designer.connect_nodes(preview_id, 0, map_id, 0); // xs (broadcasts)
    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(valid, "the wired-up network validates clean: {errors:?}");

    // Revert `input` to Auto: the wired pin becomes a capture, so the exposed
    // type collapses to a `() -> Crystal` thunk, which no longer fits `map.f`.
    clear_role_raw(&mut designer, "main", mv_id, 0);
    let (_, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        errors.iter().any(|e| e.contains("Data type mismatch")),
        "reverting the role must re-flag the existing -1 wire, got {errors:?}"
    );
}

// --- Validation -------------------------------------------------------------

/// `Supplied` + unwired + **required** pin → a **non-blocking** warning, and the
/// invocation yields a localized error rather than a panic.
///
/// Uses `materialize` rather than the design's `structure_move` example because
/// `structure_move`'s pin 0 is `same_as_input`: leaving its required `input`
/// unwired *also* trips the pre-existing **blocking** "polymorphic output pin
/// could not be resolved" rule, which would mask the blast radius this test is
/// about. `materialize` has the same shape (a required `shape` pin) but a
/// `Fixed(Crystal)` output, so the only thing under test is the new warning.
#[test]
fn roles_supplied_required_unwired_warns_non_blocking() {
    let mut designer = setup_designer_with_network("main");
    let arg_id = add_cuboid(&mut designer, "main", 1, 300.0);
    let preview_id = add_cuboid(&mut designer, "main", 2, 0.0);

    let mat_id = designer.add_node("materialize", DVec2::new(300.0, -160.0));
    let mat_pins = designer
        .node_type_registry
        .get_node_type("materialize")
        .unwrap()
        .parameters
        .len();
    designer.connect_nodes(preview_id, 0, mat_id, 0); // preview wire on `shape`
    set_role_raw(&mut designer, "main", mat_id, 0, FunctionPinRole::Delayed);
    for pin in 1..mat_pins {
        set_role_raw(
            &mut designer,
            "main",
            mat_id,
            pin,
            FunctionPinRole::Supplied,
        );
    }
    let apply_id = add_apply_of_function_pin(&mut designer, "main", mat_id, arg_id, 400.0);

    // Precondition: the property-backed pins are Supplied but *not* required
    // (they have stored values to bake in), so no warning yet.
    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(valid, "{errors:?}");
    assert!(
        !errors.iter().any(|e| e.contains("marked Supplied")),
        "property-backed pins have a stored value; no warning expected: {errors:?}"
    );

    // Now mark the **required** `shape` pin Supplied while unwired.
    clear_pin_wires(&mut designer, "main", mat_id, 0);
    set_role_raw(&mut designer, "main", mat_id, 0, FunctionPinRole::Supplied);
    let (valid, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'shape'") && e.contains("marked Supplied")),
        "expected the Supplied-required warning, got {errors:?}"
    );
    assert!(
        valid,
        "the warning is non-blocking — the rest of the network keeps evaluating: {errors:?}"
    );

    // An unrelated node still evaluates, and invoking yields a localized Error
    // (not a panic).
    match evaluate_node(&designer, "main", arg_id) {
        NetworkResult::Blueprint(_) => {}
        other => panic!(
            "an unrelated node must keep evaluating, got {}",
            other.to_display_string()
        ),
    }
    match evaluate_node(&designer, "main", apply_id) {
        NetworkResult::Error(_) => {}
        other => panic!(
            "expected a localized Error at invocation, got {}",
            other.to_display_string()
        ),
    }
}

/// The warning is gated on the `-1` pin actually being consumed: inert roles on
/// an unconsumed node must not produce noise.
#[test]
fn roles_supplied_required_warning_is_gated_on_consumption() {
    let mut designer = setup_designer_with_network("main");
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -160.0);
    set_role_raw(&mut designer, "main", mv_id, 0, FunctionPinRole::Supplied);

    let (_, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        !errors.iter().any(|e| e.contains("marked Supplied")),
        "an unconsumed node's roles are inert — no warning: {errors:?}"
    );

    // Connect a consumer of the `-1` pin → the warning appears.
    let map_id = add_map(
        &mut designer,
        "main",
        DataType::Crystal,
        DataType::Crystal,
        400.0,
    );
    wire_function_pin(&mut designer, "main", mv_id, map_id, 1);
    let (_, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        errors.iter().any(|e| e.contains("marked Supplied")),
        "a consumed node's Supplied+required+unwired pin warns: {errors:?}"
    );

    // Delete the consumer → the warning goes away.
    remove_node(&mut designer, "main", map_id);
    let (_, errors) = validate_and_errors(&mut designer, "main");
    assert!(
        !errors.iter().any(|e| e.contains("marked Supplied")),
        "removing the consumer clears the warning: {errors:?}"
    );
}

// --- Repair ------------------------------------------------------------------

/// Out-of-range role entries (a stale index after a pin-layout change) are
/// ignored by the partition and pruned by the repair pass.
#[test]
fn roles_out_of_range_entries_are_pruned_and_ignored() {
    let mut designer = setup_designer_with_network("main");
    let mv_id = add_structure_move(&mut designer, "main", IVec3::ZERO, 0.0);

    // Plant an entry past the last pin (structure_move has 4 input pins).
    set_role_raw(&mut designer, "main", mv_id, 7, FunctionPinRole::Supplied);

    // The partition ignores it — one disposition per *declared* pin.
    assert_eq!(dispositions_of(&designer, "main", mv_id).len(), 4);

    // And the repair pass prunes it.
    designer.validate_active_network();
    assert!(
        roles_of(&designer, "main", mv_id).is_empty(),
        "repair prunes role entries that no longer name a pin"
    );
}

// --- Setter + map invariant --------------------------------------------------

/// The setter normalizes `Auto` to **entry removal** (the map never stores an
/// explicit `Auto`), and a no-op change pushes no undo command.
#[test]
fn roles_setter_normalizes_auto_to_absence() {
    let mut designer = setup_designer_with_network("main");
    let mv_id = add_structure_move(&mut designer, "main", IVec3::ZERO, 0.0);
    designer.undo_stack.clear();

    // Auto → Auto is a no-op: no entry, no command.
    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Auto);
    assert!(roles_of(&designer, "main", mv_id).is_empty());
    assert!(!designer.undo_stack.can_undo(), "a no-op pushes no command");

    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Supplied);
    assert_eq!(
        roles_of(&designer, "main", mv_id).get(&1),
        Some(&FunctionPinRole::Supplied)
    );
    assert!(designer.undo_stack.can_undo());

    // Setting it back to Auto removes the entry rather than storing `Auto`.
    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Auto);
    assert!(
        roles_of(&designer, "main", mv_id).is_empty(),
        "Auto is represented by absence"
    );

    // An out-of-range pin index is rejected outright.
    designer.set_function_pin_role(&[], mv_id, 9, FunctionPinRole::Delayed);
    assert!(roles_of(&designer, "main", mv_id).is_empty());
}

// --- API surface (Phase 3) ---------------------------------------------------

/// The sidebar renders `APIFunctionPinRoleView::effective` verbatim, so it must
/// be the shared partition — not a UI-side re-derivation that could silently
/// disagree with the resolver and the closure synthesizer. Check the whole row
/// set against `function_pin_dispositions` for a node mixing all three roles ×
/// wired/unwired.
#[test]
fn api_function_pin_role_views_match_the_shared_partition() {
    let mut designer = setup_designer_with_network("main");
    let crystal_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let int_id = add_int(&mut designer, "main", 3, 200.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -120.0);

    // `input`: Delayed + wired (preview). `translation`: Supplied + unwired
    // (stored/gizmo). `subdivision`: Supplied + wired (frozen capture). That
    // covers a parameter, a capture-stored, and a capture-wire in one node.
    designer.connect_nodes(crystal_id, 0, mv_id, 0);
    designer.connect_nodes(int_id, 0, mv_id, 2);
    designer.set_function_pin_role(&[], mv_id, 0, FunctionPinRole::Delayed);
    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Supplied);
    designer.set_function_pin_role(&[], mv_id, 2, FunctionPinRole::Supplied);

    let registry = &designer.node_type_registry;
    let node = registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&mv_id)
        .unwrap();
    let node_type = registry.get_node_type_for_node(node).unwrap();
    let views = build_function_pin_role_views(node, node_type);

    // One row per declared input pin, in pin order, named after the pin.
    assert_eq!(
        views
            .iter()
            .map(|v| v.pin_name.as_str())
            .collect::<Vec<_>>(),
        node_type
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
    );

    // `effective` agrees with the shared helper, row for row.
    let expected: Vec<APIFunctionPinDisposition> = function_pin_dispositions(node, node_type)
        .into_iter()
        .map(Into::into)
        .collect();
    assert_eq!(
        views.iter().map(|v| v.effective).collect::<Vec<_>>(),
        expected
    );
    // ...and is the table from the design doc, spelled out. The trailing
    // `subdiv_xyz` pin is untouched (Auto + unwired → parameter).
    assert_eq!(
        expected,
        vec![
            APIFunctionPinDisposition::Parameter,
            APIFunctionPinDisposition::CaptureStored,
            APIFunctionPinDisposition::CaptureWire,
            APIFunctionPinDisposition::Parameter,
        ]
    );

    // The stored roles and the wiring flags round-trip faithfully.
    assert_eq!(
        views.iter().map(|v| v.role).collect::<Vec<_>>(),
        vec![
            APIFunctionPinRole::Delayed,
            APIFunctionPinRole::Supplied,
            APIFunctionPinRole::Supplied,
            APIFunctionPinRole::Auto,
        ]
    );
    assert_eq!(
        views.iter().map(|v| v.wired).collect::<Vec<_>>(),
        vec![true, false, true, false]
    );

    // An `Auto` pin (no stored entry) reports `Auto` explicitly — the API
    // always names a role even though absence is the storage form.
    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Auto);
    let registry = &designer.node_type_registry;
    let node = registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&mv_id)
        .unwrap();
    let views = build_function_pin_role_views(node, registry.get_node_type_for_node(node).unwrap());
    assert_eq!(views[1].role, APIFunctionPinRole::Auto);
    assert_eq!(views[1].effective, APIFunctionPinDisposition::Parameter);
}

// --- Display relaxation (Phase 2) --------------------------------------------

/// Under the **Frontier** policy a function-mode node stays auto-hidden without
/// any special-casing: `build_reverse_dependency_map` registers the consumer's
/// `-1` wire like any other, so the node has a dependent and is not on the
/// frontier. This is what keeps the display relaxation from adding visual noise
/// by default.
#[test]
fn frontier_policy_auto_hides_function_mode_node() {
    let mut designer = setup_designer_with_network("main");
    designer.set_preferences(StructureDesignerPreferences {
        node_display_preferences: NodeDisplayPreferences {
            display_policy: NodeDisplayPolicy::PreferFrontier,
            ..Default::default()
        },
        ..Default::default()
    });

    // materialize(cuboid) → structure_move.input, structure_move.-1 → apply.f.
    let src_id = add_crystal_source(&mut designer, "main", 4, 0.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(2, 0, 0), 0.0);
    designer.connect_nodes(src_id, 0, mv_id, 0);
    configure_move_as_delayed_input(&mut designer, "main", mv_id);
    let arg_id = add_crystal_source(&mut designer, "main", 2, 200.0);
    let apply_id = add_apply_of_function_pin(&mut designer, "main", mv_id, arg_id, 200.0);
    designer.validate_active_network();
    designer.apply_node_display_policy(None);

    let net = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        !net.is_node_displayed(mv_id),
        "the `-1` wire gives the function node a dependent, so Frontier hides it"
    );
    assert!(
        net.is_node_displayed(apply_id),
        "the consumer is the frontier node and stays displayed"
    );
}

/// The gadget precondition, headless: a **selected** function-mode
/// `structure_move` with a preview wire must populate
/// `selected_node_eval_cache` during scene generation — that cache is what
/// `provide_gadget` downcasts to build the drag gizmo. Before the display
/// relaxation the scene skip returned early and the cache stayed `None`, so the
/// gizmo was unreachable.
#[test]
fn function_mode_node_populates_selected_node_eval_cache() {
    let mut designer = setup_designer_with_network("main");

    let src_id = add_crystal_source(&mut designer, "main", 4, 0.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(2, 0, 0), 0.0);
    designer.connect_nodes(src_id, 0, mv_id, 0); // Delayed => preview wire
    configure_move_as_delayed_input(&mut designer, "main", mv_id);
    let arg_id = add_crystal_source(&mut designer, "main", 2, 200.0);
    add_apply_of_function_pin(&mut designer, "main", mv_id, arg_id, 200.0);

    designer.select_node(mv_id);
    designer.set_node_display(mv_id, true);
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);

    let cache = designer
        .get_selected_node_eval_cache()
        .expect("a selected function-mode node must still populate its eval cache");
    assert!(
        cache.downcast_ref::<StructureMoveEvalCache>().is_some(),
        "the cache must be the structure_move gadget's cache type"
    );
}
