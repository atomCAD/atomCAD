//! Phase 4 tests for record types (see `doc/design_record_types.md`).
//!
//! Phase 4 covers structural subtyping for records:
//! - `is_tag_only_widening` predicate (extracted from the previous
//!   abstract-upcast block in `can_be_converted_to`).
//! - Full record arm in `can_be_converted_to`: width subtyping (extra fields
//!   on src ignored), structural depth (each declared dst field checked
//!   recursively), tag-only widenings at field level only (no `Int → Float`
//!   etc.), anonymous-named compatibility in either direction.
//! - `can_be_structurally_converted_to`: leaf positions admit only tag-only
//!   widenings; arrays recurse element-wise under the same strict rule;
//!   records delegate back to `can_be_converted_to` (whose record arm uses
//!   the strict variant for field-level checks).

use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, RecordType, can_be_structurally_converted_to, is_tag_only_widening,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};

// ---------------------------------------------------------------------------
// Test fixtures: a registry pre-populated with the named defs from the
// "Subtyping Examples" table in the design doc.
// ---------------------------------------------------------------------------

fn def(name: &str, fields: &[(&str, DataType)]) -> RecordTypeDef {
    RecordTypeDef {
        name: name.to_string(),
        fields: fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    }
}

fn examples_registry() -> NodeTypeRegistry {
    let mut r = NodeTypeRegistry::new();
    // Point = {x: Int, y: Int}
    r.add_record_type_def(def("Point", &[("x", DataType::Int), ("y", DataType::Int)]))
        .expect("Point");
    // Point3 = {x: Int, y: Int, z: Int}
    r.add_record_type_def(def(
        "Point3",
        &[
            ("x", DataType::Int),
            ("y", DataType::Int),
            ("z", DataType::Int),
        ],
    ))
    .expect("Point3");
    // PointF = {x: Float, y: Float}
    r.add_record_type_def(def(
        "PointF",
        &[("x", DataType::Float), ("y", DataType::Float)],
    ))
    .expect("PointF");
    // Box = {p: Point3}
    r.add_record_type_def(def(
        "Box",
        &[(
            "p",
            DataType::Record(RecordType::Named("Point3".to_string())),
        )],
    ))
    .expect("Box");
    // BoxXY = {p: Point}
    r.add_record_type_def(def(
        "BoxXY",
        &[(
            "p",
            DataType::Record(RecordType::Named("Point".to_string())),
        )],
    ))
    .expect("BoxXY");
    // Tagged = {a: Crystal, label: String}
    r.add_record_type_def(def(
        "Tagged",
        &[("a", DataType::Crystal), ("label", DataType::String)],
    ))
    .expect("Tagged");
    // Abstract = {a: HasAtoms, label: String}
    r.add_record_type_def(def(
        "Abstract",
        &[("a", DataType::HasAtoms), ("label", DataType::String)],
    ))
    .expect("Abstract");
    // Foo = {x: Int, y: Int} — same shape as Point, different name. Used by the
    // "structural — names ignored" example row.
    r.add_record_type_def(def("Foo", &[("x", DataType::Int), ("y", DataType::Int)]))
        .expect("Foo");
    r
}

fn rec_named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn rec_anon(fields: Vec<(&str, DataType)>) -> DataType {
    DataType::Record(RecordType::anonymous(
        fields
            .into_iter()
            .map(|(n, t)| (n.to_string(), t))
            .collect(),
    ))
}

fn arr(t: DataType) -> DataType {
    DataType::Array(Box::new(t))
}

// ---------------------------------------------------------------------------
// is_tag_only_widening: identity + every phase-upcast edge.
// ---------------------------------------------------------------------------

#[test]
fn is_tag_only_widening_identity_holds_for_every_kind() {
    // Spot-check identity at primitives, abstracts, and a record/array shape.
    let cases = [
        DataType::None,
        DataType::Bool,
        DataType::Int,
        DataType::Float,
        DataType::Vec3,
        DataType::IMat3,
        DataType::Mat3,
        DataType::Crystal,
        DataType::Molecule,
        DataType::Blueprint,
        DataType::HasAtoms,
        DataType::HasStructure,
        DataType::HasFreeLinOps,
        DataType::Structure,
        DataType::Motif,
    ];
    for t in &cases {
        assert!(
            is_tag_only_widening(t, t),
            "identity should hold for {:?}",
            t
        );
    }
}

#[test]
fn is_tag_only_widening_accepts_every_phase_upcast_edge() {
    let edges = [
        (DataType::Crystal, DataType::HasAtoms),
        (DataType::Crystal, DataType::HasStructure),
        (DataType::Molecule, DataType::HasAtoms),
        (DataType::Molecule, DataType::HasFreeLinOps),
        (DataType::Blueprint, DataType::HasStructure),
        (DataType::Blueprint, DataType::HasFreeLinOps),
    ];
    for (src, dst) in &edges {
        assert!(
            is_tag_only_widening(src, dst),
            "{:?} → {:?} should be a tag-only widening",
            src,
            dst
        );
    }
}

#[test]
fn is_tag_only_widening_rejects_value_converting_widenings() {
    // These are accepted by `can_be_converted_to` but must never be admitted
    // at a record field position.
    let pairs = [
        (DataType::Int, DataType::Float),
        (DataType::Float, DataType::Int),
        (DataType::IVec2, DataType::Vec2),
        (DataType::Vec2, DataType::IVec2),
        (DataType::IVec3, DataType::Vec3),
        (DataType::Vec3, DataType::IVec3),
        (DataType::IMat3, DataType::Mat3),
        (DataType::Mat3, DataType::IMat3),
        (DataType::LatticeVecs, DataType::DrawingPlane),
    ];
    for (src, dst) in &pairs {
        assert!(
            !is_tag_only_widening(src, dst),
            "{:?} → {:?} is value-converting and must be rejected",
            src,
            dst
        );
    }
}

#[test]
fn is_tag_only_widening_rejects_abstract_to_concrete_and_cross_abstract() {
    // No abstract → concrete.
    assert!(!is_tag_only_widening(
        &DataType::HasAtoms,
        &DataType::Crystal
    ));
    assert!(!is_tag_only_widening(
        &DataType::HasFreeLinOps,
        &DataType::Molecule
    ));
    // No cross-abstract.
    assert!(!is_tag_only_widening(
        &DataType::HasAtoms,
        &DataType::HasStructure
    ));
}

// ---------------------------------------------------------------------------
// Subtyping table from the design doc — one assertion per row.
// ---------------------------------------------------------------------------

#[test]
fn record_point3_to_point_width_subtyping_succeeds() {
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &rec_named("Point3"),
        &rec_named("Point"),
        &r,
    ));
}

#[test]
fn record_point_to_point3_missing_field_rejected() {
    let r = examples_registry();
    assert!(!DataType::can_be_converted_to(
        &rec_named("Point"),
        &rec_named("Point3"),
        &r,
    ));
}

#[test]
fn record_point_to_pointf_value_converting_widening_rejected_at_field_level() {
    // Int → Float is permitted at the top level by `can_be_converted_to` but
    // must NOT be admitted at a record field position.
    let r = examples_registry();
    assert!(!DataType::can_be_converted_to(
        &rec_named("Point"),
        &rec_named("PointF"),
        &r,
    ));
}

#[test]
fn record_box_to_boxxy_depth_plus_width() {
    // Box.p is Point3, BoxXY.p is Point; Point3 <: Point by width, so
    // Box <: BoxXY by depth.
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &rec_named("Box"),
        &rec_named("BoxXY"),
        &r,
    ));
}

#[test]
fn array_of_point3_to_array_of_point_element_wise() {
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &arr(rec_named("Point3")),
        &arr(rec_named("Point")),
        &r,
    ));
}

#[test]
fn record_tagged_to_abstract_tag_only_widening_at_field_level() {
    // Crystal → HasAtoms is a tag-only widening — admissible at a field.
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &rec_named("Tagged"),
        &rec_named("Abstract"),
        &r,
    ));
}

#[test]
fn array_of_tagged_to_array_of_abstract_tag_only_through_array() {
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &arr(rec_named("Tagged")),
        &arr(rec_named("Abstract")),
        &r,
    ));
}

#[test]
fn anonymous_record_molecule_field_to_has_free_lin_ops_field() {
    // Anonymous record forms participate in tag-only widening too.
    let r = examples_registry();
    let src = rec_anon(vec![("a", DataType::Molecule)]);
    let dst = rec_anon(vec![("a", DataType::HasFreeLinOps)]);
    assert!(DataType::can_be_converted_to(&src, &dst, &r));
}

#[test]
fn named_to_anonymous_with_matching_schema_is_structural() {
    // Foo = {x: Int, y: Int} (named) <: {x: Int, y: Int} (anonymous).
    // Names are ignored — the design's whole-table rule.
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &rec_named("Foo"),
        &rec_anon(vec![("x", DataType::Int), ("y", DataType::Int)]),
        &r,
    ));
}

#[test]
fn anonymous_to_named_with_matching_schema_is_structural() {
    let r = examples_registry();
    assert!(DataType::can_be_converted_to(
        &rec_anon(vec![("x", DataType::Int), ("y", DataType::Int)]),
        &rec_named("Point"),
        &r,
    ));
}

// ---------------------------------------------------------------------------
// Empty record `{}` is the top of the lattice.
// ---------------------------------------------------------------------------

#[test]
fn every_record_is_assignable_to_empty_anonymous_record() {
    let r = examples_registry();
    let empty = rec_anon(vec![]);
    // Named records.
    for n in ["Point", "Point3", "PointF", "Box", "Tagged", "Abstract"] {
        assert!(
            DataType::can_be_converted_to(&rec_named(n), &empty, &r),
            "{} should be <: {{}}",
            n
        );
    }
    // Anonymous record.
    let anon = rec_anon(vec![("x", DataType::Int), ("y", DataType::Float)]);
    assert!(DataType::can_be_converted_to(&anon, &empty, &r));
    // Through arrays.
    assert!(DataType::can_be_converted_to(
        &arr(rec_named("Point")),
        &arr(empty.clone()),
        &r,
    ));
    // Empty to empty (identity short-circuit, but assert explicitly).
    assert!(DataType::can_be_converted_to(&empty, &empty, &r));
}

#[test]
fn empty_record_assignable_across_named_and_anonymous_shapes() {
    // An empty record can be expressed both as `Anonymous(vec![])` and via a
    // named def with zero fields. Both forms must be top.
    let mut r = examples_registry();
    r.add_record_type_def(def("Empty", &[])).expect("Empty");
    // Any record converts to either form.
    assert!(DataType::can_be_converted_to(
        &rec_named("Point"),
        &rec_named("Empty"),
        &r,
    ));
    assert!(DataType::can_be_converted_to(
        &rec_named("Point"),
        &rec_anon(vec![]),
        &r,
    ));
    // And the two empty forms are inter-assignable.
    assert!(DataType::can_be_converted_to(
        &rec_named("Empty"),
        &rec_anon(vec![]),
        &r,
    ));
    assert!(DataType::can_be_converted_to(
        &rec_anon(vec![]),
        &rec_named("Empty"),
        &r,
    ));
}

// ---------------------------------------------------------------------------
// Dangling references are incompatible with anything.
// ---------------------------------------------------------------------------

#[test]
fn dangling_named_reference_is_incompatible_with_anything() {
    let r = examples_registry();
    let dangling = rec_named("DoesNotExist");
    let point = rec_named("Point");
    let empty = rec_anon(vec![]);
    // Dangling on either side rejects.
    assert!(!DataType::can_be_converted_to(&dangling, &point, &r));
    assert!(!DataType::can_be_converted_to(&point, &dangling, &r));
    // Even toward the top-of-lattice empty record.
    assert!(!DataType::can_be_converted_to(&dangling, &empty, &r));
    // Same-named dangling pair still compatible by the same-name short-circuit
    // (two `Named(n)` resolve to the same def by definition; absence is the
    // same on both sides so they remain trivially equal).
    let dangling2 = rec_named("DoesNotExist");
    assert!(DataType::can_be_converted_to(&dangling, &dangling2, &r));
}

// ---------------------------------------------------------------------------
// can_be_structurally_converted_to: rejects non-record/non-array widenings at
// leaf positions; arrays recurse strictly; records delegate through.
// ---------------------------------------------------------------------------

#[test]
fn structural_variant_rejects_int_to_float_at_leaf() {
    let r = examples_registry();
    assert!(!can_be_structurally_converted_to(
        &DataType::Int,
        &DataType::Float,
        &r,
    ));
}

#[test]
fn structural_variant_accepts_phase_upcast_at_leaf() {
    let r = examples_registry();
    assert!(can_be_structurally_converted_to(
        &DataType::Crystal,
        &DataType::HasAtoms,
        &r,
    ));
}

#[test]
fn structural_variant_recurses_into_arrays_strictly() {
    let r = examples_registry();
    // Array of int → array of float must be rejected even though the relaxed
    // `can_be_converted_to` accepts it.
    assert!(!can_be_structurally_converted_to(
        &arr(DataType::Int),
        &arr(DataType::Float),
        &r,
    ));
    // Array of crystal → array of has-atoms is admitted (tag-only).
    assert!(can_be_structurally_converted_to(
        &arr(DataType::Crystal),
        &arr(DataType::HasAtoms),
        &r,
    ));
}

#[test]
fn structural_variant_rejects_single_value_to_array_broadcast() {
    let r = examples_registry();
    // The relaxed form accepts T → [T]; the strict form must not.
    assert!(!can_be_structurally_converted_to(
        &DataType::Int,
        &arr(DataType::Int),
        &r,
    ));
}

// ---------------------------------------------------------------------------
// Refactor regression: the existing `data_type_test.rs` matrix tests already
// exercise the abstract-upcast arm via `can_be_converted_to`. Re-run a few
// representative cases here to lock in zero-behavior-delta after the
// `is_tag_only_widening` extraction (the original test file is unchanged so
// it also keeps running).
// ---------------------------------------------------------------------------

#[test]
fn refactor_regression_phase_upcasts_still_admitted_through_can_be_converted_to() {
    let r = NodeTypeRegistry::new();
    let edges = [
        (DataType::Crystal, DataType::HasAtoms),
        (DataType::Crystal, DataType::HasStructure),
        (DataType::Molecule, DataType::HasAtoms),
        (DataType::Molecule, DataType::HasFreeLinOps),
        (DataType::Blueprint, DataType::HasStructure),
        (DataType::Blueprint, DataType::HasFreeLinOps),
    ];
    for (src, dst) in &edges {
        assert!(
            DataType::can_be_converted_to(src, dst, &r),
            "{:?} → {:?} should still be admitted post-refactor",
            src,
            dst
        );
    }
    // Non-record value-converting widenings still admitted by the relaxed
    // form (only the strict variant rejects them).
    assert!(DataType::can_be_converted_to(
        &DataType::Int,
        &DataType::Float,
        &r,
    ));
    assert!(DataType::can_be_converted_to(
        &DataType::IVec3,
        &DataType::Vec3,
        &r,
    ));
    // No abstract → concrete.
    assert!(!DataType::can_be_converted_to(
        &DataType::HasAtoms,
        &DataType::Crystal,
        &r,
    ));
}

// ---------------------------------------------------------------------------
// Defensive: a record whose dst field is nested record requires the same
// width-subtyping at the inner level (no scalar promotion through depth).
// ---------------------------------------------------------------------------

#[test]
fn nested_record_field_still_uses_strict_field_check() {
    // {p: Point3, label: String} → {p: PointF}
    // Width works (label is dropped) but Point3 → PointF would require
    // Int→Float at the inner field level, which is rejected.
    let r = examples_registry();
    let src = rec_anon(vec![
        ("label", DataType::String),
        ("p", rec_named("Point3")),
    ]);
    let dst = rec_anon(vec![("p", rec_named("PointF"))]);
    assert!(!DataType::can_be_converted_to(&src, &dst, &r));
}

#[test]
fn extra_fields_on_source_are_ignored_under_width_subtyping() {
    // Anonymous {x: Int, y: Int, z: Int} → Point ({x, y}).
    let r = examples_registry();
    let src = rec_anon(vec![
        ("x", DataType::Int),
        ("y", DataType::Int),
        ("z", DataType::Int),
    ]);
    assert!(DataType::can_be_converted_to(&src, &rec_named("Point"), &r));
}

#[test]
fn record_arm_does_not_admit_function_partial_application_inside_field() {
    // Function partial application is admitted by `can_be_converted_to` at
    // the top level but must not leak into a record field via the strict
    // variant. (Functions never carry abstract phase types, so the strict
    // form's leaf rule rejects all non-identity function pairs.)
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;
    let r = examples_registry();
    let f1 = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int, DataType::Int],
        output_type: Box::new(DataType::Int),
    });
    let f2 = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Int),
    });
    // Top-level: partial application allowed.
    assert!(DataType::can_be_converted_to(&f1, &f2, &r));
    // Inside a record field: rejected by the strict variant.
    let src = rec_anon(vec![("f", f1)]);
    let dst = rec_anon(vec![("f", f2)]);
    assert!(!DataType::can_be_converted_to(&src, &dst, &r));
}
