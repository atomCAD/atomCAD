//! Collecting a network's validation errors together with the scope path of
//! the body each one lives in — so the user-types panel can turn an error into
//! a *jump to the offending node*, the same way Find Usages jumps to an
//! instance (error-navigation feature).
//!
//! A body (an HOF / closure zone) is a nested [`NodeNetwork`] with its own
//! `validation_errors` list and its own per-body `next_node_id` counter, so a
//! body error's `node_id` is only meaningful *within that body*. The walk here
//! tracks the chain of enclosing HOF node ids (the `scope_path`) as it descends,
//! which is exactly the address [`jump_to_usage`](crate) / `ScopeResolver` need
//! to navigate into the body.
//!
//! Everything here is read-only: no mutation, no undo, no refresh.

use super::network_validator::ZONE_BODY_INVALID_MARKER;
use super::node_network::NodeNetwork;

/// One validation error paired with the scope path of the body it lives in.
///
/// `scope_path` is the chain of HOF node ids from the network's top level down
/// to the body holding the error — empty for a top-level error. `node_id` is
/// the offending node within that scope, or `None` for a network-level error
/// with no node to anchor to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedValidationError {
    pub scope_path: Vec<u64>,
    pub node_id: Option<u64>,
    pub error_text: String,
    pub blocking: bool,
}

/// Collects every validation error in `network` and, recursively, in its zone
/// bodies at any depth.
///
/// The generic [`ZONE_BODY_INVALID_MARKER`] the validator attaches to an HOF
/// whose body is broken is **skipped**: the body's own error(s) are collected
/// below with a precise `scope_path` + `node_id` that navigate straight to the
/// fault, so the marker would only add a redundant entry that jumps to the HOF
/// instead of the real node. (The marker still drives the HOF's own red badge
/// on the canvas — that path reads `network.validation_errors` directly and is
/// untouched.)
pub fn collect_scoped_validation_errors(network: &NodeNetwork) -> Vec<ScopedValidationError> {
    let mut out = Vec::new();
    let mut scope_path = Vec::new();
    collect_in(network, &mut scope_path, &mut out);
    // Deterministic order (the walk visits `nodes`/bodies in HashMap order) so
    // the panel picker and the F8 "next error" cycle are stable across refreshes.
    // Keyed by `(scope_path, node_id)`, matching Find Usages' sort; node-less
    // network-level errors (`node_id == None`) sort first within their scope.
    out.sort_by(|a, b| {
        a.scope_path
            .cmp(&b.scope_path)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    out
}

fn collect_in(
    network: &NodeNetwork,
    scope_path: &mut Vec<u64>,
    out: &mut Vec<ScopedValidationError>,
) {
    for error in &network.validation_errors {
        if error.error_text == ZONE_BODY_INVALID_MARKER {
            continue;
        }
        out.push(ScopedValidationError {
            scope_path: scope_path.clone(),
            node_id: error.node_id,
            error_text: error.error_text.clone(),
            blocking: error.blocking,
        });
    }
    for node in network.nodes.values() {
        if let Some(body) = node.zone.as_deref() {
            scope_path.push(node.id);
            collect_in(body, scope_path, out);
            scope_path.pop();
        }
    }
}
