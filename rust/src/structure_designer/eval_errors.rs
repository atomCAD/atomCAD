//! Last-known evaluation-error snapshots per network — the evaluation half of
//! the unified error list (error-management Phase 4,
//! `doc/design_error_management.md` D6).
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
//! **Harvest scope (Phase-4 staging).** Harvested keys are eval-scoped
//! `NodeRef`s whose scope paths may contain custom-network-instance hops
//! (recorded for child-network internals but not addressable in the active
//! network's coordinate system). This phase keeps only entries whose scope
//! path resolves through the active network's **own zone-body tree** (every
//! hop must be a zone-owning node) — exactly the set that is viewable today.
//! Phase 5's origin links will add cross-network root causes.

use std::collections::HashMap;

use super::network_usages::resolve_scope_network;
use super::node_network::{NodeNetwork, NodeRef};

/// One harvested evaluation error: the offending node's address within its
/// network (zone-body scope path + node id) and the error text. Conceptually
/// the eval-side sibling of `ScopedValidationError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalErrorEntry {
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    pub error_text: String,
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

/// Harvests the scene's merged evaluation errors (`all_node_errors`) into the
/// snapshot entries for `network` (the active network):
///
/// - entries whose scope path does not resolve through `network`'s own
///   zone-body tree are dropped (custom-network internals — Phase-4 staging);
/// - entries whose node no longer exists are dropped;
/// - entries deduped against a blocking validation error on the same node are
///   dropped ([`has_blocking_validation_error`], D8).
///
/// The result is sorted deterministically (scope path, then node id) so the
/// panel picker and the F8 cycle are stable across refreshes, matching
/// `collect_scoped_validation_errors`.
pub fn harvest_eval_errors(
    network: &NodeNetwork,
    all_node_errors: &HashMap<NodeRef, String>,
) -> Vec<EvalErrorEntry> {
    let mut entries: Vec<EvalErrorEntry> = all_node_errors
        .iter()
        .filter_map(|(node_ref, error_text)| {
            let scope = resolve_scope_network(network, &node_ref.scope_path)?;
            if !scope.nodes.contains_key(&node_ref.node_id) {
                return None;
            }
            if has_blocking_validation_error(scope, node_ref.node_id) {
                return None;
            }
            Some(EvalErrorEntry {
                scope_path: node_ref.scope_path.clone(),
                node_id: node_ref.node_id,
                error_text: error_text.clone(),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.scope_path
            .cmp(&b.scope_path)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    entries
}
