//! From a viewport ray to a trace (`doc/design_output_driven_editing.md`, Rust
//! API): which displayed node does a right-click trace from, and the trace of
//! that ray on a chosen node.
//!
//! The overlap rule is click-to-activate's (`pick_displayed_candidates`), with
//! two differences: the active node is **not** privileged (the overlapping
//! outputs are equally plausible sources of the thing clicked), and gadgets are
//! never hit-tested (a right-click traces content, not a gizmo handle).

use glam::f64::DVec3;

use super::TraceReport;
use crate::structure_designer::{DisplayedPick, PerNodeRayHit, StructureDesigner};

/// The answer of [`trace_viewport_pick`].
#[derive(Debug, Clone, PartialEq)]
pub enum TracePick {
    NoHit,
    /// One displayed node was hit (or one is clearly nearest): its trace.
    Traced(TraceReport),
    /// Several displayed nodes were hit within the overlap epsilon, in
    /// distance order. Nothing has been traced: the user picks one, then the
    /// caller asks [`trace_viewport_hit`] for that node.
    Overlapping(Vec<PerNodeRayHit>),
}

/// Step 1 of a right-click: trace the ray's hit if one displayed node is
/// clearly under it, else return the overlapping candidates untraced.
pub fn trace_viewport_pick(
    designer: &mut StructureDesigner,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> TracePick {
    match designer.pick_displayed_candidates(ray_origin, ray_direction, false) {
        DisplayedPick::NoHit => TracePick::NoHit,
        DisplayedPick::Single(hit) => TracePick::Traced(designer.trace_hit(&hit.trace_hit())),
        DisplayedPick::Overlapping(hits) => TracePick::Overlapping(hits),
    }
}

/// Step 2: trace this ray's hit on the top-level displayed node `node_id`.
/// An empty report when the ray does not hit that node.
pub fn trace_viewport_hit(
    designer: &mut StructureDesigner,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    node_id: u64,
) -> TraceReport {
    let visualization = designer
        .preferences
        .atomic_structure_visualization_preferences
        .visualization
        .clone();
    let hit = designer
        .raytrace_per_node(ray_origin, ray_direction, &visualization)
        .into_iter()
        .find(|hit| hit.node_ref.is_top_level() && hit.node_ref.node_id == node_id);
    match hit {
        Some(hit) => designer.trace_hit(&hit.trace_hit()),
        None => TraceReport::default(),
    }
}
