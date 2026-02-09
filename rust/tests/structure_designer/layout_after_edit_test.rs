//! Tests for auto-layout after edit operations.
//!
//! These tests verify that layout is correctly applied after network edit
//! operations, which is the Phase 2 integration of the layout module.

use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::layout::{LayoutAlgorithm, layout_network};
use rust_lib_flutter_cad::structure_designer::node_layout;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;

fn create_test_registry() -> NodeTypeRegistry {
    NodeTypeRegistry::new()
}

fn create_test_network() -> NodeNetwork {
    let node_type = NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_type: DataType::Geometry,
        public: true,
        node_data_creator: || {
            Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
        },
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
    };
    NodeNetwork::new(node_type)
}

/// Simulates what ai_edit_network does: edit then layout
fn edit_and_layout(network: &mut NodeNetwork, registry: &NodeTypeRegistry, code: &str) {
    // Apply edit
    let result = edit_network(network, registry, code, true);
    assert!(result.success, "Edit should succeed: {:?}", result.errors);

    // Apply layout (simulating auto_layout_after_edit = true)
    layout_network(network, registry, LayoutAlgorithm::TopologicalGrid);
}

// =============================================================================
// Layout After Edit Tests
// =============================================================================

#[test]
fn test_simple_network_layout_after_edit() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    edit_and_layout(
        &mut network,
        &registry,
        r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            cuboid1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
            union1 = union { shapes: [sphere1, cuboid1] }
            output union1
        "#,
    );

    // Find nodes by name
    let sphere_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("sphere1"))
        .map(|(&id, _)| id)
        .expect("sphere1 should exist");
    let cuboid_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("cuboid1"))
        .map(|(&id, _)| id)
        .expect("cuboid1 should exist");
    let union_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("union1"))
        .map(|(&id, _)| id)
        .expect("union1 should exist");

    // Get positions
    let sphere_pos = network.nodes.get(&sphere_id).unwrap().position;
    let cuboid_pos = network.nodes.get(&cuboid_id).unwrap().position;
    let union_pos = network.nodes.get(&union_id).unwrap().position;

    // Verify topological ordering: sources (sphere, cuboid) should be left of union
    assert!(
        sphere_pos.x < union_pos.x,
        "sphere should be left of union: {} < {}",
        sphere_pos.x,
        union_pos.x
    );
    assert!(
        cuboid_pos.x < union_pos.x,
        "cuboid should be left of union: {} < {}",
        cuboid_pos.x,
        union_pos.x
    );

    // Sources should be in the same column (same depth)
    assert!(
        (sphere_pos.x - cuboid_pos.x).abs() < 0.001,
        "sphere and cuboid should have same X: {} == {}",
        sphere_pos.x,
        cuboid_pos.x
    );
}

#[test]
fn test_linear_chain_layout_after_edit() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    edit_and_layout(
        &mut network,
        &registry,
        r#"
            f1 = float { value: 5.0 }
            sphere1 = sphere { radius: f1 }
            union1 = union { shapes: [sphere1] }
            output union1
        "#,
    );

    // Find nodes
    let f1_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("f1"))
        .map(|(&id, _)| id)
        .expect("f1 should exist");
    let sphere_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("sphere1"))
        .map(|(&id, _)| id)
        .expect("sphere1 should exist");
    let union_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("union1"))
        .map(|(&id, _)| id)
        .expect("union1 should exist");

    // Get positions
    let f1_pos = network.nodes.get(&f1_id).unwrap().position;
    let sphere_pos = network.nodes.get(&sphere_id).unwrap().position;
    let union_pos = network.nodes.get(&union_id).unwrap().position;

    // Verify chain ordering: f1 -> sphere1 -> union1
    assert!(
        f1_pos.x < sphere_pos.x,
        "float should be left of sphere: {} < {}",
        f1_pos.x,
        sphere_pos.x
    );
    assert!(
        sphere_pos.x < union_pos.x,
        "sphere should be left of union: {} < {}",
        sphere_pos.x,
        union_pos.x
    );
}

#[test]
fn test_diamond_pattern_layout_after_edit() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    edit_and_layout(
        &mut network,
        &registry,
        r#"
            r1 = float { value: 5.0 }
            sphere1 = sphere { radius: r1 }
            sphere2 = sphere { radius: r1 }
            union1 = union { shapes: [sphere1, sphere2] }
            output union1
        "#,
    );

    // Find nodes
    let r1_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("r1"))
        .map(|(&id, _)| id)
        .expect("r1 should exist");
    let sphere1_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("sphere1"))
        .map(|(&id, _)| id)
        .expect("sphere1 should exist");
    let sphere2_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("sphere2"))
        .map(|(&id, _)| id)
        .expect("sphere2 should exist");
    let union_id = network
        .nodes
        .iter()
        .find(|(_, n)| n.custom_name.as_deref() == Some("union1"))
        .map(|(&id, _)| id)
        .expect("union1 should exist");

    // Get positions
    let r1_pos = network.nodes.get(&r1_id).unwrap().position;
    let sphere1_pos = network.nodes.get(&sphere1_id).unwrap().position;
    let sphere2_pos = network.nodes.get(&sphere2_id).unwrap().position;
    let union_pos = network.nodes.get(&union_id).unwrap().position;

    // Verify diamond: r1 at depth 0, spheres at depth 1, union at depth 2
    assert!(r1_pos.x < sphere1_pos.x, "r1 should be left of sphere1");
    assert!(r1_pos.x < sphere2_pos.x, "r1 should be left of sphere2");
    assert!(
        sphere1_pos.x < union_pos.x,
        "sphere1 should be left of union"
    );
    assert!(
        sphere2_pos.x < union_pos.x,
        "sphere2 should be left of union"
    );

    // Spheres should be in same column
    assert!(
        (sphere1_pos.x - sphere2_pos.x).abs() < 0.001,
        "spheres should have same X"
    );

    // Spheres should have different Y
    assert!(
        (sphere1_pos.y - sphere2_pos.y).abs() > 1.0,
        "spheres should have different Y"
    );
}

#[test]
fn test_no_overlap_after_layout() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    // Create a more complex network
    edit_and_layout(
        &mut network,
        &registry,
        r#"
            f1 = float { value: 1.0 }
            f2 = float { value: 2.0 }
            f3 = float { value: 3.0 }
            sphere1 = sphere { radius: f1 }
            sphere2 = sphere { radius: f2 }
            sphere3 = sphere { radius: f3 }
            union1 = union { shapes: [sphere1, sphere2] }
            union2 = union { shapes: [sphere2, sphere3] }
            union_final = union { shapes: [union1, union2] }
            output union_final
        "#,
    );

    // Collect all positions and verify no overlaps
    let positions: Vec<_> = network
        .nodes
        .iter()
        .map(|(&id, node)| {
            let num_params = registry
                .get_node_type(&node.node_type_name)
                .map(|nt| nt.parameters.len())
                .unwrap_or(0);
            let size = node_layout::estimate_node_size(num_params, true);
            (id, node.position, size)
        })
        .collect();

    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            let (id1, pos1, size1) = positions[i];
            let (id2, pos2, size2) = positions[j];

            let overlaps = node_layout::nodes_overlap(pos1, size1, pos2, size2, 0.0);
            assert!(
                !overlaps,
                "Nodes {} and {} should not overlap at {:?} and {:?}",
                id1, id2, pos1, pos2
            );
        }
    }
}

#[test]
fn test_layout_algorithm_selection() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    // First, edit the network
    let result = edit_network(
        &mut network,
        &registry,
        r#"
            sphere1 = sphere { radius: 5 }
            cuboid1 = cuboid { extent: (10, 10, 10) }
        "#,
        true,
    );
    assert!(result.success);

    // Apply TopologicalGrid layout
    layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);

    // Verify nodes have positions
    for (_, node) in &network.nodes {
        assert!(
            node.position.x > 0.0 || node.position.y > 0.0,
            "Node should have non-zero position after layout"
        );
    }

    // Test that Sugiyama falls back to TopologicalGrid (produces same result pattern)
    let mut network2 = create_test_network();
    edit_network(
        &mut network2,
        &registry,
        r#"
            sphere1 = sphere { radius: 5 }
            cuboid1 = cuboid { extent: (10, 10, 10) }
        "#,
        true,
    );
    layout_network(&mut network2, &registry, LayoutAlgorithm::Sugiyama);

    // Both should have laid out the network (Sugiyama falls back to TopologicalGrid)
    for (_, node) in &network2.nodes {
        assert!(
            node.position.x > 0.0 || node.position.y > 0.0,
            "Node should have non-zero position with Sugiyama (fallback)"
        );
    }
}

#[test]
fn test_empty_network_edit() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    // Edit with empty content
    let result = edit_network(&mut network, &registry, "", true);
    assert!(result.success);

    // Apply layout to empty network (should not panic)
    layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);

    assert!(network.nodes.is_empty());
}

#[test]
fn test_incremental_edit_with_layout() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    // First edit: create some nodes
    edit_and_layout(
        &mut network,
        &registry,
        r#"
            sphere1 = sphere { radius: 5 }
        "#,
    );

    let sphere_pos_before = network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("sphere1"))
        .unwrap()
        .position;

    // Second edit: add more nodes (incremental)
    let result = edit_network(
        &mut network,
        &registry,
        r#"
            cuboid1 = cuboid { extent: (10, 10, 10) }
            union1 = union { shapes: [sphere1, cuboid1] }
            output union1
        "#,
        false, // incremental mode
    );
    assert!(result.success);

    // Apply layout again
    layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);

    // Verify all nodes have proper positions
    let sphere_pos = network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("sphere1"))
        .unwrap()
        .position;
    let cuboid_pos = network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("cuboid1"))
        .unwrap()
        .position;
    let union_pos = network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("union1"))
        .unwrap()
        .position;

    // Union should be to the right of its inputs
    assert!(sphere_pos.x < union_pos.x);
    assert!(cuboid_pos.x < union_pos.x);

    // Position may have changed after incremental edit+layout
    // (this is expected behavior - full layout recalculates all positions)
    let _ = sphere_pos_before; // acknowledge we read it
}
