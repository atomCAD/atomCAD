//! Tests for the `collect` node (Phase 2 of `doc/design_iterators.md`).
//!
//! `collect` materializes an `Iter[T]` value into an `Array[T]` by exhausting
//! the walker. Phase 2 ships only the node itself — no upstream node yet
//! produces `NetworkResult::Iterator(_)` directly, but the implicit
//! `[T] → Iter[T]` wire conversion (Phase 1) reaches the `collect` input by
//! eagerly wrapping any incoming array as a `Walker::from_array`.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
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
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
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
fn test_collect_default() {
    let data = CollectData::default();
    assert_eq!(data.element_type, DataType::Int);
}

#[test]
fn test_collect_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("collect");
    assert!(node_type.is_some(), "collect should be registered");
    let nt = node_type.unwrap();
    assert_eq!(nt.name, "collect");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "iter");
    assert_eq!(
        nt.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::Array(Box::new(DataType::Int)));
}

// ============================================================================
// calculate_custom_node_type tests
// ============================================================================

#[test]
fn test_collect_custom_type_int() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("collect").unwrap();
    let data = CollectData {
        element_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Int))
    );
}

#[test]
fn test_collect_custom_type_float() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("collect").unwrap();
    let data = CollectData {
        element_type: DataType::Float,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Float))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Float))
    );
}

#[test]
fn test_collect_custom_type_structure() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("collect").unwrap();
    let data = CollectData {
        element_type: DataType::Structure,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();

    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Structure))
    );
    assert_eq!(
        *custom.output_type(),
        DataType::Array(Box::new(DataType::Structure))
    );
}

// ============================================================================
// Evaluation: array source feeds the iterator pin via the implicit
// `[T] → Iter[T]` wire conversion (Phase 1).
// ============================================================================

#[test]
fn test_collect_drains_array_source_into_array() {
    let mut designer = setup_designer_with_network("test");

    // Source: range produces [1, 2, 3] (Phase 2: range still emits Array[Int]).
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        range_id,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 3,
        }),
    );

    let collect_id = designer.add_node("collect", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(range_id, 0, collect_id, 0);

    let result = evaluate_node(&designer, "test", collect_id);
    match result {
        NetworkResult::Array(items) => {
            let ints: Vec<i32> = items
                .iter()
                .map(|r| match r {
                    NetworkResult::Int(v) => *v,
                    other => panic!("Expected Int, got {:?}", other.to_display_string()),
                })
                .collect();
            assert_eq!(ints, vec![1, 2, 3]);
        }
        other => panic!("Expected Array, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_collect_empty_array() {
    let mut designer = setup_designer_with_network("test");

    // sequence with input_count=1 and no wires produces an empty Array[Int].
    let seq_id = designer.add_node("sequence", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::Int,
            input_count: 1,
        }),
    );

    let collect_id = designer.add_node("collect", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(seq_id, 0, collect_id, 0);

    let result = evaluate_node(&designer, "test", collect_id);
    match result {
        NetworkResult::Array(items) => assert_eq!(items.len(), 0),
        other => panic!("Expected Array, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_collect_scalar_broadcasts_to_singleton_array() {
    // `Int → Iter[Int]` is the documented single-element broadcast rule.
    let mut designer = setup_designer_with_network("test");

    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        int_id,
        Box::new(IntData { value: 42 }),
    );

    let collect_id = designer.add_node("collect", DVec2::new(200.0, 0.0));
    designer.validate_active_network();

    designer.connect_nodes(int_id, 0, collect_id, 0);

    let result = evaluate_node(&designer, "test", collect_id);
    match result {
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 1);
            match &items[0] {
                NetworkResult::Int(v) => assert_eq!(*v, 42),
                other => panic!("Expected Int(42), got {:?}", other.to_display_string()),
            }
        }
        other => panic!("Expected Array, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: unconnected pin propagates as None
// ============================================================================

#[test]
fn test_collect_unconnected_iter_yields_none() {
    let mut designer = setup_designer_with_network("test");

    let collect_id = designer.add_node("collect", DVec2::new(0.0, 0.0));
    designer.validate_active_network();

    let result = evaluate_node(&designer, "test", collect_id);
    match result {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Text properties roundtrip
// ============================================================================

#[test]
fn test_collect_text_properties_roundtrip() {
    let original = CollectData {
        element_type: DataType::Structure,
    };
    let props = original.get_text_properties();
    assert_eq!(props.len(), 1);

    let mut restored = CollectData::default();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.element_type, original.element_type);
}

#[test]
fn test_collect_text_properties_values() {
    let data = CollectData {
        element_type: DataType::Float,
    };
    let props = data.get_text_properties();
    assert_eq!(
        props[0],
        (
            "element_type".to_string(),
            TextValue::DataType(DataType::Float)
        )
    );
}

// ============================================================================
// Serde roundtrip
// ============================================================================

#[test]
fn test_collect_data_serde_roundtrip() {
    let original = CollectData {
        element_type: DataType::Float,
    };
    let json = serde_json::to_value(&original).unwrap();
    assert_eq!(json["element_type"], "Float");
    let restored: CollectData = serde_json::from_value(json).unwrap();
    assert_eq!(restored.element_type, original.element_type);
}

// ============================================================================
// clone_box
// ============================================================================

#[test]
fn test_collect_clone_box() {
    let data = CollectData {
        element_type: DataType::Vec3,
    };
    let cloned = data.clone_box();
    let props = cloned.get_text_properties();
    let map = props_to_hashmap(props);
    assert_eq!(
        map.get("element_type"),
        Some(&TextValue::DataType(DataType::Vec3))
    );
}

// ============================================================================
// Text format roundtrip (serialize_network → edit_network)
// ============================================================================

#[test]
fn test_collect_text_format_roundtrip() {
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
            c1 = collect { element_type: Float }
            output c1
        "#,
        true,
    );
    assert!(
        result.success,
        "Initial edit should succeed: {:?}",
        result.errors
    );

    let collect_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "collect")
        .unwrap();
    let props: HashMap<String, TextValue> = collect_node
        .data
        .get_text_properties()
        .into_iter()
        .collect();
    assert_eq!(
        props.get("element_type"),
        Some(&TextValue::DataType(DataType::Float))
    );

    let serialized = serialize_network(&network, &registry, None);
    assert!(
        serialized.contains("collect"),
        "serialized text should contain 'collect'"
    );
    assert!(
        serialized.contains("element_type: Float"),
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
fn test_collect_cnnd_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let mut designer = setup_designer_with_network("main");

    let collect_id = designer.add_node("collect", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        collect_id,
        Box::new(CollectData {
            element_type: DataType::Vec3,
        }),
    );
    designer.validate_active_network();

    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("collect.cnnd");
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
        .find(|(_, n)| n.node_type_name == "collect")
        .expect("collect node should survive roundtrip");
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<CollectData>()
        .expect("collect node should carry CollectData");
    assert_eq!(
        data.element_type,
        DataType::Vec3,
        "element_type should survive .cnnd roundtrip"
    );
}
