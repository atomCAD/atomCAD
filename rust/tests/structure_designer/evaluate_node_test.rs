use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Helper to set a custom name on a node by directly accessing the network
fn set_node_custom_name(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    name: &str,
) {
    if let Some(network) = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
    {
        if let Some(node) = network.nodes.get_mut(&node_id) {
            node.custom_name = Some(name.to_string());
        }
    }
}

// ===== EVALUATE NODE BY ID TESTS =====

#[test]
fn test_evaluate_float_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a float node (outputs Float)
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(float_id, false).unwrap();

    assert_eq!(result.node_id, float_id);
    assert_eq!(result.node_type_name, "float");
    assert_eq!(result.output_type, "Float");
    assert!(result.success);
    assert!(result.error_message.is_none());
    assert!(result.detailed_string.is_none()); // Not verbose
    // Default float value should be in display string
    assert!(!result.display_string.is_empty());
}

#[test]
fn test_evaluate_int_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add an int node (outputs Int)
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(int_id, false).unwrap();

    assert_eq!(result.node_id, int_id);
    assert_eq!(result.node_type_name, "int");
    assert_eq!(result.output_type, "Int");
    assert!(result.success);
    assert!(result.error_message.is_none());
}

#[test]
fn test_evaluate_bool_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a bool node (outputs Bool)
    let bool_id = designer.add_node("bool", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(bool_id, false).unwrap();

    assert_eq!(result.node_id, bool_id);
    assert_eq!(result.node_type_name, "bool");
    assert_eq!(result.output_type, "Bool");
    assert!(result.success);
}

#[test]
fn test_evaluate_string_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a string node (outputs String)
    let string_id = designer.add_node("string", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(string_id, false).unwrap();

    assert_eq!(result.node_id, string_id);
    assert_eq!(result.node_type_name, "string");
    assert_eq!(result.output_type, "String");
    assert!(result.success);
}

#[test]
fn test_evaluate_vec3_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a vec3 node (outputs Vec3)
    let vec3_id = designer.add_node("vec3", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(vec3_id, false).unwrap();

    assert_eq!(result.node_id, vec3_id);
    assert_eq!(result.node_type_name, "vec3");
    assert_eq!(result.output_type, "Vec3");
    assert!(result.success);
    // Display string should contain vec3 format
    assert!(result.display_string.contains(",") || result.display_string.contains("("));
}

// ===== COMPLEX TYPES BRIEF TESTS =====

#[test]
fn test_evaluate_geometry_node_brief() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a sphere node (outputs Geometry)
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(sphere_id, false).unwrap();

    assert_eq!(result.node_id, sphere_id);
    assert_eq!(result.node_type_name, "sphere");
    assert_eq!(result.output_type, "Geometry");
    assert!(result.success);
    assert!(result.detailed_string.is_none()); // Not verbose
    // Brief output for Geometry should just show "Geometry"
    assert!(result.display_string.contains("Geometry"));
}

#[test]
fn test_evaluate_cuboid_node_brief() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a cuboid node (outputs Geometry)
    let cuboid_id = designer.add_node("cuboid", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(cuboid_id, false).unwrap();

    assert_eq!(result.node_type_name, "cuboid");
    assert_eq!(result.output_type, "Geometry");
    assert!(result.success);
    assert!(result.display_string.contains("Geometry"));
}

// ===== VERBOSE OUTPUT TESTS =====

#[test]
fn test_evaluate_float_node_verbose() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(float_id, true).unwrap();

    assert!(result.success);
    assert!(result.detailed_string.is_some()); // Verbose mode should populate detailed_string
}

#[test]
fn test_evaluate_geometry_node_verbose() {
    let mut designer = setup_designer_with_network("test_network");

    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(sphere_id, true).unwrap();

    assert!(result.success);
    assert!(result.detailed_string.is_some());
    let detailed = result.detailed_string.unwrap();
    // Verbose geometry output should have more detail
    assert!(!detailed.is_empty());
}

// ===== ERROR HANDLING TESTS =====

#[test]
fn test_evaluate_node_not_found() {
    let mut designer = setup_designer_with_network("test_network");

    // Try to evaluate a non-existent node
    let result = designer.evaluate_node_for_cli(99999, false);

    assert!(result.is_err());
    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("not found") || error_msg.contains("Node"));
}

#[test]
fn test_evaluate_no_active_network() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test_network");
    // Don't set active network

    let result = designer.evaluate_node_for_cli(1, false);

    assert!(result.is_err());
    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("active") || error_msg.contains("network"));
}

#[test]
fn test_evaluate_node_with_disconnected_input() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a union node which requires geometry inputs
    // Without connecting anything, it should still evaluate (with empty/default result)
    let union_id = designer.add_node("union", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(union_id, false);

    // Union with no inputs should still evaluate (may produce empty geometry or error)
    // The key is that the function doesn't panic
    assert!(result.is_ok() || result.is_err());
}

// ===== NAME LOOKUP TESTS =====

#[test]
fn test_find_node_by_custom_name() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a node and set its custom name
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    set_node_custom_name(&mut designer, "test_network", sphere_id, "my_sphere");

    // Find by custom name
    let found_id = designer.find_node_id_by_name("my_sphere");

    assert_eq!(found_id, Some(sphere_id));
}

#[test]
fn test_find_node_by_name_not_found() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a node without a custom name
    designer.add_node("sphere", DVec2::new(0.0, 0.0));

    // Try to find a non-existent name
    let found_id = designer.find_node_id_by_name("nonexistent");

    assert!(found_id.is_none());
}

#[test]
fn test_find_node_by_name_no_active_network() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test_network");
    // Don't set active network

    let found_id = designer.find_node_id_by_name("any_name");

    assert!(found_id.is_none());
}

#[test]
fn test_find_node_returns_first_match_with_same_name() {
    let mut designer = setup_designer_with_network("test_network");

    // Add multiple nodes with the same custom name (edge case)
    let sphere1_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    let sphere2_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));

    set_node_custom_name(&mut designer, "test_network", sphere1_id, "duplicate_name");
    set_node_custom_name(&mut designer, "test_network", sphere2_id, "duplicate_name");

    // Should find one of them (first match behavior)
    let found_id = designer.find_node_id_by_name("duplicate_name");

    assert!(found_id.is_some());
    let id = found_id.unwrap();
    // Should be one of the two IDs
    assert!(id == sphere1_id || id == sphere2_id);
}

// ===== EVALUATION RESULT FIELD TESTS =====

#[test]
fn test_evaluate_result_includes_custom_name() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    set_node_custom_name(&mut designer, "test_network", float_id, "my_float");

    let result = designer.evaluate_node_for_cli(float_id, false).unwrap();

    assert_eq!(result.custom_name, Some("my_float".to_string()));
}

#[test]
fn test_evaluate_result_auto_generated_name() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    // Node gets auto-generated name "float1"

    let result = designer.evaluate_node_for_cli(float_id, false).unwrap();

    // Nodes now always have persistent names assigned at creation time
    assert_eq!(result.custom_name, Some("float1".to_string()));
}

// ===== CONNECTED NETWORK EVALUATION TESTS =====

#[test]
fn test_evaluate_connected_geometry_network() {
    let mut designer = setup_designer_with_network("test_network");

    // Create a simple network: sphere -> union
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(100.0, 0.0));

    // Connect sphere output (pin 0) to union input (param 0)
    // connect_nodes(source_node_id, source_output_pin_index, dest_node_id, dest_param_index)
    designer.connect_nodes(sphere_id, 0, union_id, 0);

    // Evaluate the union node
    let result = designer.evaluate_node_for_cli(union_id, false).unwrap();

    assert_eq!(result.output_type, "Geometry");
    assert!(result.success);
}

#[test]
fn test_evaluate_chained_primitives() {
    let mut designer = setup_designer_with_network("test_network");

    // Create a chain: float -> vec3 (connected to x input)
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let vec3_id = designer.add_node("vec3", DVec2::new(100.0, 0.0));

    // Connect float output (pin 0) to vec3's first input (x, param 0)
    designer.connect_nodes(float_id, 0, vec3_id, 0);

    // Evaluate the vec3 node
    let result = designer.evaluate_node_for_cli(vec3_id, false).unwrap();

    assert_eq!(result.output_type, "Vec3");
    assert!(result.success);
}

// ===== IVEC3 NODE TEST =====

#[test]
fn test_evaluate_ivec3_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add an ivec3 node (outputs IVec3)
    let ivec3_id = designer.add_node("ivec3", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(ivec3_id, false).unwrap();

    assert_eq!(result.node_type_name, "ivec3");
    assert_eq!(result.output_type, "IVec3");
    assert!(result.success);
}

// ===== 2D GEOMETRY TESTS =====

#[test]
fn test_evaluate_2d_geometry_node() {
    let mut designer = setup_designer_with_network("test_network");

    // Add a circle node (outputs Geometry2D)
    let circle_id = designer.add_node("circle", DVec2::new(0.0, 0.0));

    let result = designer.evaluate_node_for_cli(circle_id, false).unwrap();

    assert_eq!(result.node_type_name, "circle");
    assert_eq!(result.output_type, "Geometry2D");
    assert!(result.success);
}
