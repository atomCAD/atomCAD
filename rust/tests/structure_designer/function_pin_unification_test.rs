//! Function-pin unification Phase A tests.
//!
//! Covers the type-system plumbing for `DataType::AnyFunction { leading_params }`:
//! compatibility rule (concrete `Function(_)` into `AnyFunction`), reverse
//! direction rejection (`AnyFunction` as a source is invalid against everything
//! but `Unit`), canonicalization through `leading_params`, parser/Display
//! round-trip (`Function*`, `Function(T1, T2, *)`), and the
//! `NetworkResult::Function → AnyFunction` identity passthrough.
//!
//! No node uses `AnyFunction` yet — Phase B retrofits `apply.f` and Phase C
//! retrofits `map.f`. See `doc/design_function_pin_unification.md`.
//!
//! Test layout mirrors `currying_test.rs` and `function_value_test.rs`:
//! one section per behaviour, each test is a single assertion or two.

use std::collections::HashMap;
use std::sync::Arc;

use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, FunctionType, canonicalize_data_type,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::evaluator::zone_closure::ZoneClosure;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Compatibility: concrete Function flows into AnyFunction
// =============================================================================

#[test]
fn function_one_param_flows_into_unconstrained_any_function() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Bool));
    let dst = DataType::AnyFunction {
        leading_params: vec![],
    };
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
    // Same rule under the strict (no-broadcast) variant — there's no
    // broadcast involved in this conversion, so it must accept.
    assert!(DataType::can_be_converted_to_strict_no_broadcast(
        &src, &dst, &registry
    ));
}

#[test]
fn function_two_params_starts_with_leading_param_is_accepted() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType::new(
        vec![DataType::Int, DataType::Bool],
        DataType::String,
    ));
    let dst = DataType::AnyFunction {
        leading_params: vec![DataType::Int],
    };
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn function_one_param_with_mismatched_leading_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType::new(vec![DataType::Bool], DataType::String));
    let dst = DataType::AnyFunction {
        leading_params: vec![DataType::Int],
    };
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn function_too_short_for_leading_params_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType::new(vec![], DataType::Int));
    let dst = DataType::AnyFunction {
        leading_params: vec![DataType::Int],
    };
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn leading_param_pairwise_uses_can_be_converted_to() {
    // The compatibility rule allows leaf-level conversions (Int → Float)
    // for each leading param — same as Function-to-Function structural match.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType::new(
        vec![DataType::Int, DataType::Bool],
        DataType::String,
    ));
    let dst = DataType::AnyFunction {
        leading_params: vec![DataType::Float],
    };
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

// =============================================================================
// Reverse direction (AnyFunction as source) is rejected
// =============================================================================

#[test]
fn any_function_into_concrete_function_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::AnyFunction {
        leading_params: vec![],
    };
    let dst = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Int));
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
    assert!(!DataType::can_be_converted_to_strict_no_broadcast(
        &src, &dst, &registry
    ));
}

#[test]
fn any_function_into_any_function_is_only_identity() {
    // `AnyFunction` as a source is rejected against everything except identity
    // (top short-circuit) and `Unit` (discard rule). Two `AnyFunction`s with
    // distinct `leading_params` lists are not implicitly comparable.
    let registry = NodeTypeRegistry::new();
    let same = DataType::AnyFunction {
        leading_params: vec![DataType::Int],
    };
    let different = DataType::AnyFunction {
        leading_params: vec![],
    };
    // Identity holds.
    assert!(DataType::can_be_converted_to(&same, &same, &registry));
    // Non-identity is rejected — AnyFunction-as-source.
    assert!(!DataType::can_be_converted_to(&same, &different, &registry));
}

#[test]
fn any_function_to_unit_is_permitted_by_discard_rule() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::AnyFunction {
        leading_params: vec![],
    };
    // `T → Unit` is universal; AnyFunction is no exception.
    assert!(DataType::can_be_converted_to(
        &src,
        &DataType::Unit,
        &registry
    ));
}

// =============================================================================
// Canonicalization
// =============================================================================

#[test]
fn canonicalize_recurses_into_leading_params() {
    // A non-canonical `Function((A,), Function((B,), C))` inside `leading_params`
    // must collapse to `Function((A, B), C)` after `canonicalize_data_type`.
    let non_canonical_inner = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Bool],
        output_type: Box::new(DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Float),
        })),
    });
    let mut ty = DataType::AnyFunction {
        leading_params: vec![non_canonical_inner],
    };
    canonicalize_data_type(&mut ty);

    let DataType::AnyFunction { leading_params } = ty else {
        panic!("expected AnyFunction");
    };
    assert_eq!(leading_params.len(), 1);
    let DataType::Function(inner_ft) = &leading_params[0] else {
        panic!("expected nested Function inside leading_params");
    };
    assert_eq!(
        inner_ft.parameter_types,
        vec![DataType::Bool, DataType::Int]
    );
    assert_eq!(*inner_ft.output_type, DataType::Float);
}

// =============================================================================
// Parser / Display round-trip
// =============================================================================

#[test]
fn display_unconstrained_any_function() {
    let ty = DataType::AnyFunction {
        leading_params: vec![],
    };
    assert_eq!(format!("{}", ty), "Function*");
}

#[test]
fn display_constrained_any_function() {
    let ty = DataType::AnyFunction {
        leading_params: vec![DataType::Int, DataType::Bool],
    };
    assert_eq!(format!("{}", ty), "Function(Int,Bool,*)");
}

#[test]
fn parse_unconstrained_any_function() {
    let ty = DataType::from_string("Function*").expect("parse Function*");
    assert_eq!(
        ty,
        DataType::AnyFunction {
            leading_params: vec![]
        }
    );
}

#[test]
fn parse_single_leading_param_any_function() {
    let ty = DataType::from_string("Function(Int,*)").expect("parse Function(Int,*)");
    assert_eq!(
        ty,
        DataType::AnyFunction {
            leading_params: vec![DataType::Int]
        }
    );
}

#[test]
fn parse_multi_leading_param_any_function() {
    let ty = DataType::from_string("Function(Int,Bool,*)").expect("parse Function(Int,Bool,*)");
    assert_eq!(
        ty,
        DataType::AnyFunction {
            leading_params: vec![DataType::Int, DataType::Bool]
        }
    );
}

#[test]
fn parse_nested_function_inside_leading_params() {
    // `Function((A) -> B, *)` is accepted; the nested function leading param
    // is parsed as a concrete `Function(A) -> B`.
    let ty = DataType::from_string("Function((Int) -> Bool,*)")
        .expect("parse Function((Int) -> Bool,*)");
    let DataType::AnyFunction { leading_params } = ty else {
        panic!("expected AnyFunction");
    };
    assert_eq!(leading_params.len(), 1);
    let DataType::Function(ft) = &leading_params[0] else {
        panic!("expected nested Function leading param");
    };
    assert_eq!(ft.parameter_types, vec![DataType::Int]);
    assert_eq!(*ft.output_type, DataType::Bool);
}

#[test]
fn display_parse_round_trip_unconstrained() {
    let ty = DataType::AnyFunction {
        leading_params: vec![],
    };
    let parsed = DataType::from_string(&format!("{}", ty)).expect("round-trip parse");
    assert_eq!(parsed, ty);
}

#[test]
fn display_parse_round_trip_multi_leading_params() {
    let ty = DataType::AnyFunction {
        leading_params: vec![DataType::Int, DataType::Bool],
    };
    let parsed = DataType::from_string(&format!("{}", ty)).expect("round-trip parse");
    assert_eq!(parsed, ty);
}

// =============================================================================
// NetworkResult::convert_to identity passthrough
// =============================================================================

/// Build a `NetworkResult::Function(ZoneClosure)` whose carried `param_types`
/// / `return_type` describe `(Int) -> Bool`. The body / captures / wires are
/// empty — convert_to only consults the variant tag, not the closure shape.
/// Mirrors the helper used by `function_value_test.rs`.
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
fn function_value_into_any_function_is_identity_passthrough() {
    let registry = NodeTypeRegistry::new();
    let value = make_function_value(vec![DataType::Int], DataType::Bool);
    let src_ty = value.infer_data_type().unwrap();
    let dst_ty = DataType::AnyFunction {
        leading_params: vec![],
    };
    assert!(DataType::can_be_converted_to(&src_ty, &dst_ty, &registry));

    let converted = value.convert_to(&src_ty, &dst_ty, &registry);
    // The runtime variant is unchanged and the carried metadata still reports
    // the original concrete Function type — convert_to performs no rewriting.
    assert_eq!(converted.infer_data_type().unwrap(), src_ty);
    assert!(matches!(converted, NetworkResult::Function(_)));
}

#[test]
fn function_value_into_constrained_any_function_is_identity_passthrough() {
    let registry = NodeTypeRegistry::new();
    let value = make_function_value(vec![DataType::Int, DataType::Bool], DataType::String);
    let src_ty = value.infer_data_type().unwrap();
    let dst_ty = DataType::AnyFunction {
        leading_params: vec![DataType::Int],
    };
    assert!(DataType::can_be_converted_to(&src_ty, &dst_ty, &registry));

    let converted = value.convert_to(&src_ty, &dst_ty, &registry);
    assert_eq!(converted.infer_data_type().unwrap(), src_ty);
    assert!(matches!(converted, NetworkResult::Function(_)));
}
