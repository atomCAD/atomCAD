use super::atom_edit_data::*;
use super::types::*;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::crystolecule::simulation::force_field::RestrainedForceField;
use crate::crystolecule::simulation::minimize::MinimizationConfig;
use crate::crystolecule::simulation::minimize::{
    minimize_with_force_field, steepest_descent_steps,
};
use crate::crystolecule::simulation::topology::MolecularTopology;
use crate::crystolecule::simulation::uff::{UffForceField, VdwMode};
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec3;
use std::collections::{HashMap, HashSet};

/// Freeze mode for atom_edit energy minimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimizeFreezeMode {
    /// Only diff atoms move; base atoms are frozen at their original positions.
    FreezeBase,
    /// All atoms move freely.
    FreeAll,
    /// Only selected atoms move; everything else is frozen.
    FreeSelected,
}

/// Minimizes the atomic structure in the active atom_edit node using UFF.
///
/// Evaluates the full base+diff structure, runs L-BFGS minimization with the
/// chosen freeze strategy, and writes moved atom positions back into the diff.
///
/// Returns a human-readable result message, or an error string.
pub fn minimize_atom_edit(
    structure_designer: &mut StructureDesigner,
    freeze_mode: MinimizeFreezeMode,
) -> Result<String, String> {
    // Phase 1: Gather info (immutable borrows, all owned data returned)
    let (topology, force_field, frozen_indices, result_to_source) = {
        let atom_edit_data =
            get_active_atom_edit_data(structure_designer).ok_or("No active atom_edit node")?;

        // Check if we're in diff view — minimization always operates on the full result
        if atom_edit_data.output_diff {
            return Err("Switch to result view before minimizing".to_string());
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

        // Build topology from the evaluated result
        let vdw_mode = if structure_designer
            .preferences
            .simulation_preferences
            .use_vdw_cutoff
        {
            VdwMode::Cutoff(6.0)
        } else {
            VdwMode::AllPairs
        };
        let topology = match &vdw_mode {
            VdwMode::AllPairs => MolecularTopology::from_structure(result_structure),
            VdwMode::Cutoff(_) => MolecularTopology::from_structure_bonded_only(result_structure),
        };
        if topology.num_atoms == 0 {
            return Ok("No atoms to minimize".to_string());
        }

        // Build topology_index → AtomSource map for write-back
        let result_to_source: Vec<Option<AtomSource>> = topology
            .atom_ids
            .iter()
            .map(|&result_id| eval_cache.provenance.sources.get(&result_id).cloned())
            .collect();

        // Determine frozen set (topology indices) — computed before force field
        // so cutoff mode can skip frozen-frozen vdW pairs.
        let frozen_indices: Vec<usize> = match freeze_mode {
            MinimizeFreezeMode::FreezeBase => topology
                .atom_ids
                .iter()
                .enumerate()
                .filter(|(_, result_id)| {
                    let is_base = matches!(
                        eval_cache.provenance.sources.get(result_id),
                        Some(AtomSource::BasePassthrough(_))
                    );
                    let is_frozen = result_structure
                        .get_atom(**result_id)
                        .is_some_and(|atom| atom.is_frozen());
                    is_base || is_frozen
                })
                .map(|(i, _)| i)
                .collect(),
            MinimizeFreezeMode::FreeAll => topology
                .atom_ids
                .iter()
                .enumerate()
                .filter(|(_, result_id)| {
                    result_structure
                        .get_atom(**result_id)
                        .is_some_and(|atom| atom.is_frozen())
                })
                .map(|(i, _)| i)
                .collect(),
            MinimizeFreezeMode::FreeSelected => {
                // Build set of selected result atom IDs from selection + provenance
                let mut selected_result_ids: HashSet<u32> = HashSet::new();
                for &base_id in &atom_edit_data.selection.selected_base_atoms {
                    if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                        selected_result_ids.insert(result_id);
                    }
                }
                for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
                    if let Some(&result_id) = eval_cache.provenance.diff_to_result.get(&diff_id) {
                        selected_result_ids.insert(result_id);
                    }
                }
                if selected_result_ids.is_empty() {
                    return Err("No atoms selected — select atoms to minimize first".to_string());
                }
                // Freeze everything NOT selected
                topology
                    .atom_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, result_id)| !selected_result_ids.contains(result_id))
                    .map(|(i, _)| i)
                    .collect()
            }
        };

        let force_field =
            UffForceField::from_topology_with_frozen(&topology, vdw_mode, &frozen_indices)?;

        (topology, force_field, frozen_indices, result_to_source)
    };

    // Phase 2: Minimize (no borrows on structure_designer)
    let mut positions = topology.positions.clone();
    let config = MinimizationConfig::default();
    let start = std::time::Instant::now();
    let result = minimize_with_force_field(&force_field, &mut positions, &config, &frozen_indices);
    let elapsed_ms = start.elapsed().as_millis();

    // Phase 3: Write back moved positions into the diff (mutable borrow)
    let atom_edit_data =
        get_selected_atom_edit_data_mut(structure_designer).ok_or("No active atom_edit node")?;

    for (topo_idx, source) in result_to_source.iter().enumerate() {
        let new_pos = DVec3::new(
            positions[topo_idx * 3],
            positions[topo_idx * 3 + 1],
            positions[topo_idx * 3 + 2],
        );
        let old_pos = DVec3::new(
            topology.positions[topo_idx * 3],
            topology.positions[topo_idx * 3 + 1],
            topology.positions[topo_idx * 3 + 2],
        );

        if (new_pos - old_pos).length() < 1e-6 {
            continue;
        }

        match source {
            Some(AtomSource::DiffAdded(diff_id))
            | Some(AtomSource::DiffMatchedBase { diff_id, .. }) => {
                atom_edit_data.set_position_recorded(*diff_id, new_pos);
            }
            Some(AtomSource::BasePassthrough(_)) => {
                // FreeAll mode only — base atom moved, add to diff with anchor
                let atomic_number = topology.atomic_numbers[topo_idx];
                let new_diff_id = atom_edit_data.add_atom_recorded(atomic_number, new_pos);
                atom_edit_data.set_anchor_recorded(new_diff_id, old_pos);
            }
            None => {
                // No provenance info — skip
            }
        }
    }

    Ok(format!(
        "Minimization {} after {} iterations (energy: {:.4} kcal/mol, {}ms)",
        if result.converged {
            "converged"
        } else {
            "stopped"
        },
        result.iterations,
        result.energy,
        elapsed_ms
    ))
}

// =============================================================================
// Continuous minimization (real-time during drag)
// =============================================================================

/// Runs a few steepest descent steps during a drag frame to relax neighbors.
///
/// Reads preferences to determine the method (frozen vs. spring) and step count,
/// then delegates to `continuous_minimize_impl`.
pub fn continuous_minimize_during_drag(
    structure_designer: &mut StructureDesigner,
    promoted_base_atoms: &mut HashMap<u32, u32>,
) -> Result<(), String> {
    let prefs = &structure_designer.preferences.simulation_preferences;
    let steps = prefs.continuous_minimization_steps_per_frame;
    let use_springs = prefs.continuous_minimization_use_springs;
    continuous_minimize_impl(
        structure_designer,
        steps,
        promoted_base_atoms,
        !use_springs, // freeze_selected: true for Method 1, false for Method 2
        use_springs,  // use_springs
    )
}

/// Runs a burst of steepest descent steps after the user releases the mouse.
///
/// Unlike per-frame minimization, the settle burst does NOT freeze or
/// spring-restrain the selected atoms — the user has released the mouse,
/// so there is no cursor position to constrain to. The entire structure
/// relaxes freely (only persistent-frozen atoms remain frozen).
pub fn continuous_minimize_settle(
    structure_designer: &mut StructureDesigner,
    promoted_base_atoms: &mut HashMap<u32, u32>,
) -> Result<(), String> {
    let settle_steps = structure_designer
        .preferences
        .simulation_preferences
        .continuous_minimization_settle_steps;

    if settle_steps == 0 {
        return Ok(());
    }

    continuous_minimize_impl(
        structure_designer,
        settle_steps,
        promoted_base_atoms,
        false, // freeze_selected
        false, // use_springs
    )
}

/// Shared implementation for continuous minimization.
///
/// `freeze_selected`: if true, selected atoms are hard-frozen (Method 1 during drag).
/// `use_springs`: if true, selected atoms are spring-restrained (Method 2 during drag).
/// Both false during settle burst — selected atoms relax freely.
fn continuous_minimize_impl(
    structure_designer: &mut StructureDesigner,
    steps: u32,
    promoted_base_atoms: &mut HashMap<u32, u32>,
    freeze_selected: bool,
    use_springs: bool,
) -> Result<(), String> {
    let prefs = &structure_designer.preferences.simulation_preferences;
    let spring_k = prefs.continuous_minimization_spring_constant;

    // Phase 1: Gather (immutable borrows)
    let (topology, force_field, frozen_indices, selected_topo_indices, result_to_source) = {
        let atom_edit_data =
            get_active_atom_edit_data(structure_designer).ok_or("No active atom_edit node")?;

        if atom_edit_data.output_diff {
            return Ok(()); // No-op in diff view
        }

        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        let result_structure = structure_designer
            .get_atomic_structure_from_selected_node()
            .ok_or("No result structure")?;

        let vdw_mode = if structure_designer
            .preferences
            .simulation_preferences
            .use_vdw_cutoff
        {
            VdwMode::Cutoff(6.0)
        } else {
            VdwMode::AllPairs
        };

        let topology = match &vdw_mode {
            VdwMode::AllPairs => MolecularTopology::from_structure(result_structure),
            VdwMode::Cutoff(_) => MolecularTopology::from_structure_bonded_only(result_structure),
        };

        if topology.num_atoms == 0 {
            return Ok(());
        }

        // Build selected result IDs from selection + provenance
        let mut selected_result_ids: HashSet<u32> = HashSet::new();
        for &base_id in &atom_edit_data.selection.selected_base_atoms {
            if let Some(&rid) = eval_cache.provenance.base_to_result.get(&base_id) {
                selected_result_ids.insert(rid);
            }
        }
        for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
            if let Some(&rid) = eval_cache.provenance.diff_to_result.get(&diff_id) {
                selected_result_ids.insert(rid);
            }
        }

        // Frozen indices: always include persistent-frozen atoms.
        // If freeze_selected (Method 1): also freeze selected atoms.
        let frozen_indices: Vec<usize> = topology
            .atom_ids
            .iter()
            .enumerate()
            .filter(|(_, result_id)| {
                let is_frozen_flag = result_structure
                    .get_atom(**result_id)
                    .is_some_and(|atom| atom.is_frozen());
                let is_selected = selected_result_ids.contains(result_id);

                if freeze_selected {
                    is_selected || is_frozen_flag
                } else {
                    is_frozen_flag
                }
            })
            .map(|(i, _)| i)
            .collect();

        // For Method 2: build list of selected topology indices for spring targets
        let selected_topo_indices: Vec<usize> = if use_springs {
            topology
                .atom_ids
                .iter()
                .enumerate()
                .filter(|(_, rid)| selected_result_ids.contains(rid))
                .map(|(topo_idx, _)| topo_idx)
                .collect()
        } else {
            Vec::new()
        };

        let force_field =
            UffForceField::from_topology_with_frozen(&topology, vdw_mode, &frozen_indices)?;

        let result_to_source: Vec<Option<AtomSource>> = topology
            .atom_ids
            .iter()
            .map(|&rid| eval_cache.provenance.sources.get(&rid).cloned())
            .collect();

        (
            topology,
            force_field,
            frozen_indices,
            selected_topo_indices,
            result_to_source,
        )
    };

    // Phase 1b: Patch stale positions from the diff.
    // The topology was built from the stale result_structure. Patch all atoms
    // that have current diff positions (selected atoms + neighbors moved by
    // previous frames).
    let mut positions = topology.positions.clone();
    {
        let atom_edit_data =
            get_active_atom_edit_data(structure_designer).ok_or("No active atom_edit node")?;
        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        for (topo_idx, result_id) in topology.atom_ids.iter().enumerate() {
            if let Some(source) = eval_cache.provenance.sources.get(result_id) {
                let current_pos = match source {
                    AtomSource::DiffAdded(diff_id)
                    | AtomSource::DiffMatchedBase { diff_id, .. } => {
                        atom_edit_data.diff.get_atom(*diff_id).map(|a| a.position)
                    }
                    AtomSource::BasePassthrough(base_id) => {
                        // Check if promoted in a previous frame
                        promoted_base_atoms.get(base_id).and_then(|&diff_id| {
                            atom_edit_data.diff.get_atom(diff_id).map(|a| a.position)
                        })
                    }
                };
                if let Some(pos) = current_pos {
                    let base = topo_idx * 3;
                    positions[base] = pos.x;
                    positions[base + 1] = pos.y;
                    positions[base + 2] = pos.z;
                }
            }
        }
    }

    // Save pre-minimization positions for the movement threshold check
    let pre_minimize_positions = positions.clone();

    // Phase 2: Minimize (no borrows on structure_designer)
    // Build spring restraints from patched (current) positions
    let selected_restraints: Vec<(usize, f64, f64, f64)> = selected_topo_indices
        .iter()
        .map(|&topo_idx| {
            let base = topo_idx * 3;
            (
                topo_idx,
                positions[base],
                positions[base + 1],
                positions[base + 2],
            )
        })
        .collect();

    if use_springs && !selected_restraints.is_empty() {
        // Method 2: steepest descent with spring-restrained force field
        let restrained_ff = RestrainedForceField {
            base: &force_field,
            restraints: selected_restraints,
            spring_constant: spring_k,
        };
        steepest_descent_steps(&restrained_ff, &mut positions, &frozen_indices, steps, 0.1);
    } else {
        // Method 1 (or settle): steepest descent with selected atoms frozen
        steepest_descent_steps(&force_field, &mut positions, &frozen_indices, steps, 0.1);
    }

    // Phase 3: Write back (mutable borrow)
    let atom_edit_data =
        get_selected_atom_edit_data_mut(structure_designer).ok_or("No active atom_edit node")?;

    for (topo_idx, source) in result_to_source.iter().enumerate() {
        let new_pos = DVec3::new(
            positions[topo_idx * 3],
            positions[topo_idx * 3 + 1],
            positions[topo_idx * 3 + 2],
        );
        let old_pos = DVec3::new(
            pre_minimize_positions[topo_idx * 3],
            pre_minimize_positions[topo_idx * 3 + 1],
            pre_minimize_positions[topo_idx * 3 + 2],
        );

        if (new_pos - old_pos).length() < 1e-6 {
            continue;
        }

        match source {
            Some(AtomSource::DiffAdded(diff_id))
            | Some(AtomSource::DiffMatchedBase { diff_id, .. }) => {
                atom_edit_data.set_position_recorded(*diff_id, new_pos);
            }
            Some(AtomSource::BasePassthrough(base_id)) => {
                // Base atom moved by minimizer — promote to diff.
                if let Some(&existing_diff_id) = promoted_base_atoms.get(base_id) {
                    // Already promoted in a previous frame — just update position
                    atom_edit_data.set_position_recorded(existing_diff_id, new_pos);
                } else {
                    // First promotion — create diff entry with anchor at the
                    // original base position (from the stale result_structure).
                    let atomic_number = topology.atomic_numbers[topo_idx];
                    let original_pos = DVec3::new(
                        topology.positions[topo_idx * 3],
                        topology.positions[topo_idx * 3 + 1],
                        topology.positions[topo_idx * 3 + 2],
                    );
                    let new_diff_id = atom_edit_data.add_atom_recorded(atomic_number, new_pos);
                    atom_edit_data.set_anchor_recorded(new_diff_id, original_pos);
                    promoted_base_atoms.insert(*base_id, new_diff_id);
                }
            }
            None => {}
        }
    }

    Ok(())
}
