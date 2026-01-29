use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

#[test]
fn test_add_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(100.0, 200.0));
    assert_ne!(node_id, 0, "Node ID should be non-zero on success");
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert_eq!(network.nodes.len(), 1);
    
    let node = network.nodes.get(&node_id).unwrap();
    assert_eq!(node.position, DVec2::new(100.0, 200.0));
    assert_eq!(node.node_type_name, "float");
}

#[test]
fn test_move_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.move_node(node_id, DVec2::new(300.0, 400.0));
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    assert_eq!(node.position, DVec2::new(300.0, 400.0));
}

#[test]
fn test_connect_nodes() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let sphere_node = network.nodes.get(&sphere_id).unwrap();
    
    assert!(!sphere_node.arguments[0].is_empty());
    assert_eq!(sphere_node.arguments[0].get_node_id(), Some(float_id));
}

#[test]
fn test_get_connected_node_ids() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));
    
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    designer.connect_nodes(sphere_id, 0, union_id, 0);
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    
    let connected = network.get_connected_node_ids(sphere_id);
    assert!(connected.contains(&float_id), "Sphere should be connected to float");
    assert!(connected.contains(&union_id), "Sphere should be connected to union");
    assert_eq!(connected.len(), 2);
}

#[test]
fn test_select_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    assert!(network.select_node(node_id));
    assert!(network.is_node_selected(node_id));
    assert_eq!(network.active_node_id, Some(node_id));
    assert!(network.selected_wires.is_empty());
}

#[test]
fn test_select_wire() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    assert!(network.select_wire(float_id, 0, sphere_id, 0));
    assert!(!network.selected_wires.is_empty());
    assert!(network.selected_node_ids.is_empty());
    
    let wire = &network.selected_wires[0];
    assert_eq!(wire.source_node_id, float_id);
    assert_eq!(wire.destination_node_id, sphere_id);
}

#[test]
fn test_clear_selection() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    network.select_node(node_id);
    
    assert!(!network.selected_node_ids.is_empty());
    
    network.clear_selection();
    
    assert!(network.selected_node_ids.is_empty());
    assert!(network.active_node_id.is_none());
    assert!(network.selected_wires.is_empty());
}

#[test]
fn test_set_return_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    assert!(network.set_return_node(sphere_id));
    assert_eq!(network.return_node_id, Some(sphere_id));
    
    assert!(!network.set_return_node(9999));
}

#[test]
fn test_duplicate_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let original_id = designer.add_node("float", DVec2::new(100.0, 100.0));
    
    let duplicated_id = designer.duplicate_node(original_id);
    assert_ne!(duplicated_id, 0);
    assert_ne!(duplicated_id, original_id);
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    
    assert!(network.nodes.contains_key(&original_id));
    assert!(network.nodes.contains_key(&duplicated_id));
    
    let original = network.nodes.get(&original_id).unwrap();
    let duplicated = network.nodes.get(&duplicated_id).unwrap();
    assert_eq!(original.node_type_name, duplicated.node_type_name);
}

#[test]
fn test_delete_selected_node() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    
    {
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_node(node_id);
        assert!(network.nodes.contains_key(&node_id));
    }
    
    designer.delete_selected();
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert!(!network.nodes.contains_key(&node_id));
}

#[test]
fn test_delete_selected_wire() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    
    {
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        assert!(!network.nodes.get(&sphere_id).unwrap().arguments[0].is_empty());
        network.select_wire(float_id, 0, sphere_id, 0);
    }
    
    designer.delete_selected();
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert!(network.nodes.get(&sphere_id).unwrap().arguments[0].is_empty());
}

#[test]
fn test_build_reverse_dependency_map() {
    let mut designer = setup_designer_with_network("test_network");
    
    let a_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let b_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let c_id = designer.add_node("union", DVec2::new(200.0, 0.0));
    
    designer.connect_nodes(a_id, 0, b_id, 0);
    designer.connect_nodes(b_id, 0, c_id, 0);
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let reverse_map = network.build_reverse_dependency_map();
    
    assert!(reverse_map.get(&a_id).unwrap().contains(&b_id));
    assert!(reverse_map.get(&b_id).unwrap().contains(&c_id));
    assert!(reverse_map.get(&c_id).is_none());
}

#[test]
fn test_node_display_toggle() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    assert!(network.is_node_displayed(node_id));
    
    network.set_node_display(node_id, false);
    assert!(!network.is_node_displayed(node_id));
    
    network.set_node_display(node_id, true);
    assert!(network.is_node_displayed(node_id));
}

#[test]
fn test_can_connect_incompatible_types() {
    let mut designer = setup_designer_with_network("test_network");
    
    let string_id = designer.add_node("string", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    
    // String output should not be connectable to sphere's Float radius input
    assert!(!designer.can_connect_nodes(string_id, 0, sphere_id, 0));
}

// ===== MULTI-SELECTION TESTS =====

#[test]
fn test_toggle_node_selection() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Toggle on first node
    assert!(network.toggle_node_selection(node1));
    assert!(network.is_node_selected(node1));
    assert_eq!(network.active_node_id, Some(node1));
    
    // Toggle on second node (adds to selection)
    assert!(network.toggle_node_selection(node2));
    assert!(network.is_node_selected(node1));
    assert!(network.is_node_selected(node2));
    assert_eq!(network.active_node_id, Some(node2));
    
    // Toggle off first node (removes from selection)
    assert!(network.toggle_node_selection(node1));
    assert!(!network.is_node_selected(node1));
    assert!(network.is_node_selected(node2));
    
    // Toggle off second node
    assert!(network.toggle_node_selection(node2));
    assert!(!network.is_node_selected(node2));
    assert!(network.active_node_id.is_none());
}

#[test]
fn test_add_node_to_selection() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let node3 = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select first node normally
    network.select_node(node1);
    assert!(network.is_node_selected(node1));
    assert_eq!(network.selected_node_ids.len(), 1);
    
    // Add second node to selection
    assert!(network.add_node_to_selection(node2));
    assert!(network.is_node_selected(node1));
    assert!(network.is_node_selected(node2));
    assert_eq!(network.selected_node_ids.len(), 2);
    assert_eq!(network.active_node_id, Some(node2));
    
    // Add third node
    assert!(network.add_node_to_selection(node3));
    assert_eq!(network.selected_node_ids.len(), 3);
    assert_eq!(network.active_node_id, Some(node3));
    
    // Adding non-existent node returns false
    assert!(!network.add_node_to_selection(99999));
}

#[test]
fn test_select_nodes_batch() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let node3 = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select multiple nodes at once
    assert!(network.select_nodes(vec![node1, node2, node3]));
    assert!(network.is_node_selected(node1));
    assert!(network.is_node_selected(node2));
    assert!(network.is_node_selected(node3));
    assert_eq!(network.selected_node_ids.len(), 3);
    // Active node is the last one
    assert_eq!(network.active_node_id, Some(node3));
    
    // Selecting clears previous selection
    assert!(network.select_nodes(vec![node1]));
    assert!(network.is_node_selected(node1));
    assert!(!network.is_node_selected(node2));
    assert!(!network.is_node_selected(node3));
    assert_eq!(network.selected_node_ids.len(), 1);
    
    // Empty selection returns false
    assert!(!network.select_nodes(vec![]));
    
    // Non-existent nodes are ignored
    assert!(network.select_nodes(vec![node1, 99999, node2]));
    assert_eq!(network.selected_node_ids.len(), 2);
}

#[test]
fn test_toggle_nodes_selection_batch() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let node3 = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select node1 first
    network.select_node(node1);
    
    // Toggle nodes 1, 2 - should remove 1, add 2
    network.toggle_nodes_selection(vec![node1, node2]);
    assert!(!network.is_node_selected(node1));
    assert!(network.is_node_selected(node2));
    assert_eq!(network.active_node_id, Some(node2));
    
    // Toggle nodes 2, 3 - should remove 2, add 3
    network.toggle_nodes_selection(vec![node2, node3]);
    assert!(!network.is_node_selected(node2));
    assert!(network.is_node_selected(node3));
}

#[test]
fn test_add_nodes_to_selection_batch() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let node3 = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select node1 first
    network.select_node(node1);
    assert_eq!(network.selected_node_ids.len(), 1);
    
    // Add nodes 2, 3 to selection
    network.add_nodes_to_selection(vec![node2, node3]);
    assert!(network.is_node_selected(node1));
    assert!(network.is_node_selected(node2));
    assert!(network.is_node_selected(node3));
    assert_eq!(network.selected_node_ids.len(), 3);
    assert_eq!(network.active_node_id, Some(node3));
}

#[test]
fn test_move_selected_nodes() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 100.0));
    let node3 = designer.add_node("float", DVec2::new(200.0, 200.0));
    
    {
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_nodes(vec![node1, node2]);
    }
    
    // Move selected nodes
    designer.move_selected_nodes(DVec2::new(50.0, 25.0));
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    
    // node1 and node2 should be moved
    assert_eq!(network.nodes.get(&node1).unwrap().position, DVec2::new(50.0, 25.0));
    assert_eq!(network.nodes.get(&node2).unwrap().position, DVec2::new(150.0, 125.0));
    // node3 should NOT be moved (not selected)
    assert_eq!(network.nodes.get(&node3).unwrap().position, DVec2::new(200.0, 200.0));
}

#[test]
fn test_delete_multiple_selected_nodes() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let node3 = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    {
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_nodes(vec![node1, node2]);
    }
    
    designer.delete_selected();
    
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert!(!network.nodes.contains_key(&node1));
    assert!(!network.nodes.contains_key(&node2));
    assert!(network.nodes.contains_key(&node3));
}

#[test]
fn test_toggle_wire_selection() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Toggle on wire
    assert!(network.toggle_wire_selection(float_id, 0, sphere_id, 0));
    assert!(network.is_wire_selected(float_id, 0, sphere_id, 0));
    assert_eq!(network.selected_wires.len(), 1);
    
    // Toggle off wire
    assert!(network.toggle_wire_selection(float_id, 0, sphere_id, 0));
    assert!(!network.is_wire_selected(float_id, 0, sphere_id, 0));
    assert!(network.selected_wires.is_empty());
}

#[test]
fn test_add_wire_to_selection() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let float2 = designer.add_node("float", DVec2::new(0.0, 100.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float1, 0, union_id, 0);
    designer.connect_nodes(float2, 0, union_id, 1);
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select first wire
    network.select_wire(float1, 0, union_id, 0);
    assert_eq!(network.selected_wires.len(), 1);
    
    // Add second wire to selection
    assert!(network.add_wire_to_selection(float2, 0, union_id, 1));
    assert_eq!(network.selected_wires.len(), 2);
    assert!(network.is_wire_selected(float1, 0, union_id, 0));
    assert!(network.is_wire_selected(float2, 0, union_id, 1));
}

#[test]
fn test_mixed_node_wire_selection_with_modifiers() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select a node
    network.select_node(float_id);
    assert!(network.is_node_selected(float_id));
    
    // Add wire with modifier (should not clear nodes)
    network.add_wire_to_selection(float_id, 0, sphere_id, 0);
    assert!(network.is_node_selected(float_id));
    assert!(network.is_wire_selected(float_id, 0, sphere_id, 0));
    
    // Toggle another node (should not clear wires)
    network.toggle_node_selection(sphere_id);
    assert!(network.is_node_selected(float_id));
    assert!(network.is_node_selected(sphere_id));
    assert!(network.is_wire_selected(float_id, 0, sphere_id, 0));
}

#[test]
fn test_active_node_tracking() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node1 = designer.add_node("float", DVec2::new(0.0, 0.0));
    let node2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let _node3 = designer.add_node("float", DVec2::new(200.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    // Select node1, it becomes active
    network.select_node(node1);
    assert!(network.is_node_active(node1));
    assert!(!network.is_node_active(node2));
    
    // Add node2, node2 becomes active (most recently added)
    network.add_node_to_selection(node2);
    assert!(!network.is_node_active(node1));
    assert!(network.is_node_active(node2));
    
    // Toggle off node2, active moves to remaining node
    network.toggle_node_selection(node2);
    assert!(network.is_node_active(node1));
    assert!(!network.is_node_active(node2));
    
    // Clear selection, no active node
    network.clear_selection();
    assert!(network.active_node_id.is_none());
}

// ===== PERSISTENT NODE NAMES TESTS (Phase 1) =====

#[test]
fn test_generate_unique_display_name_empty_network() {
    let designer = setup_designer_with_network("test_network");
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert_eq!(network.generate_unique_display_name("cuboid"), "cuboid1");
}

#[test]
fn test_generate_unique_display_name_increments() {
    let mut designer = setup_designer_with_network("test_network");
    designer.add_node("cuboid", DVec2::ZERO);  // Gets cuboid1
    designer.add_node("cuboid", DVec2::ZERO);  // Gets cuboid2
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert_eq!(network.generate_unique_display_name("cuboid"), "cuboid3");
}

#[test]
fn test_generate_unique_display_name_after_deletion() {
    let mut designer = setup_designer_with_network("test_network");
    let id1 = designer.add_node("cuboid", DVec2::ZERO);  // cuboid1
    let _id2 = designer.add_node("cuboid", DVec2::ZERO);  // cuboid2

    // Delete cuboid1
    {
        let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
        network.select_node(id1);
    }
    designer.delete_selected();

    // Next cuboid should be cuboid3, NOT cuboid1 (no reuse)
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert_eq!(network.generate_unique_display_name("cuboid"), "cuboid3");
}

#[test]
fn test_add_node_assigns_custom_name() {
    let mut designer = setup_designer_with_network("test_network");
    let id = designer.add_node("sphere", DVec2::ZERO);
    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let node = network.nodes.get(&id).unwrap();
    assert_eq!(node.custom_name, Some("sphere1".to_string()));
}

#[test]
fn test_add_node_assigns_unique_names_per_type() {
    let mut designer = setup_designer_with_network("test_network");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let sphere2_id = designer.add_node("sphere", DVec2::ZERO);

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    assert_eq!(network.nodes.get(&sphere_id).unwrap().custom_name, Some("sphere1".to_string()));
    assert_eq!(network.nodes.get(&cuboid_id).unwrap().custom_name, Some("cuboid1".to_string()));
    assert_eq!(network.nodes.get(&sphere2_id).unwrap().custom_name, Some("sphere2".to_string()));
}

#[test]
fn test_duplicate_node_gets_unique_name() {
    let mut designer = setup_designer_with_network("test_network");
    let id1 = designer.add_node("cuboid", DVec2::ZERO);  // cuboid1
    let id2 = designer.duplicate_node(id1);
    assert_ne!(id2, 0);

    let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
    let node1 = network.nodes.get(&id1).unwrap();
    let node2 = network.nodes.get(&id2).unwrap();

    assert_eq!(node1.custom_name, Some("cuboid1".to_string()));
    assert_eq!(node2.custom_name, Some("cuboid2".to_string()));
}
