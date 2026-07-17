//! Currying Phase 1 + 2 tests.
//!
//! Phase 1 — canonical `FunctionType` storage. Verifies that:
//! 1. `FunctionType::new` collapses nested `Function` returns into a single
//!    flat parameter list.
//! 2. `canonicalize_data_type` recurses through `Array`, `Iterator`, function
//!    parameters/returns, and `Record::Anonymous` shapes.
//! 3. serde `Deserialize` routes through `FunctionType::new` so JSON-loaded
//!    non-canonical forms canonicalize as they enter memory.
//! 4. The data-type string parser produces canonical output for nested
//!    function syntax (`A -> B -> C -> D`).
//! 5. `canonicalize_network` flattens DataType fields stored on node-data
//!    variants (ClosureData/MapData/etc.) and on record type defs.
//! 6. The existing fixture set is already canonical — canonicalization is the
//!    identity for every shape we ship with.
//!
//! Phase 2 — `ZoneClosure.pre_supplied_args` substrate. Verifies that
//! `run_closure_once` prepends `pre_supplied_args` to the caller-supplied
//! iteration frame so the body's `ZoneInput { pin_index }` resolution lines up
//! positionally. No node yet *produces* a non-empty value (Phase 3's `apply`
//! rewrite is what will), so the rest of the closure / HOF suite is the
//! byte-identical regression check.
//!
//! See `doc/design_currying.md`.

use std::collections::HashMap;
use std::sync::Arc;

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::canonicalize::{
    canonicalize_network, canonicalize_record_type_defs,
};
use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, FunctionType, RecordType, canonicalize_data_type,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::evaluator::zone_closure::{
    ZoneClosure, run_closure_once,
};
use rust_lib_flutter_cad::structure_designer::node_data::{NoData, NodeData};
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, CollapseMode, DEFAULT_BODY_HEIGHT, DEFAULT_BODY_WIDTH, IncomingWire, Node,
    NodeNetwork, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_type::{
    NodeType, no_data_loader, no_data_saver,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// Helper: build a non-canonical Function((A,), Function((B, C), D)) value
// the only way callers can after Phase 1 — via struct literal, bypassing
// FunctionType::new.
fn non_canonical_a_then_bc_to_d() -> DataType {
    DataType::Function(FunctionType {
        parameter_types: vec![DataType::Float],
        output_type: Box::new(DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int, DataType::Bool],
            output_type: Box::new(DataType::String),
        })),
    })
}

fn canonical_abc_to_d() -> DataType {
    DataType::Function(FunctionType::new(
        vec![DataType::Float, DataType::Int, DataType::Bool],
        DataType::String,
    ))
}

// =============================================================================
// FunctionType::new golden cases
// =============================================================================

#[test]
fn function_type_new_absorbs_nested_function_return() {
    // (A) -> ((B, C) -> D) should canonicalize to (A, B, C) -> D.
    let nested_return = DataType::Function(FunctionType::new(
        vec![DataType::Int, DataType::Bool],
        DataType::String,
    ));
    let ft = FunctionType::new(vec![DataType::Float], nested_return);
    assert_eq!(
        ft.parameter_types,
        vec![DataType::Float, DataType::Int, DataType::Bool]
    );
    assert_eq!(*ft.output_type, DataType::String);
}

#[test]
fn function_type_new_is_identity_on_already_flat() {
    let ft = FunctionType::new(
        vec![DataType::Float, DataType::Int, DataType::Bool],
        DataType::String,
    );
    assert_eq!(
        ft.parameter_types,
        vec![DataType::Float, DataType::Int, DataType::Bool]
    );
    assert_eq!(*ft.output_type, DataType::String);
}

#[test]
fn function_type_new_collapses_three_layer_curried_chain() {
    // A -> B -> C -> D, built as right-associative single-param functions,
    // collapses to (A, B, C) -> D.
    let inner = DataType::Function(FunctionType::new(vec![DataType::Bool], DataType::String));
    let middle = DataType::Function(FunctionType::new(vec![DataType::Int], inner));
    let ft = FunctionType::new(vec![DataType::Float], middle);
    assert_eq!(
        ft.parameter_types,
        vec![DataType::Float, DataType::Int, DataType::Bool]
    );
    assert_eq!(*ft.output_type, DataType::String);
}

#[test]
fn function_type_new_zero_arg_with_function_return_absorbs() {
    // () -> ((Int) -> Float) becomes (Int) -> Float.
    let inner = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Float));
    let ft = FunctionType::new(vec![], inner);
    assert_eq!(ft.parameter_types, vec![DataType::Int]);
    assert_eq!(*ft.output_type, DataType::Float);
}

#[test]
fn function_type_new_preserves_zero_arity_when_return_is_not_function() {
    let ft = FunctionType::new(vec![], DataType::Float);
    assert!(ft.parameter_types.is_empty());
    assert_eq!(*ft.output_type, DataType::Float);
}

// =============================================================================
// canonicalize_data_type — recursion through container shapes
// =============================================================================

#[test]
fn canonicalize_walks_through_iterator() {
    let mut t = DataType::Iterator(Box::new(non_canonical_a_then_bc_to_d()));
    canonicalize_data_type(&mut t);
    assert_eq!(t, DataType::Iterator(Box::new(canonical_abc_to_d())));
}

#[test]
fn canonicalize_walks_through_array() {
    let mut t = DataType::Array(Box::new(non_canonical_a_then_bc_to_d()));
    canonicalize_data_type(&mut t);
    assert_eq!(t, DataType::Array(Box::new(canonical_abc_to_d())));
}

#[test]
fn canonicalize_walks_through_nested_function_parameter() {
    // ((A) -> ((B, C) -> D)) -> E carries a non-canonical Function in its
    // parameter list; canonicalization must flatten the inner parameter type
    // as well as any outer one.
    let mut t = DataType::Function(FunctionType {
        parameter_types: vec![non_canonical_a_then_bc_to_d()],
        output_type: Box::new(DataType::Vec3),
    });
    canonicalize_data_type(&mut t);
    let expected = DataType::Function(FunctionType::new(
        vec![canonical_abc_to_d()],
        DataType::Vec3,
    ));
    assert_eq!(t, expected);
}

#[test]
fn canonicalize_walks_through_anonymous_record_fields() {
    let mut t = DataType::Record(RecordType::anonymous(vec![
        ("f".to_string(), non_canonical_a_then_bc_to_d()),
        (
            "g".to_string(),
            DataType::Iterator(Box::new(non_canonical_a_then_bc_to_d())),
        ),
    ]));
    canonicalize_data_type(&mut t);
    let expected = DataType::Record(RecordType::anonymous(vec![
        ("f".to_string(), canonical_abc_to_d()),
        (
            "g".to_string(),
            DataType::Iterator(Box::new(canonical_abc_to_d())),
        ),
    ]));
    assert_eq!(t, expected);
}

#[test]
fn canonicalize_is_identity_on_already_canonical() {
    let mut t = canonical_abc_to_d();
    let expected = t.clone();
    canonicalize_data_type(&mut t);
    assert_eq!(t, expected);
}

#[test]
fn canonicalize_is_identity_on_leaf_types() {
    for mut t in [
        DataType::Bool,
        DataType::Int,
        DataType::Float,
        DataType::Vec3,
        DataType::Crystal,
        DataType::Record(RecordType::Named("Foo".to_string())),
    ] {
        let expected = t.clone();
        canonicalize_data_type(&mut t);
        assert_eq!(t, expected, "leaf type should be unchanged");
    }
}

// =============================================================================
// Serde deserialize routing
// =============================================================================

#[test]
fn deserialize_canonicalizes_nested_function() {
    let json = serde_json::json!({
        "Function": {
            "parameter_types": ["Float"],
            "output_type": {
                "Function": {
                    "parameter_types": ["Int", "Bool"],
                    "output_type": "String"
                }
            }
        }
    });
    let parsed: DataType = serde_json::from_value(json).expect("deserialize");
    assert_eq!(parsed, canonical_abc_to_d());
}

#[test]
fn deserialize_canonicalizes_nested_function_inside_iterator() {
    let json = serde_json::json!({
        "Iterator": {
            "Function": {
                "parameter_types": ["Float"],
                "output_type": {
                    "Function": {
                        "parameter_types": ["Int"],
                        "output_type": "Bool"
                    }
                }
            }
        }
    });
    let parsed: DataType = serde_json::from_value(json).expect("deserialize");
    let expected = DataType::Iterator(Box::new(DataType::Function(FunctionType::new(
        vec![DataType::Float, DataType::Int],
        DataType::Bool,
    ))));
    assert_eq!(parsed, expected);
}

// =============================================================================
// Parser routing
// =============================================================================

#[test]
fn parser_produces_canonical_for_right_associative_chain() {
    // `Float -> Int -> Bool -> String` parses right-associative but should
    // store as a single flat (Float, Int, Bool) -> String.
    let parsed = DataType::from_string("Float -> Int -> Bool -> String").expect("parse");
    assert_eq!(parsed, canonical_abc_to_d());
}

#[test]
fn parser_produces_canonical_for_paren_then_arrow() {
    // `(Float, Int) => (Bool -> String)` should flatten to (Float, Int, Bool) -> String.
    let parsed = DataType::from_string("(Float, Int) => (Bool -> String)").expect("parse");
    assert_eq!(parsed, canonical_abc_to_d());
}

#[test]
fn parser_produces_canonical_for_zero_arg_function_returning_function() {
    // `() -> (Int -> Float)` should flatten to (Int) -> Float.
    let parsed = DataType::from_string("() -> (Int -> Float)").expect("parse");
    let expected = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Float));
    assert_eq!(parsed, expected);
}

// =============================================================================
// canonicalize_network driver
// =============================================================================

#[test]
fn canonicalize_network_flattens_closure_data_type_args() {
    // Build a network containing a `closure` node whose `type_args[-1]` was
    // hand-set to a non-canonical Function value (the situation a hand-edited
    // .cnnd file or a struct-literal test fixture can produce).
    let registry = NodeTypeRegistry::new();
    let closure_node_type = registry
        .built_in_node_types
        .get("closure")
        .expect("closure node type")
        .clone();

    // Build a wrapping custom node type for the network (the network's
    // signature pin types are independent of this test).
    let net_type = NodeType {
        name: "test_net".to_string(),
        description: String::new(),
        summary: None,
        category: closure_node_type.category,
        parameters: vec![],
        output_pins: vec![],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: false,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    };
    let mut network = NodeNetwork::new(net_type);

    // Insert a closure node with a non-canonical type_args[-1].
    let closure_data = ClosureData {
        kind: rust_lib_flutter_cad::structure_designer::nodes::closure::ClosureKind::Custom,
        type_args: vec![DataType::Float, non_canonical_a_then_bc_to_d()],
        param_names: vec!["x".to_string()],
        custom_label: None,
    };
    let node = Node {
        id: 7,
        node_type_name: "closure".to_string(),
        custom_name: None,
        position: glam::f64::DVec2::ZERO,
        arguments: (0..closure_node_type.parameters.len())
            .map(|_| Argument::new())
            .collect(),
        data: Box::new(closure_data),
        custom_node_type: None,
        zone: None,
        zone_output_arguments: Vec::new(),
        body_width: DEFAULT_BODY_WIDTH,
        body_height: DEFAULT_BODY_HEIGHT,
        collapse_mode: CollapseMode::Auto,
        function_pin_roles: std::collections::BTreeMap::new(),
    };
    network.nodes.insert(7, node);
    network.next_node_id = 8;

    canonicalize_network(&mut network);

    let n = network.nodes.get(&7).expect("closure node");
    let cd = n
        .data
        .as_ref()
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("ClosureData");
    assert_eq!(cd.type_args[0], DataType::Float);
    assert_eq!(cd.type_args[1], canonical_abc_to_d());
}

#[test]
fn canonicalize_network_flattens_map_data_output_type() {
    let registry = NodeTypeRegistry::new();
    let map_node_type = registry
        .built_in_node_types
        .get("map")
        .expect("map node type")
        .clone();

    let net_type = NodeType {
        name: "test_net".to_string(),
        description: String::new(),
        summary: None,
        category: map_node_type.category,
        parameters: vec![],
        output_pins: vec![],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: false,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    };
    let mut network = NodeNetwork::new(net_type);

    let map_data = MapData {
        input_type: DataType::Float,
        output_type: non_canonical_a_then_bc_to_d(),
    };
    let node = Node {
        id: 3,
        node_type_name: "map".to_string(),
        custom_name: None,
        position: glam::f64::DVec2::ZERO,
        arguments: (0..map_node_type.parameters.len())
            .map(|_| Argument::new())
            .collect(),
        data: Box::new(map_data),
        custom_node_type: None,
        zone: None,
        zone_output_arguments: Vec::new(),
        body_width: DEFAULT_BODY_WIDTH,
        body_height: DEFAULT_BODY_HEIGHT,
        collapse_mode: CollapseMode::Auto,
        function_pin_roles: std::collections::BTreeMap::new(),
    };
    network.nodes.insert(3, node);
    network.next_node_id = 4;

    canonicalize_network(&mut network);

    let n = network.nodes.get(&3).expect("map node");
    let md = n
        .data
        .as_ref()
        .as_any_ref()
        .downcast_ref::<MapData>()
        .expect("MapData");
    assert_eq!(md.input_type, DataType::Float);
    assert_eq!(md.output_type, canonical_abc_to_d());
}

#[test]
fn canonicalize_record_type_defs_flattens_field_types() {
    let mut defs = HashMap::new();
    defs.insert(
        "Pair".to_string(),
        RecordTypeDef::from_named_fields(
            "Pair".to_string(),
            vec![
                ("f".to_string(), non_canonical_a_then_bc_to_d()),
                ("g".to_string(), DataType::Float),
            ],
        ),
    );
    canonicalize_record_type_defs(&mut defs);
    let def = defs.get("Pair").expect("Pair");
    assert_eq!(def.fields[0].data_type, canonical_abc_to_d());
    assert_eq!(def.fields[1].data_type, DataType::Float);
}

// =============================================================================
// Existing-fixture regression — canonicalization is the identity for every
// built-in node type's declared pin signatures.
// =============================================================================

#[test]
fn built_in_node_type_signatures_are_already_canonical() {
    let registry = NodeTypeRegistry::new();
    for (name, node_type) in &registry.built_in_node_types {
        // Snapshot signature.
        let before_params: Vec<DataType> = node_type
            .parameters
            .iter()
            .map(|p| p.data_type.clone())
            .collect();
        let before_outputs: Vec<DataType> = node_type
            .output_pins
            .iter()
            .map(|p| p.data_type.fixed_type().cloned().unwrap_or(DataType::None))
            .collect();

        // Re-canonicalize the in-memory signature and compare. We use the
        // public canonicalize_data_type walker directly so we don't need
        // mutable access to the registry's stored types.
        for (i, p) in node_type.parameters.iter().enumerate() {
            let mut canonical = p.data_type.clone();
            canonicalize_data_type(&mut canonical);
            assert_eq!(
                canonical, before_params[i],
                "built-in node type '{}' parameter {} '{}' is not canonical",
                name, i, p.name
            );
        }
        for (i, p) in node_type.output_pins.iter().enumerate() {
            if let Some(declared) = p.data_type.fixed_type() {
                let mut canonical = declared.clone();
                canonicalize_data_type(&mut canonical);
                assert_eq!(
                    canonical, before_outputs[i],
                    "built-in node type '{}' output pin {} '{}' is not canonical",
                    name, i, p.name
                );
            }
        }
    }
}

// ============================================================================
// Phase 2 — `ZoneClosure.pre_supplied_args` prepend
// ============================================================================
//
// The substrate change: a `ZoneClosure` carries an `Arc<Vec<NetworkResult>>`
// of args already bound by partial application, prepended inside
// `run_closure_once` before the caller-supplied frame is pushed. Until Phase 3
// no node produces a non-empty value, so we build a real two-param `Custom`
// closure end-to-end, take its emitted `Function` value, and hand-construct a
// derived `ZoneClosure` with `pre_supplied_args` populated to verify the
// prepend.

/// Extract an `Int` payload, panicking with a clear message otherwise.
/// (`NetworkResult` doesn't derive `PartialEq`/`Debug`, so `assert_eq!` on it
/// isn't available; matching it out at the call site is the project's standard
/// pattern — see `closures_test::extract_int`.)
fn phase2_extract_int(result: NetworkResult) -> i32 {
    match result {
        NetworkResult::Int(v) => v,
        NetworkResult::Error(msg) => panic!("expected Int, got Error: {msg}"),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

/// Set node data and refresh the node's custom-type cache (so its argument
/// pins, zone-input pins etc. match the new data shape).
fn phase2_set_node_data(
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

/// Add an `expr` node into a zone-owning node's body with the given parameters
/// (all `Int` here). Returns the new body node's id.
fn phase2_add_expr_to_body(
    designer: &mut StructureDesigner,
    owner_network: &str,
    owner_node_id: u64,
    expression: &str,
    param_names: &[&str],
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(owner_network).unwrap();
    let owner_node = network.nodes.get_mut(&owner_node_id).unwrap();
    let body = owner_node
        .zone_mut()
        .expect("zone-owning node missing zone");

    let expr_params: Vec<ExprParameter> = param_names
        .iter()
        .map(|name| ExprParameter {
            id: None,
            name: (*name).to_string(),
            data_type: DataType::Int,
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

/// Wire the owner's zone-input pin (an `element` / `acc` / Custom param) into a
/// body node argument.
fn phase2_wire_zone_input_to_body_node(
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

/// Wire a body node into the owner's first zone-output pin.
fn phase2_wire_body_node_to_zone_output(
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

/// Build a 2-arg `Custom`-kind closure `(a, b) -> a + b` (all `Int`) on the
/// active "main" network and return the emitted `Function` value together with
/// the registry it was built against. The returned `ZoneClosure` has
/// `pre_supplied_args` empty (the substrate's freshly-built default) and
/// `param_types = [Int, Int]`.
fn build_two_int_param_add_closure() -> (StructureDesigner, ZoneClosure) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let closure_id = designer.add_node("closure", DVec2::new(0.0, 0.0));
    phase2_set_node_data(
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

    let expr_id = phase2_add_expr_to_body(&mut designer, "main", closure_id, "a + b", &["a", "b"]);
    phase2_wire_zone_input_to_body_node(&mut designer, "main", closure_id, 0, expr_id, 0);
    phase2_wire_zone_input_to_body_node(&mut designer, "main", closure_id, 1, expr_id, 1);
    phase2_wire_body_node_to_zone_output(&mut designer, "main", closure_id, expr_id);

    // Evaluate the closure node to obtain its `Function` value.
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("main").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&stack, closure_id, 0, registry, false, &mut context);

    let zc = match result {
        NetworkResult::Function(zc) => zc,
        other => panic!(
            "expected NetworkResult::Function from closure node, got {}",
            other.to_display_string()
        ),
    };
    assert!(
        zc.pre_supplied_args.is_empty(),
        "a freshly-built closure must carry an empty pre_supplied_args vector"
    );
    assert_eq!(
        zc.param_types,
        vec![DataType::Int, DataType::Int],
        "freshly-built `Custom` (Int, Int) -> Int closure must declare two Int params"
    );

    (designer, zc)
}

/// Baseline regression: when `pre_supplied_args` is empty (the freshly-built
/// default — every existing call site in Phase 2), `run_closure_once` consumes
/// exactly the caller-supplied frame, unchanged. `args = [10, 5]` on the
/// `(a, b) -> a + b` body resolves to `Int(15)`.
#[test]
fn run_closure_once_with_empty_pre_supplied_args_is_byte_identical() {
    let (designer, zc) = build_two_int_param_add_closure();

    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let result = run_closure_once(
        &evaluator,
        &[],
        &designer.node_type_registry,
        &mut context,
        &zc,
        vec![NetworkResult::Int(10), NetworkResult::Int(5)],
    );

    assert_eq!(phase2_extract_int(result), 15);
}

/// The Phase 2 invariant: `pre_supplied_args` is prepended to the
/// caller-supplied frame before the body resolves `ZoneInput { pin_index }`
/// references. With `pre_supplied_args = [10]` bound into slot 0 and
/// `args = [5]` filling slot 1, the body sees `a = 10, b = 5`, so
/// `a + b = 15` — *not* `5 + 0` (which would be the bug where prepending
/// silently drops).
#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn run_closure_once_prepends_pre_supplied_args_before_caller_frame() {
    let (designer, base_zc) = build_two_int_param_add_closure();

    // Hand-construct a partially-applied derivative: clone every shared field,
    // bind the first param (slot 0 = `a` = 10), leave the second to the
    // caller. `param_types` shrinks to the *remaining* unbound slot — the
    // body's actual frame size after the prepend is still 2.
    let partial = ZoneClosure {
        body: Arc::clone(&base_zc.body),
        captures: Arc::clone(&base_zc.captures),
        zone_output_wires: Arc::clone(&base_zc.zone_output_wires),
        owner_node_id: base_zc.owner_node_id,
        param_types: vec![DataType::Int],
        return_type: base_zc.return_type.clone(),
        pre_supplied_args: Arc::new(vec![NetworkResult::Int(10)]),
    };

    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let result = run_closure_once(
        &evaluator,
        &[],
        &designer.node_type_registry,
        &mut context,
        &partial,
        vec![NetworkResult::Int(5)],
    );

    assert_eq!(
        phase2_extract_int(result),
        15,
        "pre_supplied_args [10] must prepend before caller args [5] so the body sees a=10, b=5"
    );
}

/// Pre-supplied args must preserve the order in which they were bound (oldest
/// first), so chained partial applications compose correctly: with both slots
/// bound via `pre_supplied_args = [10, 5]` and an empty caller frame
/// (`param_types = []`), the body still resolves to `Int(15)`.
#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn run_closure_once_with_fully_bound_pre_supplied_args_runs_zero_arg_body() {
    let (designer, base_zc) = build_two_int_param_add_closure();

    let fully_bound = ZoneClosure {
        body: Arc::clone(&base_zc.body),
        captures: Arc::clone(&base_zc.captures),
        zone_output_wires: Arc::clone(&base_zc.zone_output_wires),
        owner_node_id: base_zc.owner_node_id,
        param_types: vec![],
        return_type: base_zc.return_type.clone(),
        pre_supplied_args: Arc::new(vec![NetworkResult::Int(10), NetworkResult::Int(5)]),
    };

    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let result = run_closure_once(
        &evaluator,
        &[],
        &designer.node_type_registry,
        &mut context,
        &fully_bound,
        vec![],
    );

    assert_eq!(phase2_extract_int(result), 15);
}

/// Cloning a `ZoneClosure` whose `pre_supplied_args` is non-empty must share
/// the underlying `Vec` (Walker `Clone` independence / Invariant 2): the body
/// / captures / wires / pre-supplied args are all `Arc`-backed, so a clone is
/// refcount bumps only. Verified by `Arc::ptr_eq` on the clone's vector.
#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn zone_closure_clone_shares_pre_supplied_args_arc() {
    let (_designer, base_zc) = build_two_int_param_add_closure();
    let with_partial = ZoneClosure {
        body: Arc::clone(&base_zc.body),
        captures: Arc::clone(&base_zc.captures),
        zone_output_wires: Arc::clone(&base_zc.zone_output_wires),
        owner_node_id: base_zc.owner_node_id,
        param_types: vec![DataType::Int],
        return_type: base_zc.return_type.clone(),
        pre_supplied_args: Arc::new(vec![NetworkResult::Int(10)]),
    };
    let cloned = with_partial.clone();
    assert!(Arc::ptr_eq(
        &with_partial.pre_supplied_args,
        &cloned.pre_supplied_args
    ));
}

// =============================================================================
// Phase 3 — `apply` partial application + recursive consumption
// =============================================================================
//
// Helpers mirror the Phase 2 pattern: build a Custom-kind closure with N int
// params, wire it into an `apply`, optionally leave a prefix of arg pins
// unwired to exercise partial application. See `doc/design_currying.md`
// §"`apply` semantics".

use rust_lib_flutter_cad::structure_designer::nodes::apply::ApplyData;

/// Evaluate a node and return the result against the active registry.
fn phase3_evaluate_node(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

/// Add an `int` constant.
fn phase3_add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
    let id = designer.add_node("int", DVec2::new(0.0, y));
    phase2_set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

/// Build an N-int-param `Custom` closure on the "main" network whose body
/// computes the given expression over the params. Returns the closure node id.
fn phase3_add_custom_int_closure(
    designer: &mut StructureDesigner,
    network: &str,
    param_names: &[&str],
    expression: &str,
    y: f64,
) -> u64 {
    let mut type_args: Vec<DataType> = vec![DataType::Int; param_names.len()];
    type_args.push(DataType::Int); // return type
    let closure_id = designer.add_node("closure", DVec2::new(0.0, y));
    phase2_set_node_data(
        designer,
        network,
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args,
            param_names: param_names.iter().map(|s| s.to_string()).collect(),
            custom_label: None,
        }),
    );
    let expr_id = phase2_add_expr_to_body(designer, network, closure_id, expression, param_names);
    for (i, _) in param_names.iter().enumerate() {
        phase2_wire_zone_input_to_body_node(designer, network, closure_id, i, expr_id, i);
    }
    phase2_wire_body_node_to_zone_output(designer, network, closure_id, expr_id);
    closure_id
}

/// Add an `apply` node with default ApplyData (irrelevant when `f` is wired
/// — the repair post-pass overrides the layout from the wired source's flat
/// function type).
fn phase3_add_apply(designer: &mut StructureDesigner, network: &str, y: f64) -> u64 {
    let apply_id = designer.add_node("apply", DVec2::new(0.0, y));
    // Default ApplyData is map-like `(Float) -> Float`; overridden after we
    // wire f. We don't need to pre-shape it.
    phase2_set_node_data(designer, network, apply_id, Box::new(ApplyData::default()));
    apply_id
}

fn phase3_extract_int(result: NetworkResult) -> i32 {
    match result {
        NetworkResult::Int(v) => v,
        NetworkResult::Error(msg) => panic!("expected Int, got Error: {msg}"),
        other => panic!("expected Int, got {}", other.to_display_string()),
    }
}

// ----------------------------------------------------------------------------
// Test 1: full apply unchanged — N == k == n_body, single loop iteration.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_full_eval_three_args() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);
    let a = phase3_add_int(&mut designer, "main", 2, -100.0);
    let b = phase3_add_int(&mut designer, "main", 3, -50.0);
    let c = phase3_add_int(&mut designer, "main", 4, 0.0);

    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g, 0, app, 0); // f — triggers post-pass shape derivation
    designer.connect_nodes(a, 0, app, 1);
    designer.connect_nodes(b, 0, app, 2);
    designer.connect_nodes(c, 0, app, 3);

    let result = phase3_evaluate_node(&designer, "main", app);
    assert_eq!(phase3_extract_int(result), 9);
}

// ----------------------------------------------------------------------------
// Test 2: one-arg partial yields a 2-arg function; chain via another `apply`.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_partial_then_full_via_second_apply() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);
    let a = phase3_add_int(&mut designer, "main", 10, -100.0);
    let b = phase3_add_int(&mut designer, "main", 20, -50.0);
    let c = phase3_add_int(&mut designer, "main", 30, 0.0);

    // First apply: k=1, leaves a 2-arg function.
    let app1 = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g, 0, app1, 0);
    designer.connect_nodes(a, 0, app1, 1);
    // arg1, arg2 left unwired

    // Second apply: consume the remaining 2-arg function with both args.
    let app2 = phase3_add_apply(&mut designer, "main", 100.0);
    designer.connect_nodes(app1, 0, app2, 0); // f = partial
    designer.connect_nodes(b, 0, app2, 1);
    designer.connect_nodes(c, 0, app2, 2);

    let result = phase3_evaluate_node(&designer, "main", app2);
    assert_eq!(phase3_extract_int(result), 60);
}

// ----------------------------------------------------------------------------
// Test 3: identity partial — k=0 with a non-thunk f returns f unchanged.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_identity_partial_returns_f_unchanged() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g = phase3_add_custom_int_closure(&mut designer, "main", &["a", "b"], "a * b", -100.0);
    let app = phase3_add_apply(&mut designer, "main", 0.0);
    designer.connect_nodes(g, 0, app, 0); // f only, no arg pins wired

    let result = phase3_evaluate_node(&designer, "main", app);
    // Identity partial: apply returns the wired closure as a Function value.
    match result {
        NetworkResult::Function(zc) => {
            // The closure's declared canonical flat type stays `(Int, Int) -> Int`.
            let ft = zc.function_type();
            assert_eq!(ft.parameter_types, vec![DataType::Int, DataType::Int]);
            assert_eq!(*ft.output_type, DataType::Int);
            // Identity partial: no args were bound, so pre_supplied_args is empty.
            assert!(zc.pre_supplied_args.is_empty());
        }
        other => panic!(
            "expected Function (identity partial), got {}",
            other.to_display_string()
        ),
    }
}

// ----------------------------------------------------------------------------
// Test 4: 0-arity thunk — declared params empty, identity guard does not fire,
// loop runs once with n_body=0 and returns the body's value.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_zero_arity_thunk_is_forced() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // A 0-arity Custom closure: param_names = [], type_args = [Int],
    // body is a single `int` constant wired to zone_output.
    let closure_id = designer.add_node("closure", DVec2::new(0.0, -100.0));
    phase2_set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int],
            param_names: vec![],
            custom_label: None,
        }),
    );
    // Add an int constant into the closure's body and wire to zone_output.
    {
        use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
        let registry = &mut designer.node_type_registry;
        let body = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&closure_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let int_id = body.add_node("int", DVec2::ZERO, 0, Box::new(IntData { value: 42 }));
        if body
            .nodes
            .get(&closure_id)
            .and_then(|n| n.zone_output_arguments.first())
            .is_none()
        {
            // No-op; the population step ensured zone_output_arguments has one
            // entry per declared zone-output pin.
        }
        let owner = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&closure_id)
            .unwrap();
        if owner.zone_output_arguments.is_empty() {
            owner.zone_output_arguments.push(Argument::new());
        }
        owner.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: int_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(closure_id, 0, app, 0); // f only — no arg pins on apply

    let result = phase3_evaluate_node(&designer, "main", app);
    assert_eq!(
        phase3_extract_int(result),
        42,
        "0-arity thunk must be forced: declared arity 0 ⇒ identity guard does not fire ⇒ body runs"
    );
}

// ----------------------------------------------------------------------------
// Test 5: prefix-only validation — arg0 unwired, arg1 wired is rejected.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_prefix_only_validation_rejects_gap() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);
    let b = phase3_add_int(&mut designer, "main", 5, -50.0);

    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g, 0, app, 0); // f wired ⇒ apply shape derived
    // Skip arg0; wire arg1 only — prefix-only rule should fire.
    designer.connect_nodes(b, 0, app, 2);

    let valid = designer
        .validate_active_network()
        .map(|r| r.valid)
        .unwrap_or(false);
    assert!(
        !valid,
        "apply with non-prefix wiring (arg0 unwired, arg1 wired) must be invalid"
    );

    let errors: Vec<String> = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    assert!(
        errors
            .iter()
            .any(|s| s.contains("contiguous prefix") || s.contains("prefix")),
        "expected a prefix-only error message; got: {:?}",
        errors
    );
}

// ----------------------------------------------------------------------------
// Test 6: currying-equivalent acceptance — Phase 1 canonicalization means a
// closure authored as `A -> ((B, C) -> D)` is stored as `(A, B, C) -> D`,
// so an apply with three arg pins accepts it directly.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_canonical_flat_arity_drives_arg_pins() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Closure with stored type_args[-1] = Function((B, C), D), which Phase 1
    // canonicalizes so the closure's declared output type is the flat
    // (A, B, C) -> D. We use Ints throughout for body computability — the
    // canonicalization itself is type-agnostic.
    let g =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);
    // Verify the closure's declared output is canonical 3-arg.
    let nt = {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let node = net.nodes.get(&g).unwrap();
        designer
            .node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .clone()
    };
    let out_ty = nt.output_type().clone();
    let DataType::Function(ft) = out_ty else {
        panic!("expected closure output type to be Function(_)");
    };
    assert_eq!(
        ft.parameter_types.len(),
        3,
        "Custom (a, b, c) -> Int closure must declare a 3-parameter flat function type"
    );
    assert_eq!(*ft.output_type, DataType::Int);

    // Wire into apply: post-pass derives 3 arg pins from the source's flat type.
    let a = phase3_add_int(&mut designer, "main", 1, -100.0);
    let b = phase3_add_int(&mut designer, "main", 2, -50.0);
    let c = phase3_add_int(&mut designer, "main", 3, 0.0);
    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g, 0, app, 0);
    designer.connect_nodes(a, 0, app, 1);
    designer.connect_nodes(b, 0, app, 2);
    designer.connect_nodes(c, 0, app, 3);

    // After connect_nodes (which revalidates because dest_is_function_pin and/or
    // the f wire), apply's custom_node_type should have exactly 4 parameters
    // (f + 3 arg pins).
    let apply_param_count = {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let node = net.nodes.get(&app).unwrap();
        designer
            .node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .parameters
            .len()
    };
    assert_eq!(
        apply_param_count, 4,
        "apply wired to a canonical 3-arg Function source must expose f + 3 arg pins"
    );

    let result = phase3_evaluate_node(&designer, "main", app);
    assert_eq!(phase3_extract_int(result), 6);
}

// ----------------------------------------------------------------------------
// Test 6b: rewiring apply.f to a LOWER-arity source shrinks the arg pins and
// drops the now-orphaned arg wires (while preserving the surviving ones).
//
// This guards the load/validate-path change that switched apply's post-pass to
// the *preserving-args* variant and reordered it ahead of
// `repair_network_arguments`. Preserving keeps arguments positionally; the
// arity *shrink* therefore relies on `repair_network_arguments` truncating the
// tail. The surviving `arg0` wire must stay put; the orphaned `arg1`/`arg2`
// wires (to the old 3-arg call) must be gone.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_rewire_f_to_lower_arity_shrinks_arg_pins() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g3 =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -250.0);
    let g1 = phase3_add_custom_int_closure(&mut designer, "main", &["a"], "a + 100", -200.0);
    let a = phase3_add_int(&mut designer, "main", 2, -100.0);
    let b = phase3_add_int(&mut designer, "main", 3, -50.0);
    let c = phase3_add_int(&mut designer, "main", 4, 0.0);

    // Start fully wired against the 3-arg source: f + arg0..arg2.
    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g3, 0, app, 0);
    designer.connect_nodes(a, 0, app, 1);
    designer.connect_nodes(b, 0, app, 2);
    designer.connect_nodes(c, 0, app, 3);

    let param_count = |designer: &StructureDesigner| {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let node = net.nodes.get(&app).unwrap();
        designer
            .node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .parameters
            .len()
    };
    assert_eq!(param_count(&designer), 4, "f + 3 arg pins when wired to g3");
    assert_eq!(
        phase3_extract_int(phase3_evaluate_node(&designer, "main", app)),
        9
    );

    // Rewire f to the 1-arg source. `f` is a single-connection pin, so this
    // replaces the g3 wire; the connect revalidates and reshapes the node.
    designer.connect_nodes(g1, 0, app, 0);

    // The arg-pin layout must shrink to f + arg0.
    assert_eq!(
        param_count(&designer),
        2,
        "rewiring f to a 1-arg source must shrink apply to f + arg0"
    );

    // Inspect the surviving wires: f -> g1, arg0 -> a, and NO wires to b or c.
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let node = net.nodes.get(&app).unwrap();
        assert_eq!(
            node.arguments.len(),
            2,
            "stale arg1/arg2 slots must be truncated, not left dangling"
        );
        // f pin -> g1
        let f_src: Vec<u64> = node.arguments[0]
            .incoming_wires
            .iter()
            .map(|w| w.source_node_id)
            .collect();
        assert_eq!(f_src, vec![g1], "f must now point at the 1-arg source");
        // arg0 -> a survived
        let arg0_src: Vec<u64> = node.arguments[1]
            .incoming_wires
            .iter()
            .map(|w| w.source_node_id)
            .collect();
        assert_eq!(
            arg0_src,
            vec![a],
            "the surviving arg0 wire (to `a`) must be kept"
        );
        // b and c must not appear anywhere in apply's arguments.
        let all_sources: Vec<u64> = node
            .arguments
            .iter()
            .flat_map(|arg| arg.incoming_wires.iter().map(|w| w.source_node_id))
            .collect();
        assert!(
            !all_sources.contains(&b) && !all_sources.contains(&c),
            "orphaned arg1/arg2 wires (to b, c) must be dropped; got sources {:?}",
            all_sources
        );
    }

    // Full eval against the 1-arg function: g1(a) = a + 100 = 102.
    assert_eq!(
        phase3_extract_int(phase3_evaluate_node(&designer, "main", app)),
        102
    );
}

// ----------------------------------------------------------------------------
// Test 7: apply's output pin retypes to Function on partial wiring.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase3_output_pin_retypes_on_partial_wiring() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);
    let a = phase3_add_int(&mut designer, "main", 1, -100.0);

    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g, 0, app, 0);
    designer.connect_nodes(a, 0, app, 1);
    // arg1, arg2 left unwired ⇒ k=1, output type should be Function((Int, Int), Int)

    let out_ty = {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let node = net.nodes.get(&app).unwrap();
        designer
            .node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .output_type()
            .clone()
    };
    match out_ty {
        DataType::Function(ft) => {
            assert_eq!(
                ft.parameter_types,
                vec![DataType::Int, DataType::Int],
                "partial apply with k=1/N=3 must expose a 2-arg remaining function on its output"
            );
            assert_eq!(*ft.output_type, DataType::Int);
        }
        other => panic!("expected Function output type, got {:?}", other),
    }
}

// =============================================================================
// Phase 4 — HOF auto-partialization on `map`
// =============================================================================
//
// When `map.f` is wired with a `Function` source whose parameter list starts
// with `[element_type]`, the excess parameters become a partial-application
// tail. `map.output_type` is derived from `f`: empty tail ⇒ `R`, non-empty
// ⇒ `Function(tail, R)`. The post-pass in `update_map_pin_layouts_for_network`
// overrides the map node's `custom_node_type` so the standard structural wire
// check passes against the source's exact function type; the connect-time
// gate uses a `starts_with([element_type])` rule to admit the first wire.
// See `doc/design_currying.md` §"HOF auto-partialization (`map`)".

use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;

/// Add a `map` node with the given input/output type stored in `MapData`.
/// The element type drives `xs`'s `Iter[T]` and the f-pin's
/// `Function([T], output_type)`. `output_type` is the user-set fallback used
/// when `f` is disconnected.
fn phase4_add_map(
    designer: &mut StructureDesigner,
    network: &str,
    input_type: DataType,
    output_type: DataType,
    y: f64,
) -> u64 {
    let id = designer.add_node("map", DVec2::new(0.0, y));
    phase2_set_node_data(
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

/// Add a `range(start, step, count)` source yielding `Iter[Int]`.
fn phase4_add_range(
    designer: &mut StructureDesigner,
    network: &str,
    start: i32,
    step: i32,
    count: i32,
    y: f64,
) -> u64 {
    let id = designer.add_node("range", DVec2::new(0.0, y));
    phase2_set_node_data(
        designer,
        network,
        id,
        Box::new(RangeData { start, step, count }),
    );
    id
}

/// Evaluate `map_node_id` against the active "main" network and drain the
/// resulting `Iterator(Walker)` into a `Vec<NetworkResult>`. Panics if the
/// node didn't emit an iterator (i.e. emitted an error or non-Iterator value).
fn phase4_drain_map_walker(
    designer: &StructureDesigner,
    network_name: &str,
    map_node_id: u64,
) -> Vec<NetworkResult> {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&stack, map_node_id, 0, registry, false, &mut context);

    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        NetworkResult::Error(msg) => panic!("map evaluation failed: {msg}"),
        other => panic!(
            "expected NetworkResult::Iterator from map, got {}",
            other.to_display_string()
        ),
    };
    let mut out = Vec::new();
    while let Some(item) = walker.next(&evaluator, registry, &mut context) {
        out.push(item);
    }
    out
}

/// Returns the *resolved* output type of `node_id`'s pin 0 (output) against
/// the active network — what downstream consumers see, including the
/// derivation our post-pass performs for `map`.
fn phase4_output_type(designer: &StructureDesigner, network_name: &str, node_id: u64) -> DataType {
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

// ----------------------------------------------------------------------------
// Test 1 (headline scenario): a `(Int, Int) -> Int` source flows into map.f
// over `Iter[Int]`. The map's output type derives to `Iter[Function((Int,), Int)]`.
// ----------------------------------------------------------------------------

#[test]
fn map_phase4_higher_arity_source_derives_output_type() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g = phase3_add_custom_int_closure(&mut designer, "main", &["x", "y"], "x * y", -200.0);
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    // Wire closure to map.f. Connect must succeed via the starts-with rule:
    // declared map.f is `AnyFunction { leading_params: [Int] }` (Function-pin
    // Unification Phase C); the source is `Function([Int, Int], Int)`, which
    // satisfies the leading-prefix constraint via the standard
    // `Function → AnyFunction` compatibility rule (Phase A).
    assert!(
        designer.can_connect_nodes(g, 0, m, 1),
        "starts-with rule must admit a (Int, Int) -> Int source into map.f over Iter[Int]"
    );
    designer.connect_nodes(g, 0, m, 1);

    let out_ty = phase4_output_type(&designer, "main", m);
    let DataType::Iterator(inner) = out_ty else {
        panic!("expected map output to be Iter[_], got {:?}", out_ty);
    };
    let DataType::Function(ft) = *inner else {
        panic!(
            "expected map output element to be a partial Function((Int,), Int), got non-Function"
        );
    };
    assert_eq!(
        ft.parameter_types,
        vec![DataType::Int],
        "tail after consuming the leading element_type should be a single Int param"
    );
    assert_eq!(*ft.output_type, DataType::Int);
}

// ----------------------------------------------------------------------------
// Test 2 (headline eval): each pulled element is a partially-applied closure
// carrying the iteration value in `pre_supplied_args[0]`.
// ----------------------------------------------------------------------------

#[test]
fn map_phase4_partial_application_per_element() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g = phase3_add_custom_int_closure(&mut designer, "main", &["x", "y"], "x * y", -200.0);
    let r = phase4_add_range(&mut designer, "main", 10, 1, 3, -100.0);
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(r, 0, m, 0); // xs
    designer.connect_nodes(g, 0, m, 1); // f

    let items = phase4_drain_map_walker(&designer, "main", m);
    assert_eq!(items.len(), 3, "range(10, 1, 3) yields three elements");

    for (i, item) in items.into_iter().enumerate() {
        let expected_x = 10 + i as i32;
        let zc = match item {
            NetworkResult::Function(zc) => zc,
            other => panic!(
                "expected each map element to be a partial Function value, got {}",
                other.to_display_string()
            ),
        };
        // The first slot was consumed by the iteration element; the partial
        // closure declares the remaining one Int slot (y) as still callable.
        // The body's actual frame stays size-2 because pre_supplied_args
        // prepends the bound element when the partial is forced.
        assert_eq!(zc.param_types, vec![DataType::Int]);
        assert_eq!(zc.return_type, DataType::Int);
        // The iteration element is bound into pre_supplied_args[0].
        assert_eq!(zc.pre_supplied_args.len(), 1);
        let bound = match &zc.pre_supplied_args[0] {
            NetworkResult::Int(n) => *n,
            other => panic!(
                "expected pre_supplied_args[0] to be Int (the iteration element), got {}",
                other.to_display_string()
            ),
        };
        assert_eq!(
            bound, expected_x,
            "iteration element {} must travel via pre_supplied_args[0]",
            expected_x
        );

        // Force the partial by applying the remaining Int param (y = 10) via
        // run_closure_once. Result is x * 10.
        let evaluator = NetworkEvaluator::new();
        let mut context = NetworkEvaluationContext::new();
        let result = run_closure_once(
            &evaluator,
            &[],
            &designer.node_type_registry,
            &mut context,
            &zc,
            vec![NetworkResult::Int(10)],
        );
        assert_eq!(phase3_extract_int(result), expected_x * 10);
    }
}

// ----------------------------------------------------------------------------
// Test 3 (exact arity): a `(Int) -> Int` source flows in normally; output is
// `Iter[Int]` (no partial-application tail).
// ----------------------------------------------------------------------------

#[test]
fn map_phase4_exact_arity_source_output_is_element_type() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g = phase3_add_custom_int_closure(&mut designer, "main", &["x"], "x * x", -200.0);
    let r = phase4_add_range(&mut designer, "main", 1, 1, 4, -100.0);
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(r, 0, m, 0);
    designer.connect_nodes(g, 0, m, 1);

    let out_ty = phase4_output_type(&designer, "main", m);
    assert_eq!(
        out_ty,
        DataType::Iterator(Box::new(DataType::Int)),
        "exact-arity (Int) -> Int source should leave map output as Iter[Int] (no partial tail)"
    );

    let items = phase4_drain_map_walker(&designer, "main", m);
    let values: Vec<i32> = items
        .into_iter()
        .map(|r| match r {
            NetworkResult::Int(n) => n,
            other => panic!("expected Int, got {}", other.to_display_string()),
        })
        .collect();
    assert_eq!(values, vec![1, 4, 9, 16]);
}

// ----------------------------------------------------------------------------
// Test 4 (mismatch reject): a source whose first param doesn't match the
// element_type is rejected at connect time.
// ----------------------------------------------------------------------------

#[test]
fn map_phase4_first_param_mismatch_rejected_at_connect_time() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Build a 2-arg closure with first param Bool — doesn't start with [Int],
    // so the starts-with rule must reject it for a map over Iter[Int].
    let closure_id = designer.add_node("closure", DVec2::new(0.0, -100.0));
    phase2_set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Bool, DataType::Int, DataType::Int],
            param_names: vec!["b".into(), "x".into()],
            custom_label: None,
        }),
    );
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    assert!(
        !designer.can_connect_nodes(closure_id, 0, m, 1),
        "a (Bool, Int) -> Int source must be rejected for map.f over Iter[Int] \
         (starts-with rule fails — first param is Bool, not Int)"
    );
}

// ----------------------------------------------------------------------------
// Test 5 (f disconnect restores stored output_type): a starts-with override
// installed by a previous wire must be reverted when `f` is disconnected.
// ----------------------------------------------------------------------------

#[test]
fn map_phase4_f_disconnect_restores_stored_output_type() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Stored output_type = Bool. Connecting a (Int, Int) -> Int source should
    // override it to Iter[Function((Int,), Int)]; disconnect must restore
    // Iter[Bool] (today's behavior — the user-configured stored value).
    let g = phase3_add_custom_int_closure(&mut designer, "main", &["x", "y"], "x * y", -200.0);
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Bool, 0.0);
    designer.connect_nodes(g, 0, m, 1);

    {
        let out_ty = phase4_output_type(&designer, "main", m);
        let DataType::Iterator(inner) = out_ty.clone() else {
            panic!("expected Iter[_] after wire, got {:?}", out_ty);
        };
        assert!(
            matches!(*inner, DataType::Function(_)),
            "after wiring, map output should be Iter[Function(...)], got Iter[{:?}]",
            *inner
        );
    }

    // Disconnect by selecting and deleting the f wire.
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        net.selected_wires.clear();
        net.selected_wires.push(
            rust_lib_flutter_cad::structure_designer::node_network::Wire {
                source_node_id: g,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
                destination_node_id: m,
                destination_argument_index: 1,
                destination_argument_kind:
                    rust_lib_flutter_cad::structure_designer::node_network::ArgumentKind::External,
            },
        );
    }
    designer.delete_selected();

    let out_ty = phase4_output_type(&designer, "main", m);
    assert_eq!(
        out_ty,
        DataType::Iterator(Box::new(DataType::Bool)),
        "after disconnecting f, map output_type must fall back to MapData's stored Bool"
    );
}

// ----------------------------------------------------------------------------
// Test 6 (filter exact-arity unchanged): the auto-partial rule is only on
// `map`. `filter.f` still requires exact-arity `(T) -> Bool`.
// ----------------------------------------------------------------------------

#[test]
fn map_phase4_filter_exact_arity_unchanged() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // 2-arg closure (Int, Int) -> Bool — does not match filter's `(Int) -> Bool`
    // shape under same-arity rules. Filter is exact-arity, so this must be
    // rejected even though the first param happens to match the element_type.
    let closure_id = designer.add_node("closure", DVec2::new(0.0, -100.0));
    phase2_set_node_data(
        &mut designer,
        "main",
        closure_id,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int, DataType::Bool],
            param_names: vec!["x".into(), "y".into()],
            custom_label: None,
        }),
    );

    let filt_id = designer.add_node("filter", DVec2::new(0.0, 0.0));
    phase2_set_node_data(
        &mut designer,
        "main",
        filt_id,
        Box::new(FilterData {
            element_type: DataType::Int,
        }),
    );

    assert!(
        !designer.can_connect_nodes(closure_id, 0, filt_id, 1),
        "filter.f must keep exact-arity matching — a (Int, Int) -> Bool source must not satisfy a (Int) -> Bool pin"
    );
}

// =============================================================================
// Function-pin unification Phase B — `apply.f` declared type is permanently
// `AnyFunction { leading_params: vec![] }`, regardless of wiring state. The
// post-pass no longer rewrites it; the standard
// `Function(_) → AnyFunction { vec![] }` compatibility rule (Phase A) makes
// the f wire type-check on its own. See
// `doc/design_function_pin_unification.md` (Phase B).
// =============================================================================

/// Helper: read the declared type of `apply`'s f pin (parameter index 0) from
/// its current custom_node_type.
fn apply_f_pin_type(designer: &StructureDesigner, network_name: &str, apply_id: u64) -> DataType {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = net.nodes.get(&apply_id).unwrap();
    designer
        .node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters[0]
        .data_type
        .clone()
}

// ----------------------------------------------------------------------------
// Test B1: f pin starts as AnyFunction { vec![] } on a freshly-added apply.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase_b_f_pin_is_any_function_when_unwired() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let app = phase3_add_apply(&mut designer, "main", 0.0);

    assert_eq!(
        apply_f_pin_type(&designer, "main", app),
        DataType::AnyFunction {
            leading_params: vec![]
        },
        "freshly added apply must expose f as AnyFunction {{ vec![] }} (no leading-param constraint)"
    );
}

// ----------------------------------------------------------------------------
// Test B2: f pin stays AnyFunction { vec![] } after a 3-arg closure is wired
// in — the post-pass installs arg-pin layout + output type but leaves the
// f-pin's declared type unchanged.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase_b_f_pin_stays_any_function_after_wiring() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);
    let app = phase3_add_apply(&mut designer, "main", 50.0);
    designer.connect_nodes(g, 0, app, 0);

    assert_eq!(
        apply_f_pin_type(&designer, "main", app),
        DataType::AnyFunction {
            leading_params: vec![]
        },
        "apply.f must remain AnyFunction {{ vec![] }} after wiring — the post-pass derives arg-pin layout but does not rewrite f"
    );

    // Sanity check: the post-pass still installed the f + 3 arg-pin layout.
    let param_count = {
        let net = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap();
        let node = net.nodes.get(&app).unwrap();
        designer
            .node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .parameters
            .len()
    };
    assert_eq!(
        param_count, 4,
        "apply wired to a 3-arg closure must expose f + 3 arg pins"
    );
}

// ----------------------------------------------------------------------------
// Test B3: `can_connect_nodes` accepts a closure source into apply.f via the
// standard `Function(_) → AnyFunction { vec![] }` rule — no name-matched
// exception is required. Exercised with a 1- and 3-arg closure.
// ----------------------------------------------------------------------------

#[test]
fn apply_phase_b_any_function_rule_accepts_arbitrary_arity_sources() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Closures of different arities all flow into apply.f via the AnyFunction
    // unconstrained rule. Without the Phase A rule + Phase B declared-type
    // change, the connect check would require exact match against the
    // default `(Float) -> Float`.
    let g1 = phase3_add_custom_int_closure(&mut designer, "main", &["a"], "a + 1", -300.0);
    let g3 =
        phase3_add_custom_int_closure(&mut designer, "main", &["a", "b", "c"], "a + b + c", -200.0);

    let app = phase3_add_apply(&mut designer, "main", 50.0);

    assert!(
        designer.can_connect_nodes(g1, 0, app, 0),
        "1-arg closure must connect to apply.f via the AnyFunction rule"
    );
    assert!(
        designer.can_connect_nodes(g3, 0, app, 0),
        "3-arg closure must connect to apply.f via the AnyFunction rule"
    );
}

// =============================================================================
// Function-pin unification Phase C — `map.f` declared type is permanently
// `AnyFunction { leading_params: vec![element_type] }`, regardless of wiring
// state. The post-pass no longer rewrites the f-pin type; the
// `Function(_) → AnyFunction { [element_type] }` compatibility rule (Phase A)
// makes the f wire type-check on its own, including for higher-arity sources
// that participate in HOF auto-partialization. The name-matched starts-with
// exception in `can_connect_nodes` is removed. See
// `doc/design_function_pin_unification.md` (Phase C).
// =============================================================================

/// Helper: read the declared type of `map`'s f pin (parameter index 1) from
/// its current custom_node_type.
fn map_f_pin_type(designer: &StructureDesigner, network_name: &str, map_id: u64) -> DataType {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = net.nodes.get(&map_id).unwrap();
    designer
        .node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters[1]
        .data_type
        .clone()
}

// ----------------------------------------------------------------------------
// Test C1: map.f is AnyFunction { vec![element_type] } on a freshly-added map
// for various MapData configurations.
// ----------------------------------------------------------------------------

#[test]
fn map_phase_c_f_pin_is_any_function_when_unwired() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let m_int = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    assert_eq!(
        map_f_pin_type(&designer, "main", m_int),
        DataType::AnyFunction {
            leading_params: vec![DataType::Int],
        },
        "Int-input map.f must declare AnyFunction {{ leading_params: [Int] }}"
    );

    let m_float = phase4_add_map(
        &mut designer,
        "main",
        DataType::Float,
        DataType::Bool,
        100.0,
    );
    assert_eq!(
        map_f_pin_type(&designer, "main", m_float),
        DataType::AnyFunction {
            leading_params: vec![DataType::Float],
        },
        "Float-input map.f must declare AnyFunction {{ leading_params: [Float] }}"
    );
}

// ----------------------------------------------------------------------------
// Test C2: map.f stays AnyFunction { vec![element_type] } after a higher-arity
// closure is wired in — the post-pass derives the output pin type but leaves
// the f-pin's declared type unchanged.
// ----------------------------------------------------------------------------

#[test]
fn map_phase_c_f_pin_stays_any_function_after_wiring() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let g = phase3_add_custom_int_closure(&mut designer, "main", &["x", "y"], "x * y", -200.0);
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    designer.connect_nodes(g, 0, m, 1);

    assert_eq!(
        map_f_pin_type(&designer, "main", m),
        DataType::AnyFunction {
            leading_params: vec![DataType::Int],
        },
        "map.f must remain AnyFunction {{ leading_params: [Int] }} after wiring — \
         the post-pass derives output type but does not rewrite f"
    );

    // Sanity check: the post-pass still derived the partial-application output.
    let out_ty = phase4_output_type(&designer, "main", m);
    let DataType::Iterator(inner) = out_ty else {
        panic!("expected Iter[_] output after wiring");
    };
    assert!(
        matches!(*inner, DataType::Function(_)),
        "output should derive to Iter[Function((Int,), Int)] for a 2-arg source"
    );
}

// ----------------------------------------------------------------------------
// Test C3: map.f declared type tracks input_type changes via
// `calculate_custom_node_type`. Changing MapData.input_type rederives the
// leading_params entry.
// ----------------------------------------------------------------------------

#[test]
fn map_phase_c_f_pin_tracks_input_type() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Start as Int → Int.
    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);
    assert_eq!(
        map_f_pin_type(&designer, "main", m),
        DataType::AnyFunction {
            leading_params: vec![DataType::Int],
        },
    );

    // Flip to Float → Float by replacing MapData.
    phase2_set_node_data(
        &mut designer,
        "main",
        m,
        Box::new(MapData {
            input_type: DataType::Float,
            output_type: DataType::Float,
        }),
    );

    assert_eq!(
        map_f_pin_type(&designer, "main", m),
        DataType::AnyFunction {
            leading_params: vec![DataType::Float],
        },
        "after flipping MapData.input_type to Float, map.f's AnyFunction \
         leading_params must rederive to [Float]"
    );
}

// ----------------------------------------------------------------------------
// Test C4: the name-matched starts-with exception is gone — `can_connect_nodes`
// now routes through the standard AnyFunction compatibility rule for both
// exact-arity and higher-arity sources.
// ----------------------------------------------------------------------------

#[test]
fn map_phase_c_any_function_rule_accepts_higher_arity_sources() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // 1-arg (Int) -> Int and 3-arg (Int, Int, Int) -> Int closures both
    // satisfy the AnyFunction { leading_params: [Int] } constraint on
    // map.f over Iter[Int].
    let g1 = phase3_add_custom_int_closure(&mut designer, "main", &["x"], "x * x", -300.0);
    let g3 =
        phase3_add_custom_int_closure(&mut designer, "main", &["x", "y", "z"], "x+y+z", -200.0);

    let m = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    assert!(
        designer.can_connect_nodes(g1, 0, m, 1),
        "1-arg (Int)->Int source must connect to map.f via the AnyFunction rule"
    );
    assert!(
        designer.can_connect_nodes(g3, 0, m, 1),
        "3-arg (Int,Int,Int)->Int source must connect to map.f via the AnyFunction rule"
    );

    // First-param mismatch is still rejected.
    let g_bool = {
        let id = designer.add_node("closure", DVec2::new(0.0, -400.0));
        phase2_set_node_data(
            &mut designer,
            "main",
            id,
            Box::new(ClosureData {
                kind: ClosureKind::Custom,
                type_args: vec![DataType::Bool, DataType::Int, DataType::Int],
                param_names: vec!["b".into(), "x".into()],
                custom_label: None,
            }),
        );
        id
    };
    assert!(
        !designer.can_connect_nodes(g_bool, 0, m, 1),
        "(Bool, Int) -> Int source must still be rejected (leading param Bool ≠ Int)"
    );
}

// =============================================================================
// Apply-pin post-pass recurses into zone bodies.
//
// Regression for the bug where dragging a function-typed wire into the `f`
// pin of an `apply` node that lives *inside* a zone body produced no arg
// pins. `update_apply_pin_layouts_for_network` historically only ran on the
// top-level network, so a body-internal apply's layout stayed collapsed to
// the bare `f` pin. The fix makes the post-pass descend into every HOF zone
// body. See `node_type_registry.rs::update_apply_pin_layouts_for_network`.
// =============================================================================

#[test]
fn apply_pin_layout_post_pass_recurses_into_zone_body() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Outer `map` whose body will host the apply. (Any zone-owning HOF works;
    // the post-pass keys on `Node.zone.is_some()`.)
    let map_id = phase4_add_map(&mut designer, "main", DataType::Int, DataType::Int, 0.0);

    // Add a `closure` source ((Int) -> Int) and an `apply` consumer into the
    // map body, then wire apply.f ← closure (depth-0, same body).
    let (closure_id, apply_id) = {
        let map_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let closure_id = map_body.add_node(
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
        let apply_id = map_body.add_node(
            "apply",
            DVec2::new(200.0, 0.0),
            2,
            Box::new(ApplyData {
                kind: ClosureKind::Map,
                type_args: vec![DataType::Int, DataType::Int],
                param_names: vec![],
            }),
        );
        (closure_id, apply_id)
    };

    // Populate caches for the two body nodes (sizes apply's arg slots; with
    // Phase D, apply's ApplyData-driven layout is just the bare `f` pin until
    // the post-pass derives the arg pins from the wired source).
    for nid in [closure_id, apply_id] {
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

    // Sanity: before the post-pass, the body apply is collapsed to the bare
    // `f` pin (1 parameter).
    let params_before = body_apply_param_count(&designer, map_id, apply_id);
    assert_eq!(
        params_before, 1,
        "before the post-pass, the body apply should carry only its `f` pin"
    );

    // Wire apply.f ← closure inside the body.
    {
        let map_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        map_body.nodes.get_mut(&apply_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: closure_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Run the post-pass on the TOP network only. The fix makes it recurse
    // into the map body and derive the body apply's arg-pin layout from the
    // wired (Int) -> Int source.
    {
        let mut main = designer
            .node_type_registry
            .node_networks
            .remove("main")
            .unwrap();
        designer
            .node_type_registry
            .update_apply_pin_layouts_for_network(&mut main);
        designer
            .node_type_registry
            .node_networks
            .insert("main".to_string(), main);
    }

    // After the recursive post-pass, the body apply has its `f` pin plus one
    // derived arg pin (the (Int) -> Int source's single parameter).
    let params_after = body_apply_param_count(&designer, map_id, apply_id);
    assert_eq!(
        params_after, 2,
        "the recursive post-pass must derive the body apply's arg pin from \
         the wired (Int) -> Int source (f pin + 1 arg pin)"
    );
}

/// Read the parameter count of an `apply` node living inside `map_id`'s body.
fn body_apply_param_count(designer: &StructureDesigner, map_id: u64, apply_id: u64) -> usize {
    let map_node = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap();
    let body = map_node.zone.as_ref().unwrap();
    let apply_node = body.nodes.get(&apply_id).unwrap();
    apply_node
        .custom_node_type
        .as_ref()
        .map(|nt| nt.parameters.len())
        .expect("body apply should carry a custom_node_type")
}

// =============================================================================
// Apply-pin post-pass resolves a body apply's `f` fed by a ZONE-INPUT pin.
//
// The follow-up bug: dragging a function-typed wire from a zone-input pin
// (the body's `element` / `acc`) into a body apply's `f` produced no arg pins,
// because the post-pass resolved the f-source only within the body's own frame
// (`source_scope_depth == 0`). The fix threads the ancestor chain and resolves
// the zone-input source against the enclosing HOF's `zone_input_pins`.
// =============================================================================

#[test]
fn apply_pin_layout_resolves_zone_input_function_source() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Outer `map` whose element type is itself a function `(Int) -> Int`, so
    // the body's `element` zone-input pin carries that function value.
    let elem_fn = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Int));
    let map_id = phase4_add_map(&mut designer, "main", elem_fn, DataType::Int, 0.0);

    // Add an `apply` into the map body.
    let apply_id = {
        let map_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        map_body.add_node(
            "apply",
            DVec2::new(200.0, 0.0),
            2,
            Box::new(ApplyData {
                kind: ClosureKind::Map,
                type_args: vec![DataType::Int, DataType::Int],
                param_names: vec![],
            }),
        )
    };

    // Populate apply's cache (collapses to the bare `f` pin under Phase D).
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
            .get_mut(&apply_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Wire apply.f ← the map's `element` zone-input pin (pin_index 0, depth 1).
    {
        let map_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        map_body.nodes.get_mut(&apply_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }

    // Run the post-pass on the TOP network. The threaded ancestor chain lets
    // the body apply resolve its `f` source against the map's zone-input pin
    // type `(Int) -> Int`.
    {
        let mut main = designer
            .node_type_registry
            .node_networks
            .remove("main")
            .unwrap();
        designer
            .node_type_registry
            .update_apply_pin_layouts_for_network(&mut main);
        designer
            .node_type_registry
            .node_networks
            .insert("main".to_string(), main);
    }

    let params_after = body_apply_param_count(&designer, map_id, apply_id);
    assert_eq!(
        params_after, 2,
        "the body apply must derive its arg pin from the zone-input `(Int) -> Int` \
         source (f pin + 1 arg pin)"
    );
}

// =============================================================================
// The cross-scope body apply layout is re-derived at LOAD time.
//
// `.cnnd` files saved by old code never derived a body apply's arg pins from a
// zone-input `f`. This test proves the load path re-derives them: it builds the
// zone-input-fed body apply, saves it to a real `.cnnd`, then loads it through
// the full `StructureDesigner::load_node_networks` path (which runs
// `repair_node_network` per network and then `validate_network` over every
// network in dependency order — the latter being what runs the recursive,
// ancestor-aware post-pass). The reloaded body apply must carry its arg pin.
// =============================================================================

#[test]
fn apply_pin_layout_rederived_on_cnnd_load_with_zone_input_f() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;

    // Build the source design: a `map` over `Iter[(Int) -> Int]` whose body
    // apply takes its `f` from the map's `element` zone-input pin.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let elem_fn = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Int));
    let map_id = phase4_add_map(&mut designer, "main", elem_fn, DataType::Int, 0.0);

    let apply_id = {
        let map_body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let apply_id = map_body.add_node(
            "apply",
            DVec2::new(200.0, 0.0),
            2,
            Box::new(ApplyData {
                kind: ClosureKind::Map,
                type_args: vec![DataType::Int, DataType::Int],
                param_names: vec![],
            }),
        );
        map_body.nodes.get_mut(&apply_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        apply_id
    };

    // Save to a temp `.cnnd`.
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("zone_input_apply.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save .cnnd");

    // Load into a fresh designer through the full load path (repair +
    // validate-all-networks).
    let mut loaded = StructureDesigner::new();
    loaded
        .load_node_networks(path.to_str().unwrap())
        .expect("load .cnnd");

    // The reloaded body apply carries its derived arg pin (f + 1 arg = 2).
    let params = body_apply_param_count(&loaded, map_id, apply_id);
    assert_eq!(
        params, 2,
        "loading a .cnnd must re-derive the body apply's arg pin from the \
         zone-input `(Int) -> Int` source"
    );
}

/// Read the `arguments` (wire-slot) count of an `apply` node living inside
/// `owner_id`'s body. The post-pass derives the *pin layout*
/// (`custom_node_type.parameters`); the matching `arguments` slots are grown
/// separately by `repair_network_arguments`. The two must agree, or connection
/// gating (which indexes `arguments`) rejects wires into the extra pins.
fn body_apply_arg_count(designer: &StructureDesigner, owner_id: u64, apply_id: u64) -> usize {
    let body = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&owner_id)
        .unwrap()
        .zone
        .as_ref()
        .expect("owner HOF should own a body");
    body.nodes.get(&apply_id).unwrap().arguments.len()
}

// =============================================================================
// Regression: https://github.com/atomCAD/atomCAD/issues/331
//
// "apply nodes inside a closure: cannot connect a fitting-type wire to the
// derived arg pins." A body-internal `apply` whose `f` is wired derives its
// arg-pin LAYOUT (`custom_node_type.parameters` → `[f, arg0, …]`) via the
// recursive apply post-pass. On the *interactive* validate path the post-pass
// installs that layout with `refresh_args = false` (positional preservation —
// it must stay non-destructive toward an under-derived `.cnnd` load), so it
// does NOT grow the node's `arguments` vector. Growing `arguments` to match the
// new pin count is the job of `repair_network_arguments` — which, before the
// fix, walked only the TOP-LEVEL `network.nodes` and skipped every zone body
// (the recurring "bare `network.nodes` walk skips body nodes" bug class, see
// `structure_designer/AGENTS.md`).
//
// Net effect inside a body: `parameters.len() == 2` but `arguments.len() == 1`.
// The arg pin renders in the UI (it comes from `parameters`), but
// `NodeNetwork::can_connect_nodes` rejects every wire into it because its
// `dest_param_index >= dest_node.arguments.len()` guard fires. The identical
// `apply` at the TOP level works (top-level `repair_network_arguments` grows
// its args) — which is exactly why the user saw it fail *only* inside a
// closure/zone body.
//
// Reported against a `closure` body; reproduced here with a `closure` outer
// whose `(Int) -> Int` parameter (the "delayed argument") feeds `apply.f`.
// The root cause is body-type-agnostic, so the same fix covers `map` / `filter`
// / `fold` / `foreach` bodies too.
// =============================================================================

#[test]
fn apply_arg_pin_connectable_inside_closure_body() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Outer `closure` with a single parameter `g : (Int) -> Int` — the user's
    // "delayed argument". Its zone-input pin 0 therefore carries a function
    // value that an inner `apply` can call.
    let outer = designer.add_node("closure", DVec2::new(0.0, 0.0));
    let g_type = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Int));
    phase2_set_node_data(
        &mut designer,
        "main",
        outer,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![g_type, DataType::Int],
            param_names: vec!["g".to_string()],
            custom_label: None,
        }),
    );

    // Add an `apply` and an `int` source INSIDE the closure body through the
    // real scoped add path, so `apply` starts with its bare `[f]` layout
    // (`arguments.len() == 1`).
    let apply_id = designer.add_node_scoped(&[outer], "apply", DVec2::new(200.0, 0.0), None);
    let int_id = designer.add_node_scoped(&[outer], "int", DVec2::new(0.0, 120.0), None);

    // Wire apply.f ← the closure's `g` zone-input pin (pin 0, depth 1).
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&outer)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.nodes.get_mut(&apply_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: outer,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }

    // The interactive path: validate the active network. This runs the apply
    // post-pass (preserving-args variant) followed by `repair_network_arguments`.
    designer.validate_active_network();

    // The derived pin layout has `f` + one `arg0` (this already passed before
    // the fix — the layout was correct; only the wire-slot count lagged).
    assert_eq!(
        body_apply_param_count(&designer, outer, apply_id),
        2,
        "body apply must derive [f, arg0] from its (Int) -> Int zone-input source"
    );

    // The core invariant the bug violated: the `arguments` vector must grow to
    // match the derived pin count even inside a zone body.
    assert_eq!(
        body_apply_arg_count(&designer, outer, apply_id),
        2,
        "repair_network_arguments must grow a BODY apply's arguments to match \
         its derived pin count (issue #331)"
    );

    // The user-visible symptom: an `Int` source must be connectable to the
    // derived `arg0` pin (param index 1) inside the body.
    assert!(
        designer.can_connect_nodes_scoped(&[outer], int_id, 0, apply_id, 1),
        "an Int source must connect to the body apply's derived arg0 pin (issue #331)"
    );

    // …and actually performing the scoped connect must land the wire.
    designer.connect_nodes_scoped(&[outer], int_id, 0, apply_id, 1);
    let arg0_wired = {
        let body = designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap()
            .nodes
            .get(&outer)
            .unwrap()
            .zone
            .as_ref()
            .unwrap();
        !body.nodes.get(&apply_id).unwrap().arguments[1]
            .incoming_wires
            .is_empty()
    };
    assert!(
        arg0_wired,
        "the scoped connect must persist a wire into the body apply's arg0"
    );
}
