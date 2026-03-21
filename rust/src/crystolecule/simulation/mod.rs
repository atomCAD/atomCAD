pub mod force_field;
pub mod minimize;
pub mod spatial_grid;
pub mod topology;
pub mod uff;

use crate::crystolecule::atomic_structure::AtomicStructure;
use glam::DVec3;
use minimize::{MinimizationConfig, minimize_with_force_field};
use topology::MolecularTopology;
use uff::UffForceField;
use uff::VdwMode;

/// Maximum number of atoms that `minimize_energy` will accept.
/// Beyond this limit, the O(N²) nonbonded pair enumeration and per-iteration
/// force evaluation become prohibitively expensive (issue #271).
pub const MAX_MINIMIZE_ATOMS: usize = 2000;

/// Result of an energy minimization run.
#[derive(Debug)]
pub struct MinimizationResult {
    /// Final energy in kcal/mol.
    pub energy: f64,
    /// Number of optimizer iterations performed.
    pub iterations: u32,
    /// Whether the optimizer converged within tolerance.
    pub converged: bool,
    /// Human-readable summary message.
    pub message: String,
}

/// Performs energy minimization on an atomic structure using the UFF force field.
///
/// Updates atom positions in-place to lower-energy configurations.
/// Atoms with the frozen flag (`atom.is_frozen()`) are held fixed.
///
/// Returns an error if the structure exceeds `MAX_MINIMIZE_ATOMS` to prevent
/// the UI from freezing on large inputs (issue #271).
///
/// # Arguments
///
/// * `structure` - A mutable reference to the atomic structure to minimize
/// * `vdw_mode` - Van der Waals computation mode (AllPairs or Cutoff)
///
/// # Returns
///
/// Returns `Ok(MinimizationResult)` with final energy, iteration count, and convergence info,
/// or `Err` with a description of what went wrong.
pub fn minimize_energy(
    structure: &mut AtomicStructure,
    vdw_mode: VdwMode,
) -> Result<MinimizationResult, String> {
    let num_atoms = structure.get_num_of_atoms();
    if num_atoms > MAX_MINIMIZE_ATOMS {
        return Err(format!(
            "Structure has {} atoms, which exceeds the minimization limit of {}. \
             Large structures cause excessive computation time.",
            num_atoms, MAX_MINIMIZE_ATOMS
        ));
    }

    let topology = match &vdw_mode {
        VdwMode::AllPairs => MolecularTopology::from_structure(structure),
        VdwMode::Cutoff(_) => MolecularTopology::from_structure_bonded_only(structure),
    };
    if topology.num_atoms == 0 {
        return Ok(MinimizationResult {
            energy: 0.0,
            iterations: 0,
            converged: true,
            message: "No atoms to minimize".to_string(),
        });
    }

    // Collect frozen indices from atom flags
    let frozen: Vec<usize> = topology
        .atom_ids
        .iter()
        .enumerate()
        .filter(|(_, atom_id)| {
            structure
                .get_atom(**atom_id)
                .is_some_and(|atom| atom.is_frozen())
        })
        .map(|(i, _)| i)
        .collect();

    let ff = if frozen.is_empty() {
        UffForceField::from_topology_with_vdw_mode(&topology, vdw_mode)?
    } else {
        UffForceField::from_topology_with_frozen(&topology, vdw_mode, &frozen)?
    };
    let mut positions = topology.positions.clone();
    let config = MinimizationConfig::default();

    let result = minimize_with_force_field(&ff, &mut positions, &config, &frozen);

    // Write optimized positions back into the AtomicStructure.
    for (i, &atom_id) in topology.atom_ids.iter().enumerate() {
        let x = positions[i * 3];
        let y = positions[i * 3 + 1];
        let z = positions[i * 3 + 2];
        structure.set_atom_position(atom_id, DVec3::new(x, y, z));
    }

    let status = if result.converged {
        "converged"
    } else {
        "did not converge"
    };
    let message = format!(
        "UFF minimization {} after {} iterations (energy: {:.4} kcal/mol)",
        status, result.iterations, result.energy
    );

    Ok(MinimizationResult {
        energy: result.energy,
        iterations: result.iterations,
        converged: result.converged,
        message,
    })
}
