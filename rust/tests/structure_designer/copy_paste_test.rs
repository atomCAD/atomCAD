use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

// ===== COPY SINGLE NODE =====

#[test]
fn test_copy_single_node_and_paste() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(100.0, 200.0));

    // Select and copy
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_node(float_id);
    }
    assert!(designer.copy_selection());

    // Paste at a new position
    let new_ids = designer.paste_at_position(DVec2::new(300.0, 400.0));
    assert_eq!(new_ids.len(), 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Original still exists
    assert!(network.nodes.contains_key(&float_id));

    // New node exists with correct type
    let new_node = network.nodes.get(&new_ids[0]).unwrap();
    assert_eq!(new_node.node_type_name, "float");

    // New node should be at paste position (clipboard is centered at origin)
    assert_eq!(new_node.position, DVec2::new(300.0, 400.0));

    // New node should have a unique display name
    let orig_node = network.nodes.get(&float_id).unwrap();
    assert_ne!(new_node.custom_name, orig_node.custom_name);

    // New node should be displayed
    assert!(network.is_node_displayed(new_ids[0]));

    // New node should be selected
    assert!(network.is_node_selected(new_ids[0]));
}

// ===== COPY MULTIPLE CONNECTED NODES =====

#[test]
fn test_copy_connected_nodes_preserves_internal_wires() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);

    // Select both nodes
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_nodes(vec![float_id, sphere_id]);
    }
    assert!(designer.copy_selection());

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 300.0));
    assert_eq!(new_ids.len(), 2);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Find the pasted sphere (the one with node_type_name "sphere" among new_ids)
    let pasted_sphere_id = new_ids
        .iter()
        .find(|&&id| network.nodes.get(&id).unwrap().node_type_name == "sphere")
        .unwrap();
    let pasted_float_id = new_ids
        .iter()
        .find(|&&id| network.nodes.get(&id).unwrap().node_type_name == "float")
        .unwrap();

    // The pasted sphere should be wired to the pasted float (internal wire preserved)
    let pasted_sphere = network.nodes.get(pasted_sphere_id).unwrap();
    assert_eq!(
        pasted_sphere.arguments[0].get_node_id(),
        Some(*pasted_float_id)
    );

    // The wire should NOT point to the original float
    assert_ne!(pasted_sphere.arguments[0].get_node_id(), Some(float_id));
}

#[test]
fn test_copy_drops_external_wires() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);

    // Select only the sphere (not the float it's connected to)
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_node(sphere_id);
    }
    assert!(designer.copy_selection());

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 300.0));
    assert_eq!(new_ids.len(), 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let pasted_sphere = network.nodes.get(&new_ids[0]).unwrap();

    // The external wire to the original float should be dropped
    assert!(pasted_sphere.arguments[0].is_empty());
}

// ===== EMPTY SELECTION =====

#[test]
fn test_copy_empty_selection_returns_false() {
    let mut designer = setup_designer_with_network("test_network");
    designer.add_node("float", DVec2::new(0.0, 0.0));

    // No selection — copy should fail
    assert!(!designer.copy_selection());
    assert!(!designer.has_clipboard_content());
}

// ===== EMPTY CLIPBOARD =====

#[test]
fn test_paste_empty_clipboard_returns_empty() {
    let mut designer = setup_designer_with_network("test_network");

    // No copy performed — clipboard is None
    assert!(!designer.has_clipboard_content());
    let new_ids = designer.paste_at_position(DVec2::new(0.0, 0.0));
    assert!(new_ids.is_empty());
}

// ===== REPEATED PASTE =====

#[test]
fn test_repeated_paste_creates_fresh_ids() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(100.0, 100.0));

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();

    let ids1 = designer.paste_at_position(DVec2::new(200.0, 200.0));
    let ids2 = designer.paste_at_position(DVec2::new(300.0, 300.0));

    assert_eq!(ids1.len(), 1);
    assert_eq!(ids2.len(), 1);

    // Each paste should produce different IDs
    assert_ne!(ids1[0], ids2[0]);
    assert_ne!(ids1[0], float_id);
    assert_ne!(ids2[0], float_id);

    // All three nodes exist
    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(network.nodes.contains_key(&float_id));
    assert!(network.nodes.contains_key(&ids1[0]));
    assert!(network.nodes.contains_key(&ids2[0]));
}

#[test]
fn test_repeated_paste_unique_display_names() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();

    let ids1 = designer.paste_at_position(DVec2::new(100.0, 0.0));
    let ids2 = designer.paste_at_position(DVec2::new(200.0, 0.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let name0 = &network.nodes.get(&float_id).unwrap().custom_name;
    let name1 = &network.nodes.get(&ids1[0]).unwrap().custom_name;
    let name2 = &network.nodes.get(&ids2[0]).unwrap().custom_name;

    // All names should be unique
    assert_ne!(name0, name1);
    assert_ne!(name0, name2);
    assert_ne!(name1, name2);
}

// ===== CUT =====

#[test]
fn test_cut_removes_original_and_fills_clipboard() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(100.0, 100.0));

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_node(float_id);
    }
    assert!(designer.cut_selection());
    assert!(designer.has_clipboard_content());

    // Original node should be deleted
    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    assert!(!network.nodes.contains_key(&float_id));

    // Paste should work from the clipboard
    let new_ids = designer.paste_at_position(DVec2::new(200.0, 200.0));
    assert_eq!(new_ids.len(), 1);
}

#[test]
fn test_cut_empty_selection_returns_false() {
    let mut designer = setup_designer_with_network("test_network");
    designer.add_node("float", DVec2::new(0.0, 0.0));

    // No selection — cut should fail
    assert!(!designer.cut_selection());
}

// ===== CROSS-NETWORK PASTE =====

#[test]
fn test_paste_into_different_network() {
    let mut designer = setup_designer_with_network("network_a");

    let float_id = designer.add_node("float", DVec2::new(100.0, 100.0));

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("network_a")
            .unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();

    // Switch to a different network
    designer.add_node_network("network_b");
    designer.set_active_node_network_name(Some("network_b".to_string()));

    let new_ids = designer.paste_at_position(DVec2::new(50.0, 50.0));
    assert_eq!(new_ids.len(), 1);

    // Verify the pasted node is in network_b
    let network_b = designer
        .node_type_registry
        .node_networks
        .get("network_b")
        .unwrap();
    let pasted = network_b.nodes.get(&new_ids[0]).unwrap();
    assert_eq!(pasted.node_type_name, "float");
    assert_eq!(pasted.position, DVec2::new(50.0, 50.0));

    // Original still in network_a
    let network_a = designer
        .node_type_registry
        .node_networks
        .get("network_a")
        .unwrap();
    assert!(network_a.nodes.contains_key(&float_id));
}

// ===== CLIPBOARD SURVIVES RENAME =====

#[test]
fn test_clipboard_survives_network_rename() {
    let mut designer = setup_designer_with_network("main");

    // Create a second network and add a node of that custom type to main
    designer.add_node_network("helper");

    // Copy a node from main (any built-in type)
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();
    assert!(designer.has_clipboard_content());

    // Rename a network — clipboard should survive since it has no nodes of that type
    designer.rename_node_network("helper", "helper_renamed");
    assert!(designer.has_clipboard_content());
}

#[test]
fn test_clipboard_updates_node_type_name_on_rename() {
    let mut designer = setup_designer_with_network("main");

    // Create a custom network "helper" with a return node so it can be used as a node type
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("helper")
            .unwrap();
        network.set_return_node(sphere_id);
    }
    designer.validate_active_network();

    // Switch to main and add a node of type "helper"
    designer.set_active_node_network_name(Some("main".to_string()));
    let helper_node_id = designer.add_node("helper", DVec2::new(0.0, 0.0));

    // Select and copy the helper node
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_node(helper_node_id);
    }
    designer.copy_selection();
    assert!(designer.has_clipboard_content());

    // Rename the helper network
    designer.rename_node_network("helper", "helper_v2");

    // Clipboard should still exist and its node_type_name should be updated
    assert!(designer.has_clipboard_content());
    let clipboard = designer.clipboard.as_ref().unwrap();
    let clipboard_node = clipboard.nodes.values().next().unwrap();
    assert_eq!(clipboard_node.node_type_name, "helper_v2");
}

// ===== CLIPBOARD RENAME CASCADE DESCENDS INTO ZONE BODIES =====
//
// Sibling of issue #331's body-skip: when an HOF/closure node is on the
// clipboard and its inline body references a custom network, renaming that
// network must update the body instance's `node_type_name` too. The clipboard
// fixup loops in `rename_node_network` / `rename_namespace` walked only the
// top-level `clipboard.nodes`, so a copied-then-renamed-then-pasted body kept a
// stale/dangling reference. (The authoritative registry-side rename,
// `apply_rename_core`, already recurses via `walk_all_nodes_mut`.)

/// Read the `node_type_name` of the single instance node living inside the
/// clipboard HOF node's zone body. Panics if the clipboard isn't a single HOF
/// with a one-node body (the exact shape these tests build).
fn clipboard_body_instance_type_name(designer: &StructureDesigner) -> String {
    let clipboard = designer.clipboard.as_ref().expect("clipboard present");
    let hof = clipboard
        .nodes
        .values()
        .find(|n| n.zone.is_some())
        .expect("clipboard should hold an HOF node with a body");
    let body = hof.zone.as_ref().unwrap();
    body.nodes
        .values()
        .next()
        .expect("HOF body should hold the instance node")
        .node_type_name
        .clone()
}

#[test]
fn test_clipboard_updates_body_node_type_name_on_rename() {
    let mut designer = setup_designer_with_network("main");

    // Custom network "helper" usable as a node type.
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("helper")
            .unwrap();
        network.set_return_node(sphere_id);
    }
    designer.validate_active_network();

    // main: a `map` HOF whose BODY holds a "helper" instance.
    designer.set_active_node_network_name(Some("main".to_string()));
    let map_id = designer.add_node("map", DVec2::ZERO);
    designer.add_node_scoped(&[map_id], "helper", DVec2::ZERO, None);

    // Select & copy the map node — its body travels with it onto the clipboard.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_node(map_id);
    }
    assert!(designer.copy_selection());
    assert_eq!(
        clipboard_body_instance_type_name(&designer),
        "helper",
        "precondition: clipboard body instance is 'helper' before rename"
    );

    designer.rename_node_network("helper", "helper_v2");

    assert_eq!(
        clipboard_body_instance_type_name(&designer),
        "helper_v2",
        "clipboard rename cascade must descend into HOF zone bodies"
    );
}

#[test]
fn test_clipboard_updates_body_node_type_name_on_namespace_rename() {
    let mut designer = setup_designer_with_network("main");

    // Custom network "ns.helper" usable as a node type.
    designer.add_node_network("ns.helper");
    designer.set_active_node_network_name(Some("ns.helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("ns.helper")
            .unwrap();
        network.set_return_node(sphere_id);
    }
    designer.validate_active_network();

    // main: a `map` HOF whose BODY holds an "ns.helper" instance.
    designer.set_active_node_network_name(Some("main".to_string()));
    let map_id = designer.add_node("map", DVec2::ZERO);
    designer.add_node_scoped(&[map_id], "ns.helper", DVec2::ZERO, None);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_node(map_id);
    }
    assert!(designer.copy_selection());
    assert_eq!(
        clipboard_body_instance_type_name(&designer),
        "ns.helper",
        "precondition: clipboard body instance is 'ns.helper' before rename"
    );

    // Bulk namespace rename: ns.* -> ns2.*
    designer.rename_namespace("ns", "ns2");

    assert_eq!(
        clipboard_body_instance_type_name(&designer),
        "ns2.helper",
        "namespace rename cascade must descend into HOF zone bodies"
    );
}

// ===== CLIPBOARD INVALIDATION ON DELETE =====

#[test]
fn test_clipboard_cleared_on_referenced_network_delete() {
    let mut designer = setup_designer_with_network("main");

    // Create a custom network "helper" with a return node
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("helper")
            .unwrap();
        network.set_return_node(sphere_id);
    }
    designer.validate_active_network();

    // Switch to main and add a node of type "helper"
    designer.set_active_node_network_name(Some("main".to_string()));
    let helper_node_id = designer.add_node("helper", DVec2::new(0.0, 0.0));

    // Copy the helper node
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_node(helper_node_id);
    }
    designer.copy_selection();
    assert!(designer.has_clipboard_content());

    // Delete the helper node from main first (so "helper" network is not referenced)
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        network.select_node(helper_node_id);
    }
    designer.delete_selected();

    // Now delete the helper network
    assert!(designer.delete_node_network("helper").is_ok());

    // Clipboard should be cleared because it references the deleted "helper" type
    assert!(!designer.has_clipboard_content());
}

// ===== RETURN NODE NOT PRESERVED ON PASTE =====

#[test]
fn test_pasted_nodes_are_not_return_node() {
    let mut designer = setup_designer_with_network("test_network");

    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));

    // Set as return node
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.set_return_node(sphere_id);
        network.select_node(sphere_id);
    }

    designer.copy_selection();
    let new_ids = designer.paste_at_position(DVec2::new(200.0, 200.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // The return node should still be the original, NOT the pasted one
    assert_eq!(network.return_node_id, Some(sphere_id));
    assert_ne!(network.return_node_id, Some(new_ids[0]));
}

// ===== POSITION CENTERING =====

#[test]
fn test_copy_centers_clipboard_at_origin() {
    let mut designer = setup_designer_with_network("test_network");

    // Two nodes at (100, 100) and (300, 100) — centroid is (200, 100)
    let id1 = designer.add_node("float", DVec2::new(100.0, 100.0));
    let id2 = designer.add_node("float", DVec2::new(300.0, 100.0));

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_nodes(vec![id1, id2]);
    }
    designer.copy_selection();

    // Paste at (500, 500) — nodes should be at (400, 500) and (600, 500)
    let new_ids = designer.paste_at_position(DVec2::new(500.0, 500.0));
    assert_eq!(new_ids.len(), 2);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let mut positions: Vec<DVec2> = new_ids
        .iter()
        .map(|id| network.nodes.get(id).unwrap().position)
        .collect();
    positions.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

    assert_eq!(positions[0], DVec2::new(400.0, 500.0));
    assert_eq!(positions[1], DVec2::new(600.0, 500.0));
}

// ===== HAS CLIPBOARD CONTENT =====

#[test]
fn test_has_clipboard_content() {
    let mut designer = setup_designer_with_network("test_network");

    assert!(!designer.has_clipboard_content());

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();
    assert!(designer.has_clipboard_content());
}

// ===== PASTED NODES ARE SELECTED =====

#[test]
fn test_pasted_nodes_become_selected() {
    let mut designer = setup_designer_with_network("test_network");

    let id1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let id2 = designer.add_node("float", DVec2::new(100.0, 0.0));

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.select_nodes(vec![id1, id2]);
    }
    designer.copy_selection();

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 200.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Pasted nodes should be selected
    for &new_id in &new_ids {
        assert!(network.is_node_selected(new_id));
    }

    // Original nodes should NOT be selected (select_nodes clears previous selection)
    assert!(!network.is_node_selected(id1));
    assert!(!network.is_node_selected(id2));
}

// ===== ZONE (HOF body) COPY / PASTE / CUT =====
//
// Copy/cut operate on the selection wherever it lives (single-scope
// invariant), and paste targets the scope it is given. These exercise the
// scoped clipboard paths against a `map` HOF body. See
// `doc/design_zones_ui.md`.

/// main + a top-level `map` HOF (whose body is auto-initialized). Returns the
/// map node id so tests can address its body via scope_path `[map_id]`.
fn setup_with_map() -> (StructureDesigner, u64) {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(100.0, 100.0));
    assert_ne!(map_id, 0, "failed to add map node");
    (designer, map_id)
}

#[test]
fn test_find_selection_scope_locates_body_and_top_level() {
    let (mut designer, map_id) = setup_with_map();

    // Nothing selected anywhere.
    assert_eq!(designer.find_selection_scope(), None);

    // Top-level selection → empty scope path.
    let top = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], top);
    assert_eq!(designer.find_selection_scope(), Some(vec![]));

    // Body selection → [map_id]. Selecting in the body clears top-level
    // (single-scope invariant), so the located scope is unambiguous.
    let inner = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], inner);
    assert_eq!(designer.find_selection_scope(), Some(vec![map_id]));
}

#[test]
fn test_copy_body_selection_and_paste_into_same_body() {
    let (mut designer, map_id) = setup_with_map();

    // Two connected nodes inside the body: int -> collect.
    let int_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let collect_id = designer.add_node_scoped(&[map_id], "collect", DVec2::new(200.0, 0.0), None);
    designer.connect_nodes_scoped(&[map_id], int_id, 0, collect_id, 0);

    // Select both body nodes and copy. copy_selection ignores the active
    // scope and locates the selection itself.
    designer.select_nodes_scoped(&[map_id], vec![int_id, collect_id]);
    assert!(designer.copy_selection());

    // Paste into the same body.
    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(0.0, 300.0));
    assert_eq!(new_ids.len(), 2);

    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert_eq!(body.nodes.len(), 4, "body now holds originals + pastes");

    // Internal wire preserved among the pasted pair (remapped to the new int,
    // not the original).
    let pasted_collect = new_ids
        .iter()
        .find(|&&id| body.nodes.get(&id).unwrap().node_type_name == "collect")
        .unwrap();
    let pasted_int = new_ids
        .iter()
        .find(|&&id| body.nodes.get(&id).unwrap().node_type_name == "int")
        .unwrap();
    assert_eq!(
        body.nodes.get(pasted_collect).unwrap().arguments[0].get_node_id(),
        Some(*pasted_int),
        "pasted collect should wire to the pasted int"
    );

    // Pasted nodes are selected in the body.
    assert!(body.is_node_selected(*pasted_collect));
    assert!(body.is_node_selected(*pasted_int));

    // Top-level untouched (still just the map node).
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(main.nodes.len(), 1);
}

#[test]
fn test_copy_top_level_and_paste_into_body() {
    let (mut designer, map_id) = setup_with_map();

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], float_id);
    assert!(designer.copy_selection());

    // Paste into the map's body.
    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(10.0, 10.0));
    assert_eq!(new_ids.len(), 1);

    // The pasted node lives in the body.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(body.nodes.contains_key(&new_ids[0]));
    assert_eq!(body.nodes.get(&new_ids[0]).unwrap().node_type_name, "float");

    // Single-scope invariant: selecting the pasted body nodes cleared the
    // top-level selection of the source float.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(main.selected_node_ids.is_empty());
    // Original float still present at top level (copy, not cut).
    assert!(main.nodes.contains_key(&float_id));
}

#[test]
fn test_copy_body_node_drops_cross_scope_capture_on_paste() {
    use rust_lib_flutter_cad::structure_designer::node_network::{IncomingWire, SourcePin};

    let (mut designer, map_id) = setup_with_map();

    // Top-level source captured by a body node.
    let k_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let body_node = designer.add_node_scoped(&[map_id], "collect", DVec2::new(10.0, 10.0), None);

    // Author the capture wire by hand (depth 1 into the body node).
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

    // Copy just the body node, then paste it at the top level.
    designer.select_node_scoped(&[map_id], body_node);
    assert!(designer.copy_selection());
    let new_ids = designer.paste_at_position_scoped(&[], DVec2::new(0.0, 300.0));
    assert_eq!(new_ids.len(), 1);

    // The cross-scope capture is dropped — the pasted node has no incoming wire.
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert!(
        main.nodes.get(&new_ids[0]).unwrap().arguments[0].is_empty(),
        "cross-scope capture must be dropped on paste"
    );
}

#[test]
fn test_cut_body_selection_removes_from_body() {
    let (mut designer, map_id) = setup_with_map();

    let int_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let collect_id = designer.add_node_scoped(&[map_id], "collect", DVec2::new(200.0, 0.0), None);
    designer.select_nodes_scoped(&[map_id], vec![int_id, collect_id]);

    assert!(designer.cut_selection());
    assert!(designer.has_clipboard_content());

    // Both body nodes are gone.
    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert!(!body.nodes.contains_key(&int_id));
    assert!(!body.nodes.contains_key(&collect_id));

    // Paste back into the body restores two nodes.
    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(0.0, 0.0));
    assert_eq!(new_ids.len(), 2);
}

#[test]
fn test_scoped_paste_undo_redo_round_trip() {
    let (mut designer, map_id) = setup_with_map();

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], float_id);
    designer.copy_selection();

    let body_count = |d: &StructureDesigner| d.get_scope_network(&[map_id]).unwrap().nodes.len();
    assert_eq!(body_count(&designer), 0);

    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(10.0, 10.0));
    assert_eq!(new_ids.len(), 1);
    assert_eq!(body_count(&designer), 1);

    // Undo removes the pasted body node.
    assert!(designer.undo());
    assert_eq!(body_count(&designer), 0);

    // Redo restores it.
    assert!(designer.redo());
    assert_eq!(body_count(&designer), 1);
}

#[test]
fn test_scoped_cut_undo_restores_body_nodes() {
    let (mut designer, map_id) = setup_with_map();

    let int_id = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    designer.select_node_scoped(&[map_id], int_id);

    let body_count = |d: &StructureDesigner| d.get_scope_network(&[map_id]).unwrap().nodes.len();
    assert_eq!(body_count(&designer), 1);

    assert!(designer.cut_selection());
    assert_eq!(body_count(&designer), 0);

    // Undo the cut's body delete restores the node.
    assert!(designer.undo());
    assert_eq!(body_count(&designer), 1);
}

#[test]
fn test_paste_into_body_shifts_content_inside_rect() {
    // Pasting near the body's top-left corner must not leave nodes at negative
    // body-local coords (which render clipped outside the rect). The body's
    // content is shifted right/down so the top-left-most node clears the inset.
    let (mut designer, map_id) = setup_with_map();

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], float_id);
    designer.copy_selection();

    // Paste at a body-local position that would land the node above/left of
    // the body interior origin (negative coords).
    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(-50.0, -30.0));
    assert_eq!(new_ids.len(), 1);

    let body = designer.get_scope_network(&[map_id]).unwrap();
    let pos = body.nodes.get(&new_ids[0]).unwrap().position;
    // The single pasted node becomes the top-left-most, so it lands exactly at
    // the inset on both axes.
    assert!(
        pos.x >= 8.0 - f64::EPSILON,
        "node x must clear the inset: {}",
        pos.x
    );
    assert!(
        pos.y >= 8.0 - f64::EPSILON,
        "node y must clear the inset: {}",
        pos.y
    );
    assert_eq!(pos, DVec2::new(8.0, 8.0));
}

#[test]
fn test_paste_into_body_preserves_relative_layout_when_shifting() {
    // When the body already has valid content and a paste lands partly past
    // the corner, the whole body shifts rigidly: relative offsets between all
    // nodes (existing + pasted) are preserved.
    let (mut designer, map_id) = setup_with_map();

    // Existing body node well inside the rect.
    let existing = designer.add_node_scoped(&[map_id], "int", DVec2::new(100.0, 100.0), None);
    let existing_before = designer
        .get_scope_network(&[map_id])
        .unwrap()
        .nodes
        .get(&existing)
        .unwrap()
        .position;

    // Copy a top-level node and paste it past the top-left corner.
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], float_id);
    designer.copy_selection();
    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(-42.0, -42.0));
    assert_eq!(new_ids.len(), 1);

    let body = designer.get_scope_network(&[map_id]).unwrap();
    let pasted = body.nodes.get(&new_ids[0]).unwrap().position;
    let existing_after = body.nodes.get(&existing).unwrap().position;

    // Pasted node clears the inset.
    assert_eq!(pasted, DVec2::new(8.0, 8.0));
    // Existing node shifted by the same delta (50, 50): relative layout intact.
    assert_eq!(
        existing_after - existing_before,
        pasted - DVec2::new(-42.0, -42.0)
    );
}

#[test]
fn test_paste_into_body_no_shift_when_already_inside() {
    // A paste that lands fully inside the rect must not move anything.
    let (mut designer, map_id) = setup_with_map();

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], float_id);
    designer.copy_selection();

    let new_ids = designer.paste_at_position_scoped(&[map_id], DVec2::new(120.0, 90.0));
    assert_eq!(new_ids.len(), 1);

    let body = designer.get_scope_network(&[map_id]).unwrap();
    assert_eq!(
        body.nodes.get(&new_ids[0]).unwrap().position,
        DVec2::new(120.0, 90.0),
        "a paste fully inside the rect must not be shifted"
    );
}

#[test]
fn test_paste_scoped_empty_path_matches_top_level() {
    // With an empty scope_path the scoped paste delegates to the top-level
    // path, so it behaves exactly like paste_at_position.
    let mut designer = setup_designer_with_network("main");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.select_node_scoped(&[], float_id);
    designer.copy_selection();

    let new_ids = designer.paste_at_position_scoped(&[], DVec2::new(50.0, 50.0));
    assert_eq!(new_ids.len(), 1);
    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(
        main.nodes.get(&new_ids[0]).unwrap().position,
        DVec2::new(50.0, 50.0)
    );
}
