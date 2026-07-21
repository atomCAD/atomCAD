//! Eligibility-gated collection of the nodes a refresh pass should render.
//!
//! Historically the scene was built from exactly one map: the top-level
//! network's `displayed_nodes`. Every node inside a zone body was unrenderable,
//! because a body node may reference its enclosing zone's zone-input pins
//! (`element`, `acc`), whose values only exist per invocation.
//!
//! Issue [#409] relaxes that for the one case where the obstacle does not
//! exist — a **0-ary closure**. With no parameters there is no unknown
//! iteration value, so the body's nodes are fully determined and can be
//! evaluated against the live network stack like any top-level node (captures
//! resolve by the ordinary stack walk in
//! `NetworkEvaluator::resolve_incoming_wire`).
//!
//! > A body node is **scene-evaluable** iff every zone-owning ancestor in its
//! > scope chain is a `closure` node with **zero** zone-input pins.
//!
//! We call a scope path satisfying that rule an **eligible chain**. Display
//! flags stored in an *ineligible* body are **dormant**, not cleared: the
//! collection below simply skips them, so flipping a closure's arity 0 → 1
//! stops the body rendering on the next refresh and flipping it back restores
//! the previous display state. See `doc/design_zero_ary_closure_body_display.md`
//! (§"Arity changes: derive, don't mutate").
//!
//! [#409]: https://github.com/atomCAD/atomCAD/issues/409

use crate::structure_designer::node_network::{
    Node, NodeDisplayState, NodeDisplayType, NodeNetwork, NodeRef,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;

/// The node type name of the closure node — the only zone-owning node type a
/// scene-evaluable chain may pass through.
const CLOSURE_NODE_TYPE: &str = "closure";

/// True iff `node` is a `closure` whose **resolved** custom node type declares
/// zero zone-input pins (i.e. a `ClosureKind::Custom` with no parameters).
///
/// Reading the resolved type rather than `ClosureData` directly keeps this in
/// lock-step with what the body's wires may legally reference — it is the same
/// source of truth `network_validator::validate_zones_recursive` uses.
pub fn is_zero_ary_closure(node: &Node, registry: &NodeTypeRegistry) -> bool {
    if node.node_type_name != CLOSURE_NODE_TYPE {
        return false;
    }
    registry
        .get_node_type_for_node(node)
        .is_some_and(|nt| nt.zone_input_pins.is_empty())
}

/// True iff nodes inside `node`'s **body** are scene-evaluable, given whether
/// the chain enclosing `node` already is (`parent_chain_eligible`).
///
/// This is the eligibility rule expressed as a single top-down fold step, and
/// it has two callers that must never diverge:
///
/// * [`collect_recursive`] — decides which bodies contribute displayed nodes to
///   the scene (it descends only from an already-eligible scope, so it folds
///   from `true`).
/// * `api::structure_designer::build_zone_view` — computes
///   `ZoneView::body_scene_evaluable`, the flag Flutter gates the body nodes'
///   eye toggles on.
///
/// If they disagreed the UI would offer eyes for nodes that never render (or
/// hide eyes for nodes that do), so both go through here.
pub fn is_body_scene_evaluable(
    parent_chain_eligible: bool,
    node: &Node,
    registry: &NodeTypeRegistry,
) -> bool {
    parent_chain_eligible && is_zero_ary_closure(node, registry)
}

/// Walk `scope_path` from `root` and return the body network it addresses.
/// An empty path returns `root`. `None` if any hop is missing or is not a
/// zone-owning node.
pub fn resolve_scope_network<'a>(
    root: &'a NodeNetwork,
    scope_path: &[u64],
) -> Option<&'a NodeNetwork> {
    let mut current = root;
    for hof_id in scope_path {
        let node = current.nodes.get(hof_id)?;
        current = node.zone.as_deref()?;
    }
    Some(current)
}

/// True iff every hop of `scope_path` is a 0-ary `closure` (so nodes living at
/// that scope are scene-evaluable). An empty path — the top-level network — is
/// trivially eligible.
pub fn is_eligible_chain(
    root: &NodeNetwork,
    registry: &NodeTypeRegistry,
    scope_path: &[u64],
) -> bool {
    let mut current = root;
    for hof_id in scope_path {
        let Some(node) = current.nodes.get(hof_id) else {
            return false;
        };
        if !is_zero_ary_closure(node, registry) {
            return false;
        }
        let Some(body) = node.zone.as_deref() else {
            return false;
        };
        current = body;
    }
    true
}

/// The display state stored for `node_ref` **at its own scope**, ignoring
/// eligibility (i.e. dormant flags are returned too). `None` when the scope
/// chain doesn't resolve or the node isn't displayed there.
pub fn display_state_at<'a>(
    root: &'a NodeNetwork,
    node_ref: &NodeRef,
) -> Option<&'a NodeDisplayState> {
    resolve_scope_network(root, &node_ref.scope_path)?
        .displayed_nodes
        .get(&node_ref.node_id)
}

/// Collect every displayed node the scene should render, as a scope-aware
/// [`NodeRef`]: the top-level `displayed_nodes`, plus — recursively — the
/// `displayed_nodes` of every body reachable through an **eligible** chain.
/// Ineligible bodies contribute nothing (their stored flags are dormant).
pub fn collect_displayed_node_refs(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Vec<(NodeRef, NodeDisplayType)> {
    let mut out = Vec::new();
    let mut scope_path: Vec<u64> = Vec::new();
    collect_recursive(network, registry, &mut scope_path, &mut out);
    out
}

fn collect_recursive(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    scope_path: &mut Vec<u64>,
    out: &mut Vec<(NodeRef, NodeDisplayType)>,
) {
    for (node_id, state) in &network.displayed_nodes {
        out.push((NodeRef::scoped(scope_path, *node_id), state.display_type));
    }

    // Descend only through 0-ary closures — any other zone owner (map / filter
    // / fold / foreach, or a closure with parameters) blocks the chain, and
    // so does everything nested below it. We only ever recurse from an
    // already-eligible scope, hence the `true`.
    for (node_id, node) in &network.nodes {
        let Some(body) = node.zone.as_deref() else {
            continue;
        };
        if !is_body_scene_evaluable(true, node, registry) {
            continue;
        }
        scope_path.push(*node_id);
        collect_recursive(body, registry, scope_path, out);
        scope_path.pop();
    }
}
