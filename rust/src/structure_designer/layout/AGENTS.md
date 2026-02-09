# Layout - Agent Instructions

Automatic layout algorithms for repositioning nodes in a network.

## Files

| File | Purpose |
|------|---------|
| `common.rs` | Shared types: `LayoutNode`, `LayoutResult`, entry points |
| `topological_grid.rs` | Simple layered layout (fast, reliable) |
| `sugiyama.rs` | Sugiyama-style layout with crossing minimization |

## Entry Points

- `layout_network(network, registry)` → applies layout in-place
- `compute_layout(network, registry)` → returns `LayoutResult` without mutating

## Algorithms

**TopologicalGrid:** Assigns nodes to layers by topological order, then arranges vertically within each layer. Simple and predictable.

**Sugiyama:** Multi-phase algorithm: layer assignment → crossing reduction → coordinate assignment. Better results for complex graphs but more expensive.

## Usage

Layout is triggered by:
- AI text format edits (auto-layout new nodes via `text_format/auto_layout.rs`)
- "Auto-Layout Network" menu action in the UI
- Node size estimation uses constants from `node_layout.rs` in the parent directory
