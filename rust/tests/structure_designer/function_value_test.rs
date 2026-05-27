//! Closures Phase 2 unit coverage: a hand-built
//! `NetworkResult::Function(ZoneClosure)` value round-trips through
//! `infer_data_type` / `convert_to` / `to_display_string`.
//!
//! No node *produces* a `Function` value until Phase 3 (the `closure` node),
//! so this is the only direct exercise of the repurposed `Function` variant's
//! type/display arms (`ZoneClosure::function_type`). See
//! `doc/design_closures.md` (Phase 2).

use std::collections::HashMap;
use std::sync::Arc;

use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::evaluator::zone_closure::ZoneClosure;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

/// Build a `Function` value whose carried metadata describes `(params) -> ret`.
/// The body / captures / zone-output wires are empty — the type and display
/// arms read only `param_types` / `return_type` (via `ZoneClosure::function_type`).
fn make_function_value(param_types: Vec<DataType>, return_type: DataType) -> NetworkResult {
    let mut sd = StructureDesigner::new();
    sd.add_node_network("body");
    let body = sd
        .node_type_registry
        .node_networks
        .get("body")
        .expect("body network exists")
        .clone();
    NetworkResult::Function(ZoneClosure {
        body: Arc::new(body),
        captures: Arc::new(HashMap::new()),
        zone_output_wires: Arc::new(Vec::new()),
        owner_node_id: 0,
        param_types,
        return_type,
        pre_supplied_args: Arc::new(Vec::new()),
    })
}

#[test]
fn function_value_infers_its_function_data_type() {
    let value = make_function_value(vec![DataType::Int], DataType::Int);
    let inferred = value
        .infer_data_type()
        .expect("a Function value infers a DataType::Function");
    assert_eq!(
        inferred,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Int),
        })
    );
}

#[test]
fn function_value_convert_to_is_identity_passthrough() {
    let registry = NodeTypeRegistry::new();
    let value = make_function_value(vec![DataType::Int], DataType::Int);
    let src_ty = value.infer_data_type().unwrap(); // (Int) -> Int

    // A *different* but compatible target function type (return Int -> Float).
    let dst_ty = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Float),
    });
    assert!(DataType::can_be_converted_to(&src_ty, &dst_ty, &registry));

    // Function values carry no runtime payload to convert, so `convert_to`
    // passes the value through unchanged — its metadata still reports the
    // original type.
    let converted = value.convert_to(&src_ty, &dst_ty, &registry);
    assert_eq!(converted.infer_data_type().unwrap(), src_ty);
}

#[test]
fn function_value_display_string_shows_the_function_type() {
    let value = make_function_value(vec![DataType::Int], DataType::Int);
    assert_eq!(value.to_display_string(), "Function Int -> Int");
}

#[test]
fn multi_param_function_value_infers_and_displays() {
    let value = make_function_value(vec![DataType::Float, DataType::Int], DataType::Bool);
    assert_eq!(
        value.infer_data_type().unwrap(),
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Float, DataType::Int],
            output_type: Box::new(DataType::Bool),
        })
    );
    // `DataType::Function`'s Display uses `(a,b) -> r` for arity >= 2.
    assert_eq!(value.to_display_string(), "Function (Float,Int) -> Bool");
}
