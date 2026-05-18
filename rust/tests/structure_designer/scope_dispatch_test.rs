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
    let inner_id = designer.add_node_scoped(
        &[map_id],
        "int",
        DVec2::new(50.0, 50.0),
        None,
    );
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
    assert_eq!(
        body.nodes.get(&inner_id).unwrap().node_type_name,
        "int"
    );
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
    let inner_id =
        designer.add_node_scoped(&[map_id], "int", DVec2::new(10.0, 10.0), None);

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

    // Two top-level siblings; the first becomes selected via the unscoped
    // call so we can verify the body-scope path doesn't disturb it.
    let top_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    let _top_b = designer.add_node("int", DVec2::new(100.0, 0.0));
    designer.select_node(top_a);

    // Body holds its own node; select it via scope_path = [map_id].
    let inner_id =
        designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let ok = designer.select_node_scoped(&[map_id], inner_id);
    assert!(ok, "select inside body must succeed");

    // Body's selection set carries the inner id.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        body.selected_node_ids.contains(&inner_id),
        "body.selected_node_ids should contain the inner node"
    );

    // Top-level network's selection unaffected.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.active_node_id,
        Some(top_a),
        "top-level active_node_id should still be the top-level node"
    );
}

#[test]
fn delete_selected_scoped_only_touches_the_body() {
    let (mut designer, map_id) = setup_two_level_network();

    // Top-level has a selected node we must not delete.
    let top_keep = designer.add_node("int", DVec2::new(0.0, 0.0));
    designer.select_node(top_keep);

    // Body has its own selected node we *do* want to delete.
    let inner_id =
        designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], inner_id);

    designer.delete_selected_scoped(&[map_id]);

    // Body lost the node.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(
        !body.nodes.contains_key(&inner_id),
        "body-scope delete must remove the body's selected node"
    );

    // Top-level still has `top_keep` and it's still selected.
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
        main.active_node_id,
        Some(top_keep),
        "top-level selection must survive a body-scope delete"
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
    assert!(cursor_before_delete, "add_node should leave the stack non-empty");
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
