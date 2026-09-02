//! Layout algorithms for full network reorganization.
//!
//! This module provides automatic layout algorithms to reposition all nodes in a network
//! for improved readability and visual organization. These algorithms reorganize the
//! entire network and are used:
//! - When "Auto-Layout Network" is triggered from the menu
//! - After AI edit operations (when auto_layout_after_edit is enabled)
//!
//! # Available Algorithms
//!
//! | Algorithm | Use Case | Status |
//! |-----------|----------|--------|
//! | **Topological Grid** | AI-created networks, general purpose | Implemented |
//! | **Sugiyama** | Complex DAGs requiring minimal edge crossings | Implemented |
//!
//! Note: Incremental positioning of new nodes during editing is handled separately
//! by the `text_format::auto_layout` module, not through these algorithms.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::layout::{layout_network, LayoutAlgorithm};
//!
//! // Layout the entire network using the default algorithm
//! layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);
//! ```
//!
//! # Module Structure
//!
//! - `common.rs` - Shared utilities (depth computation, graph traversal)
//! - `size.rs` - `rendered_node_size`, the one node-size function
//! - `delta.rs` - `EditDelta` / `diff_scope`, the incremental pass's input
//! - `motion.rs` - `shift_half_plane` / `cascade` / `grow_rect`, the two
//!   motion primitives the incremental pass repairs a drawing with
//! - `incremental.rs` - the incremental pass itself, step by step
//! - `topological_grid.rs` - Simple, reliable layered layout
//! - `sugiyama.rs` - Sophisticated layout with crossing minimization

pub mod common;
pub mod delta;
pub mod incremental;
pub mod motion;
pub mod size;
pub mod sugiyama;
pub mod topological_grid;

// Re-export main types and functions
pub use common::LayoutAlgorithm;
pub use delta::{
    EditDelta, WireEnd, WireKey, WireSlot, collect_all_wires, diff_scope, node_ids_by_path,
    scopes_inside_out,
};
pub use incremental::{
    Block, BodyFrame, SLIDE_WINDOW, layout_incremental, layout_scope, repair_grown,
};
pub use motion::{
    CascadeDir, Rect, ShiftOutcome, cascade, grow_rect, measure_scope, shift_half_plane,
    snap_shift_line, upstream_closure,
};
pub use size::{rendered_body_size, rendered_node_size, rendered_node_size_by_id};

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use crate::node_network::NodeNetwork;
use crate::node_type_registry::NodeTypeRegistry;

/// Layout the entire network using the specified algorithm.
///
/// This is the main entry point for layout operations. It computes new positions
/// for all nodes in the network based on the selected algorithm, then applies
/// those positions to the network.
///
/// # Arguments
/// * `network` - Mutable reference to the node network to lay out
/// * `registry` - The node type registry for looking up node information
/// * `algorithm` - The layout algorithm to use
///
/// # Example
///
/// ```rust,ignore
/// layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);
/// ```
pub fn layout_network(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
) {
    let positions = compute_layout(network, registry, algorithm);

    // Apply new positions to nodes
    for (node_id, position) in positions {
        if let Some(node) = network.nodes.get_mut(&node_id) {
            node.position = position;
        }
    }
}

/// Compute positions for a **subgraph** — only the nodes in `ids`, wired to
/// each other alone.
///
/// Step 3 of the incremental pass (`doc/design_incremental_layout.md` D2): the
/// freshly added nodes are laid out as a group, in isolation, and the result is
/// placed into the existing drawing as one rigid block. Nothing outside `ids`
/// is read as a neighbour, an edge or an obstacle, and nothing outside `ids`
/// appears in the result.
///
/// The positions are in the algorithm's own frame (starting at
/// `common::START_X` / `START_Y`); the caller normalizes and translates them.
/// Comments are excluded from the layering as everywhere else (D8) and are not
/// placed here — the full reflow's `place_comments` is deliberately not run.
pub fn layout_subgraph(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    ids: &HashSet<u64>,
    algorithm: LayoutAlgorithm,
) -> HashMap<u64, DVec2> {
    match algorithm {
        LayoutAlgorithm::TopologicalGrid => {
            topological_grid::layout_subgraph(network, registry, ids)
        }
        LayoutAlgorithm::Sugiyama => sugiyama::layout_subgraph(network, registry, ids),
    }
}

/// Compute positions for all nodes without modifying the network.
///
/// This function is useful when you want to preview the layout or
/// apply it selectively.
///
/// # Arguments
/// * `network` - The node network to analyze
/// * `registry` - The node type registry
/// * `algorithm` - The layout algorithm to use
///
/// # Returns
/// A HashMap from node ID to computed position
pub fn compute_layout(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
) -> HashMap<u64, DVec2> {
    match algorithm {
        LayoutAlgorithm::TopologicalGrid => topological_grid::layout(network, registry),
        LayoutAlgorithm::Sugiyama => sugiyama::layout(network, registry),
    }
}
