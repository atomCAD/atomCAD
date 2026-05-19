use super::node_network::{NodeNetwork, NodeRef};
use std::collections::{HashSet, VecDeque};

/// Computes all downstream transitive dependents for a set of changed nodes,
/// across every zone body in the network tree.
///
/// Uses [`NodeNetwork::build_scope_reverse_dependency_map`] which descends
/// recursively into HOF bodies and handles the four wire roles described in
/// `doc/design_zones.md` §"How wires represent each role under zones", plus
/// a synthetic body-node → enclosing-HOF edge so dirtiness inside a body
/// lifts out into the parent scope. Without that synthetic edge, editing
/// a node inside (say) a `map` body wouldn't invalidate the `map`'s output,
/// and downstream consumers in the top-level network would render stale.
///
/// # Arguments
/// * `network` - The top-level (root) node network to analyze
/// * `changed` - Set of `NodeRef`s that have changed (top-level or any body)
///
/// # Returns
/// A `HashSet<NodeRef>` containing every node that transitively depends on
/// any of the changed nodes, including the changed nodes themselves.
///
/// # Example
/// ```ignore
/// // Top-level: int_id → map_id → collect_id
/// //            (capture)  (regular wire)
/// // Editing int_id yields { NodeRef::top(int_id), NodeRef::top(map_id),
/// //                          NodeRef::top(collect_id) }.
/// ```
pub fn compute_downstream_dependents(
    network: &NodeNetwork,
    changed: &HashSet<NodeRef>,
) -> HashSet<NodeRef> {
    let downstream_map = network.build_scope_reverse_dependency_map();

    let mut result: HashSet<NodeRef> = HashSet::new();
    let mut queue: VecDeque<NodeRef> = VecDeque::new();

    // Seed the BFS. Validate that the node exists at its claimed scope —
    // ids on bodies can collide with top-level ids, so a bare existence
    // check at top level isn't enough. We trust `NodeRef` came from a
    // legitimate scoped mutation; if a seed doesn't exist (e.g. a stale
    // dirty mark for a deleted node), the BFS just produces no further
    // dependents from it, which is safe.
    for node_ref in changed {
        if node_exists(network, node_ref) && result.insert(node_ref.clone()) {
            queue.push_back(node_ref.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        if let Some(dependents) = downstream_map.get(&current) {
            for dependent in dependents {
                if result.insert(dependent.clone()) {
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    result
}

/// Returns `true` if a node with the given `NodeRef` exists in the network
/// tree rooted at `root`. Walks `scope_path` from `root` through HOF zones.
fn node_exists(root: &NodeNetwork, node_ref: &NodeRef) -> bool {
    let mut current = root;
    for hof_id in &node_ref.scope_path {
        let hof = match current.nodes.get(hof_id) {
            Some(n) => n,
            None => return false,
        };
        current = match hof.zone.as_deref() {
            Some(body) => body,
            None => return false,
        };
    }
    current.nodes.contains_key(&node_ref.node_id)
}
