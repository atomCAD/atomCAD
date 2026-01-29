use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    serializable_to_node_network, SerializableNode, SerializableNodeNetwork,
    SerializableNodeType,
};
use rust_lib_flutter_cad::structure_designer::node_network::Argument;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

fn create_built_in_node_types() -> std::collections::HashMap<String, rust_lib_flutter_cad::structure_designer::node_type::NodeType> {
    let registry = NodeTypeRegistry::new();
    registry.built_in_node_types
}

fn create_serializable_node(id: u64, node_type_name: &str, custom_name: Option<&str>) -> SerializableNode {
    // Provide proper default data based on node type
    let data = match node_type_name {
        "int" => serde_json::json!({"value": 0}),
        "float" => serde_json::json!({"value": 0.0}),
        _ => serde_json::json!({}),
    };

    SerializableNode {
        id,
        node_type_name: node_type_name.to_string(),
        custom_name: custom_name.map(|s| s.to_string()),
        position: DVec2::ZERO,
        arguments: vec![Argument::new()],
        data_type: node_type_name.to_string(),
        data,
    }
}

fn create_serializable_network(nodes: Vec<SerializableNode>) -> SerializableNodeNetwork {
    SerializableNodeNetwork {
        next_node_id: nodes.len() as u64 + 1,
        node_type: SerializableNodeType {
            name: "test_network".to_string(),
            description: "Test network".to_string(),
            summary: None,
            category: "Custom".to_string(),
            parameters: vec![],
            output_type: "Geometry".to_string(),
        },
        nodes,
        return_node_id: None,
        displayed_node_ids: vec![],
    }
}

// ===== MIGRATION TESTS (Phase 2) =====

#[test]
fn test_migration_assigns_names_to_old_nodes() {
    let built_ins = create_built_in_node_types();

    // Create a serializable network with nodes that have no custom_name (simulating old files)
    let serializable = create_serializable_network(vec![
        create_serializable_node(1, "int", None),
        create_serializable_node(2, "int", None),
    ]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    // Both nodes should now have names
    let node1 = network.nodes.get(&1).unwrap();
    let node2 = network.nodes.get(&2).unwrap();

    assert!(node1.custom_name.is_some(), "Node 1 should have a name assigned");
    assert!(node2.custom_name.is_some(), "Node 2 should have a name assigned");

    // Names should be unique
    let name1 = node1.custom_name.as_ref().unwrap();
    let name2 = node2.custom_name.as_ref().unwrap();
    assert_ne!(name1, name2, "Names should be unique");

    // Names should follow the pattern "int1", "int2"
    assert!(name1.starts_with("int"), "Name should start with type name");
    assert!(name2.starts_with("int"), "Name should start with type name");
}

#[test]
fn test_migration_preserves_existing_custom_names() {
    let built_ins = create_built_in_node_types();

    // Create a serializable network with a mix of named and unnamed nodes
    let serializable = create_serializable_network(vec![
        create_serializable_node(1, "int", Some("myint")),
        create_serializable_node(2, "int", None),
    ]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    // Existing name should be preserved
    let node1 = network.nodes.get(&1).unwrap();
    assert_eq!(node1.custom_name, Some("myint".to_string()), "Existing name should be preserved");

    // New name should be assigned to the other node
    let node2 = network.nodes.get(&2).unwrap();
    assert!(node2.custom_name.is_some(), "Node 2 should have a name assigned");
    assert_ne!(node2.custom_name.as_ref().unwrap(), "myint", "New name should be different from existing");
}

#[test]
fn test_migration_handles_multiple_types() {
    let built_ins = create_built_in_node_types();

    // Create nodes of different types without names
    let serializable = create_serializable_network(vec![
        create_serializable_node(1, "int", None),
        create_serializable_node(2, "float", None),
        create_serializable_node(3, "int", None),
        create_serializable_node(4, "float", None),
    ]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    let node1 = network.nodes.get(&1).unwrap();
    let node2 = network.nodes.get(&2).unwrap();
    let node3 = network.nodes.get(&3).unwrap();
    let node4 = network.nodes.get(&4).unwrap();

    // All nodes should have names
    assert!(node1.custom_name.is_some());
    assert!(node2.custom_name.is_some());
    assert!(node3.custom_name.is_some());
    assert!(node4.custom_name.is_some());

    // Names should be unique per type
    let name1 = node1.custom_name.as_ref().unwrap();
    let name2 = node2.custom_name.as_ref().unwrap();
    let name3 = node3.custom_name.as_ref().unwrap();
    let name4 = node4.custom_name.as_ref().unwrap();

    assert!(name1.starts_with("int"));
    assert!(name2.starts_with("float"));
    assert!(name3.starts_with("int"));
    assert!(name4.starts_with("float"));

    // Int nodes should have different numbers
    assert_ne!(name1, name3, "Int nodes should have unique names");
    // Float nodes should have different numbers
    assert_ne!(name2, name4, "Float nodes should have unique names");
}

#[test]
fn test_migration_respects_existing_name_counters() {
    let built_ins = create_built_in_node_types();

    // Create a network where one node already has "int2" as name,
    // and another unnamed int node
    let serializable = create_serializable_network(vec![
        create_serializable_node(1, "int", Some("int2")),
        create_serializable_node(2, "int", None),
    ]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    let node1 = network.nodes.get(&1).unwrap();
    let node2 = network.nodes.get(&2).unwrap();

    // Existing name should be preserved
    assert_eq!(node1.custom_name, Some("int2".to_string()));

    // New name should be int3 (not int1, to avoid future collisions)
    assert_eq!(node2.custom_name, Some("int3".to_string()), "New name should be int3 since int2 exists");
}

#[test]
fn test_migration_empty_network() {
    let built_ins = create_built_in_node_types();

    // Empty network should deserialize without errors
    let serializable = create_serializable_network(vec![]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();
    assert!(network.nodes.is_empty());
}

#[test]
fn test_migration_all_nodes_already_named() {
    let built_ins = create_built_in_node_types();

    // All nodes already have names - no migration needed
    let serializable = create_serializable_network(vec![
        create_serializable_node(1, "int", Some("a")),
        create_serializable_node(2, "int", Some("b")),
    ]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    // Names should be preserved exactly
    assert_eq!(network.nodes.get(&1).unwrap().custom_name, Some("a".to_string()));
    assert_eq!(network.nodes.get(&2).unwrap().custom_name, Some("b".to_string()));
}
