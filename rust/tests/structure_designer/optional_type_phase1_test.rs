//! Phase 1 tests for the `Optional[T]` data type (see
//! `doc/design_optional_type.md`).
//!
//! Phase 1 covers the core type-system building block — no node/records
//! integration (Phase 2) or Flutter/API (Phase 3) yet:
//! - `DataType::Optional` variant + `Display` + type-parser arm (incl.
//!   rejection of the four ill-formed inner shapes).
//! - Field-position subtyping arms in `can_be_structurally_converted_to`:
//!   `Optional[S] → Optional[T]` and `S → Optional[T]` reduce to the strict
//!   inner check; `Optional[S] → T` is rejected.
//! - `canonicalize_data_type` recursion through `Optional`.
//! - Registry validation of record defs against the ill-formed-Optional rules.

use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, FunctionType, RecordType, can_be_structurally_converted_to, canonicalize_data_type,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, RecordTypeDefError,
};

fn opt(inner: DataType) -> DataType {
    DataType::Optional(Box::new(inner))
}

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn def(name: &str, fields: &[(&str, DataType)]) -> RecordTypeDef {
    RecordTypeDef {
        name: name.to_string(),
        fields: fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Field-position subtyping (`can_be_structurally_converted_to`)
// ---------------------------------------------------------------------------

#[test]
fn bare_to_optional_accepts_tag_only_widening() {
    let r = NodeTypeRegistry::new();
    // `Crystal → Optional[HasAtoms]`: present value promoted to maybe-present,
    // inner is a tag-only phase upcast.
    assert!(can_be_structurally_converted_to(
        &DataType::Crystal,
        &opt(DataType::HasAtoms),
        &r
    ));
    // Identity inner: `Crystal → Optional[Crystal]`.
    assert!(can_be_structurally_converted_to(
        &DataType::Crystal,
        &opt(DataType::Crystal),
        &r
    ));
}

#[test]
fn bare_to_optional_rejects_value_converting_widening() {
    let r = NodeTypeRegistry::new();
    // `Int → Optional[Float]`: inner `Int → Float` is value-converting, which
    // is forbidden at field/leaf positions.
    assert!(!can_be_structurally_converted_to(
        &DataType::Int,
        &opt(DataType::Float),
        &r
    ));
}

#[test]
fn optional_to_optional_follows_inner_rule() {
    let r = NodeTypeRegistry::new();
    // Identity inner.
    assert!(can_be_structurally_converted_to(
        &opt(DataType::Crystal),
        &opt(DataType::Crystal),
        &r
    ));
    // Tag-only inner widening.
    assert!(can_be_structurally_converted_to(
        &opt(DataType::Crystal),
        &opt(DataType::HasAtoms),
        &r
    ));
    // Value-converting inner rejected.
    assert!(!can_be_structurally_converted_to(
        &opt(DataType::Int),
        &opt(DataType::Float),
        &r
    ));
}

#[test]
fn optional_to_bare_is_rejected() {
    let r = NodeTypeRegistry::new();
    // A maybe-present value cannot satisfy a field that requires presence.
    assert!(!can_be_structurally_converted_to(
        &opt(DataType::Crystal),
        &DataType::Crystal,
        &r
    ));
    assert!(!can_be_structurally_converted_to(
        &opt(DataType::HasAtoms),
        &DataType::HasAtoms,
        &r
    ));
}

#[test]
fn optional_record_inner_follows_record_structural_rule() {
    let mut r = NodeTypeRegistry::new();
    // A = {x: Int, y: Int}, B = {x: Int} — A structurally subtypes B (width).
    r.add_record_type_def(def("A", &[("x", DataType::Int), ("y", DataType::Int)]))
        .unwrap();
    r.add_record_type_def(def("B", &[("x", DataType::Int)]))
        .unwrap();

    // `Record(A) → Optional[Record(B)]` follows the ordinary record structural
    // rule (the predicate carries the registry).
    assert!(can_be_structurally_converted_to(
        &named("A"),
        &opt(named("B")),
        &r
    ));
    // Reverse width fails: `Record(B) → Optional[Record(A)]` (B lacks `y`).
    assert!(!can_be_structurally_converted_to(
        &named("B"),
        &opt(named("A")),
        &r
    ));
}

// ---------------------------------------------------------------------------
// Field-position subtyping observed through whole-record subtyping
// ---------------------------------------------------------------------------

#[test]
fn record_subtyping_through_optional_field() {
    let mut r = NodeTypeRegistry::new();
    // Src = {x: Crystal}, Dst = {x: Optional[HasAtoms]} — accepted (tag-only).
    r.add_record_type_def(def("Src", &[("x", DataType::Crystal)]))
        .unwrap();
    r.add_record_type_def(def("Dst", &[("x", opt(DataType::HasAtoms))]))
        .unwrap();
    assert!(DataType::can_be_converted_to(
        &named("Src"),
        &named("Dst"),
        &r
    ));

    // SrcI = {x: Int}, DstF = {x: Optional[Float]} — rejected (value-converting).
    r.add_record_type_def(def("SrcI", &[("x", DataType::Int)]))
        .unwrap();
    r.add_record_type_def(def("DstF", &[("x", opt(DataType::Float))]))
        .unwrap();
    assert!(!DataType::can_be_converted_to(
        &named("SrcI"),
        &named("DstF"),
        &r
    ));
}

// ---------------------------------------------------------------------------
// Parser & Display round-trip
// ---------------------------------------------------------------------------

#[test]
fn parses_and_displays_optional() {
    let t = DataType::from_string("Optional[Float]").unwrap();
    assert_eq!(t, opt(DataType::Float));
    assert_eq!(format!("{}", t), "Optional[Float]");
}

#[test]
fn optional_round_trips_through_display() {
    for inner in [
        DataType::Float,
        DataType::Crystal,
        DataType::Bool,
        named("Foo"),
        DataType::Array(Box::new(DataType::Int)),
    ] {
        let t = opt(inner);
        let s = format!("{}", t);
        let parsed = DataType::from_string(&s)
            .unwrap_or_else(|e| panic!("round-trip parse of {} failed: {}", s, e));
        assert_eq!(parsed, t, "round-trip mismatch for {}", s);
    }
}

#[test]
fn array_of_optional_is_well_formed() {
    // `[Optional[Int]]` is legal — only `Optional[..]` inner shapes are
    // restricted, the outer container is unconstrained.
    let t = DataType::from_string("[Optional[Int]]").unwrap();
    assert_eq!(t, DataType::Array(Box::new(opt(DataType::Int))));
}

#[test]
fn parser_rejects_ill_formed_optionals() {
    for s in [
        "Optional[Optional[Int]]",
        "Optional[Iter[Int]]",
        "Optional[Unit]",
        "Optional[None]",
    ] {
        assert!(
            DataType::from_string(s).is_err(),
            "{} should be rejected by the parser",
            s
        );
    }
}

#[test]
fn bare_optional_is_not_a_keyword() {
    // Bare `Optional` (not followed by `[`) is not the Optional keyword; it is
    // an unknown type name (record references must use `Record(..)` syntax).
    assert!(DataType::from_string("Optional").is_err());
    // `Record(Optional)` still resolves to a named-record reference — the
    // Optional keyword handling does not shadow it.
    assert_eq!(
        DataType::from_string("Record(Optional)").unwrap(),
        named("Optional")
    );
}

// ---------------------------------------------------------------------------
// canonicalize_data_type recursion
// ---------------------------------------------------------------------------

#[test]
fn canonicalize_recurses_into_optional() {
    // A non-canonical function (nested `Function` return) buried inside an
    // `Optional` must be flattened by `canonicalize_data_type`.
    let non_canonical = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Function(FunctionType {
            parameter_types: vec![DataType::Bool],
            output_type: Box::new(DataType::Float),
        })),
    });
    let mut t = opt(non_canonical);
    canonicalize_data_type(&mut t);

    let expected = opt(DataType::Function(FunctionType::new(
        vec![DataType::Int, DataType::Bool],
        DataType::Float,
    )));
    assert_eq!(t, expected);
}

// ---------------------------------------------------------------------------
// Registry validation of record defs (§3 enforcement)
// ---------------------------------------------------------------------------

#[test]
fn add_record_def_accepts_well_formed_optional_field() {
    let mut r = NodeTypeRegistry::new();
    assert!(
        r.add_record_type_def(def("Settings", &[("margin", opt(DataType::Float))]))
            .is_ok()
    );
}

#[test]
fn add_record_def_rejects_ill_formed_optional_field() {
    let mut r = NodeTypeRegistry::new();
    let err = r
        .add_record_type_def(def("Bad", &[("f", opt(opt(DataType::Int)))]))
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::IllFormedType(_, _, _)));

    // Ill-formed nested anywhere in the field type (here inside an array).
    let err = r
        .add_record_type_def(def(
            "Bad2",
            &[("f", DataType::Array(Box::new(opt(DataType::Unit))))],
        ))
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::IllFormedType(_, _, _)));
}

#[test]
fn update_record_def_rejects_ill_formed_optional_field() {
    let mut r = NodeTypeRegistry::new();
    r.add_record_type_def(def("Settings", &[("margin", opt(DataType::Float))]))
        .unwrap();
    let err = r
        .update_record_type_def(
            "Settings",
            vec![("margin".to_string(), opt(DataType::Unit))],
        )
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::IllFormedType(_, _, _)));
}

#[test]
fn validate_record_type_defs_detects_smuggled_ill_formed_optional() {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::validate_record_type_defs;
    let mut r = NodeTypeRegistry::new();
    // Bypass the guarded add path (as a hand-edited `.cnnd` would) by inserting
    // directly into the registry map.
    r.record_type_defs.insert(
        "Bad".to_string(),
        def("Bad", &[("f", opt(opt(DataType::Int)))]),
    );
    let errors = validate_record_type_defs(&r);
    assert!(
        errors.iter().any(|e| e.contains("ill-formed")),
        "expected an ill-formed-type error, got: {:?}",
        errors
    );
}

#[test]
fn cycle_detection_traverses_optional() {
    let mut r = NodeTypeRegistry::new();
    // A self-reference via `Optional[Record(A)]` is still a cycle.
    let err = r
        .add_record_type_def(def("A", &[("self", opt(named("A")))]))
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::CycleDetected { .. }));
}
