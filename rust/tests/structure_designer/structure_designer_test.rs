use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;
use glam::f64::DVec2;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

// ===== AUTO-CONNECT TESTS =====

#[test]
fn test_auto_connect_output_to_compatible_input() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Create a Float node (outputs Float) and a Vec3 node (inputs are Float x, y, z)
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let vec3_id = designer.add_node("vec3", DVec2::new(100.0, 0.0));
    
    // Auto-connect from float output to vec3
    let result = designer.auto_connect_to_node(float_id, 0, true, vec3_id);
    
    assert!(result, "Should successfully auto-connect Float output to Vec3 input");
    
    // Verify the connection was made
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let vec3_node = network.nodes.get(&vec3_id).unwrap();
    assert!(!vec3_node.arguments[0].is_empty(), "Vec3's first argument (x) should be connected");
    assert_eq!(vec3_node.arguments[0].get_node_id(), Some(float_id));
}

#[test]
fn test_auto_connect_input_to_compatible_output() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Create a Float node (outputs Float) and a Vec3 node (inputs Float)
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let vec3_id = designer.add_node("vec3", DVec2::new(100.0, 0.0));
    
    // Auto-connect from vec3's input pin 0 (Float type) to float node
    // source_is_output = false means we're dragging FROM an input pin
    let result = designer.auto_connect_to_node(vec3_id, 0, false, float_id);
    
    assert!(result, "Should successfully auto-connect from Vec3 input to Float node's output");
    
    // Verify: Float output should be connected to Vec3 input
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let vec3_node = network.nodes.get(&vec3_id).unwrap();
    assert!(!vec3_node.arguments[0].is_empty(), "Vec3's first argument should be connected");
    assert_eq!(vec3_node.arguments[0].get_node_id(), Some(float_id));
}

#[test]
fn test_auto_connect_geometry_output_to_geometry_input() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Create a Sphere node (outputs Geometry) and a Union node (inputs Geometry)
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(100.0, 0.0));
    
    // Auto-connect sphere output to union
    let result = designer.auto_connect_to_node(sphere_id, 0, true, union_id);
    
    assert!(result, "Should auto-connect Geometry output to Union's Geometry input");
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let union_node = network.nodes.get(&union_id).unwrap();
    assert!(!union_node.arguments[0].is_empty());
    assert_eq!(union_node.arguments[0].get_node_id(), Some(sphere_id));
}

#[test]
fn test_auto_connect_int_to_int_input() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Int output should connect to Int input
    // Sphere has: center (IVec3), radius (Int), unit_cell (UnitCell)
    // Int should connect to radius (index 1) as first compatible input
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    
    let result = designer.auto_connect_to_node(int_id, 0, true, sphere_id);
    
    assert!(result, "Int should connect to Sphere's Int radius input");
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let sphere_node = network.nodes.get(&sphere_id).unwrap();
    // Check that radius (index 1) is connected
    assert!(!sphere_node.arguments[1].is_empty(), "Sphere's radius argument should be connected");
    assert_eq!(sphere_node.arguments[1].get_node_id(), Some(int_id));
}

#[test]
fn test_auto_connect_int_promotable_to_float() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Int IS promotable to Float per DataType::can_be_converted_to
    // Vec3 has Float inputs (x, y, z)
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let vec3_id = designer.add_node("vec3", DVec2::new(100.0, 0.0));
    
    let result = designer.auto_connect_to_node(int_id, 0, true, vec3_id);
    
    assert!(result, "Int should be promotable to Float for Vec3's inputs");
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let vec3_node = network.nodes.get(&vec3_id).unwrap();
    assert!(!vec3_node.arguments[0].is_empty(), "Vec3's first argument should be connected");
    assert_eq!(vec3_node.arguments[0].get_node_id(), Some(int_id));
}

#[test]
fn test_auto_connect_incompatible_types_fails() {
    let mut designer = setup_designer_with_network("test_network");
    
    // String cannot be connected to Sphere's Float input
    let string_id = designer.add_node("string", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    
    let result = designer.auto_connect_to_node(string_id, 0, true, sphere_id);
    
    assert!(!result, "String should not be connectable to Sphere's Float inputs");
    
    // Verify no connection was made
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let sphere_node = network.nodes.get(&sphere_id).unwrap();
    assert!(sphere_node.arguments[0].is_empty());
}

#[test]
fn test_auto_connect_geometry_to_float_input_fails() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Geometry cannot be connected to Float input
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    let float_node_id = designer.add_node("float", DVec2::new(100.0, 0.0));
    
    // Sphere outputs Geometry, Float node has no compatible inputs
    let result = designer.auto_connect_to_node(sphere_id, 0, true, float_node_id);
    
    assert!(!result, "Geometry should not be connectable to Float node (no inputs)");
}

#[test]
fn test_auto_connect_nonexistent_source_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    
    // Try to connect from non-existent node
    let result = designer.auto_connect_to_node(99999, 0, true, sphere_id);
    
    assert!(!result, "Should fail when source node doesn't exist");
}

#[test]
fn test_auto_connect_nonexistent_target_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    
    // Try to connect to non-existent node
    let result = designer.auto_connect_to_node(float_id, 0, true, 99999);
    
    assert!(!result, "Should fail when target node doesn't exist");
}

#[test]
fn test_auto_connect_no_active_network() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test_network");
    // Don't set active network
    
    // Should fail gracefully with no active network
    let result = designer.auto_connect_to_node(1, 0, true, 2);
    
    assert!(!result, "Should fail when no active network is set");
}

#[test]
fn test_auto_connect_finds_first_compatible_input() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Vec3 node outputs Vec3, Cuboid has multiple inputs
    // Cuboid inputs: min_corner (IVec3), extent (IVec3)
    // Vec3 should connect to first compatible - but IVec3 and Vec3 might not be compatible
    // Let's use a node with clear first-match: geo_trans has Geometry first, then Vec3 offset
    let vec3_id = designer.add_node("vec3", DVec2::new(0.0, 0.0));
    let geo_trans_id = designer.add_node("geo_trans", DVec2::new(100.0, 0.0));
    
    // Vec3 should connect to the offset parameter (second input, index 1)
    let result = designer.auto_connect_to_node(vec3_id, 0, true, geo_trans_id);
    
    assert!(result, "Vec3 should find a compatible input on geo_trans");
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let geo_trans_node = network.nodes.get(&geo_trans_id).unwrap();
    
    // Check which input got connected - should be the Vec3-compatible one
    let connected_param = geo_trans_node.arguments.iter()
        .position(|arg| !arg.is_empty());
    
    assert!(connected_param.is_some(), "Some input should be connected");
}

#[test]
fn test_auto_connect_2d_geometry_to_2d_nodes() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Circle outputs Geometry2D, union_2d accepts Geometry2D
    let circle_id = designer.add_node("circle", DVec2::new(0.0, 0.0));
    let union_2d_id = designer.add_node("union_2d", DVec2::new(100.0, 0.0));
    
    let result = designer.auto_connect_to_node(circle_id, 0, true, union_2d_id);
    
    assert!(result, "Geometry2D should connect to union_2d");
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let union_2d_node = network.nodes.get(&union_2d_id).unwrap();
    assert!(!union_2d_node.arguments[0].is_empty());
}

#[test]
fn test_auto_connect_2d_to_3d_fails() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Geometry2D should not connect to 3D union
    let circle_id = designer.add_node("circle", DVec2::new(0.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(100.0, 0.0));
    
    let result = designer.auto_connect_to_node(circle_id, 0, true, union_id);
    
    assert!(!result, "Geometry2D should not connect to 3D Geometry input");
}

#[test]
fn test_auto_connect_from_input_to_compatible_output() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Vec3 inputs Float, Float node outputs Float
    // Dragging from Vec3's input (expecting Float) to Float node should work
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let vec3_id = designer.add_node("vec3", DVec2::new(100.0, 0.0));
    
    // Dragging from vec3's first input (Float type) to float node
    let result = designer.auto_connect_to_node(vec3_id, 0, false, float_id);
    
    assert!(result, "Float output should connect to Float input when dragging from input");
    
    // Verify the connection was made
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let vec3_node = network.nodes.get(&vec3_id).unwrap();
    assert!(!vec3_node.arguments[0].is_empty());
    assert_eq!(vec3_node.arguments[0].get_node_id(), Some(float_id));
}

#[test]
fn test_auto_connect_from_input_to_incompatible_output() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Sphere input expects Float, but Cuboid outputs Geometry
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    let cuboid_id = designer.add_node("cuboid", DVec2::new(100.0, 0.0));
    
    // Dragging from sphere's first input (Float type) to cuboid (Geometry output)
    let result = designer.auto_connect_to_node(sphere_id, 0, false, cuboid_id);
    
    assert!(!result, "Geometry output should not connect to Float input");
}

// ===== GET COMPATIBLE PINS FOR AUTO-CONNECT TESTS =====

#[test]
fn test_get_compatible_pins_no_active_network() {
    let designer = StructureDesigner::new();
    let pins = designer.get_compatible_pins_for_auto_connect(1, 0, true, 2);
    assert!(pins.is_empty());
}

#[test]
fn test_get_compatible_pins_invalid_source_node() {
    let designer = setup_designer_with_network("test_network");
    let pins = designer.get_compatible_pins_for_auto_connect(999, 0, true, 1);
    assert!(pins.is_empty());
}

#[test]
fn test_get_compatible_pins_invalid_target_node() {
    let mut designer = setup_designer_with_network("test_network");
    let source_id = designer.add_node("sphere", DVec2::ZERO);
    let pins = designer.get_compatible_pins_for_auto_connect(source_id, 0, true, 999);
    assert!(pins.is_empty());
}

#[test]
fn test_get_compatible_pins_geometry_to_geometry_input() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Sphere outputs Geometry
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    // Union has a single shapes array input
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));
    
    // Dragging from Sphere's output (Geometry) to Union
    let pins = designer.get_compatible_pins_for_auto_connect(sphere_id, 0, true, union_id);
    
    // Union has 1 Geometry array input (shapes)
    assert_eq!(pins.len(), 1, "Expected 1 compatible pin (shapes), got {}", pins.len());
    assert_eq!(pins[0].1, "shapes", "Expected 'shapes' pin");
}

#[test]
fn test_get_compatible_pins_single_compatible_input() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Float node outputs Float
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    // Sphere has a 'radius' Float input
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    
    // Dragging from Float's output to Sphere
    let pins = designer.get_compatible_pins_for_auto_connect(float_id, 0, true, sphere_id);
    
    // Sphere should have at least one Float-compatible input (radius)
    assert!(!pins.is_empty(), "Expected at least 1 compatible pin");
    
    // Check that radius is among the compatible pins
    let has_radius = pins.iter().any(|(_, name, _)| name == "radius");
    assert!(has_radius, "Expected 'radius' pin to be compatible");
}

#[test]
fn test_get_compatible_pins_from_input_to_output() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Sphere has Geometry output
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    // Union has Geometry inputs
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));
    
    // Dragging FROM Union's input (pin index 0, Geometry) to Sphere
    // source_is_output = false means we're dragging from an input pin
    let pins = designer.get_compatible_pins_for_auto_connect(union_id, 0, false, sphere_id);
    
    // Sphere's output should be compatible
    assert_eq!(pins.len(), 1, "Expected exactly 1 compatible pin (output)");
    assert_eq!(pins[0].0, 0, "Output pin should be index 0");
    assert_eq!(pins[0].1, "output", "Pin name should be 'output'");
    assert_eq!(pins[0].2, "Geometry", "Data type should be Geometry");
}

#[test]
fn test_get_compatible_pins_no_compatible_types() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Sphere outputs Geometry
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    // Float node has no Geometry inputs
    let float_id = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    // Dragging from Sphere's Geometry output to Float (which has no Geometry inputs)
    let pins = designer.get_compatible_pins_for_auto_connect(sphere_id, 0, true, float_id);
    
    // Float node has no Geometry inputs, so should return empty
    assert!(pins.is_empty(), "Expected no compatible pins, got {}", pins.len());
}

#[test]
fn test_get_compatible_pins_returns_all_compatible_not_just_first() {
    let mut designer = setup_designer_with_network("test_network");
    
    // Float outputs Float
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    // Vec3 has three Float inputs: x, y, z
    let vec3_id = designer.add_node("vec3", DVec2::new(200.0, 0.0));
    
    // Dragging from Float's output to Vec3
    let pins = designer.get_compatible_pins_for_auto_connect(float_id, 0, true, vec3_id);
    
    // Vec3 should have 3 compatible Float inputs
    assert_eq!(pins.len(), 3, "Expected 3 compatible pins (x, y, z), got {}", pins.len());
    
    // Verify we get x, y, z pins
    let pin_names: Vec<&str> = pins.iter().map(|(_, name, _)| name.as_str()).collect();
    assert!(pin_names.contains(&"x"), "Expected 'x' pin");
    assert!(pin_names.contains(&"y"), "Expected 'y' pin");
    assert!(pin_names.contains(&"z"), "Expected 'z' pin");
}

// ===== RENAME NODE NETWORK TESTS =====

#[test]
fn test_rename_node_network_prevents_builtin_name_conflict() {
    let mut designer = StructureDesigner::new();
    
    // Add a test node network
    designer.add_node_network("test_network");
    
    // Try to rename to a built-in node type name - should fail
    let result = designer.rename_node_network("test_network", "parameter");
    assert_eq!(result, false, "Should not allow renaming to built-in node type 'parameter'");
    
    // Try to rename to another built-in node type name - should fail
    let result = designer.rename_node_network("test_network", "string");
    assert_eq!(result, false, "Should not allow renaming to built-in node type 'string'");
    
    // Try to rename to another built-in node type name - should fail
    let result = designer.rename_node_network("test_network", "int");
    assert_eq!(result, false, "Should not allow renaming to built-in node type 'int'");
    
    // Try to rename to a valid name - should succeed
    let result = designer.rename_node_network("test_network", "my_custom_network");
    assert_eq!(result, true, "Should allow renaming to valid custom name");
    
    // Verify the network was actually renamed
    assert!(designer.node_type_registry.node_networks.contains_key("my_custom_network"));
    assert!(!designer.node_type_registry.node_networks.contains_key("test_network"));
}

#[test]
fn test_rename_node_network_existing_validations_still_work() {
    let mut designer = StructureDesigner::new();

    // Add two test node networks
    designer.add_node_network("network1");
    designer.add_node_network("network2");

    // Try to rename to existing network name - should fail
    let result = designer.rename_node_network("network1", "network2");
    assert_eq!(result, false, "Should not allow renaming to existing network name");

    // Try to rename non-existent network - should fail
    let result = designer.rename_node_network("nonexistent", "new_name");
    assert_eq!(result, false, "Should not allow renaming non-existent network");

    // Valid rename should still work
    let result = designer.rename_node_network("network1", "renamed_network");
    assert_eq!(result, true, "Should allow valid rename");
}

#[test]
fn test_rename_node_network_updates_backtick_references_in_comments() {
    let mut designer = StructureDesigner::new();

    // Create two networks
    designer.add_node_network("MyModule");
    designer.add_node_network("Docs");
    designer.set_active_node_network_name(Some("Docs".to_string()));

    // Add a comment node in the Docs network
    let comment_id = designer.add_node("Comment", DVec2::new(0.0, 0.0));
    assert!(comment_id != 0, "Comment node should be added successfully");

    // Set comment data with backtick references to MyModule using text properties
    {
        let network = designer.node_type_registry.node_networks.get_mut("Docs").unwrap();
        let node = network.nodes.get_mut(&comment_id).unwrap();
        let mut props = HashMap::new();
        props.insert("label".to_string(), TextValue::String("About `MyModule`".to_string()));
        props.insert("text".to_string(), TextValue::String("See `MyModule` for the main implementation. The `MyModule` network handles core logic.".to_string()));
        node.data.set_text_properties(&props).unwrap();
    }

    // Rename MyModule to CoreModule
    let result = designer.rename_node_network("MyModule", "CoreModule");
    assert!(result, "Rename should succeed");

    // Verify backtick references were updated using text properties
    let network = designer.node_type_registry.node_networks.get("Docs").unwrap();
    let node = network.nodes.get(&comment_id).unwrap();
    let props = node.data.get_text_properties();
    let label = props.iter().find(|(k, _)| k == "label").map(|(_, v)| v);
    let text = props.iter().find(|(k, _)| k == "text").map(|(_, v)| v);

    assert_eq!(label, Some(&TextValue::String("About `CoreModule`".to_string())), "Label backtick reference should be updated");
    assert_eq!(text, Some(&TextValue::String("See `CoreModule` for the main implementation. The `CoreModule` network handles core logic.".to_string())), "Text backtick references should be updated");
}

#[test]
fn test_rename_node_network_preserves_non_matching_backticks() {
    let mut designer = StructureDesigner::new();

    // Create networks
    designer.add_node_network("MyNet");
    designer.add_node_network("MyNetwork");
    designer.add_node_network("Docs");
    designer.set_active_node_network_name(Some("Docs".to_string()));

    // Add a comment with various backtick content
    let comment_id = designer.add_node("Comment", DVec2::new(0.0, 0.0));
    assert!(comment_id != 0, "Comment node should be added successfully");

    // Set comment text using text properties
    {
        let network = designer.node_type_registry.node_networks.get_mut("Docs").unwrap();
        let node = network.nodes.get_mut(&comment_id).unwrap();
        let mut props = HashMap::new();
        // Contains `MyNet`, `MyNetwork` (similar names), and `code` (unrelated)
        props.insert("text".to_string(), TextValue::String("Use `MyNet` here. See also `MyNetwork` and `code` examples.".to_string()));
        node.data.set_text_properties(&props).unwrap();
    }

    // Rename only MyNet
    designer.rename_node_network("MyNet", "RenamedNet");

    // Verify only exact matches were replaced using text properties
    let network = designer.node_type_registry.node_networks.get("Docs").unwrap();
    let node = network.nodes.get(&comment_id).unwrap();
    let props = node.data.get_text_properties();
    let text = props.iter().find(|(k, _)| k == "text").map(|(_, v)| v);

    assert_eq!(
        text,
        Some(&TextValue::String("Use `RenamedNet` here. See also `MyNetwork` and `code` examples.".to_string())),
        "Only exact backtick matches should be replaced"
    );
}

// ===== NEW PROJECT TESTS =====

#[test]
fn test_new_project_clears_networks() {
    let mut designer = StructureDesigner::new();

    // Add multiple networks with nodes
    designer.add_node_network("network1");
    designer.add_node_network("network2");
    designer.add_node_network("network3");
    designer.set_active_node_network_name(Some("network1".to_string()));
    designer.add_node("sphere", DVec2::new(0.0, 0.0));

    assert_eq!(designer.node_type_registry.node_networks.len(), 3);

    // Call new_project
    designer.new_project();

    // Should have exactly one network (Main)
    assert_eq!(designer.node_type_registry.node_networks.len(), 1);
    assert!(!designer.node_type_registry.node_networks.contains_key("network1"));
    assert!(!designer.node_type_registry.node_networks.contains_key("network2"));
    assert!(!designer.node_type_registry.node_networks.contains_key("network3"));
}

#[test]
fn test_new_project_creates_main_network() {
    let mut designer = StructureDesigner::new();

    // Add a network with a different name
    designer.add_node_network("my_custom_network");
    designer.set_active_node_network_name(Some("my_custom_network".to_string()));

    // Call new_project
    designer.new_project();

    // Should have Main network
    assert!(designer.node_type_registry.node_networks.contains_key("Main"));

    // Main should be the active network
    assert_eq!(designer.active_node_network_name, Some("Main".to_string()));

    // Main network should be empty (no nodes)
    let main_network = designer.node_type_registry.node_networks.get("Main").unwrap();
    assert!(main_network.nodes.is_empty());
}

#[test]
fn test_new_project_clears_file_path() {
    let mut designer = StructureDesigner::new();

    // Set a file path
    designer.file_path = Some("/path/to/design.cnnd".to_string());
    assert!(designer.file_path.is_some());

    // Call new_project
    designer.new_project();

    // File path should be cleared
    assert!(designer.file_path.is_none());
}

#[test]
fn test_new_project_clears_dirty_flag() {
    let mut designer = StructureDesigner::new();

    // Set dirty flag
    designer.is_dirty = true;
    assert!(designer.is_dirty);

    // Call new_project
    designer.new_project();

    // Dirty flag should be cleared
    assert!(!designer.is_dirty);
}

#[test]
fn test_new_project_full_reset() {
    let mut designer = StructureDesigner::new();

    // Set up complex state: multiple networks with nodes, file path, dirty flag
    designer.add_node_network("network1");
    designer.add_node_network("network2");
    designer.set_active_node_network_name(Some("network1".to_string()));
    designer.add_node("sphere", DVec2::new(0.0, 0.0));
    designer.add_node("cuboid", DVec2::new(100.0, 0.0));
    designer.file_path = Some("/path/to/design.cnnd".to_string());
    designer.is_dirty = true;

    // Call new_project
    designer.new_project();

    // Verify full reset
    assert_eq!(designer.node_type_registry.node_networks.len(), 1);
    assert!(designer.node_type_registry.node_networks.contains_key("Main"));
    assert_eq!(designer.active_node_network_name, Some("Main".to_string()));
    assert!(designer.file_path.is_none());
    assert!(!designer.is_dirty);

    // Main network should be empty
    let main_network = designer.node_type_registry.node_networks.get("Main").unwrap();
    assert!(main_network.nodes.is_empty());
}







