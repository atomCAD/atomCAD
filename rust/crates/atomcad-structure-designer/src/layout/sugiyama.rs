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

use crate::node_layout;
use crate::node_network::{NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;

use super::common::{
    COLUMN_GAP, START_X, START_Y, VERTICAL_GAP, compute_node_depths_within, graph_node_ids,
    place_comments,
};

// Layout constants
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

/// Where along a node's side a wire meets it, as a fraction of the way down
/// that side, in `[0, 1)`.
///
/// Crossing reduction measures a wire end as *layer position + port*, so two
/// wires into one node are ordered like the pins they land on. Counting
/// between nodes alone, both orders of two producers feeding `diff.base` and
/// `diff.sub` score zero crossings, and which one ends up on top is an
/// accident; drawn, the wrong one crosses.
pub type Port = f64;

/// The port of a dummy node, which has a single pass-through point.
const DUMMY_PORT: Port = 0.5;

/// One edge of the layered graph, with the ports at both ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerEdge {
    pub from: LayerNode,
    pub to: LayerNode,
    /// On `from`'s output side.
    pub from_port: Port,
    /// On `to`'s input side.
    pub to_port: Port,
}

/// The layered graph with dummy nodes inserted.
#[derive(Debug)]
pub struct LayeredGraph {
    pub layers: Vec<Layer>,
    /// Maps each layer node to its edges into the next layer.
    pub forward_edges: HashMap<LayerNode, Vec<LayerEdge>>,
    /// Maps each layer node to its edges from the previous layer.
    pub backward_edges: HashMap<LayerNode, Vec<LayerEdge>>,
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
    pub fn add_edge(&mut self, edge: LayerEdge) {
        self.forward_edges.entry(edge.from).or_default().push(edge);
        self.backward_edges.entry(edge.to).or_default().push(edge);
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

    let all: HashSet<u64> = network.nodes.keys().copied().collect();
    let mut positions = layout_subgraph(network, registry, &all);

    // Comments are not part of any component (D8); place them against the
    // laid-out graph (`doc/design_wire_annotations.md` D9). This is the full
    // reflow's job alone — the incremental pass places a *new* comment by its
    // own rule and never re-places an existing one
    // (`doc/design_incremental_layout.md` D9), which is why `layout_subgraph`
    // stops short of it.
    place_comments(network, registry, &mut positions);

    positions
}

/// The same algorithm restricted to a subgraph: only the nodes in `ids` are
/// placed, and only the wires **between** them are edges.
///
/// This is how Step 3 of the incremental pass lays a block of freshly added
/// nodes out in isolation (`doc/design_incremental_layout.md` D2). The
/// restriction runs all the way down — depths
/// ([`compute_node_depths_within`](super::common::compute_node_depths_within)),
/// components, dummy nodes, barycenters — so a block inherits nothing from the
/// drawing it is about to be fitted into, not even a column index.
///
/// The returned positions are in the algorithm's own frame, starting at
/// ([`START_X`], [`START_Y`]); the caller translates them where it wants them.
/// Comments are excluded, as they are from every layer assignment (D8).
pub fn layout_subgraph(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    ids: &HashSet<u64>,
) -> HashMap<u64, DVec2> {
    if ids.is_empty() {
        return HashMap::new();
    }

    // Find connected components and lay them out separately
    let components = find_connected_components(network, ids);

    if components.len() == 1 {
        // Single component: use standard layout
        layout_single_component(network, registry, &components[0], ids)
    } else {
        // Multiple components (or none at all, in a comment-only network): lay
        // out each and stack vertically
        layout_with_components(network, registry, &components, ids)
    }
}

// =============================================================================
// Connected Components
// =============================================================================

/// Find all connected components in the network.
///
/// Returns a vector of HashSets, each containing the node IDs in a component.
/// Components are sorted by size (largest first).
///
/// **Comment nodes are excluded entirely** (`doc/design_wire_annotations.md`
/// D8). Having no wires, each would otherwise form its own singleton component
/// and be stacked below the whole graph — and, tying on size against every other
/// singleton, in an order that comes from `HashMap` iteration and so differs per
/// process. `place_comments` positions them after the graph is laid out instead.
fn find_connected_components(network: &NodeNetwork, ids: &HashSet<u64>) -> Vec<HashSet<u64>> {
    let graph_nodes: HashSet<u64> = graph_node_ids(network)
        .into_iter()
        .filter(|id| ids.contains(id))
        .collect();
    let mut visited: HashSet<u64> = HashSet::new();
    let mut components: Vec<HashSet<u64>> = Vec::new();

    // Seed the BFS in ascending id order, never in `HashMap` order
    // (`doc/design_incremental_layout.md` D7). The component list is sorted by
    // size below, and `sort_by_key` is stable, so two equal-size components
    // kept the order they were discovered in — which was per-process random.
    let mut seed_ids: Vec<u64> = ids.iter().copied().collect();
    seed_ids.sort_unstable();

    for node_id in seed_ids {
        if visited.contains(&node_id) || !graph_nodes.contains(&node_id) {
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
                // Input connections. Cross-scope wires are not edges of this
                // graph: `$element` names a node in the *parent* scope, and ids
                // are unique per network, so the number can collide with a
                // node's here.
                for arg in &node.arguments {
                    for wire in &arg.incoming_wires {
                        if wire.source_scope_depth != 0 {
                            continue;
                        }
                        let source_id = wire.source_node_id;
                        if !component.contains(&source_id) && graph_nodes.contains(&source_id) {
                            queue.push_back(source_id);
                        }
                    }
                }
            }

            // Output connections (nodes that use this node as input)
            for (&other_id, other_node) in &network.nodes {
                if !graph_nodes.contains(&other_id) {
                    continue;
                }
                for arg in &other_node.arguments {
                    if arg.has_source(current) && !component.contains(&other_id) {
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
    ids: &HashSet<u64>,
) -> HashMap<u64, DVec2> {
    let mut all_positions: HashMap<u64, DVec2> = HashMap::new();
    let mut current_y: f64 = START_Y;

    for component_nodes in components {
        // Layout this component
        let component_positions = layout_single_component(network, registry, component_nodes, ids);

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
        let height = super::common::node_height(node_id, network, registry);
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
    ids: &HashSet<u64>,
) -> HashMap<u64, DVec2> {
    // Phase 1: Layer assignment (compute depths), over the subgraph alone
    let depths = compute_node_depths_within(network, ids);

    // Filter to only nodes in this component
    let component_depths: HashMap<u64, usize> = depths
        .into_iter()
        .filter(|(id, _)| component_nodes.contains(id))
        .collect();

    // Group nodes by depth into layers
    let layers = group_by_depth(&component_depths);

    // Phase 2: Dummy node insertion
    let mut graph = insert_dummy_nodes(&layers, network, registry, &component_depths);

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
///
/// Every edge carries the [`Port`]s of the pins it joins; a dummy's are
/// [`DUMMY_PORT`].
fn insert_dummy_nodes(
    layers: &[Vec<u64>],
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
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

    // For each edge, insert dummy nodes if it spans multiple layers.
    //
    // Targets are visited in ascending id order, never in `HashMap` order
    // (`doc/design_incremental_layout.md` D7): each node's edge lists come out
    // in this order, `find_primary_output` takes the first, and dummies join
    // their layers in it, so `HashMap` order would decide where a node is
    // aligned, per process.
    let mut target_ids: Vec<u64> = network.nodes.keys().copied().collect();
    target_ids.sort_unstable();

    for node_id in target_ids {
        let node = &network.nodes[&node_id];
        let Some(&target_layer) = depths.get(&node_id) else {
            continue;
        };

        let arg_count = node.arguments.len();
        for (arg_index, arg) in node.arguments.iter().enumerate() {
            let to_port = slot_port(arg_index, arg_count);
            for wire in &arg.incoming_wires {
                if wire.source_scope_depth != 0 {
                    continue; // not an edge of this graph; see the component scan
                }
                let source_id = wire.source_node_id;
                let Some(&source_layer) = depths.get(&source_id) else {
                    continue;
                };

                // Ensure source_layer < target_layer (edges go forward)
                if source_layer >= target_layer {
                    continue;
                }

                let from_port = output_port(network, registry, source_id, &wire.source_pin);

                if target_layer - source_layer > 1 {
                    // Long edge: insert dummy nodes
                    let mut prev = LayerNode::Real(source_id);
                    let mut prev_port = from_port;

                    for (seg_idx, layer_idx) in (source_layer + 1..target_layer).enumerate() {
                        let dummy = LayerNode::Dummy(source_id, node_id, seg_idx);
                        graph.layers[layer_idx].nodes.push(dummy);
                        graph.add_edge(LayerEdge {
                            from: prev,
                            to: dummy,
                            from_port: prev_port,
                            to_port: DUMMY_PORT,
                        });
                        prev = dummy;
                        prev_port = DUMMY_PORT;
                    }

                    graph.add_edge(LayerEdge {
                        from: prev,
                        to: LayerNode::Real(node_id),
                        from_port: prev_port,
                        to_port,
                    });
                } else {
                    // Short edge: direct connection
                    graph.add_edge(LayerEdge {
                        from: LayerNode::Real(source_id),
                        to: LayerNode::Real(node_id),
                        from_port,
                        to_port,
                    });
                }
            }
        }
    }

    graph
}

/// The port of slot `index` out of `count` slots stacked down one side, at the
/// slot's middle.
fn slot_port(index: usize, count: usize) -> Port {
    (index as f64 + 0.5) / count.max(index + 1) as f64
}

/// The port of the output pin a wire leaves from. The function pin (`-1`) is
/// in the title bar, above every output; a zone-input pin sits inside a body
/// and never leaves on this side, so it takes the middle.
fn output_port(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    node_id: u64,
    pin: &SourcePin,
) -> Port {
    match *pin {
        SourcePin::NodeOutput { pin_index } if pin_index < 0 => 0.0,
        SourcePin::NodeOutput { pin_index } => {
            let count = network
                .nodes
                .get(&node_id)
                .and_then(|node| registry.get_node_type_for_node(node))
                .map_or(1, |node_type| node_type.output_pins.len());
            slot_port(pin_index as usize, count)
        }
        SourcePin::ZoneInput { .. } => DUMMY_PORT,
    }
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
///
/// A neighbor's position is its index in its layer plus the [`Port`] the edge
/// meets it at, so the barycenter sees which pin a wire lands on.
fn compute_barycenter(
    node: LayerNode,
    neighbor_positions: &HashMap<LayerNode, usize>,
    edges: &HashMap<LayerNode, Vec<LayerEdge>>,
    direction: Direction,
) -> f64 {
    let edges = match edges.get(&node) {
        Some(e) if !e.is_empty() => e,
        _ => return f64::MAX, // No neighbors, sort to end
    };

    let positions: Vec<f64> = edges
        .iter()
        .filter_map(|edge| {
            let (neighbor, port) = match direction {
                Direction::Backward => (edge.from, edge.from_port),
                Direction::Forward => (edge.to, edge.to_port),
            };
            neighbor_positions
                .get(&neighbor)
                .map(|&pos| pos as f64 + port)
        })
        .collect();

    if positions.is_empty() {
        return f64::MAX;
    }

    positions.iter().sum::<f64>() / positions.len() as f64
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
            let bc = compute_barycenter(node, &neighbor_positions, edges, direction);
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
/// Two edges cross if and only if their source ends and target ends have
/// opposite orderings. An end is its node's index in the layer plus its
/// [`Port`], so two wires into different pins of one node are ordered too;
/// two wires sharing an end (one pin, or one output fanning out) never cross.
fn count_crossings(graph: &LayeredGraph, layer_a: usize, layer_b: usize) -> usize {
    // Collect all edges between layer_a and layer_b as (source end, target end)
    let mut edges: Vec<(f64, f64)> = Vec::new();

    // Build position map for layer_b
    let pos_b: HashMap<LayerNode, usize> = graph.layers[layer_b]
        .nodes
        .iter()
        .enumerate()
        .map(|(pos, &node)| (node, pos))
        .collect();

    for (idx_a, &node_a) in graph.layers[layer_a].nodes.iter().enumerate() {
        if let Some(out_edges) = graph.forward_edges.get(&node_a) {
            for edge in out_edges {
                if let Some(&idx_b) = pos_b.get(&edge.to) {
                    edges.push((idx_a as f64 + edge.from_port, idx_b as f64 + edge.to_port));
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

    // Per-layer column x. A layer is as wide as its widest member, so an
    // expanded HOF (460 px and up) no longer overhangs the next column the way
    // a fixed `COLUMN_WIDTH` pitch made it
    // (`doc/design_incremental_layout.md`, "Prerequisite: one size function").
    let column_x = column_positions(graph, network, registry);

    for (layer_idx, layer) in graph.layers.iter().enumerate() {
        let x = column_x[layer_idx];

        // Calculate total height of this layer
        let layer_height = calculate_layer_height(layer, network, registry);

        // Center this layer vertically relative to the tallest layer
        let y_offset = (max_layer_height - layer_height) / 2.0;
        let mut y = START_Y + y_offset;

        for &node in &layer.nodes {
            match node {
                LayerNode::Real(id) => {
                    positions.insert(id, DVec2::new(x, y));
                    y += super::common::node_height(id, network, registry) + VERTICAL_GAP;
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

/// The x coordinate of every layer, each column sized by its widest member.
///
/// `x[0] = START_X`; `x[k+1] = x[k] + width(k) + COLUMN_GAP`, where `width(k)`
/// is the widest **real** node in layer `k`, never below `NODE_WIDTH` so a
/// layer of dummies alone keeps the ordinary pitch. On a network of uniform
/// 160 px nodes this reproduces the old `START_X + k * COLUMN_WIDTH` exactly.
fn column_positions(
    graph: &LayeredGraph,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Vec<f64> {
    let mut xs = Vec::with_capacity(graph.layers.len());
    let mut x = START_X;
    for layer in &graph.layers {
        xs.push(x);
        let width = layer
            .nodes
            .iter()
            .filter_map(|node| node.real_id())
            .map(|id| super::common::node_width(id, network, registry))
            .fold(node_layout::NODE_WIDTH, f64::max);
        x += width + COLUMN_GAP;
    }
    xs
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
                total_height += super::common::node_height(id, network, registry);
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
                    ) && let Some(pos) = positions.get_mut(&node_id)
                    {
                        pos.y = target_y;
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
    let edges = graph.forward_edges.get(&node)?;

    // Find the first real node target in the next layer
    for edge in edges {
        let target = edge.to;
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
    let proposed_pos = DVec2::new(
        positions.get(&node_id).map(|p| p.x).unwrap_or(START_X),
        target_y,
    );
    let proposed_size = super::common::node_size(node_id, network, registry);

    // Check against other nodes in the same layer
    for &other in &graph.layers[layer_idx].nodes {
        if let LayerNode::Real(other_id) = other {
            if other_id == node_id {
                continue;
            }

            if let Some(&other_pos) = positions.get(&other_id) {
                let other_size = super::common::node_size(other_id, network, registry);

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
pub fn create_layered_graph_for_testing(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> LayeredGraph {
    let all: HashSet<u64> = network.nodes.keys().copied().collect();
    let depths = compute_node_depths_within(network, &all);
    let layers = group_by_depth(&depths);
    insert_dummy_nodes(&layers, network, registry, &depths)
}
