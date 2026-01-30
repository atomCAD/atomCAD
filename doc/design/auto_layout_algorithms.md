# Design: Auto-Layout Algorithms for Node Networks

## Overview

atomCAD supports automatic layout of node networks to improve readability and visual organization. This is particularly important for networks created programmatically through `atomcad-cli`, where node positions must be calculated automatically.

This document describes the layout algorithm framework and the initial **Topological Grid Layout** algorithm. Future algorithms (Layered/Sugiyama and Incremental) are briefly mentioned and will be detailed in separate design documents.

---

## Problem Statement

When AI agents create node networks through `atomcad-cli`, they don't specify node positions. The current auto-layout algorithm positions nodes incrementally as they are created, resulting in poor layouts:

- Source nodes (no inputs) spread horizontally in a single row
- Networks become extremely wide with no vertical organization
- No consideration of the DAG structure or edge crossings
- Related nodes are not visually grouped

A better approach is to lay out the **entire network** after all nodes are created, using graph layout algorithms designed for directed acyclic graphs (DAGs).

---

## Layout Algorithm Framework

### Architecture

Multiple layout algorithms are supported through a common interface:

```rust
/// Trait for layout algorithms that can position nodes in a network.
pub trait LayoutAlgorithm {
    /// Compute positions for all nodes in the network.
    ///
    /// Returns a map from node ID to new position.
    fn layout(&self, network: &NodeNetwork, registry: &NodeTypeRegistry) -> HashMap<u64, DVec2>;

    /// Human-readable name for this algorithm.
    fn name(&self) -> &'static str;
}
```

### Available Algorithms

| Algorithm | Use Case | Status |
|-----------|----------|--------|
| **Topological Grid** | AI-created networks, general purpose | Implemented |
| **Layered/Sugiyama** | Complex DAGs requiring minimal edge crossings | Planned |
| **Incremental** | User-edited networks where layout should be preserved | Planned |

### Algorithm Selection

The layout algorithm can be selected:

1. **Programmatically**: When calling layout functions from Rust code
2. **Via CLI**: Using `atomcad-cli` flags (e.g., `--layout=topological-grid`)
3. **Automatically**: Based on heuristics about the network's origin and structure

---

## Algorithm 1: Topological Grid Layout

The Topological Grid Layout is a simple, deterministic algorithm that produces clean, readable layouts for most DAG structures.

### Algorithm Description

**Input**: A `NodeNetwork` containing nodes with connections (arguments)

**Output**: New positions for all nodes, organized in columns by topological depth

### Step 1: Compute Node Depths

Each node is assigned a "depth" based on its position in the dependency graph:

```
depth(node) = 0                           if node has no input connections
depth(node) = max(depth(inputs)) + 1      otherwise
```

This ensures that:
- Source nodes (primitives, literals) are at depth 0
- Each node appears to the right of all its dependencies
- The final output/return node has the highest depth

```rust
fn compute_depths(network: &NodeNetwork) -> HashMap<u64, usize> {
    let mut depths: HashMap<u64, usize> = HashMap::new();
    let mut visited: HashSet<u64> = HashSet::new();

    fn visit(
        node_id: u64,
        network: &NodeNetwork,
        depths: &mut HashMap<u64, usize>,
        visited: &mut HashSet<u64>,
    ) -> usize {
        if let Some(&depth) = depths.get(&node_id) {
            return depth;
        }

        if visited.contains(&node_id) {
            return 0; // Cycle detected, treat as source
        }
        visited.insert(node_id);

        let node = match network.nodes.get(&node_id) {
            Some(n) => n,
            None => return 0,
        };

        // Find maximum depth of all input connections
        let max_input_depth = node.arguments
            .iter()
            .flat_map(|arg| arg.argument_output_pins.keys())
            .map(|&source_id| visit(source_id, network, depths, visited))
            .max()
            .unwrap_or(0);

        let depth = if node.arguments.iter().all(|a| a.argument_output_pins.is_empty()) {
            0  // No inputs = source node
        } else {
            max_input_depth + 1
        };

        depths.insert(node_id, depth);
        depth
    }

    for &node_id in network.nodes.keys() {
        visit(node_id, network, &mut depths, &mut visited);
    }

    depths
}
```

### Step 2: Group Nodes by Depth (Columns)

Nodes are grouped into columns based on their computed depth:

```rust
fn group_by_depth(depths: &HashMap<u64, usize>) -> Vec<Vec<u64>> {
    let max_depth = depths.values().copied().max().unwrap_or(0);
    let mut columns: Vec<Vec<u64>> = vec![vec![]; max_depth + 1];

    for (&node_id, &depth) in depths {
        columns[depth].push(node_id);
    }

    columns
}
```

### Step 3: Order Nodes Within Columns

Nodes within each column are sorted to minimize edge crossings. A simple heuristic is to sort by the average Y position of connected nodes in the previous column (barycenter method):

```rust
fn order_column(
    column: &mut Vec<u64>,
    prev_column_positions: &HashMap<u64, f64>,  // node_id -> y position
    network: &NodeNetwork,
) {
    column.sort_by(|&a, &b| {
        let a_barycenter = compute_barycenter(a, prev_column_positions, network);
        let b_barycenter = compute_barycenter(b, prev_column_positions, network);
        a_barycenter.partial_cmp(&b_barycenter).unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn compute_barycenter(
    node_id: u64,
    prev_positions: &HashMap<u64, f64>,
    network: &NodeNetwork,
) -> f64 {
    let node = match network.nodes.get(&node_id) {
        Some(n) => n,
        None => return 0.0,
    };

    let input_positions: Vec<f64> = node.arguments
        .iter()
        .flat_map(|arg| arg.argument_output_pins.keys())
        .filter_map(|&source_id| prev_positions.get(&source_id).copied())
        .collect();

    if input_positions.is_empty() {
        0.0  // No connections to previous column
    } else {
        input_positions.iter().sum::<f64>() / input_positions.len() as f64
    }
}
```

### Step 4: Assign Coordinates

Finally, X and Y coordinates are assigned based on column and position within column:

```rust
fn assign_positions(
    columns: &[Vec<u64>],
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    const START_X: f64 = 100.0;
    const START_Y: f64 = 100.0;
    const COLUMN_WIDTH: f64 = 250.0;  // NODE_WIDTH + horizontal gap
    const VERTICAL_GAP: f64 = 30.0;

    let mut positions: HashMap<u64, DVec2> = HashMap::new();

    for (col_index, column) in columns.iter().enumerate() {
        let x = START_X + col_index as f64 * COLUMN_WIDTH;

        // Calculate total height of this column
        let total_height: f64 = column.iter()
            .map(|&id| get_node_height(id, network, registry) + VERTICAL_GAP)
            .sum();

        // Center column vertically
        let mut y = START_Y + (columns.iter().map(|c| /* max column height */).max() - total_height) / 2.0;

        for &node_id in column {
            positions.insert(node_id, DVec2::new(x, y));
            y += get_node_height(node_id, network, registry) + VERTICAL_GAP;
        }
    }

    positions
}
```

### Visual Example

**Before** (current incremental algorithm):
```
┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐ ┌───┐
│box│ │sph│ │cyl│ │uni│ │fil│ │tra│ │ret│
└───┘ └───┘ └───┘ └───┘ └───┘ └───┘ └───┘
  └─────┴─────┴─────┘     │     │     │
          └───────────────┘     │     │
                └───────────────┘     │
                        └─────────────┘
```

**After** (Topological Grid Layout):
```
Column 0      Column 1      Column 2      Column 3
┌───────┐     ┌───────┐     ┌───────┐     ┌───────┐
│ box   │────▶│       │     │       │     │       │
└───────┘     │ union │────▶│ fillet│────▶│return │
┌───────┐     │       │     │       │     │       │
│sphere │────▶│       │     └───────┘     └───────┘
└───────┘     └───────┘
┌───────┐         │
│ cyl   │─────────┘
└───────┘
```

### Complexity

- **Time**: O(V + E) for depth computation, O(V log V) for sorting within columns
- **Space**: O(V) for storing depths and positions

### Limitations

- Does not minimize edge crossings optimally (uses simple barycenter heuristic)
- All nodes in a column have the same X position (no staggering)
- Does not handle very wide columns gracefully

---

## Future Algorithms

### Layered/Sugiyama Layout

The Sugiyama algorithm is the gold standard for DAG visualization. It follows the same conceptual framework as Topological Grid (layer assignment → node ordering → coordinate assignment) but with sophisticated techniques at each phase:

1. **Dummy nodes**: Inserts invisible nodes for edges spanning multiple layers, enabling proper edge routing
2. **Crossing minimization**: Multiple iteration passes (sweep down, sweep up, repeat) until edge crossings stabilize
3. **Coordinate assignment**: Algorithms like Brandes-Köpf for compact horizontal positioning
4. **Edge routing**: Bend points for long edges, routed through dummy nodes

Despite the conceptual similarity, Sugiyama will be implemented in a **separate file** (`sugiyama.rs`) rather than as configuration options on the topological grid. This isolates the complexity and provides a reliable fallback if Sugiyama has bugs.

This algorithm will be documented in `sugiyama_layout.md` and is recommended for complex networks where visual clarity is critical.

### Incremental/Layout-Preserving Algorithm

When users manually arrange nodes and then use AI to modify the network, the existing layout should be preserved as much as possible. The incremental algorithm:

1. **Detects user-organized layouts** vs. AI-generated ones
2. **Preserves existing node positions** for unmodified nodes
3. **Positions new nodes contextually** based on local spacing and alignment patterns
4. **Makes minimal adjustments** only when necessary to avoid overlaps

This algorithm will be documented in `incremental_layout.md` and is essential for good UX when AI assists with user-created networks.

---

## Implementation Location

Layout algorithms are implemented in a dedicated module:

```
rust/src/structure_designer/layout/
├── mod.rs                 # Public API, LayoutAlgorithm trait, re-exports
├── common.rs              # Shared utilities (depth computation, graph traversal)
├── topological_grid.rs    # Simple, reliable layered layout
├── sugiyama.rs            # Sophisticated layout with crossing minimization (future)
└── incremental.rs         # Layout-preserving for user-edited networks (future)
```

### Design Rationale: Separate Files per Algorithm

Each major algorithm has its own file rather than combining them with configuration flags. This provides:

1. **Isolation of complexity**: Sugiyama is complex and has higher bug risk. Keeping it separate from topological grid prevents contamination of the simple, reliable algorithm.

2. **Clear fallback path**: If Sugiyama misbehaves, users can switch to topological grid with confidence that it's a completely independent code path.

3. **Maintainability**: `topological_grid.rs` remains simple and easy to audit. New contributors can understand it quickly without wading through Sugiyama complexity.

4. **Acceptable duplication**: Basic layer assignment logic (~20-30 lines) may be duplicated, but `common.rs` minimizes this. The clarity benefits outweigh the small duplication cost.

### Shared Code (common.rs)

The following utilities are shared across algorithms:

- `compute_node_depths()` - Assign depth/layer to each node based on dependencies
- `get_input_node_ids()` - Get all nodes that feed into a given node
- `get_output_node_ids()` - Get all nodes that consume a given node's output
- `find_source_nodes()` - Find nodes with no input connections
- `find_sink_nodes()` - Find nodes with no output connections
- Common types: `LayoutResult`, `NodeBounds`, etc.

### Migration from auto_layout.rs

The existing `text_format/auto_layout.rs` contains the incremental `calculate_new_node_position()` function. This will be:

1. Kept for backward compatibility during transition
2. Eventually moved to `layout/incremental.rs` or deprecated in favor of the new incremental algorithm

---

## API Integration

### Rust API

```rust
// In layout/mod.rs

/// Available layout algorithms.
#[derive(Debug, Clone, Copy, Default)]
pub enum LayoutAlgorithm {
    /// Simple layered layout based on topological depth. Fast and reliable.
    #[default]
    TopologicalGrid,
    /// Sophisticated layered layout with crossing minimization. Better quality, more complex.
    Sugiyama,
    /// Preserves existing layout, only positions new nodes. For user-edited networks.
    Incremental,
}

/// Layout the entire network using the specified algorithm.
pub fn layout_network(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
) {
    let positions = match algorithm {
        LayoutAlgorithm::TopologicalGrid => topological_grid::layout(network, registry),
        LayoutAlgorithm::Sugiyama => sugiyama::layout(network, registry),
        LayoutAlgorithm::Incremental => incremental::layout(network, registry),
    };

    // Apply new positions
    for (node_id, position) in positions {
        if let Some(node) = network.nodes.get_mut(&node_id) {
            node.position = position;
        }
    }
}
```

Each algorithm module exposes a simple `layout()` function with the same signature:

```rust
// In layout/topological_grid.rs
pub fn layout(network: &NodeNetwork, registry: &NodeTypeRegistry) -> HashMap<u64, DVec2>;

// In layout/sugiyama.rs (future)
pub fn layout(network: &NodeNetwork, registry: &NodeTypeRegistry) -> HashMap<u64, DVec2>;

// In layout/incremental.rs (future)
pub fn layout(network: &NodeNetwork, registry: &NodeTypeRegistry) -> HashMap<u64, DVec2>;
```
```

### CLI Integration

The `atomcad-cli` tool can trigger layout after edits:

```bash
# Apply topological grid layout after editing
atomcad-cli edit network.cnnd --layout=topological-grid "add sphere..."

# Preserve existing layout (incremental mode)
atomcad-cli edit network.cnnd --layout=preserve "modify union..."

# Auto-select based on network characteristics
atomcad-cli edit network.cnnd --layout=auto "..."
```

---

## Testing Strategy

### Unit Tests

1. **Depth computation**: Verify correct depths for various DAG structures
2. **Column ordering**: Verify barycenter sorting produces expected order
3. **Position assignment**: Verify no overlaps, correct spacing

### Integration Tests

1. **Full layout roundtrip**: Create network via text format, layout, verify positions
2. **Snapshot tests**: Compare layout output against known-good snapshots
3. **Edge cases**: Empty network, single node, disconnected components, diamond patterns

### Visual Testing

Manual inspection of layouts for representative networks:
- Simple linear chain
- Diamond dependency pattern
- Multiple independent subgraphs
- Wide fan-out (one node feeding many)
- Deep narrow graph
