// Unit tests for multi-output pin data structures (Phase 1 + Phase 2 + Phase 6)
// and EvalOutput display overrides.

use glam::DVec2;
use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::motif::Motif;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use rust_lib_flutter_cad::structure_designer::node_data::EvalOutput;
use rust_lib_flutter_cad::structure_designer::node_network::{
    NodeDisplayState, NodeDisplayType, NodeNetwork,
};
use rust_lib_flutter_cad::structure_designer::node_type::OutputPinDefinition;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashSet;

// ===== OutputPinDefinition tests =====

#[test]
fn test_output_pin_definition_single() {
    let pins = OutputPinDefinition::single(DataType::Blueprint);
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].name, "result");
    assert_eq!(pins[0].data_type, DataType::Blueprint);
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
    assert_eq!(*sphere_type.output_type(), DataType::Blueprint);
}

#[test]
fn test_node_type_get_output_pin_type_pin0() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.get_output_pin_type(0), DataType::Blueprint);
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
                data_type: "Blueprint".to_string(),
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
            "output_pins": [{"name": "result", "data_type": "Blueprint"}]
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

// ===== Phase 4: NodeDisplayState PartialEq tests =====

#[test]
fn test_node_display_state_partial_eq_same() {
    let a = NodeDisplayState::normal();
    let b = NodeDisplayState::normal();
    assert_eq!(a, b);
}

#[test]
fn test_node_display_state_partial_eq_different_pins() {
    let a = NodeDisplayState::normal(); // {0}
    let mut b = NodeDisplayState::normal();
    b.displayed_pins.insert(1); // {0, 1}
    assert_ne!(a, b);
}

#[test]
fn test_node_display_state_partial_eq_different_type() {
    let a = NodeDisplayState::with_type(NodeDisplayType::Normal);
    let b = NodeDisplayState::with_type(NodeDisplayType::Ghost);
    assert_ne!(a, b);
}

#[test]
fn test_toggle_output_pin_display_on_node_network() {
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

    // Toggle pin 1 on
    network.set_pin_displayed(node_id, 1, true);
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0, 1]))
    );

    // Toggle pin 1 off
    network.set_pin_displayed(node_id, 1, false);
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0]))
    );
}

#[test]
fn test_set_pin_displayed_re_adds_removed_node() {
    let mut network = NodeNetwork::new_empty();
    let node_id = network.add_node(
        "sphere",
        glam::f64::DVec2::ZERO,
        1,
        Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
    );

    // Remove pin 0 — node is removed from displayed_nodes
    network.set_pin_displayed(node_id, 0, false);
    assert!(!network.is_node_displayed(node_id));

    // Re-add pin 0 — node should be re-added to displayed_nodes
    network.set_pin_displayed(node_id, 0, true);
    assert!(network.is_node_displayed(node_id));
    assert_eq!(
        network.get_displayed_pins(node_id),
        Some(&HashSet::from([0]))
    );
}

#[test]
fn test_node_layout_height_with_multi_output() {
    use rust_lib_flutter_cad::structure_designer::node_layout;

    // Single output: 0 inputs, 1 output = title(30) + max(0, 22, 25)=25 + pad(8) = 63
    let h1 = node_layout::estimate_node_height(0, 1, false);
    assert!((h1 - 63.0).abs() < 0.001);

    // Two outputs: 0 inputs, 2 outputs = title(30) + max(0, 44, 25)=44 + pad(8) = 82
    let h2 = node_layout::estimate_node_height(0, 2, false);
    assert!((h2 - 82.0).abs() < 0.001);

    // More inputs than outputs: 3 inputs, 2 outputs = title(30) + max(66, 44, 25)=66 + pad(8) = 104
    let h3 = node_layout::estimate_node_height(3, 2, false);
    assert!((h3 - 104.0).abs() < 0.001);

    // More outputs than inputs: 1 input, 3 outputs = title(30) + max(22, 66, 25)=66 + pad(8) = 104
    let h4 = node_layout::estimate_node_height(1, 3, false);
    assert!((h4 - 104.0).abs() < 0.001);
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
        displayed_pins: std::collections::HashSet::from([0]),
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        show_unit_cell_wireframe: false,
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
        displayed_pins: std::collections::HashSet::from([0, 1]),
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        show_unit_cell_wireframe: false,
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
        displayed_pins: std::collections::HashSet::from([1]),
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        show_unit_cell_wireframe: false,
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
        displayed_pins: std::collections::HashSet::new(),
        node_errors: std::collections::HashMap::new(),
        node_output_strings: std::collections::HashMap::new(),
        unit_cell: None,
        show_unit_cell_wireframe: false,
        selected_node_eval_cache: None,
    };

    assert_eq!(scene_data.interactive_pin_index(), None);
}

// ===== Phase 6: Custom Network Multi-Output tests =====

fn setup_designer_with_network(name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(name);
    designer.set_active_node_network_name(Some(name.to_string()));
    designer
}

/// Helper to evaluate a node in a network and return the NetworkResult for a specific pin.
fn evaluate_pin(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    pin_index: i32,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(
        &network_stack,
        node_id,
        pin_index,
        registry,
        false,
        &mut context,
    )
}

/// Custom network with a multi-output return node propagates all output pins to the custom node type.
#[test]
fn test_custom_network_multi_output_return_node_propagates_pins() {
    let mut designer = setup_designer_with_network("inner");

    // Add an atom_edit node (which has 2 output pins: result + diff)
    let atom_edit_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.set_return_node_id(Some(atom_edit_id));

    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();

    // The custom network type should now have 2 output pins matching atom_edit
    assert_eq!(
        network.node_type.output_pin_count(),
        2,
        "Custom network should have 2 output pins from atom_edit return node"
    );
    assert_eq!(network.node_type.output_pins[0].name, "result");
    assert_eq!(*network.node_type.output_type(), DataType::Atomic);
    assert_eq!(network.node_type.output_pins[1].name, "diff");
    assert_eq!(network.node_type.output_pins[1].data_type, DataType::Atomic);
    assert!(network.node_type.has_multi_output());
}

/// Custom network with single-output return node behaves as before (one output pin).
#[test]
fn test_custom_network_single_output_return_node() {
    let mut designer = setup_designer_with_network("inner");

    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));

    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();

    assert_eq!(network.node_type.output_pin_count(), 1);
    assert_eq!(network.node_type.output_pins[0].name, "result");
    assert_eq!(*network.node_type.output_type(), DataType::Blueprint);
    assert!(!network.node_type.has_multi_output());
}

/// Switching return node from multi-output to single-output updates output_pins.
#[test]
fn test_custom_network_return_node_change_multi_to_single() {
    let mut designer = setup_designer_with_network("inner");

    // Start with atom_edit (2 pins)
    let atom_edit_id = designer.add_node("atom_edit", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.set_return_node_id(Some(atom_edit_id));

    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();
    assert_eq!(network.node_type.output_pin_count(), 2);

    // Switch to sphere (1 pin)
    designer.set_return_node_id(Some(sphere_id));

    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();
    assert_eq!(network.node_type.output_pin_count(), 1);
    assert_eq!(*network.node_type.output_type(), DataType::Blueprint);
}

/// Switching return node from single-output to multi-output updates output_pins.
#[test]
fn test_custom_network_return_node_change_single_to_multi() {
    let mut designer = setup_designer_with_network("inner");

    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));

    // Start with sphere (1 pin)
    designer.set_return_node_id(Some(sphere_id));
    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();
    assert_eq!(network.node_type.output_pin_count(), 1);

    // Switch to atom_edit (2 pins)
    designer.set_return_node_id(Some(atom_edit_id));
    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();
    assert_eq!(network.node_type.output_pin_count(), 2);
}

/// No return node → single output pin with DataType::None.
#[test]
fn test_custom_network_no_return_node() {
    let mut designer = setup_designer_with_network("inner");
    designer.add_node("sphere", DVec2::ZERO);
    // Don't set return node

    let network = designer
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap();
    assert_eq!(network.node_type.output_pin_count(), 1);
    assert_eq!(*network.node_type.output_type(), DataType::None);
}

/// Using a custom network as a node in another network: pin 0 evaluation works.
#[test]
fn test_custom_network_node_evaluate_pin0() {
    let mut designer = setup_designer_with_network("inner");

    // inner: sphere as return node
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));

    // Create outer network and add a node of type "inner"
    designer.add_node_network("outer");
    designer.set_active_node_network_name(Some("outer".to_string()));
    let inner_node_id = designer.add_node("inner", DVec2::ZERO);

    // Evaluate pin 0 of the custom node
    let result = evaluate_pin(&designer, "outer", inner_node_id, 0);
    assert!(
        !matches!(result, NetworkResult::Error(_)),
        "Pin 0 evaluation should not error"
    );
}

/// Using a custom network with multi-output return node: pin 1 evaluation passes through.
#[test]
fn test_custom_network_node_evaluate_pin1() {
    let mut designer = setup_designer_with_network("inner");

    // inner: atom_edit as return node (2 output pins)
    let atom_edit_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.set_return_node_id(Some(atom_edit_id));

    // Create outer network and add a node of type "inner"
    designer.add_node_network("outer");
    designer.set_active_node_network_name(Some("outer".to_string()));
    let inner_node_id = designer.add_node("inner", DVec2::ZERO);

    // Evaluate pin 0 (result)
    let result_pin0 = evaluate_pin(&designer, "outer", inner_node_id, 0);
    assert!(
        matches!(result_pin0, NetworkResult::Crystal(_) | NetworkResult::Molecule(_)),
        "Pin 0 should be Atomic"
    );

    // Evaluate pin 1 (diff) — should also be Atomic
    let result_pin1 = evaluate_pin(&designer, "outer", inner_node_id, 1);
    assert!(
        matches!(result_pin1, NetworkResult::Crystal(_) | NetworkResult::Molecule(_)),
        "Pin 1 should be Atomic"
    );
}

/// Wiring from pin 1 of a custom node to a downstream node works.
#[test]
fn test_custom_network_wire_from_pin1() {
    let mut designer = setup_designer_with_network("inner");

    // inner: atom_edit as return node
    let atom_edit_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.set_return_node_id(Some(atom_edit_id));

    // outer: use inner node, wire pin 1 to apply_diff's base input
    designer.add_node_network("outer");
    designer.set_active_node_network_name(Some("outer".to_string()));
    let inner_node_id = designer.add_node("inner", DVec2::ZERO);
    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 0.0));

    // Wire from pin 1 (diff output) of inner to input 0 (base) of apply_diff
    designer.connect_nodes(inner_node_id, 1, apply_diff_id, 0);

    // Validate the network — should be valid with the wire to pin 1
    designer.validate_active_network();
    let network = designer
        .node_type_registry
        .node_networks
        .get("outer")
        .unwrap();
    assert!(
        network.valid,
        "Network should be valid with wire from pin 1"
    );

    // The wire should still exist
    let apply_diff_node = network.nodes.get(&apply_diff_id).unwrap();
    assert!(
        !apply_diff_node.arguments[0].argument_output_pins.is_empty(),
        "Wire from pin 1 should be preserved"
    );
    assert_eq!(
        apply_diff_node.arguments[0].argument_output_pins[&inner_node_id], 1,
        "Wire should reference pin index 1"
    );
}

/// When return node changes from multi to single output, wires to removed pins are disconnected.
#[test]
fn test_custom_network_shrink_output_pins_disconnects_wires() {
    let mut designer = setup_designer_with_network("inner");

    // inner: atom_edit as return node (2 pins)
    let atom_edit_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.set_return_node_id(Some(atom_edit_id));

    // outer: wire from pin 1 of inner to a downstream node
    designer.add_node_network("outer");
    designer.set_active_node_network_name(Some("outer".to_string()));
    let inner_node_id = designer.add_node("inner", DVec2::ZERO);
    let apply_diff_id = designer.add_node("apply_diff", DVec2::new(200.0, 0.0));
    designer.connect_nodes(inner_node_id, 1, apply_diff_id, 0);

    // Verify the wire exists
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get("outer")
            .unwrap();
        let node = network.nodes.get(&apply_diff_id).unwrap();
        assert_eq!(node.arguments[0].argument_output_pins.len(), 1);
    }

    // Now change inner's return node to sphere (single output)
    designer.set_active_node_network_name(Some("inner".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));

    // The outer network should be re-validated and the wire to pin 1 should be removed
    // because inner now only has pin 0
    let network = designer
        .node_type_registry
        .node_networks
        .get("outer")
        .unwrap();
    let node = network.nodes.get(&apply_diff_id).unwrap();
    assert!(
        node.arguments[0].argument_output_pins.is_empty(),
        "Wire to pin 1 should be disconnected after inner shrinks to single output"
    );
}

// ===== EvalOutput display override tests =====

#[test]
fn test_eval_output_display_override_basic() {
    // Pin 0 wire = Motif, display override = Atomic
    let motif = Motif {
        parameters: vec![],
        sites: vec![],
        bonds: vec![],
        bonds_by_site1_index: vec![],
        bonds_by_site2_index: vec![],
    };
    let viz = AtomicStructure::new();

    let mut output = EvalOutput::multi(vec![
        NetworkResult::Motif(motif),
        NetworkResult::Molecule(MoleculeData { atoms: AtomicStructure::new(), geo_tree_root: None }),
    ]);
    output.set_display_override(0, NetworkResult::Molecule(MoleculeData { atoms: viz, geo_tree_root: None }));

    // Wire value: get(0) returns Motif
    assert!(matches!(output.get(0), NetworkResult::Motif(_)));

    // Display value: get_display(0) returns Atomic (the override)
    assert!(matches!(output.get_display(0), NetworkResult::Crystal(_) | NetworkResult::Molecule(_)));

    // Pin 1 has no override: get_display(1) falls back to wire value
    assert!(matches!(output.get_display(1), NetworkResult::Crystal(_) | NetworkResult::Molecule(_)));
}

#[test]
fn test_eval_output_display_override_fallback() {
    let output = EvalOutput::single(NetworkResult::Float(42.0));

    // No display overrides set — get_display falls back to wire result
    assert!(matches!(output.get_display(0), NetworkResult::Float(v) if v == 42.0));
}

#[test]
fn test_eval_output_display_results_default_empty() {
    let single = EvalOutput::single(NetworkResult::Int(1));
    assert!(single.display_results.is_empty());

    let multi = EvalOutput::multi(vec![NetworkResult::Int(1), NetworkResult::Int(2)]);
    assert!(multi.display_results.is_empty());
}

// ===== NetworkResult::infer_data_type() tests =====

#[test]
fn test_infer_data_type_primitives() {
    assert_eq!(
        NetworkResult::Bool(true).infer_data_type(),
        Some(DataType::Bool)
    );
    assert_eq!(
        NetworkResult::String("hi".into()).infer_data_type(),
        Some(DataType::String)
    );
    assert_eq!(NetworkResult::Int(1).infer_data_type(), Some(DataType::Int));
    assert_eq!(
        NetworkResult::Float(1.0).infer_data_type(),
        Some(DataType::Float)
    );
}

#[test]
fn test_infer_data_type_vectors() {
    assert_eq!(
        NetworkResult::Vec2(DVec2::ZERO).infer_data_type(),
        Some(DataType::Vec2)
    );
    assert_eq!(
        NetworkResult::Vec3(DVec3::ZERO).infer_data_type(),
        Some(DataType::Vec3)
    );
    assert_eq!(
        NetworkResult::IVec2(glam::IVec2::ZERO).infer_data_type(),
        Some(DataType::IVec2)
    );
    assert_eq!(
        NetworkResult::IVec3(IVec3::ZERO).infer_data_type(),
        Some(DataType::IVec3)
    );
}

#[test]
fn test_infer_data_type_complex() {
    assert_eq!(
        NetworkResult::Molecule(MoleculeData { atoms: AtomicStructure::new(), geo_tree_root: None }).infer_data_type(),
        Some(DataType::Molecule)
    );
    let motif = Motif {
        parameters: vec![],
        sites: vec![],
        bonds: vec![],
        bonds_by_site1_index: vec![],
        bonds_by_site2_index: vec![],
    };
    assert_eq!(
        NetworkResult::Motif(motif).infer_data_type(),
        Some(DataType::Motif)
    );
}

#[test]
fn test_infer_data_type_none_variants() {
    assert_eq!(NetworkResult::None.infer_data_type(), None);
    assert_eq!(NetworkResult::Error("err".into()).infer_data_type(), None);
    assert_eq!(NetworkResult::Array(vec![]).infer_data_type(), None);
}
