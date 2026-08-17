use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type::NodeTypeCategory;
use atomcad_structure_designer::node_type::{NodeType, OutputPinDefinition};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::sphere::SphereData;
use atomcad_structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
use atomcad_test_support::sample_path_str;
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
    let load_result =
        load_node_networks_from_file(&mut registry, file_path).expect("Failed to load CNND file");

    let network = registry
        .node_networks
        .get(&load_result.first_network_name)
        .expect("Network not found");

    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();

    let mut network_stack = Vec::new();
    network_stack.push(NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    });

    let mut displayed_node_outputs: Vec<DisplayedNodeOutput> = Vec::new();

    for &node_id in network.displayed_nodes.keys() {
        let result = evaluator.evaluate(&network_stack, node_id, 0, &registry, false, &mut context);
        let node = network
            .nodes
            .get(&node_id)
            .expect("Displayed node not found");
        displayed_node_outputs.push(DisplayedNodeOutput {
            node_id,
            node_type: node.node_type_name.clone(),
            output: result.to_detailed_string(),
        });
    }

    displayed_node_outputs.sort_by_key(|n| n.node_id);

    EvaluationSnapshot {
        network_name: load_result.first_network_name,
        node_count: network.nodes.len(),
        return_node_id: network.return_node_id,
        displayed_node_outputs,
    }
}

#[test]
fn test_diamond_cnnd_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("diamond.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_hexagem_cnnd_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("hexagem.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_extrude_demo_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("extrude-demo.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_mof5_motif_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("MOF5-motif.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_rutile_motif_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("rutile-motif.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_halfspace_demo_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("half-space-and-miller-index-demo.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
#[ignore = "rotation-demo.cnnd uses pre-zones `map` with function-pin closures; .cnnd migration is deferred (see doc/design_zones.md, Out of scope)"]
fn test_rotation_demo_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("rotation-demo.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_pattern_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("pattern.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_nut_bolt_evaluation() {
    let snapshot = evaluate_cnnd_file(&sample_path_str("nut-bolt.cnnd"));
    insta::assert_json_snapshot!(snapshot);
}

#[test]
fn test_sphere_node_basic() {
    let registry = NodeTypeRegistry::new();

    let output_type = NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        node_data_creator: || Box::new(atomcad_structure_designer::node_data::NoData {}),
        node_data_saver: atomcad_structure_designer::node_type::no_data_saver,
        node_data_loader: atomcad_structure_designer::node_type::no_data_loader,
        zone_input_pins: vec![],
        zone_output_pins: vec![],
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
    network_stack.push(NetworkStackElement {
        is_zone_body: false,
        node_network: &network,
        node_id: 0,
    });

    let result = evaluator.evaluate(
        &network_stack,
        sphere_node_id,
        0,
        &registry,
        false,
        &mut context,
    );

    insta::assert_snapshot!(result.to_detailed_string());
}
