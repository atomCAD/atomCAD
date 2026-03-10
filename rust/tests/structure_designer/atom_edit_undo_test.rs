/// Tests for the atom_edit undo/redo system (Phase A).
///
/// Verifies that the DiffRecorder captures deltas correctly and that
/// AtomEditMutationCommand can undo/redo atom and bond operations.
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, with_atom_edit_undo,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::DiffRecorder;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Helpers
// =============================================================================

fn setup_atom_edit() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);
    // Clear the undo stack so tests start from a clean state
    // (add_node and add_network push their own undo commands)
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

fn diff_atom_count(designer: &mut StructureDesigner) -> usize {
    get_data_mut(designer).diff.get_num_of_atoms()
}

fn diff_bond_count(designer: &mut StructureDesigner) -> usize {
    get_data_mut(designer).diff.get_num_of_bonds()
}

// =============================================================================
// Prerequisite: add_atom_with_id
// =============================================================================

#[test]
fn add_atom_with_id_basic() {
    let mut structure = AtomicStructure::new_diff();
    structure.add_atom_with_id(1, 6, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(structure.get_num_of_atoms(), 1);
    let atom = structure.get_atom(1).unwrap();
    assert_eq!(atom.atomic_number, 6);
    assert!((atom.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn add_atom_with_id_with_gap() {
    let mut structure = AtomicStructure::new_diff();
    // Add atom at ID 1
    structure.add_atom(6, DVec3::ZERO);
    // Add atom at ID 5 (should pad with None)
    structure.add_atom_with_id(5, 7, DVec3::new(1.0, 0.0, 0.0));
    assert_eq!(structure.get_num_of_atoms(), 2);
    assert!(structure.get_atom(2).is_none());
    assert!(structure.get_atom(3).is_none());
    assert!(structure.get_atom(4).is_none());
    assert!(structure.get_atom(5).is_some());
}

#[test]
fn add_atom_with_id_updates_grid() {
    let mut structure = AtomicStructure::new_diff();
    let pos = DVec3::new(1.0, 2.0, 3.0);
    structure.add_atom_with_id(1, 6, pos);
    let nearby = structure.get_atoms_in_radius(&pos, 1.0);
    assert!(nearby.contains(&1));
}

#[test]
fn add_atom_with_id_increments_num_atoms() {
    let mut structure = AtomicStructure::new_diff();
    assert_eq!(structure.get_num_of_atoms(), 0);
    structure.add_atom_with_id(3, 6, DVec3::ZERO);
    assert_eq!(structure.get_num_of_atoms(), 1);
    structure.add_atom_with_id(5, 7, DVec3::X);
    assert_eq!(structure.get_num_of_atoms(), 2);
}

#[test]
#[should_panic(expected = "Slot 1 already occupied")]
fn add_atom_with_id_panics_on_occupied_slot() {
    let mut structure = AtomicStructure::new_diff();
    structure.add_atom_with_id(1, 6, DVec3::ZERO);
    structure.add_atom_with_id(1, 7, DVec3::X); // should panic
}

// =============================================================================
// DiffRecorder Coalescing
// =============================================================================

#[test]
fn coalesce_modified_modified() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::{
        AtomDelta, AtomState,
    };

    let mut rec = DiffRecorder::default();
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::ZERO,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
    });
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::Y,
            anchor: None,
        }),
    });
    rec.coalesce();
    assert_eq!(rec.atom_deltas.len(), 1);
    let d = &rec.atom_deltas[0];
    assert!((d.before.as_ref().unwrap().position - DVec3::ZERO).length() < 1e-10);
    assert!((d.after.as_ref().unwrap().position - DVec3::Y).length() < 1e-10);
}

#[test]
fn coalesce_added_modified() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::{
        AtomDelta, AtomState,
    };

    let mut rec = DiffRecorder::default();
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: None,
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
    });
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::Y,
            anchor: None,
        }),
    });
    rec.coalesce();
    assert_eq!(rec.atom_deltas.len(), 1);
    let d = &rec.atom_deltas[0];
    assert!(d.before.is_none()); // Still an Add
    assert!((d.after.as_ref().unwrap().position - DVec3::Y).length() < 1e-10);
}

#[test]
fn coalesce_modified_removed() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::{
        AtomDelta, AtomState,
    };

    let mut rec = DiffRecorder::default();
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::ZERO,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
    });
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
        after: None,
    });
    rec.coalesce();
    assert_eq!(rec.atom_deltas.len(), 1);
    let d = &rec.atom_deltas[0];
    assert!((d.before.as_ref().unwrap().position - DVec3::ZERO).length() < 1e-10);
    assert!(d.after.is_none()); // Removed
}

#[test]
fn coalesce_different_atoms_not_merged() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::{
        AtomDelta, AtomState,
    };

    let mut rec = DiffRecorder::default();
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::ZERO,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
    });
    rec.atom_deltas.push(AtomDelta {
        atom_id: 2,
        before: Some(AtomState {
            atomic_number: 7,
            position: DVec3::Y,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 7,
            position: DVec3::Z,
            anchor: None,
        }),
    });
    rec.coalesce();
    assert_eq!(rec.atom_deltas.len(), 2);
}

#[test]
fn coalesce_non_consecutive_not_merged() {
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::{
        AtomDelta, AtomState,
    };

    let mut rec = DiffRecorder::default();
    // Modified(id=1), Modified(id=2), Modified(id=1)
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::ZERO,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
    });
    rec.atom_deltas.push(AtomDelta {
        atom_id: 2,
        before: Some(AtomState {
            atomic_number: 7,
            position: DVec3::Y,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 7,
            position: DVec3::Z,
            anchor: None,
        }),
    });
    rec.atom_deltas.push(AtomDelta {
        atom_id: 1,
        before: Some(AtomState {
            atomic_number: 6,
            position: DVec3::X,
            anchor: None,
        }),
        after: Some(AtomState {
            atomic_number: 6,
            position: DVec3::Y,
            anchor: None,
        }),
    });
    rec.coalesce();
    // Non-consecutive same-atom deltas are NOT merged
    assert_eq!(rec.atom_deltas.len(), 3);
}

// =============================================================================
// Recording: add_atom_to_diff records correctly
// =============================================================================

#[test]
fn recording_add_atom_produces_delta() {
    let mut data = AtomEditData::new();
    data.begin_recording();
    let id = data.add_atom_to_diff(6, DVec3::new(1.0, 2.0, 3.0));
    let rec = data.end_recording().unwrap();
    assert_eq!(rec.atom_deltas.len(), 1);
    let delta = &rec.atom_deltas[0];
    assert_eq!(delta.atom_id, id);
    assert!(delta.before.is_none());
    let after = delta.after.as_ref().unwrap();
    assert_eq!(after.atomic_number, 6);
    assert!((after.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn recording_remove_from_diff_produces_delta() {
    let mut data = AtomEditData::new();
    let id = data.add_atom_to_diff(6, DVec3::new(1.0, 2.0, 3.0));
    data.begin_recording();
    data.remove_from_diff(id);
    let rec = data.end_recording().unwrap();
    assert_eq!(rec.atom_deltas.len(), 1);
    let delta = &rec.atom_deltas[0];
    assert_eq!(delta.atom_id, id);
    assert!(delta.after.is_none());
    let before = delta.before.as_ref().unwrap();
    assert_eq!(before.atomic_number, 6);
}

#[test]
fn recording_move_in_diff_produces_delta() {
    let mut data = AtomEditData::new();
    let id = data.add_atom_to_diff(6, DVec3::ZERO);
    data.begin_recording();
    data.move_in_diff(id, DVec3::new(5.0, 0.0, 0.0));
    let rec = data.end_recording().unwrap();
    assert_eq!(rec.atom_deltas.len(), 1);
    let delta = &rec.atom_deltas[0];
    assert_eq!(delta.atom_id, id);
    let before = delta.before.as_ref().unwrap();
    assert!((before.position - DVec3::ZERO).length() < 1e-10);
    let after = delta.after.as_ref().unwrap();
    assert!((after.position - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-10);
}

#[test]
fn recording_add_bond_produces_delta() {
    let mut data = AtomEditData::new();
    let id1 = data.add_atom_to_diff(6, DVec3::ZERO);
    let id2 = data.add_atom_to_diff(6, DVec3::X);
    data.begin_recording();
    data.add_bond_in_diff(id1, id2, BOND_SINGLE);
    let rec = data.end_recording().unwrap();
    assert_eq!(rec.bond_deltas.len(), 1);
    let delta = &rec.bond_deltas[0];
    assert!(delta.old_order.is_none());
    assert_eq!(delta.new_order, Some(BOND_SINGLE));
}

#[test]
fn recording_without_begin_produces_no_deltas() {
    let mut data = AtomEditData::new();
    // No begin_recording()
    data.add_atom_to_diff(6, DVec3::ZERO);
    assert!(data.end_recording().is_none());
}

// =============================================================================
// with_atom_edit_undo: integration test
// =============================================================================

#[test]
fn undo_atom_edit_add_atom() {
    let mut designer = setup_atom_edit();
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Add an atom via with_atom_edit_undo
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut_inner(sd);
        data.add_atom_to_diff(6, DVec3::new(1.0, 2.0, 3.0));
    });

    assert_eq!(diff_atom_count(&mut designer), 1);
    assert!(designer.undo_stack.can_undo());
    assert_eq!(designer.undo_stack.undo_description(), Some("Add atom"));

    // Undo
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Verify the atom has correct properties after redo
    let data = get_data_mut(&mut designer);
    let atom = data.diff.get_atom(1).unwrap();
    assert_eq!(atom.atomic_number, 6);
    assert!((atom.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn undo_atom_edit_add_atom_with_bond() {
    let mut designer = setup_atom_edit();

    // Add two atoms and a bond
    with_atom_edit_undo(&mut designer, "Add atoms + bond", |sd| {
        let data = get_data_mut_inner(sd);
        let id1 = data.add_atom_to_diff(6, DVec3::ZERO);
        let id2 = data.add_atom_to_diff(6, DVec3::X);
        data.add_bond_in_diff(id1, id2, BOND_SINGLE);
    });

    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo — atoms and bond should be removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Redo — atoms and bond should be restored
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_remove_atom() {
    let mut designer = setup_atom_edit();

    // First add atoms (not undoable in this scope)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(7, DVec3::X);
    }
    assert_eq!(diff_atom_count(&mut designer), 2);

    // Remove one atom via undo recording
    with_atom_edit_undo(&mut designer, "Remove atom", |sd| {
        let data = get_data_mut_inner(sd);
        data.remove_from_diff(1);
    });
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Undo — atom should be restored
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    let data = get_data_mut(&mut designer);
    let atom = data.diff.get_atom(1).unwrap();
    assert_eq!(atom.atomic_number, 6);

    // Redo — atom should be removed again
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_move_atom() {
    let mut designer = setup_atom_edit();

    // Add an atom
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }

    // Move it
    with_atom_edit_undo(&mut designer, "Move atom", |sd| {
        let data = get_data_mut_inner(sd);
        data.move_in_diff(1, DVec3::new(5.0, 0.0, 0.0));
    });

    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert!((atom.position - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-10);
    }

    // Undo — atom should be back at origin
    assert!(designer.undo());
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert!((atom.position - DVec3::ZERO).length() < 1e-10);
    }

    // Redo — atom should be moved again
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert!((atom.position - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-10);
    }
}

#[test]
fn undo_clears_selection() {
    let mut designer = setup_atom_edit();

    // Add an atom and select it
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut_inner(sd);
        let id = data.add_atom_to_diff(6, DVec3::ZERO);
        data.selection.selected_diff_atoms.insert(id);
    });

    {
        let data = get_data_mut(&mut designer);
        assert!(!data.selection.selected_diff_atoms.is_empty());
    }

    // Undo — selection should be cleared
    assert!(designer.undo());
    {
        let data = get_data_mut(&mut designer);
        assert!(data.selection.selected_diff_atoms.is_empty());
        assert!(data.selection.selected_base_atoms.is_empty());
    }
}

#[test]
fn undo_atom_edit_delete_atom_with_bonds_restores_both() {
    let mut designer = setup_atom_edit();

    // Add two bonded atoms (not undoable)
    {
        let data = get_data_mut(&mut designer);
        let id1 = data.add_atom_to_diff(6, DVec3::ZERO);
        let id2 = data.add_atom_to_diff(6, DVec3::X);
        data.add_bond_in_diff(id1, id2, BOND_SINGLE);
    }
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Delete atom 1 (which has a bond to atom 2)
    with_atom_edit_undo(&mut designer, "Delete atom", |sd| {
        let data = get_data_mut_inner(sd);
        data.remove_from_diff(1);
    });
    assert_eq!(diff_atom_count(&mut designer), 1);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Undo — atom and bond should both be restored (three-pass ordering)
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Verify bond is correct
    let data = get_data_mut(&mut designer);
    let atom1 = data.diff.get_atom(1).unwrap();
    assert_eq!(atom1.bonds.len(), 1);
    assert_eq!(atom1.bonds[0].other_atom_id(), 2);
}

#[test]
fn no_command_pushed_for_empty_recording() {
    let mut designer = setup_atom_edit();

    with_atom_edit_undo(&mut designer, "No-op", |_sd| {
        // Mutation closure does nothing
    });

    assert!(!designer.undo_stack.can_undo());
}

// =============================================================================
// Inner helper: get AtomEditData without marking changed
// =============================================================================

fn get_data_mut_inner(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}
