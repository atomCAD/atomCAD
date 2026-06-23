//! Phase 1 tests for the **tool-based** atom placement guideline (issue #368).
//!
//! Exercises the `AtomEditTool::Guideline` state machine — the `Define` phase
//! (defining set + create), the `Active` phase (place / pick / set-position /
//! unpick / clear), and the undo + node-deselect hooks — entirely from Rust.
//! See `doc/atom_edit/design_atom_guidelines.md` (Phase 1).
//!
//! The legacy modal-guideline tests live in `atom_edit_guideline_state_test.rs`;
//! this file covers the new tool that will replace it.

use glam::f64::DVec3;
use std::collections::HashMap;

use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, AtomEditTool, AtomRef, BaseAtomPromotionInfo, DiffAtomKind, Guideline,
    GuidelineDragState, GuidelineError, GuidelinePhase, GuidelineTool, classify_diff_atom,
    with_atom_edit_undo,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const EPS: f64 = 1e-9;

// =============================================================================
// Helpers
// =============================================================================

/// A guideline along the +x axis through the origin.
fn x_axis_guideline() -> Guideline {
    Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0))
}

/// Put the data into the Guideline tool's `Define` phase (empty defining set).
fn enter_define(data: &mut AtomEditData) {
    data.active_tool = AtomEditTool::Guideline(GuidelineTool::new());
}

/// Put the data into the Guideline tool's `Active` phase with the given line and
/// no atom picked (Place mode).
fn enter_active(data: &mut AtomEditData, g: Guideline) {
    enter_active_picked(data, g, None);
}

/// Put the data into the Guideline tool's `Active` phase with an explicit pick.
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

// =============================================================================
// Define — building the line
// =============================================================================

#[test]
fn toggle_defining_caps_at_three_and_toggles_off() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    data.guideline_toggle_defining(AtomRef::Diff(1));
    data.guideline_toggle_defining(AtomRef::Diff(2));
    data.guideline_toggle_defining(AtomRef::Diff(3));
    data.guideline_toggle_defining(AtomRef::Diff(4)); // capped — ignored
    assert_eq!(data.guideline_defining().len(), 3);

    // Toggling an already-present atom removes it.
    data.guideline_toggle_defining(AtomRef::Diff(2));
    let defining = data.guideline_defining();
    assert_eq!(defining.len(), 2);
    assert!(!defining.contains(&AtomRef::Diff(2)));
}

#[test]
fn clear_defining_empties_the_set() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    data.guideline_toggle_defining(AtomRef::Diff(1));
    data.guideline_toggle_defining(AtomRef::Diff(2));
    data.guideline_clear_defining();
    assert!(data.guideline_defining().is_empty());
}

#[test]
fn create_from_three_diff_atoms_populates_active() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(1.0, 0.0, 0.0);
    let c = DVec3::new(0.5, 3.0_f64.sqrt() / 2.0, 0.0);
    let ia = data.diff.add_atom(6, a);
    let ib = data.diff.add_atom(6, b);
    let ic = data.diff.add_atom(6, c);
    data.guideline_toggle_defining(AtomRef::Diff(ia));
    data.guideline_toggle_defining(AtomRef::Diff(ib));
    data.guideline_toggle_defining(AtomRef::Diff(ic));

    data.guideline_create_from_defining(&HashMap::new())
        .unwrap();

    let g = data.guideline_active().expect("active guideline");
    let centroid = (a + b + c) / 3.0; // == circumcenter for equilateral
    assert!((g.origin - centroid).length() < 1e-9);
    assert!((g.direction.length() - 1.0).abs() < 1e-9);
    assert_eq!(g.t, 0.0);
    // No atom is picked yet — Place mode.
    assert_eq!(data.guideline_picked(), None);
}

#[test]
fn create_from_two_diff_atoms_direction_follows_pick_order() {
    let p0 = DVec3::new(0.0, 0.0, 0.0);
    let p1 = DVec3::new(0.0, 0.0, 4.0);

    let build = |first_then_second: bool| -> Guideline {
        let mut data = AtomEditData::new();
        enter_define(&mut data);
        let i0 = data.diff.add_atom(6, p0);
        let i1 = data.diff.add_atom(6, p1);
        if first_then_second {
            data.guideline_toggle_defining(AtomRef::Diff(i0));
            data.guideline_toggle_defining(AtomRef::Diff(i1));
        } else {
            data.guideline_toggle_defining(AtomRef::Diff(i1));
            data.guideline_toggle_defining(AtomRef::Diff(i0));
        }
        data.guideline_create_from_defining(&HashMap::new())
            .unwrap();
        data.guideline_active().unwrap()
    };

    let dir_ab = build(true).direction;
    let dir_ba = build(false).direction;
    assert!((dir_ab + dir_ba).length() < 1e-9);
}

#[test]
fn create_from_one_atom_uses_entered_direction() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    let p = DVec3::new(2.0, -1.0, 5.0);
    let id = data.diff.add_atom(6, p);
    data.guideline_toggle_defining(AtomRef::Diff(id));
    data.guideline_set_entered_direction(DVec3::new(0.0, 0.0, 3.0));

    data.guideline_create_from_defining(&HashMap::new())
        .unwrap();

    let g = data.guideline_active().unwrap();
    assert!((g.origin - p).length() < 1e-9);
    assert!((g.direction - DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
}

#[test]
fn create_resolves_base_atoms_via_supplied_positions() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    data.guideline_toggle_defining(AtomRef::Base(10));
    data.guideline_toggle_defining(AtomRef::Base(20));

    let mut base_positions = HashMap::new();
    base_positions.insert(10, DVec3::new(0.0, 0.0, 0.0));
    base_positions.insert(20, DVec3::new(2.0, 0.0, 0.0));

    data.guideline_create_from_defining(&base_positions)
        .unwrap();

    let g = data.guideline_active().unwrap();
    assert!((g.origin - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
    assert!((g.direction - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
}

#[test]
fn create_collinear_three_atoms_errs_and_stays_in_define() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    let ia = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let ib = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let ic = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    data.guideline_toggle_defining(AtomRef::Diff(ia));
    data.guideline_toggle_defining(AtomRef::Diff(ib));
    data.guideline_toggle_defining(AtomRef::Diff(ic));

    let res = data.guideline_create_from_defining(&HashMap::new());
    assert_eq!(res, Err(GuidelineError::Collinear));
    // Still in Define with the defining set intact (cheap to retry).
    assert!(data.guideline_active().is_none());
    assert_eq!(data.guideline_defining().len(), 3);
}

// =============================================================================
// Active — place / pick / set-position / unpick
// =============================================================================

#[test]
fn place_atom_creates_pure_addition_and_auto_picks() {
    let mut data = AtomEditData::new();
    data.selected_atomic_number = 7;
    let mut g = x_axis_guideline();
    g.t = 3.0;
    enter_active(&mut data, g);

    let id = data.guideline_place_atom().expect("atom placed");

    assert_eq!(data.diff.get_num_of_atoms(), 1);
    assert_eq!(data.diff.get_num_of_bonds(), 0);
    let atom = data.diff.get_atom(id).unwrap();
    assert_eq!(atom.atomic_number, 7);
    assert!((atom.position - DVec3::new(3.0, 0.0, 0.0)).length() < EPS);
    // Pure addition: no anchor.
    assert_eq!(
        classify_diff_atom(&data.diff, id),
        DiffAtomKind::PureAddition
    );
    assert!(!data.diff.has_anchor_position(id));
    // Auto-pick: now in Move with the just-placed atom picked.
    assert_eq!(data.guideline_picked(), Some(AtomRef::Diff(id)));
}

#[test]
fn place_atom_without_active_phase_is_none() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);
    assert!(data.guideline_place_atom().is_none());
    assert_eq!(data.diff.get_num_of_atoms(), 0);
}

#[test]
fn pick_pure_addition_diff_atom_snaps_with_no_anchor() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(4.0, 3.0, 0.0)); // off the line
    enter_active(&mut data, x_axis_guideline());

    data.guideline_pick_atom(AtomRef::Diff(id), None);

    let atom = data.diff.get_atom(id).unwrap();
    // Snapped onto the line at its projection (t = 4).
    assert!((atom.position - DVec3::new(4.0, 0.0, 0.0)).length() < EPS);
    // Pure addition stays anchor-free.
    assert!(!data.diff.has_anchor_position(id));
    assert_eq!(
        classify_diff_atom(&data.diff, id),
        DiffAtomKind::PureAddition
    );
    assert_eq!(data.guideline_picked(), Some(AtomRef::Diff(id)));
    assert!((data.guideline_active().unwrap().t - 4.0).abs() < EPS);
}

#[test]
fn pick_base_atom_promotes_and_snaps_with_anchor() {
    let mut data = AtomEditData::new();
    enter_active(&mut data, x_axis_guideline());

    let info = BaseAtomPromotionInfo {
        base_id: 42,
        atomic_number: 6,
        position: DVec3::new(2.0, 5.0, 0.0),
        existing_diff_id: None,
        flags: 0,
    };
    data.guideline_pick_atom(AtomRef::Base(42), Some(&info));

    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let diff_id = match data.guideline_picked() {
        Some(AtomRef::Diff(id)) => id,
        other => panic!("expected a promoted diff pick, got {other:?}"),
    };
    let atom = data.diff.get_atom(diff_id).unwrap();
    // Snapped onto the line at the base atom's projection (t = 2).
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < EPS);
    assert_eq!(
        classify_diff_atom(&data.diff, diff_id),
        DiffAtomKind::MatchedBase
    );
    let anchor = data.diff.anchor_position(diff_id).copied().unwrap();
    assert!((anchor - DVec3::new(2.0, 5.0, 0.0)).length() < EPS);
    assert!((data.guideline_active().unwrap().t - 2.0).abs() < EPS);
}

#[test]
fn pick_base_atom_without_promotion_info_is_noop() {
    let mut data = AtomEditData::new();
    enter_active(&mut data, x_axis_guideline());
    data.guideline_pick_atom(AtomRef::Base(42), None);
    assert_eq!(data.diff.get_num_of_atoms(), 0);
    assert_eq!(data.guideline_picked(), None);
}

#[test]
fn set_position_in_move_slides_picked_atom_onto_line() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    enter_active_picked(&mut data, x_axis_guideline(), Some(AtomRef::Diff(id)));

    data.guideline_set_position(7.0, None);

    let atom = data.diff.get_atom(id).unwrap();
    // On-the-line only: lands exactly on the line at t = 7.
    assert!((atom.position - DVec3::new(7.0, 0.0, 0.0)).length() < EPS);
    assert_eq!(data.guideline_active().unwrap().t, 7.0);
}

#[test]
fn set_position_in_place_moves_marker_only() {
    let mut data = AtomEditData::new();
    enter_active(&mut data, x_axis_guideline()); // no pick → Place mode

    data.guideline_set_position(4.5, None);

    assert_eq!(data.guideline_active().unwrap().t, 4.5);
    assert_eq!(data.diff.get_num_of_atoms(), 0);
}

#[test]
fn unpick_returns_to_place_keeping_t() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    let mut g = x_axis_guideline();
    g.t = 5.0;
    enter_active_picked(&mut data, g, Some(AtomRef::Diff(id)));

    data.guideline_unpick();

    assert_eq!(data.guideline_picked(), None);
    // Marker stays where it was left (continuity), atom untouched.
    assert_eq!(data.guideline_active().unwrap().t, 5.0);
    assert!((data.diff.get_atom(id).unwrap().position - DVec3::new(5.0, 0.0, 0.0)).length() < EPS);
}

#[test]
fn direction_and_distance_persist_across_clear_and_rebuild() {
    let mut data = AtomEditData::new();
    enter_define(&mut data);

    // First line: 1-atom directional, with a direction and a distance.
    let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    data.guideline_toggle_defining(AtomRef::Diff(id_a));
    data.guideline_set_entered_direction(DVec3::new(0.0, 0.0, 2.0)); // non-unit
    data.guideline_create_from_defining(&HashMap::new())
        .unwrap();
    data.guideline_set_position(3.5, None); // remembers the distance (Place mode)

    // Clear back to Define and rebuild from a DIFFERENT anchor without
    // re-entering direction or distance.
    data.guideline_tool_clear();
    assert!(data.guideline_active().is_none());
    let id_b = data.diff.add_atom(6, DVec3::new(10.0, 0.0, 0.0));
    data.guideline_toggle_defining(AtomRef::Diff(id_b));
    data.guideline_create_from_defining(&HashMap::new())
        .unwrap();

    let g = data.guideline_active().unwrap();
    assert!((g.origin - DVec3::new(10.0, 0.0, 0.0)).length() < EPS); // new anchor
    assert!((g.direction - DVec3::new(0.0, 0.0, 1.0)).length() < EPS); // remembered dir
    assert!((g.t - 3.5).abs() < EPS); // remembered distance seeds the new line
}

#[test]
fn clear_returns_to_define_dropping_the_line() {
    let mut data = AtomEditData::new();
    enter_active(&mut data, x_axis_guideline());
    data.guideline_tool_clear();
    assert!(data.guideline_active().is_none());
    assert!(data.guideline_defining().is_empty());
    // Tool stays active (still the Guideline tool, now in Define).
    assert!(matches!(data.active_tool, AtomEditTool::Guideline(_)));
}

// =============================================================================
// Undo / redo + node-deselect hooks (via StructureDesigner)
// =============================================================================

fn setup_atom_edit() -> (StructureDesigner, u64) {
    use glam::f64::DVec2;
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);
    designer.undo_stack.clear();
    (designer, node_id)
}

fn data_by_id(designer: &mut StructureDesigner, node_id: u64) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

#[test]
fn undo_place_removes_atom_and_auto_unpicks() {
    let (mut designer, node_id) = setup_atom_edit();
    {
        let data = data_by_id(&mut designer, node_id);
        let mut g = x_axis_guideline();
        g.t = 3.0;
        enter_active(data, g);
    }

    with_atom_edit_undo(&mut designer, "Place atom on guideline", |sd| {
        data_by_id(sd, node_id).guideline_place_atom();
    });
    {
        let data = data_by_id(&mut designer, node_id);
        assert_eq!(data.diff.get_num_of_atoms(), 1);
        assert!(matches!(data.guideline_picked(), Some(AtomRef::Diff(_))));
    }

    assert!(designer.undo());
    {
        let data = data_by_id(&mut designer, node_id);
        assert_eq!(data.diff.get_num_of_atoms(), 0);
        // Auto-unpick: the placed atom is gone, so nothing stays picked.
        assert_eq!(data.guideline_picked(), None);
    }
}

#[test]
fn undo_pick_snap_restores_offline_position_and_auto_unpicks() {
    let (mut designer, node_id) = setup_atom_edit();
    let off_line = DVec3::new(2.0, 5.0, 0.0);
    let id = {
        let data = data_by_id(&mut designer, node_id);
        let id = data.diff.add_atom(6, off_line);
        enter_active(data, x_axis_guideline());
        id
    };

    with_atom_edit_undo(&mut designer, "Pick on guideline", |sd| {
        data_by_id(sd, node_id).guideline_pick_atom(AtomRef::Diff(id), None);
    });
    {
        let data = data_by_id(&mut designer, node_id);
        assert!(
            (data.diff.get_atom(id).unwrap().position - DVec3::new(2.0, 0.0, 0.0)).length() < EPS
        );
        assert_eq!(data.guideline_picked(), Some(AtomRef::Diff(id)));
    }

    assert!(designer.undo());
    {
        let data = data_by_id(&mut designer, node_id);
        assert!((data.diff.get_atom(id).unwrap().position - off_line).length() < EPS);
        assert_eq!(data.guideline_picked(), None);
    }
}

#[test]
fn deselecting_the_node_drops_the_guideline_tool() {
    use glam::f64::DVec2;
    let (mut designer, node_id) = setup_atom_edit();
    {
        let data = data_by_id(&mut designer, node_id);
        enter_active(data, x_axis_guideline());
    }
    // Add and select a different node — leaving the atom_edit node.
    let other_id = designer.add_node("float", DVec2::new(5.0, 0.0));
    designer.select_node(other_id);

    // The atom_edit node's transient Guideline tool is reset to Default.
    let data = data_by_id(&mut designer, node_id);
    assert!(matches!(data.active_tool, AtomEditTool::Default(_)));
}
