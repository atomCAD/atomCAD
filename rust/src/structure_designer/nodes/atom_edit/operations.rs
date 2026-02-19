use super::atom_edit_data::*;
use super::types::*;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::util::transform::Transform;
use glam::f64::DVec3;

/// Delete all selected atoms and bonds.
///
/// In result view:
/// - Base atoms: adds delete markers at their positions.
/// - Diff-added atoms: removed from diff entirely.
/// - Diff-matched atoms: converted to delete markers.
/// - Selected bonds: adds bond delete markers (bond_order = 0).
///
/// In diff view (reversal semantics — "delete the edit"):
/// - Delete marker atoms: removed from diff (restores base atom).
/// - Atoms with anchors (moved/replaced base atoms): converted to delete markers.
/// - Pure addition atoms: removed from diff entirely.
/// - Bond delete markers: removed from diff (restores base bond).
/// - Normal bonds: removed from diff.
pub fn delete_selected_atoms_and_bonds(structure_designer: &mut StructureDesigner) {
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return,
    };

    if is_diff_view {
        delete_selected_in_diff_view(structure_designer);
    } else {
        delete_selected_in_result_view(structure_designer);
    }
}

/// Delete selected items in result view (provenance-based).
fn delete_selected_in_result_view(structure_designer: &mut StructureDesigner) {
    // Phase 1: Gather info about what to delete (immutable borrows)
    let (base_atoms_to_delete, diff_atoms_to_delete, bonds_to_delete) = {
        let eval_cache = match structure_designer.get_selected_node_eval_cache() {
            Some(cache) => cache,
            None => return,
        };
        let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
            Some(cache) => cache,
            None => return,
        };
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        // Base atoms: need their positions for delete markers
        let mut base_to_delete: Vec<(u32, DVec3)> = Vec::new();
        for &base_id in &atom_edit_data.selection.selected_base_atoms {
            if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                if let Some(atom) = result_structure.get_atom(result_id) {
                    base_to_delete.push((base_id, atom.position));
                }
            }
        }

        // Diff atoms: need to know if they're pure additions or matched base atoms
        let mut diff_to_delete: Vec<(u32, bool)> = Vec::new(); // (diff_id, is_pure_addition)
        for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
            let is_pure_addition = match eval_cache.provenance.diff_to_result.get(&diff_id) {
                Some(&res_id) => matches!(
                    eval_cache.provenance.sources.get(&res_id),
                    Some(AtomSource::DiffAdded(_))
                ),
                None => true, // Not in result (e.g., already a delete marker) — removable
            };
            diff_to_delete.push((diff_id, is_pure_addition));
        }

        // Bonds: need endpoint provenance and positions for identity entries
        let mut bond_deletions: Vec<BondDeletionInfo> = Vec::new();
        for bond_ref in &atom_edit_data.selection.selected_bonds {
            let source_a = eval_cache.provenance.sources.get(&bond_ref.atom_id1);
            let source_b = eval_cache.provenance.sources.get(&bond_ref.atom_id2);

            if let (Some(source_a), Some(source_b)) = (source_a, source_b) {
                let diff_id_a = get_diff_id_from_source(source_a);
                let diff_id_b = get_diff_id_from_source(source_b);

                let identity_a = if diff_id_a.is_none() {
                    result_structure
                        .get_atom(bond_ref.atom_id1)
                        .map(|a| (a.atomic_number, a.position))
                } else {
                    None
                };
                let identity_b = if diff_id_b.is_none() {
                    result_structure
                        .get_atom(bond_ref.atom_id2)
                        .map(|a| (a.atomic_number, a.position))
                } else {
                    None
                };

                bond_deletions.push(BondDeletionInfo {
                    diff_id_a,
                    diff_id_b,
                    identity_a,
                    identity_b,
                });
            }
        }

        (base_to_delete, diff_to_delete, bond_deletions)
    };

    // Phase 2: Apply deletions
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_delete_result_view(
        &base_atoms_to_delete,
        &diff_atoms_to_delete,
        &bonds_to_delete,
    );
}

/// Delete selected items in diff view (reversal semantics).
fn delete_selected_in_diff_view(structure_designer: &mut StructureDesigner) {
    // Phase 1: Gather what to delete (immutable borrows)
    let (diff_atoms_to_delete, bonds_to_delete) = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        let diff_atoms: Vec<(u32, DiffAtomKind)> = atom_edit_data
            .selection
            .selected_diff_atoms
            .iter()
            .map(|&diff_id| {
                let kind = classify_diff_atom(&atom_edit_data.diff, diff_id);
                (diff_id, kind)
            })
            .collect();

        let bonds: Vec<crate::crystolecule::atomic_structure::BondReference> = atom_edit_data
            .selection
            .selected_bonds
            .iter()
            .cloned()
            .collect();

        (diff_atoms, bonds)
    };

    // Phase 2: Apply deletions
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_delete_diff_view(&diff_atoms_to_delete, &bonds_to_delete);
}

/// Replace all selected atoms with a new element.
///
/// - Diff atoms: updates atomic_number in the diff directly.
/// - Base atoms: adds to diff with the new element at the base position.
///   Moves selection from selected_base_atoms to selected_diff_atoms.
pub fn replace_selected_atoms(structure_designer: &mut StructureDesigner, atomic_number: i16) {
    // Phase 1: Gather base atom info (immutable borrows)
    let base_atoms_to_replace = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        // In diff view, there are no base atoms in the selection — skip provenance
        if atom_edit_data.output_diff {
            Vec::new()
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };
            let result_structure =
                match structure_designer.get_atomic_structure_from_selected_node() {
                    Some(s) => s,
                    None => return,
                };

            let mut base_atoms: Vec<(u32, DVec3)> = Vec::new();
            for &base_id in &atom_edit_data.selection.selected_base_atoms {
                if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                    if let Some(atom) = result_structure.get_atom(result_id) {
                        base_atoms.push((base_id, atom.position));
                    }
                }
            }
            base_atoms
        }
    };

    // Phase 2: Apply replacements
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_replace(atomic_number, &base_atoms_to_replace);
}

/// Transform selected atoms using an absolute transform.
///
/// Computes the relative delta from the current selection transform, then:
/// - Diff atoms: updates position in the diff (anchor stays).
/// - Base atoms: adds to diff at new position with anchor at old position.
///   Moves selection from selected_base_atoms to selected_diff_atoms.
///
/// Updates selection_transform algebraically (no re-evaluation needed).
pub fn transform_selected(structure_designer: &mut StructureDesigner, abs_transform: &Transform) {
    // Phase 1: Gather info (immutable borrows)
    let (current_transform, base_atoms_info) = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        let current_transform = match atom_edit_data.selection.selection_transform.clone() {
            Some(t) => t,
            None => return,
        };

        // In diff view, there are no base atoms in the selection — skip provenance
        let base_info: Vec<(u32, i16, DVec3)> = if atom_edit_data.output_diff {
            Vec::new()
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };
            let result_structure =
                match structure_designer.get_atomic_structure_from_selected_node() {
                    Some(s) => s,
                    None => return,
                };

            // Collect base atom info for adding to diff with anchors
            let mut info: Vec<(u32, i16, DVec3)> = Vec::new();
            for &base_id in &atom_edit_data.selection.selected_base_atoms {
                if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                    if let Some(atom) = result_structure.get_atom(result_id) {
                        info.push((base_id, atom.atomic_number, atom.position));
                    }
                }
            }
            info
        };

        (current_transform, base_info)
    };

    // Compute relative transform (delta from current to desired)
    let relative = abs_transform.delta_from(&current_transform);

    // Phase 2: Apply transforms
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_transform(&relative, &base_atoms_info);
}

/// Apply a world-space displacement to all selected atom positions.
///
/// During screen-plane dragging, this is called on every mouse-move frame.
/// - Diff atoms: position updated in-place, anchor set on first move
/// - Base atoms: added to diff with anchor at original position, then moved to
///   the provenance-based diff selection so subsequent deltas update the same atom
pub(super) fn drag_selected_by_delta(structure_designer: &mut StructureDesigner, delta: DVec3) {
    // Phase 1: Gather info about base atoms that need to be added to the diff
    let base_atoms_info: Vec<(u32, i16, DVec3)> = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        // In diff view, there are no base atoms to convert
        if atom_edit_data.output_diff {
            Vec::new()
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };
            let result_structure =
                match structure_designer.get_atomic_structure_from_selected_node() {
                    Some(s) => s,
                    None => return,
                };

            let mut info: Vec<(u32, i16, DVec3)> = Vec::new();
            for &base_id in &atom_edit_data.selection.selected_base_atoms {
                if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                    if let Some(atom) = result_structure.get_atom(result_id) {
                        info.push((base_id, atom.atomic_number, atom.position));
                    }
                }
            }
            info
        }
    };

    // Phase 2: Apply delta to diff atoms & convert base atoms to diff
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    // Move existing diff atoms
    let diff_ids: Vec<u32> = atom_edit_data
        .selection
        .selected_diff_atoms
        .iter()
        .copied()
        .collect();
    for diff_id in diff_ids {
        if let Some(atom) = atom_edit_data.diff.get_atom(diff_id) {
            let new_pos = atom.position + delta;
            atom_edit_data.move_in_diff(diff_id, new_pos);
        }
    }

    // Convert base atoms to diff atoms (first move only — subsequent frames
    // will find them in selected_diff_atoms since we move them there)
    for (base_id, atomic_number, old_position) in &base_atoms_info {
        let new_position = *old_position + delta;
        let new_diff_id = atom_edit_data.diff.add_atom(*atomic_number, new_position);
        atom_edit_data
            .diff
            .set_anchor_position(new_diff_id, *old_position);
        atom_edit_data.selection.selected_base_atoms.remove(base_id);
        atom_edit_data
            .selection
            .selected_diff_atoms
            .insert(new_diff_id);
    }

    // Update selection transform to reflect the displacement
    if let Some(ref mut transform) = atom_edit_data.selection.selection_transform {
        transform.translation += delta;
    }
    atom_edit_data.selection.clear_bonds();
}
