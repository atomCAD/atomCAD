//! Find Usages — collecting the *dependents* of a custom node network
//! (issue #414, `doc/design_find_usages.md` Phase 1).
//!
//! A usage of a network is an **instance node**: a node whose
//! `node_type_name` equals the network's name. That covers every reference
//! form, including an instance consumed as a function value through its `-1`
//! pin — that is still an instance node. Usages can live inside HOF/closure
//! zone bodies at any depth, so the walks here are recursive and track the
//! chain of enclosing HOF node ids (the `scope_path`), which
//! [`walk_all_nodes`](super::node_network::walk_all_nodes) does not expose.
//!
//! Everything in this module is read-only: no mutation, no undo, no refresh.

use std::collections::HashMap;

use super::node_network::NodeNetwork;
use super::node_type_registry::NodeTypeRegistry;

/// One reference to a custom network, addressed by the same triple the rest
/// of the codebase uses (`NodeRef` + the host network's name).
///
/// `scope_path` is the chain of HOF node ids from `host_network`'s top level
/// down to the body the instance lives in — empty for a top-level usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkUsage {
    pub host_network: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

/// Collects every usage of `network_name` across the whole registry.
///
/// The result is sorted by `(host_network, scope_path, node_id)` so callers
/// (and tests) get a stable order out of the `HashMap`-backed registry.
pub fn collect_network_usages(
    registry: &NodeTypeRegistry,
    network_name: &str,
) -> Vec<NetworkUsage> {
    let mut usages = Vec::new();
    for (host_network, network) in registry.node_networks.iter() {
        let mut scope_path = Vec::new();
        collect_in_network(
            network,
            &mut scope_path,
            &mut |scope_path, node_id| {
                usages.push(NetworkUsage {
                    host_network: host_network.clone(),
                    scope_path: scope_path.to_vec(),
                    node_id,
                });
            },
            network_name,
        );
    }
    usages.sort_by(|a, b| {
        a.host_network
            .cmp(&b.host_network)
            .then_with(|| a.scope_path.cmp(&b.scope_path))
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    usages
}

/// Counts usages of *every* named type in one pass over the registry, keyed by
/// the referenced `node_type_name`.
///
/// Only names that are actually referenced appear in the map; a network with
/// no usages is simply absent (the panel renders nothing for those anyway).
/// Built-in node type names are counted too — they share the key space with
/// custom networks — so callers that only care about networks should look up
/// by network name rather than iterating the map.
pub fn collect_network_usage_counts(registry: &NodeTypeRegistry) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for network in registry.node_networks.values() {
        count_in_network(network, &mut counts);
    }
    counts
}

/// Walks `scope_path` (a chain of HOF node ids) down from `network`, returning
/// the body network it names, or `None` if the path doesn't resolve. Rooted at
/// an explicit network rather than the *active* one, which is what Find Usages
/// and error-navigation need — the target generally lives in some other network.
pub fn resolve_scope_network<'a>(
    network: &'a NodeNetwork,
    scope_path: &[u64],
) -> Option<&'a NodeNetwork> {
    let mut current = network;
    for hof_id in scope_path {
        current = current.nodes.get(hof_id)?.zone.as_deref()?;
    }
    Some(current)
}

/// Resolves the chain of enclosing HOF nodes named by `scope_path` into
/// display labels, starting from `network`'s top level. Stops early (returning
/// what it has) if the path doesn't resolve — a caller-facing display string is
/// never worth a panic.
pub fn resolve_scope_labels(network: &NodeNetwork, scope_path: &[u64]) -> Vec<String> {
    let mut labels = Vec::with_capacity(scope_path.len());
    let mut current = network;
    for hof_id in scope_path {
        let Some(node) = current.nodes.get(hof_id) else {
            break;
        };
        labels.push(node_label(node));
        let Some(zone) = node.zone.as_deref() else {
            break;
        };
        current = zone;
    }
    labels
}

/// The display label for a node: its name when it has one, its type name
/// otherwise. `add_node` auto-assigns a per-network-unique name (`helper1`,
/// `helper2`, …) which the user can rename, so this is the identifier the
/// text format shows and the one that distinguishes sibling instances; the
/// type-name fallback only fires for a nameless hand-authored node.
pub fn node_label(node: &super::node_network::Node) -> String {
    node.custom_name
        .clone()
        .unwrap_or_else(|| node.node_type_name.clone())
}

/// Recursive chain-tracking walk of one network, reporting every node whose
/// type name is `network_name` along with the body chain it was found in.
fn collect_in_network(
    network: &NodeNetwork,
    scope_path: &mut Vec<u64>,
    report: &mut impl FnMut(&[u64], u64),
    network_name: &str,
) {
    for node in network.nodes.values() {
        if node.node_type_name == network_name {
            report(scope_path, node.id);
        }
        if let Some(body) = node.zone.as_deref() {
            scope_path.push(node.id);
            collect_in_network(body, scope_path, report, network_name);
            scope_path.pop();
        }
    }
}

/// Recursive counting walk — the batched counterpart of [`collect_in_network`].
/// No scope path is needed since only the totals are reported.
fn count_in_network(network: &NodeNetwork, counts: &mut HashMap<String, u32>) {
    for node in network.nodes.values() {
        *counts.entry(node.node_type_name.clone()).or_insert(0) += 1;
        if let Some(body) = node.zone.as_deref() {
            count_in_network(body, counts);
        }
    }
}
