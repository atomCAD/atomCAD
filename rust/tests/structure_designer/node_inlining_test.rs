//! Tests for the inline-custom-node building blocks (Phase 1):
//! `make_space_for_inline` and `copy_content_into`.

use glam::f64::DVec2;
use std::sync::Arc;

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::{EvalOutput, NodeData};
use rust_lib_flutter_cad::structure_designer::node_inlining::{
    copy_content_into, make_space_for_inline, splice_inline_boundary,
};
use rust_lib_flutter_cad::structure_designer::node_network::{
    Argument, IncomingWire, Node, NodeDisplayType, NodeNetwork, SourcePin,
};
use rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;
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

// ---------------------------------------------------------------------------
// splice_inline_boundary — scaffolding
// ---------------------------------------------------------------------------

/// Adds a `parameter` node with the given `param_index` (one default input pin).
fn add_param(net: &mut NodeNetwork, param_index: usize) -> u64 {
    let data = ParameterData {
        param_id: Some(param_index as u64 + 1),
        param_index,
        param_name: format!("p{param_index}"),
        data_type: DataType::Int,
        sort_order: param_index as i32,
        data_type_str: None,
        error: None,
    };
    net.add_node("parameter", DVec2::ZERO, 1, Box::new(data))
}

/// Wire `dst.arguments[arg]` to read `src` pin `pin` in the same scope (depth 0).
fn wire(net: &mut NodeNetwork, dst: u64, arg: usize, src: u64, pin: i32) {
    net.nodes.get_mut(&dst).unwrap().arguments[arg].set_source(src, pin);
}

fn wires_of(net: &NodeNetwork, node: u64, arg: usize) -> &[IncomingWire] {
    &net.nodes[&node].arguments[arg].incoming_wires
}

/// The single node id inside `node`'s zone body (tests build one-node bodies).
fn only_body_node(net: &NodeNetwork, node: u64) -> u64 {
    let body = net.nodes[&node].zone.as_ref().unwrap();
    *body.nodes.keys().next().unwrap()
}

fn body_wires(net: &NodeNetwork, hof: u64, body_node: u64, arg: usize) -> &[IncomingWire] {
    let body = net.nodes[&hof].zone.as_ref().unwrap();
    &body.nodes[&body_node].arguments[arg].incoming_wires
}

/// Runs the full inline splice (copy + boundary fix-up) on a hand-built pair.
/// Positions are irrelevant to these tests, so `content_min`/`anchor` are zero.
fn inline_direct(
    target: &mut NodeNetwork,
    source: &NodeNetwork,
    instance_id: u64,
) -> HashMap<u64, u64> {
    let id_mapping = copy_content_into(target, source, DVec2::ZERO, DVec2::ZERO);
    splice_inline_boundary(target, instance_id, source, &id_mapping);
    id_mapping
}

// ---------------------------------------------------------------------------
// splice_inline_boundary — flat cases
// ---------------------------------------------------------------------------

#[test]
fn splice_basic_single_param_single_output() {
    // N: p0 -> c (value node consuming the param); return = c.
    let mut source = NodeNetwork::new_empty();
    let p0 = add_param(&mut source, 0);
    let c = add(&mut source, "value", DVec2::ZERO, 1);
    wire(&mut source, c, 0, p0, 0);
    source.return_node_id = Some(c);

    // target: X -> I(pin0); Y reads I(pin0).
    let mut target = NodeNetwork::new_empty();
    let x = add(&mut target, "float", DVec2::ZERO, 0);
    let i = add(&mut target, "helper", DVec2::ZERO, 1);
    wire(&mut target, i, 0, x, 0);
    let y = add(&mut target, "value", DVec2::ZERO, 1);
    wire(&mut target, y, 0, i, 0);

    let mapping = inline_direct(&mut target, &source, i);
    let c2 = mapping[&c];

    // Instance gone; copied content present.
    assert!(!target.nodes.contains_key(&i));
    assert!(target.nodes.contains_key(&c2));
    // Param ref inside the copied content spliced to the instance's input source X.
    assert_eq!(wires_of(&target, c2, 0), &[IncomingWire::node_output(x, 0)]);
    // Consumer of the instance output repointed to the return node (copied c).
    assert_eq!(wires_of(&target, y, 0), &[IncomingWire::node_output(c2, 0)]);
}

#[test]
fn splice_multi_parameter() {
    // N: p0, p1 -> c (2-input node); return = c.
    let mut source = NodeNetwork::new_empty();
    let p0 = add_param(&mut source, 0);
    let p1 = add_param(&mut source, 1);
    let c = add(&mut source, "value", DVec2::ZERO, 2);
    wire(&mut source, c, 0, p0, 0);
    wire(&mut source, c, 1, p1, 0);
    source.return_node_id = Some(c);

    // target: X -> I(pin0), Z -> I(pin1).
    let mut target = NodeNetwork::new_empty();
    let x = add(&mut target, "float", DVec2::ZERO, 0);
    let z = add(&mut target, "float", DVec2::ZERO, 0);
    let i = add(&mut target, "helper", DVec2::ZERO, 2);
    wire(&mut target, i, 0, x, 0);
    wire(&mut target, i, 1, z, 0);

    let mapping = inline_direct(&mut target, &source, i);
    let c2 = mapping[&c];

    // Each copied param ref goes to the matching instance input source.
    assert_eq!(wires_of(&target, c2, 0), &[IncomingWire::node_output(x, 0)]);
    assert_eq!(wires_of(&target, c2, 1), &[IncomingWire::node_output(z, 0)]);
}

#[test]
fn splice_multi_output_preserves_pin_index() {
    // Consumer reads the instance's output pin 1; after inline it reads the
    // return node's pin 1 (multi-output passthrough).
    let mut source = NodeNetwork::new_empty();
    let c = add(&mut source, "value", DVec2::ZERO, 0);
    source.return_node_id = Some(c);

    let mut target = NodeNetwork::new_empty();
    let i = add(&mut target, "helper", DVec2::ZERO, 0);
    let y = add(&mut target, "value", DVec2::ZERO, 1);
    wire(&mut target, y, 0, i, 1); // read pin 1

    let mapping = inline_direct(&mut target, &source, i);
    let c2 = mapping[&c];

    assert_eq!(wires_of(&target, y, 0), &[IncomingWire::node_output(c2, 1)]);
}

#[test]
fn splice_unconnected_input_uses_param_default() {
    // p0 has a default value provider d inside N; the instance pin is unwired,
    // so the copied param ref falls back to the (remapped) default.
    let mut source = NodeNetwork::new_empty();
    let p0 = add_param(&mut source, 0);
    let d = add(&mut source, "int", DVec2::ZERO, 0); // default provider
    wire(&mut source, p0, 0, d, 0); // parameter's default input
    let c = add(&mut source, "value", DVec2::ZERO, 1);
    wire(&mut source, c, 0, p0, 0);
    source.return_node_id = Some(c);

    let mut target = NodeNetwork::new_empty();
    let i = add(&mut target, "helper", DVec2::ZERO, 1); // pin 0 unconnected

    let mapping = inline_direct(&mut target, &source, i);
    let c2 = mapping[&c];
    let d2 = mapping[&d];

    // The param ref resolves to the copied default provider.
    assert_eq!(
        wires_of(&target, c2, 0),
        &[IncomingWire::node_output(d2, 0)]
    );
}

#[test]
fn splice_unconnected_input_no_default_drops_wire() {
    let mut source = NodeNetwork::new_empty();
    let p0 = add_param(&mut source, 0);
    let c = add(&mut source, "value", DVec2::ZERO, 1);
    wire(&mut source, c, 0, p0, 0);
    source.return_node_id = Some(c);

    let mut target = NodeNetwork::new_empty();
    let i = add(&mut target, "helper", DVec2::ZERO, 1); // unconnected, no default

    let mapping = inline_direct(&mut target, &source, i);
    let c2 = mapping[&c];

    // No instance wire and no default -> the param ref is dropped.
    assert!(wires_of(&target, c2, 0).is_empty());
}

#[test]
fn splice_no_return_drops_consumers() {
    let mut source = NodeNetwork::new_empty();
    let _c = add(&mut source, "value", DVec2::ZERO, 0);
    // return_node_id stays None.

    let mut target = NodeNetwork::new_empty();
    let i = add(&mut target, "helper", DVec2::ZERO, 0);
    let y = add(&mut target, "value", DVec2::ZERO, 1);
    wire(&mut target, y, 0, i, 0);

    inline_direct(&mut target, &source, i);

    // No return node -> the consumer wire is dropped.
    assert!(wires_of(&target, y, 0).is_empty());
}

// ---------------------------------------------------------------------------
// splice_inline_boundary — scope-aware (bodies / captures)
// ---------------------------------------------------------------------------

#[allow(clippy::arc_with_non_send_sync)]
fn attach_body(net: &mut NodeNetwork, hof: u64, body: NodeNetwork) {
    net.nodes.get_mut(&hof).unwrap().zone = Some(std::sync::Arc::new(body));
}

#[test]
fn splice_nested_capture_of_parameter_k1() {
    // N: p0; hof(map) whose body node captures p0 at depth 1; return = hof.
    let mut source = NodeNetwork::new_empty();
    let p0 = add_param(&mut source, 0);
    let hof = add(&mut source, "map", DVec2::ZERO, 0);
    let mut body = NodeNetwork::new_empty();
    let e = body.add_node("value", DVec2::ZERO, 1, Box::new(MockNodeData));
    body.nodes.get_mut(&e).unwrap().arguments[0].set_source_full(
        p0,
        SourcePin::NodeOutput { pin_index: 0 },
        1,
    );
    attach_body(&mut source, hof, body);
    source.return_node_id = Some(hof);

    // target: X -> I(pin0).
    let mut target = NodeNetwork::new_empty();
    let x = add(&mut target, "float", DVec2::ZERO, 0);
    let i = add(&mut target, "helper", DVec2::ZERO, 1);
    wire(&mut target, i, 0, x, 0);

    let mapping = inline_direct(&mut target, &source, i);
    let hof2 = mapping[&hof];
    let e2 = only_body_node(&target, hof2);

    // The depth-1 capture of p0 becomes a depth-1 capture of X (instance source,
    // shifted by k=1).
    assert_eq!(
        body_wires(&target, hof2, e2, 0),
        &[IncomingWire {
            source_node_id: x,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        }]
    );
}

#[test]
#[allow(clippy::arc_with_non_send_sync)] // matches NodeNetwork's own Arc usage
fn splice_nested_capture_of_parameter_k2() {
    // N: p0; outer map whose body holds an inner map whose body captures p0 at
    // depth 2.
    let mut source = NodeNetwork::new_empty();
    let p0 = add_param(&mut source, 0);
    let outer = add(&mut source, "map", DVec2::ZERO, 0);

    let mut outer_body = NodeNetwork::new_empty();
    let inner = outer_body.add_node("map", DVec2::ZERO, 0, Box::new(MockNodeData));
    let mut inner_body = NodeNetwork::new_empty();
    let leaf = inner_body.add_node("value", DVec2::ZERO, 1, Box::new(MockNodeData));
    inner_body.nodes.get_mut(&leaf).unwrap().arguments[0].set_source_full(
        p0,
        SourcePin::NodeOutput { pin_index: 0 },
        2,
    );
    outer_body.nodes.get_mut(&inner).unwrap().zone = Some(std::sync::Arc::new(inner_body));
    attach_body(&mut source, outer, outer_body);
    source.return_node_id = Some(outer);

    let mut target = NodeNetwork::new_empty();
    let x = add(&mut target, "float", DVec2::ZERO, 0);
    let i = add(&mut target, "helper", DVec2::ZERO, 1);
    wire(&mut target, i, 0, x, 0);

    let mapping = inline_direct(&mut target, &source, i);
    let outer2 = mapping[&outer];

    // Reach the leaf two bodies deep and verify the depth-2 capture now reaches
    // X at depth 2.
    let outer_b = target.nodes[&outer2].zone.as_ref().unwrap();
    let inner_id = *outer_b.nodes.keys().next().unwrap();
    let inner_b = outer_b.nodes[&inner_id].zone.as_ref().unwrap();
    let leaf_id = *inner_b.nodes.keys().next().unwrap();
    let leaf_wires = &inner_b.nodes[&leaf_id].arguments[0].incoming_wires;
    assert_eq!(
        leaf_wires,
        &[IncomingWire {
            source_node_id: x,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 2,
        }]
    );
}

#[test]
fn splice_nested_capture_of_copied_node() {
    // N: content a; hof whose body captures sibling a at depth 1; return = hof.
    let mut source = NodeNetwork::new_empty();
    let a = add(&mut source, "int", DVec2::ZERO, 0);
    let hof = add(&mut source, "map", DVec2::ZERO, 0);
    let mut body = NodeNetwork::new_empty();
    let e = body.add_node("value", DVec2::ZERO, 1, Box::new(MockNodeData));
    body.nodes.get_mut(&e).unwrap().arguments[0].set_source_full(
        a,
        SourcePin::NodeOutput { pin_index: 0 },
        1,
    );
    attach_body(&mut source, hof, body);
    source.return_node_id = Some(hof);

    let mut target = NodeNetwork::new_empty();
    let i = add(&mut target, "helper", DVec2::ZERO, 0);

    let mapping = inline_direct(&mut target, &source, i);
    let hof2 = mapping[&hof];
    let a2 = mapping[&a];
    let e2 = only_body_node(&target, hof2);

    // Capture of a co-copied sibling: remapped through id_mapping, depth kept.
    assert_eq!(
        body_wires(&target, hof2, e2, 0),
        &[IncomingWire {
            source_node_id: a2,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        }]
    );
}

#[test]
fn splice_new_id_collision_with_old_param_id() {
    // Engineer a copied node whose NEW id equals an OLD parameter id, and verify
    // classification (by N's old id space) still distinguishes a param capture
    // from a copied-node capture.
    let mut source = NodeNetwork::new_empty(); // next_node_id = 1
    let p0 = add_param(&mut source, 0); // old id 1
    let a = add(&mut source, "int", DVec2::ZERO, 0); // old id 2
    let hof = add(&mut source, "map", DVec2::ZERO, 0); // old id 3
    let mut body = NodeNetwork::new_empty();
    let e = body.add_node("value", DVec2::ZERO, 2, Box::new(MockNodeData));
    // arg0 captures sibling a (old 2), arg1 captures param p0 (old 1).
    body.nodes.get_mut(&e).unwrap().arguments[0].set_source_full(
        a,
        SourcePin::NodeOutput { pin_index: 0 },
        1,
    );
    body.nodes.get_mut(&e).unwrap().arguments[1].set_source_full(
        p0,
        SourcePin::NodeOutput { pin_index: 0 },
        1,
    );
    attach_body(&mut source, hof, body);
    source.return_node_id = Some(a);

    // target: a manual instance (id 100) wired to X (id 50); force next_node_id
    // to 1 so the first copied node (a) gets new id 1 == old p0 id.
    let mut target = NodeNetwork::new_empty();
    let x = 50u64;
    target.nodes.insert(
        x,
        Node {
            id: x,
            node_type_name: "float".to_string(),
            custom_name: Some("x".to_string()),
            position: DVec2::ZERO,
            arguments: vec![],
            data: Box::new(MockNodeData),
            custom_node_type: None,
            zone: None,
            zone_output_arguments: vec![],
            body_width: 320.0,
            body_height: 180.0,
            collapse_mode:
                rust_lib_flutter_cad::structure_designer::node_network::CollapseMode::Auto,
        },
    );
    let inst = 100u64;
    let mut inst_arg = Argument::new();
    inst_arg.set_source(x, 0);
    target.nodes.insert(
        inst,
        Node {
            id: inst,
            node_type_name: "helper".to_string(),
            custom_name: Some("inst".to_string()),
            position: DVec2::ZERO,
            arguments: vec![inst_arg],
            data: Box::new(MockNodeData),
            custom_node_type: None,
            zone: None,
            zone_output_arguments: vec![],
            body_width: 320.0,
            body_height: 180.0,
            collapse_mode:
                rust_lib_flutter_cad::structure_designer::node_network::CollapseMode::Auto,
        },
    );
    target.next_node_id = 1; // force the collision

    let mapping = inline_direct(&mut target, &source, inst);
    let a2 = mapping[&a];
    let hof2 = mapping[&hof];
    assert_eq!(a2, 1, "copied `a` should take new id 1 (== old p0 id)");

    let e2 = only_body_node(&target, hof2);
    // arg0 (capture of old a=2) -> remapped to a2 (new id 1).
    assert_eq!(
        body_wires(&target, hof2, e2, 0),
        &[IncomingWire {
            source_node_id: a2,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        }]
    );
    // arg1 (capture of old p0=1) -> param-spliced to X, NOT misclassified as a2.
    assert_eq!(
        body_wires(&target, hof2, e2, 1),
        &[IncomingWire {
            source_node_id: x,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        }]
    );
}

#[test]
fn splice_sibling_hof_body_captures_instance_output() {
    // A pre-existing sibling HOF in the target captures the instance's output
    // from inside its body (depth 1); Descent B repoints it to the return node.
    let mut source = NodeNetwork::new_empty();
    let c = add(&mut source, "value", DVec2::ZERO, 0);
    source.return_node_id = Some(c);

    let mut target = NodeNetwork::new_empty();
    let i = add(&mut target, "helper", DVec2::ZERO, 0);
    let sibling = add(&mut target, "map", DVec2::ZERO, 0);
    let mut body = NodeNetwork::new_empty();
    let g = body.add_node("value", DVec2::ZERO, 1, Box::new(MockNodeData));
    body.nodes.get_mut(&g).unwrap().arguments[0].set_source_full(
        i,
        SourcePin::NodeOutput { pin_index: 0 },
        1,
    );
    attach_body(&mut target, sibling, body);

    let mapping = inline_direct(&mut target, &source, i);
    let c2 = mapping[&c];
    let g_id = only_body_node(&target, sibling);

    // The deep capture of the instance output now reads the return node, depth kept.
    assert_eq!(
        body_wires(&target, sibling, g_id, 0),
        &[IncomingWire {
            source_node_id: c2,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        }]
    );
}

// ---------------------------------------------------------------------------
// Orchestrator (StructureDesigner) + undo
// ---------------------------------------------------------------------------

/// Build a `StructureDesigner` with a custom network `helper` (one Int param,
/// an `int` content node as return) and a `main` network holding one instance
/// of `helper`. Returns (designer, instance_node_id).
fn setup_with_helper_instance() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();

    // Build "helper": parameter + int content (return).
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    designer.add_node("parameter", DVec2::new(-200.0, 0.0));
    let content = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.set_return_node_id(Some(content));

    // Build "main" with one instance of helper.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let inst = designer.add_node("helper", DVec2::new(0.0, 0.0));

    (designer, inst)
}

#[test]
fn inline_orchestrator_basic_removes_instance_and_adds_content() {
    let (mut designer, inst) = setup_with_helper_instance();

    designer.inline_custom_node(vec![], inst).unwrap();

    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    // Instance gone.
    assert!(!main.nodes.contains_key(&inst));
    // No remaining "helper" instance; the int content was copied in.
    assert!(main.nodes.values().all(|n| n.node_type_name != "helper"));
    assert!(main.nodes.values().any(|n| n.node_type_name == "int"));
    // The "helper" definition is untouched in the registry.
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("helper")
    );
}

#[test]
fn inline_rejects_non_custom_node() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let sphere = designer.add_node("sphere", DVec2::ZERO);

    let result = designer.inline_custom_node(vec![], sphere);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Only custom network nodes can be inlined"
    );
}

#[test]
fn inline_undo_redo_roundtrip() {
    let (mut designer, inst) = setup_with_helper_instance();
    designer.undo_stack.clear();

    let before = snapshot_main(&mut designer);
    designer.inline_custom_node(vec![], inst).unwrap();
    let after = snapshot_main(&mut designer);
    assert_ne!(before, after, "inline should change the network");

    designer.undo();
    assert_eq!(
        snapshot_main(&mut designer),
        before,
        "undo restores network"
    );

    designer.redo();
    assert_eq!(snapshot_main(&mut designer), after, "redo reapplies inline");
}

/// Serialize the `main` network to a normalized JSON value (HashMap-order
/// independent) for undo/redo comparison.
fn snapshot_main(designer: &mut StructureDesigner) -> serde_json::Value {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::node_network_to_serializable;

    let registry = &mut designer.node_type_registry;
    let (built_in_types, node_networks) =
        (&registry.built_in_node_types, &mut registry.node_networks);
    let network = node_networks.get_mut("main").unwrap();
    let serializable = node_network_to_serializable(network, built_in_types, None).unwrap();
    let mut value = serde_json::to_value(&serializable).unwrap();
    normalize_json(&mut value);
    value
}

// ---------------------------------------------------------------------------
// Orchestrator — inlining inside a zone body (Phase 3)
// ---------------------------------------------------------------------------

/// Build a `StructureDesigner` with a custom network `helper` (one param, an
/// `int` content node as return) and a `main` network whose top level holds a
/// `map` HOF; an instance of `helper` is added *inside* the map's body.
/// Returns (designer, map_id, instance_node_id_in_body).
fn setup_helper_instance_in_body() -> (StructureDesigner, u64, u64) {
    let mut designer = StructureDesigner::new();

    // Build "helper": parameter + int content (return).
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    designer.add_node("parameter", DVec2::new(-200.0, 0.0));
    let content = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.set_return_node_id(Some(content));

    // Build "main" with a top-level `map` whose body holds a helper instance.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    let inst = designer.add_node_scoped(&[map_id], "helper", DVec2::new(0.0, 0.0), None);
    assert_ne!(inst, 0, "failed to add helper instance into the map body");

    (designer, map_id, inst)
}

#[test]
fn inline_inside_body_removes_instance_and_adds_content() {
    let (mut designer, map_id, inst) = setup_helper_instance_in_body();

    designer.inline_custom_node(vec![map_id], inst).unwrap();

    let body = designer.get_scope_network(&[map_id]).unwrap();
    // Instance gone from the body; the int content was copied in.
    assert!(!body.nodes.contains_key(&inst));
    assert!(body.nodes.values().all(|n| n.node_type_name != "helper"));
    assert!(body.nodes.values().any(|n| n.node_type_name == "int"));
    // The "helper" definition is untouched in the registry.
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("helper")
    );
}

#[test]
fn inline_inside_body_preserves_instance_capture_wire() {
    // The instance's input pin is fed by a *capture* (depth 1) of a top-level
    // node `x`. helper's content consumes that parameter, so after inlining the
    // copied content's parameter reference must be spliced to the capture,
    // verbatim at nesting k = 0 (depth == k + d_I == 0 + 1 == 1).
    let mut designer = StructureDesigner::new();

    // helper: parameter p0; content = array_at whose Int `index` pin reads p0;
    // return = array_at.
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let p0 = designer.add_node("parameter", DVec2::new(-200.0, 0.0));
    let content = designer.add_node("array_at", DVec2::new(0.0, 0.0));
    {
        let helper = designer
            .node_type_registry
            .node_networks
            .get_mut("helper")
            .unwrap();
        // arg[1] is array_at's Int `index` pin.
        helper.nodes.get_mut(&content).unwrap().arguments[1].set_source(p0, 0);
    }
    designer.set_return_node_id(Some(content));

    // main: a top-level Int source `x`, a `map`, and a helper instance in the
    // body whose pin 0 captures `x` at depth 1.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let x = designer.add_node("int", DVec2::new(-300.0, -100.0));
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    let inst = designer.add_node_scoped(&[map_id], "helper", DVec2::new(0.0, 0.0), None);
    {
        let body = designer.get_scope_network_mut(&[map_id]).unwrap();
        body.nodes.get_mut(&inst).unwrap().arguments[0].set_source_full(
            x,
            SourcePin::NodeOutput { pin_index: 0 },
            1,
        );
    }

    designer.inline_custom_node(vec![map_id], inst).unwrap();

    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        !body.nodes.contains_key(&inst),
        "instance removed from body"
    );
    let copied = body
        .nodes
        .values()
        .find(|n| n.node_type_name == "array_at")
        .expect("array_at content copied into the body");
    // The copied parameter reference resolves to the instance's capture of `x`
    // at depth 1 (k = 0 + d_I = 1).
    assert_eq!(
        copied.arguments[1].incoming_wires,
        vec![IncomingWire {
            source_node_id: x,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 1,
        }]
    );
}

#[test]
fn inline_inside_body_undo_redo_roundtrip() {
    let (mut designer, map_id, inst) = setup_helper_instance_in_body();
    designer.undo_stack.clear();

    let before = snapshot_body(&mut designer, map_id);
    designer.inline_custom_node(vec![map_id], inst).unwrap();
    let after = snapshot_body(&mut designer, map_id);
    assert_ne!(before, after, "inline should change the body");

    designer.undo();
    assert_eq!(
        snapshot_body(&mut designer, map_id),
        before,
        "undo restores the body"
    );

    designer.redo();
    assert_eq!(
        snapshot_body(&mut designer, map_id),
        after,
        "redo reapplies the inline"
    );
}

/// Serialize the `map` body of `main` to a normalized JSON value for undo/redo
/// comparison (HashMap-order independent).
fn snapshot_body(designer: &mut StructureDesigner, map_id: u64) -> serde_json::Value {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::node_network_to_serializable;

    let registry = &mut designer.node_type_registry;
    let (built_in_types, node_networks) =
        (&registry.built_in_node_types, &mut registry.node_networks);
    let main = node_networks.get_mut("main").unwrap();
    let body = main.nodes.get_mut(&map_id).unwrap().zone_mut().unwrap();
    let serializable = node_network_to_serializable(body, built_in_types, None).unwrap();
    let mut value = serde_json::to_value(&serializable).unwrap();
    normalize_json(&mut value);
    value
}

/// Sort HashMap-derived arrays (`nodes`, `displayed_node_ids`,
/// `displayed_output_pins`) so comparison is deterministic.
fn normalize_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if (key == "nodes" || key == "displayed_node_ids" || key == "displayed_output_pins")
                    && let serde_json::Value::Array(arr) = val
                {
                    arr.sort_by(|a, b| {
                        serde_json::to_string(a)
                            .unwrap_or_default()
                            .cmp(&serde_json::to_string(b).unwrap_or_default())
                    });
                }
                normalize_json(val);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                normalize_json(v);
            }
        }
        _ => {}
    }
}
