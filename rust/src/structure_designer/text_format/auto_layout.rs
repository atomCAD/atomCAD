//! Auto-layout for AI-created nodes.
//!
//! This module provides smart positioning for nodes created via the edit command.
//! It places new nodes based on their input connections while avoiding overlap
//! with existing nodes.
//!
//! # Layout Strategy
//!
//! 1. **Nodes with inputs**: Place to the right of their source nodes, at the
//!    average Y position of all sources.
//! 2. **Nodes without inputs**: Place in empty space, typically to the right
//!    of all existing nodes.
//! 3. **Overlap avoidance**: If the proposed position overlaps existing nodes,
//!    try positions below until a non-overlapping position is found.
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::structure_designer::text_format::auto_layout;
//!
//! let position = auto_layout::calculate_new_node_position(
//!     &network,
//!     &registry,
//!     "sphere",
//!     &[], // no input connections
//! );
//! ```

use glam::DVec2;

use crate::structure_designer::node_layout;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// Calculate position for a new node based on its connections.
///
/// Uses the `node_layout` module for accurate size estimation and overlap detection.
///
/// # Arguments
/// * `network` - The node network containing existing nodes
/// * `registry` - The node type registry for looking up node types
/// * `node_type_name` - The type name of the new node being created
/// * `input_connections` - List of (source_node_id, source_output_pin) for pending connections
///
/// # Returns
/// A `DVec2` with the (x, y) position for the new node
pub fn calculate_new_node_position(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    node_type_name: &str,
    input_connections: &[(u64, i32)],
) -> DVec2 {
    // Get the size of the new node based on its parameter count
    let new_node_size = get_node_size(registry, node_type_name);

    // Strategy 1: Place to the right of input nodes
    if !input_connections.is_empty() {
        let source_data: Vec<(DVec2, DVec2)> = input_connections
            .iter()
            .filter_map(|(id, _)| {
                let node = network.nodes.get(id)?;
                let source_type = &node.node_type_name;
                let source_size = get_node_size(registry, source_type);
                Some((DVec2::new(node.position.x, node.position.y), source_size))
            })
            .collect();

        if !source_data.is_empty() {
            // X: to the right of rightmost source (accounting for source width)
            let max_x = source_data
                .iter()
                .map(|(pos, size)| pos.x + size.x + node_layout::DEFAULT_HORIZONTAL_GAP)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            // Y: average Y of all sources
            let avg_y =
                source_data.iter().map(|(pos, _)| pos.y).sum::<f64>() / source_data.len() as f64;

            let proposed = DVec2::new(max_x, avg_y);

            // Check for overlap and adjust if needed
            return find_non_overlapping_position(network, registry, proposed, new_node_size);
        }
    }

    // Strategy 2: Find empty space for nodes with no inputs
    find_empty_position(network, registry, new_node_size)
}

/// Get the estimated size of a node based on its type.
///
/// Uses the node type's parameter count to estimate height.
pub fn get_node_size(registry: &NodeTypeRegistry, node_type_name: &str) -> DVec2 {
    let num_params = registry
        .get_node_type(node_type_name)
        .map(|nt| nt.parameters.len())
        .unwrap_or(0);

    node_layout::estimate_node_size(num_params, true)
}

/// Find a position near the proposed location that doesn't overlap existing nodes.
///
/// Tries the proposed position first, then positions below it with increasing
/// vertical offset until a non-overlapping position is found.
///
/// # Arguments
/// * `network` - The node network containing existing nodes
/// * `registry` - The node type registry
/// * `proposed` - The initially proposed position
/// * `new_node_size` - The size of the new node
///
/// # Returns
/// A position that doesn't overlap any existing nodes
fn find_non_overlapping_position(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    proposed: DVec2,
    new_node_size: DVec2,
) -> DVec2 {
    let existing_nodes = get_existing_node_bounds(network, registry);

    // Check if proposed position overlaps any existing node
    if !node_layout::overlaps_any(
        proposed,
        new_node_size,
        existing_nodes.iter().copied(),
        node_layout::DEFAULT_VERTICAL_GAP,
    ) {
        return proposed;
    }

    // Try positions below the proposed location
    for offset in 1..50 {
        let offset_y = offset as f64 * (new_node_size.y + node_layout::DEFAULT_VERTICAL_GAP);
        let new_pos = DVec2::new(proposed.x, proposed.y + offset_y);
        if !node_layout::overlaps_any(
            new_pos,
            new_node_size,
            existing_nodes.iter().copied(),
            node_layout::DEFAULT_VERTICAL_GAP,
        ) {
            return new_pos;
        }
    }

    // Fallback: just return proposed position (will overlap)
    proposed
}

/// Find an empty position in the canvas for nodes with no input connections.
///
/// Places the node to the right of all existing nodes, or at a default
/// starting position if the network is empty.
///
/// # Arguments
/// * `network` - The node network containing existing nodes
/// * `registry` - The node type registry
/// * `new_node_size` - The size of the new node
///
/// # Returns
/// A position in empty space suitable for a new node
fn find_empty_position(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    new_node_size: DVec2,
) -> DVec2 {
    // Default starting position for empty networks
    const DEFAULT_START_X: f64 = 100.0;
    const DEFAULT_START_Y: f64 = 100.0;

    if network.nodes.is_empty() {
        return DVec2::new(DEFAULT_START_X, DEFAULT_START_Y);
    }

    let existing_bounds = get_existing_node_bounds(network, registry);

    // Find the rightmost edge of all existing nodes
    let max_right = existing_bounds
        .iter()
        .map(|(pos, size)| pos.x + size.x)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);

    // Calculate average Y position for vertical centering
    let avg_y =
        network.nodes.values().map(|n| n.position.y).sum::<f64>() / network.nodes.len() as f64;

    let proposed = DVec2::new(max_right + node_layout::DEFAULT_HORIZONTAL_GAP * 2.0, avg_y);

    // Ensure we don't overlap (in case of unusual layouts)
    find_non_overlapping_position(network, registry, proposed, new_node_size)
}

/// Get bounds (position, size) for all existing nodes in the network.
///
/// # Arguments
/// * `network` - The node network
/// * `registry` - The node type registry
///
/// # Returns
/// Vector of (position, size) tuples for each node
fn get_existing_node_bounds(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Vec<(DVec2, DVec2)> {
    network
        .nodes
        .values()
        .map(|node| {
            let size = get_node_size(registry, &node.node_type_name);
            (DVec2::new(node.position.x, node.position.y), size)
        })
        .collect()
}
