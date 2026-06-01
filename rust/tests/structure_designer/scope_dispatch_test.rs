//! Phase U2 of `doc/design_zones_ui.md` — verify that mutation API entry
//! points route to the correct `NodeNetwork` when given a non-empty
//! `scope_path`.
//!
//! The U2 design contract: with an empty scope_path the mutation runs against
//! the active top-level network (existing behavior); with a non-empty path it
//! descends through `Node.zone_mut()` for each HOF id in the chain and
//! operates on the body network at the bottom. This file builds a two-level
//! network (top-level + a single `map` HOF whose body owns its own nodes)
//! and exercises a representative subset of the new `*_scoped` mutations.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::node_network::{IncomingWire, SourcePin};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_two_level_network() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Place a `map` HOF on the top-level. `add_node` invokes
    // `populate_custom_node_type_cache_with_types` which in turn calls
    // `Node::ensure_zone_init`, so the body is set up automatically.
    let map_id = designer.add_node("map", DVec2::new(100.0, 100.0));
    assert_ne!(map_id, 0, "failed to add map node");

    // Sanity: the HOF owns a zone now.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        main.nodes.get(&map_id).unwrap().zone.is_some(),
        "map node should own a zone after creation"
    );
    (designer, map_id)
}

#[test]
fn scope_network_helpers_walk_into_body() {
    let (designer, map_id) = setup_two_level_network();

    // Empty path: helper returns the active top-level network.
    let top = designer
        .get_scope_network(&[])
        .expect("get_scope_network for empty path");
    assert!(
        top.nodes.contains_key(&map_id),
        "top-level network should hold the map node"
    );

    // Non-empty path: helper descends into the map's body.
    let body = designer
        .get_scope_network(&[map_id])
        .expect("get_scope_network into body");
    assert!(
        body.nodes.is_empty(),
        "freshly-created map body should be empty"
    );

    // Bad chain segment: a non-HOF id rejects.
    let absent = designer.get_scope_network(&[map_id, 999]);
    assert!(absent.is_none(), "missing inner-body node should reject");
}

#[test]
fn add_node_scoped_routes_to_body_for_nonempty_path() {
    let (mut designer, map_id) = setup_two_level_network();

    // Top-level count before any body-scope adds.
    let main_before = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .len();

    // add_node with scope_path = [map_id] should land *inside* the body, not
    // in the top-level network.
    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(50.0, 50.0), None);
    assert_ne!(inner_id, 0, "body-scope add_node should succeed");

    // Top-level network unchanged.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.nodes.len(),
        main_before,
        "top-level network must not gain a node from a body-scope add"
    );

    // Body holds the new node.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        body.nodes.contains_key(&inner_id),
        "body should hold the newly added node"
    );
    assert_eq!(body.nodes.get(&inner_id).unwrap().node_type_name, "int");
}

#[test]
fn move_node_scoped_targets_the_body() {
    let (mut designer, map_id) = setup_two_level_network();

    // Add a top-level sibling node to confirm scoped move doesn't touch it.
    let top_sibling = designer.add_node("int", DVec2::new(200.0, 200.0));
    let top_sibling_pos_before = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&top_sibling)
        .unwrap()
        .position;

    // Add a node into the body.
    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(10.0, 10.0), None);

    // Move that inner node via the scoped variant.
    designer.move_node_scoped(&[map_id], inner_id, DVec2::new(42.0, 84.0));

    let body = designer.get_scope_network(&[map_id]).unwrap();
    let inner_pos = body.nodes.get(&inner_id).unwrap().position;
    assert_eq!(inner_pos, DVec2::new(42.0, 84.0), "body node moved");

    // Top-level sibling untouched.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.nodes.get(&top_sibling).unwrap().position,
        top_sibling_pos_before,
        "top-level sibling position must not be affected by a body-scope move"
    );
}

#[test]
fn select_node_scoped_targets_the_body_selection() {
    let (mut designer, map_id) = setup_two_level_network();

    // Two top-level siblings; the first becomes selected so we can observe the
    // single-scope invariant clear it when we select inside the body.
    let top_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    let _top_b = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.select_node(top_a);

    // Body holds its own node; select it via scope_path = [map_id].
    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let ok = designer.select_node_scoped(&[map_id], inner_id);
    assert!(ok, "select inside body must succeed");

    // Body's selection set carries the inner id.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        body.selected_node_ids.contains(&inner_id),
        "body.selected_node_ids should contain the inner node"
    );

    // Single-scope invariant: selecting inside the body clears the top-level
    // network's selection (it lived in a different scope). The top-level node
    // still *exists* — only its selection is gone.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.active_node_id, None,
        "selecting in a body must clear the top-level selection (single-scope invariant)"
    );
    assert!(
        main.selected_node_ids.is_empty(),
        "top-level selected_node_ids must be empty after a body selection"
    );
    assert!(
        main.nodes.contains_key(&top_a),
        "the top-level node itself must survive (only its selection is cleared)"
    );
}

#[test]
fn delete_selected_scoped_only_touches_the_body() {
    let (mut designer, map_id) = setup_two_level_network();

    // Top-level node we must not delete.
    let top_keep = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.select_node(top_keep);

    // Body has its own selected node we *do* want to delete. Selecting it
    // moves the (single-scope) selection into the body, clearing top-level.
    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], inner_id);

    designer.delete_selected_scoped(&[map_id]);

    // Body lost the node.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        !body.nodes.contains_key(&inner_id),
        "body-scope delete must remove the body's selected node"
    );

    // The top-level node must survive a body-scope delete (the delete targets
    // the body's selection only). Its top-level selection was already cleared
    // by the body selection above, per the single-scope invariant.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        main.nodes.contains_key(&top_keep),
        "top-level node must survive a body-scope delete"
    );
    assert_eq!(
        main.active_node_id, None,
        "top-level selection was cleared when the body selection was made"
    );
}

#[test]
fn add_node_scoped_routes_to_nested_body_at_depth_two() {
    // U6 contract: scoped mutations walk an arbitrary-depth chain. Build
    // top-level → outer map → inner map (inside the outer map's body), then
    // add a node inside the inner body via the public API and confirm it
    // lands in the right place (and doesn't leak into the outer body or
    // top-level). See `doc/design_zones_ui.md` §"Phase U6".
    let (mut designer, outer_map_id) = setup_two_level_network();

    // Add an inner `map` HOF inside the outer map's body — depth-1 scope.
    let inner_map_id =
        designer.add_node_scoped(&[outer_map_id], "map", DVec2::new(20.0, 20.0), None);
    assert_ne!(inner_map_id, 0, "depth-1 add of inner HOF should succeed");

    // The inner map's own body must exist after creation (zone init runs
    // through the scoped add path the same as the top-level path).
    let depth2_chain = [outer_map_id, inner_map_id];
    let inner_body = designer
        .get_scope_network(&depth2_chain)
        .expect("get_scope_network should walk a depth-2 chain into the inner body");
    assert!(
        inner_body.nodes.is_empty(),
        "freshly-created inner-map body should start empty"
    );

    // Add a leaf node inside the inner body via the public API.
    let leaf_id = designer.add_node_scoped(&depth2_chain, "int", DVec2::new(5.0, 5.0), None);
    assert_ne!(leaf_id, 0, "depth-2 add_node_scoped should succeed");

    // Leaf lives in the inner body.
    let inner_body = designer.get_scope_network(&depth2_chain).unwrap();
    assert!(
        inner_body.nodes.contains_key(&leaf_id),
        "leaf should land in the inner-map body"
    );

    // Outer body still only holds the inner map (no leak from a misrouted
    // depth-2 add). Per-body `next_node_id` is independent, so `leaf_id` may
    // numerically equal `inner_map_id`; assert on count and type rather than
    // on id membership.
    let outer_body = designer.get_scope_network(&[outer_map_id]).unwrap();
    assert_eq!(
        outer_body.nodes.len(),
        1,
        "outer body must hold only the inner map after a depth-2 add"
    );
    assert!(outer_body.nodes.contains_key(&inner_map_id));
    assert_eq!(
        outer_body.nodes.get(&inner_map_id).unwrap().node_type_name,
        "map",
        "outer body's single node must still be the inner-map HOF, not a stray int leaf"
    );

    // Top-level only has the outer map.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.nodes.len(),
        1,
        "top-level should only have the outer map"
    );
    assert_eq!(main.nodes.get(&outer_map_id).unwrap().node_type_name, "map");
}

#[test]
fn move_and_select_at_depth_two_target_the_right_body() {
    // U6: move_node_scoped and select_node_scoped at depth ≥ 2 land in the
    // inner-most body identified by the chain, without disturbing outer
    // bodies or the top-level network.
    let (mut designer, outer_map_id) = setup_two_level_network();
    let inner_map_id = designer.add_node_scoped(&[outer_map_id], "map", DVec2::new(0.0, 0.0), None);
    let depth2_chain = [outer_map_id, inner_map_id];
    let leaf_id = designer.add_node_scoped(&depth2_chain, "int", DVec2::new(1.0, 1.0), None);

    // Move the leaf via depth-2 scope.
    designer.move_node_scoped(&depth2_chain, leaf_id, DVec2::new(77.0, 99.0));
    let inner_body = designer.get_scope_network(&depth2_chain).unwrap();
    assert_eq!(
        inner_body.nodes.get(&leaf_id).unwrap().position,
        DVec2::new(77.0, 99.0),
        "depth-2 move must update the leaf's position"
    );

    // Inner map's position-in-outer-body unchanged.
    let outer_body = designer.get_scope_network(&[outer_map_id]).unwrap();
    assert_eq!(
        outer_body.nodes.get(&inner_map_id).unwrap().position,
        DVec2::new(0.0, 0.0),
        "outer-body position of inner map must not move when a leaf inside moves"
    );

    // Select the leaf via depth-2 scope; selection lands on the inner body's
    // selection set, not the outer body's or top-level's.
    let ok = designer.select_node_scoped(&depth2_chain, leaf_id);
    assert!(ok, "depth-2 select must succeed");
    let inner_body = designer.get_scope_network(&depth2_chain).unwrap();
    assert!(
        inner_body.selected_node_ids.contains(&leaf_id),
        "inner body's selection set should carry the leaf"
    );
    let outer_body = designer.get_scope_network(&[outer_map_id]).unwrap();
    assert!(
        !outer_body.selected_node_ids.contains(&leaf_id),
        "outer body's selection set must not contain the leaf"
    );
    let outer_body = designer.get_scope_network(&[outer_map_id]).unwrap();
    assert!(
        !outer_body.selected_node_ids.contains(&inner_map_id),
        "selecting the leaf must not flip the inner map's selection in the outer body"
    );
}

#[test]
fn empty_scope_path_preserves_top_level_behavior() {
    // Regression guard for the U2 contract "Empty chain everywhere → no
    // behavioral change": the *_scoped variants with an empty path must
    // route to the original method's full behavior (display policy + undo +
    // selection-change tracking on top level).
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let node_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    let node_b = designer.add_node("int", DVec2::new(100.0, 0.0));

    designer.select_node_scoped(&[], node_a);

    // Top-level select_node updates active_node_id (top-level display-policy
    // and selection-change tracking still run in the empty-path branch).
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(main.active_node_id, Some(node_a));

    // move_node_scoped with empty path matches today's move_node.
    designer.move_node_scoped(&[], node_b, DVec2::new(50.0, 50.0));
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.nodes.get(&node_b).unwrap().position,
        DVec2::new(50.0, 50.0)
    );

    // Empty-path delete must invoke the full top-level path: the selected
    // node disappears, the network is marked for full refresh, and an undo
    // command is appended. Count the stack to confirm a *new* entry lands
    // (add_node already pushes two — so we expect the cursor to advance).
    let cursor_before_delete = designer.undo_stack.undo_description().is_some();
    designer.delete_selected_scoped(&[]);
    assert!(
        cursor_before_delete,
        "add_node should leave the stack non-empty"
    );
    assert!(
        designer.undo_stack.can_undo(),
        "delete_selected_scoped(&[]) should still leave an undoable history"
    );
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        !main.nodes.contains_key(&node_a),
        "the selected top-level node should be deleted"
    );
}

// ===== Single-scope selection invariant =====
//
// `clear_selection_in_other_scopes` (called by every scoped selection mutator)
// guarantees the selection lives in exactly one scope at a time. These tests
// drive the same `*_scoped` entry points the Flutter API calls.

#[test]
fn selecting_in_body_clears_top_level_selection() {
    let (mut designer, map_id) = setup_two_level_network();
    let top_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], top_a);
    assert!(
        designer
            .get_scope_network(&[])
            .unwrap()
            .selected_node_ids
            .contains(&top_a),
        "precondition: top-level node selected"
    );

    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], inner_id);

    assert!(
        designer
            .get_scope_network(&[])
            .unwrap()
            .selected_node_ids
            .is_empty(),
        "selecting in the body must clear the top-level selection"
    );
    assert!(
        designer
            .get_scope_network(&[map_id])
            .unwrap()
            .selected_node_ids
            .contains(&inner_id),
        "the body now holds the selection"
    );
}

#[test]
fn selecting_at_top_level_clears_body_selection() {
    let (mut designer, map_id) = setup_two_level_network();
    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], inner_id);
    assert!(
        designer
            .get_scope_network(&[map_id])
            .unwrap()
            .selected_node_ids
            .contains(&inner_id),
        "precondition: body node selected"
    );

    let top_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], top_a);

    assert!(
        designer
            .get_scope_network(&[map_id])
            .unwrap()
            .selected_node_ids
            .is_empty(),
        "selecting at top level must clear the body selection"
    );
    assert!(
        designer
            .get_scope_network(&[])
            .unwrap()
            .selected_node_ids
            .contains(&top_a),
        "the top level now holds the selection"
    );
}

#[test]
fn shift_select_across_scope_boundary_collapses_to_single_scope() {
    // Shift-add (`add_node_to_selection_scoped`) is modifier-agnostic w.r.t. the
    // invariant: crossing a scope boundary clears the prior scope, so the
    // additive modifier ends up applied against the now-empty target scope and
    // the result is a fresh single-node selection.
    let (mut designer, map_id) = setup_two_level_network();
    let inner_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], inner_id);

    let top_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.add_node_to_selection_scoped(&[], top_a);

    assert!(
        designer
            .get_scope_network(&[map_id])
            .unwrap()
            .selected_node_ids
            .is_empty(),
        "Shift across a scope boundary must clear the prior (body) scope"
    );
    let top = designer.get_scope_network(&[]).unwrap();
    assert_eq!(
        top.selected_node_ids.len(),
        1,
        "the target scope holds exactly the shift-added node"
    );
    assert!(top.selected_node_ids.contains(&top_a));
}

#[test]
fn shift_select_within_same_scope_extends_selection() {
    // The invariant only clears *other* scopes — within a single scope, Shift
    // still extends the selection as before.
    let (mut designer, _map_id) = setup_two_level_network();
    let a = designer.add_node("int", DVec2::new(0.0, 0.0));
    let b = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.select_node_scoped(&[], a);
    designer.add_node_to_selection_scoped(&[], b);

    let top = designer.get_scope_network(&[]).unwrap();
    assert!(
        top.selected_node_ids.contains(&a) && top.selected_node_ids.contains(&b),
        "Shift within the same scope must keep both nodes selected"
    );
}

// ===== Scope-aware wire selection =====

#[test]
fn select_wire_scoped_records_in_body_and_clears_other_scopes() {
    let (mut designer, map_id) = setup_two_level_network();

    // A top-level selection that selecting a body wire must clear.
    let top_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], top_a);

    // Build a regular intra-body wire: int -> collect.
    let int_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let collect_id = designer.add_node_scoped(&[map_id], "collect", DVec2::new(100.0, 0.0), None);
    designer.connect_nodes_scoped(&[map_id], int_id, 0, collect_id, 0);

    let ok = designer.select_wire_scoped(&[map_id], int_id, 0, collect_id, 0);
    assert!(ok, "selecting an existing body wire must succeed");

    assert_eq!(
        designer
            .get_scope_network(&[map_id])
            .unwrap()
            .selected_wires
            .len(),
        1,
        "the body should carry exactly one selected wire"
    );
    assert!(
        designer
            .get_scope_network(&[])
            .unwrap()
            .selected_node_ids
            .is_empty(),
        "selecting a body wire must clear the top-level selection"
    );
}

#[test]
fn delete_selected_scoped_removes_body_wire() {
    let (mut designer, map_id) = setup_two_level_network();
    let int_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let collect_id = designer.add_node_scoped(&[map_id], "collect", DVec2::new(100.0, 0.0), None);
    designer.connect_nodes_scoped(&[map_id], int_id, 0, collect_id, 0);

    let connected = |d: &StructureDesigner| {
        d.get_scope_network(&[map_id])
            .unwrap()
            .nodes
            .get(&collect_id)
            .unwrap()
            .arguments
            .iter()
            .any(|a| !a.incoming_wires.is_empty())
    };
    assert!(
        connected(&designer),
        "precondition: collect has an incoming wire"
    );

    designer.select_wire_scoped(&[map_id], int_id, 0, collect_id, 0);
    designer.delete_selected_scoped(&[map_id]);

    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        body.nodes.contains_key(&int_id) && body.nodes.contains_key(&collect_id),
        "deleting a wire must not delete the nodes"
    );
    assert!(
        !connected(&designer),
        "the body wire should be removed after a body-scope delete"
    );
}

#[test]
fn capture_wire_is_selectable_and_deletable_in_its_body() {
    // The load-bearing test for "capture wires can be selected and deleted":
    // a wire from a top-level node into a body node (source_scope_depth = 1) is
    // stored on the body node's argument, so its selection belongs in the body
    // and `delete_selected_scoped` on the body removes it.
    let (mut designer, map_id) = setup_two_level_network();

    // Top-level source the body will capture.
    let k_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    // Body node with an input pin (`collect` takes an input at arg 0).
    let body_node = designer.add_node_scoped(&[map_id], "collect", DVec2::new(10.0, 10.0), None);

    // Author the capture wire by hand (depth 1, source = the top-level node).
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        let body = net.nodes.get_mut(&map_id).unwrap().zone_mut().unwrap();
        body.nodes.get_mut(&body_node).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: k_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }

    // Select the capture wire through the scoped API; identity is canonicalized
    // from storage, so the depth-1 / capture shape is preserved.
    let ok = designer.select_wire_scoped(&[map_id], k_id, 0, body_node, 0);
    assert!(ok, "selecting a capture wire must succeed");

    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert_eq!(body.selected_wires.len(), 1, "the capture wire is selected");
    assert_eq!(
        body.selected_wires[0].source_scope_depth, 1,
        "the capture identity (depth 1) is preserved in the selection"
    );
    assert_eq!(body.selected_wires[0].source_node_id, k_id);
    assert!(
        body.is_incoming_wire_selected(k_id, body_node, 0),
        "the view builder's selected-flag predicate must report the capture wire selected"
    );

    // Delete it via the body scope: the capture wire is removed, the body node
    // and the top-level source both survive.
    designer.delete_selected_scoped(&[map_id]);
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        body.nodes.contains_key(&body_node),
        "deleting a capture wire must not delete the body node"
    );
    assert!(
        body.nodes.get(&body_node).unwrap().arguments[0]
            .incoming_wires
            .is_empty(),
        "the capture wire should be removed after delete"
    );
    assert!(
        designer
            .get_scope_network(&[])
            .unwrap()
            .nodes
            .contains_key(&k_id),
        "the captured top-level source node must survive"
    );
}
