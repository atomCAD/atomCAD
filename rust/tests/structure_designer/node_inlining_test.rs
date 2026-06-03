//! Tests for the inline-custom-node building blocks (Phase 1):
//! `make_space_for_inline` and `copy_content_into`.

use glam::f64::DVec2;
use std::sync::Arc;

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::{EvalOutput, NodeData};
use rust_lib_flutter_cad::structure_designer::node_inlining::{
    copy_content_into, make_space_for_inline,
};
use rust_lib_flutter_cad::structure_designer::node_network::{NodeDisplayType, NodeNetwork};
use rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Test scaffolding
// ---------------------------------------------------------------------------

/// Minimal NodeData stub for building test networks.
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
        EvalOutput::single(NetworkResult::Error("MockNodeData eval".to_string()))
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

/// Adds a node at `position` and returns its id.
fn add(network: &mut NodeNetwork, type_name: &str, position: DVec2, num_params: usize) -> u64 {
    network.add_node(type_name, position, num_params, Box::new(MockNodeData))
}

fn set_name(network: &mut NodeNetwork, id: u64, name: &str) {
    network.nodes.get_mut(&id).unwrap().custom_name = Some(name.to_string());
}

fn pos(network: &NodeNetwork, id: u64) -> DVec2 {
    network.nodes[&id].position
}

// ---------------------------------------------------------------------------
// make_space_for_inline
// ---------------------------------------------------------------------------

#[test]
fn make_space_shifts_lower_right_quadrant_on_both_axes() {
    let mut network = NodeNetwork::new_empty();

    let anchor = DVec2::new(0.0, 0.0);
    let original_size = DVec2::new(160.0, 100.0);
    let content_size = DVec2::new(300.0, 250.0);
    // Expected delta = max(0, content - original) = (140, 150).

    let instance = add(&mut network, "union", anchor, 0);
    let right_only = add(&mut network, "union", DVec2::new(200.0, 0.0), 0); // x past, y within
    let below_only = add(&mut network, "union", DVec2::new(0.0, 200.0), 0); // y past, x within
    let lower_right = add(&mut network, "union", DVec2::new(300.0, 300.0), 0); // both past
    let overlapping = add(&mut network, "union", DVec2::new(50.0, 50.0), 0); // neither past

    let delta = make_space_for_inline(&mut network, instance, anchor, original_size, content_size);

    assert_eq!(delta, DVec2::new(140.0, 150.0));

    // Instance never moves (it's the anchor).
    assert_eq!(pos(&network, instance), anchor);
    // Right-only: x shifts, y stays.
    assert_eq!(pos(&network, right_only), DVec2::new(340.0, 0.0));
    // Below-only: y shifts, x stays.
    assert_eq!(pos(&network, below_only), DVec2::new(0.0, 350.0));
    // Lower-right: both shift.
    assert_eq!(pos(&network, lower_right), DVec2::new(440.0, 450.0));
    // Overlapping: no move.
    assert_eq!(pos(&network, overlapping), DVec2::new(50.0, 50.0));
}

#[test]
fn make_space_no_move_when_content_fits() {
    let mut network = NodeNetwork::new_empty();

    let anchor = DVec2::new(10.0, 10.0);
    let original_size = DVec2::new(160.0, 100.0);
    // Content smaller than original on both axes -> delta clamps to zero.
    let content_size = DVec2::new(120.0, 50.0);

    let instance = add(&mut network, "union", anchor, 0);
    let far = add(&mut network, "union", DVec2::new(500.0, 500.0), 0);

    let delta = make_space_for_inline(&mut network, instance, anchor, original_size, content_size);

    assert_eq!(delta, DVec2::ZERO);
    assert_eq!(pos(&network, far), DVec2::new(500.0, 500.0));
    assert_eq!(pos(&network, instance), anchor);
}

#[test]
fn make_space_excludes_only_the_instance() {
    // A node sharing the instance's lower-right region still moves; only the
    // instance id is excluded.
    let mut network = NodeNetwork::new_empty();
    let anchor = DVec2::new(0.0, 0.0);
    let original_size = DVec2::new(100.0, 100.0);
    let content_size = DVec2::new(200.0, 200.0); // delta = (100, 100)

    let instance = add(&mut network, "union", anchor, 0);
    // Another node at the exact same spot as the instance: it is NOT the
    // instance, and its position (0,0) is not strictly past the edges, so it
    // must not move either — confirms the strict `>` comparison.
    let coincident = add(&mut network, "union", anchor, 0);
    let past = add(&mut network, "union", DVec2::new(150.0, 150.0), 0);

    let delta = make_space_for_inline(&mut network, instance, anchor, original_size, content_size);

    assert_eq!(delta, DVec2::new(100.0, 100.0));
    assert_eq!(pos(&network, coincident), anchor);
    assert_eq!(pos(&network, past), DVec2::new(250.0, 250.0));
}

// ---------------------------------------------------------------------------
// copy_content_into
// ---------------------------------------------------------------------------

#[test]
fn copy_content_skips_parameter_nodes() {
    let mut source = NodeNetwork::new_empty();
    let p0 = add(&mut source, "parameter", DVec2::new(-300.0, 0.0), 1);
    let n1 = add(&mut source, "sphere", DVec2::new(0.0, 0.0), 0);
    let n2 = add(&mut source, "union", DVec2::new(100.0, 50.0), 2);

    let mut target = NodeNetwork::new_empty();

    let mapping = copy_content_into(&mut target, &source, DVec2::ZERO, DVec2::ZERO);

    // Parameter node absent; both non-parameter nodes mapped.
    assert_eq!(mapping.len(), 2);
    assert!(!mapping.contains_key(&p0));
    assert!(mapping.contains_key(&n1));
    assert!(mapping.contains_key(&n2));

    // Target gained exactly the two non-parameter nodes.
    assert_eq!(target.nodes.len(), 2);
    for (&old, &new) in &mapping {
        assert!(target.nodes.contains_key(&new));
        assert_eq!(
            target.nodes[&new].node_type_name,
            source.nodes[&old].node_type_name
        );
    }
}

#[test]
fn copy_content_shifts_positions_relative_to_content_min() {
    let mut source = NodeNetwork::new_empty();
    let n1 = add(&mut source, "sphere", DVec2::new(40.0, 40.0), 0);
    let n2 = add(&mut source, "union", DVec2::new(100.0, 90.0), 0);

    let mut target = NodeNetwork::new_empty();

    let content_min = DVec2::new(40.0, 40.0); // top-left of source content bbox
    let anchor = DVec2::new(1000.0, 500.0);

    let mapping = copy_content_into(&mut target, &source, anchor, content_min);

    // new_position = anchor + (old.position - content_min)
    let new1 = mapping[&n1];
    let new2 = mapping[&n2];
    assert_eq!(pos(&target, new1), DVec2::new(1000.0, 500.0)); // landed exactly on anchor
    assert_eq!(pos(&target, new2), DVec2::new(1060.0, 550.0));
}

#[test]
fn copy_content_allocates_fresh_ids_and_advances_counter() {
    let mut source = NodeNetwork::new_empty();
    let _n1 = add(&mut source, "sphere", DVec2::ZERO, 0);
    let _n2 = add(&mut source, "union", DVec2::ZERO, 0);

    let mut target = NodeNetwork::new_empty();
    // Push target's counter up so fresh ids are clearly distinct.
    let _existing = add(&mut target, "cuboid", DVec2::ZERO, 0);
    let next_before = target.next_node_id;

    let mapping = copy_content_into(&mut target, &source, DVec2::ZERO, DVec2::ZERO);

    // All new ids are >= the counter value before the copy, and unique.
    let new_ids: HashSet<u64> = mapping.values().copied().collect();
    assert_eq!(new_ids.len(), 2);
    for &id in &new_ids {
        assert!(id >= next_before);
    }
    assert_eq!(target.next_node_id, next_before + 2);
}

#[test]
#[allow(clippy::arc_with_non_send_sync)] // matches NodeNetwork's own Arc usage
fn copy_content_preserves_body_arc_verbatim() {
    let mut source = NodeNetwork::new_empty();
    let hof = add(&mut source, "map", DVec2::ZERO, 0);
    // Attach a body network as a shared Arc.
    let body = Arc::new(NodeNetwork::new_empty());
    source.nodes.get_mut(&hof).unwrap().zone = Some(body.clone());

    let mut target = NodeNetwork::new_empty();
    let mapping = copy_content_into(&mut target, &source, DVec2::ZERO, DVec2::ZERO);

    let new_hof = mapping[&hof];
    let copied_zone = target.nodes[&new_hof].zone.as_ref().expect("zone copied");
    // The copied body must be the SAME allocation (CoW share, no deep walk).
    assert!(Arc::ptr_eq(copied_zone, &body));
}

#[test]
fn copy_content_inherits_display_state_from_source() {
    let mut source = NodeNetwork::new_empty();
    let shown = add(&mut source, "sphere", DVec2::ZERO, 0); // displayed by add_node
    let hidden = add(&mut source, "union", DVec2::ZERO, 0);
    source.set_node_display_type(shown, Some(NodeDisplayType::Ghost));
    source.set_node_display_type(hidden, None); // not displayed

    let mut target = NodeNetwork::new_empty();
    let mapping = copy_content_into(&mut target, &source, DVec2::ZERO, DVec2::ZERO);

    let new_shown = mapping[&shown];
    let new_hidden = mapping[&hidden];
    assert_eq!(
        target.get_node_display_type(new_shown),
        Some(NodeDisplayType::Ghost)
    );
    assert!(!target.is_node_displayed(new_hidden));
}

#[test]
fn copy_content_dedups_name_collisions() {
    let mut source = NodeNetwork::new_empty();
    let s = add(&mut source, "sphere", DVec2::ZERO, 0);
    set_name(&mut source, s, "widget");

    let mut target = NodeNetwork::new_empty();
    let existing = add(&mut target, "cuboid", DVec2::ZERO, 0);
    set_name(&mut target, existing, "widget");

    let mapping = copy_content_into(&mut target, &source, DVec2::ZERO, DVec2::ZERO);

    let copied = mapping[&s];
    let copied_name = target.nodes[&copied].custom_name.clone().unwrap();
    assert_ne!(copied_name, "widget");
    assert_eq!(copied_name, "widget_2");
}

#[test]
fn copy_content_preserves_name_when_no_collision() {
    let mut source = NodeNetwork::new_empty();
    let s = add(&mut source, "sphere", DVec2::ZERO, 0);
    set_name(&mut source, s, "unique_name");

    let mut target = NodeNetwork::new_empty();
    let mapping = copy_content_into(&mut target, &source, DVec2::ZERO, DVec2::ZERO);

    let copied = mapping[&s];
    assert_eq!(
        target.nodes[&copied].custom_name.as_deref(),
        Some("unique_name")
    );
}
