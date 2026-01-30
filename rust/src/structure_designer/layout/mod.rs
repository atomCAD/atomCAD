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
//! use crate::structure_designer::layout::{layout_network, LayoutAlgorithm};
//!
//! // Layout the entire network using the default algorithm
//! layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);
//! ```
//!
//! # Module Structure
//!
//! - `common.rs` - Shared utilities (depth computation, graph traversal)
//! - `topological_grid.rs` - Simple, reliable layered layout
//! - `sugiyama.rs` - Sophisticated layout with crossing minimization

pub mod common;
pub mod sugiyama;
pub mod topological_grid;

// Re-export main types and functions
pub use common::LayoutAlgorithm;

use std::collections::HashMap;

use glam::DVec2;

use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

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
