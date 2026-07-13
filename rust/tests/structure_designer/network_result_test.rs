use glam::{DVec2, IVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::drawing_plane::DrawingPlane;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    CrystalData, GeometrySummary2D, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::util::transform::Transform2D;

fn make_crystal() -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: AtomicStructure::new(),
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

fn make_molecule() -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: AtomicStructure::new(),
        geo_tree_root: None,
    })
}

#[test]
fn infer_data_type_crystal() {
    assert_eq!(make_crystal().infer_data_type(), Some(DataType::Crystal));
}

#[test]
fn infer_data_type_molecule() {
    assert_eq!(make_molecule().infer_data_type(), Some(DataType::Molecule));
}

#[test]
fn extract_atomic_accepts_crystal_and_molecule() {
    assert!(make_crystal().extract_atomic().is_some());
    assert!(make_molecule().extract_atomic().is_some());
    assert!(NetworkResult::Int(42).extract_atomic().is_none());
    assert!(NetworkResult::None.extract_atomic().is_none());
}

#[test]
fn extract_crystal_only_matches_crystal() {
    assert!(make_crystal().extract_crystal().is_some());
    assert!(make_molecule().extract_crystal().is_none());
    assert!(NetworkResult::Int(1).extract_crystal().is_none());
}

#[test]
fn extract_molecule_only_matches_molecule() {
    assert!(make_molecule().extract_molecule().is_some());
    assert!(make_crystal().extract_molecule().is_none());
    assert!(NetworkResult::Int(1).extract_molecule().is_none());
}

// --- construction_plane ---------------------------------------------------
//
// `construction_plane()` backs the view-up "from displayed plane" action
// (issue #349). Unlike `extract_drawing_plane`, it must also reach into a
// `Geometry2D`'s embedded plane — a `rect`/`circle` output carries the same
// plane its downstream `extrude` reads, so the action has to find it there too.

/// A drawing plane with a distinctive `center` so a returned plane can be
/// proven to be *this* one (embedded pass-through), not a coincidental default.
fn plane_with_marker() -> DrawingPlane {
    let mut dp = DrawingPlane::default();
    dp.center = IVec3::new(3, 4, 5);
    dp
}

#[test]
fn construction_plane_from_drawing_plane_result() {
    let result = NetworkResult::DrawingPlane(plane_with_marker());
    let plane = result.construction_plane().expect("DrawingPlane has a plane");
    assert_eq!(plane.center, IVec3::new(3, 4, 5));
}

#[test]
fn construction_plane_reaches_into_geometry2d() {
    // The bug (issue #349 follow-up): a rect node's Geometry2D output dropped
    // its plane at the scene level, so "from displayed plane" failed on it even
    // though extrude could read the same plane.
    let result = NetworkResult::Geometry2D(GeometrySummary2D {
        drawing_plane: plane_with_marker(),
        frame_transform: Transform2D::new(DVec2::ZERO, 0.0),
        geo_tree_root: GeoNode::circle(DVec2::ZERO, 1.0),
    });
    let plane = result
        .construction_plane()
        .expect("Geometry2D carries an embedded plane");
    assert_eq!(plane.center, IVec3::new(3, 4, 5));
}

#[test]
fn construction_plane_none_for_non_geometry() {
    assert!(NetworkResult::Int(1).construction_plane().is_none());
    assert!(make_crystal().construction_plane().is_none());
    assert!(NetworkResult::None.construction_plane().is_none());
}

// --- to_display_string_capped --------------------------------------------
//
// Tooltip-side helper that truncates `Array` (and arrays nested inside
// `Record` fields) so the per-pin display string can't explode on large or
// deeply nested arrays. Non-array variants delegate to `to_display_string`
// and must render identically.

fn ints(values: &[i32]) -> NetworkResult {
    NetworkResult::Array(values.iter().map(|&v| NetworkResult::Int(v)).collect())
}

#[test]
fn capped_array_short_renders_in_full() {
    // len <= cap → no `...` suffix, identical to the uncapped rendering.
    let arr = ints(&[1, 2, 3, 4, 5]);
    assert_eq!(arr.to_display_string_capped(20), "[1, 2, 3, 4, 5]");
    assert_eq!(arr.to_display_string_capped(20), arr.to_display_string());
}

#[test]
fn capped_array_at_cap_renders_in_full() {
    // len == cap → show all, no truncation marker.
    let values: Vec<i32> = (0..20).collect();
    let arr = ints(&values);
    assert!(!arr.to_display_string_capped(20).contains("..."));
}

#[test]
fn capped_array_over_cap_truncates_with_ellipsis() {
    // len > cap → first `cap` elements followed by `, ...]`.
    let values: Vec<i32> = (0..25).collect();
    let arr = ints(&values);
    let expected = format!(
        "[{}, ...]",
        (0..20)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert_eq!(arr.to_display_string_capped(20), expected);
}

#[test]
fn capped_array_recurses_into_nested_arrays() {
    // The cap must apply to inner arrays too — otherwise a single tooltip
    // could still print thousands of inner elements.
    let inner: Vec<i32> = (0..30).collect();
    let outer = NetworkResult::Array(vec![ints(&inner), ints(&inner)]);
    let inner_capped = format!(
        "[{}, ...]",
        (0..5).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
    );
    assert_eq!(
        outer.to_display_string_capped(5),
        format!("[{}, {}]", inner_capped, inner_capped)
    );
}

#[test]
fn capped_record_field_recurses() {
    // Arrays nested inside record fields also get capped.
    let arr = ints(&(0..30).collect::<Vec<i32>>());
    let record = NetworkResult::record(vec![("xs".to_string(), arr)]);
    let arr_capped = format!(
        "[{}, ...]",
        (0..5).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
    );
    assert_eq!(
        record.to_display_string_capped(5),
        format!("{{xs: {}}}", arr_capped)
    );
}

#[test]
fn capped_non_array_delegates_to_uncapped() {
    // Scalars and other non-Array/Record variants pass through unchanged.
    for s in [
        NetworkResult::Int(42),
        NetworkResult::Bool(false),
        NetworkResult::Float(1.5),
        NetworkResult::String("hi".into()),
    ] {
        assert_eq!(s.to_display_string_capped(20), s.to_display_string());
    }
}

#[test]
fn capped_array_cap_zero_shows_only_ellipsis() {
    // Edge case: cap=0 on a non-empty array shows just `[...]`. (Not a
    // realistic setting, but the contract should still be well-defined.)
    let arr = ints(&[1, 2, 3]);
    assert_eq!(arr.to_display_string_capped(0), "[...]");
}

#[test]
fn capped_array_empty_renders_empty_brackets() {
    let arr = NetworkResult::Array(Vec::new());
    assert_eq!(arr.to_display_string_capped(20), "[]");
}

/// Runtime-guard invariant (§6.5 / OQ3): no `NetworkResult` variant should
/// ever infer an abstract data type. The evaluator's post-eval guard
/// (`evaluate_all_outputs`) replaces any value whose `infer_data_type()` is
/// abstract with `NetworkResult::Error`; this test proves the guard's
/// invariant holds for every concrete-typed variant today. If a new variant
/// is added that breaks this, the guard fires in release and asserts in debug.
#[test]
fn no_network_result_variant_infers_abstract_type() {
    let samples: Vec<NetworkResult> = vec![
        NetworkResult::None,
        NetworkResult::Bool(true),
        NetworkResult::Int(0),
        NetworkResult::Float(0.0),
        NetworkResult::String(String::new()),
        make_crystal(),
        make_molecule(),
        NetworkResult::Error("e".into()),
        NetworkResult::Array(vec![]),
    ];
    for s in samples {
        if let Some(t) = s.infer_data_type() {
            assert!(
                !t.is_abstract(),
                "NetworkResult variant unexpectedly inferred abstract type {:?}",
                t
            );
        }
    }
}
