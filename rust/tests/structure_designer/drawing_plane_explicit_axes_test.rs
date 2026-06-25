//! Phase 2 tests for the drawing_plane explicit in-plane axes feature.
//!
//! Covers the node-data layer added in phase 2: `Option<IVec3>` Miller index +
//! `u`/`v` axes, serialization (no version bump), three-state pin resolution,
//! and eval-through-network for the four orientation cases (A–D) plus errors.
//! The pure geometry (`DrawingPlane::from_spec`) is covered in
//! `tests/crystolecule/drawing_plane_test.rs`.

use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::nodes::drawing_plane::DrawingPlaneData;
use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test_network");
    designer.set_active_node_network_name(Some("test_network".to_string()));
    designer
}

/// Adds an ivec3 literal node with the given value.
fn add_ivec3(designer: &mut StructureDesigner, value: IVec3) -> u64 {
    let id = designer.add_node("ivec3", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(id, Box::new(IVec3Data { value }));
    id
}

// ============================================================================
// Serialization (no version bump)
// ============================================================================

#[test]
fn old_file_miller_array_deserializes_as_some() {
    // Old files always carry `miller_index: [h,k,l]` and no `u_axis`/`v_axis`.
    let json = r#"{
        "max_miller_index": 1,
        "miller_index": [1, 1, 0],
        "center": [0, 0, 0],
        "shift": 2,
        "subdivision": 1
    }"#;
    let data: DrawingPlaneData = serde_json::from_str(json).unwrap();
    assert_eq!(data.miller_index, Some(IVec3::new(1, 1, 0)));
    assert_eq!(data.u_axis, None);
    assert_eq!(data.v_axis, None);
}

#[test]
fn new_file_with_null_miller_and_axes_roundtrips() {
    // Case D on disk: explicit `u`/`v`, derived (null) Miller index.
    let original = DrawingPlaneData {
        max_miller_index: 1,
        miller_index: None,
        center: IVec3::new(2, 0, 0),
        shift: 1,
        subdivision: 1,
        u_axis: Some(IVec3::new(2, 0, 0)),
        v_axis: Some(IVec3::new(0, 1, 0)),
    };
    let json = serde_json::to_string(&original).unwrap();
    // `miller_index: null` is the only new on-disk state.
    assert!(json.contains("\"miller_index\":null"));
    let restored: DrawingPlaneData = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.miller_index, None);
    assert_eq!(restored.u_axis, Some(IVec3::new(2, 0, 0)));
    assert_eq!(restored.v_axis, Some(IVec3::new(0, 1, 0)));
}

#[test]
fn absent_axes_default_to_none() {
    // A minimal object (only the always-present fields) leaves `u`/`v` unset.
    let json = r#"{
        "max_miller_index": 1,
        "miller_index": [0, 0, 1],
        "center": [0, 0, 0],
        "shift": 0,
        "subdivision": 1
    }"#;
    let data: DrawingPlaneData = serde_json::from_str(json).unwrap();
    assert_eq!(data.u_axis, None);
    assert_eq!(data.v_axis, None);
}

// ============================================================================
// Eval-through-network: cases A–D
// ============================================================================

#[test]
fn eval_case_a_default_auto_axes() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(
        result.success,
        "case A should succeed: {:?}",
        result.error_message
    );
    assert_eq!(result.output_type, "DrawingPlane");
    // Default Miller index (0,0,1).
    assert!(result.display_string.contains("miller_index=(0, 0, 1)"));
}

#[test]
fn eval_case_d_derives_miller_from_stored_axes() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    // Case D: stored `m` unset, stored `u`/`v` set. m = reduce(u × v) = (0,0,1).
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: None,
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: Some(IVec3::new(1, 0, 0)),
            v_axis: Some(IVec3::new(0, 1, 0)),
        }),
    );
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(
        result.success,
        "case D should succeed: {:?}",
        result.error_message
    );
    // The displayed (resolved) Miller index is the derived one.
    assert!(
        result.display_string.contains("miller_index=(0, 0, 1)"),
        "expected derived miller (0,0,1), got: {}",
        result.display_string
    );
}

#[test]
fn eval_case_c_explicit_axes_succeed() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: Some(IVec3::new(0, 0, 1)),
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: Some(IVec3::new(1, 0, 0)),
            v_axis: Some(IVec3::new(0, 1, 0)),
        }),
    );
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(
        result.success,
        "case C should succeed: {:?}",
        result.error_message
    );
    assert!(result.display_string.contains("miller_index=(0, 0, 1)"));
}

// ============================================================================
// Eval errors
// ============================================================================

#[test]
fn eval_under_specified_is_error() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    // m unset, no u/v at all.
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: None,
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: None,
            v_axis: None,
        }),
    );
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(!result.success);
    let msg = result.error_message.unwrap();
    assert!(
        msg.contains("orientation unspecified"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn eval_v_only_is_error() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: Some(IVec3::new(0, 0, 1)),
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: None,
            v_axis: Some(IVec3::new(0, 1, 0)),
        }),
    );
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(!result.success);
    assert!(result.error_message.unwrap().contains("specify `u`"));
}

#[test]
fn eval_case_d_parallel_axes_is_error() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: None,
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: Some(IVec3::new(1, 0, 0)),
            v_axis: Some(IVec3::new(2, 0, 0)), // parallel to u
        }),
    );
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(!result.success);
    assert!(
        result
            .error_message
            .unwrap()
            .to_lowercase()
            .contains("parallel")
    );
}

// ============================================================================
// Three-state resolution precedence (wired pin > stored field > unset)
// ============================================================================

#[test]
fn wired_miller_pin_overrides_stored_field() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    // Stored m is (1,0,0); the wired pin supplies (0,0,1) and must win.
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: Some(IVec3::new(1, 0, 0)),
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: None,
            v_axis: None,
        }),
    );
    let m_src = add_ivec3(&mut designer, IVec3::new(0, 0, 1));
    designer.connect_nodes(m_src, 0, dp, 1); // pin 1 = m_index

    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(result.success, "{:?}", result.error_message);
    assert!(
        result.display_string.contains("miller_index=(0, 0, 1)"),
        "wired pin should win, got: {}",
        result.display_string
    );
}

#[test]
fn stored_field_used_when_pin_disconnected() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 2,
            miller_index: Some(IVec3::new(1, 1, 0)),
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: None,
            v_axis: None,
        }),
    );
    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(result.success, "{:?}", result.error_message);
    assert!(result.display_string.contains("miller_index=(1, 1, 0)"));
}

#[test]
fn mixed_stored_miller_and_wired_u_resolves_case_c() {
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    // m from stored field; u + v from wired pins — the wiring most prone to
    // mis-indexing (u is pin 5, v is pin 6).
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: Some(IVec3::new(0, 0, 1)),
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: None,
            v_axis: None,
        }),
    );
    let u_src = add_ivec3(&mut designer, IVec3::new(1, 0, 0));
    let v_src = add_ivec3(&mut designer, IVec3::new(0, 1, 0));
    designer.connect_nodes(u_src, 0, dp, 5); // pin 5 = u
    designer.connect_nodes(v_src, 0, dp, 6); // pin 6 = v

    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(
        result.success,
        "mixed case C should succeed: {:?}",
        result.error_message
    );
    assert!(result.display_string.contains("miller_index=(0, 0, 1)"));
}

#[test]
fn wired_axes_derive_miller_when_stored_unset() {
    // Case D driven entirely by wired pins (stored m/u/v all unset).
    let mut designer = setup();
    let dp = designer.add_node("drawing_plane", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        dp,
        Box::new(DrawingPlaneData {
            max_miller_index: 1,
            miller_index: None,
            center: IVec3::new(0, 0, 0),
            shift: 0,
            subdivision: 1,
            u_axis: None,
            v_axis: None,
        }),
    );
    let u_src = add_ivec3(&mut designer, IVec3::new(1, 0, 0));
    let v_src = add_ivec3(&mut designer, IVec3::new(0, 1, 0));
    designer.connect_nodes(u_src, 0, dp, 5);
    designer.connect_nodes(v_src, 0, dp, 6);

    let result = designer.evaluate_node_for_cli(dp, false).unwrap();
    assert!(result.success, "{:?}", result.error_message);
    assert!(result.display_string.contains("miller_index=(0, 0, 1)"));
}
