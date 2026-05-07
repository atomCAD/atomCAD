//! Phase 1 — `Unit` type. Tests the type system, runtime value, conversions,
//! and text-format round-trip. See `doc/design_node_execution.md`.

use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
use rust_lib_flutter_cad::structure_designer::evaluator::iterator_walker::Walker;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

// ============================================================================
// `T → Unit` universal widening (the "discard" rule)
// ============================================================================

#[test]
fn float_to_unit_is_allowed_at_type_level() {
    let registry = NodeTypeRegistry::new();
    assert!(DataType::can_be_converted_to(
        &DataType::Float,
        &DataType::Unit,
        &registry
    ));
}

#[test]
fn float_to_unit_runtime_produces_unit() {
    let registry = NodeTypeRegistry::new();
    let r = NetworkResult::Float(2.5).convert_to(&DataType::Float, &DataType::Unit, &registry);
    assert!(matches!(r, NetworkResult::Unit));
}

#[test]
fn iter_float_to_unit_produces_unit_and_drops_walker() {
    // The walker is discarded without being drained — the runtime side of the
    // universal `T → Unit` widening for iterator sources.
    let registry = NodeTypeRegistry::new();
    let walker = Walker::from_array(vec![
        NetworkResult::Float(1.0),
        NetworkResult::Float(2.0),
        NetworkResult::Float(3.0),
    ]);
    let src_type = DataType::Iterator(Box::new(DataType::Float));
    let r = NetworkResult::Iterator(walker).convert_to(&src_type, &DataType::Unit, &registry);
    assert!(matches!(r, NetworkResult::Unit));
}

#[test]
fn iter_float_to_unit_is_allowed_at_type_level() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Float));
    assert!(DataType::can_be_converted_to(
        &src,
        &DataType::Unit,
        &registry
    ));
}

// ============================================================================
// `Unit → T` rejection
// ============================================================================

#[test]
fn unit_to_float_rejected() {
    let registry = NodeTypeRegistry::new();
    assert!(!DataType::can_be_converted_to(
        &DataType::Unit,
        &DataType::Float,
        &registry
    ));
}

#[test]
fn unit_to_int_rejected() {
    let registry = NodeTypeRegistry::new();
    assert!(!DataType::can_be_converted_to(
        &DataType::Unit,
        &DataType::Int,
        &registry
    ));
}

#[test]
fn unit_to_iter_float_rejected() {
    // A scalar `T → Iter[T]` broadcast requires `T → T`. Unit → Float is
    // rejected, so Unit → Iter[Float] is also rejected.
    let registry = NodeTypeRegistry::new();
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(!DataType::can_be_converted_to(
        &DataType::Unit,
        &dst,
        &registry
    ));
}

#[test]
fn unit_to_array_float_rejected() {
    let registry = NodeTypeRegistry::new();
    let dst = DataType::Array(Box::new(DataType::Float));
    assert!(!DataType::can_be_converted_to(
        &DataType::Unit,
        &dst,
        &registry
    ));
}

// ============================================================================
// Iterator passthrough rule: `Iter[Unit] → Iter[Unit]` only
// ============================================================================

#[test]
fn iter_unit_to_iter_unit_identity_allowed() {
    let registry = NodeTypeRegistry::new();
    let t = DataType::Iterator(Box::new(DataType::Unit));
    assert!(DataType::can_be_converted_to(&t, &t, &registry));
}

#[test]
fn iter_float_to_iter_unit_rejected() {
    // Per design: `Iter[S] → Iter[T]` with `S ≠ T` is disallowed in v1, even
    // for `T = Unit`. Use a `collect` + scalar discard if you really need it.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Float));
    let dst = DataType::Iterator(Box::new(DataType::Unit));
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

// ============================================================================
// Function covariant output rule: `[A] → T` widens to `[A] → Unit`
// ============================================================================

#[test]
fn function_int_to_string_widens_to_int_to_unit() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::String),
    });
    let dst = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Unit),
    });
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn function_int_to_unit_does_not_widen_to_int_to_float() {
    // The reverse direction is forbidden by `Unit → T` rejection on the
    // output position.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Unit),
    });
    let dst = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Float),
    });
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

// ============================================================================
// Identity + display + infer
// ============================================================================

#[test]
fn unit_identity_holds() {
    let registry = NodeTypeRegistry::new();
    assert!(DataType::can_be_converted_to(
        &DataType::Unit,
        &DataType::Unit,
        &registry
    ));
}

#[test]
fn unit_display_string_is_paren_pair() {
    assert_eq!(NetworkResult::Unit.to_display_string(), "()");
}

#[test]
fn unit_infer_data_type() {
    assert_eq!(NetworkResult::Unit.infer_data_type(), Some(DataType::Unit));
}

#[test]
fn unit_is_not_abstract() {
    assert!(!DataType::Unit.is_abstract());
}

// ============================================================================
// Text-format round-trip
// ============================================================================

#[test]
fn unit_parses_and_displays() {
    let parsed = DataType::from_string("Unit").unwrap();
    assert_eq!(parsed, DataType::Unit);
    assert_eq!(parsed.to_string(), "Unit");
}

#[test]
fn iter_unit_parses_and_displays() {
    let parsed = DataType::from_string("Iter[Unit]").unwrap();
    assert_eq!(parsed, DataType::Iterator(Box::new(DataType::Unit)));
    assert_eq!(parsed.to_string(), "Iter[Unit]");
}

#[test]
fn function_to_unit_parses_and_displays() {
    let parsed = DataType::from_string("Int -> Unit").unwrap();
    assert_eq!(
        parsed,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Unit),
        })
    );
    assert_eq!(parsed.to_string(), "Int -> Unit");
}
