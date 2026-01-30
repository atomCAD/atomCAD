//! Tests for the topological grid layout algorithm.
//!
//! These tests verify:
//! - Depth computation for various DAG structures
//! - Column ordering with barycenter heuristic
//! - Position assignment with correct spacing
//! - Edge cases: empty networks, single nodes, disconnected components

use glam::DVec2;

use rust_lib_flutter_cad::structure_designer::layout::common::{
    compute_node_depths, find_sink_nodes, find_source_nodes, get_input_node_ids,
    get_output_node_ids, LayoutAlgorithm,
};
use rust_lib_flutter_cad::structure_designer::layout::compute_layout;
use rust_lib_flutter_cad::structure_designer::node_layout;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

/// Helper to create a StructureDesigner with a test network
fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

// =============================================================================
// Depth Computation Tests
// =============================================================================

#[test]
fn test_empty_network_depths() {
    let designer = setup_designer_with_network("test_network");
    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let depths = compute_node_depths(network);
    assert!(depths.is_empty());
}

#[test]
fn test_single_node_depth() {
    let mut designer = setup_designer_with_network("test_network");
    let node_id = designer.add_node("float", DVec2::ZERO);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let depths = compute_node_depths(network);
    assert_eq!(depths.get(&node_id), Some(&0));
}

#[test]
fn test_linear_chain_depths() {
    // float -> sphere -> union (linear chain)
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0); // float -> sphere (radius)
    designer.connect_nodes(sphere_id, 0, union_id, 0); // sphere -> union

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let depths = compute_node_depths(network);

    assert_eq!(depths.get(&float_id), Some(&0)); // source
    assert_eq!(depths.get(&sphere_id), Some(&1)); // depth 1
    assert_eq!(depths.get(&union_id), Some(&2)); // depth 2
}

#[test]
fn test_diamond_pattern_depths() {
    //      float
    //      /   \
    //  sphere1 sphere2
    //      \   /
    //      union
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere1_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let sphere2_id = designer.add_node("sphere", DVec2::new(100.0, 100.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 50.0));

    designer.connect_nodes(float_id, 0, sphere1_id, 0); // float -> sphere1
    designer.connect_nodes(float_id, 0, sphere2_id, 0); // float -> sphere2
    designer.connect_nodes(sphere1_id, 0, union_id, 0); // sphere1 -> union
    designer.connect_nodes(sphere2_id, 0, union_id, 1); // sphere2 -> union

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let depths = compute_node_depths(network);

    assert_eq!(depths.get(&float_id), Some(&0)); // source at depth 0
    assert_eq!(depths.get(&sphere1_id), Some(&1)); // depth 1
    assert_eq!(depths.get(&sphere2_id), Some(&1)); // depth 1
    assert_eq!(depths.get(&union_id), Some(&2)); // depth 2
}

#[test]
fn test_multiple_sources_depths() {
    // float1  float2 (both sources)
    //    \    /
    //     union
    let mut designer = setup_designer_with_network("test_network");
    let float1_id = designer.add_node("float", DVec2::ZERO);
    let float2_id = designer.add_node("float", DVec2::new(0.0, 100.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 50.0));

    // Both floats connect to union
    designer.connect_nodes(float1_id, 0, union_id, 0);
    designer.connect_nodes(float2_id, 0, union_id, 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let depths = compute_node_depths(network);

    assert_eq!(depths.get(&float1_id), Some(&0)); // source
    assert_eq!(depths.get(&float2_id), Some(&0)); // source
    assert_eq!(depths.get(&union_id), Some(&1)); // depth 1
}

#[test]
fn test_disconnected_components_depths() {
    // Two separate chains: float1->sphere1  and  float2->sphere2
    let mut designer = setup_designer_with_network("test_network");
    let float1_id = designer.add_node("float", DVec2::ZERO);
    let sphere1_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let float2_id = designer.add_node("float", DVec2::new(0.0, 200.0));
    let sphere2_id = designer.add_node("sphere", DVec2::new(100.0, 200.0));

    designer.connect_nodes(float1_id, 0, sphere1_id, 0);
    designer.connect_nodes(float2_id, 0, sphere2_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let depths = compute_node_depths(network);

    assert_eq!(depths.get(&float1_id), Some(&0));
    assert_eq!(depths.get(&sphere1_id), Some(&1));
    assert_eq!(depths.get(&float2_id), Some(&0));
    assert_eq!(depths.get(&sphere2_id), Some(&1));
}

// =============================================================================
// Source/Sink Node Tests
// =============================================================================

#[test]
fn test_find_source_nodes() {
    // float -> sphere -> union
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0);
    designer.connect_nodes(sphere_id, 0, union_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let sources = find_source_nodes(network);

    assert_eq!(sources.len(), 1);
    assert!(sources.contains(&float_id));
}

#[test]
fn test_find_sink_nodes() {
    // float -> sphere -> union
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0);
    designer.connect_nodes(sphere_id, 0, union_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let sinks = find_sink_nodes(network);

    assert_eq!(sinks.len(), 1);
    assert!(sinks.contains(&union_id));
}

#[test]
fn test_get_input_output_node_ids() {
    // float -> sphere -> union
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0);
    designer.connect_nodes(sphere_id, 0, union_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Test inputs
    assert!(get_input_node_ids(network, float_id).is_empty());
    assert!(get_input_node_ids(network, sphere_id).contains(&float_id));
    assert!(get_input_node_ids(network, union_id).contains(&sphere_id));

    // Test outputs
    assert!(get_output_node_ids(network, float_id).contains(&sphere_id));
    assert!(get_output_node_ids(network, sphere_id).contains(&union_id));
    assert!(get_output_node_ids(network, union_id).is_empty());
}

// =============================================================================
// Layout Tests
// =============================================================================

#[test]
fn test_empty_network_layout() {
    let designer = setup_designer_with_network("test_network");
    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );
    assert!(positions.is_empty());
}

#[test]
fn test_single_node_layout() {
    let mut designer = setup_designer_with_network("test_network");
    let node_id = designer.add_node("float", DVec2::ZERO);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    assert_eq!(positions.len(), 1);
    assert!(positions.contains_key(&node_id));

    // Node should be at start position
    let pos = positions.get(&node_id).unwrap();
    assert!(pos.x >= 100.0, "X position should be >= 100: {}", pos.x);
    assert!(pos.y >= 100.0, "Y position should be >= 100: {}", pos.y);
}

#[test]
fn test_linear_chain_layout_x_ordering() {
    // float -> sphere -> union should have increasing X positions
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0);
    designer.connect_nodes(sphere_id, 0, union_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    let pos_float = positions.get(&float_id).unwrap();
    let pos_sphere = positions.get(&sphere_id).unwrap();
    let pos_union = positions.get(&union_id).unwrap();

    // X should increase with depth
    assert!(
        pos_float.x < pos_sphere.x,
        "float.x < sphere.x: {} < {}",
        pos_float.x,
        pos_sphere.x
    );
    assert!(
        pos_sphere.x < pos_union.x,
        "sphere.x < union.x: {} < {}",
        pos_sphere.x,
        pos_union.x
    );
}

#[test]
fn test_nodes_in_same_column_have_same_x() {
    // Diamond: float at depth 0, sphere1 and sphere2 at depth 1, union at depth 2
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere1_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let sphere2_id = designer.add_node("sphere", DVec2::new(100.0, 100.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 50.0));

    designer.connect_nodes(float_id, 0, sphere1_id, 0);
    designer.connect_nodes(float_id, 0, sphere2_id, 0);
    designer.connect_nodes(sphere1_id, 0, union_id, 0);
    designer.connect_nodes(sphere2_id, 0, union_id, 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    let pos_sphere1 = positions.get(&sphere1_id).unwrap();
    let pos_sphere2 = positions.get(&sphere2_id).unwrap();

    // Both spheres should have the same X (same column)
    assert!(
        (pos_sphere1.x - pos_sphere2.x).abs() < 0.001,
        "sphere1.x == sphere2.x: {} == {}",
        pos_sphere1.x,
        pos_sphere2.x
    );
}

#[test]
fn test_nodes_in_same_column_have_different_y() {
    // Two sources float1 and float2 should be in same column but different Y
    let mut designer = setup_designer_with_network("test_network");
    let float1_id = designer.add_node("float", DVec2::ZERO);
    let float2_id = designer.add_node("float", DVec2::new(0.0, 100.0));
    let union_id = designer.add_node("union", DVec2::new(200.0, 50.0));

    designer.connect_nodes(float1_id, 0, union_id, 0);
    designer.connect_nodes(float2_id, 0, union_id, 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    let pos_float1 = positions.get(&float1_id).unwrap();
    let pos_float2 = positions.get(&float2_id).unwrap();

    // float1 and float2 should have different Y positions
    assert!(
        (pos_float1.y - pos_float2.y).abs() > 1.0,
        "float1.y != float2.y: {} != {}",
        pos_float1.y,
        pos_float2.y
    );
}

#[test]
fn test_no_node_overlap() {
    // Create a more complex network and verify no nodes overlap
    let mut designer = setup_designer_with_network("test_network");

    // Create source nodes
    let f1 = designer.add_node("float", DVec2::ZERO);
    let f2 = designer.add_node("float", DVec2::new(0.0, 100.0));
    let f3 = designer.add_node("float", DVec2::new(0.0, 200.0));

    // Create intermediate nodes
    let s1 = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let s2 = designer.add_node("sphere", DVec2::new(100.0, 100.0));
    let s3 = designer.add_node("sphere", DVec2::new(100.0, 200.0));

    // Create union nodes
    let u1 = designer.add_node("union", DVec2::new(200.0, 50.0));
    let u2 = designer.add_node("union", DVec2::new(200.0, 150.0));

    // Create final union
    let final_union = designer.add_node("union", DVec2::new(300.0, 100.0));

    // Connect them
    designer.connect_nodes(f1, 0, s1, 0);
    designer.connect_nodes(f2, 0, s2, 0);
    designer.connect_nodes(f3, 0, s3, 0);
    designer.connect_nodes(s1, 0, u1, 0);
    designer.connect_nodes(s2, 0, u1, 1);
    designer.connect_nodes(s2, 0, u2, 0);
    designer.connect_nodes(s3, 0, u2, 1);
    designer.connect_nodes(u1, 0, final_union, 0);
    designer.connect_nodes(u2, 0, final_union, 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    // Build a map of node ID to its actual size based on node type
    use std::collections::HashMap;
    let node_sizes: HashMap<u64, DVec2> = network
        .nodes
        .iter()
        .map(|(&id, node)| {
            let num_params = designer
                .node_type_registry
                .get_node_type(&node.node_type_name)
                .map(|nt| nt.parameters.len())
                .unwrap_or(0);
            let size = node_layout::estimate_node_size(num_params, true);
            (id, size)
        })
        .collect();

    // Check for overlaps using actual node sizes
    // Use 0 gap since the algorithm already provides spacing
    let position_vec: Vec<(u64, DVec2)> = positions.iter().map(|(&id, &p)| (id, p)).collect();

    for i in 0..position_vec.len() {
        for j in (i + 1)..position_vec.len() {
            let (id1, pos1) = position_vec[i];
            let (id2, pos2) = position_vec[j];

            let size1 = node_sizes.get(&id1).copied().unwrap_or(DVec2::new(160.0, 83.0));
            let size2 = node_sizes.get(&id2).copied().unwrap_or(DVec2::new(160.0, 83.0));

            // Check if boxes overlap (no additional gap required, algorithm handles spacing)
            let overlaps = node_layout::nodes_overlap(pos1, size1, pos2, size2, 0.0);
            assert!(
                !overlaps,
                "Nodes {} and {} overlap at positions {:?} (size {:?}) and {:?} (size {:?})",
                id1, id2, pos1, size1, pos2, size2
            );
        }
    }
}

#[test]
fn test_layout_network_modifies_positions() {
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);

    designer.connect_nodes(float_id, 0, sphere_id, 0);

    // Set initial positions to zero
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        network.nodes.get_mut(&float_id).unwrap().position = DVec2::ZERO;
        network.nodes.get_mut(&sphere_id).unwrap().position = DVec2::ZERO;
    }

    // Compute layout positions first, then apply them
    let positions = {
        let network = designer
            .node_type_registry
            .node_networks
            .get("test_network")
            .unwrap();
        compute_layout(
            network,
            &designer.node_type_registry,
            LayoutAlgorithm::TopologicalGrid,
        )
    };

    // Apply positions to network
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test_network")
            .unwrap();
        for (node_id, position) in positions {
            if let Some(node) = network.nodes.get_mut(&node_id) {
                node.position = position;
            }
        }
    }

    // Verify positions have changed
    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let pos_float = network.nodes.get(&float_id).unwrap().position;
    let pos_sphere = network.nodes.get(&sphere_id).unwrap().position;

    assert!(
        pos_float.x > 0.0 || pos_float.y > 0.0,
        "Float position should change"
    );
    assert!(
        pos_sphere.x > 0.0 || pos_sphere.y > 0.0,
        "Sphere position should change"
    );
    assert!(
        pos_float.x < pos_sphere.x,
        "Float should be to the left of Sphere"
    );
}

#[test]
fn test_layout_algorithm_enum() {
    // Test that all algorithm variants exist and have default
    let default_algo = LayoutAlgorithm::default();
    assert_eq!(default_algo, LayoutAlgorithm::TopologicalGrid);

    // Test that other variants can be created (even if they fall back to TopologicalGrid)
    let _sugiyama = LayoutAlgorithm::Sugiyama;
    let _incremental = LayoutAlgorithm::Incremental;
}

#[test]
fn test_disconnected_components_layout() {
    // Two separate chains should both be laid out properly
    let mut designer = setup_designer_with_network("test_network");
    let float1_id = designer.add_node("float", DVec2::ZERO);
    let sphere1_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let float2_id = designer.add_node("float", DVec2::new(0.0, 200.0));
    let sphere2_id = designer.add_node("sphere", DVec2::new(100.0, 200.0));

    designer.connect_nodes(float1_id, 0, sphere1_id, 0);
    designer.connect_nodes(float2_id, 0, sphere2_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    // All nodes should have positions
    assert_eq!(positions.len(), 4);

    // Check X ordering within each chain
    let pos_float1 = positions.get(&float1_id).unwrap();
    let pos_sphere1 = positions.get(&sphere1_id).unwrap();
    let pos_float2 = positions.get(&float2_id).unwrap();
    let pos_sphere2 = positions.get(&sphere2_id).unwrap();

    assert!(pos_float1.x < pos_sphere1.x, "Chain 1: source < sink");
    assert!(pos_float2.x < pos_sphere2.x, "Chain 2: source < sink");
}

#[test]
fn test_wide_fanout_layout() {
    // One source feeding multiple sinks
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);

    // Create multiple spheres all connected to the same float
    let mut sphere_ids = Vec::new();
    for i in 0..5 {
        let sphere_id = designer.add_node("sphere", DVec2::new(100.0, i as f64 * 100.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);
        sphere_ids.push(sphere_id);
    }

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    let pos_float = positions.get(&float_id).unwrap();

    // All spheres should be to the right of the float
    for sphere_id in &sphere_ids {
        let pos_sphere = positions.get(sphere_id).unwrap();
        assert!(
            pos_float.x < pos_sphere.x,
            "Float should be left of sphere {}",
            sphere_id
        );
    }

    // All spheres should have the same X (same column)
    let first_sphere_x = positions.get(&sphere_ids[0]).unwrap().x;
    for sphere_id in &sphere_ids[1..] {
        let sphere_x = positions.get(sphere_id).unwrap().x;
        assert!(
            (first_sphere_x - sphere_x).abs() < 0.001,
            "All spheres should have same X"
        );
    }
}

#[test]
fn test_deep_narrow_graph_layout() {
    // Long chain of nodes
    let mut designer = setup_designer_with_network("test_network");

    let mut prev_id = designer.add_node("float", DVec2::ZERO);
    let mut node_ids = vec![prev_id];

    // Create a chain of 5 sphere nodes
    for i in 0..5 {
        let sphere_id = designer.add_node("sphere", DVec2::new((i + 1) as f64 * 100.0, 0.0));
        designer.connect_nodes(prev_id, 0, sphere_id, 0);
        node_ids.push(sphere_id);
        prev_id = sphere_id;
    }

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    // Each node should have increasing X
    for i in 0..(node_ids.len() - 1) {
        let pos_curr = positions.get(&node_ids[i]).unwrap();
        let pos_next = positions.get(&node_ids[i + 1]).unwrap();
        assert!(
            pos_curr.x < pos_next.x,
            "Node {} should be left of node {}: {} < {}",
            node_ids[i],
            node_ids[i + 1],
            pos_curr.x,
            pos_next.x
        );
    }
}
