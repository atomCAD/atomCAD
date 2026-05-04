//! Phase 1 tests for record types (see `doc/design_record_types.md`).
//!
//! Phase 1 covers type/value plumbing only:
//! - `DataType::Record(RecordType)` with `Named` / `Anonymous` variants and
//!   canonicalization of anonymous fields.
//! - `NetworkResult::Record(...)` with the `record(...)` canonicalizing
//!   constructor and `extract_record_field` lookup.
//! - `infer_data_type` on a record value returns an anonymous record type with
//!   canonical field order.
//! - `can_be_converted_to` accepts the new `&NodeTypeRegistry` parameter and
//!   short-circuits `Named(n) → Named(n)` without resolving the registry.
//! - The threading change does not regress non-record subtyping.
//!
//! Full structural subtyping for records (width + depth) is intentionally a
//! Phase 4 task and is not exercised here.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

// ---------------------------------------------------------------------------
// RecordType: equality + Hash invariants under canonical (sorted) construction.
// ---------------------------------------------------------------------------

#[test]
fn anonymous_record_type_canonicalizes_field_order_on_construction() {
    let a = RecordType::anonymous(vec![
        ("y".to_string(), DataType::Int),
        ("x".to_string(), DataType::Int),
    ]);
    let b = RecordType::anonymous(vec![
        ("x".to_string(), DataType::Int),
        ("y".to_string(), DataType::Int),
    ]);
    assert_eq!(a, b, "differently-ordered anon records should compare equal");
    assert_eq!(
        hash_of(&a),
        hash_of(&b),
        "differently-ordered anon records should hash equal"
    );

    // Direct inspection of the canonical storage order on `a`.
    match a {
        RecordType::Anonymous(fs) => {
            let names: Vec<&str> = fs.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["x", "y"]);
        }
        _ => panic!("expected Anonymous"),
    }
}

#[test]
fn named_and_anonymous_are_never_equal_even_with_matching_schema() {
    // The design promises that `Named(n)` and `Anonymous(...)` are never `==`,
    // even when the resolved fields match. Use `can_be_converted_to` for
    // structural compatibility instead (Phase 4).
    let named = RecordType::named("Point".to_string());
    let anon = RecordType::anonymous(vec![
        ("x".to_string(), DataType::Int),
        ("y".to_string(), DataType::Int),
    ]);
    assert_ne!(named, anon);
}

#[test]
fn two_named_record_types_with_the_same_name_are_equal() {
    let a = RecordType::named("Foo".to_string());
    let b = RecordType::named("Foo".to_string());
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));
}

// ---------------------------------------------------------------------------
// can_be_converted_to: same-name Named→Named short-circuit + non-record regression.
// ---------------------------------------------------------------------------

#[test]
fn can_be_converted_to_short_circuits_named_to_named_with_same_name() {
    // The same-name short-circuit must fire even with an empty registry, since
    // resolving the def is unnecessary when the names already match.
    let registry = NodeTypeRegistry::new();
    let foo = DataType::Record(RecordType::named("Foo".to_string()));
    assert!(DataType::can_be_converted_to(&foo, &foo, &registry));
}

#[test]
fn can_be_converted_to_rejects_named_pairs_with_different_names_in_phase_1() {
    // Phase 4 will accept structurally-compatible records under different
    // names. In Phase 1 only the same-name short-circuit is wired up.
    let registry = NodeTypeRegistry::new();
    let foo = DataType::Record(RecordType::named("Foo".to_string()));
    let bar = DataType::Record(RecordType::named("Bar".to_string()));
    assert!(!DataType::can_be_converted_to(&foo, &bar, &registry));
}

#[test]
fn can_be_converted_to_anonymous_to_anonymous_uses_width_subtyping_post_phase_4() {
    // Phase 1 deliberately rejected anonymous→anonymous with mismatched
    // schemas; Phase 4 enabled width subtyping, so `b = {x, y}` is now
    // assignable to `a = {x}` (extra fields on the source pass through under
    // pass-through coercion). The Phase 1 negative form has been flipped to
    // match the Phase 4 rule. The full structural-subtyping table lives in
    // `record_types_phase4_test.rs`.
    let registry = NodeTypeRegistry::new();
    let a = DataType::Record(RecordType::anonymous(vec![(
        "x".to_string(),
        DataType::Int,
    )]));
    let b = DataType::Record(RecordType::anonymous(vec![
        ("x".to_string(), DataType::Int),
        ("y".to_string(), DataType::Int),
    ]));
    assert!(DataType::can_be_converted_to(&b, &a, &registry));
    // Narrowing in the other direction is still rejected — `a` is missing the
    // `y` field that `b` requires.
    assert!(!DataType::can_be_converted_to(&a, &b, &registry));

    // Same schema (width-equal): the early `source == dest` identity check
    // still returns true, which is correct in any phase.
    let c = DataType::Record(RecordType::anonymous(vec![(
        "x".to_string(),
        DataType::Int,
    )]));
    assert!(DataType::can_be_converted_to(&a, &c, &registry));
}

#[test]
fn can_be_converted_to_does_not_regress_non_record_phase_upcasts() {
    // Spot-check that threading `&NodeTypeRegistry` through the function has
    // not broken existing concrete-to-abstract phase upcasts.
    let registry = NodeTypeRegistry::new();
    assert!(DataType::can_be_converted_to(
        &DataType::Crystal,
        &DataType::HasAtoms,
        &registry,
    ));
    assert!(DataType::can_be_converted_to(
        &DataType::Molecule,
        &DataType::HasFreeLinOps,
        &registry,
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::HasAtoms,
        &DataType::Crystal,
        &registry,
    ));
    // Int → Float still permitted.
    assert!(DataType::can_be_converted_to(
        &DataType::Int,
        &DataType::Float,
        &registry,
    ));
    // Single-to-array broadcast still permitted.
    let arr_int = DataType::Array(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(
        &DataType::Int,
        &arr_int,
        &registry,
    ));
    // Element-wise array conversion still permitted.
    let arr_molecule = DataType::Array(Box::new(DataType::Molecule));
    let arr_has_atoms = DataType::Array(Box::new(DataType::HasAtoms));
    assert!(DataType::can_be_converted_to(
        &arr_molecule,
        &arr_has_atoms,
        &registry,
    ));
}

// ---------------------------------------------------------------------------
// NetworkResult::record + extract_record_field round-trip.
// ---------------------------------------------------------------------------

#[test]
fn network_result_record_constructor_canonicalizes_field_order() {
    let r = NetworkResult::record(vec![
        ("y".to_string(), NetworkResult::Int(2)),
        ("x".to_string(), NetworkResult::Int(1)),
    ]);
    match &r {
        NetworkResult::Record(fs) => {
            let names: Vec<&str> = fs.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["x", "y"]);
        }
        _ => panic!("expected Record"),
    }
}

#[test]
fn extract_record_field_finds_each_field_regardless_of_construction_order() {
    let r = NetworkResult::record(vec![
        ("z".to_string(), NetworkResult::Int(3)),
        ("x".to_string(), NetworkResult::Int(1)),
        ("y".to_string(), NetworkResult::Int(2)),
    ]);
    assert!(matches!(r.extract_record_field("x"), Some(NetworkResult::Int(1))));
    assert!(matches!(r.extract_record_field("y"), Some(NetworkResult::Int(2))));
    assert!(matches!(r.extract_record_field("z"), Some(NetworkResult::Int(3))));
    assert!(r.extract_record_field("missing").is_none());
}

#[test]
fn extract_record_field_returns_none_for_non_record_values() {
    let r = NetworkResult::Int(42);
    assert!(r.extract_record_field("x").is_none());
}

// ---------------------------------------------------------------------------
// infer_data_type on records.
// ---------------------------------------------------------------------------

#[test]
fn infer_data_type_on_record_returns_anonymous_with_canonical_order() {
    // Build a record with shuffled field order; infer_data_type should still
    // return the schema in canonical (sorted) order.
    let r = NetworkResult::record(vec![
        ("y".to_string(), NetworkResult::Int(2)),
        ("x".to_string(), NetworkResult::Int(1)),
    ]);
    let ty = r.infer_data_type().expect("should infer");
    match ty {
        DataType::Record(RecordType::Anonymous(fs)) => {
            let names: Vec<&str> = fs.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["x", "y"]);
            assert_eq!(fs[0].1, DataType::Int);
            assert_eq!(fs[1].1, DataType::Int);
        }
        other => panic!("expected anonymous record, got {:?}", other),
    }
}

#[test]
fn infer_data_type_on_record_returns_none_when_a_field_is_inferable_only_partially() {
    // A field whose value is itself non-inferable (e.g. None) makes the whole
    // record non-inferable — same convention used by arrays.
    let r = NetworkResult::record(vec![
        ("ok".to_string(), NetworkResult::Int(1)),
        ("bad".to_string(), NetworkResult::None),
    ]);
    assert!(r.infer_data_type().is_none());
}

#[test]
fn infer_data_type_round_trip_via_anonymous_helper_is_canonical() {
    // The schema returned by infer_data_type compares equal to one constructed
    // independently via `RecordType::anonymous`, regardless of the original
    // value's authored field order.
    let r = NetworkResult::record(vec![
        ("b".to_string(), NetworkResult::Bool(true)),
        ("a".to_string(), NetworkResult::Int(7)),
    ]);
    let inferred = r.infer_data_type().expect("should infer");
    let expected = DataType::Record(RecordType::anonymous(vec![
        ("a".to_string(), DataType::Int),
        ("b".to_string(), DataType::Bool),
    ]));
    assert_eq!(inferred, expected);
}
