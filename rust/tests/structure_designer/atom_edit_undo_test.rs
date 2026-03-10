/// Tests for the atom_edit undo/redo system (Phases A-D).
///
/// Verifies that the DiffRecorder captures deltas correctly and that
/// AtomEditMutationCommand can undo/redo atom and bond operations,
/// including drag coalescing (Phase C) and complex operations (Phase D).
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{BOND_DOUBLE, BOND_SINGLE};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, BondReference, DELETED_SITE_ATOMIC_NUMBER, UNCHANGED_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, begin_atom_edit_drag, end_atom_edit_drag, with_atom_edit_undo,
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

/// Snapshot helper: returns (atom_count, bond_count, Vec<(id, atomic_number, position)>)
fn snapshot_diff(designer: &mut StructureDesigner) -> (usize, usize, Vec<(u32, i16, DVec3)>) {
    let data = get_data_mut(designer);
    let atom_count = data.diff.get_num_of_atoms();
    let bond_count = data.diff.get_num_of_bonds();
    let mut atoms: Vec<(u32, i16, DVec3)> = data
        .diff
        .iter_atoms()
        .map(|(&id, atom)| (id, atom.atomic_number, atom.position))
        .collect();
    atoms.sort_by_key(|(id, _, _)| *id);
    (atom_count, bond_count, atoms)
}

// =============================================================================
// Phase B: Simple Operations — Delete
// =============================================================================

#[test]
fn undo_atom_edit_delete_diff_atoms() {
    let mut designer = setup_atom_edit();

    // Add 3 atoms (not undoable in this scope)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(7, DVec3::X);
        data.add_atom_to_diff(8, DVec3::Y);
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 3);

    // Delete atoms 1 and 2 via remove_from_diff (simulates diff-view delete)
    with_atom_edit_undo(&mut designer, "Delete atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.remove_from_diff(1);
        data.remove_from_diff(2);
    });
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Undo
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_delete_with_convert_to_delete_marker() {
    let mut designer = setup_atom_edit();

    // Add a diff atom with an anchor (simulates a matched base atom)
    {
        let data = get_data_mut(&mut designer);
        let id = data.add_atom_to_diff(6, DVec3::new(1.0, 0.0, 0.0));
        data.diff.set_anchor_position(id, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 1);
    assert_eq!(before.2[0].1, 6); // atomic_number = 6

    // Convert to delete marker
    with_atom_edit_undo(&mut designer, "Delete atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.convert_to_delete_marker(1);
    });

    // Should still have 1 atom (the delete marker replaced the original)
    // convert_to_delete_marker removes old + adds new marker
    {
        let data = get_data_mut(&mut designer);
        // The delete marker has atomic_number = 0 (DELETED_SITE_ATOMIC_NUMBER)
        let atom_count = data.diff.get_num_of_atoms();
        assert_eq!(atom_count, 1);
    }

    // Undo — should restore original atom
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo — should have delete marker again
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_num_of_atoms(), 1);
    }
}

#[test]
fn undo_atom_edit_mark_for_deletion() {
    let mut designer = setup_atom_edit();
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Mark a position for deletion (adds a delete marker atom)
    with_atom_edit_undo(&mut designer, "Delete atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.mark_for_deletion(DVec3::new(1.0, 2.0, 3.0));
    });
    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    }

    // Undo
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
}

// =============================================================================
// Phase B: Simple Operations — Replace
// =============================================================================

#[test]
fn undo_atom_edit_replace_diff_atoms() {
    let mut designer = setup_atom_edit();

    // Add a carbon atom
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);

    // Replace with nitrogen (atomic_number = 7)
    with_atom_edit_undo(&mut designer, "Replace atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_atomic_number_recorded(1, 7);
    });

    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 7);
    }

    // Undo — should restore carbon
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo — should be nitrogen again
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 7);
    }
}

#[test]
fn undo_atom_edit_replace_with_anchor() {
    let mut designer = setup_atom_edit();

    // Add an UNCHANGED marker (simulates an existing base atom reference)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    }
    let before = snapshot_diff(&mut designer);

    // Replace: set atomic_number + anchor (simulates apply_replace promoting an entry)
    with_atom_edit_undo(&mut designer, "Replace atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_atomic_number_recorded(1, 8); // Oxygen
        data.set_anchor_recorded(1, DVec3::new(1.0, 0.0, 0.0));
    });

    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 8);
        assert!(data.diff.anchor_position(1).is_some());
    }

    // Undo — should restore UNCHANGED marker without anchor
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(
            data.diff.get_atom(1).unwrap().atomic_number,
            UNCHANGED_ATOMIC_NUMBER
        );
        assert!(data.diff.anchor_position(1).is_none());
    }

    // Redo
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 8);
    }
}

// =============================================================================
// Phase B: Simple Operations — Add Bond
// =============================================================================

#[test]
fn undo_atom_edit_add_bond() {
    let mut designer = setup_atom_edit();

    // Add two atoms (not undoable)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(6, DVec3::X);
    }
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Add a bond
    with_atom_edit_undo(&mut designer, "Add bond", |sd| {
        let data = get_data_mut_inner(sd);
        data.add_bond_in_diff(1, 2, BOND_SINGLE);
    });
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo
    assert!(designer.undo());
    assert_eq!(diff_bond_count(&mut designer), 0);
    assert_eq!(diff_atom_count(&mut designer), 2); // atoms should still exist

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_bond_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_add_bond_with_unchanged_promotion() {
    let mut designer = setup_atom_edit();

    // Add one real atom and simulate a bond that requires UNCHANGED promotion
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Add an UNCHANGED atom (base-passthrough promotion) + bond
    with_atom_edit_undo(&mut designer, "Add bond", |sd| {
        let data = get_data_mut_inner(sd);
        let unchanged_id = data.add_atom_recorded(UNCHANGED_ATOMIC_NUMBER, DVec3::X);
        data.add_bond_in_diff(1, unchanged_id, BOND_SINGLE);
    });
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo — UNCHANGED atom and bond should be removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 1);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);
}

// =============================================================================
// Phase B: Simple Operations — Change Bond Order
// =============================================================================

#[test]
fn undo_atom_edit_change_bond_order() {
    let mut designer = setup_atom_edit();

    // Add two bonded atoms (not undoable)
    {
        let data = get_data_mut(&mut designer);
        let id1 = data.add_atom_to_diff(6, DVec3::ZERO);
        let id2 = data.add_atom_to_diff(6, DVec3::X);
        data.add_bond_in_diff(id1, id2, BOND_SINGLE);
    }
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Change bond order to double
    with_atom_edit_undo(&mut designer, "Change bond order", |sd| {
        let data = get_data_mut_inner(sd);
        data.add_bond_in_diff(1, 2, BOND_DOUBLE);
    });

    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        let bond = atom.bonds.iter().find(|b| b.other_atom_id() == 2).unwrap();
        assert_eq!(bond.bond_order(), BOND_DOUBLE);
    }

    // Undo — should restore single bond
    assert!(designer.undo());
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        let bond = atom.bonds.iter().find(|b| b.other_atom_id() == 2).unwrap();
        assert_eq!(bond.bond_order(), BOND_SINGLE);
    }

    // Redo
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        let bond = atom.bonds.iter().find(|b| b.other_atom_id() == 2).unwrap();
        assert_eq!(bond.bond_order(), BOND_DOUBLE);
    }
}

#[test]
fn undo_atom_edit_delete_bond_in_diff() {
    let mut designer = setup_atom_edit();

    // Add two bonded atoms (not undoable)
    {
        let data = get_data_mut(&mut designer);
        let id1 = data.add_atom_to_diff(6, DVec3::ZERO);
        let id2 = data.add_atom_to_diff(6, DVec3::X);
        data.add_bond_in_diff(id1, id2, BOND_SINGLE);
    }
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Delete the bond (simulates diff-view bond deletion)
    with_atom_edit_undo(&mut designer, "Delete bond", |sd| {
        let data = get_data_mut_inner(sd);
        data.delete_bond_recorded(&BondReference {
            atom_id1: 1,
            atom_id2: 2,
        });
    });
    assert_eq!(diff_bond_count(&mut designer), 0);
    assert_eq!(diff_atom_count(&mut designer), 2); // atoms remain

    // Undo — bond should be restored
    assert!(designer.undo());
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_bond_count(&mut designer), 0);
}

// =============================================================================
// Phase B: Simple Operations — Transform
// =============================================================================

#[test]
fn undo_atom_edit_transform_diff_atoms() {
    let mut designer = setup_atom_edit();

    // Add atoms (not undoable)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(7, DVec3::X);
    }
    let before = snapshot_diff(&mut designer);

    // Move atoms using recorded methods (simulates apply_transform for diff atoms)
    with_atom_edit_undo(&mut designer, "Move atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.move_in_diff(1, DVec3::new(0.0, 5.0, 0.0));
        data.move_in_diff(2, DVec3::new(1.0, 5.0, 0.0));
    });

    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-10);
    }

    // Undo
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-10);
    }
}

#[test]
fn undo_atom_edit_transform_with_promotion() {
    let mut designer = setup_atom_edit();

    // Simulate promotion: add a new atom with anchor (base atom promotion)
    with_atom_edit_undo(&mut designer, "Move atoms", |sd| {
        let data = get_data_mut_inner(sd);
        let id = data.add_atom_recorded(6, DVec3::new(1.0, 0.0, 0.0));
        data.set_anchor_recorded(id, DVec3::ZERO);
    });

    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        assert!(data.diff.anchor_position(1).is_some());
    }

    // Undo — atom and anchor should be removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        assert!(data.diff.anchor_position(1).is_some());
    }
}

// =============================================================================
// Phase B: Sequence Tests
// =============================================================================

#[test]
fn undo_atom_edit_multi_step_sequence() {
    let mut designer = setup_atom_edit();

    // Step 1: Add atom
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut_inner(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    let after_add = snapshot_diff(&mut designer);

    // Step 2: Add second atom + bond
    with_atom_edit_undo(&mut designer, "Add atom + bond", |sd| {
        let data = get_data_mut_inner(sd);
        let id2 = data.add_atom_to_diff(7, DVec3::X);
        data.add_bond_in_diff(1, id2, BOND_SINGLE);
    });
    let after_bond = snapshot_diff(&mut designer);

    // Step 3: Replace atom 1
    with_atom_edit_undo(&mut designer, "Replace atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_atomic_number_recorded(1, 8);
    });

    // Undo step 3
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), after_bond);

    // Undo step 2
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), after_add);

    // Undo step 1
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo all
    assert!(designer.redo());
    assert_eq!(snapshot_diff(&mut designer), after_add);
    assert!(designer.redo());
    assert_eq!(snapshot_diff(&mut designer), after_bond);
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 8);
    }
}

#[test]
fn undo_atom_edit_delete_bond_marker_in_diff_view() {
    let mut designer = setup_atom_edit();

    // Add two atoms + a bond "delete marker" (bond_order = BOND_DELETED)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(6, DVec3::X);
        data.delete_bond_in_diff(1, 2); // adds a BOND_DELETED marker
    }
    assert_eq!(diff_bond_count(&mut designer), 1); // the delete marker bond

    // Remove the bond delete marker (simulates diff-view deletion of a bond entry)
    with_atom_edit_undo(&mut designer, "Delete bond", |sd| {
        let data = get_data_mut_inner(sd);
        data.delete_bond_recorded(&BondReference {
            atom_id1: 1,
            atom_id2: 2,
        });
    });
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Undo — bond delete marker should be restored
    assert!(designer.undo());
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_bond_count(&mut designer), 0);
}

// =============================================================================
// Phase C: Drag Coalescing
// =============================================================================

#[test]
fn undo_atom_edit_drag_is_single_step() {
    let mut designer = setup_atom_edit();

    // Add two atoms (not undoable)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(6, DVec3::X);
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 2);

    // Simulate a multi-frame drag: begin_recording, multiple move calls, end_recording
    begin_atom_edit_drag(&mut designer);

    // Frame 1: move both atoms by (0.1, 0, 0)
    {
        let data = get_data_mut_inner(&mut designer);
        data.move_in_diff(1, DVec3::new(0.1, 0.0, 0.0));
        data.move_in_diff(2, DVec3::new(1.1, 0.0, 0.0));
    }
    // Frame 2: move both atoms by another (0.1, 0, 0)
    {
        let data = get_data_mut_inner(&mut designer);
        data.move_in_diff(1, DVec3::new(0.2, 0.0, 0.0));
        data.move_in_diff(2, DVec3::new(1.2, 0.0, 0.0));
    }
    // Frame 3: move to final positions
    {
        let data = get_data_mut_inner(&mut designer);
        data.move_in_diff(1, DVec3::new(1.0, 0.0, 0.0));
        data.move_in_diff(2, DVec3::new(2.0, 0.0, 0.0));
    }

    end_atom_edit_drag(&mut designer);

    // Should have exactly one undo step
    assert!(designer.undo_stack.can_undo());
    assert_eq!(designer.undo_stack.undo_description(), Some("Move atoms"));

    // Verify final positions
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
        assert!((data.diff.get_atom(2).unwrap().position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
    }

    // Undo — should restore original positions in one step
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo — should restore final positions
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
        assert!((data.diff.get_atom(2).unwrap().position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
    }

    // Should not be able to undo further (only one command was pushed)
    assert!(designer.undo());
    assert!(!designer.undo_stack.can_undo());
}

#[test]
fn drag_without_movement_creates_no_command() {
    let mut designer = setup_atom_edit();

    // Add an atom
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }

    // Begin and immediately end drag without any movement
    begin_atom_edit_drag(&mut designer);
    end_atom_edit_drag(&mut designer);

    // No command should be pushed
    assert!(!designer.undo_stack.can_undo());
}

#[test]
fn undo_atom_edit_drag_with_promotion() {
    let mut designer = setup_atom_edit();

    // Add one diff atom (not undoable)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 1);

    // Simulate a drag that includes a base atom promotion:
    // begin_recording, add atom (promotion), move it, end_recording
    begin_atom_edit_drag(&mut designer);

    {
        let data = get_data_mut_inner(&mut designer);
        // Move existing diff atom
        data.move_in_diff(1, DVec3::new(0.5, 0.0, 0.0));
        // Promote a base atom: add + anchor (as drag_selected_by_delta does)
        let new_id = data.add_atom_recorded(7, DVec3::new(1.5, 0.0, 0.0));
        data.set_anchor_recorded(new_id, DVec3::X);
    }

    end_atom_edit_drag(&mut designer);

    assert_eq!(diff_atom_count(&mut designer), 2);
    assert!(designer.undo_stack.can_undo());

    // Undo — should restore to just the original atom
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo — promoted atom should be back
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    {
        let data = get_data_mut(&mut designer);
        let atom2 = data.diff.get_atom(2).unwrap();
        assert_eq!(atom2.atomic_number, 7);
        assert!(data.diff.anchor_position(2).is_some());
    }
}

#[test]
fn undo_atom_edit_drag_cancelled() {
    let mut designer = setup_atom_edit();

    // Add an atom
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);

    // Simulate a drag that gets cancelled after movement
    begin_atom_edit_drag(&mut designer);

    {
        let data = get_data_mut_inner(&mut designer);
        data.move_in_diff(1, DVec3::new(5.0, 0.0, 0.0));
    }

    // End drag (even if it was "cancelled", the positions have been mutated
    // and the undo command records the changes so they can be undone)
    end_atom_edit_drag(&mut designer);

    // A command should have been pushed
    assert!(designer.undo_stack.can_undo());

    // Undo restores original position
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);
}

#[test]
fn undo_atom_edit_drag_coalesces_many_frames() {
    let mut designer = setup_atom_edit();

    // Add 5 atoms
    {
        let data = get_data_mut(&mut designer);
        for i in 0..5 {
            data.add_atom_to_diff(6, DVec3::new(i as f64, 0.0, 0.0));
        }
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 5);

    // Simulate a 60-frame drag moving all atoms by (0, 1, 0) total
    begin_atom_edit_drag(&mut designer);
    for frame in 1..=60 {
        let frac = frame as f64 / 60.0;
        let data = get_data_mut_inner(&mut designer);
        for i in 0..5u32 {
            let id = i + 1;
            let new_pos = DVec3::new(i as f64, frac, 0.0);
            data.move_in_diff(id, new_pos);
        }
    }
    end_atom_edit_drag(&mut designer);

    // Should be a single undo step
    assert!(designer.undo_stack.can_undo());

    // Undo — all atoms back to original positions
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        for i in 0..5u32 {
            let id = i + 1;
            let atom = data.diff.get_atom(id).unwrap();
            assert!((atom.position - DVec3::new(i as f64, 1.0, 0.0)).length() < 1e-10);
        }
    }
}

#[test]
fn undo_atom_edit_drag_then_other_ops() {
    let mut designer = setup_atom_edit();

    // Add an atom (undoable)
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut_inner(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    let after_add = snapshot_diff(&mut designer);

    // Drag the atom
    begin_atom_edit_drag(&mut designer);
    {
        let data = get_data_mut_inner(&mut designer);
        data.move_in_diff(1, DVec3::new(5.0, 0.0, 0.0));
    }
    end_atom_edit_drag(&mut designer);
    let after_drag = snapshot_diff(&mut designer);

    // Replace atom's element
    with_atom_edit_undo(&mut designer, "Replace atoms", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_atomic_number_recorded(1, 7);
    });

    // Undo replace
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), after_drag);

    // Undo drag
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), after_add);

    // Undo add
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo all three
    assert!(designer.redo()); // add
    assert_eq!(snapshot_diff(&mut designer), after_add);
    assert!(designer.redo()); // drag
    assert_eq!(snapshot_diff(&mut designer), after_drag);
    assert!(designer.redo()); // replace
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 7);
    }
}

// =============================================================================
// Phase D: Complex Operations — Minimization
// =============================================================================

#[test]
fn undo_atom_edit_minimize_position_changes() {
    // Simulate what minimization does: move existing diff atoms via set_position_recorded.
    let mut designer = setup_atom_edit();

    // Add two atoms
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::new(0.0, 0.0, 0.0));
        data.add_atom_to_diff(6, DVec3::new(2.0, 0.0, 0.0));
    }
    let before = snapshot_diff(&mut designer);

    // Simulate minimize: move atoms to new positions via recorded methods
    with_atom_edit_undo(&mut designer, "Minimize structure", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_position_recorded(1, DVec3::new(0.1, 0.0, 0.0));
        data.set_position_recorded(2, DVec3::new(1.9, 0.0, 0.0));
    });

    // Verify atoms moved
    {
        let data = get_data_mut(&mut designer);
        let pos1 = data.diff.get_atom(1).unwrap().position;
        let pos2 = data.diff.get_atom(2).unwrap().position;
        assert!((pos1.x - 0.1).abs() < 1e-10);
        assert!((pos2.x - 1.9).abs() < 1e-10);
    }

    // Undo → positions restored
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), before);

    // Redo → positions moved again
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position.x - 0.1).abs() < 1e-10);
    }
}

#[test]
fn undo_atom_edit_minimize_with_base_promotion() {
    // Simulate FreeAll mode: base atoms are promoted to diff with anchor.
    let mut designer = setup_atom_edit();
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Simulate minimize promoting a base atom via add_atom_recorded + set_anchor_recorded
    with_atom_edit_undo(&mut designer, "Minimize structure", |sd| {
        let data = get_data_mut_inner(sd);
        let new_id = data.add_atom_recorded(6, DVec3::new(0.5, 0.0, 0.0));
        data.set_anchor_recorded(new_id, DVec3::ZERO);
    });

    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        assert!(data.diff.anchor_position(1).is_some());
    }

    // Undo → atom removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo → atom re-added with anchor
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        assert!(data.diff.anchor_position(1).is_some());
    }
}

// =============================================================================
// Phase D: Complex Operations — Hydrogen Passivation
// =============================================================================

#[test]
fn undo_atom_edit_add_hydrogen() {
    // Simulate add_hydrogen: adds H atoms + bonds to diff.
    let mut designer = setup_atom_edit();

    // Add a carbon atom as the parent
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);

    // Simulate adding hydrogen: add H atom + bond
    with_atom_edit_undo(&mut designer, "Add hydrogen", |sd| {
        let data = get_data_mut_inner(sd);
        let h_id = data.add_atom_recorded(1, DVec3::new(1.09, 0.0, 0.0));
        data.add_bond_recorded(1, h_id, BOND_SINGLE);
    });

    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo → H atom and bond removed
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Redo → H atom and bond restored
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_add_hydrogen_with_base_promotion() {
    // Simulate add_hydrogen when the parent is a base passthrough atom:
    // Adds UNCHANGED marker for parent + H atom + bond.
    let mut designer = setup_atom_edit();
    assert_eq!(diff_atom_count(&mut designer), 0);

    with_atom_edit_undo(&mut designer, "Add hydrogen", |sd| {
        let data = get_data_mut_inner(sd);
        // Promote base atom (UNCHANGED marker)
        let parent_id = data.add_atom_recorded(UNCHANGED_ATOMIC_NUMBER, DVec3::ZERO);
        // Add H atom
        let h_id = data.add_atom_recorded(1, DVec3::new(1.09, 0.0, 0.0));
        // Bond them
        data.add_bond_recorded(parent_id, h_id, BOND_SINGLE);
    });

    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo → both atoms and bond removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Redo → restored
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_remove_hydrogen() {
    // Simulate remove_hydrogen: removes H atoms from diff.
    let mut designer = setup_atom_edit();

    // Setup: C atom + H atom + bond
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        let h_id = data.add_atom_to_diff(1, DVec3::new(1.09, 0.0, 0.0));
        data.add_bond_in_diff(1, h_id, BOND_SINGLE);
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 2);

    // Remove the hydrogen via remove_from_diff (which is already recorded)
    with_atom_edit_undo(&mut designer, "Remove hydrogen", |sd| {
        let data = get_data_mut_inner(sd);
        data.remove_from_diff(2); // H atom
    });

    assert_eq!(diff_atom_count(&mut designer), 1);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Undo → H atom and bond restored
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Redo → H atom removed again
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
    assert_eq!(diff_bond_count(&mut designer), 0);
}

// =============================================================================
// Phase D: Complex Operations — Modify Measurement
// =============================================================================

#[test]
fn undo_atom_edit_modify_distance_move_diff_atom() {
    // Simulate modify_distance: moves an existing diff atom.
    let mut designer = setup_atom_edit();

    // Add two bonded atoms
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::new(0.0, 0.0, 0.0));
        data.add_atom_to_diff(6, DVec3::new(1.54, 0.0, 0.0));
        data.add_bond_in_diff(1, 2, BOND_SINGLE);
    }
    let before = snapshot_diff(&mut designer);

    // Simulate modify distance: move atom 2 to new position
    with_atom_edit_undo(&mut designer, "Modify distance", |sd| {
        let data = get_data_mut_inner(sd);
        data.move_in_diff(2, DVec3::new(2.0, 0.0, 0.0));
    });

    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(2).unwrap().position.x - 2.0).abs() < 1e-10);
    }

    // Undo → position restored
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), before);

    // Redo → moved again
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(2).unwrap().position.x - 2.0).abs() < 1e-10);
    }
}

#[test]
fn undo_atom_edit_modify_distance_base_promotion() {
    // Simulate modify_distance when atom is base passthrough:
    // adds atom to diff with anchor, then moves it.
    let mut designer = setup_atom_edit();
    assert_eq!(diff_atom_count(&mut designer), 0);

    with_atom_edit_undo(&mut designer, "Modify distance", |sd| {
        let data = get_data_mut_inner(sd);
        let id = data.add_atom_recorded(6, DVec3::new(1.0, 0.0, 0.0));
        data.set_anchor_recorded(id, DVec3::new(1.0, 0.0, 0.0));
        data.move_in_diff(id, DVec3::new(1.5, 0.0, 0.0));
    });

    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert!((atom.position.x - 1.5).abs() < 1e-10);
        assert!(data.diff.anchor_position(1).is_some());
    }

    // Undo → atom removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo → atom re-added in moved position
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 1);
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position.x - 1.5).abs() < 1e-10);
    }
}

#[test]
fn undo_atom_edit_modify_unchanged_marker_promotion() {
    // Simulate modify_measurement when an UNCHANGED marker needs promotion:
    // set_atomic_number_recorded + set_anchor_recorded + move_in_diff
    let mut designer = setup_atom_edit();

    // Add an UNCHANGED marker
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.2[0].1, UNCHANGED_ATOMIC_NUMBER);

    // Promote UNCHANGED marker and move
    with_atom_edit_undo(&mut designer, "Modify distance", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_atomic_number_recorded(1, 6);
        data.set_anchor_recorded(1, DVec3::new(1.0, 0.0, 0.0));
        data.move_in_diff(1, DVec3::new(1.5, 0.0, 0.0));
    });

    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert_eq!(atom.atomic_number, 6);
        assert!((atom.position.x - 1.5).abs() < 1e-10);
        assert!(data.diff.anchor_position(1).is_some());
    }

    // Undo → UNCHANGED marker restored
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);
    {
        let data = get_data_mut(&mut designer);
        let atom = data.diff.get_atom(1).unwrap();
        assert_eq!(atom.atomic_number, UNCHANGED_ATOMIC_NUMBER);
        assert!(data.diff.anchor_position(1).is_none());
    }

    // Redo → promoted and moved
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert_eq!(data.diff.get_atom(1).unwrap().atomic_number, 6);
    }
}

// =============================================================================
// Phase D: Complex Operations — Guided Placement
// =============================================================================

#[test]
fn undo_atom_edit_guided_placement() {
    // Simulate guided placement: adds an atom + bond at a guide dot position.
    let mut designer = setup_atom_edit();

    // Add an anchor atom (the atom the user clicked on to start guided placement)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);

    // Simulate place_guided_atom: add new atom + bond
    with_atom_edit_undo(&mut designer, "Place atom", |sd| {
        let data = get_data_mut_inner(sd);
        let new_id = data.add_atom_to_diff(6, DVec3::new(1.54, 0.0, 0.0));
        data.add_bond_in_diff(1, new_id, BOND_SINGLE);
    });

    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo → new atom and bond removed
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Redo → atom and bond re-added
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_guided_placement_with_base_promotion() {
    // Simulate guided placement where the anchor atom was a base passthrough
    // that was promoted in start_guided_placement.
    let mut designer = setup_atom_edit();
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Promotion happens in start_guided_placement (outside the undo wrapper).
    // The add_atom_recorded is used but no undo command wraps it here.
    // We simulate the full flow as if both steps are inside one undo wrapper.
    with_atom_edit_undo(&mut designer, "Place atom", |sd| {
        let data = get_data_mut_inner(sd);
        // Simulate start_guided_placement promoting base atom
        let anchor_id = data.add_atom_recorded(6, DVec3::ZERO);
        // Simulate place_guided_atom
        let new_id = data.add_atom_to_diff(14, DVec3::new(1.54, 0.0, 0.0));
        data.add_bond_in_diff(anchor_id, new_id, BOND_SINGLE);
    });

    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo → both atoms and bond removed
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Redo
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(diff_bond_count(&mut designer), 1);
}

// =============================================================================
// Phase D: Complex Operations — Multiple Recorded Methods Coalescing
// =============================================================================

#[test]
fn undo_atom_edit_minimize_many_atoms_coalesces() {
    // When minimization moves many atoms, the coalescing should not affect
    // correctness since each atom only gets one set_position_recorded call.
    let mut designer = setup_atom_edit();

    // Add 10 atoms
    {
        let data = get_data_mut(&mut designer);
        for i in 0..10 {
            data.add_atom_to_diff(6, DVec3::new(i as f64, 0.0, 0.0));
        }
    }
    let before = snapshot_diff(&mut designer);
    assert_eq!(before.0, 10);

    // Simulate minimize: move all atoms slightly
    with_atom_edit_undo(&mut designer, "Minimize structure", |sd| {
        let data = get_data_mut_inner(sd);
        for i in 1..=10u32 {
            let new_pos = DVec3::new(i as f64 * 0.9, 0.1, 0.0);
            data.set_position_recorded(i, new_pos);
        }
    });

    // All atoms should have moved
    {
        let data = get_data_mut(&mut designer);
        for i in 1..=10u32 {
            let pos = data.diff.get_atom(i).unwrap().position;
            assert!((pos.y - 0.1).abs() < 1e-10);
        }
    }

    // Undo → all positions restored
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), before);

    // Redo → all moved again
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!((data.diff.get_atom(1).unwrap().position.y - 0.1).abs() < 1e-10);
    }
}

#[test]
fn undo_atom_edit_add_multiple_hydrogens() {
    // Simulate adding multiple H atoms to a single parent atom.
    let mut designer = setup_atom_edit();

    // Add parent carbon
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
    }
    let before = snapshot_diff(&mut designer);

    // Add 4 hydrogens (sp3 carbon)
    with_atom_edit_undo(&mut designer, "Add hydrogen", |sd| {
        let data = get_data_mut_inner(sd);
        let positions = [
            DVec3::new(1.09, 0.0, 0.0),
            DVec3::new(-0.36, 1.03, 0.0),
            DVec3::new(-0.36, -0.51, 0.89),
            DVec3::new(-0.36, -0.51, -0.89),
        ];
        for pos in &positions {
            let h_id = data.add_atom_recorded(1, *pos);
            data.add_bond_recorded(1, h_id, BOND_SINGLE);
        }
    });

    assert_eq!(diff_atom_count(&mut designer), 5); // C + 4H
    assert_eq!(diff_bond_count(&mut designer), 4);

    // Undo → all H atoms removed
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), before);
    assert_eq!(diff_bond_count(&mut designer), 0);

    // Redo → all H atoms restored
    assert!(designer.redo());
    assert_eq!(diff_atom_count(&mut designer), 5);
    assert_eq!(diff_bond_count(&mut designer), 4);
}

// =============================================================================
// Phase D: Sequence Tests — Complex Interleaved Operations
// =============================================================================

#[test]
fn undo_atom_edit_minimize_then_add_hydrogen_sequence() {
    // Tests that minimize and add_hydrogen commands work correctly in sequence.
    let mut designer = setup_atom_edit();

    // Add two atoms
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_to_diff(6, DVec3::ZERO);
        data.add_atom_to_diff(6, DVec3::new(1.54, 0.0, 0.0));
        data.add_bond_in_diff(1, 2, BOND_SINGLE);
    }
    let initial = snapshot_diff(&mut designer);

    // Step 1: Minimize — move both atoms
    with_atom_edit_undo(&mut designer, "Minimize structure", |sd| {
        let data = get_data_mut_inner(sd);
        data.set_position_recorded(1, DVec3::new(0.05, 0.0, 0.0));
        data.set_position_recorded(2, DVec3::new(1.49, 0.0, 0.0));
    });
    let after_minimize = snapshot_diff(&mut designer);

    // Step 2: Add hydrogen to atom 1
    with_atom_edit_undo(&mut designer, "Add hydrogen", |sd| {
        let data = get_data_mut_inner(sd);
        let h_id = data.add_atom_recorded(1, DVec3::new(-1.0, 0.0, 0.0));
        data.add_bond_recorded(1, h_id, BOND_SINGLE);
    });

    assert_eq!(diff_atom_count(&mut designer), 3);
    assert_eq!(diff_bond_count(&mut designer), 2);

    // Undo add hydrogen
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), after_minimize);
    assert_eq!(diff_bond_count(&mut designer), 1);

    // Undo minimize
    assert!(designer.undo());
    assert_eq!(snapshot_diff(&mut designer), initial);

    // Redo both
    assert!(designer.redo()); // minimize
    assert_eq!(snapshot_diff(&mut designer), after_minimize);
    assert!(designer.redo()); // add hydrogen
    assert_eq!(diff_atom_count(&mut designer), 3);
    assert_eq!(diff_bond_count(&mut designer), 2);
}
