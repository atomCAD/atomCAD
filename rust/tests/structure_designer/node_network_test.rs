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
    assert_eq!(network.selected_node_id, Some(node_id));
    assert!(network.selected_wire.is_none());
}

#[test]
fn test_select_wire() {
    let mut designer = setup_designer_with_network("test_network");
    
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    
    assert!(network.select_wire(float_id, 0, sphere_id, 0));
    assert!(network.selected_wire.is_some());
    assert!(network.selected_node_id.is_none());
    
    let wire = network.selected_wire.as_ref().unwrap();
    assert_eq!(wire.source_node_id, float_id);
    assert_eq!(wire.destination_node_id, sphere_id);
}

#[test]
fn test_clear_selection() {
    let mut designer = setup_designer_with_network("test_network");
    
    let node_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    
    let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
    network.select_node(node_id);
    
    assert!(network.selected_node_id.is_some());
    
    network.clear_selection();
    
    assert!(network.selected_node_id.is_none());
    assert!(network.selected_wire.is_none());
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
