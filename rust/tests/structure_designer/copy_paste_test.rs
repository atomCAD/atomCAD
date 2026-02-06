use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_node(float_id);
    }
    assert!(designer.copy_selection());

    // Paste at a new position
    let new_ids = designer.paste_at_position(DVec2::new(300.0, 400.0));
    assert_eq!(new_ids.len(), 1);

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();

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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_nodes(vec![float_id, sphere_id]);
    }
    assert!(designer.copy_selection());

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 300.0));
    assert_eq!(new_ids.len(), 2);

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();

    // Find the pasted sphere (the one with node_type_name "sphere" among new_ids)
    let pasted_sphere_id = new_ids.iter()
        .find(|&&id| network.nodes.get(&id).unwrap().node_type_name == "sphere")
        .unwrap();
    let pasted_float_id = new_ids.iter()
        .find(|&&id| network.nodes.get(&id).unwrap().node_type_name == "float")
        .unwrap();

    // The pasted sphere should be wired to the pasted float (internal wire preserved)
    let pasted_sphere = network.nodes.get(pasted_sphere_id).unwrap();
    assert_eq!(pasted_sphere.arguments[0].get_node_id(), Some(*pasted_float_id));

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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_node(sphere_id);
    }
    assert!(designer.copy_selection());

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 300.0));
    assert_eq!(new_ids.len(), 1);

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
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
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert!(network.nodes.contains_key(&float_id));
    assert!(network.nodes.contains_key(&ids1[0]));
    assert!(network.nodes.contains_key(&ids2[0]));
}

#[test]
fn test_repeated_paste_unique_display_names() {
    let mut designer = setup_designer_with_network("test_network");

    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));

    {
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();

    let ids1 = designer.paste_at_position(DVec2::new(100.0, 0.0));
    let ids2 = designer.paste_at_position(DVec2::new(200.0, 0.0));

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_node(float_id);
    }
    assert!(designer.cut_selection());
    assert!(designer.has_clipboard_content());

    // Original node should be deleted
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("network_a").unwrap();
        network.select_node(float_id);
    }
    designer.copy_selection();

    // Switch to a different network
    designer.add_node_network("network_b");
    designer.set_active_node_network_name(Some("network_b".to_string()));

    let new_ids = designer.paste_at_position(DVec2::new(50.0, 50.0));
    assert_eq!(new_ids.len(), 1);

    // Verify the pasted node is in network_b
    let network_b = designer.node_type_registry.node_networks.get("network_b").unwrap();
    let pasted = network_b.nodes.get(&new_ids[0]).unwrap();
    assert_eq!(pasted.node_type_name, "float");
    assert_eq!(pasted.position, DVec2::new(50.0, 50.0));

    // Original still in network_a
    let network_a = designer.node_type_registry.node_networks.get("network_a").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("main").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("helper").unwrap();
        network.set_return_node(sphere_id);
    }
    designer.validate_active_network();

    // Switch to main and add a node of type "helper"
    designer.set_active_node_network_name(Some("main".to_string()));
    let helper_node_id = designer.add_node("helper", DVec2::new(0.0, 0.0));

    // Select and copy the helper node
    {
        let network = designer.node_type_registry.node_networks.get_mut("main").unwrap();
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

// ===== CLIPBOARD INVALIDATION ON DELETE =====

#[test]
fn test_clipboard_cleared_on_referenced_network_delete() {
    let mut designer = setup_designer_with_network("main");

    // Create a custom network "helper" with a return node
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    {
        let network = designer.node_type_registry.node_networks.get_mut("helper").unwrap();
        network.set_return_node(sphere_id);
    }
    designer.validate_active_network();

    // Switch to main and add a node of type "helper"
    designer.set_active_node_network_name(Some("main".to_string()));
    let helper_node_id = designer.add_node("helper", DVec2::new(0.0, 0.0));

    // Copy the helper node
    {
        let network = designer.node_type_registry.node_networks.get_mut("main").unwrap();
        network.select_node(helper_node_id);
    }
    designer.copy_selection();
    assert!(designer.has_clipboard_content());

    // Delete the helper node from main first (so "helper" network is not referenced)
    {
        let network = designer.node_type_registry.node_networks.get_mut("main").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.set_return_node(sphere_id);
        network.select_node(sphere_id);
    }

    designer.copy_selection();
    let new_ids = designer.paste_at_position(DVec2::new(200.0, 200.0));

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();

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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_nodes(vec![id1, id2]);
    }
    designer.copy_selection();

    // Paste at (500, 500) — nodes should be at (400, 500) and (600, 500)
    let new_ids = designer.paste_at_position(DVec2::new(500.0, 500.0));
    assert_eq!(new_ids.len(), 2);

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let mut positions: Vec<DVec2> = new_ids.iter()
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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
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
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_nodes(vec![id1, id2]);
    }
    designer.copy_selection();

    let new_ids = designer.paste_at_position(DVec2::new(0.0, 200.0));

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();

    // Pasted nodes should be selected
    for &new_id in &new_ids {
        assert!(network.is_node_selected(new_id));
    }

    // Original nodes should NOT be selected (select_nodes clears previous selection)
    assert!(!network.is_node_selected(id1));
    assert!(!network.is_node_selected(id2));
}
