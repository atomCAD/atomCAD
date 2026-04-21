use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn set_sequence_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: SequenceData,
) {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        node,
        true,
    );
}

fn props_to_hashmap(props: Vec<(String, TextValue)>) -> HashMap<String, TextValue> {
    props.into_iter().collect()
}

// ============================================================================
// Basic evaluation tests
// ============================================================================

#[test]
fn test_sequence_no_inputs_produces_empty_array() {
    let mut designer = setup_designer_with_network("test");
    let seq_id = designer.add_node("sequence", DVec2::new(0.0, 0.0));

    let result = evaluate_node(&designer, "test", seq_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(
                items.len(),
                0,
                "Unconnected sequence should produce empty array"
            );
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_sequence_two_inputs() {
    let mut designer = setup_designer_with_network("test");

    let int1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int2 = designer.add_node("int", DVec2::new(0.0, 100.0));
    let seq_id = designer.add_node("sequence", DVec2::new(200.0, 0.0));

    // Default element_type is Atomic, change to Int for wiring compatibility
    set_sequence_data(
        &mut designer,
        "test",
        seq_id,
        SequenceData {
            element_type: DataType::Int,
            input_count: 2,
        },
    );
    designer.validate_active_network();

    designer.connect_nodes(int1, 0, seq_id, 0);
    designer.connect_nodes(int2, 0, seq_id, 1);

    let result = evaluate_node(&designer, "test", seq_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 2, "Should have 2 elements");
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_sequence_three_inputs() {
    let mut designer = setup_designer_with_network("test");

    let int1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int2 = designer.add_node("int", DVec2::new(0.0, 100.0));
    let int3 = designer.add_node("int", DVec2::new(0.0, 200.0));
    let seq_id = designer.add_node("sequence", DVec2::new(200.0, 0.0));

    set_sequence_data(
        &mut designer,
        "test",
        seq_id,
        SequenceData {
            element_type: DataType::Int,
            input_count: 3,
        },
    );
    designer.validate_active_network();

    designer.connect_nodes(int1, 0, seq_id, 0);
    designer.connect_nodes(int2, 0, seq_id, 1);
    designer.connect_nodes(int3, 0, seq_id, 2);

    let result = evaluate_node(&designer, "test", seq_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 3, "Should have 3 elements");
        }
        _ => panic!("Expected Array"),
    }
}

// ============================================================================
// Unconnected pins are skipped
// ============================================================================

#[test]
fn test_sequence_unconnected_pins_skipped() {
    let mut designer = setup_designer_with_network("test");

    let int1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int2 = designer.add_node("int", DVec2::new(0.0, 200.0));
    let seq_id = designer.add_node("sequence", DVec2::new(200.0, 0.0));

    set_sequence_data(
        &mut designer,
        "test",
        seq_id,
        SequenceData {
            element_type: DataType::Int,
            input_count: 3,
        },
    );
    designer.validate_active_network();

    // Connect pin 0 and pin 2, leave pin 1 unconnected
    designer.connect_nodes(int1, 0, seq_id, 0);
    designer.connect_nodes(int2, 0, seq_id, 2);

    let result = evaluate_node(&designer, "test", seq_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(
                items.len(),
                2,
                "Should have 2 elements (unconnected pin skipped)"
            );
        }
        _ => panic!("Expected Array"),
    }
}

// ============================================================================
// Type selector changes output array type
// ============================================================================

#[test]
fn test_sequence_element_type_changes_output() {
    let data = SequenceData {
        element_type: DataType::Float,
        input_count: 2,
    };

    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let base_type = registry.get_node_type("sequence").unwrap();
    let custom = data.calculate_custom_node_type(base_type).unwrap();

    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Float))
    );
    assert_eq!(custom.parameters.len(), 2);
    assert_eq!(custom.parameters[0].data_type, DataType::Float);
    assert_eq!(custom.parameters[1].data_type, DataType::Float);

    // Change to Blueprint
    let data2 = SequenceData {
        element_type: DataType::Blueprint,
        input_count: 3,
    };
    let custom2 = data2.calculate_custom_node_type(base_type).unwrap();
    assert_eq!(
        *custom2.output_type(),
        DataType::Array(Box::new(DataType::Blueprint))
    );
    assert_eq!(custom2.parameters.len(), 3);
    for p in &custom2.parameters {
        assert_eq!(p.data_type, DataType::Blueprint);
    }
}

// ============================================================================
// Parameter IDs for wire preservation
// ============================================================================

#[test]
fn test_sequence_parameter_ids_are_pin_indices() {
    let data = SequenceData {
        element_type: DataType::HasAtoms,
        input_count: 5,
    };

    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let base_type = registry.get_node_type("sequence").unwrap();
    let custom = data.calculate_custom_node_type(base_type).unwrap();

    for (i, param) in custom.parameters.iter().enumerate() {
        assert_eq!(param.id, Some(i as u64), "Pin {} should have id {}", i, i);
        assert_eq!(param.name, format!("{}", i));
    }
}

#[test]
fn test_sequence_changing_count_preserves_parameter_ids() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let base_type = registry.get_node_type("sequence").unwrap();

    // With 5 pins, IDs are 0,1,2,3,4
    let data5 = SequenceData {
        element_type: DataType::HasAtoms,
        input_count: 5,
    };
    let custom5 = data5.calculate_custom_node_type(base_type).unwrap();

    // Reduce to 3 pins
    let data3 = SequenceData {
        element_type: DataType::HasAtoms,
        input_count: 3,
    };
    let custom3 = data3.calculate_custom_node_type(base_type).unwrap();

    // First 3 pins should keep same IDs
    for i in 0..3 {
        assert_eq!(custom5.parameters[i].id, custom3.parameters[i].id);
    }
}

// ============================================================================
// Text properties roundtrip
// ============================================================================

#[test]
fn test_sequence_text_properties_roundtrip() {
    let original = SequenceData {
        element_type: DataType::Blueprint,
        input_count: 5,
    };

    let props = original.get_text_properties();
    assert_eq!(props.len(), 2);

    let mut restored = SequenceData::default();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.element_type, original.element_type);
    assert_eq!(restored.input_count, original.input_count);
}

#[test]
fn test_sequence_text_properties_values() {
    let data = SequenceData {
        element_type: DataType::HasAtoms,
        input_count: 3,
    };

    let props = data.get_text_properties();
    assert_eq!(
        props[0],
        (
            "element_type".to_string(),
            TextValue::DataType(DataType::HasAtoms)
        )
    );
    assert_eq!(props[1], ("count".to_string(), TextValue::Int(3)));
}

#[test]
fn test_sequence_set_text_properties_minimum_count() {
    let mut data = SequenceData::default();

    let mut props = HashMap::new();
    props.insert("count".to_string(), TextValue::Int(0));
    let result = data.set_text_properties(&props);
    assert!(result.is_err(), "count=0 should fail");
    assert!(result.unwrap_err().contains("at least 1"));
}

// ============================================================================
// Serialization roundtrip (serde)
// ============================================================================

#[test]
fn test_sequence_data_serde_roundtrip() {
    let original = SequenceData {
        element_type: DataType::HasAtoms,
        input_count: 4,
    };

    let json = serde_json::to_value(&original).unwrap();
    let restored: SequenceData = serde_json::from_value(json).unwrap();

    assert_eq!(restored.element_type, original.element_type);
    assert_eq!(restored.input_count, original.input_count);
}

#[test]
fn test_sequence_data_json_format() {
    let data = SequenceData {
        element_type: DataType::HasAtoms,
        input_count: 3,
    };

    let json = serde_json::to_value(&data).unwrap();
    assert_eq!(json["element_type"], "HasAtoms");
    assert_eq!(json["input_count"], 3);
}

// ============================================================================
// Text format roundtrip (serialize network -> edit network)
// ============================================================================

#[test]
fn test_sequence_text_format_roundtrip() {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();

    let create_network = || {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            summary: None,
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_pins: OutputPinDefinition::single(DataType::Array(Box::new(DataType::Int))),
            public: true,
            node_data_creator: || {
                Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
            },
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    };

    // Test properties-only roundtrip (no wiring — numeric pin names
    // like "0", "1" are not valid identifiers in the text format parser)
    let mut network = create_network();

    let result = edit_network(
        &mut network,
        &registry,
        r#"
            seq1 = sequence { element_type: Int, count: 3 }
            output seq1
        "#,
        true,
    );
    assert!(
        result.success,
        "Initial edit should succeed: {:?}",
        result.errors
    );

    // Verify the node was created with correct properties
    let seq_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "sequence")
        .unwrap();
    let props: HashMap<String, TextValue> =
        seq_node.data.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("element_type"),
        Some(&TextValue::DataType(DataType::Int))
    );
    assert_eq!(props.get("count"), Some(&TextValue::Int(3)));

    // Serialize
    let serialized = serialize_network(&network, &registry, None);
    assert!(
        serialized.contains("sequence"),
        "Serialized text should contain 'sequence'"
    );
    assert!(
        serialized.contains("element_type: Int"),
        "Should contain element_type property"
    );
    assert!(
        serialized.contains("count: 3"),
        "Should contain count property"
    );

    // Roundtrip: load into a fresh network
    let mut network2 = create_network();
    let result2 = edit_network(&mut network2, &registry, &serialized, true);
    assert!(
        result2.success,
        "Roundtrip edit should succeed: {:?}",
        result2.errors
    );
    assert_eq!(
        network.nodes.len(),
        network2.nodes.len(),
        "Networks should have same number of nodes"
    );
}

// ============================================================================
// Default values
// ============================================================================

#[test]
fn test_sequence_default() {
    let data = SequenceData::default();
    assert_eq!(data.element_type, DataType::HasAtoms);
    assert_eq!(data.input_count, 2);
}

// ============================================================================
// Node type registration
// ============================================================================

#[test]
fn test_sequence_registered_in_registry() {
    let registry =
        rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry::new();
    let node_type = registry.get_node_type("sequence");
    assert!(node_type.is_some(), "sequence should be registered");
    assert_eq!(node_type.unwrap().name, "sequence");
    assert!(node_type.unwrap().public);
}

// ============================================================================
// clone_box
// ============================================================================

#[test]
fn test_sequence_clone_box() {
    let data = SequenceData {
        element_type: DataType::Float,
        input_count: 7,
    };
    let cloned = data.clone_box();
    let props = cloned.get_text_properties();
    let map = props_to_hashmap(props);
    assert_eq!(
        map.get("element_type"),
        Some(&TextValue::DataType(DataType::Float))
    );
    assert_eq!(map.get("count"), Some(&TextValue::Int(7)));
}
