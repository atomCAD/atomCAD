//! Last-known evaluation-error snapshots per network — the evaluation half of
//! the unified error list (error-management Phase 4,
//! `doc/design_error_management.md` D6) — plus the root-cause classification
//! that turns a snapshot into a list of *problems* rather than a list of their
//! downstream echoes (Phase 5, D7).
//!
//! Evaluation errors live in the scene (`StructureDesignerScene::node_errors`,
//! keyed by eval-scoped [`NodeRef`]) and only exist for the *active* network's
//! displayed nodes and their upstream cones. So the user-types panel can keep
//! showing a network's runtime errors after the user switches away, each
//! refresh of the active network **harvests** the scene into a per-network
//! snapshot stored on `StructureDesigner` (runtime-only, never serialized).
//! The active network's snapshot is replaced wholesale each refresh (the scene
//! already maintains merged current state across partial refreshes, so
//! harvesting gives replace-not-accumulate semantics for free); an inactive
//! network's snapshot persists and renders dimmed — "from last evaluation".
//!
//! **Harvest scope.** Harvested keys are eval-scoped `NodeRef`s whose scope
//! paths may contain custom-network-instance hops (recorded for child-network
//! internals but not addressable in the active network's coordinate system).
//! Entries whose scope path resolves through the active network's **own
//! zone-body tree** (every hop a zone-owning node) become ordinary rows.
//! Phase 5 adds the rest: a root cause that lives *behind* a
//! custom-network hop enters the list through its jump-ready [`ErrorAddress`]
//! (the terminal origin link's value), and every derived entry carries the
//! address of its root so the panel can collapse it behind that root's row.

use std::collections::{HashMap, HashSet};

use super::network_usages::resolve_scope_network;
use super::node_network::{NodeNetwork, NodeRef};

/// The **global address of a node in the `.cnnd` document** — the same triple
/// Find Usages uses (`APINetworkUsage`): the host network's name, the chain of
/// zone-owning node ids *within that network*, and the node's id in that scope.
///
/// Unlike an eval-scoped [`NodeRef`], whose `scope_path` interleaves zone-body
/// hops and custom-network-instance hops, this is exactly what `jumpToNode`
/// consumes. The network half is the network's *name*: safe because origin
/// links are runtime-only and regenerated on every refresh, and because the one
/// place an address outlives a refresh — a stored snapshot entry — is covered by
/// D6's key lifecycle (`apply_rename_core` rewrites stored names; jump-time
/// validation catches deletions).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorAddress {
    pub host_network: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
}

/// One recorded origin link: "this consumer received an `Error` from *that*
/// node" (`doc/design_error_management.md` D7).
///
/// `address` is the jump-ready global address, computed from the live network
/// stack at record time (the only moment the evaluator knows which hops were
/// zone bodies and which were custom-network entries). `source_ref` is the
/// source's **eval-scoped** ref — the key its *own* links and its own error text
/// are stored under — so following a chain to its root is a plain map walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorOrigin {
    pub address: ErrorAddress,
    pub source_ref: NodeRef,
}

/// The terminal of an origin-link walk: where a derived error actually comes
/// from, plus that node's own error text (shown in the transient landing
/// surface after a cross-network jump — the target may legitimately show no
/// live badge when its network is evaluated standalone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCauseRef {
    pub address: ErrorAddress,
    pub error_text: String,
}

/// One harvested evaluation error: the offending node's address plus the error
/// text. Conceptually the eval-side sibling of `ScopedValidationError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalErrorEntry {
    /// `Some(name)` when the offending node lives in a network **other** than
    /// the one this snapshot belongs to — a cross-network root cause reached
    /// through origin links. `None` = this snapshot's own network, the ordinary
    /// case (`scope_path` is then a plain zone-body chain in it).
    pub host_network: Option<String>,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub error_text: String,
    /// `None` ⇒ this entry **is** a root cause and the panel lists it at top
    /// level. `Some(root)` ⇒ this entry is *derived* (its node received an
    /// `Error` through a wire) and the panel collapses it behind whichever
    /// row(s) represent `root`'s node — validation rows included, since a
    /// derived chain routinely terminates at a cone-poisoned node whose own
    /// eval entry the D8 dedupe drops.
    pub root: Option<RootCauseRef>,
}

/// The D8 dedupe predicate: whether `node_id` carries a **blocking** validation
/// error in `scope`. A poisoned node's eval entry is the synthesized
/// skip-and-synthesize propagation vehicle — showing it would print the same
/// sentence twice for one underlying fact, so every surface drops it. This is
/// deliberately a predicate check, never a text comparison: with several
/// accumulated blocking errors the synthesized join matches no single
/// validation entry byte-for-byte.
pub fn has_blocking_validation_error(scope: &NodeNetwork, node_id: u64) -> bool {
    scope
        .validation_errors
        .iter()
        .any(|e| e.blocking && e.node_id == Some(node_id))
}

/// Follow `start`'s origin links to the end of the link graph and return the
/// root cause (its address + its own error text), or `None` when `start` has no
/// links — i.e. when `start` *is* a root cause.
///
/// Multi-input fan-in makes the link graph a DAG; the walk follows the **first**
/// link (input-pin order), the deterministic choice D7 specifies for "Go to
/// root cause". The other roots are not lost: each is independently visible as
/// its own top-level row, since each is an errored node with no links.
///
/// A `visited` set bounds the walk: a wire cycle that escaped validation can
/// make the link graph cyclic, and a hang here would be exactly the failure
/// mode the cycle rule exists to prevent.
fn resolve_root_cause(
    start: &NodeRef,
    origins: &HashMap<NodeRef, Vec<ErrorOrigin>>,
    errors: &HashMap<NodeRef, String>,
) -> Option<RootCauseRef> {
    let mut visited: HashSet<NodeRef> = HashSet::new();
    let mut current = start.clone();
    let mut terminal: Option<ErrorOrigin> = None;
    while visited.insert(current.clone()) {
        let Some(link) = origins.get(&current).and_then(|links| links.first()) else {
            break;
        };
        current = link.source_ref.clone();
        terminal = Some(link.clone());
    }
    let terminal = terminal?;
    // A self-link (only reachable through a cycle) is not a root cause — the
    // node would be collapsed behind itself and vanish from the panel.
    if terminal.source_ref == *start {
        return None;
    }
    let error_text = errors
        .get(&terminal.source_ref)
        .cloned()
        .unwrap_or_else(|| terminal.address.node_id.to_string());
    Some(RootCauseRef {
        address: terminal.address,
        error_text,
    })
}

/// Harvests the scene's merged evaluation errors (`all_node_errors`) and origin
/// links (`all_origins`) into the snapshot entries for `network` (the active
/// network):
///
/// - entries whose scope path does not resolve through `network`'s own
///   zone-body tree are dropped (custom-network internals are not addressable
///   in this network's coordinate system);
/// - entries whose node no longer exists are dropped;
/// - entries deduped against a blocking validation error on the same node are
///   dropped ([`has_blocking_validation_error`], D8);
/// - every surviving entry is classified: derived entries carry their root
///   cause, root causes carry `None`;
/// - a root cause that lives behind a custom-network hop — invisible to the
///   walk above — is added as its own entry addressed by `host_network`, so the
///   collapse parent of the derived chain is always present in the list.
///
/// The result is sorted deterministically (host network, scope path, node id)
/// so the panel picker and the F8 cycle are stable across refreshes, matching
/// `collect_scoped_validation_errors`.
pub fn harvest_eval_errors(
    network: &NodeNetwork,
    all_node_errors: &HashMap<NodeRef, String>,
    all_origins: &HashMap<NodeRef, Vec<ErrorOrigin>>,
) -> Vec<EvalErrorEntry> {
    let own_name = network.node_type.name.as_str();

    // Cross-network root causes, deduped by address: several derived entries in
    // this network routinely share one root inside a custom network, and the
    // panel wants one row per underlying problem.
    let mut cross_network: HashMap<ErrorAddress, String> = HashMap::new();

    let mut entries: Vec<EvalErrorEntry> = Vec::new();
    for (node_ref, error_text) in all_node_errors {
        let root = resolve_root_cause(node_ref, all_origins, all_node_errors);
        let Some(scope) = resolve_scope_network(network, &node_ref.scope_path) else {
            continue;
        };
        if !scope.nodes.contains_key(&node_ref.node_id) {
            continue;
        }
        if let Some(root) = &root
            && root.address.host_network != own_name
        {
            cross_network
                .entry(root.address.clone())
                .or_insert_with(|| root.error_text.clone());
        }
        if has_blocking_validation_error(scope, node_ref.node_id) {
            // The node is represented by its blocking validation row(s); its
            // synthesized eval entry is dropped from every surface. Derived
            // entries still collapse behind it — they address the *node*, and
            // Flutter matches the validation row by address.
            continue;
        }
        entries.push(EvalErrorEntry {
            host_network: None,
            scope_path: node_ref.scope_path.clone(),
            node_id: node_ref.node_id,
            error_text: error_text.clone(),
            root,
        });
    }

    for (address, error_text) in cross_network {
        // A cross-network root is never deduped against a blocking validation
        // error: that validation row lives in the *other* network's list, so
        // dropping the row here would leave this network's derived entries with
        // no collapse parent at all.
        entries.push(EvalErrorEntry {
            scope_path: address.scope_path,
            node_id: address.node_id,
            host_network: Some(address.host_network),
            error_text,
            root: None,
        });
    }

    entries.sort_by(|a, b| {
        a.host_network
            .cmp(&b.host_network)
            .then_with(|| a.scope_path.cmp(&b.scope_path))
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    entries
}

/// Rewrite every stored network name from `old_name` to `new_name` across all
/// snapshots — the Phase-5 half of D6's key lifecycle. Snapshot entries are
/// deliberately long-lived (they survive network switches) and now embed
/// network *names* in their jump addresses, so a rename must rewrite them or a
/// stored row jumps to a dead name. Called by `apply_rename_core` alongside the
/// map re-key.
pub fn rewrite_network_name_in_snapshots(
    snapshots: &mut HashMap<String, Vec<EvalErrorEntry>>,
    old_name: &str,
    new_name: &str,
) {
    for entries in snapshots.values_mut() {
        for entry in entries.iter_mut() {
            if entry.host_network.as_deref() == Some(old_name) {
                entry.host_network = Some(new_name.to_string());
            }
            if let Some(root) = &mut entry.root
                && root.address.host_network == old_name
            {
                root.address.host_network = new_name.to_string();
            }
        }
    }
}
