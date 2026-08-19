//! P2 of `doc/design_scalar_fields.md`: `DataType::ScalarField` and
//! `NetworkResult::ScalarField` plumbing.
//!
//! The load-bearing test here is `infer_data_type_reports_scalar_field`.
//! `NetworkResult::infer_data_type` has a `_ => None` arm, so a missing variant
//! arm compiles cleanly and silently mis-infers the type — it is the one site in
//! this phase with no compiler backstop.

use atomcad_crystolecule::field::{GridGeometry, SampledField, ScalarField};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use std::sync::Arc;

/// A tiny 2x3x4 ramp field, `value(i, j, k) = 100*i + 10*j + k`, at 1 Å spacing.
fn ramp_field() -> SampledField {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [2, 3, 4],
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..2 {
        for j in 0..3 {
            for k in 0..4 {
                samples.push((100 * i + 10 * j + k) as f32);
            }
        }
    }
    SampledField::new(grid, samples).expect("ramp field is well-formed")
}

fn ramp_result() -> NetworkResult {
    NetworkResult::ScalarField(Arc::new(ramp_field()))
}

#[test]
fn data_type_text_round_trip() {
    assert_eq!(DataType::ScalarField.to_string(), "ScalarField");
    assert_eq!(
        DataType::from_string("ScalarField"),
        Ok(DataType::ScalarField)
    );
    // Round-trip through the string form, the shape the text format stores.
    let round_tripped = DataType::from_string(&DataType::ScalarField.to_string());
    assert_eq!(round_tripped, Ok(DataType::ScalarField));
}

#[test]
fn scalar_field_nests_in_structural_types() {
    // Not a special case for the parser: `[ScalarField]` and `Iter[ScalarField]`
    // parse and print like any other element type.
    let array = DataType::Array(Box::new(DataType::ScalarField));
    assert_eq!(array.to_string(), "[ScalarField]");
    assert_eq!(DataType::from_string("[ScalarField]"), Ok(array));

    let iter = DataType::Iterator(Box::new(DataType::ScalarField));
    assert_eq!(iter.to_string(), "Iter[ScalarField]");
    assert_eq!(DataType::from_string("Iter[ScalarField]"), Ok(iter));
}

#[test]
fn infer_data_type_reports_scalar_field() {
    // The one site with no compiler backstop (`_ => None`).
    assert_eq!(
        ramp_result().infer_data_type(),
        Some(DataType::ScalarField),
        "NetworkResult::ScalarField must infer DataType::ScalarField"
    );
}

#[test]
fn scalar_field_is_an_ordinary_concrete_type() {
    let registry = NodeTypeRegistry::new();

    // Not abstract: no pie-sliced pin rendering, no satisfier set.
    assert!(!DataType::ScalarField.is_abstract());

    // Identity converts; nothing else does in either direction.
    assert!(DataType::can_be_converted_to(
        &DataType::ScalarField,
        &DataType::ScalarField,
        &registry
    ));
    for other in [
        DataType::Float,
        DataType::Molecule,
        DataType::Crystal,
        DataType::Blueprint,
        DataType::Structure,
        DataType::Motif,
    ] {
        assert!(
            !DataType::can_be_converted_to(&DataType::ScalarField, &other, &registry),
            "ScalarField must not convert to {other}"
        );
        assert!(
            !DataType::can_be_converted_to(&other, &DataType::ScalarField, &registry),
            "{other} must not convert to ScalarField"
        );
    }

    // The universal `T -> Unit` discard widening still applies — it applies to
    // every type, so exempting ScalarField would be the special case.
    assert!(DataType::can_be_converted_to(
        &DataType::ScalarField,
        &DataType::Unit,
        &registry
    ));
}

#[test]
fn display_string_summarizes_without_dumping_samples() {
    let shown = ramp_result().to_display_string();
    assert_eq!(shown, "ScalarField 2x3x4");
}

#[test]
fn detailed_string_reports_grid_and_value_range() {
    let detailed = ramp_result().to_detailed_string();
    assert!(detailed.starts_with("ScalarField:"), "got: {detailed}");
    assert!(detailed.contains("dims: 2x3x4"), "got: {detailed}");
    // Ramp min/max: value(0,0,0) = 0, value(1,2,3) = 123.
    assert!(detailed.contains("value_range:"), "got: {detailed}");
    let range = ramp_field()
        .value_range()
        .expect("sampled field has a range");
    assert_eq!(range, (0.0, 123.0));
    // The samples themselves are never printed.
    assert!(
        detailed.len() < 400,
        "detailed string is a summary, not a dump"
    );
}

#[test]
fn network_result_clone_shares_the_payload() {
    // `Arc` is the point of the variant: cloning a result during evaluation must
    // not deep-copy megabytes of samples.
    let original = ramp_result();
    let cloned = original.clone();
    let (NetworkResult::ScalarField(a), NetworkResult::ScalarField(b)) = (&original, &cloned)
    else {
        panic!("both must be ScalarField results");
    };
    assert!(Arc::ptr_eq(a, b), "clone must share the Arc, not deep-copy");
}
