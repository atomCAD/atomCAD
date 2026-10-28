//! The shared Arrange operations (`layout::arrange`) and the GUI's Arrange
//! menu on top of them (`StructureDesigner::arrange_selected_nodes`) —
//! `doc/design_deliberate_node_placement.md` Phase 2.
//!
//! Every op is pinned on exact positions, with an unlisted bystander that must
//! not move. The menu half pins what the GUI adds: the selection order the
//! stack ops read, one undo step that also takes back `hand_moved`, and no push.

use std::collections::HashMap;

use atomcad_structure_designer::layout::{ArrangeOp, Gap, arrange, measure_scope};
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::DVec2;

fn designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn network(designer: &StructureDesigner) -> &NodeNetwork {
    designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap()
}

fn pos(designer: &StructureDesigner, id: u64) -> DVec2 {
    network(designer).nodes[&id].position
}

fn size(designer: &StructureDesigner, id: u64) -> DVec2 {
    measure_scope(network(designer), &designer.node_type_registry)[&id]
}

/// Run `op` over `ids` without committing anything.
fn targets(designer: &StructureDesigner, ids: &[u64], op: ArrangeOp) -> HashMap<u64, DVec2> {
    let net = network(designer);
    let sizes = measure_scope(net, &designer.node_type_registry);
    arrange(net, &designer.node_type_registry, &sizes, ids, op)
}

fn select(designer: &mut StructureDesigner, ids: &[u64]) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    net.clear_selection();
    for &id in ids {
        net.toggle_node_selection(id);
    }
}

/// Three `float`s spread out, plus a bystander nobody lists.
fn three_floats(designer: &mut StructureDesigner) -> (u64, u64, u64, u64) {
    let a = designer.add_node("float", DVec2::new(100.0, 40.0));
    let b = designer.add_node("float", DVec2::new(130.0, 200.0));
    let c = designer.add_node("float", DVec2::new(90.0, 500.0));
    let bystander = designer.add_node("float", DVec2::new(700.0, 700.0));
    (a, b, c, bystander)
}

// ============================================================================
// The ops
// ============================================================================

#[test]
fn align_puts_each_edge_on_the_extreme_one() {
    let mut d = designer();
    let (a, b, c, bystander) = three_floats(&mut d);

    let left = targets(&d, &[a, b, c], ArrangeOp::AlignLeft);
    assert_eq!(left[&a], DVec2::new(90.0, 40.0));
    assert_eq!(left[&b], DVec2::new(90.0, 200.0));
    assert_eq!(left[&c], DVec2::new(90.0, 500.0));
    assert!(
        !left.contains_key(&bystander),
        "only listed nodes are placed"
    );

    let right = targets(&d, &[a, b, c], ArrangeOp::AlignRight);
    assert_eq!(right[&a], DVec2::new(130.0, 40.0));
    assert_eq!(right[&c], DVec2::new(130.0, 500.0));

    let top = targets(&d, &[a, b, c], ArrangeOp::AlignTop);
    assert_eq!(top[&b], DVec2::new(130.0, 40.0));
    assert_eq!(top[&c], DVec2::new(90.0, 40.0));

    let bottom = targets(&d, &[a, b, c], ArrangeOp::AlignBottom);
    assert_eq!(
        bottom[&a],
        DVec2::new(100.0, 500.0),
        "all three are one height"
    );
    assert_eq!(bottom.len(), 3);
}

#[test]
fn stack_column_takes_the_order_given_and_the_normal_gap() {
    let mut d = designer();
    let (a, b, c, _) = three_floats(&mut d);
    let h = size(&d, a).y;

    // Written order c, a, b — not the drawing's order.
    let t = targets(&d, &[c, a, b], ArrangeOp::StackColumn(Gap::Normal));
    assert_eq!(
        t[&c],
        DVec2::new(90.0, 40.0),
        "first member at the box's top-left"
    );
    assert_eq!(t[&a], DVec2::new(90.0, 40.0 + h + 30.0));
    assert_eq!(t[&b], DVec2::new(90.0, 40.0 + 2.0 * (h + 30.0)));
}

#[test]
fn stack_row_uses_the_across_gap() {
    let mut d = designer();
    let (a, b, c, _) = three_floats(&mut d);
    let w = size(&d, a).x;

    let t = targets(&d, &[a, b, c], ArrangeOp::StackRow(Gap::Wide));
    assert_eq!(t[&a], DVec2::new(90.0, 40.0));
    assert_eq!(t[&b], DVec2::new(90.0 + w + 100.0, 40.0));
    assert_eq!(t[&c], DVec2::new(90.0 + 2.0 * (w + 100.0), 40.0));
}

#[test]
fn gap_words_have_one_set_of_numbers() {
    assert_eq!((Gap::Tight.across(), Gap::Tight.down()), (25.0, 15.0));
    assert_eq!((Gap::Normal.across(), Gap::Normal.down()), (50.0, 30.0));
    assert_eq!((Gap::Wide.across(), Gap::Wide.down()), (100.0, 60.0));
    assert_eq!((Gap::Px(42.0).across(), Gap::Px(42.0).down()), (42.0, 42.0));
}

#[test]
fn distribute_keeps_the_extremes_and_evens_the_gaps() {
    let mut d = designer();
    let (a, b, c, _) = three_floats(&mut d);
    let h = size(&d, a).y;

    let t = targets(&d, &[b, c, a], ArrangeOp::DistributeVertically);
    assert_eq!(t[&a], DVec2::new(100.0, 40.0), "the top one stays");
    assert_eq!(t[&c], DVec2::new(90.0, 500.0), "the bottom one stays");
    let gap = (500.0 + h - 40.0 - 3.0 * h) / 2.0;
    assert_eq!(t[&b], DVec2::new(130.0, 40.0 + h + gap), "x untouched");

    let t = targets(&d, &[a, b, c], ArrangeOp::DistributeHorizontally);
    assert_eq!(t[&c].x, 90.0);
    assert_eq!(t[&b].x, 130.0);
    assert_eq!(t[&a].x, 110.0, "halfway between, all three the same width");
    assert_eq!(t[&a].y, 40.0, "y untouched");
}

/// `float -> sphere.0`, `sphere -> diff.1`: a chain whose second wire lands
/// on the second pin row, so the straightened offset is not zero.
fn chain(d: &mut StructureDesigner) -> (u64, u64, u64) {
    let float = d.add_node("float", DVec2::new(0.0, 100.0));
    let sphere = d.add_node("sphere", DVec2::new(250.0, 400.0));
    let diff = d.add_node("diff", DVec2::new(500.0, -150.0));
    d.connect_nodes(float, 0, sphere, 0);
    d.connect_nodes(sphere, 0, diff, 1);
    let net = network(d);
    assert_eq!(net.nodes[&sphere].arguments[0].incoming_wires.len(), 1);
    assert_eq!(net.nodes[&diff].arguments[1].incoming_wires.len(), 1);
    (float, sphere, diff)
}

#[test]
fn straighten_follows_a_chain_in_topological_order() {
    let mut d = designer();
    let (float, sphere, diff) = chain(&mut d);

    // Listed downstream first: the op orders them itself, so `diff` aligns to
    // `sphere`'s *straightened* position.
    let t = targets(&d, &[diff, sphere], ArrangeOp::Straighten);
    assert_eq!(
        t[&sphere],
        DVec2::new(250.0, 100.0),
        "pin 0 to pin 0: tops level"
    );
    // Output pin 0 sits one row above input pin 1.
    assert_eq!(t[&diff], DVec2::new(500.0, 100.0 - 22.0));
    assert!(!t.contains_key(&float), "the unlisted source stays");
    assert_eq!(pos(&d, float), DVec2::new(0.0, 100.0));
}

#[test]
fn straighten_leaves_a_node_without_a_wired_input_where_it_is() {
    let mut d = designer();
    let (a, b, _, _) = three_floats(&mut d);
    let t = targets(&d, &[a, b], ArrangeOp::Straighten);
    assert_eq!(t[&a], pos(&d, a));
    assert_eq!(t[&b], pos(&d, b));
}

#[test]
fn layout_keeps_the_top_left_and_moves_only_the_listed() {
    let mut d = designer();
    let (float, sphere, diff) = chain(&mut d);
    let bystander = d.add_node("float", DVec2::new(900.0, 900.0));

    let t = targets(&d, &[float, sphere, diff], ArrangeOp::Layout);
    let min = t.values().copied().reduce(|a, b| a.min(b)).unwrap();
    assert_eq!(min, DVec2::new(0.0, -150.0), "the bounding box's top-left");
    assert!(t[&float].x < t[&sphere].x && t[&sphere].x < t[&diff].x);
    assert!(!t.contains_key(&bystander));
}

// ============================================================================
// The menu
// ============================================================================

#[test]
fn the_menu_stacks_the_selection_in_reading_order() {
    let mut d = designer();
    let (a, b, c, bystander) = three_floats(&mut d);
    let h = size(&d, a).y;
    select(&mut d, &[c, b, a]);

    assert!(d.arrange_selected_nodes(&[], ArrangeOp::StackColumn(Gap::Normal)));
    assert_eq!(pos(&d, a), DVec2::new(90.0, 40.0));
    assert_eq!(pos(&d, b), DVec2::new(90.0, 40.0 + h + 30.0));
    assert_eq!(pos(&d, c), DVec2::new(90.0, 40.0 + 2.0 * (h + 30.0)));
    assert_eq!(pos(&d, bystander), DVec2::new(700.0, 700.0));
}

#[test]
fn the_menu_does_not_push_what_it_lands_on() {
    let mut d = designer();
    let a = d.add_node("float", DVec2::new(0.0, 0.0));
    let b = d.add_node("float", DVec2::new(0.0, 400.0));
    // Exactly where `b` will land.
    let obstacle = d.add_node("float", DVec2::new(400.0, 0.0));
    d.node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap()
        .nodes
        .get_mut(&b)
        .unwrap()
        .position = DVec2::new(400.0, 400.0);
    select(&mut d, &[a, b]);

    assert!(d.arrange_selected_nodes(&[], ArrangeOp::AlignTop));
    assert_eq!(pos(&d, b), DVec2::new(400.0, 0.0));
    assert_eq!(
        pos(&d, obstacle),
        DVec2::new(400.0, 0.0),
        "D11: nothing is pushed"
    );
}

#[test]
fn one_undo_restores_positions_and_hand_moved() {
    let mut d = designer();
    let (a, b, c, _) = three_floats(&mut d);
    d.undo_stack.clear();
    select(&mut d, &[a, b, c]);

    assert!(d.arrange_selected_nodes(&[], ArrangeOp::AlignLeft));
    assert!(
        network(&d).nodes[&b].hand_moved,
        "an Arrange is a hand placement"
    );
    assert!(
        !network(&d).nodes[&c].hand_moved,
        "c was already on the edge: it did not move, so nothing is claimed"
    );

    assert!(d.undo());
    assert_eq!(pos(&d, a), DVec2::new(100.0, 40.0));
    assert_eq!(pos(&d, b), DVec2::new(130.0, 200.0));
    assert!(!network(&d).nodes[&a].hand_moved);
    assert!(!network(&d).nodes[&b].hand_moved);
    assert!(!d.undo(), "the whole arrange was one step");

    assert!(d.redo());
    assert_eq!(pos(&d, b), DVec2::new(90.0, 200.0));
    assert!(network(&d).nodes[&b].hand_moved);
}

#[test]
fn an_arrange_that_moves_nothing_pushes_nothing() {
    let mut d = designer();
    let a = d.add_node("float", DVec2::new(0.0, 0.0));
    let b = d.add_node("float", DVec2::new(0.0, 300.0));
    d.undo_stack.clear();
    select(&mut d, &[a, b]);

    assert!(!d.arrange_selected_nodes(&[], ArrangeOp::AlignLeft));
    assert!(!d.undo());
}

#[test]
fn the_menu_works_inside_a_body() {
    let mut d = designer();
    let map = d.add_node("map", DVec2::ZERO);
    let x = d.add_node_scoped(&[map], "float", DVec2::new(20.0, 10.0), None);
    let y = d.add_node_scoped(&[map], "float", DVec2::new(60.0, 200.0), None);
    {
        let body = d.get_scope_network_mut(&[map]).unwrap();
        body.clear_selection();
        body.toggle_node_selection(x);
        body.toggle_node_selection(y);
    }
    d.undo_stack.clear();

    assert!(d.arrange_selected_nodes(&[map], ArrangeOp::AlignLeft));
    let body = d.get_scope_network(&[map]).unwrap();
    assert_eq!(body.nodes[&y].position, DVec2::new(20.0, 200.0));
    assert!(body.nodes[&y].hand_moved);

    assert!(d.undo());
    let body = d.get_scope_network(&[map]).unwrap();
    assert_eq!(body.nodes[&y].position, DVec2::new(60.0, 200.0));
}

#[test]
fn straighten_reads_a_function_pin_from_the_title_bar() {
    // The function pin sits in the title bar (16 px down), not on the pin rows
    // that `node_layout::output_pin_position` assumes.
    let mut d = designer();
    let sphere = d.add_node("sphere", DVec2::new(0.0, 100.0));
    let apply = d.add_node("apply", DVec2::new(300.0, 400.0));
    d.connect_nodes(sphere, -1, apply, 0);
    assert_eq!(
        network(&d).nodes[&apply].arguments[0].incoming_wires.len(),
        1,
        "the function pin is wired into `apply.f`"
    );

    let t = targets(&d, &[sphere, apply], ArrangeOp::Straighten);
    assert_eq!(t[&apply], DVec2::new(300.0, 100.0 + 16.0 - (33.0 + 11.0)));
}
