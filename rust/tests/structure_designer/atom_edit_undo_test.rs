/// Tests for the atom_edit undo/redo system (Phases A-E).
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
    AtomEditData, begin_atom_edit_drag, end_atom_edit_drag, get_atom_edit_node_info_pub,
    with_atom_edit_undo,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::diff_recorder::DiffRecorder;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::undo::commands::atom_edit_frozen_change::{
    AtomEditFrozenChangeCommand, FrozenDelta, FrozenProvenance,
};
use rust_lib_flutter_cad::structure_designer::undo::commands::atom_edit_hybridization_change::{
    AtomEditHybridizationChangeCommand, HybridizationDelta, HybridizationProvenance,
};
use rust_lib_flutter_cad::structure_designer::undo::commands::atom_edit_toggle_flag::{
    AtomEditFlag, AtomEditToggleFlagCommand,
};

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
        assert!(
            (data.diff.get_atom(1).unwrap().position - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-10
        );
    }

    // Undo
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!(
            (data.diff.get_atom(1).unwrap().position - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-10
        );
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
        assert!(
            (data.diff.get_atom(1).unwrap().position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10
        );
        assert!(
            (data.diff.get_atom(2).unwrap().position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10
        );
    }

    // Undo — should restore original positions in one step
    assert!(designer.undo());
    let restored = snapshot_diff(&mut designer);
    assert_eq!(restored, before);

    // Redo — should restore final positions
    assert!(designer.redo());
    {
        let data = get_data_mut(&mut designer);
        assert!(
            (data.diff.get_atom(1).unwrap().position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10
        );
        assert!(
            (data.diff.get_atom(2).unwrap().position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10
        );
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

// =============================================================================
// Phase E: Toggle Flag Undo/Redo
// =============================================================================

/// Helper to push an AtomEditToggleFlagCommand directly on a StructureDesigner.
fn push_toggle_flag_command(
    designer: &mut StructureDesigner,
    flag: AtomEditFlag,
    description: &str,
    accessor: fn(&mut AtomEditData) -> &mut bool,
) {
    let (network_name, node_id) = get_atom_edit_node_info_pub(designer).unwrap();

    let data = get_data_mut(designer);
    let field = accessor(data);
    let old_value = *field;
    let new_value = !old_value;
    *field = new_value;
    designer.push_command(AtomEditToggleFlagCommand {
        description: description.to_string(),
        network_name,
        node_id,
        flag,
        old_value,
        new_value,
    });
}

/// Helper to toggle pin display on the active atom_edit node (uses the proper
/// `toggle_output_pin_display` path which pushes a `SetOutputPinDisplayCommand`).
fn toggle_pin_display(designer: &mut StructureDesigner, pin_index: i32) {
    let node_id = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap()
        .active_node_id
        .unwrap();
    designer.toggle_output_pin_display(node_id, pin_index);
}

/// Helper to check if the atom_edit node is in diff view (pin 1 displayed).
fn is_in_diff_view(designer: &StructureDesigner) -> bool {
    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    network
        .get_displayed_pins(node_id)
        .map_or(false, |pins| pins.contains(&1))
}

#[test]
fn undo_atom_edit_toggle_pin_display() {
    let mut designer = setup_atom_edit();
    assert!(!is_in_diff_view(&designer));

    // Toggle: show diff pin (1), hide result pin (0)
    toggle_pin_display(&mut designer, 1);
    toggle_pin_display(&mut designer, 0);
    assert!(is_in_diff_view(&designer));

    // Undo hide pin 0
    assert!(designer.undo());
    // Undo show pin 1
    assert!(designer.undo());
    assert!(!is_in_diff_view(&designer));

    // Redo both
    assert!(designer.redo());
    assert!(designer.redo());
    assert!(is_in_diff_view(&designer));
}

#[test]
fn undo_atom_edit_toggle_show_anchor_arrows() {
    let mut designer = setup_atom_edit();
    let initial = get_data_mut(&mut designer).show_anchor_arrows;

    push_toggle_flag_command(
        &mut designer,
        AtomEditFlag::ShowAnchorArrows,
        "Toggle anchor arrows",
        |d| &mut d.show_anchor_arrows,
    );
    assert_eq!(get_data_mut(&mut designer).show_anchor_arrows, !initial);

    assert!(designer.undo());
    assert_eq!(get_data_mut(&mut designer).show_anchor_arrows, initial);

    assert!(designer.redo());
    assert_eq!(get_data_mut(&mut designer).show_anchor_arrows, !initial);
}

#[test]
fn undo_atom_edit_toggle_include_base_bonds_in_diff() {
    let mut designer = setup_atom_edit();
    let initial = get_data_mut(&mut designer).include_base_bonds_in_diff;

    push_toggle_flag_command(
        &mut designer,
        AtomEditFlag::IncludeBaseBondsInDiff,
        "Toggle base bonds in diff",
        |d| &mut d.include_base_bonds_in_diff,
    );
    assert_eq!(
        get_data_mut(&mut designer).include_base_bonds_in_diff,
        !initial
    );

    assert!(designer.undo());
    assert_eq!(
        get_data_mut(&mut designer).include_base_bonds_in_diff,
        initial
    );

    assert!(designer.redo());
    assert_eq!(
        get_data_mut(&mut designer).include_base_bonds_in_diff,
        !initial
    );
}

#[test]
fn undo_atom_edit_toggle_error_on_stale_entries() {
    let mut designer = setup_atom_edit();
    assert!(!get_data_mut(&mut designer).error_on_stale_entries);

    push_toggle_flag_command(
        &mut designer,
        AtomEditFlag::ErrorOnStaleEntries,
        "Toggle error on stale entries",
        |d| &mut d.error_on_stale_entries,
    );
    assert!(get_data_mut(&mut designer).error_on_stale_entries);

    assert!(designer.undo());
    assert!(!get_data_mut(&mut designer).error_on_stale_entries);

    assert!(designer.redo());
    assert!(get_data_mut(&mut designer).error_on_stale_entries);
}

#[test]
fn undo_atom_edit_toggle_pin_display_double_toggle() {
    // Toggle to diff view, then back to result view, undo both, redo both
    let mut designer = setup_atom_edit();
    assert!(!is_in_diff_view(&designer));

    // Switch to diff view: show pin 1, hide pin 0
    toggle_pin_display(&mut designer, 1);
    toggle_pin_display(&mut designer, 0);
    assert!(is_in_diff_view(&designer));

    // Switch back to result view: show pin 0, hide pin 1
    toggle_pin_display(&mut designer, 0);
    toggle_pin_display(&mut designer, 1);
    assert!(!is_in_diff_view(&designer));

    // Undo back to diff view (undo hide pin 1, undo show pin 0)
    assert!(designer.undo());
    assert!(designer.undo());
    assert!(is_in_diff_view(&designer));

    // Undo back to result view (undo hide pin 0, undo show pin 1)
    assert!(designer.undo());
    assert!(designer.undo());
    assert!(!is_in_diff_view(&designer));

    // Redo all 4
    for _ in 0..4 {
        assert!(designer.redo());
    }
    assert!(!is_in_diff_view(&designer));
}

// =============================================================================
// Phase E: Frozen Change Undo/Redo
// =============================================================================

/// Helper to push an AtomEditFrozenChangeCommand that freezes specific atoms.
fn push_freeze_command(designer: &mut StructureDesigner, base_ids: &[u32], diff_ids: &[u32]) {
    let (network_name, node_id) = get_atom_edit_node_info_pub(designer).unwrap();
    let mut delta = FrozenDelta {
        added: Vec::new(),
        removed: Vec::new(),
    };
    let data = get_data_mut(designer);
    for &id in base_ids {
        if data.frozen_base_atoms.insert(id) {
            delta.added.push((FrozenProvenance::Base, id));
        }
    }
    for &id in diff_ids {
        if data.frozen_diff_atoms.insert(id) {
            delta.added.push((FrozenProvenance::Diff, id));
        }
    }
    if !delta.added.is_empty() || !delta.removed.is_empty() {
        designer.push_command(AtomEditFrozenChangeCommand {
            description: "Freeze selection".to_string(),
            network_name,
            node_id,
            delta,
        });
    }
}

/// Helper to push an AtomEditFrozenChangeCommand that unfreezes specific atoms.
fn push_unfreeze_command(designer: &mut StructureDesigner, base_ids: &[u32], diff_ids: &[u32]) {
    let (network_name, node_id) = get_atom_edit_node_info_pub(designer).unwrap();
    let mut delta = FrozenDelta {
        added: Vec::new(),
        removed: Vec::new(),
    };
    let data = get_data_mut(designer);
    for &id in base_ids {
        if data.frozen_base_atoms.remove(&id) {
            delta.removed.push((FrozenProvenance::Base, id));
        }
    }
    for &id in diff_ids {
        if data.frozen_diff_atoms.remove(&id) {
            delta.removed.push((FrozenProvenance::Diff, id));
        }
    }
    if !delta.added.is_empty() || !delta.removed.is_empty() {
        designer.push_command(AtomEditFrozenChangeCommand {
            description: "Unfreeze selection".to_string(),
            network_name,
            node_id,
            delta,
        });
    }
}

/// Helper to push an AtomEditFrozenChangeCommand that clears all frozen atoms.
fn push_clear_frozen_command(designer: &mut StructureDesigner) {
    let (network_name, node_id) = get_atom_edit_node_info_pub(designer).unwrap();
    let mut delta = FrozenDelta {
        added: Vec::new(),
        removed: Vec::new(),
    };
    let data = get_data_mut(designer);
    for &id in &data.frozen_base_atoms {
        delta.removed.push((FrozenProvenance::Base, id));
    }
    for &id in &data.frozen_diff_atoms {
        delta.removed.push((FrozenProvenance::Diff, id));
    }
    data.frozen_base_atoms.clear();
    data.frozen_diff_atoms.clear();
    if !delta.removed.is_empty() {
        designer.push_command(AtomEditFrozenChangeCommand {
            description: "Clear frozen atoms".to_string(),
            network_name,
            node_id,
            delta,
        });
    }
}

#[test]
fn undo_atom_edit_freeze_diff_atoms() {
    let mut designer = setup_atom_edit();

    // Add two diff atoms
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::X);
    });

    // Freeze diff atom 1
    push_freeze_command(&mut designer, &[], &[1]);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
    assert!(!get_data_mut(&mut designer).frozen_diff_atoms.contains(&2));

    // Undo freeze
    assert!(designer.undo());
    assert!(!get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));

    // Redo freeze
    assert!(designer.redo());
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
}

#[test]
fn undo_atom_edit_unfreeze_atoms() {
    let mut designer = setup_atom_edit();

    // Add a diff atom and freeze it
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    push_freeze_command(&mut designer, &[], &[1]);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));

    // Unfreeze it
    push_unfreeze_command(&mut designer, &[], &[1]);
    assert!(!get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));

    // Undo unfreeze → frozen again
    assert!(designer.undo());
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));

    // Redo unfreeze → unfrozen again
    assert!(designer.redo());
    assert!(!get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
}

#[test]
fn undo_atom_edit_clear_frozen() {
    let mut designer = setup_atom_edit();

    // Add diff atoms and freeze both
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::X);
    });
    push_freeze_command(&mut designer, &[], &[1, 2]);
    assert_eq!(get_data_mut(&mut designer).frozen_diff_atoms.len(), 2);

    // Clear all frozen
    push_clear_frozen_command(&mut designer);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.is_empty());

    // Undo clear → both restored
    assert!(designer.undo());
    assert_eq!(get_data_mut(&mut designer).frozen_diff_atoms.len(), 2);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&2));

    // Redo clear
    assert!(designer.redo());
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.is_empty());
}

#[test]
fn undo_atom_edit_freeze_already_frozen_is_noop() {
    let mut designer = setup_atom_edit();

    // Add diff atom and freeze it
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    push_freeze_command(&mut designer, &[], &[1]);

    // Try to freeze again — delta is empty so no command is pushed.
    // Undo the freeze, then verify no extra undo step was added.
    push_freeze_command(&mut designer, &[], &[1]);
    // Undo: we should undo the first freeze (not a second one)
    assert!(designer.undo());
    assert!(!get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
    // Next undo should be the add atom, not another freeze
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);
}

// =============================================================================
// Phase E: Sequence / Integration Tests
// =============================================================================

#[test]
fn undo_atom_edit_pin_display_interleaved_with_mutations() {
    // Toggle pin display, add atom, undo both in order
    let mut designer = setup_atom_edit();

    // Switch to diff view
    toggle_pin_display(&mut designer, 1);
    toggle_pin_display(&mut designer, 0);
    assert!(is_in_diff_view(&designer));

    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Undo add atom
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);
    // Pin display should still show diff view
    assert!(is_in_diff_view(&designer));

    // Undo both pin toggles
    assert!(designer.undo());
    assert!(designer.undo());
    assert!(!is_in_diff_view(&designer));

    // Redo all
    assert!(designer.redo()); // show pin 1
    assert!(designer.redo()); // hide pin 0
    assert!(is_in_diff_view(&designer));
    assert!(designer.redo()); // add atom
    assert_eq!(diff_atom_count(&mut designer), 1);
}

#[test]
fn undo_atom_edit_frozen_interleaved_with_mutations() {
    // Add atom → freeze it → delete it → undo all three
    let mut designer = setup_atom_edit();

    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    assert_eq!(diff_atom_count(&mut designer), 1);

    push_freeze_command(&mut designer, &[], &[1]);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));

    with_atom_edit_undo(&mut designer, "Delete atom", |sd| {
        let data = get_data_mut(sd);
        data.remove_from_diff(1);
    });
    assert_eq!(diff_atom_count(&mut designer), 0);

    // Undo delete → atom restored
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Undo freeze → atom still there but not frozen
    assert!(designer.undo());
    assert!(!get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
    assert_eq!(diff_atom_count(&mut designer), 1);

    // Undo add → atom gone
    assert!(designer.undo());
    assert_eq!(diff_atom_count(&mut designer), 0);
}

#[test]
fn undo_atom_edit_sequence_restores_initial_state() {
    // Comprehensive: add atom, toggle pin display, freeze, add another, toggle back, undo all
    let mut designer = setup_atom_edit();

    let initial_diff_view = is_in_diff_view(&designer);

    // 1. Add atom
    with_atom_edit_undo(&mut designer, "Add atom 1", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });

    // 2. Switch to diff view (2 undo commands: show pin 1, hide pin 0)
    toggle_pin_display(&mut designer, 1);
    toggle_pin_display(&mut designer, 0);

    // 3. Freeze atom 1
    push_freeze_command(&mut designer, &[], &[1]);

    // 4. Add another atom
    with_atom_edit_undo(&mut designer, "Add atom 2", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::X);
    });

    // 5. Switch back to result view (2 undo commands: show pin 0, hide pin 1)
    toggle_pin_display(&mut designer, 0);
    toggle_pin_display(&mut designer, 1);

    // Verify current state
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(is_in_diff_view(&designer), initial_diff_view);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));

    // Undo all 7 operations (was 5, now 7 because pin toggles are 2 commands each)
    for _ in 0..7 {
        assert!(designer.undo());
    }

    // Should be back to initial state
    assert_eq!(diff_atom_count(&mut designer), 0);
    assert_eq!(is_in_diff_view(&designer), initial_diff_view);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.is_empty());

    // Cannot undo further
    assert!(!designer.undo());

    // Redo all 7
    for _ in 0..7 {
        assert!(designer.redo());
    }

    // Back to post-all-operations state
    assert_eq!(diff_atom_count(&mut designer), 2);
    assert_eq!(is_in_diff_view(&designer), initial_diff_view);
    assert!(get_data_mut(&mut designer).frozen_diff_atoms.contains(&1));
}

// =============================================================================
// Frozen atoms and drag
// =============================================================================

use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::DragFrozenStatus;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::drag_selected_by_delta;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::transform_selected;
use rust_lib_flutter_cad::util::transform::Transform;

#[test]
fn drag_frozen_diff_atom_not_moved() {
    let mut designer = setup_atom_edit();

    // Add two diff atoms
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_recorded(6, DVec3::new(0.0, 0.0, 0.0));
        data.add_atom_recorded(7, DVec3::new(2.0, 0.0, 0.0));
        // Select both
        data.selection.selected_diff_atoms.insert(1);
        data.selection.selected_diff_atoms.insert(2);
        // Freeze atom 1
        data.frozen_diff_atoms.insert(1);
    }

    begin_atom_edit_drag(&mut designer);
    let status = drag_selected_by_delta(&mut designer, DVec3::new(1.0, 0.0, 0.0));
    end_atom_edit_drag(&mut designer);

    // Frozen atom should not have moved
    let data = get_data_mut(&mut designer);
    let atom1 = data.diff.get_atom(1).unwrap();
    assert_eq!(atom1.position, DVec3::new(0.0, 0.0, 0.0));

    // Non-frozen atom should have moved
    let atom2 = data.diff.get_atom(2).unwrap();
    assert_eq!(atom2.position, DVec3::new(3.0, 0.0, 0.0));

    assert!(matches!(status, DragFrozenStatus::SomeFrozen));
}

#[test]
fn drag_all_frozen_returns_all_frozen() {
    let mut designer = setup_atom_edit();

    // Add one diff atom, freeze it, select it
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_recorded(6, DVec3::new(0.0, 0.0, 0.0));
        data.selection.selected_diff_atoms.insert(1);
        data.frozen_diff_atoms.insert(1);
    }

    begin_atom_edit_drag(&mut designer);
    let status = drag_selected_by_delta(&mut designer, DVec3::new(1.0, 0.0, 0.0));
    end_atom_edit_drag(&mut designer);

    // Atom should not have moved
    let data = get_data_mut(&mut designer);
    let atom1 = data.diff.get_atom(1).unwrap();
    assert_eq!(atom1.position, DVec3::new(0.0, 0.0, 0.0));

    assert!(matches!(status, DragFrozenStatus::AllFrozen));
}

#[test]
fn drag_no_frozen_returns_none_frozen() {
    let mut designer = setup_atom_edit();

    // Add one diff atom, select it (not frozen)
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_recorded(6, DVec3::new(0.0, 0.0, 0.0));
        data.selection.selected_diff_atoms.insert(1);
    }

    begin_atom_edit_drag(&mut designer);
    let status = drag_selected_by_delta(&mut designer, DVec3::new(1.0, 0.0, 0.0));
    end_atom_edit_drag(&mut designer);

    // Atom should have moved
    let data = get_data_mut(&mut designer);
    let atom1 = data.diff.get_atom(1).unwrap();
    assert_eq!(atom1.position, DVec3::new(1.0, 0.0, 0.0));

    assert!(matches!(status, DragFrozenStatus::NoneFrozen));
}

// =============================================================================
// Frozen atoms: drag via transform_selected (issue #247)
// =============================================================================

/// Frozen diff atom must not be moved by transform_selected.
/// Regression test for issue #247: frozen atoms were moved by the transform panel.
#[test]
fn transform_selected_frozen_diff_atom_not_moved() {
    let mut designer = setup_atom_edit();

    // Add two diff atoms
    {
        let data = get_data_mut(&mut designer);
        data.add_atom_recorded(6, DVec3::new(0.0, 0.0, 0.0));
        data.add_atom_recorded(7, DVec3::new(2.0, 0.0, 0.0));
        // Select both
        data.selection.selected_diff_atoms.insert(1);
        data.selection.selected_diff_atoms.insert(2);
        // Set selection centroid (midpoint of the two atoms)
        data.selection.selection_transform = Some(Transform::new(
            DVec3::new(1.0, 0.0, 0.0),
            glam::f64::DQuat::IDENTITY,
        ));
        // Freeze atom 1
        data.frozen_diff_atoms.insert(1);
    }

    // Apply a transform that moves to (2, 0, 0) absolute — delta = +1 x
    let abs_transform = Transform::new(DVec3::new(2.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    with_atom_edit_undo(&mut designer, "Move atoms", |sd| {
        transform_selected(sd, &abs_transform);
    });

    let data = get_data_mut(&mut designer);
    // Frozen atom 1 must NOT have moved
    let atom1 = data.diff.get_atom(1).unwrap();
    assert_eq!(
        atom1.position,
        DVec3::new(0.0, 0.0, 0.0),
        "Frozen diff atom must not be moved by transform_selected"
    );
    // Non-frozen atom 2 must have moved
    let atom2 = data.diff.get_atom(2).unwrap();
    assert_eq!(
        atom2.position,
        DVec3::new(3.0, 0.0, 0.0),
        "Non-frozen diff atom must be moved by transform_selected"
    );
}

/// When all selected diff atoms are frozen, transform_selected must not move any.
#[test]
fn transform_selected_all_frozen_diff_atoms_not_moved() {
    let mut designer = setup_atom_edit();

    {
        let data = get_data_mut(&mut designer);
        data.add_atom_recorded(6, DVec3::new(0.0, 0.0, 0.0));
        data.add_atom_recorded(6, DVec3::new(2.0, 0.0, 0.0));
        data.selection.selected_diff_atoms.insert(1);
        data.selection.selected_diff_atoms.insert(2);
        data.selection.selection_transform = Some(Transform::new(
            DVec3::new(1.0, 0.0, 0.0),
            glam::f64::DQuat::IDENTITY,
        ));
        // Freeze both atoms
        data.frozen_diff_atoms.insert(1);
        data.frozen_diff_atoms.insert(2);
    }

    let abs_transform = Transform::new(DVec3::new(5.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    with_atom_edit_undo(&mut designer, "Move atoms", |sd| {
        transform_selected(sd, &abs_transform);
    });

    let data = get_data_mut(&mut designer);
    assert_eq!(
        data.diff.get_atom(1).unwrap().position,
        DVec3::new(0.0, 0.0, 0.0)
    );
    assert_eq!(
        data.diff.get_atom(2).unwrap().position,
        DVec3::new(2.0, 0.0, 0.0)
    );
}

/// Frozen base atoms must not contribute to the status count when dragging
/// (regression: drag_selected_by_delta correctly filters frozen base atoms).
#[test]
fn drag_frozen_base_atom_returns_all_frozen() {
    let mut designer = setup_atom_edit();

    // Manually insert a base atom ID into selected_base_atoms and freeze it.
    // (No actual wired input — gather_base_atom_promotion_info will return empty,
    // but frozen_count must still reflect the frozen atom.)
    {
        let data = get_data_mut(&mut designer);
        data.selection.selected_base_atoms.insert(42);
        data.frozen_base_atoms.insert(42);
    }

    begin_atom_edit_drag(&mut designer);
    let status = drag_selected_by_delta(&mut designer, DVec3::new(1.0, 0.0, 0.0));
    end_atom_edit_drag(&mut designer);

    // The frozen base atom was the only selected atom; nothing moved.
    assert!(
        matches!(status, DragFrozenStatus::AllFrozen),
        "Dragging only frozen base atoms must return AllFrozen"
    );
}

// =============================================================================
// merge_atomic_structure tests
// =============================================================================

#[test]
fn test_merge_atomic_structure_basic() {
    let mut designer = setup_atom_edit();

    // Build an external structure with 3 atoms and 2 bonds
    let mut ext = AtomicStructure::new();
    let a1 = ext.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Carbon
    let a2 = ext.add_atom(7, DVec3::new(1.5, 0.0, 0.0)); // Nitrogen
    let a3 = ext.add_atom(8, DVec3::new(3.0, 0.0, 0.0)); // Oxygen
    ext.add_bond_checked(a1, a2, BOND_SINGLE);
    ext.add_bond_checked(a2, a3, BOND_DOUBLE);

    let data = get_data_mut(&mut designer);
    let added_ids = data.merge_atomic_structure(&ext);

    // Should have added 3 atoms
    assert_eq!(added_ids.len(), 3);
    assert_eq!(data.diff.get_num_of_atoms(), 3);
    assert_eq!(data.diff.get_num_of_bonds(), 2);

    // Check atoms exist with correct properties
    let d1 = data.diff.get_atom(added_ids[0]).unwrap();
    assert_eq!(d1.atomic_number, 6);
    assert_eq!(d1.position, DVec3::new(0.0, 0.0, 0.0));

    let d2 = data.diff.get_atom(added_ids[1]).unwrap();
    assert_eq!(d2.atomic_number, 7);
    assert_eq!(d2.position, DVec3::new(1.5, 0.0, 0.0));

    let d3 = data.diff.get_atom(added_ids[2]).unwrap();
    assert_eq!(d3.atomic_number, 8);
    assert_eq!(d3.position, DVec3::new(3.0, 0.0, 0.0));

    // Check bonds (verify bond between first two atoms)
    let has_bond_1_2 = d1
        .bonds
        .iter()
        .any(|b| b.other_atom_id() == added_ids[1] && b.bond_order() == BOND_SINGLE);
    assert!(has_bond_1_2, "Expected single bond between atom 1 and 2");

    let has_bond_2_3 = d2
        .bonds
        .iter()
        .any(|b| b.other_atom_id() == added_ids[2] && b.bond_order() == BOND_DOUBLE);
    assert!(has_bond_2_3, "Expected double bond between atom 2 and 3");

    // No anchors — these are pure additions
    for &id in &added_ids {
        assert!(
            data.diff.anchor_position(id).is_none(),
            "Pure additions must not have anchors"
        );
    }
}

#[test]
fn test_merge_atomic_structure_incremental() {
    let mut designer = setup_atom_edit();

    // Merge first structure
    let mut ext1 = AtomicStructure::new();
    ext1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    ext1.add_atom(6, DVec3::new(1.5, 0.0, 0.0));

    let data = get_data_mut(&mut designer);
    let ids1 = data.merge_atomic_structure(&ext1);
    assert_eq!(ids1.len(), 2);

    // Merge second structure
    let mut ext2 = AtomicStructure::new();
    ext2.add_atom(7, DVec3::new(5.0, 0.0, 0.0));
    ext2.add_atom(8, DVec3::new(6.5, 0.0, 0.0));
    ext2.add_bond_checked(1, 2, BOND_SINGLE); // bond within ext2

    let ids2 = data.merge_atomic_structure(&ext2);
    assert_eq!(ids2.len(), 2);

    // All 4 atoms should exist with no ID conflicts
    assert_eq!(data.diff.get_num_of_atoms(), 4);
    assert_eq!(data.diff.get_num_of_bonds(), 1);

    // IDs should be distinct
    for id1 in &ids1 {
        for id2 in &ids2 {
            assert_ne!(id1, id2, "IDs from different merges must not collide");
        }
    }
}

#[test]
fn test_merge_atomic_structure_with_existing_edits() {
    let mut designer = setup_atom_edit();

    // Manually add an atom first
    let data = get_data_mut(&mut designer);
    let manual_id = data.add_atom_to_diff(6, DVec3::new(-5.0, 0.0, 0.0));

    // Now merge an external structure
    let mut ext = AtomicStructure::new();
    ext.add_atom(14, DVec3::new(10.0, 0.0, 0.0)); // Silicon

    let merged_ids = data.merge_atomic_structure(&ext);

    // Both should coexist
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    assert!(data.diff.get_atom(manual_id).is_some());
    assert!(data.diff.get_atom(merged_ids[0]).is_some());
    assert_ne!(manual_id, merged_ids[0]);
}

#[test]
fn test_merge_atomic_structure_undo() {
    let mut designer = setup_atom_edit();

    // Manually add one atom (outside undo wrapper, just to have pre-existing state)
    let data = get_data_mut(&mut designer);
    let _manual_id = data.add_atom_to_diff(6, DVec3::new(-5.0, 0.0, 0.0));
    designer.undo_stack.clear(); // clear any stale commands

    // Build external structure
    let mut ext = AtomicStructure::new();
    ext.add_atom(7, DVec3::new(1.0, 0.0, 0.0));
    ext.add_atom(8, DVec3::new(2.0, 0.0, 0.0));
    ext.add_bond_checked(1, 2, BOND_SINGLE);

    // Merge with undo wrapper
    with_atom_edit_undo(&mut designer, "Import XYZ", |sd| {
        let data = get_data_mut(sd);
        data.merge_atomic_structure(&ext);
    });

    // Verify merge happened
    let data = get_data_mut(&mut designer);
    assert_eq!(data.diff.get_num_of_atoms(), 3); // 1 manual + 2 merged
    assert_eq!(data.diff.get_num_of_bonds(), 1);

    // Undo
    assert!(designer.undo());

    // After undo, only the manual atom should remain
    let data = get_data_mut(&mut designer);
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    assert_eq!(data.diff.get_num_of_bonds(), 0);

    // Redo
    assert!(designer.redo());

    let data = get_data_mut(&mut designer);
    assert_eq!(data.diff.get_num_of_atoms(), 3);
    assert_eq!(data.diff.get_num_of_bonds(), 1);
}

#[test]
fn test_merge_atomic_structure_empty() {
    let mut designer = setup_atom_edit();

    let ext = AtomicStructure::new();
    let data = get_data_mut(&mut designer);
    let added_ids = data.merge_atomic_structure(&ext);

    assert!(added_ids.is_empty());
    assert_eq!(data.diff.get_num_of_atoms(), 0);
}

// =============================================================================
// Hybridization override undo tests (Phase D)
// =============================================================================

/// Helper to push an AtomEditHybridizationChangeCommand that sets hybridization
/// on specified diff atoms.
fn push_set_hybridization_command(
    designer: &mut StructureDesigner,
    diff_atom_ids: &[u32],
    value: u8,
) {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::HYBRIDIZATION_AUTO;

    let (network_name, node_id) = get_atom_edit_node_info_pub(designer).unwrap();
    let mut delta = HybridizationDelta::default();
    let data = get_data_mut(designer);
    for &diff_id in diff_atom_ids {
        let old = data
            .hybridization_override_diff_atoms
            .get(&diff_id)
            .copied();
        if value == HYBRIDIZATION_AUTO {
            if let Some(old_val) = old {
                data.hybridization_override_diff_atoms.remove(&diff_id);
                delta
                    .removed
                    .push((HybridizationProvenance::Diff, diff_id, old_val));
            }
        } else if let Some(old_val) = old {
            if old_val != value {
                data.hybridization_override_diff_atoms
                    .insert(diff_id, value);
                delta
                    .changed
                    .push((HybridizationProvenance::Diff, diff_id, old_val, value));
            }
        } else {
            data.hybridization_override_diff_atoms
                .insert(diff_id, value);
            delta
                .added
                .push((HybridizationProvenance::Diff, diff_id, value));
        }
    }
    if !delta.is_empty() {
        designer.push_command(AtomEditHybridizationChangeCommand {
            description: "Set hybridization".to_string(),
            network_name,
            node_id,
            delta,
        });
    }
}

#[test]
fn undo_atom_edit_set_hybridization_override_diff_atoms() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::HYBRIDIZATION_SP2;

    let mut designer = setup_atom_edit();

    // Add two diff atoms
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::ZERO); // Nitrogen
    });
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::X); // Nitrogen
    });

    // Set sp2 override on diff atom 1
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP2)
    );
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&2),
        None
    );

    // Undo
    assert!(designer.undo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        None
    );

    // Redo
    assert!(designer.redo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP2)
    );
}

#[test]
fn undo_atom_edit_change_hybridization_override() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::{
        HYBRIDIZATION_SP1, HYBRIDIZATION_SP2,
    };

    let mut designer = setup_atom_edit();

    // Add a diff atom
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::ZERO);
    });

    // Set sp2
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP2)
    );

    // Change to sp1
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP1);
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP1)
    );

    // Undo → back to sp2
    assert!(designer.undo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP2)
    );

    // Undo again → no override
    assert!(designer.undo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        None
    );

    // Redo → sp2
    assert!(designer.redo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP2)
    );

    // Redo → sp1
    assert!(designer.redo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP1)
    );
}

#[test]
fn undo_atom_edit_remove_hybridization_override() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::{
        HYBRIDIZATION_AUTO, HYBRIDIZATION_SP3,
    };

    let mut designer = setup_atom_edit();

    // Add a diff atom and set sp3 override
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP3);
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP3)
    );

    // Set to Auto (removes override)
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_AUTO);
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        None
    );

    // Undo → sp3 restored
    assert!(designer.undo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        Some(&HYBRIDIZATION_SP3)
    );

    // Redo → removed again
    assert!(designer.redo());
    assert_eq!(
        get_data_mut(&mut designer)
            .hybridization_override_diff_atoms
            .get(&1),
        None
    );
}

#[test]
fn undo_atom_edit_set_hybridization_already_same_is_noop() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::HYBRIDIZATION_SP2;

    let mut designer = setup_atom_edit();

    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::ZERO);
    });

    // Set sp2
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);

    // Remember the current undo description
    let desc_before = designer
        .undo_stack
        .undo_description()
        .map(|s| s.to_string());

    // Set sp2 again — should be a no-op (no command pushed)
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);

    // Undo description should be unchanged (no new command was pushed)
    assert_eq!(
        designer
            .undo_stack
            .undo_description()
            .map(|s| s.to_string()),
        desc_before
    );
}

#[test]
fn undo_atom_edit_hybridization_multiple_atoms() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::{
        HYBRIDIZATION_SP1, HYBRIDIZATION_SP2,
    };

    let mut designer = setup_atom_edit();

    // Add three diff atoms
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(6, DVec3::ZERO);
    });
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::X);
    });
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(8, DVec3::Y);
    });

    // Set sp2 on atom 1, sp1 on atom 2
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);
    push_set_hybridization_command(&mut designer, &[2], HYBRIDIZATION_SP1);

    // Set sp2 on both atoms 2 and 3 in one command
    push_set_hybridization_command(&mut designer, &[2, 3], HYBRIDIZATION_SP2);

    let data = get_data_mut(&mut designer);
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&1),
        Some(&HYBRIDIZATION_SP2)
    );
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&2),
        Some(&HYBRIDIZATION_SP2)
    );
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&3),
        Some(&HYBRIDIZATION_SP2)
    );

    // Undo the batch → atom 2 back to sp1, atom 3 removed
    assert!(designer.undo());
    let data = get_data_mut(&mut designer);
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&2),
        Some(&HYBRIDIZATION_SP1)
    );
    assert_eq!(data.hybridization_override_diff_atoms.get(&3), None);

    // Redo
    assert!(designer.redo());
    let data = get_data_mut(&mut designer);
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&2),
        Some(&HYBRIDIZATION_SP2)
    );
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&3),
        Some(&HYBRIDIZATION_SP2)
    );
}

// =============================================================================
// Hybridization override: diff view evaluation
// =============================================================================

/// Helper: evaluates the atom_edit node at a specific output pin and returns the AtomicStructure.
fn evaluate_atom_edit_pin(designer: &StructureDesigner, pin_index: i32) -> AtomicStructure {
    use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
        NetworkEvaluationContext, NetworkStackElement,
    };
    use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;

    let network_name = designer.active_node_network_name.as_ref().unwrap();
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    let mut context = NetworkEvaluationContext::new();
    let result = designer.network_evaluator.evaluate(
        &network_stack,
        node_id,
        pin_index,
        &designer.node_type_registry,
        false,
        &mut context,
    );
    match result {
        NetworkResult::Atomic(s) => s,
        _ => panic!("Expected Atomic result for pin {pin_index}"),
    }
}

/// Helper: evaluates the atom_edit node at pin 0 (result).
fn evaluate_atom_edit_output(designer: &StructureDesigner) -> AtomicStructure {
    evaluate_atom_edit_pin(designer, 0)
}

#[test]
fn hybridization_override_appears_on_diff_view_output_atoms() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::HYBRIDIZATION_SP2;

    let mut designer = setup_atom_edit();

    // Add a nitrogen atom to the diff
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::ZERO);
    });

    // Set sp2 override on diff atom 1
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);

    // Enable output_diff mode
    get_data_mut(&mut designer).output_diff = true;

    // Evaluate the node — in diff view, the output should carry the override
    let output = evaluate_atom_edit_output(&designer);
    let atom = output.get_atom(1).expect("diff atom 1 should exist in output");
    assert_eq!(
        atom.hybridization_override(),
        HYBRIDIZATION_SP2,
        "Diff view output atom should have sp2 hybridization override on Atom.flags, got Auto"
    );
}

#[test]
fn hybridization_override_appears_on_pin1_diff_output() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::HYBRIDIZATION_SP2;

    let mut designer = setup_atom_edit();

    // Add a nitrogen atom to the diff
    with_atom_edit_undo(&mut designer, "Add atom", |sd| {
        let data = get_data_mut(sd);
        data.add_atom_to_diff(7, DVec3::ZERO);
    });

    // Set sp2 override on diff atom 1
    push_set_hybridization_command(&mut designer, &[1], HYBRIDIZATION_SP2);

    // Evaluate pin 1 (diff) directly — no output_diff flag needed
    let output = evaluate_atom_edit_pin(&designer, 1);
    let atom = output.get_atom(1).expect("diff atom 1 should exist in pin 1 output");
    assert_eq!(
        atom.hybridization_override(),
        HYBRIDIZATION_SP2,
        "Pin 1 (diff) output atom should have sp2 hybridization override"
    );
}
