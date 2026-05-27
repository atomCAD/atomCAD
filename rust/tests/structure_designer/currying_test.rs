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
use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, no_data_loader, no_data_saver};
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
        ("g".to_string(), DataType::Iterator(Box::new(non_canonical_a_then_bc_to_d()))),
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
    let parsed =
        DataType::from_string("(Float, Int) => (Bool -> String)").expect("parse");
    assert_eq!(parsed, canonical_abc_to_d());
}

#[test]
fn parser_produces_canonical_for_zero_arg_function_returning_function() {
    // `() -> (Int -> Float)` should flatten to (Int) -> Float.
    let parsed = DataType::from_string("() -> (Int -> Float)").expect("parse");
    let expected =
        DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Float));
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
        RecordTypeDef {
            name: "Pair".to_string(),
            fields: vec![
                ("f".to_string(), non_canonical_a_then_bc_to_d()),
                ("g".to_string(), DataType::Float),
            ],
        },
    );
    canonicalize_record_type_defs(&mut defs);
    let def = defs.get("Pair").expect("Pair");
    assert_eq!(def.fields[0].1, canonical_abc_to_d());
    assert_eq!(def.fields[1].1, DataType::Float);
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
        let before_params: Vec<DataType> =
            node_type.parameters.iter().map(|p| p.data_type.clone()).collect();
        let before_outputs: Vec<DataType> = node_type
            .output_pins
            .iter()
            .map(|p| {
                p.data_type
                    .fixed_type()
                    .cloned()
                    .unwrap_or(DataType::None)
            })
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
    let body = owner_node.zone_mut().expect("zone-owning node missing zone");

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
    body.nodes
        .get_mut(&body_node_id)
        .unwrap()
        .arguments[body_param_index]
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
