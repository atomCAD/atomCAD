use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{NetworkEvaluator, NetworkEvaluationContext, NetworkStackElement};
use rust_lib_flutter_cad::structure_designer::nodes::sphere::SphereData;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use glam::f64::DVec2;
use glam::i32::IVec3;
use serde::Serialize;

#[derive(Serialize)]
struct EvaluationSnapshot {
    network_name: String,
    node_count: usize,
    return_node_id: Option<u64>,
    displayed_node_outputs: Vec<DisplayedNodeOutput>,
}

#[derive(Serialize)]
struct DisplayedNodeOutput {
    node_id: u64,
    node_type: String,
    output: String,
}

fn evaluate_cnnd_file(file_path: &str) -> EvaluationSnapshot {
    let mut registry = NodeTypeRegistry::new();
    let first_network_name = load_node_networks_from_file(&mut registry, file_path)
        .expect("Failed to load CNND file");
    
    let network = registry.node_networks.get(&first_network_name)
        .expect("Network not found");
    
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    
    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });
    
    let mut displayed_node_outputs: Vec<DisplayedNodeOutput> = Vec::new();
    
    for &node_id in network.displayed_node_ids.keys() {
        let result = evaluator.evaluate(&network_stack, node_id, 0, &registry, false, &mut context);
        let node = network.nodes.get(&node_id).expect("Displayed node not found");
        displayed_node_outputs.push(DisplayedNodeOutput {
            node_id,
            node_type: node.node_type_name.clone(),
            output: result.to_detailed_string(),
        });
    }
    
    displayed_node_outputs.sort_by_key(|n| n.node_id);
    
    EvaluationSnapshot {
        network_name: first_network_name,
        node_count: network.nodes.len(),
        return_node_id: network.return_node_id,
        displayed_node_outputs,
    }
}

#[test]
fn test_diamond_cnnd_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/diamond.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_hexagem_cnnd_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/hexagem.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_extrude_demo_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/extrude-demo.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_mof5_motif_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/MOF5-motif.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_rutile_motif_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/rutile-motif.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_halfspace_demo_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/half-space-and-miller-index-demo.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_rotation_demo_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/rotation-demo.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_pattern_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/pattern.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_nut_bolt_evaluation() {
    let snapshot = evaluate_cnnd_file("../samples/nut-bolt.cnnd");
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_sphere_node_basic() {
    let registry = NodeTypeRegistry::new();
    
    let output_type = NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_type: DataType::Geometry,
        node_data_creator: || Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        public: true,
    };
    
    let mut network = NodeNetwork::new(output_type);
    
    let sphere_data = Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 2,
    });
    let sphere_node_id = network.add_node("sphere", DVec2::ZERO, 3, sphere_data);
    network.return_node_id = Some(sphere_node_id);
    network.set_node_display(sphere_node_id, true);
    
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    
    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement { node_network: &network, node_id: 0 });
    
    let result = evaluator.evaluate(&network_stack, sphere_node_id, 0, &registry, false, &mut context);
    
    insta::assert_snapshot!(result.to_detailed_string());
}
