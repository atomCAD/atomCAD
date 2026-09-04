//! Finding a node by its **name path** — `doc/design_node_names_in_ui.md` D8.
//!
//! Since Phase 0 (D1) a node's `custom_name` is unique within its scope, so the
//! name is the identifier the GUI, the text format and the AI all agree on. A
//! node in a HOF / closure body is addressed by joining the stored names of its
//! scope chain with `/` — `map4/e1` — exactly the spelling
//! [`error_node_path`](super::scoped_validation_errors::error_node_path), the AI
//! History diff hunk titles and the layout log already compose. Backtick
//! quoting is a *text-format* concern and never appears in a path, and `/` is
//! refused inside a name at the source, so splitting a path on `/` is
//! unambiguous.
//!
//! Two entry points over one walk:
//!
//! - [`resolve_node_path`] — exact: one path, one node (or nothing). Used by
//!   the AI History panel's jump (D12).
//! - [`find_nodes_by_name`] — substring search over every scope, ranked. Used
//!   by the Find Node picker (D7).
//!
//! Everything here is read-only: no mutation, no undo, no refresh.

use super::network_usages::node_label;
use super::node_network::NodeNetwork;
use super::node_type_registry::NodeTypeRegistry;

/// One node addressed by the same triple the rest of the codebase uses
/// (`network` + `scope_path` + `node_id`), plus the display strings the picker
/// renders — resolved here so Flutter re-derives nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeNameMatch {
    /// Name of the network holding the node.
    pub network: String,
    /// Chain of HOF node ids from that network's top level down to the body the
    /// node lives in. Empty for a top-level node.
    pub scope_path: Vec<u64>,
    /// Id of the node **within its own scope**.
    pub node_id: u64,
    /// The node's name path: `e1` at the top level, `map4/e1` one body down.
    pub name_path: String,
    /// The node's type name (`expr`, `union`, or a custom network's name).
    pub node_type_name: String,
}

/// A node addressed within one network — the result of an exact path lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRef {
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

/// How well a candidate path matched the query, best first. Only used for
/// ordering; the picker shows no rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchRank {
    Exact,
    Prefix,
    Substring,
}

/// Resolves `path` (`e1`, `map4/e1`, `map4/c1/e1`) against `network_name`,
/// segment by segment: each segment names a node in the current scope by its
/// stored `custom_name`, and every segment but the last must own a body to
/// descend into. The last segment resolves to the node itself even when that
/// node *is* a body owner — `map4` is the `map` node, not its body.
///
/// `None` for an unknown network, a segment that matches nothing, a segment
/// that would have to descend into a node without a body, or an empty path.
pub fn resolve_node_path(
    registry: &NodeTypeRegistry,
    network_name: &str,
    path: &str,
) -> Option<NodeRef> {
    let network = registry.node_networks.get(network_name)?;
    let segments: Vec<&str> = path.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }

    let mut current: &NodeNetwork = network;
    let mut scope_path = Vec::with_capacity(segments.len() - 1);
    for (index, segment) in segments.iter().enumerate() {
        let node = node_by_name(current, segment)?;
        if index + 1 == segments.len() {
            return Some(NodeRef {
                scope_path,
                node_id: node.id,
            });
        }
        // Not the last segment, so this one must be a body owner.
        current = node.zone.as_deref()?;
        scope_path.push(node.id);
    }
    None
}

/// Every node whose name path contains `query` (case-insensitive), ranked
/// exact → prefix → substring and then by path.
///
/// Scope is the active network including its bodies at any depth, or every
/// network when `all_networks` is set; either way the active network's matches
/// come first, so widening the search never moves the rows the user was already
/// looking at. An empty query matches every path, which makes opening the
/// picker double as a name directory for the active network.
///
/// Nodes inside a *collapsed* body are included: collapsing is a canvas display
/// state, not a scope the search knows about.
pub fn find_nodes_by_name(
    registry: &NodeTypeRegistry,
    active_network: Option<&str>,
    query: &str,
    all_networks: bool,
) -> Vec<NodeNameMatch> {
    let needle = query.trim().to_lowercase();

    let mut matches = Vec::new();
    if all_networks {
        for (network_name, network) in registry.node_networks.iter() {
            collect_in_network(network, network_name, &needle, &mut matches);
        }
    } else if let Some(network_name) = active_network
        && let Some(network) = registry.node_networks.get(network_name)
    {
        collect_in_network(network, network_name, &needle, &mut matches);
    }

    matches.sort_by(|a, b| {
        // `false` sorts first, so "is not the active network" puts the active
        // network's matches at the top.
        let a_elsewhere = active_network != Some(a.0.network.as_str());
        let b_elsewhere = active_network != Some(b.0.network.as_str());
        a_elsewhere
            .cmp(&b_elsewhere)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.name_path.cmp(&b.0.name_path))
            .then_with(|| a.0.network.cmp(&b.0.network))
            .then_with(|| a.0.scope_path.cmp(&b.0.scope_path))
            .then_with(|| a.0.node_id.cmp(&b.0.node_id))
    });
    matches.into_iter().map(|(m, _)| m).collect()
}

/// The node named `name` in `network`'s own scope, lowest id first so a legacy
/// file that still holds duplicates resolves deterministically. (Since D1 every
/// write site keeps names unique, so this is a tie-break that should never
/// fire.)
fn node_by_name<'a>(network: &'a NodeNetwork, name: &str) -> Option<&'a super::node_network::Node> {
    network
        .nodes
        .values()
        .filter(|node| node_label(node) == name)
        .min_by_key(|node| node.id)
}

/// Recursive walk of one network, reporting every node whose name path matches
/// `needle`, along with the rank the match earned.
fn collect_in_network(
    network: &NodeNetwork,
    network_name: &str,
    needle: &str,
    out: &mut Vec<(NodeNameMatch, MatchRank)>,
) {
    let mut scope_path = Vec::new();
    let mut segments: Vec<String> = Vec::new();
    collect_in_scope(
        network,
        network_name,
        needle,
        &mut scope_path,
        &mut segments,
        out,
    );
}

fn collect_in_scope(
    network: &NodeNetwork,
    network_name: &str,
    needle: &str,
    scope_path: &mut Vec<u64>,
    segments: &mut Vec<String>,
    out: &mut Vec<(NodeNameMatch, MatchRank)>,
) {
    for node in network.nodes.values() {
        let name = node_label(node);
        segments.push(name);
        let name_path = segments.join("/");
        if let Some(rank) = rank_match(&name_path, needle) {
            out.push((
                NodeNameMatch {
                    network: network_name.to_string(),
                    scope_path: scope_path.clone(),
                    node_id: node.id,
                    name_path,
                    node_type_name: node.node_type_name.clone(),
                },
                rank,
            ));
        }
        if let Some(body) = node.zone.as_deref() {
            scope_path.push(node.id);
            collect_in_scope(body, network_name, needle, scope_path, segments, out);
            scope_path.pop();
        }
        segments.pop();
    }
}

/// Case-insensitive substring match with a rank. An empty needle matches every
/// path as a substring, so the ordering degenerates to "by path".
fn rank_match(name_path: &str, needle: &str) -> Option<MatchRank> {
    if needle.is_empty() {
        return Some(MatchRank::Substring);
    }
    let haystack = name_path.to_lowercase();
    if haystack == needle {
        Some(MatchRank::Exact)
    } else if haystack.starts_with(needle) {
        Some(MatchRank::Prefix)
    } else if haystack.contains(needle) {
        Some(MatchRank::Substring)
    } else {
        None
    }
}
