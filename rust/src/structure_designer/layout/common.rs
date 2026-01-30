//! Common utilities for layout algorithms.
//!
//! This module provides shared functions used by multiple layout algorithms:
//! - Depth computation for DAG traversal
//! - Graph traversal utilities
//! - Common types

use std::collections::{HashMap, HashSet};

use crate::structure_designer::node_network::NodeNetwork;

/// Available layout algorithms for full network reorganization.
///
/// These algorithms reorganize the entire network. They are used:
/// - When "Auto-Layout Network" is triggered from the menu
/// - After AI edit operations (when auto_layout_after_edit is enabled)
///
/// Note: Incremental positioning of new nodes during editing is handled
/// separately by the auto_layout module, not through this enum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutAlgorithm {
    /// Simple layered layout based on topological depth. Fast and reliable.
    /// Organizes nodes into columns by their depth in the dependency graph.
    #[default]
    TopologicalGrid,

    /// Sophisticated layered layout with crossing minimization.
    /// Uses the Sugiyama algorithm for better visual quality on complex graphs.
    /// (Not yet implemented - falls back to TopologicalGrid)
    Sugiyama,
}

/// Compute the topological depth of each node in the network.
///
/// Each node is assigned a "depth" based on its position in the dependency graph:
/// - depth(node) = 0 if node has no input connections
/// - depth(node) = max(depth(inputs)) + 1 otherwise
///
/// This ensures:
/// - Source nodes (primitives, literals) are at depth 0
/// - Each node appears to the right of all its dependencies
/// - The final output/return node has the highest depth
///
/// # Arguments
/// * `network` - The node network to analyze
///
/// # Returns
/// A HashMap from node ID to its computed depth
pub fn compute_node_depths(network: &NodeNetwork) -> HashMap<u64, usize> {
    let mut depths: HashMap<u64, usize> = HashMap::new();
    let mut visiting: HashSet<u64> = HashSet::new();

    fn visit(
        node_id: u64,
        network: &NodeNetwork,
        depths: &mut HashMap<u64, usize>,
        visiting: &mut HashSet<u64>,
    ) -> usize {
        // Return cached depth if already computed
        if let Some(&depth) = depths.get(&node_id) {
            return depth;
        }

        // Cycle detection - if we're already visiting this node, treat as source
        if visiting.contains(&node_id) {
            return 0;
        }
        visiting.insert(node_id);

        let node = match network.nodes.get(&node_id) {
            Some(n) => n,
            None => return 0,
        };

        // Find all input node IDs
        let input_ids: Vec<u64> = node
            .arguments
            .iter()
            .flat_map(|arg| arg.argument_output_pins.keys())
            .copied()
            .collect();

        // If no inputs, this is a source node at depth 0
        if input_ids.is_empty() {
            visiting.remove(&node_id);
            depths.insert(node_id, 0);
            return 0;
        }

        // Compute max depth of all inputs
        let max_input_depth = input_ids
            .iter()
            .map(|&source_id| visit(source_id, network, depths, visiting))
            .max()
            .unwrap_or(0);

        let depth = max_input_depth + 1;

        visiting.remove(&node_id);
        depths.insert(node_id, depth);
        depth
    }

    // Compute depth for all nodes
    for &node_id in network.nodes.keys() {
        visit(node_id, network, &mut depths, &mut visiting);
    }

    depths
}

/// Get all node IDs that feed into the given node (direct inputs).
///
/// # Arguments
/// * `network` - The node network
/// * `node_id` - The node to find inputs for
///
/// # Returns
/// A HashSet of node IDs that are direct inputs to the specified node
pub fn get_input_node_ids(network: &NodeNetwork, node_id: u64) -> HashSet<u64> {
    network
        .nodes
        .get(&node_id)
        .map(|node| {
            node.arguments
                .iter()
                .flat_map(|arg| arg.argument_output_pins.keys())
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

/// Get all node IDs that consume the output of the given node (direct outputs).
///
/// # Arguments
/// * `network` - The node network
/// * `node_id` - The node to find outputs for
///
/// # Returns
/// A HashSet of node IDs that receive output from the specified node
pub fn get_output_node_ids(network: &NodeNetwork, node_id: u64) -> HashSet<u64> {
    let mut outputs = HashSet::new();

    for (&other_id, other_node) in &network.nodes {
        if other_id == node_id {
            continue;
        }

        for argument in &other_node.arguments {
            if argument.argument_output_pins.contains_key(&node_id) {
                outputs.insert(other_id);
                break;
            }
        }
    }

    outputs
}

/// Find all source nodes (nodes with no input connections).
///
/// Source nodes typically include:
/// - Primitive value nodes (int, float, vec3, etc.)
/// - Literal nodes
/// - Parameter nodes
///
/// # Arguments
/// * `network` - The node network to analyze
///
/// # Returns
/// A HashSet of node IDs for all source nodes
pub fn find_source_nodes(network: &NodeNetwork) -> HashSet<u64> {
    network
        .nodes
        .iter()
        .filter_map(|(&node_id, node)| {
            let has_inputs = node
                .arguments
                .iter()
                .any(|arg| !arg.argument_output_pins.is_empty());
            if has_inputs {
                None
            } else {
                Some(node_id)
            }
        })
        .collect()
}

/// Find all sink nodes (nodes with no output connections).
///
/// Sink nodes are typically the final output nodes of a network,
/// such as return nodes or display nodes.
///
/// # Arguments
/// * `network` - The node network to analyze
///
/// # Returns
/// A HashSet of node IDs for all sink nodes
pub fn find_sink_nodes(network: &NodeNetwork) -> HashSet<u64> {
    // Build set of all nodes that are referenced as inputs
    let mut referenced_nodes: HashSet<u64> = HashSet::new();

    for node in network.nodes.values() {
        for argument in &node.arguments {
            for &source_id in argument.argument_output_pins.keys() {
                referenced_nodes.insert(source_id);
            }
        }
    }

    // Sink nodes are those not referenced by any other node
    network
        .nodes
        .keys()
        .filter(|&node_id| !referenced_nodes.contains(node_id))
        .copied()
        .collect()
}
