use rust_lib_flutter_cad::structure_designer::node_dependency_analysis::compute_downstream_dependents;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::node_data::{NodeData, NoData};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{NetworkEvaluator, NetworkStackElement, NetworkEvaluationContext};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec2;
use std::collections::HashSet;

// Mock NodeData for testing
struct MockNodeData;

impl NodeData for MockNodeData {
    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(MockNodeData)
    }
    
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }
    
    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }
    
    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &Vec<NetworkStackElement<'a>>,
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
        NetworkResult::Error("MockNodeData eval not implemented".to_string())
    }
    
    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

// Helper function to create a test NodeType
fn create_test_node_type(name: &str) -> NodeType {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    
    NodeType {
        name: name.to_string(),
        description: "Test node type".to_string(),
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![],
        output_type: DataType::None,
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
    }
}

#[test]
fn test_linear_chain() {
    // Create a network: A → B → C
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    
    let node_a = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let node_b = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    let node_c = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    
    // Connect A → B
    network.connect_nodes(node_a, 0, node_b, 0, false);
    // Connect B → C
    network.connect_nodes(node_b, 0, node_c, 0, false);
    
    // Test: changing A should affect A, B, C
    let mut changed = HashSet::new();
    changed.insert(node_a);
    
    let result = compute_downstream_dependents(&network, &changed);
    
    assert_eq!(result.len(), 3);
    assert!(result.contains(&node_a));
    assert!(result.contains(&node_b));
    assert!(result.contains(&node_c));
}

#[test]
fn test_branching() {
    // Create a network: A → B, A → C
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    
    let node_a = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let node_b = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    let node_c = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    
    // Connect A → B
    network.connect_nodes(node_a, 0, node_b, 0, false);
    // Connect A → C
    network.connect_nodes(node_a, 0, node_c, 0, false);
    
    // Test: changing A should affect A, B, C
    let mut changed = HashSet::new();
    changed.insert(node_a);
    
    let result = compute_downstream_dependents(&network, &changed);
    
    assert_eq!(result.len(), 3);
    assert!(result.contains(&node_a));
    assert!(result.contains(&node_b));
    assert!(result.contains(&node_c));
}

#[test]
fn test_multiple_changed_nodes() {
    // Create a network: A → C, B → C
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    
    let node_a = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let node_b = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let node_c = network.add_node("test", DVec2::ZERO, 2, Box::new(MockNodeData));
    
    // Connect A → C
    network.connect_nodes(node_a, 0, node_c, 0, false);
    // Connect B → C
    network.connect_nodes(node_b, 0, node_c, 1, false);
    
    // Test: changing both A and B should affect A, B, C
    let mut changed = HashSet::new();
    changed.insert(node_a);
    changed.insert(node_b);
    
    let result = compute_downstream_dependents(&network, &changed);
    
    assert_eq!(result.len(), 3);
    assert!(result.contains(&node_a));
    assert!(result.contains(&node_b));
    assert!(result.contains(&node_c));
}

#[test]
fn test_isolated_node() {
    // Create a network with isolated nodes: A → B, C (isolated)
    let mut network = NodeNetwork::new(create_test_node_type("test"));
    
    let node_a = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let node_b = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    let node_c = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    
    // Connect A → B (C is isolated)
    network.connect_nodes(node_a, 0, node_b, 0, false);
    
    // Test: changing C should only affect C
    let mut changed = HashSet::new();
    changed.insert(node_c);
    
    let result = compute_downstream_dependents(&network, &changed);
    
    assert_eq!(result.len(), 1);
    assert!(result.contains(&node_c));
}

#[test]
fn test_nonexistent_node() {
    // Test with a node ID that doesn't exist
    let network = NodeNetwork::new(create_test_node_type("test"));
    
    let mut changed = HashSet::new();
    changed.insert(999); // Non-existent node
    
    let result = compute_downstream_dependents(&network, &changed);
    
    assert_eq!(result.len(), 0);
}







