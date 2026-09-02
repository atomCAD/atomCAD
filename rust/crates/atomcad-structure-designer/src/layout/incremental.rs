//! The incremental layout pass, step by step.
//!
//! `doc/design_incremental_layout.md` §Algorithm — eight steps per scope, run
//! inside-out over the scopes of the post-edit network. Layout consumes one
//! [`EditDelta`](crate::layout::EditDelta) per scope and repairs only what that
//! delta broke; the pre-edit drawing is the baseline, however irregular.
//!
//! Phase 2 lands Step 2 and the primitives it stands on
//! ([`crate::layout::motion`]). Steps 1 and 3–8 arrive with block placement in
//! Phase 3 and the repair passes in Phase 4.

use std::collections::HashMap;

use glam::DVec2;

use crate::layout::delta::EditDelta;
use crate::layout::motion::grow_rect;
use crate::node_network::NodeNetwork;

/// **Step 2 — Repair grown nodes.**
///
/// For each `(id, old, new)` in `delta.grown`, ascending id, `grow_rect`. The
/// grown node itself never moves; everything the growth reaches does, by the
/// half-plane shift on x and the cascade on y.
///
/// Positions are re-read between entries — `grow_rect` reads the live network
/// each time — so two grown nodes in one scope compose rather than fight: the
/// second one's shift line and cascade see where the first one left things.
///
/// Runs **before** blocks are placed (Step 3), so a block is fitted against
/// obstacles that have already settled.
///
/// `sizes` is the measured placed set
/// ([`measure_scope`](crate::layout::motion::measure_scope)); its keys are the
/// nodes that count as obstacles, which from Phase 3 deliberately excludes the
/// added nodes until their block is placed.
///
/// Returns `(id, old position, new position)` for every node that moved,
/// ascending id, with one entry per node however many `grow_rect` calls touched
/// it.
pub fn repair_grown(
    network: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    delta: &EditDelta,
) -> Vec<(u64, DVec2, DVec2)> {
    // `id -> (first old position, latest new position)`, so a node moved twice
    // reports the net move rather than two overlapping ones.
    let mut net_moves: HashMap<u64, (DVec2, DVec2)> = HashMap::new();

    let mut grown = delta.grown.clone();
    grown.sort_by_key(|&(id, _, _)| id);
    for (id, old, new) in grown {
        for (moved, from, to) in grow_rect(network, sizes, id, old, new) {
            net_moves
                .entry(moved)
                .and_modify(|entry| entry.1 = to)
                .or_insert((from, to));
        }
    }

    let mut out: Vec<(u64, DVec2, DVec2)> = net_moves
        .into_iter()
        .filter(|(_, (from, to))| from != to)
        .map(|(id, (from, to))| (id, from, to))
        .collect();
    out.sort_by_key(|&(id, _, _)| id);
    out
}
