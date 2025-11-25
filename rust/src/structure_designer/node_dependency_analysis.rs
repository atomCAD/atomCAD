use std::collections::{HashSet, VecDeque};
use super::node_network::NodeNetwork;

/// Computes all downstream transitive dependent nodes for a given set of changed nodes
/// 
/// This function performs a breadth-first search (BFS) traversal starting from the changed nodes
/// to find all nodes that transitively depend on them. The result includes the changed nodes themselves.
/// 
/// # Algorithm
/// 1. Build a reverse dependency map using `NodeNetwork::build_reverse_dependency_map()`
/// 2. Perform BFS starting from all changed nodes
/// 3. For each node, add all its downstream dependents to the queue
/// 4. Continue until all reachable nodes are visited
/// 
/// # Complexity
/// - Time: O(N + E) where N = number of nodes, E = number of edges (connections)
/// - Space: O(N + E) for the reverse map and result set
/// 
/// # Arguments
/// * `network` - The node network to analyze
/// * `changed_node_ids` - Set of node IDs that have changed
/// 
/// # Returns
/// HashSet containing all nodes that transitively depend on the changed nodes (including the changed nodes themselves)
/// 
/// # Example
/// ```
/// // If we have: A → B → C and A → D
/// // And A's data changes, then compute_downstream_dependents returns {A, B, C, D}
/// ```
pub fn compute_downstream_dependents(
    network: &NodeNetwork, 
    changed_node_ids: &HashSet<u64>
) -> HashSet<u64> {
    // Step 1: Build reverse dependency map (downstream map)
    // Key: node_id, Value: Vec of nodes that depend on this node
    let downstream_map = network.build_reverse_dependency_map();
    
    // Step 2: BFS traversal to find all downstream dependents
    let mut result = HashSet::new();
    let mut queue: VecDeque<u64> = VecDeque::new();
    
    // Initialize with changed nodes
    for &node_id in changed_node_ids {
        // Only process nodes that actually exist in the network
        if network.nodes.contains_key(&node_id) {
            result.insert(node_id);
            queue.push_back(node_id);
        }
    }
    
    // BFS traversal
    while let Some(current_node_id) = queue.pop_front() {
        if let Some(dependents) = downstream_map.get(&current_node_id) {
            for &dependent_id in dependents {
                // Only queue if we haven't seen this node before
                if result.insert(dependent_id) {
                    queue.push_back(dependent_id);
                }
            }
        }
    }
    
    result
}
















