use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::EvalOutput;
use rust_lib_flutter_cad::structure_designer::node_data::{NoData, NodeData};
use rust_lib_flutter_cad::structure_designer::node_dependency_analysis::compute_downstream_dependents;
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, IncomingWire, NodeNetwork, NodeRef, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget;
use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashSet;
use std::sync::Arc;

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
    ) -> EvalOutput {
        EvalOutput::single(NetworkResult::Error(
            "MockNodeData eval not implemented".to_string(),
        ))
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
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::None),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
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
    changed.insert(NodeRef::top(node_a));

    let result = compute_downstream_dependents(&network, &changed);

    assert_eq!(result.len(), 3);
    assert!(result.contains(&NodeRef::top(node_a)));
    assert!(result.contains(&NodeRef::top(node_b)));
    assert!(result.contains(&NodeRef::top(node_c)));
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
    changed.insert(NodeRef::top(node_a));

    let result = compute_downstream_dependents(&network, &changed);

    assert_eq!(result.len(), 3);
    assert!(result.contains(&NodeRef::top(node_a)));
    assert!(result.contains(&NodeRef::top(node_b)));
    assert!(result.contains(&NodeRef::top(node_c)));
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
    changed.insert(NodeRef::top(node_a));
    changed.insert(NodeRef::top(node_b));

    let result = compute_downstream_dependents(&network, &changed);

    assert_eq!(result.len(), 3);
    assert!(result.contains(&NodeRef::top(node_a)));
    assert!(result.contains(&NodeRef::top(node_b)));
    assert!(result.contains(&NodeRef::top(node_c)));
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
    changed.insert(NodeRef::top(node_c));

    let result = compute_downstream_dependents(&network, &changed);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&NodeRef::top(node_c)));
}

#[test]
fn test_nonexistent_node() {
    // Test with a node ID that doesn't exist
    let network = NodeNetwork::new(create_test_node_type("test"));

    let mut changed = HashSet::new();
    changed.insert(NodeRef::top(999)); // Non-existent node

    let result = compute_downstream_dependents(&network, &changed);

    assert_eq!(result.len(), 0);
}

// ============================================================================
// Zone-aware dependency tests
//
// These tests cover the dirty-propagation bug introduced when zones replaced
// the old function-pin / closure machinery for HOF bodies. Three cases:
//
//   1. Capture wire: top-level int captured into a map's body. Editing the
//      int must propagate to the map (downstream consumer of the int).
//   2. Body-internal change: an expr inside a map's body changes. The map's
//      output depends on every node in its body, so the map must be marked
//      dirty too (synthetic body-node → enclosing-HOF edge).
//   3. Nested-body change: an expr inside an inner HOF whose body lives inside
//      an outer HOF's body. The synthetic edge must lift dirtiness through
//      both HOFs.
//
// Test (1) is expressible with the current `HashSet<u64>` signature — int is
// a top-level node, so its id is a valid key. Tests (2) and (3) will be added
// once `compute_downstream_dependents` becomes scope-aware (NodeRef keys),
// because they require seeding the dirty set with a body-internal id.
// ============================================================================

/// Build a fake "map" node with a single-node body that captures `source_id`
/// from the parent network, and a body-return wire. The body node returned is
/// also the only node in the body.
///
/// All nodes use `MockNodeData` and a "test" node type, so the structural
/// shape — `arguments`, `zone`, `zone_output_arguments`, `IncomingWire` — is
/// what's exercised, not any per-node-type logic. This is the same trick the
/// existing tests in this file use: build the graph manually and read out the
/// dependency walk.
fn build_capture_into_map(parent: &mut NodeNetwork, source_id: u64) -> (u64, u64) {
    // The "map" node — a top-level node owning a zone body.
    let map_id = parent.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));

    // Body containing a single "expr" node that captures `source_id`.
    let mut body = NodeNetwork::new_empty();
    let expr_id = body.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    body.nodes.get_mut(&expr_id).unwrap().arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: source_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1, // source lives one frame up
        });

    // Install the body and the body-return wire onto the map node.
    let map_node = parent.nodes.get_mut(&map_id).unwrap();
    map_node.zone = Some(Arc::new(body));
    map_node.zone_output_arguments.push(Argument::new());
    map_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: expr_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0, // source is local to the body
        });

    (map_id, expr_id)
}

#[test]
fn test_capture_wire_propagates_to_hof() {
    // Reproduces the user-reported bug: an int captured by a map body. Editing
    // the int must invalidate the map so downstream consumers (e.g. collect)
    // re-evaluate. Today `build_reverse_dependency_map` walks only the
    // top-level network's `arguments` and never descends into `node.zone`,
    // so the capture wire is invisible and the map isn't picked up.
    let mut network = NodeNetwork::new(create_test_node_type("test"));

    let int_id = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let (map_id, _expr_id) = build_capture_into_map(&mut network, int_id);

    let mut changed = HashSet::new();
    changed.insert(NodeRef::top(int_id));

    let result = compute_downstream_dependents(&network, &changed);

    assert!(
        result.contains(&NodeRef::top(int_id)),
        "the changed node itself should always be in result"
    );
    assert!(
        result.contains(&NodeRef::top(map_id)),
        "map captures int via its body — must propagate, got {:?}",
        result
    );
}

#[test]
fn test_capture_wire_propagates_transitively() {
    // int → map(captures int) → downstream("collect"). Both the map *and* the
    // downstream consumer of the map need to be in the result set. This is the
    // full user-reported scenario.
    let mut network = NodeNetwork::new(create_test_node_type("test"));

    let int_id = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let (map_id, _expr_id) = build_capture_into_map(&mut network, int_id);

    // collect node consuming the map's output.
    let collect_id = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    network.connect_nodes(map_id, 0, collect_id, 0, false);

    let mut changed = HashSet::new();
    changed.insert(NodeRef::top(int_id));

    let result = compute_downstream_dependents(&network, &changed);

    assert!(result.contains(&NodeRef::top(int_id)));
    assert!(
        result.contains(&NodeRef::top(map_id)),
        "map captures int — must propagate"
    );
    assert!(
        result.contains(&NodeRef::top(collect_id)),
        "collect consumes map — must propagate transitively"
    );
}

#[test]
fn test_body_internal_change_propagates_to_hof() {
    // Editing a node *inside* a map's body must invalidate the map. The
    // synthetic body-node → enclosing-HOF edge in
    // `build_scope_reverse_dependency_map` is what makes this work even when
    // the edit doesn't sit on the body-return path.
    let mut network = NodeNetwork::new(create_test_node_type("test"));

    let int_id = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let (map_id, expr_id) = build_capture_into_map(&mut network, int_id);

    let expr_ref = NodeRef::scoped(&[map_id], expr_id);
    let map_ref = NodeRef::top(map_id);

    let mut changed = HashSet::new();
    changed.insert(expr_ref.clone());

    let result = compute_downstream_dependents(&network, &changed);

    assert!(
        result.contains(&expr_ref),
        "the changed body node should be in result"
    );
    assert!(
        result.contains(&map_ref),
        "body-internal edit must lift to the enclosing HOF, got {:?}",
        result
    );
}

#[test]
fn test_body_internal_change_propagates_transitively() {
    // Body-internal edit must reach not just the HOF but the HOF's downstream
    // consumers (e.g. collect).
    let mut network = NodeNetwork::new(create_test_node_type("test"));

    let int_id = network.add_node("test", DVec2::ZERO, 0, Box::new(MockNodeData));
    let (map_id, expr_id) = build_capture_into_map(&mut network, int_id);

    let collect_id = network.add_node("test", DVec2::ZERO, 1, Box::new(MockNodeData));
    network.connect_nodes(map_id, 0, collect_id, 0, false);

    let mut changed = HashSet::new();
    changed.insert(NodeRef::scoped(&[map_id], expr_id));

    let result = compute_downstream_dependents(&network, &changed);

    assert!(result.contains(&NodeRef::top(map_id)));
    assert!(
        result.contains(&NodeRef::top(collect_id)),
        "downstream consumer of HOF must be dirty when body edits"
    );
}
