use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::array_concat::ArrayConcatData;
use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
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

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
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
// Default values & registration
// ============================================================================

#[test]
fn test_array_concat_default() {
    let data = ArrayConcatData::default();
    assert_eq!(data.element_type, DataType::Int);
}

#[test]
fn test_array_concat_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("array_concat");
    assert!(node_type.is_some(), "array_concat should be registered");
    let nt = node_type.unwrap();
    assert_eq!(nt.name, "array_concat");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "a");
    assert_eq!(nt.parameters[1].name, "b");
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::Array(Box::new(DataType::Int)));
}

// ============================================================================
// calculate_custom_node_type tests
// ============================================================================

#[test]
fn test_array_concat_custom_type_int() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("array_concat").unwrap();
    let data = ArrayConcatData {
        element_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::Int))
    );
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Array(Box::new(DataType::Int))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Int))
    );
}

#[test]
fn test_array_concat_custom_type_ivec3() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("array_concat").unwrap();
    let data = ArrayConcatData {
        element_type: DataType::IVec3,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::IVec3))
    );
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Array(Box::new(DataType::IVec3))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::IVec3))
    );
}

#[test]
fn test_array_concat_custom_type_structure() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("array_concat").unwrap();
    let data = ArrayConcatData {
        element_type: DataType::Structure,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Array(Box::new(DataType::Structure))
    );
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Array(Box::new(DataType::Structure))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Structure))
    );
}

// ============================================================================
// Evaluation: basic concatenation
// ============================================================================

#[test]
fn test_array_concat_int_two_non_empty() {
    let mut designer = setup_designer_with_network("test");

    // a = [1, 2]
    let range_a = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        range_a,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 2,
        }),
    );

    // b = [3, 4]
    let range_b = designer.add_node("range", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        range_b,
        Box::new(RangeData {
            start: 3,
            step: 1,
            count: 2,
        }),
    );

    let concat_id = designer.add_node("array_concat", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(range_a, 0, concat_id, 0);
    designer.connect_nodes(range_b, 0, concat_id, 1);

    let result = evaluate_node(&designer, "test", concat_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 4);
            let ints: Vec<i32> = items
                .iter()
                .map(|r| match r {
                    NetworkResult::Int(v) => *v,
                    other => panic!("Expected Int, got {:?}", other.to_display_string()),
                })
                .collect();
            assert_eq!(ints, vec![1, 2, 3, 4]);
        }
        other => panic!("Expected Array, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_array_concat_empty_left_non_empty_right() {
    let mut designer = setup_designer_with_network("test");

    // a = empty Array[Int] via sequence with input_count=1, no wires
    let seq_a = designer.add_node("sequence", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        seq_a,
        Box::new(SequenceData {
            element_type: DataType::Int,
            input_count: 1,
        }),
    );

    // b = [1, 2]
    let range_b = designer.add_node("range", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        range_b,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 2,
        }),
    );

    let concat_id = designer.add_node("array_concat", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(seq_a, 0, concat_id, 0);
    designer.connect_nodes(range_b, 0, concat_id, 1);

    let result = evaluate_node(&designer, "test", concat_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 2);
            let ints: Vec<i32> = items
                .iter()
                .map(|r| match r {
                    NetworkResult::Int(v) => *v,
                    other => panic!("Expected Int, got {:?}", other.to_display_string()),
                })
                .collect();
            assert_eq!(ints, vec![1, 2]);
        }
        other => panic!("Expected Array, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_array_concat_ivec3_preserves_order() {
    let mut designer = setup_designer_with_network("test");

    let v0 = designer.add_node("ivec3", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        v0,
        Box::new(IVec3Data {
            value: IVec3::new(1, 1, 1),
        }),
    );
    let v1 = designer.add_node("ivec3", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        v1,
        Box::new(IVec3Data {
            value: IVec3::new(2, 2, 2),
        }),
    );
    let v2 = designer.add_node("ivec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        "test",
        v2,
        Box::new(IVec3Data {
            value: IVec3::new(3, 3, 3),
        }),
    );
    let v3 = designer.add_node("ivec3", DVec2::new(0.0, 300.0));
    set_node_data(
        &mut designer,
        "test",
        v3,
        Box::new(IVec3Data {
            value: IVec3::new(4, 4, 4),
        }),
    );

    // a = [(1,1,1), (2,2,2)]
    let seq_a = designer.add_node("sequence", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        seq_a,
        Box::new(SequenceData {
            element_type: DataType::IVec3,
            input_count: 2,
        }),
    );
    // b = [(3,3,3), (4,4,4)]
    let seq_b = designer.add_node("sequence", DVec2::new(200.0, 200.0));
    set_node_data(
        &mut designer,
        "test",
        seq_b,
        Box::new(SequenceData {
            element_type: DataType::IVec3,
            input_count: 2,
        }),
    );
    designer.validate_active_network();
    designer.connect_nodes(v0, 0, seq_a, 0);
    designer.connect_nodes(v1, 0, seq_a, 1);
    designer.connect_nodes(v2, 0, seq_b, 0);
    designer.connect_nodes(v3, 0, seq_b, 1);

    let concat_id = designer.add_node("array_concat", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        concat_id,
        Box::new(ArrayConcatData {
            element_type: DataType::IVec3,
        }),
    );
    designer.validate_active_network();

    designer.connect_nodes(seq_a, 0, concat_id, 0);
    designer.connect_nodes(seq_b, 0, concat_id, 1);

    let result = evaluate_node(&designer, "test", concat_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 4);
            let vecs: Vec<IVec3> = items
                .iter()
                .map(|r| match r {
                    NetworkResult::IVec3(v) => *v,
                    other => panic!("Expected IVec3, got {:?}", other.to_display_string()),
                })
                .collect();
            assert_eq!(
                vecs,
                vec![
                    IVec3::new(1, 1, 1),
                    IVec3::new(2, 2, 2),
                    IVec3::new(3, 3, 3),
                    IVec3::new(4, 4, 4),
                ]
            );
        }
        other => panic!("Expected Array, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: unconnected pins propagate as None
// ============================================================================

#[test]
fn test_array_concat_unconnected_a_yields_none() {
    let mut designer = setup_designer_with_network("test");

    let range_b = designer.add_node("range", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        range_b,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 2,
        }),
    );

    let concat_id = designer.add_node("array_concat", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(range_b, 0, concat_id, 1);

    let result = evaluate_node(&designer, "test", concat_id);
    match result {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_array_concat_unconnected_b_yields_none() {
    let mut designer = setup_designer_with_network("test");

    let range_a = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        range_a,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 2,
        }),
    );

    let concat_id = designer.add_node("array_concat", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(range_a, 0, concat_id, 0);

    let result = evaluate_node(&designer, "test", concat_id);
    match result {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_array_concat_both_unconnected_yields_none() {
    let mut designer = setup_designer_with_network("test");

    let concat_id = designer.add_node("array_concat", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    let result = evaluate_node(&designer, "test", concat_id);
    match result {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Text properties roundtrip
// ============================================================================

#[test]
fn test_array_concat_text_properties_roundtrip() {
    let original = ArrayConcatData {
        element_type: DataType::Structure,
    };
    let props = original.get_text_properties();
    assert_eq!(props.len(), 1);

    let mut restored = ArrayConcatData::default();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.element_type, original.element_type);
}

#[test]
fn test_array_concat_text_properties_values() {
    let data = ArrayConcatData {
        element_type: DataType::IVec3,
    };
    let props = data.get_text_properties();
    assert_eq!(
        props[0],
        (
            "element_type".to_string(),
            TextValue::DataType(DataType::IVec3)
        )
    );
}

// ============================================================================
// Serde roundtrip
// ============================================================================

#[test]
fn test_array_concat_data_serde_roundtrip() {
    let original = ArrayConcatData {
        element_type: DataType::IVec3,
    };
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(json["element_type"], "IVec3");
    let restored: ArrayConcatData = serde_json::from_value(json).unwrap();
    assert_eq!(restored.element_type, original.element_type);
}

// ============================================================================
// clone_box
// ============================================================================

#[test]
fn test_array_concat_clone_box() {
    let data = ArrayConcatData {
        element_type: DataType::Float,
    };
    let cloned = data.clone_box();
    let props = cloned.get_text_properties();
    let map = props_to_hashmap(props);
    assert_eq!(
        map.get("element_type"),
        Some(&TextValue::DataType(DataType::Float))
    );
}

// ============================================================================
// Text format roundtrip (serialize_network → edit_network)
// ============================================================================

#[test]
fn test_array_concat_text_format_roundtrip() {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
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

    let mut network = create_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"
            cat1 = array_concat { element_type: IVec3 }
            output cat1
        "#,
        true,
    );
    assert!(
        result.success,
        "Initial edit should succeed: {:?}",
        result.errors
    );

    let cat_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "array_concat")
        .unwrap();
    let props: HashMap<String, TextValue> =
        cat_node.data.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("element_type"),
        Some(&TextValue::DataType(DataType::IVec3))
    );

    let serialized = serialize_network(&network, &registry, None);
    assert!(
        serialized.contains("array_concat"),
        "serialized text should contain 'array_concat'"
    );
    assert!(
        serialized.contains("element_type: IVec3"),
        "serialized text should contain element_type: {}",
        serialized
    );

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
        "networks should have same number of nodes"
    );
}

// ============================================================================
// .cnnd save/load roundtrip
// ============================================================================

#[test]
fn test_array_concat_cnnd_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let mut designer = setup_designer_with_network("main");

    let concat_id = designer.add_node("array_concat", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        concat_id,
        Box::new(ArrayConcatData {
            element_type: DataType::IVec3,
        }),
    );
    designer.validate_active_network();

    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("array_concat.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    let mut registry2 = NodeTypeRegistry::new();
    let _load = load_node_networks_from_file(&mut registry2, path.to_str().unwrap())
        .expect("load should succeed");

    let network = registry2
        .node_networks
        .get("main")
        .expect("main network should survive roundtrip");
    let (_, node) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "array_concat")
        .expect("array_concat node should survive roundtrip");
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<ArrayConcatData>()
        .expect("array_concat node should carry ArrayConcatData");
    assert_eq!(
        data.element_type,
        DataType::IVec3,
        "element_type should survive .cnnd roundtrip"
    );
}
