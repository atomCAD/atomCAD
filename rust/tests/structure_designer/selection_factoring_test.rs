//! Tests for selection factoring functionality.

use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::{NoData, NodeData};
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::selection_factoring::{
    analyze_selection_for_factoring, create_subnetwork_from_selection,
    replace_selection_with_custom_node,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashSet;

// Mock NodeData for testing
struct MockNodeData;

impl NodeData for MockNodeData {
    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(MockNodeData)
    }

    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
        NetworkResult::Error("MockNodeData eval not implemented".to_string())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

// Helper function to create a test NodeType
fn create_test_node_type(name: &str) -> NodeType {
    NodeType {
        name: name.to_string(),
        description: "Test node type".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![],
        output_type: DataType::Geometry,
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
    }
}

// Helper function to create a test NodeType with parameters
#[allow(dead_code)]
fn create_test_node_type_with_params(name: &str, num_params: usize) -> NodeType {
    use rust_lib_flutter_cad::structure_designer::node_type::Parameter;

    let mut parameters = Vec::new();
    for i in 0..num_params {
        parameters.push(Parameter {
            id: Some(i as u64 + 1),
            name: format!("param{}", i),
            data_type: DataType::Geometry,
        });
    }

    NodeType {
        name: name.to_string(),
        description: "Test node type with params".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters,
        output_type: DataType::Geometry,
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
    }
}

fn create_test_registry() -> NodeTypeRegistry {
    // The registry already has built-in types including "parameter"
    NodeTypeRegistry::new()
}

// ==================== Selection Analysis Tests ====================

#[test]
fn test_empty_selection_invalid() {
    let network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(!analysis.is_valid);
    assert_eq!(
        analysis.invalid_reason,
        Some("Select at least 1 node".to_string())
    );
}

#[test]
fn test_single_node_selection_valid() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(analysis.is_valid);
    assert!(analysis.invalid_reason.is_none());
    assert_eq!(analysis.selected_ids.len(), 1);
    assert!(analysis.selected_ids.contains(&node_a));
}

#[test]
fn test_selection_with_parameter_node_invalid() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Add a parameter node
    let param_node = network.add_node("parameter", DVec2::new(0.0, 0.0), 1, Box::new(MockNodeData));
    network.select_node(param_node);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(!analysis.is_valid);
    assert_eq!(
        analysis.invalid_reason,
        Some("Selection contains Parameter nodes".to_string())
    );
}

#[test]
fn test_selection_with_multiple_outputs_invalid() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B, A (selected) -> C
    // This means A has output to two different outside nodes
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));
    let node_c = network.add_node("union", DVec2::new(100.0, 100.0), 1, Box::new(MockNodeData));

    // Connect A -> B and A -> C
    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.connect_nodes(node_a, 0, node_c, 0, false);

    // Select only A
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(!analysis.is_valid);
    assert_eq!(
        analysis.invalid_reason,
        Some("Selection has multiple output wires".to_string())
    );
}

#[test]
fn test_selection_with_single_output_valid() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B (outside)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(analysis.is_valid);
    assert!(analysis.external_output.is_some());
    let output = analysis.external_output.as_ref().unwrap();
    assert_eq!(output.source_node_id, node_a);
    assert_eq!(output.destination_node_id, node_b);
}

#[test]
fn test_selection_with_external_inputs() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (outside) -> B (selected)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_b);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(analysis.is_valid);
    assert_eq!(analysis.external_inputs.len(), 1);
    assert_eq!(analysis.external_inputs[0].source_node_id, node_a);
    assert_eq!(analysis.external_inputs[0].destination_node_id, node_b);
}

#[test]
fn test_selection_no_external_output_valid() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Single isolated node - no outputs
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(analysis.is_valid);
    assert!(analysis.external_output.is_none());
}

#[test]
fn test_multiple_nodes_selection_with_internal_connections() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B (selected) -> C (outside)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));
    let node_c = network.add_node("union", DVec2::new(200.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.connect_nodes(node_b, 0, node_c, 0, false);

    // Select A and B
    network.select_nodes(vec![node_a, node_b]);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(analysis.is_valid);
    assert_eq!(analysis.selected_ids.len(), 2);
    assert!(analysis.selected_ids.contains(&node_a));
    assert!(analysis.selected_ids.contains(&node_b));
    // External output is from B to C
    assert!(analysis.external_output.is_some());
    assert_eq!(
        analysis.external_output.as_ref().unwrap().source_node_id,
        node_b
    );
    // No external inputs (A has no external dependencies)
    assert_eq!(analysis.external_inputs.len(), 0);
}

#[test]
fn test_bounding_box_calculation() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node(
        "cuboid",
        DVec2::new(200.0, 100.0),
        0,
        Box::new(MockNodeData),
    );

    network.select_nodes(vec![node_a, node_b]);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    assert!(analysis.is_valid);
    // Bounding box should be from (0, 0) to (200, 100)
    assert_eq!(analysis.bounding_box.0, DVec2::new(0.0, 0.0));
    assert_eq!(analysis.bounding_box.1, DVec2::new(200.0, 100.0));
}

// ==================== Subnetwork Creation Tests ====================

#[test]
fn test_create_subnetwork_basic() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create a single node selection
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);

    let subnetwork = create_subnetwork_from_selection(
        &network,
        &analysis,
        "my_subnetwork",
        &[], // No external inputs, so no params
        &registry,
    );

    assert_eq!(subnetwork.node_type.name, "my_subnetwork");
    assert_eq!(subnetwork.nodes.len(), 1);
    assert_eq!(subnetwork.node_type.parameters.len(), 0);
}

#[test]
fn test_create_subnetwork_with_parameters() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (outside) -> B (selected)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_b);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);
    assert_eq!(analysis.external_inputs.len(), 1);

    let subnetwork = create_subnetwork_from_selection(
        &network,
        &analysis,
        "my_subnetwork",
        &["input_geo".to_string()],
        &registry,
    );

    assert_eq!(subnetwork.node_type.parameters.len(), 1);
    assert_eq!(subnetwork.node_type.parameters[0].name, "input_geo");

    // Should have parameter node + the original node
    assert_eq!(subnetwork.nodes.len(), 2);

    // Check that there's a parameter node
    let param_node = subnetwork
        .nodes
        .values()
        .find(|n| n.node_type_name == "parameter");
    assert!(param_node.is_some());
}

#[test]
fn test_create_subnetwork_with_return_node() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B (outside)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);
    assert!(analysis.external_output.is_some());

    let subnetwork =
        create_subnetwork_from_selection(&network, &analysis, "my_subnetwork", &[], &registry);

    // Should have return node set
    assert!(subnetwork.return_node_id.is_some());
}

#[test]
fn test_create_subnetwork_preserves_internal_connections() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B (selected)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_nodes(vec![node_a, node_b]);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);

    let subnetwork =
        create_subnetwork_from_selection(&network, &analysis, "my_subnetwork", &[], &registry);

    assert_eq!(subnetwork.nodes.len(), 2);

    // Find the union node (node B) and verify it has a connection
    let union_node = subnetwork
        .nodes
        .values()
        .find(|n| n.node_type_name == "union")
        .unwrap();

    assert!(!union_node.arguments[0].is_empty());
}

// ==================== Replace Selection Tests ====================

#[test]
fn test_replace_selection_basic() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create single node
    let node_a = network.add_node("cuboid", DVec2::new(50.0, 50.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);

    // Replace selection
    let new_node_id =
        replace_selection_with_custom_node(&mut network, &analysis, "my_subnetwork", 0);

    // Old node should be gone
    assert!(!network.nodes.contains_key(&node_a));

    // New node should exist
    assert!(network.nodes.contains_key(&new_node_id));

    // New node should be at center of selection
    let new_node = network.nodes.get(&new_node_id).unwrap();
    assert_eq!(new_node.position, DVec2::new(50.0, 50.0));
    assert_eq!(new_node.node_type_name, "my_subnetwork");
}

#[test]
fn test_replace_selection_wires_up_inputs() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (outside) -> B (selected)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_b);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);

    let new_node_id = replace_selection_with_custom_node(
        &mut network,
        &analysis,
        "my_subnetwork",
        1, // 1 parameter
    );

    // New node should be wired to A
    let new_node = network.nodes.get(&new_node_id).unwrap();
    assert!(!new_node.arguments[0].is_empty());
    assert!(
        new_node.arguments[0]
            .argument_output_pins
            .contains_key(&node_a)
    );
}

#[test]
fn test_replace_selection_wires_up_output() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B (outside)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);

    let new_node_id =
        replace_selection_with_custom_node(&mut network, &analysis, "my_subnetwork", 0);

    // B should now be wired to new node
    let node_b_obj = network.nodes.get(&node_b).unwrap();
    assert!(!node_b_obj.arguments[0].is_empty());
    assert!(
        node_b_obj.arguments[0]
            .argument_output_pins
            .contains_key(&new_node_id)
    );
}

#[test]
fn test_replace_selection_removes_all_selected_nodes() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    // Create: A (selected) -> B (selected)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_nodes(vec![node_a, node_b]);

    let analysis = analyze_selection_for_factoring(&network, &registry);
    assert!(analysis.is_valid);

    replace_selection_with_custom_node(&mut network, &analysis, "my_subnetwork", 0);

    // Both old nodes should be gone
    assert!(!network.nodes.contains_key(&node_a));
    assert!(!network.nodes.contains_key(&node_b));

    // Only one node left (the new custom node)
    assert_eq!(network.nodes.len(), 1);
}

#[test]
fn test_replace_selection_selects_new_node() {
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    let registry = create_test_registry();

    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    let analysis = analyze_selection_for_factoring(&network, &registry);

    let new_node_id =
        replace_selection_with_custom_node(&mut network, &analysis, "my_subnetwork", 0);

    // New node should be selected
    assert!(network.is_node_selected(new_node_id));
}

// ==================== Integration Tests ====================

#[test]
fn test_full_factoring_workflow() {
    // This test simulates the full workflow from selection to factoring
    let mut sd = StructureDesigner::new();

    // Create a network
    sd.node_type_registry
        .add_node_network(NodeNetwork::new(create_test_node_type("main")));
    sd.active_node_network_name = Some("main".to_string());

    // Add nodes to the network
    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();

    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));
    let node_c = network.add_node("union", DVec2::new(200.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.connect_nodes(node_b, 0, node_c, 0, false);

    // Select A and B for factoring
    network.select_nodes(vec![node_a, node_b]);

    // Factor into subnetwork
    let result = sd.factor_selection_into_subnetwork("my_custom_node", vec![]);

    assert!(result.is_ok());
    let new_node_id = result.unwrap();

    // Verify the subnetwork was created
    assert!(
        sd.node_type_registry
            .node_networks
            .contains_key("my_custom_node")
    );

    // Verify the original network now has the custom node
    let main_network = sd.node_type_registry.node_networks.get("main").unwrap();
    assert!(main_network.nodes.contains_key(&new_node_id));
    assert!(!main_network.nodes.contains_key(&node_a));
    assert!(!main_network.nodes.contains_key(&node_b));

    // Node C should still exist and be wired to the new custom node
    assert!(main_network.nodes.contains_key(&node_c));
    let node_c_obj = main_network.nodes.get(&node_c).unwrap();
    assert!(
        node_c_obj.arguments[0]
            .argument_output_pins
            .contains_key(&new_node_id)
    );
}

#[test]
fn test_factoring_duplicate_name_fails() {
    let mut sd = StructureDesigner::new();

    // Create a network
    sd.node_type_registry
        .add_node_network(NodeNetwork::new(create_test_node_type("main")));
    sd.active_node_network_name = Some("main".to_string());

    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    // Try to create a subnetwork with a name that already exists (built-in)
    let result = sd.factor_selection_into_subnetwork("cuboid", vec![]);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("already exists"));
}

#[test]
fn test_factoring_param_count_mismatch_fails() {
    let mut sd = StructureDesigner::new();

    sd.node_type_registry
        .add_node_network(NodeNetwork::new(create_test_node_type("main")));
    sd.active_node_network_name = Some("main".to_string());

    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();

    // Create: A (outside) -> B (selected)
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    let node_b = network.add_node("union", DVec2::new(100.0, 0.0), 1, Box::new(MockNodeData));

    network.connect_nodes(node_a, 0, node_b, 0, false);
    network.select_node(node_b);

    // Analysis shows 1 external input, but we provide 0 param names
    let result = sd.factor_selection_into_subnetwork(
        "my_custom_node",
        vec![], // Wrong count!
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("mismatch"));
}

#[test]
fn test_get_factor_selection_info() {
    let mut sd = StructureDesigner::new();

    sd.node_type_registry
        .add_node_network(NodeNetwork::new(create_test_node_type("main")));
    sd.active_node_network_name = Some("main".to_string());

    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    let node_a = network.add_node("cuboid", DVec2::new(0.0, 0.0), 0, Box::new(MockNodeData));
    network.select_node(node_a);

    let info = sd.get_factor_selection_info();

    assert!(info.can_factor);
    assert!(info.invalid_reason.is_none());
    assert!(!info.suggested_name.is_empty());
}

#[test]
fn test_get_factor_selection_info_invalid_selection() {
    let mut sd = StructureDesigner::new();

    sd.node_type_registry
        .add_node_network(NodeNetwork::new(create_test_node_type("main")));
    sd.active_node_network_name = Some("main".to_string());

    // Don't select anything

    let info = sd.get_factor_selection_info();

    assert!(!info.can_factor);
    assert!(info.invalid_reason.is_some());
}
