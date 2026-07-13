//! Serde behavior for the navigation-up-axis camera fields (issue #349,
//! Phase 1). Covers round-trip, the old-file default (the D6 `(0,0,0)` trap),
//! and the sanitization done in the from-serializable conversion.
//! See `doc/design_view_up_axis.md`.

use glam::f64::DVec3;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableCameraSettings, SerializableNodeNetwork, SerializableNodeType,
    SerializableOutputPin, serializable_to_node_network,
};

fn built_in_node_types()
-> std::collections::HashMap<String, rust_lib_flutter_cad::structure_designer::node_type::NodeType>
{
    NodeTypeRegistry::new().built_in_node_types
}

/// Wraps the given camera settings in a minimal network and runs it through the
/// from-serializable conversion, returning the resolved `CameraSettings`.
fn convert_with_camera(
    camera: SerializableCameraSettings,
) -> rust_lib_flutter_cad::structure_designer::camera_settings::CameraSettings {
    let serializable = SerializableNodeNetwork {
        next_node_id: 1,
        node_type: SerializableNodeType {
            name: "test_network".to_string(),
            description: "Test network".to_string(),
            summary: None,
            category: "Custom".to_string(),
            parameters: vec![],
            output_pins: vec![SerializableOutputPin {
                name: "result".to_string(),
                data_type: "Blueprint".to_string(),
            }],
            output_type: None,
            zone_input_pins: vec![],
            zone_output_pins: vec![],
        },
        nodes: vec![],
        return_node_id: None,
        displayed_node_ids: vec![],
        displayed_output_pins: vec![],
        camera_settings: Some(camera),
    };

    let network = serializable_to_node_network(&serializable, &built_in_node_types(), None).unwrap();
    network.camera_settings.expect("camera_settings should be present")
}

fn base_camera(nav_up: DVec3, nav_up_label: &str) -> SerializableCameraSettings {
    SerializableCameraSettings {
        eye: DVec3::new(0.0, -30.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::new(0.0, 0.32, 0.95),
        orthographic: false,
        ortho_half_height: 10.0,
        pivot_point: DVec3::ZERO,
        nav_up,
        nav_up_label: nav_up_label.to_string(),
    }
}

#[test]
fn nav_up_json_round_trip() {
    let cam = base_camera(DVec3::new(1.0, 1.0, 1.0).normalize(), "(1 1 1)");
    let json = serde_json::to_string(&cam).unwrap();
    assert!(json.contains("nav_up"));
    assert!(json.contains("nav_up_label"));

    let back: SerializableCameraSettings = serde_json::from_str(&json).unwrap();
    assert!((back.nav_up - cam.nav_up).length() < 1e-12);
    assert_eq!(back.nav_up_label, "(1 1 1)");
}

#[test]
fn old_file_without_nav_fields_defaults_to_z_not_zero() {
    // A pre-feature camera settings blob has no nav_up / nav_up_label. The
    // custom serde default fn must yield +Z / "Z", never the (0,0,0) a plain
    // `#[serde(default)]` on a DVec3 would produce (the D6 trap).
    let json = r#"{
        "eye": [0.0, -30.0, 10.0],
        "target": [0.0, 0.0, 0.0],
        "up": [0.0, 0.32, 0.95],
        "orthographic": false,
        "ortho_half_height": 10.0,
        "pivot_point": [0.0, 0.0, 0.0]
    }"#;

    let cam: SerializableCameraSettings = serde_json::from_str(json).unwrap();
    assert_eq!(cam.nav_up, DVec3::Z);
    assert_eq!(cam.nav_up_label, "Z");
}

#[test]
fn conversion_sanitizes_zero_nav_up_to_z() {
    let settings = convert_with_camera(base_camera(DVec3::ZERO, "bogus"));
    assert_eq!(settings.nav_up, DVec3::Z);
    assert_eq!(settings.nav_up_label, "Z");
}

#[test]
fn conversion_sanitizes_non_finite_nav_up_to_z() {
    let settings = convert_with_camera(base_camera(DVec3::new(f64::NAN, 0.0, 0.0), "bogus"));
    assert_eq!(settings.nav_up, DVec3::Z);
    assert_eq!(settings.nav_up_label, "Z");
}

#[test]
fn conversion_normalizes_non_unit_nav_up() {
    let settings = convert_with_camera(base_camera(DVec3::new(0.0, 0.0, 5.0), "[0 0 1]"));
    assert!((settings.nav_up - DVec3::Z).length() < 1e-12);
    // A valid (just non-unit) axis keeps its label.
    assert_eq!(settings.nav_up_label, "[0 0 1]");
}
