//! Serde behavior for the per-network node-canvas viewport (pan + zoom),
//! issue #414 Phase 4. Covers round-trip and the old-file default (missing
//! field ⇒ `None`, so the editor falls back to top-left auto-framing).
//! See `doc/design_find_usages.md` D7.

use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableCanvasViewport, SerializableNodeNetwork, SerializableNodeType,
    SerializableOutputPin, serializable_to_node_network,
};

fn built_in_node_types()
-> std::collections::HashMap<String, rust_lib_flutter_cad::structure_designer::node_type::NodeType>
{
    NodeTypeRegistry::new().built_in_node_types
}

fn minimal_network(canvas_viewport: Option<SerializableCanvasViewport>) -> SerializableNodeNetwork {
    SerializableNodeNetwork {
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
        camera_settings: None,
        canvas_viewport,
    }
}

#[test]
fn canvas_viewport_json_round_trip() {
    let serializable = minimal_network(Some(SerializableCanvasViewport {
        pan_x: 123.5,
        pan_y: -42.25,
        zoom_level: 2,
    }));

    let network =
        serializable_to_node_network(&serializable, &built_in_node_types(), None).unwrap();
    let cv = network
        .canvas_viewport
        .expect("canvas_viewport should be present");
    assert_eq!(cv.pan_x, 123.5);
    assert_eq!(cv.pan_y, -42.25);
    assert_eq!(cv.zoom_level, 2);
}

#[test]
fn network_without_canvas_viewport_loads_as_none() {
    let serializable = minimal_network(None);
    let network =
        serializable_to_node_network(&serializable, &built_in_node_types(), None).unwrap();
    assert!(network.canvas_viewport.is_none());
}

#[test]
fn old_file_without_canvas_viewport_field_defaults_to_none() {
    // A pre-feature network JSON has no `canvas_viewport` key at all. The
    // `#[serde(default)]` on the field must yield `None` (the editor then
    // auto-frames the top-left node).
    let json = r#"{
        "next_node_id": 1,
        "node_type": {
            "name": "test_network",
            "description": "Test network",
            "category": "Custom",
            "parameters": [],
            "output_pins": [{"name": "result", "data_type": "Blueprint"}]
        },
        "nodes": [],
        "return_node_id": null,
        "displayed_node_ids": []
    }"#;

    let serializable: SerializableNodeNetwork = serde_json::from_str(json).unwrap();
    assert!(serializable.canvas_viewport.is_none());
}
