//! Layout algorithms for full network reorganization.
//!
//! This module provides automatic layout algorithms to reposition all nodes in a network
//! for improved readability and visual organization. These algorithms reorganize the
//! entire network and are used:
//! - When "Auto-Layout Network" is triggered from the menu
//! - After AI edit operations (when auto_layout_after_edit is enabled)
//!
//! # Available Algorithms
//!
//! | Algorithm | Use Case | Status |
//! |-----------|----------|--------|
//! | **Topological Grid** | AI-created networks, general purpose | Implemented |
//! | **Sugiyama** | Complex DAGs requiring minimal edge crossings | Implemented |
//!
//! Note: Incremental positioning of new nodes during editing is handled separately
//! by the `text_format::auto_layout` module, not through these algorithms.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::layout::{layout_network, LayoutAlgorithm};
//!
//! // Layout the entire network using the default algorithm
//! layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);
//! ```
//!
//! # Module Structure
//!
//! - `common.rs` - Shared utilities (depth computation, graph traversal)
//! - `size.rs` - `rendered_node_size`, the one node-size function
//! - `delta.rs` - `EditDelta` / `diff_scope`, the incremental pass's input
//! - `motion.rs` - `shift_half_plane` / `cascade` / `grow_rect`, the two
//!   motion primitives the incremental pass repairs a drawing with
//! - `incremental.rs` - the incremental pass itself, step by step
//! - `topological_grid.rs` - Simple, reliable layered layout
//! - `sugiyama.rs` - Sophisticated layout with crossing minimization

pub mod common;
pub mod delta;
pub mod incremental;
pub mod motion;
pub mod size;
pub mod sugiyama;
pub mod topological_grid;

// Re-export main types and functions
pub use common::LayoutAlgorithm;
pub use delta::{
    DeltaTotals, EditDelta, WireEnd, WireKey, WireSlot, collect_all_wires, diff_scope,
    node_ids_by_path, scopes_inside_out,
};
pub use incremental::{
    Block, BodyFrame, IncrementalOutcome, SLIDE_WINDOW, layout_incremental, layout_scope,
    repair_grown,
};
pub use motion::{
    CascadeDir, Rect, ShiftOutcome, cascade, grow_rect, measure_scope, shift_half_plane,
    snap_shift_line, upstream_closure,
};
pub use size::{rendered_body_size, rendered_node_size, rendered_node_size_by_id};

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use crate::node_network::NodeNetwork;
use crate::node_type_registry::NodeTypeRegistry;

/// Layout the entire network using the specified algorithm.
///
/// This is the main entry point for layout operations. It computes new positions
/// for all nodes in the network based on the selected algorithm, then applies
/// those positions to the network.
///
/// # Arguments
/// * `network` - Mutable reference to the node network to lay out
/// * `registry` - The node type registry for looking up node information
/// * `algorithm` - The layout algorithm to use
///
/// # Example
///
/// ```rust,ignore
/// layout_network(&mut network, &registry, LayoutAlgorithm::TopologicalGrid);
/// ```
pub fn layout_network(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
) {
    layout_network_scoped(network, registry, algorithm, false);
}

/// One scope's share of a body-aware full reflow.
///
/// `scope_path` is the chain of zone-owning node ids from the root down to the
/// body (empty = the top-level network) — the same addressing
/// `UndoContext::network_in_scope_mut` and `ScopedMoves` use, so the caller can
/// turn each entry straight into one undo command.
#[derive(Debug, Clone)]
pub struct ScopeLayoutMoves {
    pub scope_path: Vec<u64>,
    /// `(node id, old position, new position)`, ascending id, for the nodes
    /// that actually moved.
    pub moves: Vec<(u64, DVec2, DVec2)>,
}

/// Reflow **every scope** of `network`, deepest first, and report what moved.
///
/// The full reflow is body-aware from Phase 5 of
/// `doc/design_incremental_layout.md`. Before it, `layout_network` iterated
/// `network.nodes` and never descended into `Node.zone`, so a body was never
/// laid out at all — "the reflow is the cure" (D4) was simply false for any
/// network holding an expanded HOF, and the HOF's own footprint was measured
/// from a body nothing had ever arranged.
///
/// The order is what makes it correct rather than merely recursive: an HOF's
/// rendered size is `max(stored, body content + padding)`, so its body has to
/// settle before the scope holding it is laid out. Same inside-out rule and
/// same ordering (D12) as the incremental pass.
///
/// `respect_hand_moved` is the third documented use of
/// [`Node::hand_moved`](crate::node_network::Node::hand_moved) ("Uses of
/// `hand_moved`", off by default): the flagged nodes keep their exact
/// positions, the algorithm arranges the rest among themselves, and the
/// vertical cascade clears anything the arrangement landed on one. It is the
/// only place in this design where the flag is a constraint rather than a
/// tiebreaker — and it is a constraint the *user* asked for.
pub fn layout_network_scoped(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
    respect_hand_moved: bool,
) -> Vec<ScopeLayoutMoves> {
    let mut out = Vec::new();

    for scope_path in scopes_inside_out_by_id(network) {
        // Immutable phase: the scope lives inside `network`, so positions and
        // sizes are taken before the mutable borrow — the same split every
        // caller of the motion primitives has to make.
        let Some(scope) = scope_ref(network, &scope_path) else {
            continue;
        };
        let pinned: HashSet<u64> = if respect_hand_moved {
            scope
                .nodes
                .iter()
                .filter(|(_, node)| node.hand_moved)
                .map(|(&id, _)| id)
                .collect()
        } else {
            HashSet::new()
        };
        let positions = if pinned.is_empty() {
            compute_layout(scope, registry, algorithm)
        } else {
            compute_layout_around(scope, registry, algorithm, &pinned)
        };
        let sizes = motion::measure_scope(scope, registry);

        let Some(scope) = scope_mut(network, &scope_path) else {
            continue;
        };
        let before: HashMap<u64, DVec2> = scope
            .nodes
            .iter()
            .map(|(&id, node)| (id, node.position))
            .collect();
        for (&node_id, &position) in &positions {
            if let Some(node) = scope.nodes.get_mut(&node_id) {
                node.position = position;
            }
        }

        // Clear whatever the fresh arrangement landed on a pinned node. The
        // cascade is minimal and never moves a `fixed` node, so the user's own
        // placements survive exactly.
        if !pinned.is_empty() {
            let mut pinned_ids: Vec<u64> = pinned.iter().copied().collect();
            pinned_ids.sort_unstable();
            for id in pinned_ids {
                let (Some(&size), Some(node)) = (sizes.get(&id), scope.nodes.get(&id)) else {
                    continue;
                };
                let rect = motion::Rect::new(node.position, size);
                motion::cascade(
                    scope,
                    &sizes,
                    rect,
                    motion::CascadeDir::Split,
                    &pinned,
                    &HashSet::new(),
                );
            }
        }

        let mut moves: Vec<(u64, DVec2, DVec2)> = before
            .into_iter()
            .filter_map(|(id, from)| {
                let to = scope.nodes.get(&id)?.position;
                (to != from).then_some((id, from, to))
            })
            .collect();
        if moves.is_empty() {
            continue;
        }
        moves.sort_by_key(|&(id, _, _)| id);
        out.push(ScopeLayoutMoves { scope_path, moves });
    }

    out
}

/// A full layout of one scope with `pinned` held where they are.
///
/// The movable nodes are laid out among themselves through
/// [`layout_subgraph`], then the pinned ones are written back into the position
/// map at their real coordinates so the comment pass sees the true drawing.
/// Pinned nodes are dropped from the result: the caller must not move them.
fn compute_layout_around(
    scope: &NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
    pinned: &HashSet<u64>,
) -> HashMap<u64, DVec2> {
    let movable: HashSet<u64> = scope
        .nodes
        .keys()
        .copied()
        .filter(|id| !pinned.contains(id))
        .collect();
    let mut positions = layout_subgraph(scope, registry, &movable, algorithm);
    for &id in pinned {
        if let Some(node) = scope.nodes.get(&id) {
            positions.insert(id, node.position);
        }
    }
    common::place_comments(scope, registry, &mut positions);
    positions.retain(|id, _| !pinned.contains(id));
    positions
}

/// Every scope of `network`, deepest first — the id-only sibling of
/// [`delta::scopes_inside_out`].
///
/// The delta's walk is keyed by name path (`text_format::unique_node_names`,
/// the one naming rule of the text layer) and skips a body whose owner has no
/// `custom_name`, which is right for it: the snapshot is name-keyed and an
/// unnamed node cannot be matched. The full reflow has no snapshot and must
/// reach every body there is, so it walks ids alone.
fn scopes_inside_out_by_id(network: &NodeNetwork) -> Vec<Vec<u64>> {
    fn walk(network: &NodeNetwork, path: &mut Vec<u64>, out: &mut Vec<Vec<u64>>) {
        out.push(path.clone());
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(body) = network.nodes[&id].zone.as_deref() else {
                continue;
            };
            path.push(id);
            walk(body, path, out);
            path.pop();
        }
    }

    let mut out = Vec::new();
    walk(network, &mut Vec::new(), &mut out);
    // Deepest first. The walk already emits a deterministic id order and
    // `sort_by_key` is stable, so the result is total (D7).
    out.sort_by_key(|path| std::cmp::Reverse(path.len()));
    out
}

/// Walk down to the network a scope path names.
fn scope_ref<'n>(root: &'n NodeNetwork, scope_path: &[u64]) -> Option<&'n NodeNetwork> {
    let mut network = root;
    for owner_id in scope_path {
        network = network.nodes.get(owner_id)?.zone.as_deref()?;
    }
    Some(network)
}

/// [`scope_ref`] for mutation, through `zone_mut()` so the body `Arc`'s
/// copy-on-write stays intact.
fn scope_mut<'n>(root: &'n mut NodeNetwork, scope_path: &[u64]) -> Option<&'n mut NodeNetwork> {
    let mut network = root;
    for owner_id in scope_path {
        network = network.nodes.get_mut(owner_id)?.zone_mut()?;
    }
    Some(network)
}

/// Compute positions for a **subgraph** — only the nodes in `ids`, wired to
/// each other alone.
///
/// Step 3 of the incremental pass (`doc/design_incremental_layout.md` D2): the
/// freshly added nodes are laid out as a group, in isolation, and the result is
/// placed into the existing drawing as one rigid block. Nothing outside `ids`
/// is read as a neighbour, an edge or an obstacle, and nothing outside `ids`
/// appears in the result.
///
/// The positions are in the algorithm's own frame (starting at
/// `common::START_X` / `START_Y`); the caller normalizes and translates them.
/// Comments are excluded from the layering as everywhere else (D8) and are not
/// placed here — the full reflow's `place_comments` is deliberately not run.
pub fn layout_subgraph(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    ids: &HashSet<u64>,
    algorithm: LayoutAlgorithm,
) -> HashMap<u64, DVec2> {
    match algorithm {
        LayoutAlgorithm::TopologicalGrid => {
            topological_grid::layout_subgraph(network, registry, ids)
        }
        LayoutAlgorithm::Sugiyama => sugiyama::layout_subgraph(network, registry, ids),
    }
}

/// Compute positions for all nodes without modifying the network.
///
/// This function is useful when you want to preview the layout or
/// apply it selectively.
///
/// # Arguments
/// * `network` - The node network to analyze
/// * `registry` - The node type registry
/// * `algorithm` - The layout algorithm to use
///
/// # Returns
/// A HashMap from node ID to computed position
pub fn compute_layout(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    algorithm: LayoutAlgorithm,
) -> HashMap<u64, DVec2> {
    match algorithm {
        LayoutAlgorithm::TopologicalGrid => topological_grid::layout(network, registry),
        LayoutAlgorithm::Sugiyama => sugiyama::layout(network, registry),
    }
}
