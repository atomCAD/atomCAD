//! Phase 2 tests for the atom placement guideline (issue #368).
//!
//! Transient state on `AtomEditData` + the core mutation methods
//! (`set_guideline_from_selection`, `set_guideline_position`,
//! `set_guideline_snapped`, `place_atom_on_guideline`, `clear_guideline`,
//! `reset_guideline_snapped`). See `doc/atom_edit/design_atom_guidelines.md`.

use glam::f64::DVec3;
use std::collections::HashMap;

use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, BaseAtomPromotionInfo, DiffAtomKind, Guideline, GuidelineError,
    SelectionProvenance, classify_diff_atom, with_atom_edit_undo,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const EPS: f64 = 1e-9;

// =============================================================================
// Helpers
// =============================================================================

/// Add a diff atom and mark it selected (set + ordered), mirroring how a real
/// selection of a diff atom is recorded.
fn add_selected_diff_atom(data: &mut AtomEditData, atomic_number: i16, pos: DVec3) -> u32 {
    let id = data.diff.add_atom(atomic_number, pos);
    data.selection.selected_diff_atoms.insert(id);
    data.selection.track_selected(SelectionProvenance::Diff, id);
    id
}

/// A guideline along the +x axis through the origin.
fn x_axis_guideline() -> Guideline {
    Guideline::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0))
}

// =============================================================================
// Setup from selection (1 / 2 / 3 atoms)
// =============================================================================

#[test]
fn setup_from_three_atoms_populates_guideline() {
    let mut data = AtomEditData::new();
    // Equilateral triangle in z=0.
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(1.0, 0.0, 0.0);
    let c = DVec3::new(0.5, 3.0_f64.sqrt() / 2.0, 0.0);
    add_selected_diff_atom(&mut data, 6, a);
    add_selected_diff_atom(&mut data, 6, b);
    add_selected_diff_atom(&mut data, 6, c);

    let res = data.set_guideline_from_selection(&HashMap::new(), DVec3::ZERO);
    assert!(res.is_ok());
    let g = data.guideline.expect("guideline should be set");

    // Circumcenter == centroid for an equilateral triangle.
    let centroid = (a + b + c) / 3.0;
    assert!((g.origin - centroid).length() < 1e-9);
    assert!((g.direction.length() - 1.0).abs() < 1e-9);
    assert_eq!(g.t, 0.0);
    assert!(!g.snapped);
}

#[test]
fn setup_from_two_atoms_populates_guideline() {
    let mut data = AtomEditData::new();
    let a = DVec3::new(1.0, 2.0, 3.0);
    let b = DVec3::new(1.0, 2.0, 7.0);
    add_selected_diff_atom(&mut data, 6, a);
    add_selected_diff_atom(&mut data, 6, b);

    data.set_guideline_from_selection(&HashMap::new(), DVec3::ZERO)
        .unwrap();
    let g = data.guideline.unwrap();
    assert!((g.origin - DVec3::new(1.0, 2.0, 5.0)).length() < 1e-9);
    assert!((g.direction - DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
}

#[test]
fn setup_from_one_atom_uses_entered_direction() {
    let mut data = AtomEditData::new();
    let p = DVec3::new(2.0, -1.0, 5.0);
    add_selected_diff_atom(&mut data, 6, p);

    data.set_guideline_from_selection(&HashMap::new(), DVec3::new(0.0, 0.0, 3.0))
        .unwrap();
    let g = data.guideline.unwrap();
    assert!((g.origin - p).length() < 1e-9);
    assert!((g.direction - DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
}

#[test]
fn setup_from_two_atoms_direction_follows_selection_order() {
    // Selecting b before a flips the direction sign.
    let a = DVec3::new(0.0, 0.0, 0.0);
    let b = DVec3::new(0.0, 0.0, 4.0);

    let mut data_ab = AtomEditData::new();
    add_selected_diff_atom(&mut data_ab, 6, a);
    add_selected_diff_atom(&mut data_ab, 6, b);
    data_ab
        .set_guideline_from_selection(&HashMap::new(), DVec3::ZERO)
        .unwrap();

    let mut data_ba = AtomEditData::new();
    add_selected_diff_atom(&mut data_ba, 6, b);
    add_selected_diff_atom(&mut data_ba, 6, a);
    data_ba
        .set_guideline_from_selection(&HashMap::new(), DVec3::ZERO)
        .unwrap();

    let dir_ab = data_ab.guideline.unwrap().direction;
    let dir_ba = data_ba.guideline.unwrap().direction;
    assert!((dir_ab + dir_ba).length() < 1e-9);
}

#[test]
fn setup_resolves_base_atoms_via_supplied_positions() {
    // Two selected base atoms; positions come from the caller-supplied map.
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(10);
    data.selection.selected_base_atoms.insert(20);
    data.selection.track_selected(SelectionProvenance::Base, 10);
    data.selection.track_selected(SelectionProvenance::Base, 20);

    let mut base_positions = HashMap::new();
    base_positions.insert(10, DVec3::new(0.0, 0.0, 0.0));
    base_positions.insert(20, DVec3::new(2.0, 0.0, 0.0));

    data.set_guideline_from_selection(&base_positions, DVec3::ZERO)
        .unwrap();
    let g = data.guideline.unwrap();
    assert!((g.origin - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
    assert!((g.direction - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
}

// =============================================================================
// Degenerate setup → Err, guideline untouched
// =============================================================================

#[test]
fn setup_collinear_three_atoms_returns_err_and_leaves_none() {
    let mut data = AtomEditData::new();
    add_selected_diff_atom(&mut data, 6, DVec3::new(0.0, 0.0, 0.0));
    add_selected_diff_atom(&mut data, 6, DVec3::new(1.0, 0.0, 0.0));
    add_selected_diff_atom(&mut data, 6, DVec3::new(2.0, 0.0, 0.0));

    let res = data.set_guideline_from_selection(&HashMap::new(), DVec3::ZERO);
    assert_eq!(res, Err(GuidelineError::Collinear));
    assert!(data.guideline.is_none());
}

#[test]
fn setup_coincident_two_atoms_returns_err() {
    let mut data = AtomEditData::new();
    add_selected_diff_atom(&mut data, 6, DVec3::new(1.0, 1.0, 1.0));
    add_selected_diff_atom(&mut data, 6, DVec3::new(1.0, 1.0, 1.0));

    let res = data.set_guideline_from_selection(&HashMap::new(), DVec3::ZERO);
    assert_eq!(res, Err(GuidelineError::Coincident));
    assert!(data.guideline.is_none());
}

#[test]
fn setup_one_atom_zero_direction_returns_err() {
    let mut data = AtomEditData::new();
    add_selected_diff_atom(&mut data, 6, DVec3::new(1.0, 2.0, 3.0));

    let res = data.set_guideline_from_selection(&HashMap::new(), DVec3::ZERO);
    assert_eq!(res, Err(GuidelineError::ZeroDirection));
    assert!(data.guideline.is_none());
}

// =============================================================================
// place_atom_on_guideline
// =============================================================================

#[test]
fn place_atom_creates_pure_addition_no_bonds() {
    let mut data = AtomEditData::new();
    data.selected_atomic_number = 7;
    let mut g = x_axis_guideline();
    g.t = 3.0;
    data.guideline = Some(g);

    let id = data.place_atom_on_guideline().expect("atom placed");

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
    // Guideline stays active for repeated placement.
    assert!(data.guideline.is_some());
}

#[test]
fn place_atom_without_guideline_is_none() {
    let mut data = AtomEditData::new();
    assert!(data.place_atom_on_guideline().is_none());
    assert_eq!(data.diff.get_num_of_atoms(), 0);
}

// =============================================================================
// set_guideline_position (Move sub-mode)
// =============================================================================

#[test]
fn position_snapped_slides_atom_along_line() {
    let mut data = AtomEditData::new();
    let id = add_selected_diff_atom(&mut data, 6, DVec3::new(2.0, 5.0, 0.0));
    let mut g = x_axis_guideline();
    g.snapped = true;
    data.guideline = Some(g);

    data.set_guideline_position(7.0, None);

    let atom = data.diff.get_atom(id).unwrap();
    // Snapped: lands exactly on the line at t=7.
    assert!((atom.position - DVec3::new(7.0, 0.0, 0.0)).length() < EPS);
    assert_eq!(data.guideline.unwrap().t, 7.0);
}

#[test]
fn position_not_snapped_preserves_perpendicular_offset() {
    let mut data = AtomEditData::new();
    let id = add_selected_diff_atom(&mut data, 6, DVec3::new(2.0, 5.0, 0.0));
    data.guideline = Some(x_axis_guideline()); // snapped = false

    data.set_guideline_position(7.0, None);

    let atom = data.diff.get_atom(id).unwrap();
    // Parallel move: projection becomes 7, the (0,5,0) offset is preserved.
    assert!((atom.position - DVec3::new(7.0, 5.0, 0.0)).length() < EPS);
    let (t, offset) = data.guideline.unwrap().decompose(atom.position);
    assert!((t - 7.0).abs() < EPS);
    assert!((offset - DVec3::new(0.0, 5.0, 0.0)).length() < EPS);
}

#[test]
fn position_place_sub_mode_moves_marker_only() {
    // No atoms selected → Place sub-mode: only the marker (t) moves.
    let mut data = AtomEditData::new();
    data.guideline = Some(x_axis_guideline());

    data.set_guideline_position(4.5, None);

    assert_eq!(data.guideline.unwrap().t, 4.5);
    assert_eq!(data.diff.get_num_of_atoms(), 0);
}

#[test]
fn position_promotes_base_atom() {
    // One base atom selected, off the line; not snapped, preserve offset.
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(42);
    data.selection.track_selected(SelectionProvenance::Base, 42);
    data.guideline = Some(x_axis_guideline());

    let info = BaseAtomPromotionInfo {
        base_id: 42,
        atomic_number: 6,
        position: DVec3::new(2.0, 5.0, 0.0),
        existing_diff_id: None,
        flags: 0,
    };
    data.set_guideline_position(7.0, Some(&info));

    // Base atom promoted to diff; selection migrated.
    assert!(data.selection.selected_base_atoms.is_empty());
    assert_eq!(data.selection.selected_diff_atoms.len(), 1);
    let diff_id = *data.selection.selected_diff_atoms.iter().next().unwrap();
    let atom = data.diff.get_atom(diff_id).unwrap();
    assert!((atom.position - DVec3::new(7.0, 5.0, 0.0)).length() < EPS);
    // Anchor at the original base position → matched-base, not a pure addition.
    assert_eq!(
        classify_diff_atom(&data.diff, diff_id),
        DiffAtomKind::MatchedBase
    );
    let anchor = data.diff.anchor_position(diff_id).copied().unwrap();
    assert!((anchor - DVec3::new(2.0, 5.0, 0.0)).length() < EPS);
}

// =============================================================================
// set_guideline_snapped
// =============================================================================

#[test]
fn snap_on_zeroes_offset_at_current_projection() {
    let mut data = AtomEditData::new();
    let id = add_selected_diff_atom(&mut data, 6, DVec3::new(2.0, 5.0, 0.0));
    data.guideline = Some(x_axis_guideline());

    data.set_guideline_snapped(true, None);

    let atom = data.diff.get_atom(id).unwrap();
    // Offset zeroed, projection (t=2) preserved.
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < EPS);
    let g = data.guideline.unwrap();
    assert!(g.snapped);
    assert!((g.t - 2.0).abs() < EPS);
}

#[test]
fn snap_off_is_geometric_no_op() {
    let mut data = AtomEditData::new();
    let id = add_selected_diff_atom(&mut data, 6, DVec3::new(2.0, 5.0, 0.0));
    let mut g = x_axis_guideline();
    g.snapped = true;
    data.guideline = Some(g);

    data.set_guideline_snapped(false, None);

    let atom = data.diff.get_atom(id).unwrap();
    assert!((atom.position - DVec3::new(2.0, 5.0, 0.0)).length() < EPS);
    assert!(!data.guideline.unwrap().snapped);
}

#[test]
fn snap_on_promotes_base_atom() {
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(42);
    data.selection.track_selected(SelectionProvenance::Base, 42);
    data.guideline = Some(x_axis_guideline());

    let info = BaseAtomPromotionInfo {
        base_id: 42,
        atomic_number: 6,
        position: DVec3::new(2.0, 3.0, 0.0),
        existing_diff_id: None,
        flags: 0,
    };
    data.set_guideline_snapped(true, Some(&info));

    assert!(data.selection.selected_base_atoms.is_empty());
    assert_eq!(data.selection.selected_diff_atoms.len(), 1);
    let diff_id = *data.selection.selected_diff_atoms.iter().next().unwrap();
    let atom = data.diff.get_atom(diff_id).unwrap();
    // Snapped onto the line at the base atom's projection (t=2).
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < EPS);
    let anchor = data.diff.anchor_position(diff_id).copied().unwrap();
    assert!((anchor - DVec3::new(2.0, 3.0, 0.0)).length() < EPS);
}

#[test]
fn snap_on_directional_line_atom_already_on_line_no_move() {
    // Directional line through the atom itself: snapping performs no position change.
    let p = DVec3::new(5.0, -2.0, 1.0);
    let mut data = AtomEditData::new();
    let id = add_selected_diff_atom(&mut data, 6, p);
    data.guideline = Some(Guideline::new(p, DVec3::new(0.0, 0.0, 1.0)));

    data.set_guideline_snapped(true, None);

    let atom = data.diff.get_atom(id).unwrap();
    assert!((atom.position - p).length() < EPS);
    assert!(data.guideline.unwrap().snapped);
}

// =============================================================================
// Reset / clear
// =============================================================================

#[test]
fn clear_guideline_removes_it() {
    let mut data = AtomEditData::new();
    data.guideline = Some(x_axis_guideline());
    data.clear_guideline();
    assert!(data.guideline.is_none());
}

#[test]
fn reset_guideline_snapped_clears_bit_only() {
    let mut data = AtomEditData::new();
    let mut g = x_axis_guideline();
    g.snapped = true;
    g.t = 3.0;
    data.guideline = Some(g);

    data.reset_guideline_snapped();

    let g = data.guideline.unwrap();
    assert!(!g.snapped);
    // The line and position are untouched.
    assert_eq!(g.t, 3.0);
}

#[test]
fn guideline_survives_tool_switch() {
    let mut data = AtomEditData::new();
    data.guideline = Some(x_axis_guideline());
    data.set_active_tool(APIAtomEditTool::AddAtom);
    assert!(data.guideline.is_some());
    data.set_active_tool(APIAtomEditTool::Default);
    assert!(data.guideline.is_some());
}

// =============================================================================
// Undo / redo (via StructureDesigner)
// =============================================================================

fn setup_atom_edit() -> StructureDesigner {
    use glam::f64::DVec2;
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);
    designer.undo_stack.clear();
    designer
}

fn get_data_mut(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

#[test]
fn undo_place_atom_removes_it() {
    let mut designer = setup_atom_edit();
    {
        let data = get_data_mut(&mut designer);
        let mut g = x_axis_guideline();
        g.t = 3.0;
        data.guideline = Some(g);
    }

    with_atom_edit_undo(&mut designer, "Place atom on guideline", |sd| {
        get_data_mut(sd).place_atom_on_guideline();
    });
    assert_eq!(get_data_mut(&mut designer).diff.get_num_of_atoms(), 1);

    assert!(designer.undo());
    assert_eq!(get_data_mut(&mut designer).diff.get_num_of_atoms(), 0);

    assert!(designer.redo());
    assert_eq!(get_data_mut(&mut designer).diff.get_num_of_atoms(), 1);
}

#[test]
fn undo_snap_move_restores_offline_position_and_resets_snapped() {
    let mut designer = setup_atom_edit();
    let off_line = DVec3::new(2.0, 5.0, 0.0);
    let id = {
        let data = get_data_mut(&mut designer);
        let id = add_selected_diff_atom(data, 6, off_line);
        data.guideline = Some(x_axis_guideline());
        id
    };

    // Snap the atom onto the line (a recorded move).
    with_atom_edit_undo(&mut designer, "Snap to guideline", |sd| {
        get_data_mut(sd).set_guideline_snapped(true, None);
    });
    {
        let data = get_data_mut(&mut designer);
        assert!(
            (data.diff.get_atom(id).unwrap().position - DVec3::new(2.0, 0.0, 0.0)).length() < EPS
        );
        assert!(data.guideline.unwrap().snapped);
    }

    // Undo restores the off-line position AND resets the snapped bit (issue #1 guard).
    assert!(designer.undo());
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(id).unwrap().position - off_line).length() < EPS);
        assert!(
            !data.guideline.unwrap().snapped,
            "stale snapped bit must reset on undo"
        );
    }
}
