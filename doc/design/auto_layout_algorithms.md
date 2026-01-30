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

---

## User Configuration

### StructureDesignerPreferences

The layout algorithm is configured through `StructureDesignerPreferences`, not exposed to AI agents via CLI. AI agents should not be aware of node positioning—layout is a user-facing concern.

```rust
// In api/structure_designer/structure_designer_preferences.rs

/// Layout algorithm preference for auto-layout operations.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum LayoutAlgorithmPreference {
    /// Simple layered layout based on topological depth. Fast and reliable.
    #[default]
    TopologicalGrid,
    /// Sophisticated layered layout with crossing minimization.
    Sugiyama,
    /// Preserves existing layout, only positions new nodes.
    Incremental,
}

pub struct StructureDesignerPreferences {
    // ... existing fields ...

    /// The layout algorithm to use for auto-layout operations.
    pub layout_algorithm: LayoutAlgorithmPreference,
}
```

### Preferences Dialog

The layout algorithm is selectable via a dropdown in the Preferences dialog:

| Setting | Options | Default |
|---------|---------|---------|
| **Auto-Layout Algorithm** | Topological Grid, Sugiyama, Incremental | Topological Grid |

The dropdown should display user-friendly names:
- "Topological Grid" → Simple, fast layout organized by dependency depth
- "Sugiyama" → Advanced layout with crossing minimization (when implemented)
- "Incremental" → Preserves existing positions, only places new nodes (when implemented)

---

## When Auto-Layout is Triggered

Auto-layout is applied in two scenarios:

### 1. After atomcad-cli Edit Operations

When an AI agent modifies a network through `atomcad-cli edit`, the layout algorithm from `StructureDesignerPreferences` is automatically applied to the entire network. The AI agent is not aware of this—it simply edits the network structure, and the application handles positioning.

```rust
// In atomcad-cli edit handler (pseudocode)
fn handle_edit_command(network: &mut NodeNetwork, edit_text: &str) {
    // Apply the edit (AI agent's changes)
    apply_text_edit(network, edit_text);

    // Auto-layout using user's preferred algorithm
    let algorithm = preferences.layout_algorithm;
    layout::layout_network(network, registry, algorithm.into());
}
```

### 2. Manual Menu Item

A menu item allows users to manually trigger auto-layout on the active network:

**Menu Location**: `Edit > Auto-Layout Network` (or similar)

**Behavior**:
1. Applies the layout algorithm from preferences to the currently active network
2. All node positions are recalculated
3. The operation is undoable

```dart
// In Flutter menu handler (pseudocode)
void onAutoLayoutMenuItemClicked() {
  final algorithm = preferences.layoutAlgorithm;
  final network = structureDesigner.activeNetwork;

  // Call Rust API to perform layout
  api.layoutNetwork(network.id, algorithm);

  // Refresh the UI
  notifyListeners();
}
```

**Note**: This menu item is useful when:
- A user imports a network with poor layout
- A user wants to reorganize after manual edits
- Testing different layout algorithms on the same network

---

## Testing Strategy

### Unit Tests

1. **Depth computation**: Verify correct depths for various DAG structures
2. **Column ordering**: Verify barycenter sorting produces expected order
3. **Position assignment**: Verify no overlaps, correct spacing

### Integration Tests

1. **Full layout roundtrip**: Create network via text format, layout, verify positions
3. **Edge cases**: Empty network, single node, disconnected components, diamond patterns

### Visual Testing

Manual inspection of layouts for representative networks:
- Simple linear chain
- Diamond dependency pattern
- Multiple independent subgraphs
- Wide fan-out (one node feeding many)
- Deep narrow graph

---

## Implementation Plan

The implementation is divided into three phases to keep each phase manageable and testable.

### Phase 1: Core Layout Module (Rust)

**Goal**: Implement the layout module infrastructure and topological grid algorithm in Rust.

**Tasks**:

1. **Create module structure**
   - Create `rust/src/structure_designer/layout/mod.rs`
   - Create `rust/src/structure_designer/layout/common.rs`
   - Create `rust/src/structure_designer/layout/topological_grid.rs`
   - Add `pub mod layout;` to `rust/src/structure_designer/mod.rs`

2. **Implement common.rs**
   - `compute_node_depths()` - depth calculation via dependency traversal
   - `get_input_node_ids()` - get nodes feeding into a given node
   - `find_source_nodes()` - find nodes with no inputs
   - `LayoutAlgorithm` enum (TopologicalGrid only for now, others as stubs)

3. **Implement topological_grid.rs**
   - `layout()` - main entry point
   - `group_by_depth()` - organize nodes into columns
   - `order_column()` - barycenter-based ordering within columns
   - `assign_positions()` - compute final X/Y coordinates

4. **Implement mod.rs**
   - `layout_network()` - public API that dispatches to algorithm implementations
   - Re-export public types

5. **Add unit tests**
   - Create `rust/tests/layout_topological_grid.rs`
   - Test depth computation for various graph shapes
   - Test column ordering
   - Test position assignment (no overlaps, correct spacing)
   - Test edge cases: empty network, single node, disconnected components

**Deliverables**:
- Working `layout::layout_network()` function callable from Rust
- Unit tests passing

---

### Phase 2: atomcad-cli Integration

**Goal**: Automatically apply layout after `atomcad-cli edit` operations.

**Tasks**:

1. **Add LayoutAlgorithmPreference to preferences**
   - Add `LayoutAlgorithmPreference` enum to `structure_designer_preferences.rs`
   - Add `layout_algorithm: LayoutAlgorithmPreference` field to `StructureDesignerPreferences`
   - Default to `TopologicalGrid`

2. **Hook layout into edit command**
   - In `atomcad-cli` edit handler, after applying text edits:
   - Read `layout_algorithm` from preferences
   - Call `layout::layout_network()` with the selected algorithm
   - Save the updated network with new positions

3. **Add integration test**
   - Create test that runs `atomcad-cli edit`, then verifies node positions are laid out correctly
   - Test that layout respects the preference setting

**Deliverables**:
- `atomcad-cli edit` automatically produces well-laid-out networks
- Preference field exists (UI comes in Phase 3)

---

### Phase 3: UI Integration (Flutter)

**Goal**: Add preferences dropdown and manual auto-layout menu item.

**Tasks**:

1. **Expose layout API to Flutter**
   - Add `layout_network(algorithm: LayoutAlgorithmPreference)` to Rust API
   - Run `flutter_rust_bridge_codegen generate`

2. **Add preferences dropdown**
   - Add dropdown to preferences dialog for "Auto-Layout Algorithm"
   - Options: "Topological Grid" (and placeholders for future algorithms)
   - Wire up to `StructureDesignerPreferences.layout_algorithm`

3. **Add menu item**
   - Add "Auto-Layout Network" to Edit menu (or appropriate location)
   - On click: call layout API with current preference, refresh canvas
   - Ensure operation is undoable (integrate with undo system if applicable)

4. **Visual verification**
   - Manual testing with various networks
   - Verify UI updates correctly after layout

**Deliverables**:
- Users can select layout algorithm in preferences
- Users can manually trigger auto-layout via menu
- Full end-to-end workflow functional

---

### Phase Summary

| Phase | Focus | Key Deliverable |
|-------|-------|-----------------|
| **1** | Rust core | `layout::layout_network()` + tests |
| **2** | CLI integration | Auto-layout after `atomcad-cli edit` |
| **3** | Flutter UI | Preferences dropdown + menu item |

Each phase is independently testable and provides incremental value. Phase 1 is a prerequisite for Phases 2 and 3, but Phases 2 and 3 can be done in either order.
