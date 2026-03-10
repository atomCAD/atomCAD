use super::atom_edit_data::*;
use super::types::*;
use crate::crystolecule::atomic_structure::UNCHANGED_ATOMIC_NUMBER;
use crate::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec3;
use std::collections::HashMap;

// ============================================================================
// Hydrogen depassivation (removal)
// ============================================================================

/// Info gathered in Phase 1 for each hydrogen atom to remove.
struct HRemovalInfo {
    /// Provenance of this hydrogen atom in the result structure.
    source: AtomSource,
    /// Position of the hydrogen atom (needed for delete markers).
    position: DVec3,
}

/// Removes hydrogen atoms from the active atom_edit node.
///
/// Scans the result structure for hydrogen atoms matching the selection filter,
/// then removes them from the diff using provenance-based deletion:
/// - DiffAdded: remove from diff entirely
/// - DiffMatchedBase: convert to delete marker
/// - BasePassthrough: add a delete marker at the atom's position
///
/// Returns a human-readable result message, or an error string.
pub fn remove_hydrogen_atom_edit(
    structure_designer: &mut StructureDesigner,
    selected_only: bool,
) -> Result<String, String> {
    // Phase 1: Gather (immutable borrows, all owned data returned)
    let removals = {
        let atom_edit_data =
            get_active_atom_edit_data(structure_designer).ok_or("No active atom_edit node")?;

        if atom_edit_data.output_diff {
            return Err("Switch to result view before removing hydrogens".to_string());
        }

        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?;
        let eval_cache = eval_cache
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        let result_structure = structure_designer
            .get_atomic_structure_from_selected_node()
            .ok_or("No result structure")?;

        let mut h_atoms_to_remove: Vec<HRemovalInfo> = Vec::new();

        for &atom_id in result_structure
            .atom_ids()
            .copied()
            .collect::<Vec<_>>()
            .iter()
        {
            let atom = match result_structure.get_atom(atom_id) {
                Some(a) => a,
                None => continue,
            };
            if atom.atomic_number != 1 {
                continue;
            }

            if selected_only {
                let self_selected = atom.is_selected();
                let neighbor_selected =
                    atom.bonds
                        .iter()
                        .filter(|b| !b.is_delete_marker())
                        .any(|b| {
                            result_structure
                                .get_atom(b.other_atom_id())
                                .is_some_and(|n| n.is_selected())
                        });
                if !self_selected && !neighbor_selected {
                    continue;
                }
            }

            let source = match eval_cache.provenance.sources.get(&atom_id) {
                Some(s) => s.clone(),
                None => continue,
            };

            h_atoms_to_remove.push(HRemovalInfo {
                source,
                position: atom.position,
            });
        }

        h_atoms_to_remove
    };

    if removals.is_empty() {
        return Ok("No hydrogen atoms to remove".to_string());
    }

    // Phase 2: No additional computation needed

    // Phase 3: Mutate (mutable borrow on atom_edit_data)
    let atom_edit_data =
        get_selected_atom_edit_data_mut(structure_designer).ok_or("No active atom_edit node")?;

    let mut h_count = 0;
    for removal in &removals {
        match &removal.source {
            AtomSource::DiffAdded(diff_id) => {
                // Pure addition — remove from diff entirely
                atom_edit_data.remove_from_diff(*diff_id);
            }
            AtomSource::DiffMatchedBase { diff_id, .. } => {
                // Matched base atom — convert to delete marker
                atom_edit_data.convert_to_delete_marker(*diff_id);
            }
            AtomSource::BasePassthrough(_) => {
                // Base passthrough — add a delete marker at its position
                atom_edit_data.mark_for_deletion(removal.position);
            }
        }
        h_count += 1;
    }

    // Clear selection (hydrogen atoms may have been selected)
    atom_edit_data.selection.clear();

    Ok(format!("Removed {} hydrogen atoms", h_count))
}

/// Info gathered in Phase 1 for each hydrogen atom to be placed.
struct HPlacement {
    /// World position for the new hydrogen atom.
    h_position: DVec3,
    /// Provenance of the parent atom in the result structure.
    parent_source: AtomSource,
}

/// Info about a base passthrough parent atom that needs promotion to the diff.
struct BaseParentInfo {
    position: DVec3,
}

/// Adds hydrogen atoms to satisfy valence requirements in the active atom_edit node.
///
/// Evaluates the full base+diff result structure, runs `add_hydrogens()` on a clone,
/// then writes the new H atoms and bonds back into the diff. Parent atoms that exist
/// only in the base are promoted to the diff (with anchor) before bonding.
///
/// Returns a human-readable result message, or an error string.
pub fn add_hydrogen_atom_edit(
    structure_designer: &mut StructureDesigner,
    selected_only: bool,
) -> Result<String, String> {
    // Phase 1: Gather (immutable borrows, all owned data returned)
    let (placements, base_parent_info) = {
        let atom_edit_data =
            get_active_atom_edit_data(structure_designer).ok_or("No active atom_edit node")?;

        if atom_edit_data.output_diff {
            return Err("Switch to result view before adding hydrogens".to_string());
        }

        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?;
        let eval_cache = eval_cache
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        let result_structure = structure_designer
            .get_atomic_structure_from_selected_node()
            .ok_or("No result structure")?;

        // Run the core algorithm on a clone of the result structure
        let mut cloned = result_structure.clone();
        let options = AddHydrogensOptions {
            selected_only,
            skip_already_passivated: true,
        };
        let result = add_hydrogens(&mut cloned, &options);

        if result.hydrogens_added == 0 {
            return Ok("No hydrogen atoms needed".to_string());
        }

        // Collect the new H atoms and their parent bonds as owned data.
        // New atoms in `cloned` are those not present in the original result.
        let original_atom_ids: std::collections::HashSet<u32> =
            result_structure.atom_ids().copied().collect();

        let mut placement_list: Vec<HPlacement> = Vec::new();
        let mut base_parents: HashMap<u32, BaseParentInfo> = HashMap::new();

        for &h_id in cloned.atom_ids().copied().collect::<Vec<u32>>().iter() {
            if original_atom_ids.contains(&h_id) {
                continue; // existing atom, not a new H
            }
            let h_atom = match cloned.get_atom(h_id) {
                Some(a) => a,
                None => continue,
            };
            let h_position = h_atom.position;

            // Find the parent atom (the non-H atom this H is bonded to)
            let parent_result_id = match h_atom
                .bonds
                .iter()
                .find(|b| !b.is_delete_marker())
                .map(|b| b.other_atom_id())
            {
                Some(id) => id,
                None => continue, // orphan H, skip
            };

            // Look up the parent's provenance
            let parent_source = match eval_cache.provenance.sources.get(&parent_result_id) {
                Some(s) => s.clone(),
                None => continue, // no provenance info, skip
            };

            // For base passthrough parents, collect their atom info for promotion.
            // Key by base_id (not parent_result_id) because the mutation phase
            // looks up by base_id from AtomSource::BasePassthrough(base_id).
            // These IDs differ when the base structure has gaps from deleted atoms.
            if let AtomSource::BasePassthrough(base_id) = &parent_source {
                if let std::collections::hash_map::Entry::Vacant(e) = base_parents.entry(*base_id) {
                    if let Some(parent_atom) = result_structure.get_atom(parent_result_id) {
                        e.insert(BaseParentInfo {
                            position: parent_atom.position,
                        });
                    }
                }
            }

            placement_list.push(HPlacement {
                h_position,
                parent_source,
            });
        }

        (placement_list, base_parents)
    };

    if placements.is_empty() {
        return Ok("No hydrogen atoms needed".to_string());
    }

    // Phase 2: No additional computation needed

    // Phase 3: Mutate (mutable borrow on atom_edit_data)
    let atom_edit_data =
        get_selected_atom_edit_data_mut(structure_designer).ok_or("No active atom_edit node")?;

    // Track promoted base atoms: base_id -> diff_id (to avoid promoting the same atom twice)
    let mut promoted_base_atoms: HashMap<u32, u32> = HashMap::new();
    let mut h_count = 0;

    for placement in &placements {
        let parent_diff_id = match &placement.parent_source {
            AtomSource::DiffAdded(diff_id) | AtomSource::DiffMatchedBase { diff_id, .. } => {
                // Parent is already in the diff, use its diff_id directly
                *diff_id
            }
            AtomSource::BasePassthrough(base_id) => {
                // Parent is NOT in diff yet — promote it (add_atom + set_anchor_position)
                if let Some(&already_promoted_id) = promoted_base_atoms.get(base_id) {
                    already_promoted_id
                } else if let Some(parent_info) = base_parent_info.get(base_id) {
                    let new_diff_id = atom_edit_data
                        .add_atom_recorded(UNCHANGED_ATOMIC_NUMBER, parent_info.position);
                    promoted_base_atoms.insert(*base_id, new_diff_id);
                    new_diff_id
                } else {
                    continue; // no parent info, skip
                }
            }
        };

        // Add H atom to the diff
        let h_id = atom_edit_data.add_atom_recorded(1, placement.h_position);
        atom_edit_data
            .diff
            .set_atom_hydrogen_passivation(h_id, true);
        atom_edit_data.add_bond_recorded(parent_diff_id, h_id, BOND_SINGLE);
        h_count += 1;
    }

    Ok(format!("Added {} hydrogen atoms", h_count))
}
