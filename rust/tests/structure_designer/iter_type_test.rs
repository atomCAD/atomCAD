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
use rust_lib_flutter_cad::structure_designer::evaluator::iterator_walker::Walker;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

/// Drain an `Iter[T]` value into its element `NetworkResult`s.
fn drain(registry: &NodeTypeRegistry, result: NetworkResult) -> Vec<NetworkResult> {
    let mut walker = match result {
        NetworkResult::Iterator(w) => w,
        other => panic!("expected Iterator, got {}", other.to_display_string()),
    };
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let mut out = Vec::new();
    while out.len() < 4096 {
        match walker.next(&evaluator, registry, &mut context) {
            None => return out,
            Some(v) => out.push(v),
        }
    }
    panic!("drain exceeded cap of 4096 elements");
}

fn as_ints(values: Vec<NetworkResult>) -> Vec<i32> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Int(v) => v,
            other => panic!("expected Int element, got {}", other.to_display_string()),
        })
        .collect()
}

fn as_floats(values: Vec<NetworkResult>) -> Vec<f64> {
    values
        .into_iter()
        .map(|r| match r {
            NetworkResult::Float(v) => v,
            other => panic!("expected Float element, got {}", other.to_display_string()),
        })
        .collect()
}

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
fn iter_to_iter_with_element_widening_is_allowed() {
    // `Iter[Int] → Iter[Float]`: lazy element conversion (open question #2 of
    // `doc/design_iterators.md`, now implemented). The runtime wraps the source
    // walker in `Walker::convert` and runs `convert_to` per pulled element.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn iter_to_iter_with_element_narrowing_is_allowed() {
    // The reverse `Iter[Float] → Iter[Int]` is also a permitted scalar
    // conversion (truncating, mirroring the scalar `Float → Int` rule).
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Float));
    let dst = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn iter_to_iter_with_incompatible_element_types_is_still_rejected() {
    // The element-level rule still gates: `Int → String` is not a permitted
    // scalar conversion, so neither is `Iter[Int] → Iter[String]`.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::String));
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn nested_iter_of_iter_element_widening_is_allowed() {
    // `Iter[Iter[Int]] → Iter[Iter[Float]]` recurses: the outer rule defers to
    // the inner `Iter[Int] → Iter[Float]`, which is now allowed.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Iterator(Box::new(DataType::Int))));
    let dst = DataType::Iterator(Box::new(DataType::Iterator(Box::new(DataType::Float))));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
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

// ============================================================================
// Runtime value conversion (`NetworkResult::convert_to`) into `Iter[T]`
// ============================================================================
//
// The type-level rules above are about *whether* a wire is allowed; these
// tests pin the *value-level* behavior of `convert_to` when an `Iter[T]`
// destination is involved. The load-bearing one is
// `iterator_value_with_none_source_passes_through`: a live `Iterator` value
// must never be broadcast into a one-element stream, even when the declared
// `source_type` fails to resolve to `Iterator(_)` (which happens when an
// `Iter[T]` flows through a non-HOF `ZoneInput` — a custom network used via
// its function pin — and the source type falls back to `infer_data_type()`,
// which has no `Iterator` arm and yields `None`). Regression guard for the
// "Arithmetic operation not supported for these types" bug where a downstream
// `map` saw the whole iterator as its single element.

#[test]
fn iterator_value_with_none_source_passes_through() {
    let registry = NodeTypeRegistry::new();
    let value = NetworkResult::Iterator(Walker::from_array(vec![
        NetworkResult::Int(1),
        NetworkResult::Int(2),
        NetworkResult::Int(3),
    ]));
    // Source type unresolved (`None`) — as it is for an iterator flowing
    // through a non-HOF `ZoneInput`. Must pass the iterator through unchanged,
    // NOT wrap it as a single element.
    let converted = value.convert_to(
        &DataType::None,
        &DataType::Iterator(Box::new(DataType::Int)),
        &registry,
    );
    assert_eq!(as_ints(drain(&registry, converted)), vec![1, 2, 3]);
}

#[test]
fn iterator_value_with_iter_source_passes_through() {
    let registry = NodeTypeRegistry::new();
    let iter_int = DataType::Iterator(Box::new(DataType::Int));
    let value = NetworkResult::Iterator(Walker::from_array(vec![
        NetworkResult::Int(10),
        NetworkResult::Int(20),
    ]));
    let converted = value.convert_to(&iter_int, &iter_int, &registry);
    assert_eq!(as_ints(drain(&registry, converted)), vec![10, 20]);
}

#[test]
fn scalar_value_still_broadcasts_into_singleton_iter() {
    // The runtime-value guard must NOT suppress the legitimate `S → Iter[T]`
    // single-element broadcast for a non-iterator value.
    let registry = NodeTypeRegistry::new();
    let converted = NetworkResult::Int(7).convert_to(
        &DataType::Int,
        &DataType::Iterator(Box::new(DataType::Int)),
        &registry,
    );
    assert_eq!(as_ints(drain(&registry, converted)), vec![7]);
}

#[test]
fn array_value_still_wraps_into_iter_elementwise() {
    // The `[S] → Iter[T]` eager element wrap is unaffected by the guard.
    let registry = NodeTypeRegistry::new();
    let value = NetworkResult::Array(vec![NetworkResult::Int(4), NetworkResult::Int(5)]);
    let converted = value.convert_to(
        &DataType::Array(Box::new(DataType::Int)),
        &DataType::Iterator(Box::new(DataType::Int)),
        &registry,
    );
    assert_eq!(as_ints(drain(&registry, converted)), vec![4, 5]);
}

// ============================================================================
// Lazy `Iter[S] → Iter[T]` element conversion (the converting walker)
// ============================================================================
//
// Type-level acceptance is covered above; these tests pin the runtime
// behavior of the `Walker::convert` variant that `convert_to` installs when an
// `Iter[S]` value flows into an `Iter[T]` slot with `S ≠ T`.

#[test]
fn iter_int_value_converts_elementwise_to_iter_float() {
    let registry = NodeTypeRegistry::new();
    let value = NetworkResult::Iterator(Walker::from_array(vec![
        NetworkResult::Int(1),
        NetworkResult::Int(2),
        NetworkResult::Int(3),
    ]));
    let converted = value.convert_to(
        &DataType::Iterator(Box::new(DataType::Int)),
        &DataType::Iterator(Box::new(DataType::Float)),
        &registry,
    );
    assert_eq!(as_floats(drain(&registry, converted)), vec![1.0, 2.0, 3.0]);
}

#[test]
fn iter_float_value_converts_elementwise_to_iter_int_truncating() {
    // Mirrors the scalar `Float → Int` rule: each element is rounded.
    let registry = NodeTypeRegistry::new();
    let value = NetworkResult::Iterator(Walker::from_array(vec![
        NetworkResult::Float(1.4),
        NetworkResult::Float(2.6),
        NetworkResult::Float(-0.5),
    ]));
    let converted = value.convert_to(
        &DataType::Iterator(Box::new(DataType::Float)),
        &DataType::Iterator(Box::new(DataType::Int)),
        &registry,
    );
    // `convert_to` uses `f64::round` (round-half-away-from-zero): -0.5 → -1.
    assert_eq!(as_ints(drain(&registry, converted)), vec![1, 3, -1]);
}

#[test]
fn empty_iter_converts_to_empty_iter() {
    let registry = NodeTypeRegistry::new();
    let value = NetworkResult::Iterator(Walker::from_array(vec![]));
    let converted = value.convert_to(
        &DataType::Iterator(Box::new(DataType::Int)),
        &DataType::Iterator(Box::new(DataType::Float)),
        &registry,
    );
    assert!(drain(&registry, converted).is_empty());
}

/// Pull one element and extract it as `Option<f64>` (`None` = stream end).
/// `NetworkResult` implements neither `PartialEq` nor `Debug`, so tests
/// project to a comparable primitive before asserting.
fn next_float(
    walker: &mut Walker,
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> Option<f64> {
    match walker.next(evaluator, registry, context) {
        None => None,
        Some(NetworkResult::Float(v)) => Some(v),
        Some(other) => panic!("expected Float element, got {}", other.to_display_string()),
    }
}

#[test]
fn converting_walker_clone_advances_independently() {
    // Invariant 2 (clone independence): cloning a converting walker — as every
    // `NetworkResult` read site does — must yield a walker whose `next`
    // advances independently of the original.
    let registry = NodeTypeRegistry::new();
    let converted = NetworkResult::Iterator(Walker::from_array(vec![
        NetworkResult::Int(7),
        NetworkResult::Int(8),
    ]))
    .convert_to(
        &DataType::Iterator(Box::new(DataType::Int)),
        &DataType::Iterator(Box::new(DataType::Float)),
        &registry,
    );
    let mut walker = match converted {
        NetworkResult::Iterator(w) => w,
        other => panic!("expected Iterator, got {}", other.to_display_string()),
    };
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();

    // Advance the original by one element, then clone.
    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        Some(7.0)
    );
    let mut cloned = walker.clone();

    // Draining the clone must not disturb the original's position.
    assert_eq!(
        next_float(&mut cloned, &evaluator, &registry, &mut context),
        Some(8.0)
    );
    assert_eq!(
        next_float(&mut cloned, &evaluator, &registry, &mut context),
        None
    );

    // The original still has exactly its own remaining element.
    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        Some(8.0)
    );
    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        None
    );
}

#[test]
fn converting_walker_resets() {
    let registry = NodeTypeRegistry::new();
    let converted = NetworkResult::Iterator(Walker::from_array(vec![
        NetworkResult::Int(1),
        NetworkResult::Int(2),
    ]))
    .convert_to(
        &DataType::Iterator(Box::new(DataType::Int)),
        &DataType::Iterator(Box::new(DataType::Float)),
        &registry,
    );
    let mut walker = match converted {
        NetworkResult::Iterator(w) => w,
        other => panic!("expected Iterator, got {}", other.to_display_string()),
    };
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();

    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        Some(1.0)
    );
    walker.reset();
    // After reset the converted stream replays from the start.
    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        Some(1.0)
    );
    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        Some(2.0)
    );
    assert_eq!(
        next_float(&mut walker, &evaluator, &registry, &mut context),
        None
    );
}
