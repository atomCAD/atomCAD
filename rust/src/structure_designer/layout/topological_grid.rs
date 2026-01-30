//! Topological Grid Layout Algorithm
//!
//! A simple, deterministic algorithm that produces clean, readable layouts
//! for most DAG structures. Organizes nodes into columns based on their
//! topological depth in the dependency graph.
//!
//! ## Algorithm Steps
//!
//! 1. **Compute depths**: Assign each node a depth based on dependency traversal
//! 2. **Group by depth**: Organize nodes into columns (one per depth level)
//! 3. **Order within columns**: Sort nodes using barycenter heuristic to minimize crossings
//! 4. **Assign coordinates**: Compute final X/Y positions with proper spacing
//!
//! ## Complexity
//!
//! - Time: O(V + E) for depth computation, O(V log V) for sorting within columns
//! - Space: O(V) for storing depths and positions

use std::collections::HashMap;

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

/// Compute positions for all nodes using the topological grid layout algorithm.
///
/// This algorithm:
/// 1. Computes the topological depth of each node
/// 2. Groups nodes into columns by depth
/// 3. Orders nodes within each column to minimize edge crossings
/// 4. Assigns final X/Y coordinates
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

    // Step 1: Compute depths
    let depths = compute_node_depths(network);

    // Step 2: Group nodes by depth into columns
    let mut columns = group_by_depth(&depths);

    // Step 3: Order nodes within each column to minimize edge crossings
    order_columns(&mut columns, network);

    // Step 4: Assign final coordinates
    assign_positions(&columns, network, registry)
}

/// Group nodes into columns based on their computed depth.
///
/// # Arguments
/// * `depths` - HashMap from node ID to depth
///
/// # Returns
/// A Vec where index is the column (depth) and value is the list of node IDs in that column
fn group_by_depth(depths: &HashMap<u64, usize>) -> Vec<Vec<u64>> {
    let max_depth = depths.values().copied().max().unwrap_or(0);
    let mut columns: Vec<Vec<u64>> = vec![vec![]; max_depth + 1];

    for (&node_id, &depth) in depths {
        columns[depth].push(node_id);
    }

    // Sort node IDs within each column for deterministic output
    for column in &mut columns {
        column.sort();
    }

    columns
}

/// Order nodes within each column to minimize edge crossings.
///
/// Uses the barycenter heuristic: sort nodes by the average Y position
/// of their connected nodes in the previous column.
///
/// # Arguments
/// * `columns` - Mutable reference to the columns to reorder
/// * `network` - The node network for connection information
fn order_columns(columns: &mut [Vec<u64>], network: &NodeNetwork) {
    // Track Y positions of nodes in the previous column
    // For the first pass, we use the initial sorted order as positions
    let mut prev_column_positions: HashMap<u64, f64> = HashMap::new();

    // Initialize positions for first column (depth 0)
    if !columns.is_empty() {
        for (i, &node_id) in columns[0].iter().enumerate() {
            prev_column_positions.insert(node_id, i as f64);
        }
    }

    // Process each column starting from column 1
    for col_idx in 1..columns.len() {
        order_single_column(&mut columns[col_idx], &prev_column_positions, network);

        // Update positions for next iteration
        prev_column_positions.clear();
        for (i, &node_id) in columns[col_idx].iter().enumerate() {
            prev_column_positions.insert(node_id, i as f64);
        }
    }

    // Optional: Do a backward pass to further reduce crossings
    // (Process columns from right to left using next column positions)
    let mut next_column_positions: HashMap<u64, f64> = HashMap::new();

    if columns.len() > 1 {
        // Initialize with last column positions
        let last_col = columns.len() - 1;
        for (i, &node_id) in columns[last_col].iter().enumerate() {
            next_column_positions.insert(node_id, i as f64);
        }

        // Process columns from second-to-last backwards
        for col_idx in (0..last_col).rev() {
            order_single_column_backward(&mut columns[col_idx], &next_column_positions, network);

            // Update positions for next iteration
            next_column_positions.clear();
            for (i, &node_id) in columns[col_idx].iter().enumerate() {
                next_column_positions.insert(node_id, i as f64);
            }
        }
    }
}

/// Order a single column by barycenter of input connections (forward pass).
fn order_single_column(
    column: &mut Vec<u64>,
    prev_column_positions: &HashMap<u64, f64>,
    network: &NodeNetwork,
) {
    column.sort_by(|&a, &b| {
        let a_barycenter = compute_input_barycenter(a, prev_column_positions, network);
        let b_barycenter = compute_input_barycenter(b, prev_column_positions, network);
        a_barycenter
            .partial_cmp(&b_barycenter)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Order a single column by barycenter of output connections (backward pass).
fn order_single_column_backward(
    column: &mut Vec<u64>,
    next_column_positions: &HashMap<u64, f64>,
    network: &NodeNetwork,
) {
    column.sort_by(|&a, &b| {
        let a_barycenter = compute_output_barycenter(a, next_column_positions, network);
        let b_barycenter = compute_output_barycenter(b, next_column_positions, network);
        a_barycenter
            .partial_cmp(&b_barycenter)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Compute the barycenter (average position) of a node's input connections.
///
/// # Arguments
/// * `node_id` - The node to compute barycenter for
/// * `prev_positions` - Positions of nodes in the previous column
/// * `network` - The node network
///
/// # Returns
/// The average Y position of connected input nodes, or 0.0 if none
fn compute_input_barycenter(
    node_id: u64,
    prev_positions: &HashMap<u64, f64>,
    network: &NodeNetwork,
) -> f64 {
    let node = match network.nodes.get(&node_id) {
        Some(n) => n,
        None => return 0.0,
    };

    let input_positions: Vec<f64> = node
        .arguments
        .iter()
        .flat_map(|arg| arg.argument_output_pins.keys())
        .filter_map(|&source_id| prev_positions.get(&source_id).copied())
        .collect();

    if input_positions.is_empty() {
        // No connections to previous column - use a high value to push to bottom
        // This keeps unconnected nodes grouped at the bottom of columns
        f64::MAX / 2.0
    } else {
        input_positions.iter().sum::<f64>() / input_positions.len() as f64
    }
}

/// Compute the barycenter (average position) of a node's output connections.
///
/// # Arguments
/// * `node_id` - The node to compute barycenter for
/// * `next_positions` - Positions of nodes in the next column
/// * `network` - The node network
///
/// # Returns
/// The average Y position of connected output nodes, or 0.0 if none
fn compute_output_barycenter(
    node_id: u64,
    next_positions: &HashMap<u64, f64>,
    network: &NodeNetwork,
) -> f64 {
    // Find all nodes that use this node as input
    let output_positions: Vec<f64> = network
        .nodes
        .iter()
        .filter_map(|(&other_id, other_node)| {
            // Check if this node is referenced in other_node's arguments
            let is_connected = other_node
                .arguments
                .iter()
                .any(|arg| arg.argument_output_pins.contains_key(&node_id));

            if is_connected {
                next_positions.get(&other_id).copied()
            } else {
                None
            }
        })
        .collect();

    if output_positions.is_empty() {
        f64::MAX / 2.0
    } else {
        output_positions.iter().sum::<f64>() / output_positions.len() as f64
    }
}

/// Assign final X and Y coordinates to all nodes.
///
/// X position is determined by column (depth).
/// Y position is determined by order within column, with vertical centering.
///
/// # Arguments
/// * `columns` - The ordered columns of node IDs
/// * `network` - The node network
/// * `registry` - The node type registry for size lookups
///
/// # Returns
/// HashMap from node ID to final position
fn assign_positions(
    columns: &[Vec<u64>],
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> HashMap<u64, DVec2> {
    let mut positions: HashMap<u64, DVec2> = HashMap::new();

    // Calculate the maximum total height across all columns for centering
    let max_column_height = columns
        .iter()
        .map(|column| calculate_column_height(column, network, registry))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    for (col_index, column) in columns.iter().enumerate() {
        let x = START_X + col_index as f64 * COLUMN_WIDTH;

        // Calculate total height of this column
        let column_height = calculate_column_height(column, network, registry);

        // Center this column vertically relative to the tallest column
        let y_offset = (max_column_height - column_height) / 2.0;
        let mut y = START_Y + y_offset;

        for &node_id in column {
            positions.insert(node_id, DVec2::new(x, y));

            // Move Y down for next node
            let node_height = get_node_height(node_id, network, registry);
            y += node_height + VERTICAL_GAP;
        }
    }

    positions
}

/// Calculate the total height of a column including gaps.
fn calculate_column_height(
    column: &[u64],
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> f64 {
    if column.is_empty() {
        return 0.0;
    }

    let total_node_height: f64 = column
        .iter()
        .map(|&id| get_node_height(id, network, registry))
        .sum();

    let gaps = (column.len() - 1) as f64 * VERTICAL_GAP;

    total_node_height + gaps
}

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
