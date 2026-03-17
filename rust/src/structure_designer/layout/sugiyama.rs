//! Sugiyama (Layered) Layout Algorithm
//!
//! The Sugiyama algorithm is the gold standard for drawing directed acyclic graphs (DAGs).
//! It produces hierarchical layouts with nodes organized in layers, minimized edge crossings,
//! and clean edge routing for long-span connections.
//!
//! ## Algorithm Phases
//!
//! 1. **Layer Assignment**: Assign each node to a horizontal layer (same as depth)
//! 2. **Dummy Node Insertion**: Add invisible nodes for edges spanning multiple layers
//! 3. **Crossing Minimization**: Reorder nodes within layers to minimize edge crossings
//! 4. **Coordinate Assignment**: Assign X/Y positions with vertical alignment
//!
//! ## When to Use
//!
//! Sugiyama excels at:
//! - Complex DAGs with many cross-connections
//! - Diamond/merge patterns where nodes merge multiple inputs
//! - Networks with long-span edges
//! - Presentation/export quality layouts

use std::collections::{HashMap, HashSet, VecDeque};

use glam::DVec2;

use crate::structure_designer::node_layout;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

use super::common::compute_node_depths;

// Layout constants
const START_X: f64 = 100.0;
const START_Y: f64 = 100.0;
const COLUMN_WIDTH: f64 = 210.0; // NODE_WIDTH (160) + horizontal gap (50)
const VERTICAL_GAP: f64 = 30.0;
const COMPONENT_GAP: f64 = 80.0;

// =============================================================================
// Data Structures
// =============================================================================

/// Represents either a real node or a dummy node for edge routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerNode {
    /// A real node from the network.
    Real(u64),
    /// A dummy node for long edge routing.
    /// Contains (source_node_id, target_node_id, segment_index).
    Dummy(u64, u64, usize),
}

impl LayerNode {
    /// Returns the real node ID if this is a Real node, None otherwise.
    pub fn real_id(&self) -> Option<u64> {
        match self {
            LayerNode::Real(id) => Some(*id),
            LayerNode::Dummy(_, _, _) => None,
        }
    }
}

/// A layer containing both real and dummy nodes in order.
#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub nodes: Vec<LayerNode>,
}

/// Direction for barycenter sweeps.
#[derive(Debug, Clone, Copy)]
enum Direction {
    /// Look at previous layer (backward edges)
    Backward,
    /// Look at next layer (forward edges)
    Forward,
}

/// The layered graph with dummy nodes inserted.
#[derive(Debug)]
pub struct LayeredGraph {
    pub layers: Vec<Layer>,
    /// Maps each layer node to its neighbors in the next layer.
    pub forward_edges: HashMap<LayerNode, Vec<LayerNode>>,
    /// Maps each layer node to its neighbors in the previous layer.
    pub backward_edges: HashMap<LayerNode, Vec<LayerNode>>,
}

impl LayeredGraph {
    pub fn new(num_layers: usize) -> Self {
        Self {
            layers: (0..num_layers).map(|_| Layer::default()).collect(),
            forward_edges: HashMap::new(),
            backward_edges: HashMap::new(),
        }
    }

    /// Add an edge between two layer nodes.
    pub fn add_edge(&mut self, from: LayerNode, to: LayerNode) {
        self.forward_edges.entry(from).or_default().push(to);
        self.backward_edges.entry(to).or_default().push(from);
    }

    /// Get the layer index for a node.
    pub fn get_layer_index(&self, node: LayerNode) -> Option<usize> {
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            if layer.nodes.contains(&node) {
                return Some(layer_idx);
            }
        }
        None
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

/// Compute positions for all nodes using the Sugiyama layout algorithm.
///
/// This algorithm:
/// 1. Handles disconnected components separately
/// 2. For each component:
///    a. Computes node depths (layer assignment)
///    b. Inserts dummy nodes for long edges
///    c. Minimizes edge crossings via barycenter sweeping
///    d. Assigns final X/Y coordinates
///
/// # Arguments
/// * `network` - The node network to lay out
/// * `registry` - The node type registry for looking up node sizes
///
/// # Returns
/// A HashMap from node ID to new position
pub fn layout(network: &NodeNetwork, registry: &NodeTypeRegistry) -> HashMap<u64, DVec2> {
    // Handle empty network
    if network.nodes.is_empty() {
        return HashMap::new();
    }

    // Find connected components and lay them out separately
    let components = find_connected_components(network);

    if components.len() == 1 {
        // Single component: use standard layout
        return layout_single_component(network, registry, &components[0]);
    }

    // Multiple components: lay out each and stack vertically
    layout_with_components(network, registry, &components)
}

// =============================================================================
// Connected Components
// =============================================================================

/// Find all connected components in the network.
///
/// Returns a vector of HashSets, each containing the node IDs in a component.
/// Components are sorted by size (largest first).
fn find_connected_components(network: &NodeNetwork) -> Vec<HashSet<u64>> {
    let mut visited: HashSet<u64> = HashSet::new();
    let mut components: Vec<HashSet<u64>> = Vec::new();

    for &node_id in network.nodes.keys() {
        if visited.contains(&node_id) {
            continue;
        }

        // BFS to find all nodes in this component
        let mut component: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<u64> = VecDeque::new();
        queue.push_back(node_id);

        while let Some(current) = queue.pop_front() {
            if !component.insert(current) {
                continue;
            }
            visited.insert(current);

            // Add all connected nodes (both inputs and outputs)
            if let Some(node) = network.nodes.get(&current) {
                // Input connections
                for arg in &node.arguments {
                    for &source_id in arg.argument_output_pins.keys() {
                        if !component.contains(&source_id) {
                            queue.push_back(source_id);
                        }
                    }
                }
            }

            // Output connections (nodes that use this node as input)
            for (&other_id, other_node) in &network.nodes {
                for arg in &other_node.arguments {
                    if arg.argument_output_pins.contains_key(&current)
                        && !component.contains(&other_id)
                    {
                        queue.push_back(other_id);
                    }
                }
            }
        }

        components.push(component);
    }

    // Sort by size (largest first)
    components.sort_by_key(|c| std::cmp::Reverse(c.len()));

    components
}

/// Layout multiple disconnected components, stacking them vertically.
fn layout_with_components(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    components: &[HashSet<u64>],
) -> HashMap<u64, DVec2> {
    let mut all_positions: HashMap<u64, DVec2> = HashMap::new();
    let mut current_y: f64 = START_Y;

    for component_nodes in components {
        // Layout this component
        let component_positions = layout_single_component(network, registry, component_nodes);

        // Find the bounding box of this component
        let (min_y, max_y) = component_bounding_box(&component_positions, network, registry);
        let component_height = max_y - min_y;

        // Offset positions to stack below previous components
        let y_offset = current_y - min_y;

        for (node_id, pos) in component_positions {
            all_positions.insert(node_id, DVec2::new(pos.x, pos.y + y_offset));
        }

        current_y += component_height + COMPONENT_GAP;
    }

    all_positions
}

/// Calculate the bounding box (min_y, max_y) for a set of positioned nodes.
fn component_bounding_box(
    positions: &HashMap<u64, DVec2>,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> (f64, f64) {
    if positions.is_empty() {
        return (START_Y, START_Y);
    }

    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for (&node_id, &pos) in positions {
        let height = get_node_height(node_id, network, registry);
        min_y = min_y.min(pos.y);
        max_y = max_y.max(pos.y + height);
    }

    (min_y, max_y)
}

// =============================================================================
// Single Component Layout
// =============================================================================

/// Layout a single connected component.
fn layout_single_component(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    component_nodes: &HashSet<u64>,
) -> HashMap<u64, DVec2> {
    // Phase 1: Layer assignment (compute depths)
    let depths = compute_node_depths(network);

    // Filter to only nodes in this component
    let component_depths: HashMap<u64, usize> = depths
        .into_iter()
        .filter(|(id, _)| component_nodes.contains(id))
        .collect();

    // Group nodes by depth into layers
    let layers = group_by_depth(&component_depths);

    // Phase 2: Dummy node insertion
    let mut graph = insert_dummy_nodes(&layers, network, &component_depths);

    // Phase 3: Crossing minimization
    minimize_crossings(&mut graph);

    // Phase 4: Coordinate assignment
    assign_coordinates(&graph, network, registry)
}

// =============================================================================
// Phase 1: Layer Assignment (Group by Depth)
// =============================================================================

/// Group nodes into layers based on their computed depth.
fn group_by_depth(depths: &HashMap<u64, usize>) -> Vec<Vec<u64>> {
    if depths.is_empty() {
        return Vec::new();
    }

    let max_depth = depths.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<u64>> = vec![vec![]; max_depth + 1];

    for (&node_id, &depth) in depths {
        layers[depth].push(node_id);
    }

    // Sort node IDs within each layer for deterministic output
    for layer in &mut layers {
        layer.sort();
    }

    layers
}

// =============================================================================
// Phase 2: Dummy Node Insertion
// =============================================================================

/// Insert dummy nodes for edges that span multiple layers.
///
/// When an edge spans multiple layers (e.g., from layer 0 to layer 3),
/// dummy nodes are inserted at each intermediate layer to:
/// - Enable proper edge routing with bend points
/// - Allow crossing minimization to account for long edges
fn insert_dummy_nodes(
    layers: &[Vec<u64>],
    network: &NodeNetwork,
    depths: &HashMap<u64, usize>,
) -> LayeredGraph {
    if layers.is_empty() {
        return LayeredGraph::new(0);
    }

    let mut graph = LayeredGraph::new(layers.len());

    // Add all real nodes to their layers
    for (layer_idx, layer_nodes) in layers.iter().enumerate() {
        for &node_id in layer_nodes {
            graph.layers[layer_idx].nodes.push(LayerNode::Real(node_id));
        }
    }

    // For each edge, insert dummy nodes if it spans multiple layers
    for (&node_id, node) in &network.nodes {
        let Some(&target_layer) = depths.get(&node_id) else {
            continue;
        };

        for arg in &node.arguments {
            for &source_id in arg.argument_output_pins.keys() {
                let Some(&source_layer) = depths.get(&source_id) else {
                    continue;
                };

                // Ensure source_layer < target_layer (edges go forward)
                if source_layer >= target_layer {
                    continue;
                }

                if target_layer - source_layer > 1 {
                    // Long edge: insert dummy nodes
                    let mut prev = LayerNode::Real(source_id);

                    for (seg_idx, layer_idx) in (source_layer + 1..target_layer).enumerate() {
                        let dummy = LayerNode::Dummy(source_id, node_id, seg_idx);
                        graph.layers[layer_idx].nodes.push(dummy);
                        graph.add_edge(prev, dummy);
                        prev = dummy;
                    }

                    graph.add_edge(prev, LayerNode::Real(node_id));
                } else {
                    // Short edge: direct connection
                    graph.add_edge(LayerNode::Real(source_id), LayerNode::Real(node_id));
                }
            }
        }
    }

    graph
}

// =============================================================================
// Phase 3: Crossing Minimization
// =============================================================================

/// Minimize edge crossings by reordering nodes within each layer.
///
/// Uses the iterative barycenter method:
/// - Sweep down (layer 0 → layer n-1), reordering by backward neighbors
/// - Sweep up (layer n-1 → layer 0), reordering by forward neighbors
/// - Repeat until no improvement
fn minimize_crossings(graph: &mut LayeredGraph) {
    if graph.layers.len() < 2 {
        return;
    }

    let max_iterations = 24; // Limit iterations to prevent infinite loops
    let mut iteration = 0;

    loop {
        if iteration >= max_iterations {
            break;
        }
        iteration += 1;

        let mut improved = false;

        // Sweep down (layer 0 → layer n-1)
        for layer_idx in 1..graph.layers.len() {
            let old_crossings = count_crossings(graph, layer_idx - 1, layer_idx);
            reorder_by_barycenter(graph, layer_idx, Direction::Backward);
            let new_crossings = count_crossings(graph, layer_idx - 1, layer_idx);

            if new_crossings < old_crossings {
                improved = true;
            }
        }

        // Sweep up (layer n-1 → layer 0)
        for layer_idx in (0..graph.layers.len() - 1).rev() {
            let old_crossings = count_crossings(graph, layer_idx, layer_idx + 1);
            reorder_by_barycenter(graph, layer_idx, Direction::Forward);
            let new_crossings = count_crossings(graph, layer_idx, layer_idx + 1);

            if new_crossings < old_crossings {
                improved = true;
            }
        }

        if !improved {
            break;
        }
    }
}

/// Compute the barycenter (average position) for a node based on its neighbors.
fn compute_barycenter(
    node: LayerNode,
    neighbor_positions: &HashMap<LayerNode, usize>,
    edges: &HashMap<LayerNode, Vec<LayerNode>>,
) -> f64 {
    let neighbors = match edges.get(&node) {
        Some(n) if !n.is_empty() => n,
        _ => return f64::MAX, // No neighbors, sort to end
    };

    let positions: Vec<usize> = neighbors
        .iter()
        .filter_map(|n| neighbor_positions.get(n).copied())
        .collect();

    if positions.is_empty() {
        return f64::MAX;
    }

    let sum: usize = positions.iter().sum();
    sum as f64 / positions.len() as f64
}

/// Reorder nodes in a layer based on the barycenter of their neighbors.
fn reorder_by_barycenter(graph: &mut LayeredGraph, layer_idx: usize, direction: Direction) {
    let neighbor_layer = match direction {
        Direction::Backward => {
            if layer_idx == 0 {
                return;
            }
            layer_idx - 1
        }
        Direction::Forward => {
            if layer_idx + 1 >= graph.layers.len() {
                return;
            }
            layer_idx + 1
        }
    };

    // Build position map for neighbor layer
    let neighbor_positions: HashMap<LayerNode, usize> = graph.layers[neighbor_layer]
        .nodes
        .iter()
        .enumerate()
        .map(|(pos, &node)| (node, pos))
        .collect();

    // Get edges based on direction
    let edges = match direction {
        Direction::Backward => &graph.backward_edges,
        Direction::Forward => &graph.forward_edges,
    };

    // Calculate barycenters for all nodes in this layer
    let mut node_barycenters: Vec<(LayerNode, f64)> = graph.layers[layer_idx]
        .nodes
        .iter()
        .map(|&node| {
            let bc = compute_barycenter(node, &neighbor_positions, edges);
            (node, bc)
        })
        .collect();

    // Sort by barycenter (stable sort to preserve order for equal values)
    node_barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Update the layer with the new ordering
    graph.layers[layer_idx].nodes = node_barycenters.into_iter().map(|(node, _)| node).collect();
}

/// Count the number of edge crossings between two adjacent layers.
///
/// Two edges cross if and only if their source positions and target positions
/// have opposite orderings.
fn count_crossings(graph: &LayeredGraph, layer_a: usize, layer_b: usize) -> usize {
    // Collect all edges between layer_a and layer_b as (source_pos, target_pos)
    let mut edges: Vec<(usize, usize)> = Vec::new();

    // Build position map for layer_b
    let pos_b: HashMap<LayerNode, usize> = graph.layers[layer_b]
        .nodes
        .iter()
        .enumerate()
        .map(|(pos, &node)| (node, pos))
        .collect();

    for (idx_a, &node_a) in graph.layers[layer_a].nodes.iter().enumerate() {
        if let Some(targets) = graph.forward_edges.get(&node_a) {
            for &target in targets {
                if let Some(&idx_b) = pos_b.get(&target) {
                    edges.push((idx_a, idx_b));
                }
            }
        }
    }

    // Count inversions (crossings)
    // Two edges (s1, t1) and (s2, t2) cross if (s1 < s2 && t1 > t2) || (s1 > s2 && t1 < t2)
    let mut crossings = 0;
    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let (s1, t1) = edges[i];
            let (s2, t2) = edges[j];
            if (s1 < s2 && t1 > t2) || (s1 > s2 && t1 < t2) {
                crossings += 1;
            }
        }
    }

    crossings
}

// =============================================================================
// Phase 4: Coordinate Assignment
// =============================================================================

/// Assign X and Y coordinates to all nodes.
///
/// This is a simplified version of the Brandes-Köpf algorithm that:
/// 1. Places nodes at fixed column widths based on layer
/// 2. Centers columns vertically
/// 3. Attempts to align connected nodes vertically
fn assign_coordinates(
    graph: &LayeredGraph,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    let mut positions: HashMap<u64, DVec2> = HashMap::new();
    let mut dummy_positions: HashMap<LayerNode, DVec2> = HashMap::new();

    // First pass: assign positions based on layer order
    // Calculate the maximum total height across all layers for centering
    let max_layer_height = graph
        .layers
        .iter()
        .map(|layer| calculate_layer_height(layer, network, registry))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    for (layer_idx, layer) in graph.layers.iter().enumerate() {
        let x = START_X + layer_idx as f64 * COLUMN_WIDTH;

        // Calculate total height of this layer
        let layer_height = calculate_layer_height(layer, network, registry);

        // Center this layer vertically relative to the tallest layer
        let y_offset = (max_layer_height - layer_height) / 2.0;
        let mut y = START_Y + y_offset;

        for &node in &layer.nodes {
            match node {
                LayerNode::Real(id) => {
                    positions.insert(id, DVec2::new(x, y));
                    y += get_node_height(id, network, registry) + VERTICAL_GAP;
                }
                LayerNode::Dummy(_, _, _) => {
                    // Dummy nodes get positions for edge routing (stored separately)
                    dummy_positions.insert(node, DVec2::new(x, y));
                    y += VERTICAL_GAP;
                }
            }
        }
    }

    // Second pass: vertical alignment refinement
    // Try to align nodes vertically with their primary connection
    refine_vertical_alignment(&mut positions, graph, network, registry);

    positions
}

/// Calculate the total height of a layer including gaps.
fn calculate_layer_height(
    layer: &Layer,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> f64 {
    if layer.nodes.is_empty() {
        return 0.0;
    }

    let mut total_height = 0.0;
    let mut node_count = 0;

    for &node in &layer.nodes {
        match node {
            LayerNode::Real(id) => {
                total_height += get_node_height(id, network, registry);
                node_count += 1;
            }
            LayerNode::Dummy(_, _, _) => {
                // Dummy nodes contribute minimal height
                node_count += 1;
            }
        }
    }

    if node_count > 1 {
        total_height += (node_count - 1) as f64 * VERTICAL_GAP;
    }

    total_height
}

/// Refine vertical alignment by trying to align nodes with their outputs.
fn refine_vertical_alignment(
    positions: &mut HashMap<u64, DVec2>,
    graph: &LayeredGraph,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) {
    // For each layer (right to left), try to align nodes with their outputs
    for layer_idx in (0..graph.layers.len().saturating_sub(1)).rev() {
        for &node in &graph.layers[layer_idx].nodes {
            let LayerNode::Real(node_id) = node else {
                continue;
            };

            // Find the primary output (first connected node in next layer)
            let primary_output = find_primary_output(node_id, graph, layer_idx);

            if let Some(output_id) = primary_output {
                let output_pos = positions.get(&output_id).copied();
                let node_pos = positions.get(&node_id).copied();

                if let (Some(out_pos), Some(_cur_pos)) = (output_pos, node_pos) {
                    // Try to move toward the output's Y position
                    let target_y = out_pos.y;

                    // Check if we can move without overlapping
                    if can_move_to_y(
                        node_id, target_y, layer_idx, positions, graph, network, registry,
                    ) {
                        if let Some(pos) = positions.get_mut(&node_id) {
                            pos.y = target_y;
                        }
                    }
                }
            }
        }
    }
}

/// Find the primary output node for a given node (first connected node in next layer).
fn find_primary_output(node_id: u64, graph: &LayeredGraph, layer_idx: usize) -> Option<u64> {
    let node = LayerNode::Real(node_id);
    let next_layer_idx = layer_idx + 1;

    if next_layer_idx >= graph.layers.len() {
        return None;
    }

    // Get forward edges from this node
    let targets = graph.forward_edges.get(&node)?;

    // Find the first real node target in the next layer
    for &target in targets {
        if let LayerNode::Real(target_id) = target {
            // Check if this target is in the next layer
            if graph.layers[next_layer_idx].nodes.contains(&target) {
                return Some(target_id);
            }
        }
    }

    None
}

/// Check if a node can move to a new Y position without overlapping other nodes.
fn can_move_to_y(
    node_id: u64,
    target_y: f64,
    layer_idx: usize,
    positions: &HashMap<u64, DVec2>,
    graph: &LayeredGraph,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> bool {
    let node_height = get_node_height(node_id, network, registry);
    let proposed_pos = DVec2::new(
        positions.get(&node_id).map(|p| p.x).unwrap_or(START_X),
        target_y,
    );
    let proposed_size = DVec2::new(node_layout::NODE_WIDTH, node_height);

    // Check against other nodes in the same layer
    for &other in &graph.layers[layer_idx].nodes {
        if let LayerNode::Real(other_id) = other {
            if other_id == node_id {
                continue;
            }

            if let Some(&other_pos) = positions.get(&other_id) {
                let other_height = get_node_height(other_id, network, registry);
                let other_size = DVec2::new(node_layout::NODE_WIDTH, other_height);

                if node_layout::nodes_overlap(
                    proposed_pos,
                    proposed_size,
                    other_pos,
                    other_size,
                    VERTICAL_GAP,
                ) {
                    return false;
                }
            }
        }
    }

    true
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Get the estimated height of a node.
fn get_node_height(node_id: u64, network: &NodeNetwork, registry: &NodeTypeRegistry) -> f64 {
    let node = match network.nodes.get(&node_id) {
        Some(n) => n,
        None => return node_layout::estimate_node_height(0, true),
    };

    let num_params = registry
        .get_node_type(&node.node_type_name)
        .map(|nt| nt.parameters.len())
        .unwrap_or(0);

    node_layout::estimate_node_height(num_params, true)
}

// =============================================================================
// Public Utilities for Testing
// =============================================================================

/// Count the total number of edge crossings in the entire graph.
/// Useful for testing and comparing layout quality.
pub fn count_total_crossings(graph: &LayeredGraph) -> usize {
    let mut total = 0;
    for i in 0..graph.layers.len().saturating_sub(1) {
        total += count_crossings(graph, i, i + 1);
    }
    total
}

/// Create a layered graph for testing purposes.
/// Exposes the internal insert_dummy_nodes function.
pub fn create_layered_graph_for_testing(network: &NodeNetwork) -> LayeredGraph {
    let depths = compute_node_depths(network);
    let layers = group_by_depth(&depths);
    insert_dummy_nodes(&layers, network, &depths)
}
