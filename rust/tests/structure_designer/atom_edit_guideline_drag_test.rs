//! Phase 4 tests for the atom placement guideline (issue #368).
//!
//! Default-tool **constrained drag**: `AtomEditData::drag_along_guideline` slides
//! the single selected atom to the point on the guideline closest to the cursor
//! ray, but only in Move sub-mode (exactly one atom selected) with a `snapped`
//! guideline. These exercise the projection + apply, not the pointer plumbing.
//! See `doc/atom_edit/design_atom_guidelines.md`.

use glam::f64::DVec3;

use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, AtomEditTool, AtomRef, Guideline, GuidelineDragState, GuidelinePhase,
    GuidelineTool, SelectionProvenance,
};

const EPS: f64 = 1e-9;

// =============================================================================
// Helpers
// =============================================================================

/// Add a diff atom and mark it selected (set + ordered).
fn add_selected_diff_atom(data: &mut AtomEditData, atomic_number: i16, pos: DVec3) -> u32 {
    let id = data.diff.add_atom(atomic_number, pos);
    data.selection.selected_diff_atoms.insert(id);
    data.selection.track_selected(SelectionProvenance::Diff, id);
    id
}

/// A snapped guideline along +x through the origin.
fn snapped_x_axis_guideline() -> Guideline {
    let mut g = Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));
    g.snapped = true;
    g
}

// =============================================================================
// Constrained drag — the move equals point_at(closest_t_to_ray)
// =============================================================================

#[test]
fn constrained_drag_moves_atom_to_closest_point_on_line() {
    let mut data = AtomEditData::new();
    // Atom starts off the line; snapped drag must ignore the offset.
    let id = add_selected_diff_atom(&mut data, 6, DVec3::new(2.0, 7.0, 0.0));
    let g = snapped_x_axis_guideline();
    data.guideline = Some(g);

    // A ray through (5,10,0) pointing -y crosses the x-axis at x = 5 (t = 5).
    let ray_origin = DVec3::new(5.0, 10.0, 0.0);
    let ray_dir = DVec3::new(0.0, -1.0, 0.0);

    let moved = data.drag_along_guideline(ray_origin, ray_dir, None);
    assert!(moved, "snapped single-atom drag should apply");

    let expected_t = g.closest_t_to_ray(ray_origin, ray_dir).unwrap();
    let target = g.point_at(expected_t);
    assert!((target - DVec3::new(5.0, 0.0, 0.0)).length() < EPS);

    let atom = data.diff.get_atom(id).unwrap();
    assert!((atom.position - target).length() < EPS);

    // Off-line (perpendicular) component is zero after the constrained move.
    let (t, offset) = data.guideline.unwrap().decompose(atom.position);
    assert!((t - expected_t).abs() < EPS);
    assert!(offset.length() < EPS);
    // Live `t` updated on the guideline.
    assert!((data.guideline.unwrap().t - expected_t).abs() < EPS);
}

// =============================================================================
// Fall-through cases — caller drops to the free screen-plane drag
// =============================================================================

#[test]
fn not_snapped_single_atom_drag_is_unaffected() {
    let mut data = AtomEditData::new();
    let start = DVec3::new(2.0, 7.0, 0.0);
    let id = add_selected_diff_atom(&mut data, 6, start);
    // Same line, but NOT snapped.
    data.guideline = Some(Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0)));

    let moved =
        data.drag_along_guideline(DVec3::new(5.0, 10.0, 0.0), DVec3::new(0.0, -1.0, 0.0), None);

    assert!(!moved, "not-snapped drag must fall through to free 3D drag");
    // Atom untouched, guideline `t` untouched.
    assert!((data.diff.get_atom(id).unwrap().position - start).length() < EPS);
    assert_eq!(data.guideline.unwrap().t, 0.0);
}

#[test]
fn two_atom_drag_is_unaffected() {
    let mut data = AtomEditData::new();
    let p0 = DVec3::new(2.0, 7.0, 0.0);
    let p1 = DVec3::new(-3.0, 1.0, 4.0);
    let id0 = add_selected_diff_atom(&mut data, 6, p0);
    let id1 = add_selected_diff_atom(&mut data, 6, p1);
    data.guideline = Some(snapped_x_axis_guideline());

    let moved =
        data.drag_along_guideline(DVec3::new(5.0, 10.0, 0.0), DVec3::new(0.0, -1.0, 0.0), None);

    assert!(!moved, "≥2-atom drag must fall through (unconstrained)");
    assert!((data.diff.get_atom(id0).unwrap().position - p0).length() < EPS);
    assert!((data.diff.get_atom(id1).unwrap().position - p1).length() < EPS);
    assert_eq!(data.guideline.unwrap().t, 0.0);
}

#[test]
fn parallel_ray_does_not_move_the_atom() {
    let mut data = AtomEditData::new();
    let start = DVec3::new(2.0, 0.0, 0.0);
    let id = add_selected_diff_atom(&mut data, 6, start);
    data.guideline = Some(snapped_x_axis_guideline());

    // Ray parallel to the x-axis guideline → no unique foot.
    let moved =
        data.drag_along_guideline(DVec3::new(0.0, 5.0, 0.0), DVec3::new(1.0, 0.0, 0.0), None);

    assert!(!moved, "a parallel ray yields no constrained move");
    assert!((data.diff.get_atom(id).unwrap().position - start).length() < EPS);
    assert_eq!(data.guideline.unwrap().t, 0.0);
}

#[test]
fn no_guideline_drag_is_unaffected() {
    let mut data = AtomEditData::new();
    let start = DVec3::new(2.0, 7.0, 0.0);
    let id = add_selected_diff_atom(&mut data, 6, start);
    // No guideline set.

    let moved =
        data.drag_along_guideline(DVec3::new(5.0, 10.0, 0.0), DVec3::new(0.0, -1.0, 0.0), None);

    assert!(!moved);
    assert!((data.diff.get_atom(id).unwrap().position - start).length() < EPS);
}

// =============================================================================
// Phase 2 — tool-based constrained drag (AtomEditTool::Guideline)
//
// `guideline_drag_picked_to_ray` slides the picked atom onto the foot of the
// cursor ray on the line; `guideline_drag_ghost_to_ray` slides the Place-mode
// ghost marker (sets `t`) without moving any atom.
// =============================================================================

/// Enter the Guideline tool's `Active` phase with the given line and pick.
fn enter_active_picked(data: &mut AtomEditData, g: Guideline, picked: Option<AtomRef>) {
    data.active_tool = AtomEditTool::Guideline(GuidelineTool {
        phase: GuidelinePhase::Active {
            guideline: g,
            picked,
            drag: GuidelineDragState::Idle,
        },
        entered_direction: DVec3::ZERO,
        remembered_t: g.t,
        pending: None,
    });
}

/// An x-axis guideline through the origin (tool form; no `snapped` bit).
fn x_axis_guideline() -> Guideline {
    Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0))
}

#[test]
fn tool_picked_drag_moves_atom_to_closest_point_on_line() {
    let mut data = AtomEditData::new();
    // Atom starts off the line; the constrained drag must zero the offset.
    let id = data.diff.add_atom(6, DVec3::new(2.0, 7.0, 0.0));
    let g = x_axis_guideline();
    enter_active_picked(&mut data, g, Some(AtomRef::Diff(id)));

    // A ray through (5,10,0) pointing -y crosses the x-axis at x = 5 (t = 5).
    let ray_origin = DVec3::new(5.0, 10.0, 0.0);
    let ray_dir = DVec3::new(0.0, -1.0, 0.0);

    let moved = data.guideline_drag_picked_to_ray(ray_origin, ray_dir, None);
    assert!(moved, "Move-mode constrained drag should apply");

    let expected_t = g.closest_t_to_ray(ray_origin, ray_dir).unwrap();
    let target = g.point_at(expected_t);
    assert!((target - DVec3::new(5.0, 0.0, 0.0)).length() < EPS);

    let atom = data.diff.get_atom(id).unwrap();
    assert!((atom.position - target).length() < EPS);

    // Off-line (perpendicular) component is zero after the constrained move.
    let active = data.guideline_active().unwrap();
    let (t, offset) = active.decompose(atom.position);
    assert!((t - expected_t).abs() < EPS);
    assert!(offset.length() < EPS);
    // Live `t` tracks the foot.
    assert!((active.t - expected_t).abs() < EPS);
}

#[test]
fn tool_picked_drag_parallel_ray_is_a_no_op() {
    let mut data = AtomEditData::new();
    let start = DVec3::new(2.0, 0.0, 0.0);
    let id = data.diff.add_atom(6, start);
    enter_active_picked(&mut data, x_axis_guideline(), Some(AtomRef::Diff(id)));

    // Ray parallel to the x-axis guideline → no unique foot.
    let moved = data.guideline_drag_picked_to_ray(
        DVec3::new(0.0, 5.0, 0.0),
        DVec3::new(1.0, 0.0, 0.0),
        None,
    );

    assert!(!moved, "a parallel ray yields no constrained move");
    assert!((data.diff.get_atom(id).unwrap().position - start).length() < EPS);
    assert_eq!(data.guideline_active().unwrap().t, 0.0);
}

#[test]
fn tool_picked_drag_without_pick_is_a_no_op() {
    // Place mode (no atom picked): the picked-drag path must not engage.
    let mut data = AtomEditData::new();
    enter_active_picked(&mut data, x_axis_guideline(), None);

    let moved = data.guideline_drag_picked_to_ray(
        DVec3::new(5.0, 10.0, 0.0),
        DVec3::new(0.0, -1.0, 0.0),
        None,
    );
    assert!(!moved, "no pick → picked-drag is a no-op");
    assert_eq!(data.guideline_active().unwrap().t, 0.0);
}

#[test]
fn tool_ghost_drag_sets_t_without_mutating_atoms() {
    let mut data = AtomEditData::new();
    // A stray diff atom that must NOT move during a ghost drag.
    let stray = data.diff.add_atom(6, DVec3::new(9.0, 9.0, 9.0));
    enter_active_picked(&mut data, x_axis_guideline(), None);

    let ray_origin = DVec3::new(5.0, 10.0, 0.0);
    let ray_dir = DVec3::new(0.0, -1.0, 0.0);
    let moved = data.guideline_drag_ghost_to_ray(ray_origin, ray_dir);
    assert!(moved, "Place-mode ghost drag should apply");

    let expected_t = x_axis_guideline()
        .closest_t_to_ray(ray_origin, ray_dir)
        .unwrap();
    assert!((data.guideline_active().unwrap().t - expected_t).abs() < EPS);
    // No atom moved.
    assert!(
        (data.diff.get_atom(stray).unwrap().position - DVec3::new(9.0, 9.0, 9.0)).length() < EPS
    );
}

#[test]
fn tool_ghost_drag_while_picked_is_a_no_op() {
    // Move mode (atom picked): the ghost-drag path must not engage.
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::ZERO);
    enter_active_picked(&mut data, x_axis_guideline(), Some(AtomRef::Diff(id)));

    let moved =
        data.guideline_drag_ghost_to_ray(DVec3::new(5.0, 10.0, 0.0), DVec3::new(0.0, -1.0, 0.0));
    assert!(!moved, "an atom is picked → ghost-drag is a no-op");
    assert_eq!(data.guideline_active().unwrap().t, 0.0);
}
