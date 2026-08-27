//! P1 of `doc/design_isosurface_node.md`: the `IsosurfaceData` value,
//! `DataType::Isosurface` and `NetworkResult::Isosurface` plumbing.
//!
//! Two tests here carry weight that the compiler does not:
//!
//! - `infer_data_type_reports_isosurface` — `NetworkResult::infer_data_type`
//!   has a `_ => None` arm, so a missing variant arm compiles cleanly and
//!   silently mis-infers the pin type. It is the only site in this phase with
//!   no compiler backstop.
//! - `heap_bytes_counts_a_shared_field_once` — the `heap_bytes` match *is*
//!   exhaustive, so a missing arm fails the build, but a *wrong* arm is silent.
//!   An `IsosurfaceData` can reach the same `Arc` twice.

use atomcad_crystolecule::field::isosurface::{
    Colormap, IsosurfaceColoring, IsosurfaceData, LevelBasis,
};
use atomcad_crystolecule::field::{GridGeometry, SampledField, ScalarField};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_util::memory_size_estimator::MemorySizeEstimator;
use glam::Vec3;
use glam::f64::DVec3;
use std::sync::Arc;

/// A signed ramp field at 1 Å spacing, sized by `dims`.
fn ramp_field(dims: [usize; 3]) -> Arc<dyn ScalarField> {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [DVec3::X, DVec3::Y, DVec3::Z],
        dims,
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..dims[0] {
        for j in 0..dims[1] {
            for k in 0..dims[2] {
                samples.push(((i + j + k) as f32) - 3.0);
            }
        }
    }
    Arc::new(SampledField::new(grid, samples).expect("ramp field is well-formed"))
}

fn phase_data(field: Arc<dyn ScalarField>) -> IsosurfaceData {
    IsosurfaceData {
        field,
        level: 0.02,
        level_basis: LevelBasis::Absolute,
        coloring: IsosurfaceColoring::Phase {
            positive: Vec3::new(0.20, 0.40, 0.90),
            negative: Vec3::new(0.90, 0.30, 0.25),
        },
        alpha: 0.4,
    }
}

fn colormapped_data(surface: Arc<dyn ScalarField>, color: Arc<dyn ScalarField>) -> IsosurfaceData {
    IsosurfaceData {
        field: surface,
        level: 0.002,
        level_basis: LevelBasis::Absolute,
        coloring: IsosurfaceColoring::Field {
            field: color,
            range: (-0.05, 0.05),
            colormap: Colormap::BlueWhiteRed,
        },
        alpha: 1.0,
    }
}

#[test]
fn data_type_text_round_trip() {
    assert_eq!(DataType::Isosurface.to_string(), "Isosurface");
    assert_eq!(
        DataType::from_string("Isosurface"),
        Ok(DataType::Isosurface)
    );
    let round_tripped = DataType::from_string(&DataType::Isosurface.to_string());
    assert_eq!(round_tripped, Ok(DataType::Isosurface));
}

#[test]
fn isosurface_nests_in_structural_types() {
    let array = DataType::Array(Box::new(DataType::Isosurface));
    assert_eq!(array.to_string(), "[Isosurface]");
    assert_eq!(DataType::from_string("[Isosurface]"), Ok(array));

    let iter = DataType::Iterator(Box::new(DataType::Isosurface));
    assert_eq!(iter.to_string(), "Iter[Isosurface]");
    assert_eq!(DataType::from_string("Iter[Isosurface]"), Ok(iter));
}

#[test]
fn infer_data_type_reports_isosurface() {
    // The one site with no compiler backstop (`_ => None`).
    let result = NetworkResult::Isosurface(phase_data(ramp_field([2, 3, 4])));
    assert_eq!(
        result.infer_data_type(),
        Some(DataType::Isosurface),
        "NetworkResult::Isosurface must infer DataType::Isosurface"
    );
}

#[test]
fn isosurface_is_an_ordinary_concrete_type() {
    let registry = NodeTypeRegistry::new();

    assert!(!DataType::Isosurface.is_abstract());

    // Identity converts; nothing else does in either direction — in particular
    // there is no `ScalarField -> Isosurface` widening.
    assert!(DataType::can_be_converted_to(
        &DataType::Isosurface,
        &DataType::Isosurface,
        &registry
    ));
    for other in [
        DataType::ScalarField,
        DataType::Float,
        DataType::Molecule,
        DataType::Blueprint,
        DataType::Geometry2D,
    ] {
        assert!(
            !DataType::can_be_converted_to(&DataType::Isosurface, &other, &registry),
            "Isosurface must not convert to {other}"
        );
        assert!(
            !DataType::can_be_converted_to(&other, &DataType::Isosurface, &registry),
            "{other} must not convert to Isosurface"
        );
    }

    // The universal `T -> Unit` discard widening still applies.
    assert!(DataType::can_be_converted_to(
        &DataType::Isosurface,
        &DataType::Unit,
        &registry
    ));
}

#[test]
fn display_string_summarizes_level_and_coloring_mode() {
    let phase = NetworkResult::Isosurface(phase_data(ramp_field([2, 3, 4]))).to_display_string();
    assert!(phase.starts_with("Isosurface\n"), "got: {phase}");
    assert!(phase.contains("level:"), "got: {phase}");
    assert!(phase.contains("color:  phase"), "got: {phase}");
    assert!(phase.contains("field:  2x3x4"), "got: {phase}");
    // The enclosed fraction is what makes an isovalue legible on an unfamiliar
    // field, so the *hover* readout carries it - not only the verbose CLI one.
    // The wording is load-bearing: never "% of the electron density", which
    // would be false for an orbital amplitude.
    assert!(phase.contains("of ∫|v|"), "got: {phase}");

    let field = ramp_field([2, 3, 4]);
    let mapped =
        NetworkResult::Isosurface(colormapped_data(field.clone(), field)).to_display_string();
    assert!(mapped.contains("color:  colormap"), "got: {mapped}");
}

#[test]
fn detailed_string_reports_level_coloring_and_field_dims() {
    let detailed =
        NetworkResult::Isosurface(phase_data(ramp_field([2, 3, 4]))).to_detailed_string();
    assert!(detailed.starts_with("Isosurface:"), "got: {detailed}");
    assert!(detailed.contains("level:"), "got: {detailed}");
    assert!(detailed.contains("alpha:"), "got: {detailed}");
    assert!(detailed.contains("field: 2x3x4"), "got: {detailed}");
    assert!(detailed.contains("coloring: phase"), "got: {detailed}");

    let mapped = NetworkResult::Isosurface(colormapped_data(
        ramp_field([2, 3, 4]),
        ramp_field([5, 5, 5]),
    ))
    .to_detailed_string();
    assert!(mapped.contains("coloring: BlueWhiteRed"), "got: {mapped}");
    assert!(mapped.contains("color_field: 5x5x5"), "got: {mapped}");
    assert!(mapped.contains("range:"), "got: {mapped}");
    // Still a summary: the samples themselves are never printed.
    assert!(
        mapped.len() < 500,
        "detailed string is a summary, not a dump"
    );
}

/// Required by `doc/design_isosurface_node.md`: the value range is how a user
/// picks a workable `level`, since a `level` outside the field's range renders
/// nothing and is deliberately silent.
///
/// **This asserts `to_display_string`, not `to_detailed_string`,** and the
/// distinction is the whole point. The pin-hover tooltip renders
/// `NodeView.outputPinStrings`, which is built from `to_display_string`;
/// `to_detailed_string` is reachable only from the CLI/AI `evaluate_node
/// --verbose` path. An earlier version of this test asserted the detailed
/// string while its own doc comment claimed to be about pin hover — so the
/// design's stated prerequisite ("the value range then shows on pin hover")
/// was false in the GUI and the suite said nothing.
#[test]
fn scalar_field_hover_readout_carries_the_value_range() {
    let field = ramp_field([2, 3, 4]);
    let shown = NetworkResult::ScalarField(field.clone()).to_display_string();
    assert!(shown.contains("grid:   2 x 3 x 4"), "got: {shown}");
    assert!(shown.contains("values:"), "got: {shown}");
    assert!(field.value_range().is_some());
}

#[test]
fn heap_bytes_counts_a_shared_field_once() {
    // A field painted with itself: the same `Arc` reached twice.
    let field = ramp_field([12, 12, 12]);
    let shared = NetworkResult::Isosurface(colormapped_data(field.clone(), field.clone()));
    let bare = NetworkResult::ScalarField(field.clone());

    let shared_bytes = shared.estimate_memory_bytes();
    let bare_bytes = bare.estimate_memory_bytes();
    let field_bytes = field.estimate_memory_bytes();

    assert!(
        shared_bytes < bare_bytes + field_bytes,
        "a field used as both surface and color field must be counted once, got {shared_bytes} against a double-counted {}",
        bare_bytes + field_bytes
    );

    // Two *distinct* fields, by contrast, are both charged.
    let other = ramp_field([12, 12, 12]);
    let distinct = NetworkResult::Isosurface(colormapped_data(field.clone(), other));
    assert!(
        distinct.estimate_memory_bytes() > shared_bytes,
        "two distinct fields must cost more than one field used twice"
    );

    // And the shared case still charges the field it does hold.
    assert!(shared_bytes >= field_bytes);
}

#[test]
fn network_result_clone_shares_both_payloads() {
    let surface = ramp_field([4, 4, 4]);
    let color = ramp_field([4, 4, 4]);
    let original = NetworkResult::Isosurface(colormapped_data(surface, color));
    let cloned = original.clone();

    let (NetworkResult::Isosurface(a), NetworkResult::Isosurface(b)) = (&original, &cloned) else {
        panic!("both must be Isosurface results");
    };
    assert!(
        Arc::ptr_eq(&a.field, &b.field),
        "clone must share the surface field's Arc"
    );
    let (IsosurfaceColoring::Field { field: ca, .. }, IsosurfaceColoring::Field { field: cb, .. }) =
        (&a.coloring, &b.coloring)
    else {
        panic!("both must carry a color field");
    };
    assert!(
        Arc::ptr_eq(ca, cb),
        "clone must share the color field's Arc too"
    );
}

#[test]
fn colormap_defaults_to_blue_white_red_and_round_trips_through_serde() {
    assert_eq!(Colormap::default(), Colormap::BlueWhiteRed);
    // `Colormap` is node data that lands in the `.cnnd`, so its serialized form
    // matters even before a node produces one.
    let json = serde_json::to_string(&Colormap::BlueWhiteRed).expect("serializes");
    assert_eq!(json, "\"BlueWhiteRed\"");
    let back: Colormap = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, Colormap::BlueWhiteRed);
}
