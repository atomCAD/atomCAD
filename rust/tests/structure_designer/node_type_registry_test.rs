use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::node_type::{no_data_saver, no_data_loader};
use rust_lib_flutter_cad::structure_designer::node_data::NoData;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

fn create_test_network(name: &str) -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: name.to_string(),
        description: "".to_string(),
        category: NodeTypeCategory::Custom,
        parameters: Vec::new(),
        output_type: DataType::None,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
        public: true,
    })
}

#[test]
fn test_get_compatible_node_types_from_geometry_output() {
    let registry = NodeTypeRegistry::new();
    
    // Dragging from a Geometry output pin
    let categories = registry.get_compatible_node_types(&DataType::Geometry, true);
    
    // Should find nodes with Geometry input pins
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // Union, Intersect, Diff all accept Geometry inputs
    assert!(all_node_names.contains(&"union"), "union should accept Geometry");
    assert!(all_node_names.contains(&"intersect"), "intersect should accept Geometry");
    assert!(all_node_names.contains(&"diff"), "diff should accept Geometry");
    assert!(all_node_names.contains(&"atom_fill"), "atom_fill should accept Geometry");
    
    // Float node should NOT be in the list (no Geometry input)
    assert!(!all_node_names.contains(&"float"), "float should not accept Geometry");
}

#[test]
fn test_get_compatible_node_types_from_float_output() {
    let registry = NodeTypeRegistry::new();
    
    // Dragging from a Float output pin
    let categories = registry.get_compatible_node_types(&DataType::Float, true);
    
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // Sphere has Int radius input, and Float converts to Int
    assert!(all_node_names.contains(&"sphere"), "sphere should accept Float (converts to Int)");
    
    // vec3 node accepts Float inputs
    assert!(all_node_names.contains(&"vec3"), "vec3 should accept Float");
}

#[test]
fn test_get_compatible_node_types_to_geometry_input() {
    let registry = NodeTypeRegistry::new();
    
    // Dragging from a Geometry INPUT pin (looking for nodes that OUTPUT Geometry)
    let categories = registry.get_compatible_node_types(&DataType::Geometry, false);
    
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // These nodes output Geometry
    assert!(all_node_names.contains(&"sphere"), "sphere outputs Geometry");
    assert!(all_node_names.contains(&"cuboid"), "cuboid outputs Geometry");
    assert!(all_node_names.contains(&"union"), "union outputs Geometry");
    assert!(all_node_names.contains(&"extrude"), "extrude outputs Geometry");
    
    // Float node outputs Float, not Geometry
    assert!(!all_node_names.contains(&"float"), "float does not output Geometry");
}

#[test]
fn test_get_compatible_node_types_to_float_input() {
    let registry = NodeTypeRegistry::new();
    
    // Dragging from a Float INPUT pin (looking for nodes that OUTPUT Float)
    let categories = registry.get_compatible_node_types(&DataType::Float, false);
    
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // Float node outputs Float
    assert!(all_node_names.contains(&"float"), "float outputs Float");
    
    // Int converts to Float
    assert!(all_node_names.contains(&"int"), "int output converts to Float");
    
    // Sphere outputs Geometry, not Float
    assert!(!all_node_names.contains(&"sphere"), "sphere does not output Float");
}

#[test]
fn test_get_compatible_node_types_array_compatibility() {
    let registry = NodeTypeRegistry::new();
    
    // Dragging single Geometry - should match nodes with [Geometry] array inputs
    // because DataType::can_be_converted_to allows T -> [T] conversion
    let categories = registry.get_compatible_node_types(&DataType::Geometry, true);
    
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // Union accepts [Geometry] array, and single Geometry can convert to [Geometry]
    assert!(all_node_names.contains(&"union"), "union should accept single Geometry (converts to array)");
}

#[test]
fn test_get_compatible_node_types_returns_grouped_categories() {
    let registry = NodeTypeRegistry::new();
    
    let categories = registry.get_compatible_node_types(&DataType::Geometry, true);
    
    // Should have categories, not just a flat list
    assert!(!categories.is_empty(), "Should return at least one category");
    
    // Each category should have nodes
    for category in &categories {
        assert!(!category.nodes.is_empty(), "Category should have nodes");
    }
}

#[test]
fn test_get_compatible_node_types_no_matches() {
    let registry = NodeTypeRegistry::new();
    
    // UnitCell is a specialized type - few nodes output it
    // When dragging FROM UnitCell output, looking for nodes with UnitCell input
    let categories = registry.get_compatible_node_types(&DataType::UnitCell, true);
    
    // Should still return valid result (possibly with matches like atom_fill, drawing_plane)
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // drawing_plane and atom_fill accept UnitCell
    assert!(all_node_names.contains(&"drawing_plane") || all_node_names.contains(&"atom_fill"), 
        "Should find nodes that accept UnitCell");
}

#[test]
fn test_get_compatible_node_types_excludes_non_public_nodes() {
    let registry = NodeTypeRegistry::new();
    
    // Value node is not public (internal helper node)
    let categories = registry.get_compatible_node_types(&DataType::Float, true);
    
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // Value node should not appear (it's not public)
    assert!(!all_node_names.contains(&"value"), "value node should not be in results (not public)");
}

// ===== BASIC REGISTRY TESTS =====

#[test]
fn test_new_registry_has_built_in_node_types() {
    let registry = NodeTypeRegistry::new();
    
    // Should have many built-in node types
    assert!(registry.built_in_node_types.len() > 20, "Should have many built-in node types");
    
    // Should have no node networks initially
    assert!(registry.node_networks.is_empty(), "Should have no custom networks initially");
    
    // Should have no design file name
    assert!(registry.design_file_name.is_none());
}

#[test]
fn test_get_node_type_returns_built_in_types() {
    let registry = NodeTypeRegistry::new();
    
    // Should find common built-in types
    assert!(registry.get_node_type("float").is_some(), "Should find float node type");
    assert!(registry.get_node_type("int").is_some(), "Should find int node type");
    assert!(registry.get_node_type("sphere").is_some(), "Should find sphere node type");
    assert!(registry.get_node_type("union").is_some(), "Should find union node type");
    
    // Should return None for non-existent types
    assert!(registry.get_node_type("nonexistent_type").is_none());
}

#[test]
fn test_get_node_type_returns_correct_properties() {
    let registry = NodeTypeRegistry::new();
    
    let float_type = registry.get_node_type("float").unwrap();
    assert_eq!(float_type.name, "float");
    assert_eq!(float_type.output_type, DataType::Float);
    
    let sphere_type = registry.get_node_type("sphere").unwrap();
    assert_eq!(sphere_type.name, "sphere");
    assert_eq!(sphere_type.output_type, DataType::Geometry);
}

#[test]
fn test_is_custom_node_type_returns_false_for_built_in() {
    let registry = NodeTypeRegistry::new();
    
    assert!(!registry.is_custom_node_type("float"), "float is not custom");
    assert!(!registry.is_custom_node_type("sphere"), "sphere is not custom");
    assert!(!registry.is_custom_node_type("union"), "union is not custom");
    assert!(!registry.is_custom_node_type("nonexistent"), "nonexistent is not custom");
}

#[test]
fn test_is_custom_node_type_returns_true_for_custom() {
    let mut registry = NodeTypeRegistry::new();
    
    registry.add_node_network(create_test_network("my_custom_network"));
    
    assert!(registry.is_custom_node_type("my_custom_network"), "my_custom_network is custom");
    assert!(!registry.is_custom_node_type("float"), "float is still not custom");
}

#[test]
fn test_add_node_network() {
    let mut registry = NodeTypeRegistry::new();
    
    assert!(registry.node_networks.is_empty());
    
    registry.add_node_network(create_test_network("test_network"));
    
    assert_eq!(registry.node_networks.len(), 1);
    assert!(registry.node_networks.contains_key("test_network"));
}

#[test]
fn test_get_node_network_names_empty() {
    let registry = NodeTypeRegistry::new();
    
    let names = registry.get_node_network_names();
    assert!(names.is_empty());
}

#[test]
fn test_get_node_network_names_returns_sorted() {
    let mut registry = NodeTypeRegistry::new();
    
    // Add networks in non-alphabetical order
    registry.add_node_network(create_test_network("zebra"));
    registry.add_node_network(create_test_network("alpha"));
    registry.add_node_network(create_test_network("middle"));
    
    let names = registry.get_node_network_names();
    
    assert_eq!(names.len(), 3);
    assert_eq!(names[0], "alpha");
    assert_eq!(names[1], "middle");
    assert_eq!(names[2], "zebra");
}

#[test]
fn test_get_node_type_views_returns_categories_in_order() {
    let registry = NodeTypeRegistry::new();
    
    let categories = registry.get_node_type_views();
    
    // Should have multiple categories
    assert!(categories.len() >= 5, "Should have at least 5 categories");
    
    // First category should be Annotation
    assert_eq!(categories[0].category, NodeTypeCategory::Annotation);
    
    // Categories should be in semantic order
    let category_order: Vec<NodeTypeCategory> = categories.iter()
        .map(|c| c.category.clone())
        .collect();
    
    // Check expected order (only categories that have nodes)
    let annotation_idx = category_order.iter().position(|c| *c == NodeTypeCategory::Annotation);
    let math_idx = category_order.iter().position(|c| *c == NodeTypeCategory::MathAndProgramming);
    let geo2d_idx = category_order.iter().position(|c| *c == NodeTypeCategory::Geometry2D);
    let geo3d_idx = category_order.iter().position(|c| *c == NodeTypeCategory::Geometry3D);
    let atomic_idx = category_order.iter().position(|c| *c == NodeTypeCategory::AtomicStructure);
    
    assert!(annotation_idx < math_idx, "Annotation should come before MathAndProgramming");
    assert!(math_idx < geo2d_idx, "MathAndProgramming should come before Geometry2D");
    assert!(geo2d_idx < geo3d_idx, "Geometry2D should come before Geometry3D");
    assert!(geo3d_idx < atomic_idx, "Geometry3D should come before AtomicStructure");
}

#[test]
fn test_get_node_type_views_nodes_sorted_alphabetically() {
    let registry = NodeTypeRegistry::new();
    
    let categories = registry.get_node_type_views();
    
    // For each category, nodes should be sorted alphabetically
    for category in &categories {
        let names: Vec<&str> = category.nodes.iter().map(|n| n.name.as_str()).collect();
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names, "Nodes in {:?} should be sorted", category.category);
    }
}

#[test]
fn test_get_node_type_views_includes_custom_networks() {
    let mut registry = NodeTypeRegistry::new();
    
    registry.add_node_network(create_test_network("my_custom_node"));
    
    let categories = registry.get_node_type_views();
    
    // Should have a Custom category
    let custom_category = categories.iter().find(|c| c.category == NodeTypeCategory::Custom);
    assert!(custom_category.is_some(), "Should have Custom category");
    
    let custom_nodes: Vec<&str> = custom_category.unwrap().nodes.iter()
        .map(|n| n.name.as_str())
        .collect();
    assert!(custom_nodes.contains(&"my_custom_node"));
}

#[test]
fn test_get_node_type_views_excludes_non_public_nodes() {
    let registry = NodeTypeRegistry::new();
    
    let categories = registry.get_node_type_views();
    
    let all_node_names: Vec<&str> = categories.iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    
    // value node is not public, should not appear
    assert!(!all_node_names.contains(&"value"), "value node should not appear (not public)");
}

#[test]
fn test_get_node_type_finds_custom_network_type() {
    let mut registry = NodeTypeRegistry::new();
    
    registry.add_node_network(create_test_network("custom_geo"));
    
    // Should find the custom network as a node type
    let node_type = registry.get_node_type("custom_geo");
    assert!(node_type.is_some(), "Should find custom network as node type");
    assert_eq!(node_type.unwrap().name, "custom_geo");
}

#[test]
fn test_get_node_networks_with_validation_empty() {
    let registry = NodeTypeRegistry::new();
    
    let networks = registry.get_node_networks_with_validation();
    assert!(networks.is_empty());
}

#[test]
fn test_get_node_networks_with_validation_returns_sorted() {
    let mut registry = NodeTypeRegistry::new();
    
    registry.add_node_network(create_test_network("zebra_net"));
    registry.add_node_network(create_test_network("alpha_net"));
    
    let networks = registry.get_node_networks_with_validation();
    
    assert_eq!(networks.len(), 2);
    assert_eq!(networks[0].name, "alpha_net");
    assert_eq!(networks[1].name, "zebra_net");
}
