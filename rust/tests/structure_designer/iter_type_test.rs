//! Type-system + wire-validation tests for `Iter[T]` (Phase 1 of
//! `doc/design_iterators.md`).
//!
//! Coverage:
//! - `DataType::can_be_converted_to` for every documented `Iter[T]` rule
//!   (and every documented rejection).
//! - Closure-capture restriction: a function pin whose source has an
//!   `Iter[T]` value pin is rejected by the network validator.
//! - Top-level parameter rejection at the CLI binding layer.

use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, FunctionType, RecordType, contains_iterator,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

// ============================================================================
// Wire-time conversion rules
// ============================================================================

#[test]
fn array_to_iter_same_element_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Array(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn array_to_iter_with_element_widening_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Array(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn scalar_to_iter_is_allowed_via_broadcast() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Int;
    let dst = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn scalar_to_iter_with_widening_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Int;
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn iter_identity_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let t = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&t, &t, &registry));
}

#[test]
fn iter_to_iter_with_different_element_types_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(
        !DataType::can_be_converted_to(&src, &dst, &registry),
        "Iter[Int] â†’ Iter[Float] is reserved for a follow-up; not implicit in v1"
    );
}

#[test]
fn iter_to_array_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Array(Box::new(DataType::Int));
    assert!(
        !DataType::can_be_converted_to(&src, &dst, &registry),
        "Iter[T] â†’ [T] requires an explicit `collect` node"
    );
}

#[test]
fn iter_to_scalar_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Int;
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn iter_inside_array_disallowed_widening_to_array_only_iter() {
    // `[Iter[Int]] â†’ [Int]` would require unwrapping the iterator at every
    // element, which is not an implicit conversion.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))));
    let dst = DataType::Array(Box::new(DataType::Int));
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

// ============================================================================
// `Iter[T]` parsing roundtrip
// ============================================================================

#[test]
fn iter_int_parses_and_displays() {
    let parsed = DataType::from_string("Iter[Int]").unwrap();
    assert_eq!(parsed, DataType::Iterator(Box::new(DataType::Int)));
    assert_eq!(parsed.to_string(), "Iter[Int]");
}

#[test]
fn nested_iter_parses() {
    let parsed = DataType::from_string("Iter[Iter[Int]]").unwrap();
    assert_eq!(
        parsed,
        DataType::Iterator(Box::new(DataType::Iterator(Box::new(DataType::Int))))
    );
}

#[test]
fn array_of_iter_parses() {
    let parsed = DataType::from_string("[Iter[Int]]").unwrap();
    assert_eq!(
        parsed,
        DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))))
    );
}

#[test]
fn bare_iter_without_brackets_is_rejected() {
    // `Iter` alone is not a valid type â€” the bracket is mandatory.
    assert!(DataType::from_string("Iter").is_err());
}

// ============================================================================
// `contains_iterator` helper
// ============================================================================

#[test]
fn contains_iterator_recognizes_direct() {
    assert!(contains_iterator(&DataType::Iterator(Box::new(
        DataType::Int
    ))));
}

#[test]
fn contains_iterator_recurses_into_array() {
    let t = DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))));
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_recurses_into_function_param() {
    let t = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Iterator(Box::new(DataType::Int))],
        output_type: Box::new(DataType::Int),
    });
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_recurses_into_function_return() {
    let t = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Iterator(Box::new(DataType::Int))),
    });
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_recurses_into_anonymous_record() {
    let t = DataType::Record(RecordType::Anonymous(vec![(
        "field".to_string(),
        DataType::Iterator(Box::new(DataType::Int)),
    )]));
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_returns_false_for_iter_free_types() {
    assert!(!contains_iterator(&DataType::Int));
    assert!(!contains_iterator(&DataType::Array(Box::new(
        DataType::Float
    ))));
    assert!(!contains_iterator(&DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Float),
    })));
}

// ============================================================================
// Closure-capture restriction (validator)
// ============================================================================
//
// Phase 5 of `doc/design_zones.md` retired the function-pin parameter from
// every user-facing HOF (`map`, `filter`, `fold`, `foreach`). The
// closure-capture validator (rejecting `Iter[T]` captured via a value pin)
// is still in place for the `DataType::Function` plumbing, but no node has
// a function-pin input anymore, so there's nothing user-visible to wire
// against. The previous `filter.f` / `map.f` regression tests for this path
// were removed in Phase 5; if a user-facing function-pin parameter is ever
// reintroduced, restore an analogous regression test here.

// ============================================================================
// Top-level parameter rejection (CLI binding layer)
// ============================================================================
//
// `cli_runner::parse_cli_parameters` is the binding layer. It is `pub(crate)`
// (no `pub` qualifier on its `mod`-level `fn`), so we exercise the rule
// indirectly: a network whose parameter has type `Iter[Int]` is set up, and
// we re-run the equivalent `contains_iterator(&param.data_type)` predicate
// that the CLI uses. If `contains_iterator` returns `true`, the CLI runner
// rejects the parameter with the documented error message.
//
// The end-to-end CLI run is exercised in CLI integration tests; here we
// verify the predicate that gates the rejection.

#[test]
fn cli_top_level_parameter_with_iter_type_is_flagged() {
    // The predicate that gates CLI parameter rejection is `contains_iterator`.
    // `cli_runner::parse_cli_parameters` checks `contains_iterator(&param_def.data_type)`
    // before parsing and returns an explanatory error mentioning `Iter[T]` and
    // `collect`. If `contains_iterator` ever returns false for a declared
    // `Iter` type, the CLI rejection silently disappears â€” so lock the
    // predicate's behavior in here.
    assert!(contains_iterator(&DataType::Iterator(Box::new(
        DataType::Int
    ))));
    assert!(contains_iterator(&DataType::Array(Box::new(
        DataType::Iterator(Box::new(DataType::Int))
    ))));
    assert!(!contains_iterator(&DataType::Int));
    assert!(!contains_iterator(&DataType::Array(Box::new(
        DataType::Float
    ))));
}
