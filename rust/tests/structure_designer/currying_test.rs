//! Currying Phase 1 tests: canonical `FunctionType` storage.
//!
//! Verifies that:
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
//! See `doc/design_currying.md` Phase 1.

use std::collections::HashMap;

use rust_lib_flutter_cad::structure_designer::canonicalize::{
    canonicalize_network, canonicalize_record_type_defs,
};
use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, FunctionType, RecordType, canonicalize_data_type,
};
use rust_lib_flutter_cad::structure_designer::node_data::NoData;
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, CollapseMode, DEFAULT_BODY_HEIGHT, DEFAULT_BODY_WIDTH, Node, NodeNetwork,
};
use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, no_data_loader, no_data_saver};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::closure::ClosureData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;

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
