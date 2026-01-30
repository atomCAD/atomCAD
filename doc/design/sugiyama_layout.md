# Design: Sugiyama (Layered) Layout Algorithm

## Overview

The Sugiyama algorithm is the gold standard for drawing directed acyclic graphs (DAGs). It produces hierarchical layouts with nodes organized in layers, minimized edge crossings, and clean edge routing for long-span connections.

This document details the implementation of the Sugiyama algorithm for atomCAD's node network layout system, building on the framework established in `auto_layout_algorithms.md`.

---

## When to Use Sugiyama vs. Topological Grid

### Sugiyama Excels At

| Use Case | Why Sugiyama is Better |
|----------|----------------------|
| **Complex DAGs with many cross-connections** | Crossing minimization dramatically reduces visual clutter |
| **Diamond/merge patterns** | Nodes that merge multiple inputs are positioned to minimize crossing edges |
| **Wide fan-out followed by fan-in** | Barycenter sweeping properly handles reconvergent paths |
| **Networks with long-span edges** | Dummy nodes provide clean edge routing with proper bend points |
| **Presentation/export quality** | Professional-looking layouts suitable for documentation |

### Topological Grid is Sufficient For

| Use Case | Why Topological Grid Works |
|----------|---------------------------|
| **Simple linear chains** | No crossings to minimize anyway |
| **Small networks (< 15 nodes)** | Visual complexity is manageable |
| **Quick iteration during design** | Faster layout, "good enough" results |
| **Networks with clear layered structure** | Natural separation already exists |

### Visual Comparison

**Topological Grid** (single barycenter pass):
```
       ┌───┐     ┌───┐
       │ A │────▶│ D │──┐
       └───┘     └───┘  │    ┌───┐
       ┌───┐       ╲    └───▶│   │
       │ B │────────╲────────│ F │
       └───┘         ╲       │   │
       ┌───┐     ┌───┐╲ ┌───▶│   │
       │ C │────▶│ E │──X────┘   │
       └───┘     └───┘  crossing └───┘
```

**Sugiyama** (multiple sweeps + proper ordering):
```
       ┌───┐     ┌───┐
       │ A │────▶│ D │───┐
       └───┘     └───┘   │   ┌───┐
       ┌───┐             └──▶│   │
       │ B │─────────────────│ F │
       └───┘             ┌──▶│   │
       ┌───┐     ┌───┐   │   └───┘
       │ C │────▶│ E │───┘
       └───┘     └───┘
            no crossings
```

---

## Algorithm Overview

The Sugiyama algorithm consists of four phases:

```
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: Layer Assignment                                   │
│  Assign each node to a horizontal layer (same as depth)     │
├─────────────────────────────────────────────────────────────┤
│  Phase 2: Dummy Node Insertion                               │
│  Add invisible nodes for edges spanning multiple layers     │
├─────────────────────────────────────────────────────────────┤
│  Phase 3: Crossing Minimization                              │
│  Reorder nodes within layers to minimize edge crossings     │
├─────────────────────────────────────────────────────────────┤
│  Phase 4: Coordinate Assignment (Brandes-Köpf)              │
│  Assign X/Y positions with vertical alignment               │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Layer Assignment

Layer assignment is identical to the depth computation in Topological Grid:

```rust
depth(node) = 0                           if node has no input connections
depth(node) = max(depth(inputs)) + 1      otherwise
```

This phase reuses `common::compute_node_depths()` from the shared layout utilities.

**Output**: `HashMap<NodeId, usize>` mapping each node to its layer.

---

## Phase 2: Dummy Node Insertion

### Problem

When an edge spans multiple layers (e.g., from layer 0 to layer 3), it crosses through intermediate layers. Without dummy nodes:
- The edge visually overlaps nodes in intermediate layers
- Crossing minimization cannot account for these long edges
- Edge routing has no defined path through intermediate layers

### Solution

Insert invisible "dummy nodes" at each intermediate layer:

```
Before:                          After:
Layer 0    Layer 3              Layer 0  Layer 1  Layer 2  Layer 3
┌───┐      ┌───┐                ┌───┐    ┌───┐    ┌───┐    ┌───┐
│ A │─────▶│ B │                │ A │───▶│ d1│───▶│ d2│───▶│ B │
└───┘      └───┘                └───┘    └───┘    └───┘    └───┘
                                         dummy    dummy
```

### Data Structures

```rust
/// Represents either a real node or a dummy node for edge routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerNode {
    /// A real node from the network.
    Real(u64),
    /// A dummy node for long edge routing.
    /// Contains (source_node_id, target_node_id, segment_index).
    Dummy(u64, u64, usize),
}

/// A layer containing both real and dummy nodes in order.
pub struct Layer {
    pub nodes: Vec<LayerNode>,
}

/// The layered graph with dummy nodes inserted.
pub struct LayeredGraph {
    pub layers: Vec<Layer>,
    /// Maps each layer node to its neighbors in the next layer.
    pub forward_edges: HashMap<LayerNode, Vec<LayerNode>>,
    /// Maps each layer node to its neighbors in the previous layer.
    pub backward_edges: HashMap<LayerNode, Vec<LayerNode>>,
}
```

### Algorithm

```rust
fn insert_dummy_nodes(
    layers: &[Vec<u64>],
    network: &NodeNetwork,
) -> LayeredGraph {
    let mut layered_graph = LayeredGraph::new(layers.len());

    // Add all real nodes to their layers
    for (layer_idx, layer_nodes) in layers.iter().enumerate() {
        for &node_id in layer_nodes {
            layered_graph.layers[layer_idx].nodes.push(LayerNode::Real(node_id));
        }
    }

    // For each edge, insert dummy nodes if it spans multiple layers
    for (&node_id, node) in &network.nodes {
        let target_layer = node_depths[&node_id];

        for arg in &node.arguments {
            for &source_id in arg.argument_output_pins.keys() {
                let source_layer = node_depths[&source_id];

                if target_layer - source_layer > 1 {
                    // Long edge: insert dummy nodes
                    let mut prev = LayerNode::Real(source_id);

                    for (seg_idx, layer_idx) in (source_layer + 1..target_layer).enumerate() {
                        let dummy = LayerNode::Dummy(source_id, node_id, seg_idx);
                        layered_graph.layers[layer_idx].nodes.push(dummy);
                        layered_graph.add_edge(prev, dummy);
                        prev = dummy;
                    }

                    layered_graph.add_edge(prev, LayerNode::Real(node_id));
                } else {
                    // Short edge: direct connection
                    layered_graph.add_edge(
                        LayerNode::Real(source_id),
                        LayerNode::Real(node_id),
                    );
                }
            }
        }
    }

    layered_graph
}
```

---

## Phase 3: Crossing Minimization

### Objective

Minimize the number of edge crossings by reordering nodes within each layer.

### Algorithm: Iterative Barycenter Method

The algorithm sweeps through layers, repositioning nodes based on the average (barycenter) position of their neighbors:

```rust
fn minimize_crossings(graph: &mut LayeredGraph) {
    let mut improved = true;

    while improved {
        improved = false;

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
    }
}
```

### Barycenter Calculation

For each node, compute the average position of its connected neighbors:

```rust
fn compute_barycenter(
    node: LayerNode,
    neighbor_positions: &HashMap<LayerNode, usize>,
    edges: &HashMap<LayerNode, Vec<LayerNode>>,
) -> f64 {
    let neighbors = match edges.get(&node) {
        Some(n) if !n.is_empty() => n,
        _ => return f64::MAX, // No neighbors, sort to end
    };

    let sum: usize = neighbors
        .iter()
        .filter_map(|n| neighbor_positions.get(n))
        .sum();

    sum as f64 / neighbors.len() as f64
}

fn reorder_by_barycenter(
    graph: &mut LayeredGraph,
    layer_idx: usize,
    direction: Direction,
) {
    let neighbor_layer = match direction {
        Direction::Backward => layer_idx - 1,
        Direction::Forward => layer_idx + 1,
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

    // Sort current layer by barycenter
    graph.layers[layer_idx].nodes.sort_by(|&a, &b| {
        let a_bc = compute_barycenter(a, &neighbor_positions, edges);
        let b_bc = compute_barycenter(b, &neighbor_positions, edges);
        a_bc.partial_cmp(&b_bc).unwrap_or(std::cmp::Ordering::Equal)
    });
}
```

### Counting Crossings

Two edges cross if and only if their source positions and target positions have opposite orderings:

```rust
fn count_crossings(
    graph: &LayeredGraph,
    layer_a: usize,
    layer_b: usize,
) -> usize {
    // Collect all edges between layer_a and layer_b as (source_pos, target_pos)
    let mut edges: Vec<(usize, usize)> = Vec::new();

    for (pos_a, &node_a) in graph.layers[layer_a].nodes.iter().enumerate() {
        if let Some(targets) = graph.forward_edges.get(&node_a) {
            for &target in targets {
                if let Some(pos_b) = graph.layers[layer_b]
                    .nodes
                    .iter()
                    .position(|&n| n == target)
                {
                    edges.push((pos_a, pos_b));
                }
            }
        }
    }

    // Count inversions (crossings)
    let mut crossings = 0;
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            let (s1, t1) = edges[i];
            let (s2, t2) = edges[j];
            // Crossing occurs when (s1 < s2 && t1 > t2) || (s1 > s2 && t1 < t2)
            if (s1 < s2 && t1 > t2) || (s1 > s2 && t1 < t2) {
                crossings += 1;
            }
        }
    }

    crossings
}
```

### Convergence

The algorithm iterates until a complete down-sweep and up-sweep produces no improvement. In practice, most graphs converge within 4-8 iterations.

---

## Phase 4: Coordinate Assignment (Brandes-Köpf)

### Objective

Assign X and Y coordinates to nodes such that:
1. Nodes in the same layer have the same X coordinate
2. Connected nodes are vertically aligned when possible
3. The layout is compact without overlaps

### Algorithm Overview

The Brandes-Köpf algorithm produces compact layouts by:
1. **Type 1 conflict resolution**: Handling edges that would cause vertical alignment conflicts
2. **Vertical alignment**: Grouping nodes into "blocks" that share the same Y coordinate
3. **Horizontal compaction**: Assigning X coordinates to minimize width

### Simplified Implementation

For the initial implementation, we use a simplified version that captures the key benefits:

```rust
fn assign_coordinates(
    graph: &LayeredGraph,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    const START_X: f64 = 100.0;
    const START_Y: f64 = 100.0;
    const COLUMN_WIDTH: f64 = 250.0;
    const VERTICAL_GAP: f64 = 30.0;

    let mut positions: HashMap<u64, DVec2> = HashMap::new();

    // First pass: assign positions based on layer order
    for (layer_idx, layer) in graph.layers.iter().enumerate() {
        let x = START_X + layer_idx as f64 * COLUMN_WIDTH;

        // Calculate total height needed for this layer
        let total_height: f64 = layer.nodes.iter()
            .filter_map(|node| match node {
                LayerNode::Real(id) => Some(get_node_height(*id, network, registry) + VERTICAL_GAP),
                LayerNode::Dummy(_, _, _) => Some(VERTICAL_GAP), // Dummy nodes are just routing points
            })
            .sum();

        let mut y = START_Y;

        for &node in &layer.nodes {
            match node {
                LayerNode::Real(id) => {
                    positions.insert(id, DVec2::new(x, y));
                    y += get_node_height(id, network, registry) + VERTICAL_GAP;
                }
                LayerNode::Dummy(_, _, _) => {
                    // Dummy nodes don't get final positions; they're for routing
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
```

### Vertical Alignment Refinement

```rust
fn refine_vertical_alignment(
    positions: &mut HashMap<u64, DVec2>,
    graph: &LayeredGraph,
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) {
    // For each layer (right to left), try to align nodes with their outputs
    for layer_idx in (0..graph.layers.len() - 1).rev() {
        for &node in &graph.layers[layer_idx].nodes {
            let LayerNode::Real(node_id) = node else { continue };

            // Find the primary output (node in next layer that uses this node's output)
            let primary_output = find_primary_output(node_id, graph, layer_idx);

            if let Some(output_id) = primary_output {
                let output_pos = positions.get(&output_id).copied();
                let node_pos = positions.get(&node_id).copied();

                if let (Some(out_y), Some(cur_y)) = (output_pos.map(|p| p.y), node_pos.map(|p| p.y)) {
                    // Try to move toward the output's Y position
                    let target_y = out_y;

                    // Check if we can move without overlapping
                    if can_move_to_y(node_id, target_y, layer_idx, positions, graph, network, registry) {
                        if let Some(pos) = positions.get_mut(&node_id) {
                            pos.y = target_y;
                        }
                    }
                }
            }
        }
    }
}
```

### Full Brandes-Köpf (Future Enhancement)

The complete Brandes-Köpf algorithm involves four separate layout calculations (upper-left, upper-right, lower-left, lower-right) that are then combined. This provides optimal compactness but adds significant complexity.

For the initial implementation, the simplified version above provides the key benefit (vertical alignment) without the full complexity. The complete algorithm can be added later if needed.

---

## Handling Disconnected Components

When a network contains multiple disconnected components (subgraphs with no edges between them), each component is laid out independently and then arranged together.

### Strategy

1. **Identify components**: Use depth-first search to find connected components
2. **Sort by size**: Order components by node count (largest first)
3. **Layout each independently**: Apply the full Sugiyama algorithm to each component
4. **Stack vertically**: Place components top-to-bottom with gaps between them

### Visual Example

```
┌─────────────────────────────────────────────────┐
│  Component 1 (largest - 8 nodes)                │
│  ┌───┐     ┌───┐     ┌───┐                      │
│  │ A │────▶│ C │────▶│ E │                      │
│  └───┘     └───┘     └───┘                      │
│  ┌───┐     ┌───┐     ┌───┐     ┌───┐     ┌───┐ │
│  │ B │────▶│ D │────▶│ F │────▶│ G │────▶│ H │ │
│  └───┘     └───┘     └───┘     └───┘     └───┘ │
├─────────────────────────────────────────────────┤
│  Component 2 (3 nodes)                          │
│  ┌───┐     ┌───┐     ┌───┐                      │
│  │ X │────▶│ Y │────▶│ Z │                      │
│  └───┘     └───┘     └───┘                      │
├─────────────────────────────────────────────────┤
│  Component 3 (1 node)                           │
│  ┌───┐                                          │
│  │ W │                                          │
│  └───┘                                          │
└─────────────────────────────────────────────────┘
```

### Algorithm

```rust
fn find_connected_components(network: &NodeNetwork) -> Vec<HashSet<u64>> {
    let mut visited: HashSet<u64> = HashSet::new();
    let mut components: Vec<HashSet<u64>> = Vec::new();

    for &node_id in network.nodes.keys() {
        if visited.contains(&node_id) {
            continue;
        }

        // BFS/DFS to find all nodes in this component
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
                    if arg.argument_output_pins.contains_key(&current) {
                        if !component.contains(&other_id) {
                            queue.push_back(other_id);
                        }
                    }
                }
            }
        }

        components.push(component);
    }

    // Sort by size (largest first)
    components.sort_by(|a, b| b.len().cmp(&a.len()));

    components
}

fn layout_with_components(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    const COMPONENT_GAP: f64 = 80.0;

    let components = find_connected_components(network);

    if components.len() == 1 {
        // Single component: use standard layout
        return layout_single_component(network, registry);
    }

    let mut all_positions: HashMap<u64, DVec2> = HashMap::new();
    let mut current_y: f64 = START_Y;

    for component_nodes in &components {
        // Create a sub-network containing only this component
        let sub_network = extract_subnetwork(network, component_nodes);

        // Layout this component
        let component_positions = layout_single_component(&sub_network, registry);

        // Find the bounding box of this component
        let (min_y, max_y) = component_bounding_box(&component_positions, &sub_network, registry);
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
```

### Integration with Main Layout

The component handling wraps the core Sugiyama algorithm:

```rust
// In sugiyama.rs - main entry point
pub fn layout(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    // Handle disconnected components
    layout_with_components(network, registry)
}
```

---

## Edge Routing

### Bend Points for Long Edges

Dummy nodes provide natural bend points for edges spanning multiple layers:

```rust
fn compute_edge_path(
    source_id: u64,
    target_id: u64,
    graph: &LayeredGraph,
    positions: &HashMap<u64, DVec2>,
    dummy_positions: &HashMap<LayerNode, DVec2>,
) -> Vec<DVec2> {
    let mut path = vec![positions[&source_id]];

    // Find dummy nodes for this edge
    let source_layer = get_layer(LayerNode::Real(source_id), graph);
    let target_layer = get_layer(LayerNode::Real(target_id), graph);

    for layer_idx in (source_layer + 1)..target_layer {
        for (seg_idx, &node) in graph.layers[layer_idx].nodes.iter().enumerate() {
            if let LayerNode::Dummy(s, t, _) = node {
                if s == source_id && t == target_id {
                    path.push(dummy_positions[&node]);
                }
            }
        }
    }

    path.push(positions[&target_id]);
    path
}
```

### Rendering Bend Points

The renderer draws edges as polylines through the bend points:

```
┌───────┐                           ┌───────┐
│ Source│──●──────●──────●─────────▶│Target │
└───────┘  │      │      │          └───────┘
           │      │      │
        layer 1  layer 2  layer 3
        (dummy)  (dummy)  (dummy)
```

---

## Implementation Structure

```
rust/src/structure_designer/layout/
├── mod.rs                 # Public API, algorithm dispatch
├── common.rs              # Shared utilities
├── topological_grid.rs    # Existing simple algorithm
└── sugiyama.rs            # This algorithm
    ├── LayerNode enum
    ├── LayeredGraph struct
    ├── insert_dummy_nodes()
    ├── minimize_crossings()
    ├── assign_coordinates()
    └── layout() - main entry point
```

### Public API

```rust
// In sugiyama.rs
pub fn layout(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    // Phase 1: Layer assignment (reuse common)
    let depths = common::compute_node_depths(network);
    let layers = group_by_depth(&depths);

    // Phase 2: Dummy node insertion
    let mut graph = insert_dummy_nodes(&layers, network, &depths);

    // Phase 3: Crossing minimization
    minimize_crossings(&mut graph);

    // Phase 4: Coordinate assignment
    assign_coordinates(&graph, network, registry)
}
```

---

## Testing Strategy

### Unit Tests

1. **Dummy node insertion**
   - Edge spanning 2 layers → 0 dummy nodes
   - Edge spanning 3 layers → 1 dummy node
   - Edge spanning N layers → N-2 dummy nodes

2. **Crossing detection**
   - Two parallel edges → 0 crossings
   - Two crossing edges → 1 crossing
   - Complex patterns with known crossing counts

3. **Barycenter calculation**
   - Node with one neighbor
   - Node with multiple neighbors
   - Node with no neighbors (should sort to end)

4. **Crossing minimization**
   - Simple 2-layer graph with obvious optimal ordering
   - Diamond pattern should have 0 crossings after minimization

### Integration Tests

1. **Full layout roundtrip**
   - Create network via text format
   - Apply Sugiyama layout
   - Verify no overlapping nodes
   - Verify crossing count is minimized

2. **Comparison with Topological Grid**
   - Same network laid out with both algorithms
   - Sugiyama should have ≤ crossings

3. **Edge cases**
   - Empty network
   - Single node
   - Linear chain (should match topological grid)
   - Disconnected components


---

## Performance Considerations

### Complexity

| Phase | Time Complexity | Space Complexity |
|-------|----------------|------------------|
| Layer assignment | O(V + E) | O(V) |
| Dummy insertion | O(E × max_span) | O(V + dummies) |
| Crossing minimization | O(iterations × layers × V²) | O(V) |
| Coordinate assignment | O(V) | O(V) |

Where:
- V = number of nodes
- E = number of edges
- max_span = maximum edge span (layers crossed)
- iterations = number of sweeps until convergence (typically 4-8)

### Practical Performance

For typical atomCAD networks (10-100 nodes):
- Layout completes in < 10ms
- Memory overhead is minimal

For very large networks (1000+ nodes):
- Consider capping iterations if needed
- May want to add progress indication for UI

---

## Future Enhancements

### Full Brandes-Köpf

Implement the complete four-pass coordinate assignment for optimal compactness.

### Incremental Updates

When nodes are added/removed, update the layout incrementally rather than recomputing from scratch.

### Port-Aware Layout

Consider specific input/output pin positions when calculating edge routing and node alignment.

### Constraint Support

Allow users to pin certain nodes to fixed positions while laying out the rest.

---

## References

- Sugiyama, K., Tagawa, S., & Toda, M. (1981). Methods for visual understanding of hierarchical system structures.
- Brandes, U., & Köpf, B. (2001). Fast and simple horizontal coordinate assignment.
- Graphviz dot layout algorithm (practical implementation reference)
