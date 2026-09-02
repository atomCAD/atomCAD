//! `Node::hand_moved` — "a human put this node here"
//! (`doc/design_incremental_layout.md` D5).
//!
//! A **tiebreaker, never a hard constraint**: the incremental pass prefers to
//! push a node nobody placed deliberately, and an explicit full reflow can
//! offer to respect these. Nothing reads it as permission to skip a repair, so
//! there is no test here asserting that a flagged node is immovable — that
//! would be pinning the wrong contract.
//!
//! What is pinned: exactly one path sets it (the drag), it survives the file
//! and the clipboard, an older file loads without it, and one Ctrl+Z takes back
//! both the position and the claim.

use atomcad_structure_designer::node_network::Node;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    node_network_to_serializable, serializable_to_node_network,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::DVec2;

fn designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn node(designer: &StructureDesigner, id: u64) -> &Node {
    designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap()
        .nodes
        .get(&id)
        .unwrap()
}

fn drag(designer: &mut StructureDesigner, id: u64, delta: DVec2) {
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.clear_selection();
        network.select_node(id);
    }
    designer.begin_move_nodes();
    designer.move_selected_nodes(delta);
    designer.end_move_nodes();
}

// ============================================================================
// Setting it
// ============================================================================

#[test]
fn a_fresh_node_is_not_hand_moved() {
    let mut designer = designer();
    let id = designer.add_node("sphere", DVec2::ZERO);
    assert!(!node(&designer, id).hand_moved);
}

#[test]
fn a_drag_sets_it_and_undo_takes_it_back() {
    let mut designer = designer();
    let id = designer.add_node("sphere", DVec2::ZERO);
    designer.undo_stack.clear();

    drag(&mut designer, id, DVec2::new(40.0, 0.0));
    assert!(node(&designer, id).hand_moved, "a drag is a hand placement");

    assert!(designer.undo());
    assert!(
        !node(&designer, id).hand_moved,
        "the flag rides inside the move command, so one Ctrl+Z takes back both \
         the position and the claim"
    );
    assert_eq!(node(&designer, id).position, DVec2::ZERO);

    assert!(designer.redo());
    assert!(node(&designer, id).hand_moved);
}

#[test]
fn a_click_without_a_drag_sets_nothing() {
    let mut designer = designer();
    let id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id);
    }
    designer.begin_move_nodes();
    designer.end_move_nodes();

    assert!(!node(&designer, id).hand_moved);
}

#[test]
fn a_full_reflow_is_not_a_hand_placement() {
    // Open question 2 of the design, decided in the "keep the flags" direction:
    // a reflow moves everything, so it claims nothing about intent — and
    // clearing the flags would destroy exactly the record they exist to keep.
    let mut designer = designer();
    let a = designer.add_node("float", DVec2::ZERO);
    let b = designer.add_node("sphere", DVec2::ZERO);
    drag(&mut designer, a, DVec2::new(300.0, 0.0));

    designer.layout_active_network();

    assert!(
        node(&designer, a).hand_moved,
        "the reflow left the flag alone"
    );
    assert!(!node(&designer, b).hand_moved, "and did not invent one");
}

// ============================================================================
// Carrying it
// ============================================================================

#[test]
fn a_duplicate_inherits_the_flag() {
    let mut designer = designer();
    let id = designer.add_node("sphere", DVec2::ZERO);
    drag(&mut designer, id, DVec2::new(40.0, 0.0));

    let copy = designer.duplicate_node(id);
    assert!(
        node(&designer, copy).hand_moved,
        "a copy of a deliberately placed node is itself deliberately placed"
    );
}

#[test]
fn a_pasted_copy_inherits_the_flag() {
    let mut designer = designer();
    let id = designer.add_node("sphere", DVec2::ZERO);
    drag(&mut designer, id, DVec2::new(40.0, 0.0));

    assert!(designer.copy_selection());
    let pasted = designer.paste_at_position(DVec2::new(400.0, 400.0));
    assert_eq!(pasted.len(), 1);
    assert!(
        node(&designer, pasted[0]).hand_moved,
        "the clipboard carries the flag (paste and duplicate are separate \
         `Node` constructions and both have to)"
    );
}

#[test]
fn it_round_trips_through_cnnd_and_an_older_file_loads_without_it() {
    let mut designer = designer();
    let moved = designer.add_node("sphere", DVec2::ZERO);
    let still = designer.add_node("float", DVec2::new(0.0, 200.0));
    drag(&mut designer, moved, DVec2::new(40.0, 0.0));

    let built_ins = designer.node_type_registry.built_in_node_types.clone();
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let serializable = node_network_to_serializable(network, &built_ins, None).unwrap();

    // The flag is skipped when false, so a network nobody has dragged
    // serializes byte-identically to a pre-flag file — no migration, no bump.
    let json = serde_json::to_string(&serializable).unwrap();
    assert_eq!(
        json.matches("hand_moved").count(),
        1,
        "only the dragged node writes the field: {json}"
    );

    let reloaded = serializable_to_node_network(&serializable, &built_ins, None).unwrap();
    assert!(reloaded.nodes[&moved].hand_moved);
    assert!(!reloaded.nodes[&still].hand_moved);

    // And a file that predates the field loads as "not hand-moved".
    let mut stripped: serde_json::Value = serde_json::from_str(&json).unwrap();
    for node in stripped["nodes"].as_array_mut().unwrap() {
        node.as_object_mut().unwrap().remove("hand_moved");
    }
    let older: atomcad_structure_designer::serialization::node_networks_serialization::SerializableNodeNetwork =
        serde_json::from_value(stripped).expect("a pre-flag file still deserializes");
    let older = serializable_to_node_network(&older, &built_ins, None).unwrap();
    assert!(!older.nodes[&moved].hand_moved);
}
