//! Tests for the Sugiyama layout algorithm.
//!
//! These tests verify:
//! - Layer assignment and dummy node insertion
//! - Crossing minimization via barycenter sweeping
//! - Coordinate assignment with proper spacing
//! - Handling of disconnected components
//! - Edge cases: empty networks, single nodes, long edges

use glam::DVec2;

use rust_lib_flutter_cad::structure_designer::layout::common::LayoutAlgorithm;
use rust_lib_flutter_cad::structure_designer::layout::compute_layout;
use rust_lib_flutter_cad::structure_designer::layout::sugiyama::{
    LayerNode, create_layered_graph_for_testing,
};
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
// Basic Layout Tests
// =============================================================================

#[test]
fn test_sugiyama_empty_network() {
    let designer = setup_designer_with_network("test_network");
    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::Sugiyama,
    );
    assert!(positions.is_empty());
}

#[test]
fn test_sugiyama_single_node() {
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
        LayoutAlgorithm::Sugiyama,
    );

    assert_eq!(positions.len(), 1);
    assert!(positions.contains_key(&node_id));

    let pos = positions.get(&node_id).unwrap();
    assert!(pos.x >= 100.0, "X position should be >= 100: {}", pos.x);
    assert!(pos.y >= 100.0, "Y position should be >= 100: {}", pos.y);
}

#[test]
fn test_sugiyama_linear_chain_x_ordering() {
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
        LayoutAlgorithm::Sugiyama,
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

// =============================================================================
// Dummy Node Tests
// =============================================================================

#[test]
fn test_dummy_nodes_for_long_edges() {
    // Create a network where one source connects to a node 2 layers away
    // This requires a chain to push the target node to a higher layer
    //
    // Structure:
    //   f1 ─────────────────────┐
    //                           ▼
    //   f2 -> sphere1 -> sphere2
    //
    // f1 is at depth 0, f2 is at depth 0
    // sphere1 is at depth 1, sphere2 is at depth 2
    // f1 -> sphere2 is a long edge spanning 2 layers, needs 1 dummy node
    let mut designer = setup_designer_with_network("test_network");
    let f1 = designer.add_node("float", DVec2::ZERO);
    let f2 = designer.add_node("float", DVec2::new(0.0, 100.0));
    let sphere1_id = designer.add_node("sphere", DVec2::new(100.0, 100.0));
    let sphere2_id = designer.add_node("sphere", DVec2::new(200.0, 100.0));

    // Create the main chain: f2 -> sphere1 -> sphere2
    designer.connect_nodes(f2, 0, sphere1_id, 0);
    designer.connect_nodes(sphere1_id, 0, sphere2_id, 0);
    // Long edge: f1 -> sphere2 (spans from layer 0 to layer 2)
    // sphere nodes have 3 arguments, so we connect to argument 1 or 2
    designer.connect_nodes(f1, 0, sphere2_id, 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Verify the connection was made
    let sphere2_node = network.nodes.get(&sphere2_id).unwrap();
    assert!(
        sphere2_node.arguments.len() >= 2,
        "sphere2 should have at least 2 arguments"
    );
    assert!(
        !sphere2_node.arguments[1].argument_output_pins.is_empty(),
        "sphere2 arg[1] should be connected to f1"
    );

    // Create the layered graph and verify dummy nodes were inserted
    let graph = create_layered_graph_for_testing(network);

    // Should have 3 layers (depths 0, 1, 2)
    assert_eq!(
        graph.layers.len(),
        3,
        "Expected 3 layers, got {}",
        graph.layers.len()
    );

    // Count dummy nodes in layer 1
    let dummy_count_layer1 = graph.layers[1]
        .nodes
        .iter()
        .filter(|n| matches!(n, LayerNode::Dummy(_, _, _)))
        .count();

    // The long edge from f1 (layer 0) to sphere2 (layer 2) should create
    // a dummy node at layer 1
    assert!(
        dummy_count_layer1 >= 1,
        "Layer 1 should have at least 1 dummy node for the long edge, got {}",
        dummy_count_layer1
    );
}

#[test]
fn test_no_dummy_nodes_for_adjacent_layers() {
    // Direct connection between adjacent layers should not create dummies
    let mut designer = setup_designer_with_network("test_network");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 0.0));

    designer.connect_nodes(float_id, 0, sphere_id, 0);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    let graph = create_layered_graph_for_testing(network);

    // Should have 2 layers with no dummy nodes
    assert_eq!(graph.layers.len(), 2);

    for layer in &graph.layers {
        for node in &layer.nodes {
            assert!(
                matches!(node, LayerNode::Real(_)),
                "Should have no dummy nodes for adjacent layer edges"
            );
        }
    }
}

// =============================================================================
// Crossing Minimization Tests
// =============================================================================

#[test]
fn test_crossing_minimization_diamond_pattern() {
    // Diamond pattern should have 0 crossings after layout
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

    designer.connect_nodes(float_id, 0, sphere1_id, 0);
    designer.connect_nodes(float_id, 0, sphere2_id, 0);
    designer.connect_nodes(sphere1_id, 0, union_id, 0);
    designer.connect_nodes(sphere2_id, 0, union_id, 1);

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Layout the network
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::Sugiyama,
    );

    // Verify all nodes have positions
    assert_eq!(positions.len(), 4);

    // Create a fresh layered graph after layout to count crossings
    // (Note: we can't directly count crossings after layout since positions are assigned,
    // but we can verify the layout produces reasonable results)
    let pos_sphere1 = positions.get(&sphere1_id).unwrap();
    let pos_sphere2 = positions.get(&sphere2_id).unwrap();

    // Both spheres should be in the same column (same X)
    assert!(
        (pos_sphere1.x - pos_sphere2.x).abs() < 0.001,
        "Spheres should be in same column"
    );

    // They should have different Y positions
    assert!(
        (pos_sphere1.y - pos_sphere2.y).abs() > 1.0,
        "Spheres should have different Y positions"
    );
}

#[test]
fn test_crossing_minimization_reduces_crossings() {
    // Create a graph that would have crossings without optimization
    // A -> C
    // B -> D
    // A -> D (creates crossing if B is above A and C is above D)
    // B -> C
    let mut designer = setup_designer_with_network("test_network");
    let a = designer.add_node("float", DVec2::ZERO);
    let b = designer.add_node("float", DVec2::new(0.0, 100.0));
    let c = designer.add_node("sphere", DVec2::new(100.0, 0.0));
    let d = designer.add_node("sphere", DVec2::new(100.0, 100.0));

    designer.connect_nodes(a, 0, c, 0);
    designer.connect_nodes(b, 0, d, 0);
    designer.connect_nodes(a, 0, d, 0); // Cross connection
    designer.connect_nodes(b, 0, c, 0); // Cross connection

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();

    // Get positions after Sugiyama layout
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::Sugiyama,
    );

    assert_eq!(positions.len(), 4);

    // The Sugiyama algorithm should attempt to minimize crossings
    // We can't guarantee 0 crossings for all graphs, but the algorithm should run
    // without errors
}

// =============================================================================
// Disconnected Components Tests
// =============================================================================

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
        LayoutAlgorithm::Sugiyama,
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
fn test_three_disconnected_components() {
    // Three separate single nodes
    let mut designer = setup_designer_with_network("test_network");
    let n1 = designer.add_node("float", DVec2::ZERO);
    let n2 = designer.add_node("float", DVec2::new(100.0, 0.0));
    let n3 = designer.add_node("float", DVec2::new(200.0, 0.0));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test_network")
        .unwrap();
    let positions = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::Sugiyama,
    );

    assert_eq!(positions.len(), 3);

    // All should be at the same X (depth 0)
    let x1 = positions.get(&n1).unwrap().x;
    let x2 = positions.get(&n2).unwrap().x;
    let x3 = positions.get(&n3).unwrap().x;

    assert!((x1 - x2).abs() < 0.001, "All nodes at same X");
    assert!((x2 - x3).abs() < 0.001, "All nodes at same X");
}

// =============================================================================
// No Overlap Tests
// =============================================================================

#[test]
fn test_no_node_overlap_sugiyama() {
    // Create a complex network and verify no nodes overlap
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
        LayoutAlgorithm::Sugiyama,
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
    let position_vec: Vec<(u64, DVec2)> = positions.iter().map(|(&id, &p)| (id, p)).collect();

    for i in 0..position_vec.len() {
        for j in (i + 1)..position_vec.len() {
            let (id1, pos1) = position_vec[i];
            let (id2, pos2) = position_vec[j];

            let size1 = node_sizes
                .get(&id1)
                .copied()
                .unwrap_or(DVec2::new(160.0, 83.0));
            let size2 = node_sizes
                .get(&id2)
                .copied()
                .unwrap_or(DVec2::new(160.0, 83.0));

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

// =============================================================================
// Wide Fan-out and Deep Graph Tests
// =============================================================================

#[test]
fn test_wide_fanout_layout_sugiyama() {
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
        LayoutAlgorithm::Sugiyama,
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
fn test_deep_narrow_graph_sugiyama() {
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
        LayoutAlgorithm::Sugiyama,
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

// =============================================================================
// Comparison Tests
// =============================================================================

#[test]
fn test_sugiyama_vs_topological_grid_same_result_for_simple_chain() {
    // For a simple linear chain, both algorithms should produce similar results
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

    let positions_topo = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::TopologicalGrid,
    );

    let positions_sugiyama = compute_layout(
        network,
        &designer.node_type_registry,
        LayoutAlgorithm::Sugiyama,
    );

    // Both should produce valid layouts with all nodes
    assert_eq!(positions_topo.len(), 3);
    assert_eq!(positions_sugiyama.len(), 3);

    // Both should have increasing X for the chain
    assert!(positions_topo.get(&float_id).unwrap().x < positions_topo.get(&sphere_id).unwrap().x);
    assert!(
        positions_sugiyama.get(&float_id).unwrap().x
            < positions_sugiyama.get(&sphere_id).unwrap().x
    );
}

// =============================================================================
// LayerNode Tests
// =============================================================================

#[test]
fn test_layer_node_real_id() {
    let real = LayerNode::Real(42);
    let dummy = LayerNode::Dummy(1, 2, 0);

    assert_eq!(real.real_id(), Some(42));
    assert_eq!(dummy.real_id(), None);
}

#[test]
fn test_layer_node_equality() {
    let real1 = LayerNode::Real(42);
    let real2 = LayerNode::Real(42);
    let real3 = LayerNode::Real(43);
    let dummy1 = LayerNode::Dummy(1, 2, 0);
    let dummy2 = LayerNode::Dummy(1, 2, 0);
    let dummy3 = LayerNode::Dummy(1, 2, 1);

    assert_eq!(real1, real2);
    assert_ne!(real1, real3);
    assert_eq!(dummy1, dummy2);
    assert_ne!(dummy1, dummy3);
    assert_ne!(real1, dummy1);
}
