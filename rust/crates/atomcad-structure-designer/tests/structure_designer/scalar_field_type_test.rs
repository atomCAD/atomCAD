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

/// The pin-hover readout. It is a **block**, not a one-liner: a scalar field
/// carries no semantic tag, so none of these facts can be inferred from the
/// others, and this is the only surface a GUI user has — `to_detailed_string`
/// is reachable only from the CLI/AI `--verbose` path.
#[test]
fn display_string_reports_the_whole_shape_without_dumping_samples() {
    let shown = ramp_result().to_display_string();

    assert!(shown.starts_with("ScalarField"), "got: {shown}");
    assert!(
        shown.contains("grid:   2 x 3 x 4 samples (24)"),
        "got: {shown}"
    );
    // 1 Å axes.
    assert!(
        shown.contains("step:   1.000 x 1.000 x 1.000 A"),
        "got: {shown}"
    );
    // Node-centered bounds: 2x3x4 samples span 1x2x3 steps, NOT 2x3x4.
    assert!(
        shown.contains("extent: 1.00 x 2.00 x 3.00 A"),
        "got: {shown}"
    );
    assert!(
        shown.contains("box:    (0.00, 0.00, 0.00) .. (1.00, 2.00, 3.00)"),
        "got: {shown}"
    );
    // Ramp min/max: value(0,0,0) = 0, value(1,2,3) = 123. Printed plainly
    // rather than as `0.0000e0 .. 1.2300e2` — scientific notation is for the
    // magnitudes that need it, and reading `1.2300e2` as 123 is work the reader
    // should not have to do (`atomcad_util::number_format`).
    assert!(shown.contains("values: 0 .. 123"), "got: {shown}");
    assert!(shown.contains("memory:"), "got: {shown}");

    // An axis-aligned grid must NOT be flagged as sheared.
    assert!(!shown.contains("sheared"), "got: {shown}");
    // No description was attached, so no stray blank line stands in for one.
    assert_eq!(shown.lines().count(), 7, "got: {shown}");

    // The samples themselves are never printed. The tooltip caps at 15 lines /
    // 500 chars, and a readout that overflows it loses its own last line.
    assert!(
        shown.len() < 400,
        "the readout is a summary, not a dump: {shown}"
    );
    assert!(
        shown.lines().count() <= 15,
        "the tooltip truncates past 15 lines"
    );
}

/// A non-negative field says so, because it has no negative lobe — the
/// extractor skips a whole sign pass and the reader should expect one surface
/// rather than two.
#[test]
fn display_string_names_the_signedness() {
    assert!(
        ramp_result().to_display_string().contains("(non-negative)"),
        "the ramp runs 0..123"
    );

    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims: [2, 2, 2],
    };
    let signed = SampledField::new(grid, vec![-1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0])
        .expect("well-formed");
    assert!(
        NetworkResult::ScalarField(Arc::new(signed))
            .to_display_string()
            .contains("(signed)")
    );
}

/// `GridGeometry::spacing` returns three axis *lengths* and is exact only for
/// an axis-aligned grid — the `.cube` format permits shear. The lengths are
/// still printed (they are the steps along the axes) but the reader is told
/// they do not describe a box.
#[test]
fn display_string_flags_a_sheared_grid() {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::new(0.5, 1.0, 0.0), DVec3::Z],
        dims: [2, 2, 2],
    };
    let sheared = SampledField::new(grid, vec![0.0; 8]).expect("well-formed");
    assert!(
        NetworkResult::ScalarField(Arc::new(sheared))
            .to_display_string()
            .contains("sheared - axes are not orthogonal")
    );
}

/// A description is the only thing that says *what* a field is — the values
/// alone cannot distinguish an orbital amplitude from a density. It renders
/// directly under the header, and its absence leaves no blank line.
#[test]
fn display_string_carries_the_description_when_there_is_one() {
    let described = ramp_field().with_description("Total SCF Density\n  MO 5 (HOMO)");
    let shown = NetworkResult::ScalarField(Arc::new(described)).to_display_string();

    // Blank/whitespace lines are dropped and the rest joined, so a `.cube` whose
    // second comment line is empty does not produce a dangling separator.
    assert!(
        shown.contains("Total SCF Density - MO 5 (HOMO)"),
        "got: {shown}"
    );
    assert_eq!(
        shown.lines().nth(1).map(str::trim),
        Some("Total SCF Density - MO 5 (HOMO)"),
        "the description sits directly under the header: {shown}"
    );
}

/// `to_detailed_string` is the same builder plus the grid's origin and axis
/// vectors. Sharing one source is what stops the CLI readout and the tooltip
/// drifting apart.
#[test]
fn detailed_string_adds_origin_and_axes_to_the_same_block() {
    let detailed = ramp_result().to_detailed_string();
    let shown = ramp_result().to_display_string();

    // Everything the tooltip shows is here too.
    for line in shown.lines() {
        assert!(
            detailed.contains(line.trim()),
            "detailed dropped {line:?}: {detailed}"
        );
    }
    assert!(
        detailed.contains("origin: (0.000000, 0.000000, 0.000000)"),
        "got: {detailed}"
    );
    assert!(
        detailed.contains("axis a: (1.000000, 0.000000, 0.000000)"),
        "got: {detailed}"
    );
    assert!(
        detailed.contains("axis b: (0.000000, 1.000000, 0.000000)"),
        "got: {detailed}"
    );
    assert!(
        detailed.contains("axis c: (0.000000, 0.000000, 1.000000)"),
        "got: {detailed}"
    );

    let range = ramp_field()
        .value_range()
        .expect("sampled field has a range");
    assert_eq!(range, (0.0, 123.0));

    // The samples themselves are never printed.
    assert!(
        detailed.len() < 600,
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
