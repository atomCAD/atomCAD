//! Pointer state machine for the placement-guideline **tool** (issue #368,
//! Phase 2 viewport).
//!
//! Mirrors `default_tool.rs`: `pointer_down` captures the press (hit-tested to an
//! `AtomRef` or empty), `pointer_move` applies the click-vs-drag threshold and
//! drives the in-progress drag, `pointer_up` commits a click. The constrained
//! drag math itself lives on `AtomEditData` (`guideline_drag_picked_to_ray` /
//! `guideline_drag_ghost_to_ray`) so it stays unit-testable independent of this
//! plumbing. See `doc/atom_edit/design_atom_guidelines.md`.
//!
//! Behaviour by phase (see the design doc's state table):
//! - **Define:** click an atom toggles it in the defining set; click empty clears
//!   the set. No drag.
//! - **Place** (Active, no atom picked): click an atom picks + snaps it onto the
//!   line (→ Move); click empty is a no-op; a drag on the marker / empty space
//!   slides the ghost marker (sets `t`); a drag that begins on an atom picks it
//!   and starts a constrained drag.
//! - **Move** (Active, an atom picked): click the picked atom keeps it; click a
//!   different atom picks that one; click empty unpicks (→ Place); a drag on an
//!   atom constrained-drags it; a drag from empty unpicks (no slide).

use super::atom_edit_data::{
    begin_atom_edit_drag, end_atom_edit_drag, get_active_atom_edit_data,
    get_atom_edit_data_mut_transient, get_selected_atom_edit_data_mut, with_atom_edit_undo,
};
use super::operations::{BaseAtomPromotionInfo, gather_base_atom_promotion_info};
use super::types::*;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, effective_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};
use std::collections::HashSet;

/// Which user-visible state the Guideline tool is in. Derived from the tool
/// phase + whether an atom is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhaseKind {
    Define,
    /// Active, no atom picked (the ghost marker is the active point).
    Place,
    /// Active, an atom picked (it is the active point).
    Move,
}

/// Snapshot of the tool's interaction-relevant state, read under one immutable
/// borrow so the rest of each handler can mutate freely.
struct ToolSnapshot {
    kind: PhaseKind,
    pending: Option<GuidelinePending>,
    drag: GuidelineDragState,
    picked: Option<AtomRef>,
}

fn read_snapshot(structure_designer: &StructureDesigner) -> Option<ToolSnapshot> {
    let data = get_active_atom_edit_data(structure_designer)?;
    match &data.active_tool {
        AtomEditTool::Guideline(tool) => {
            let (kind, drag, picked) = match &tool.phase {
                GuidelinePhase::Define { .. } => {
                    (PhaseKind::Define, GuidelineDragState::Idle, None)
                }
                GuidelinePhase::Active { picked, drag, .. } => {
                    let kind = if picked.is_some() {
                        PhaseKind::Move
                    } else {
                        PhaseKind::Place
                    };
                    (kind, *drag, *picked)
                }
            };
            Some(ToolSnapshot {
                kind,
                pending: tool.pending,
                drag,
                picked,
            })
        }
        _ => None,
    }
}

/// Run `f` against the active node's `GuidelineTool` (transient — no
/// `mark_node_data_changed`). No-op if the active node isn't in the tool.
fn with_tool<F: FnOnce(&mut GuidelineTool)>(structure_designer: &mut StructureDesigner, f: F) {
    if let Some(data) = get_atom_edit_data_mut_transient(structure_designer)
        && let AtomEditTool::Guideline(tool) = &mut data.active_tool
    {
        f(tool);
    }
}

fn set_pending(structure_designer: &mut StructureDesigner, pending: Option<GuidelinePending>) {
    with_tool(structure_designer, |tool| tool.pending = pending);
}

fn set_drag_state(structure_designer: &mut StructureDesigner, state: GuidelineDragState) {
    with_tool(structure_designer, |tool| {
        if let GuidelinePhase::Active { drag, .. } = &mut tool.phase {
            *drag = state;
        }
    });
}

/// Hit-test the cursor ray against the selected node's displayed atoms and
/// resolve the hit to a tool-local `AtomRef` (provenance-mapped in result view,
/// taken directly in diff view). `None` when the ray misses every atom.
fn hit_test_atom_ref(
    structure_designer: &StructureDesigner,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> Option<AtomRef> {
    let atom_id = {
        let result_structure = structure_designer.get_atomic_structure_from_selected_node()?;
        let visualization = &structure_designer
            .preferences
            .atomic_structure_visualization_preferences
            .visualization;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                display_prefs::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                display_prefs::AtomicStructureVisualization::SpaceFilling
            }
        };
        let hit = result_structure.hit_test(
            ray_origin,
            ray_direction,
            visualization,
            |atom| effective_displayed_atom_radius(result_structure, atom, &display_visualization),
            BAS_STICK_RADIUS,
        );
        match hit {
            HitTestResult::Atom(id, _) => id,
            _ => return None,
        }
    };

    // Diff view: the hit id IS a diff atom id.
    if structure_designer.is_selected_node_in_diff_view() {
        return Some(AtomRef::Diff(atom_id));
    }

    // Result view: map the result atom id back to its provenance.
    let cache = structure_designer.get_selected_node_eval_cache()?;
    let cache = cache.downcast_ref::<AtomEditEvalCache>()?;
    match cache.provenance.sources.get(&atom_id)? {
        AtomSource::BasePassthrough(base_id) => Some(AtomRef::Base(*base_id)),
        AtomSource::DiffMatchedBase { diff_id, .. } => Some(AtomRef::Diff(*diff_id)),
        AtomSource::DiffAdded(diff_id) => Some(AtomRef::Diff(*diff_id)),
    }
}

/// Gather base-atom promotion info for picking `atom` (empty unless it is a base
/// atom in result view).
fn gather_pick_promotion(
    structure_designer: &StructureDesigner,
    atom: AtomRef,
) -> Vec<BaseAtomPromotionInfo> {
    match atom {
        AtomRef::Base(base_id) if !structure_designer.is_selected_node_in_diff_view() => {
            let set: HashSet<u32> = std::iter::once(base_id).collect();
            gather_base_atom_promotion_info(structure_designer, &set)
        }
        _ => Vec::new(),
    }
}

/// Apply the pick-and-snap mutation (no undo bookkeeping of its own — the caller
/// wraps it in `with_atom_edit_undo` for a click, or inside an open
/// `begin/end_atom_edit_drag` session for a drag, so the snap coalesces with the
/// subsequent slide into a single undo step).
fn do_pick(
    structure_designer: &mut StructureDesigner,
    atom: AtomRef,
    info: &[BaseAtomPromotionInfo],
) {
    if let Some(data) = get_selected_atom_edit_data_mut(structure_designer) {
        data.guideline_pick_atom(atom, info.first());
    }
}

/// Pick-and-snap as a standalone (click) undo step.
fn pick_and_snap_click(structure_designer: &mut StructureDesigner, atom: AtomRef) {
    let info = gather_pick_promotion(structure_designer, atom);
    with_atom_edit_undo(structure_designer, "Pick atom onto guideline", |sd| {
        do_pick(sd, atom, &info);
    });
}

// =============================================================================
// Public event handlers
// =============================================================================

/// Reset the Guideline tool's transient interaction state (pointer cancel, or
/// switching away from the tool mid-drag). Ends any open drag-undo session.
pub fn guideline_reset_interaction(structure_designer: &mut StructureDesigner) {
    let was_picked_dragging = read_snapshot(structure_designer)
        .map(|s| s.drag == GuidelineDragState::PickedDragging)
        .unwrap_or(false);
    if was_picked_dragging {
        end_atom_edit_drag(structure_designer);
    }
    set_drag_state(structure_designer, GuidelineDragState::Idle);
    set_pending(structure_designer, None);
}

/// Pointer-down: hit-test and record the pending press. No mutation yet — a click
/// commits on `pointer_up`, a drag commits once the threshold is crossed.
pub fn guideline_pointer_down(
    structure_designer: &mut StructureDesigner,
    screen_pos: DVec2,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> bool {
    if read_snapshot(structure_designer).is_none() {
        return false;
    }
    let hit = hit_test_atom_ref(structure_designer, ray_origin, ray_direction);
    set_pending(
        structure_designer,
        Some(GuidelinePending {
            mouse_down: screen_pos,
            hit,
        }),
    );
    false
}

/// Pointer-move: cross the click-vs-drag threshold to start a drag, then drive
/// the in-progress drag. Returns `true` when something visible changed (so the
/// caller can request a redecorate refresh).
pub fn guideline_pointer_move(
    structure_designer: &mut StructureDesigner,
    screen_pos: DVec2,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> bool {
    let snapshot = match read_snapshot(structure_designer) {
        Some(s) => s,
        None => return false,
    };

    // Already dragging: keep driving it.
    match snapshot.drag {
        GuidelineDragState::GhostDragging => {
            return drag_ghost_frame(structure_designer, ray_origin, ray_direction);
        }
        GuidelineDragState::PickedDragging => {
            return drag_picked_frame(
                structure_designer,
                snapshot.picked,
                ray_origin,
                ray_direction,
            );
        }
        GuidelineDragState::Idle => {}
    }

    // Pre-threshold: decide whether to start a drag.
    let pending = match snapshot.pending {
        Some(p) => p,
        None => return false,
    };
    if screen_pos.distance(pending.mouse_down) <= DRAG_THRESHOLD {
        return false;
    }

    match snapshot.kind {
        // Define has no drag — the press still resolves to a click on release.
        PhaseKind::Define => false,
        PhaseKind::Place | PhaseKind::Move => {
            match pending.hit {
                // Drag began on an atom → pick it (unless already picked) and
                // start a constrained drag, coalesced into one undo step.
                Some(atom) => {
                    set_pending(structure_designer, None);
                    begin_atom_edit_drag(structure_designer);
                    if snapshot.picked != Some(atom) {
                        let info = gather_pick_promotion(structure_designer, atom);
                        do_pick(structure_designer, atom, &info);
                    }
                    set_drag_state(structure_designer, GuidelineDragState::PickedDragging);
                    // The pick may have migrated a base atom to a diff id.
                    let picked = read_snapshot(structure_designer).and_then(|s| s.picked);
                    drag_picked_frame(structure_designer, picked, ray_origin, ray_direction)
                }
                // Drag began on empty space / the marker dot.
                None => match snapshot.kind {
                    PhaseKind::Place => {
                        set_pending(structure_designer, None);
                        set_drag_state(structure_designer, GuidelineDragState::GhostDragging);
                        drag_ghost_frame(structure_designer, ray_origin, ray_direction)
                    }
                    // Move mode, drag from empty: unpick (→ Place), no slide.
                    PhaseKind::Move => {
                        set_pending(structure_designer, None);
                        unpick(structure_designer);
                        true
                    }
                    PhaseKind::Define => false,
                },
            }
        }
    }
}

/// Pointer-up: finish a drag, or commit a click.
pub fn guideline_pointer_up(
    structure_designer: &mut StructureDesigner,
    _screen_pos: DVec2,
    _ray_origin: &DVec3,
    _ray_direction: &DVec3,
) -> bool {
    let snapshot = match read_snapshot(structure_designer) {
        Some(s) => s,
        None => return false,
    };

    // End an in-progress drag.
    match snapshot.drag {
        GuidelineDragState::PickedDragging => {
            end_atom_edit_drag(structure_designer);
            set_drag_state(structure_designer, GuidelineDragState::Idle);
            set_pending(structure_designer, None);
            return true;
        }
        GuidelineDragState::GhostDragging => {
            set_drag_state(structure_designer, GuidelineDragState::Idle);
            set_pending(structure_designer, None);
            return true;
        }
        GuidelineDragState::Idle => {}
    }

    // No drag — this was a click. Commit it based on the pending hit.
    let pending = match snapshot.pending {
        Some(p) => p,
        None => return false,
    };
    set_pending(structure_designer, None);

    match snapshot.kind {
        PhaseKind::Define => {
            if let Some(data) = get_atom_edit_data_mut_transient(structure_designer) {
                match pending.hit {
                    Some(atom) => data.guideline_toggle_defining(atom),
                    None => data.guideline_clear_defining(),
                }
            }
            true
        }
        PhaseKind::Place => match pending.hit {
            // Pick + snap → Move.
            Some(atom) => {
                pick_and_snap_click(structure_designer, atom);
                true
            }
            // Click empty in Place: no-op (placement is the Place button only).
            None => false,
        },
        PhaseKind::Move => match pending.hit {
            Some(atom) => {
                if snapshot.picked == Some(atom) {
                    // Click the picked atom: keep it picked.
                    false
                } else {
                    // Click a different atom: pick that one.
                    pick_and_snap_click(structure_designer, atom);
                    true
                }
            }
            // Click empty in Move: unpick → Place.
            None => {
                unpick(structure_designer);
                true
            }
        },
    }
}

// =============================================================================
// Drag-frame + small mutation helpers
// =============================================================================

fn unpick(structure_designer: &mut StructureDesigner) {
    if let Some(data) = get_atom_edit_data_mut_transient(structure_designer) {
        data.guideline_unpick();
    }
}

fn drag_ghost_frame(
    structure_designer: &mut StructureDesigner,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> bool {
    if let Some(data) = get_selected_atom_edit_data_mut(structure_designer) {
        data.guideline_drag_ghost_to_ray(*ray_origin, *ray_direction)
    } else {
        false
    }
}

fn drag_picked_frame(
    structure_designer: &mut StructureDesigner,
    picked: Option<AtomRef>,
    ray_origin: &DVec3,
    ray_direction: &DVec3,
) -> bool {
    // A base-atom pick is migrated to a diff id by `do_pick`, so by drag time the
    // picked atom is a diff atom and needs no promotion info. Gather defensively
    // for the rare directly-set base pick.
    let info = match picked {
        Some(atom) => gather_pick_promotion(structure_designer, atom),
        None => Vec::new(),
    };
    if let Some(data) = get_selected_atom_edit_data_mut(structure_designer) {
        data.guideline_drag_picked_to_ray(*ray_origin, *ray_direction, info.first())
    } else {
        false
    }
}
