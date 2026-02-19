use super::atom_edit_data::*;
use super::types::*;
use crate::crystolecule::atomic_structure_diff::AtomSource;
use crate::crystolecule::simulation::minimize::MinimizationConfig;
use crate::crystolecule::simulation::minimize::minimize_with_force_field;
use crate::crystolecule::simulation::topology::MolecularTopology;
use crate::crystolecule::simulation::uff::{UffForceField, VdwMode};
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec3;
use std::collections::HashSet;

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
                    matches!(
                        eval_cache.provenance.sources.get(result_id),
                        Some(AtomSource::BasePassthrough(_))
                    )
                })
                .map(|(i, _)| i)
                .collect(),
            MinimizeFreezeMode::FreeAll => Vec::new(),
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
                atom_edit_data.diff.set_atom_position(*diff_id, new_pos);
            }
            Some(AtomSource::BasePassthrough(_)) => {
                // FreeAll mode only — base atom moved, add to diff with anchor
                let atomic_number = topology.atomic_numbers[topo_idx];
                let new_diff_id = atom_edit_data.diff.add_atom(atomic_number, new_pos);
                atom_edit_data
                    .diff
                    .set_anchor_position(new_diff_id, old_pos);
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
