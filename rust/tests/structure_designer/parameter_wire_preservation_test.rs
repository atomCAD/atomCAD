use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData; // Import trait for set_text_properties
use rust_lib_flutter_cad::structure_designer::nodes::expr::ExprData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
/// Tests for parameter wire preservation when parameters are renamed, added, removed, or reordered.
///
/// This follows TDD: tests are written first, then the fix is implemented.
/// - Working scenario tests (S1-S6, E1-E5) should PASS before and after the fix
/// - Broken scenario tests (S8, E7) should FAIL before fix, PASS after fix
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

fn setup_designer() -> StructureDesigner {
    StructureDesigner::new()
}

/// Helper to get a node's argument connection count at a given parameter index
fn get_wire_count(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    param_index: usize,
) -> usize {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    node.arguments
        .get(param_index)
        .map(|a| a.argument_output_pins.len())
        .unwrap_or(0)
}

/// Helper to check if a specific wire exists
fn has_wire_from(
    designer: &StructureDesigner,
    network_name: &str,
    dest_node_id: u64,
    param_index: usize,
    source_node_id: u64,
) -> bool {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = network.nodes.get(&dest_node_id).unwrap();
    node.arguments
        .get(param_index)
        .map(|a| a.argument_output_pins.contains_key(&source_node_id))
        .unwrap_or(false)
}

/// Helper to set a parameter node's name via text properties
fn set_parameter_name(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    new_name: &str,
) {
    designer.set_active_node_network_name(Some(network_name.to_string()));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
        let mut props = HashMap::new();
        props.insert(
            "param_name".to_string(),
            TextValue::String(new_name.to_string()),
        );
        props.insert(
            "data_type".to_string(),
            TextValue::DataType(param_data.data_type.clone()),
        );
        props.insert(
            "sort_order".to_string(),
            TextValue::Int(param_data.sort_order),
        );
        props.insert(
            "param_index".to_string(),
            TextValue::Int(param_data.param_index as i32),
        );
        param_data.set_text_properties(&props).unwrap();
    }
    // Trigger validation to propagate changes
    designer.validate_active_network();
}

/// Helper to set a parameter node's sort order
fn set_parameter_sort_order(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    sort_order: i32,
) {
    designer.set_active_node_network_name(Some(network_name.to_string()));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
        let mut props = HashMap::new();
        props.insert(
            "param_name".to_string(),
            TextValue::String(param_data.param_name.clone()),
        );
        props.insert(
            "data_type".to_string(),
            TextValue::DataType(param_data.data_type.clone()),
        );
        props.insert("sort_order".to_string(), TextValue::Int(sort_order));
        props.insert(
            "param_index".to_string(),
            TextValue::Int(param_data.param_index as i32),
        );
        param_data.set_text_properties(&props).unwrap();
    }
    // Trigger validation to propagate changes
    designer.validate_active_network();
}

/// Helper to set a parameter node's data type
fn set_parameter_type(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data_type: DataType,
) {
    designer.set_active_node_network_name(Some(network_name.to_string()));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
        let mut props = HashMap::new();
        props.insert(
            "param_name".to_string(),
            TextValue::String(param_data.param_name.clone()),
        );
        props.insert("data_type".to_string(), TextValue::DataType(data_type));
        props.insert(
            "sort_order".to_string(),
            TextValue::Int(param_data.sort_order),
        );
        props.insert(
            "param_index".to_string(),
            TextValue::Int(param_data.param_index as i32),
        );
        param_data.set_text_properties(&props).unwrap();
    }
    // Trigger validation to propagate changes
    designer.validate_active_network();
}

/// Helper to get the number of parameters in a network's node type
fn get_parameter_count(designer: &StructureDesigner, network_name: &str) -> usize {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    network.node_type.parameters.len()
}

/// Helper to get parameter names in order
fn get_parameter_names(designer: &StructureDesigner, network_name: &str) -> Vec<String> {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    network
        .node_type
        .parameters
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

// =============================================================================
// WORKING SUBNETWORK SCENARIOS (S1-S6) - Must pass before and after fix
// =============================================================================

/// S1: Add parameter at end preserves existing wires
#[test]
fn test_subnetwork_add_parameter_preserves_existing_wires() {
    let mut designer = setup_designer();

    // Create subnetwork "MyFilter" with one parameter "size"
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param_id, "size");

    // Add an output node to make the network usable
    let output_id = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    assert_eq!(get_parameter_count(&designer, "MyFilter"), 1);

    // Create main network that uses MyFilter
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));

    // Wire int to "size" parameter (index 0)
    designer.connect_nodes(int_id, 0, filter_id, 0);

    // Verify wire exists
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int_id),
        "Wire should exist before adding parameter"
    );

    // Add second parameter to MyFilter
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    let param2_id = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    set_parameter_name(&mut designer, "MyFilter", param2_id, "radius");
    set_parameter_sort_order(&mut designer, "MyFilter", param2_id, 1);

    assert_eq!(get_parameter_count(&designer, "MyFilter"), 2);

    // Verify original wire is preserved at index 0
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int_id),
        "Original wire should be preserved after adding parameter"
    );

    // Verify new parameter has no wire
    assert_eq!(
        get_wire_count(&designer, "main", filter_id, 1),
        0,
        "New parameter should have no wire"
    );
}

/// S2: Add parameter in middle repositions wires correctly
#[test]
fn test_subnetwork_add_parameter_in_middle_repositions_wires() {
    let mut designer = setup_designer();

    // Create subnetwork with params at sort_order 0 and 2
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));

    let param1_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param1_id, "first");
    set_parameter_sort_order(&mut designer, "MyFilter", param1_id, 0);

    let param2_id = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    set_parameter_name(&mut designer, "MyFilter", param2_id, "last");
    set_parameter_sort_order(&mut designer, "MyFilter", param2_id, 2);

    let output_id = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    assert_eq!(
        get_parameter_names(&designer, "MyFilter"),
        vec!["first", "last"]
    );

    // Create main network and wire both parameters
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let int1_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int2_id = designer.add_node("int", DVec2::new(0.0, 50.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));

    // Wire to first (index 0) and last (index 1)
    designer.connect_nodes(int1_id, 0, filter_id, 0); // first
    designer.connect_nodes(int2_id, 0, filter_id, 1); // last

    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int1_id),
        "first wire should exist"
    );
    assert!(
        has_wire_from(&designer, "main", filter_id, 1, int2_id),
        "last wire should exist"
    );

    // Add parameter in the middle (sort_order 1)
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    let param_middle_id = designer.add_node("parameter", DVec2::new(0.0, 50.0));
    set_parameter_name(&mut designer, "MyFilter", param_middle_id, "middle");
    set_parameter_sort_order(&mut designer, "MyFilter", param_middle_id, 1);

    assert_eq!(
        get_parameter_names(&designer, "MyFilter"),
        vec!["first", "middle", "last"]
    );

    // Verify wires follow their parameters
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int1_id),
        "first wire should still be at index 0"
    );
    assert!(
        has_wire_from(&designer, "main", filter_id, 2, int2_id),
        "last wire should now be at index 2"
    );
    assert_eq!(
        get_wire_count(&designer, "main", filter_id, 1),
        0,
        "middle (new) should have no wire"
    );
}

/// S3: Remove parameter disconnects wire but preserves others
#[test]
fn test_subnetwork_remove_parameter_disconnects_wire() {
    let mut designer = setup_designer();

    // Create subnetwork with two parameters
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));

    let param1_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param1_id, "size");
    set_parameter_sort_order(&mut designer, "MyFilter", param1_id, 0);

    let param2_id = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    set_parameter_name(&mut designer, "MyFilter", param2_id, "radius");
    set_parameter_sort_order(&mut designer, "MyFilter", param2_id, 1);

    let output_id = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    assert_eq!(get_parameter_count(&designer, "MyFilter"), 2);

    // Create main network and wire both parameters
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let int1_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int2_id = designer.add_node("int", DVec2::new(0.0, 50.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));

    designer.connect_nodes(int1_id, 0, filter_id, 0); // size
    designer.connect_nodes(int2_id, 0, filter_id, 1); // radius

    assert!(has_wire_from(&designer, "main", filter_id, 0, int1_id));
    assert!(has_wire_from(&designer, "main", filter_id, 1, int2_id));

    // Delete the radius parameter
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    designer.select_node(param2_id);
    designer.delete_selected();
    designer.validate_active_network();

    assert_eq!(get_parameter_count(&designer, "MyFilter"), 1);
    assert_eq!(get_parameter_names(&designer, "MyFilter"), vec!["size"]);

    // Verify size wire is preserved
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int1_id),
        "size wire should be preserved"
    );
}

/// S4: Reorder parameters via sort_order - wires follow their parameter names
#[test]
fn test_subnetwork_reorder_parameters_wires_follow_names() {
    let mut designer = setup_designer();

    // Create subnetwork with two parameters (size at 0, radius at 1)
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));

    let param1_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param1_id, "size");
    set_parameter_sort_order(&mut designer, "MyFilter", param1_id, 0);

    let param2_id = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    set_parameter_name(&mut designer, "MyFilter", param2_id, "radius");
    set_parameter_sort_order(&mut designer, "MyFilter", param2_id, 1);

    let output_id = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    assert_eq!(
        get_parameter_names(&designer, "MyFilter"),
        vec!["size", "radius"]
    );

    // Create main network and wire both parameters
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let int_size = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int_radius = designer.add_node("int", DVec2::new(0.0, 50.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));

    designer.connect_nodes(int_size, 0, filter_id, 0); // size at index 0
    designer.connect_nodes(int_radius, 0, filter_id, 1); // radius at index 1

    // Verify initial wiring
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int_size),
        "size wire at index 0"
    );
    assert!(
        has_wire_from(&designer, "main", filter_id, 1, int_radius),
        "radius wire at index 1"
    );

    // Swap sort order: radius becomes -1 (first), size becomes 1 (second)
    set_parameter_sort_order(&mut designer, "MyFilter", param2_id, -1); // radius first
    set_parameter_sort_order(&mut designer, "MyFilter", param1_id, 1); // size second

    assert_eq!(
        get_parameter_names(&designer, "MyFilter"),
        vec!["radius", "size"]
    );

    // After reorder: radius at index 0, size at index 1
    // Wires should follow their parameter names to new positions
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int_radius),
        "radius wire should be at index 0 after reorder"
    );
    assert!(
        has_wire_from(&designer, "main", filter_id, 1, int_size),
        "size wire should be at index 1 after reorder"
    );
}

/// S5: Change parameter type preserves wire (validation may fail but wire exists)
#[test]
fn test_subnetwork_change_parameter_type_preserves_wire() {
    let mut designer = setup_designer();

    // Create subnetwork with Float parameter
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));

    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param_id, "value");
    set_parameter_type(&mut designer, "MyFilter", param_id, DataType::Float);

    let output_id = designer.add_node("float", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    // Create main network and wire a Float source
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));

    designer.connect_nodes(float_id, 0, filter_id, 0);

    assert!(
        has_wire_from(&designer, "main", filter_id, 0, float_id),
        "Float wire should exist"
    );

    // Change parameter type to Int
    set_parameter_type(&mut designer, "MyFilter", param_id, DataType::Int);

    // Wire should still exist (type mismatch is handled by validation, not wire preservation)
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, float_id),
        "Wire should be preserved after type change (validation handles type mismatch)"
    );
}

/// S6: Multiple parent networks all get updated correctly
#[test]
fn test_subnetwork_multiple_parents_all_repaired() {
    let mut designer = setup_designer();

    // Create subnetwork with one parameter
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));

    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param_id, "size");

    let output_id = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    // Create two parent networks that use MyFilter
    designer.add_node_network("parentA");
    designer.set_active_node_network_name(Some("parentA".to_string()));
    let int_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    let filter_a = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));
    designer.connect_nodes(int_a, 0, filter_a, 0);

    designer.add_node_network("parentB");
    designer.set_active_node_network_name(Some("parentB".to_string()));
    let int_b = designer.add_node("int", DVec2::new(0.0, 0.0));
    let filter_b = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));
    designer.connect_nodes(int_b, 0, filter_b, 0);

    // Verify both have wires
    assert!(has_wire_from(&designer, "parentA", filter_a, 0, int_a));
    assert!(has_wire_from(&designer, "parentB", filter_b, 0, int_b));

    // Add a second parameter to MyFilter
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    let param2_id = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    set_parameter_name(&mut designer, "MyFilter", param2_id, "radius");
    set_parameter_sort_order(&mut designer, "MyFilter", param2_id, 1);

    // Both parent networks should have:
    // - Original wire preserved at index 0
    // - New empty argument at index 1
    assert!(
        has_wire_from(&designer, "parentA", filter_a, 0, int_a),
        "parentA wire should be preserved"
    );
    assert_eq!(
        get_wire_count(&designer, "parentA", filter_a, 1),
        0,
        "parentA new param should have no wire"
    );

    assert!(
        has_wire_from(&designer, "parentB", filter_b, 0, int_b),
        "parentB wire should be preserved"
    );
    assert_eq!(
        get_wire_count(&designer, "parentB", filter_b, 1),
        0,
        "parentB new param should have no wire"
    );
}

// =============================================================================
// WORKING EXPR NODE SCENARIOS (E1-E5) - Must pass before and after fix
// =============================================================================

/// Helper to update expr node's parameters and expression
fn update_expr_parameters(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    params: Vec<(&str, DataType)>,
    expression: &str,
) {
    designer.set_active_node_network_name(Some(network_name.to_string()));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();

    if let Some(expr_data) = node.data.as_any_mut().downcast_mut::<ExprData>() {
        // Build parameters array
        let params_arr: Vec<TextValue> = params
            .iter()
            .map(|(name, dt)| {
                TextValue::Object(vec![
                    ("name".to_string(), TextValue::String(name.to_string())),
                    ("data_type".to_string(), TextValue::DataType(dt.clone())),
                ])
            })
            .collect();

        let mut props = HashMap::new();
        props.insert(
            "expression".to_string(),
            TextValue::String(expression.to_string()),
        );
        props.insert("parameters".to_string(), TextValue::Array(params_arr));

        expr_data.set_text_properties(&props).unwrap();

        // Recalculate custom node type
        let base_node_type = designer
            .node_type_registry
            .built_in_node_types
            .get("expr")
            .unwrap()
            .clone();
        if let Some(custom_type) = expr_data.calculate_custom_node_type(&base_node_type) {
            node.set_custom_node_type(Some(custom_type), true);
        }
    }

    designer.validate_active_network();
}

/// E1: Add parameter to expr preserves existing wire
#[test]
fn test_expr_add_parameter_preserves_existing_wires() {
    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Create int source and expr node
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    // Default expr has parameter "x", wire to it
    designer.connect_nodes(int_id, 0, expr_id, 0);

    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_id),
        "Initial wire should exist"
    );

    // Add a second parameter "y"
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Int), ("y", DataType::Int)],
        "x + y",
    );

    // Original wire should be preserved
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_id),
        "Wire to x should be preserved after adding y"
    );
    assert_eq!(
        get_wire_count(&designer, "main", expr_id, 1),
        0,
        "New parameter y should have no wire"
    );
}

/// E2: Remove parameter from expr disconnects wire but preserves others
#[test]
fn test_expr_remove_parameter_disconnects_wire() {
    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let int_x = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int_y = designer.add_node("int", DVec2::new(0.0, 50.0));
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    // Set up expr with two parameters
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Int), ("y", DataType::Int)],
        "x + y",
    );

    // Wire both
    designer.connect_nodes(int_x, 0, expr_id, 0); // x
    designer.connect_nodes(int_y, 0, expr_id, 1); // y

    assert!(has_wire_from(&designer, "main", expr_id, 0, int_x));
    assert!(has_wire_from(&designer, "main", expr_id, 1, int_y));

    // Remove parameter y
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Int)],
        "x * 2",
    );

    // Wire to x should be preserved
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_x),
        "Wire to x should be preserved after removing y"
    );
}

/// E3: Change expr parameter type preserves wire
#[test]
fn test_expr_change_parameter_type_preserves_wire() {
    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    designer.connect_nodes(int_id, 0, expr_id, 0);

    assert!(has_wire_from(&designer, "main", expr_id, 0, int_id));

    // Change x from Int to Float (Int can be promoted to Float)
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Float)],
        "x * 2.0",
    );

    // Wire should be preserved
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_id),
        "Wire should be preserved after type change"
    );
}

/// E4: Change only expression preserves all wires
#[test]
fn test_expr_change_expression_only_preserves_all_wires() {
    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let int_x = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int_y = designer.add_node("int", DVec2::new(0.0, 50.0));
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    // Set up expr with two parameters
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Int), ("y", DataType::Int)],
        "x + y",
    );

    designer.connect_nodes(int_x, 0, expr_id, 0);
    designer.connect_nodes(int_y, 0, expr_id, 1);

    // Change only expression, keep same parameters
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Int), ("y", DataType::Int)],
        "x * y + 1",
    );

    // Both wires should be preserved
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_x),
        "Wire to x should be preserved"
    );
    assert!(
        has_wire_from(&designer, "main", expr_id, 1, int_y),
        "Wire to y should be preserved"
    );
}

/// E5: Reorder expr parameters - wires follow their names
#[test]
fn test_expr_reorder_parameters_wires_follow_names() {
    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let int_x = designer.add_node("int", DVec2::new(0.0, 0.0));
    let int_y = designer.add_node("int", DVec2::new(0.0, 50.0));
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    // Initial order: x at 0, y at 1
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("x", DataType::Int), ("y", DataType::Int)],
        "x + y",
    );

    designer.connect_nodes(int_x, 0, expr_id, 0); // x
    designer.connect_nodes(int_y, 0, expr_id, 1); // y

    // Reorder to y, x
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("y", DataType::Int), ("x", DataType::Int)],
        "y + x",
    );

    // After reorder: y at 0, x at 1
    // Wires should follow their parameter names
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_y),
        "Wire to y should now be at index 0"
    );
    assert!(
        has_wire_from(&designer, "main", expr_id, 1, int_x),
        "Wire to x should now be at index 1"
    );
}

// =============================================================================
// BROKEN SCENARIOS (S8, E7) - MUST FAIL before fix, MUST PASS after fix
// =============================================================================

/// S8: Rename subnetwork parameter preserves wire
/// THIS TEST SHOULD FAIL BEFORE THE FIX IS IMPLEMENTED
#[test]
fn test_subnetwork_rename_parameter_preserves_wire() {
    let mut designer = setup_designer();

    // Create subnetwork "MyFilter" with parameter "size"
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_name(&mut designer, "MyFilter", param_id, "size");

    let output_id = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.set_return_node_id(Some(output_id));
    designer.validate_active_network();

    // Create main network that uses MyFilter
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));

    // Wire int to "size" parameter (index 0)
    designer.connect_nodes(int_id, 0, filter_id, 0);

    // Verify wire exists before rename
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int_id),
        "Wire should exist before rename"
    );

    // Rename parameter from "size" to "length"
    set_parameter_name(&mut designer, "MyFilter", param_id, "length");

    // Verify parameter was renamed
    assert_eq!(
        get_parameter_names(&designer, "MyFilter"),
        vec!["length"],
        "Parameter should be renamed to 'length'"
    );

    // THIS IS THE KEY ASSERTION - Wire should be preserved after rename
    // BEFORE FIX: This will FAIL because wire preservation uses name matching
    // AFTER FIX: This should PASS because wire preservation will use ID matching
    assert!(
        has_wire_from(&designer, "main", filter_id, 0, int_id),
        "Wire should be preserved after parameter rename (S8)"
    );
}

/// E7: Rename expr parameter preserves wire
/// THIS TEST SHOULD FAIL BEFORE THE FIX IS IMPLEMENTED
#[test]
fn test_expr_rename_parameter_preserves_wire() {
    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Create int node as wire source
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));

    // Create expr node with parameter "x"
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    // Wire int to expr's "x" parameter (index 0)
    designer.connect_nodes(int_id, 0, expr_id, 0);

    // Verify wire exists before rename
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_id),
        "Wire should exist before rename"
    );

    // Rename parameter from "x" to "input"
    // This changes the parameter name but keeps the same position
    update_expr_parameters(
        &mut designer,
        "main",
        expr_id,
        vec![("input", DataType::Int)],
        "input * 2",
    );

    // THIS IS THE KEY ASSERTION - Wire should be preserved after rename
    // BEFORE FIX: This will FAIL because wire preservation uses name matching
    // AFTER FIX: This should PASS because wire preservation will use ID matching
    assert!(
        has_wire_from(&designer, "main", expr_id, 0, int_id),
        "Wire should be preserved after expr parameter rename (E7)"
    );
}
