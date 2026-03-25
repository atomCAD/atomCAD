// Unit tests for multi-output pin data structures (Phase 1 + Phase 2).

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::EvalOutput;
use rust_lib_flutter_cad::structure_designer::node_network::{
    NodeDisplayState, NodeDisplayType, NodeNetwork,
};
use rust_lib_flutter_cad::structure_designer::node_type::OutputPinDefinition;
use std::collections::HashSet;

// ===== OutputPinDefinition tests =====

#[test]
fn test_output_pin_definition_single() {
    let pins = OutputPinDefinition::single(DataType::Geometry);
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "result");
    assert_eq!(pins[0].data_type, DataType::Geometry);
}

#[test]
fn test_output_pin_definition_single_none() {
    let pins = OutputPinDefinition::single(DataType::None);
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].data_type, DataType::None);
}

// ===== NodeType accessor tests =====

#[test]
fn test_node_type_output_type_accessor() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(*sphere_type.output_type(), DataType::Geometry);
}

#[test]
fn test_node_type_get_output_pin_type_pin0() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.get_output_pin_type(0), DataType::Geometry);
}

#[test]
fn test_node_type_get_output_pin_type_function_pin() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    let fn_type = sphere_type.get_output_pin_type(-1);
    assert!(matches!(fn_type, DataType::Function(_)));
}

#[test]
fn test_node_type_get_output_pin_type_out_of_range() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.get_output_pin_type(1), DataType::None);
    assert_eq!(sphere_type.get_output_pin_type(99), DataType::None);
}

#[test]
fn test_node_type_output_pin_count() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.output_pin_count(), 1);
}

#[test]
fn test_node_type_has_multi_output_single() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert!(!sphere_type.has_multi_output());
}

// ===== EvalOutput tests =====

#[test]
fn test_eval_output_single() {
    let output = EvalOutput::single(NetworkResult::Float(42.0));
    assert_eq!(output.results.len(), 1);
    assert!(matches!(output.primary(), NetworkResult::Float(v) if *v == 42.0));
}

#[test]
fn test_eval_output_multi() {
    let output = EvalOutput::multi(vec![NetworkResult::Float(1.0), NetworkResult::Int(2)]);
    assert_eq!(output.results.len(), 2);
    assert!(matches!(output.primary(), NetworkResult::Float(v) if *v == 1.0));
}

#[test]
fn test_eval_output_get_valid_index() {
    let output = EvalOutput::multi(vec![NetworkResult::Float(1.0), NetworkResult::Int(2)]);
    assert!(matches!(output.get(0), NetworkResult::Float(v) if v == 1.0));
    assert!(matches!(output.get(1), NetworkResult::Int(2)));
}

#[test]
fn test_eval_output_get_out_of_range() {
    let output = EvalOutput::single(NetworkResult::Float(1.0));
    assert!(matches!(output.get(1), NetworkResult::None));
    assert!(matches!(output.get(99), NetworkResult::None));
}

#[test]
fn test_eval_output_get_negative_index() {
    // Negative indices (like -1 for function pin) are handled by the evaluator,
    // not by EvalOutput. get() treats them as out of range.
    let output = EvalOutput::single(NetworkResult::Float(1.0));
    // -1 as i32 cast to usize wraps to a very large number, so get() returns None
    assert!(matches!(output.get(-1), NetworkResult::None));
}

#[test]
fn test_eval_output_primary() {
    let output = EvalOutput::single(NetworkResult::Error("test".to_string()));
    assert!(matches!(output.primary(), NetworkResult::Error(_)));
}

// ===== Phase 2: NodeDisplayState tests =====

#[test]
fn test_node_display_state_normal_defaults() {
    let state = NodeDisplayState::normal();
    assert_eq!(state.display_type, NodeDisplayType::Normal);
    assert_eq!(state.displayed_pins, HashSet::from([0]));
}

#[test]
fn test_node_display_state_with_type() {
    let state = NodeDisplayState::with_type(NodeDisplayType::Ghost);
    assert_eq!(state.display_type, NodeDisplayType::Ghost);
    assert_eq!(state.displayed_pins, HashSet::from([0]));
}

#[test]
fn test_set_node_display_creates_default_display_state() {
    let mut network = NodeNetwork::new_empty();
    let node_id = network.add_node(
        "sphere",
        glam::f64::DVec2::ZERO,
        1,
        Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
    );
    // add_node calls set_node_display(true) which creates a NodeDisplayState::normal()
    assert!(network.is_node_displayed(node_id));
    assert_eq!(
        network.get_node_display_type(node_id),
        Some(NodeDisplayType::Normal)
    );
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0]))
    );
}

#[test]
fn test_set_node_display_preserves_pins_on_redisplay() {
    let mut network = NodeNetwork::new_empty();
    let node_id = network.add_node(
        "sphere",
        glam::f64::DVec2::ZERO,
        1,
        Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
    );
    // Manually add pin 1 to displayed pins
    network.set_pin_displayed(node_id, 1, true);
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0, 1]))
    );

    // Hide the node
    network.set_node_display(node_id, false);
    assert!(!network.is_node_displayed(node_id));

    // Re-display — should get fresh default state (pin 0 only)
    network.set_node_display(node_id, true);
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0]))
    );
}

#[test]
fn test_set_node_display_type_preserves_pins() {
    let mut network = NodeNetwork::new_empty();
    let node_id = network.add_node(
        "sphere",
        glam::f64::DVec2::ZERO,
        1,
        Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
    );
    // Add pin 1
    network.set_pin_displayed(node_id, 1, true);

    // Change display type — should preserve pins
    network.set_node_display_type(node_id, Some(NodeDisplayType::Ghost));
    assert_eq!(
        network.get_node_display_type(node_id),
        Some(NodeDisplayType::Ghost)
    );
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0, 1]))
    );
}

#[test]
fn test_set_pin_displayed_add_and_remove() {
    let mut network = NodeNetwork::new_empty();
    let node_id = network.add_node(
        "sphere",
        glam::f64::DVec2::ZERO,
        1,
        Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
    );

    // Initially only pin 0
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0]))
    );

    // Add pin 1
    network.set_pin_displayed(node_id, 1, true);
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0, 1]))
    );

    // Remove pin 0
    network.set_pin_displayed(node_id, 0, false);
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([1]))
    );

    // Remove pin 1 — should auto-remove the node from displayed_nodes
    network.set_pin_displayed(node_id, 1, false);
    assert!(!network.is_node_displayed(node_id));
    assert_eq!(network.get_displayed_pins(node_id), None);
}

#[test]
fn test_displayed_nodes_serialization_roundtrip_default_pins() {
    // Create a network, display a node with default pins, serialize and deserialize
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::*;

    let serializable = SerializableNodeNetwork {
        next_node_id: 2,
        node_type: SerializableNodeType {
            name: "test".to_string(),
            description: String::new(),
            summary: None,
            category: "Custom".to_string(),
            parameters: vec![],
            output_pins: vec![SerializableOutputPin {
                name: "result".to_string(),
                data_type: "Geometry".to_string(),
            }],
            output_type: None,
        },
        nodes: vec![],
        return_node_id: None,
        displayed_node_ids: vec![(1, NodeDisplayType::Normal)],
        displayed_output_pins: vec![], // default pins
        camera_settings: None,
    };

    // Serialize to JSON and back
    let json = serde_json::to_string(&serializable).unwrap();
    let deserialized: SerializableNodeNetwork = serde_json::from_str(&json).unwrap();

    // displayed_output_pins should be absent (skip_serializing_if empty)
    assert!(!json.contains("displayed_output_pins"));
    assert_eq!(deserialized.displayed_node_ids.len(), 1);
    assert!(deserialized.displayed_output_pins.is_empty());
}

#[test]
fn test_displayed_nodes_serialization_roundtrip_non_default_pins() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::*;

    let serializable = SerializableNodeNetwork {
        next_node_id: 2,
        node_type: SerializableNodeType {
            name: "test".to_string(),
            description: String::new(),
            summary: None,
            category: "Custom".to_string(),
            parameters: vec![],
            output_pins: vec![
                SerializableOutputPin {
                    name: "result".to_string(),
                    data_type: "Atomic".to_string(),
                },
                SerializableOutputPin {
                    name: "diff".to_string(),
                    data_type: "Atomic".to_string(),
                },
            ],
            output_type: None,
        },
        nodes: vec![],
        return_node_id: None,
        displayed_node_ids: vec![(1, NodeDisplayType::Normal)],
        displayed_output_pins: vec![(1, vec![0, 1])], // both pins displayed
        camera_settings: None,
    };

    // Serialize to JSON and back
    let json = serde_json::to_string(&serializable).unwrap();
    let deserialized: SerializableNodeNetwork = serde_json::from_str(&json).unwrap();

    // displayed_output_pins should be present
    assert!(json.contains("displayed_output_pins"));
    assert_eq!(deserialized.displayed_output_pins.len(), 1);
    assert_eq!(deserialized.displayed_output_pins[0].0, 1);
    assert_eq!(deserialized.displayed_output_pins[0].1.len(), 2);
}

#[test]
fn test_old_format_without_displayed_output_pins_loads_with_default_pin0() {
    // Simulate an old .cnnd JSON without `displayed_output_pins`
    let json = r#"{
        "next_node_id": 2,
        "node_type": {
            "name": "test",
            "description": "",
            "category": "Custom",
            "parameters": [],
            "output_pins": [{"name": "result", "data_type": "Geometry"}]
        },
        "nodes": [],
        "return_node_id": null,
        "displayed_node_ids": [[1, "Normal"]]
    }"#;

    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::*;
    let deserialized: SerializableNodeNetwork = serde_json::from_str(json).unwrap();

    // displayed_output_pins should default to empty (serde default)
    assert!(deserialized.displayed_output_pins.is_empty());
    assert_eq!(deserialized.displayed_node_ids.len(), 1);
}

// ===== Phase 2: Interactive pin tests =====

#[test]
fn test_node_scene_data_interactive_pin_single() {
    use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
        DisplayedPinOutput, NodeOutput, NodeSceneData,
    };

    let scene_data = NodeSceneData {
        output: NodeOutput::None,
        geo_tree: None,
        pin_outputs: vec![DisplayedPinOutput {
            pin_index: 0,
            output: NodeOutput::None,
            geo_tree: None,
        }],
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        selected_node_eval_cache: None,
    };

    assert_eq!(scene_data.interactive_pin_index(), Some(0));
}

#[test]
fn test_node_scene_data_interactive_pin_multi() {
    use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
        DisplayedPinOutput, NodeOutput, NodeSceneData,
    };

    let scene_data = NodeSceneData {
        output: NodeOutput::None,
        geo_tree: None,
        pin_outputs: vec![
            DisplayedPinOutput {
                pin_index: 0,
                output: NodeOutput::None,
                geo_tree: None,
            },
            DisplayedPinOutput {
                pin_index: 1,
                output: NodeOutput::None,
                geo_tree: None,
            },
        ],
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        selected_node_eval_cache: None,
    };

    // Both displayed: interactive pin is the lowest (0)
    assert_eq!(scene_data.interactive_pin_index(), Some(0));
}

#[test]
fn test_node_scene_data_interactive_pin_only_pin1() {
    use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
        DisplayedPinOutput, NodeOutput, NodeSceneData,
    };

    let scene_data = NodeSceneData {
        output: NodeOutput::None,
        geo_tree: None,
        pin_outputs: vec![DisplayedPinOutput {
            pin_index: 1,
            output: NodeOutput::None,
            geo_tree: None,
        }],
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        selected_node_eval_cache: None,
    };

    // Only pin 1 displayed: interactive pin is 1
    assert_eq!(scene_data.interactive_pin_index(), Some(1));
}

#[test]
fn test_node_scene_data_interactive_pin_empty() {
    use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
        NodeOutput, NodeSceneData,
    };

    let scene_data = NodeSceneData {
        output: NodeOutput::None,
        geo_tree: None,
        pin_outputs: vec![],
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        selected_node_eval_cache: None,
    };

    assert_eq!(scene_data.interactive_pin_index(), None);
}
