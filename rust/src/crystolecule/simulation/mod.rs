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

/// Maximum number of *unfrozen* atoms `minimize_energy` will accept.
/// Free atoms drive the per-iteration cost (issue #271); frozen atoms
/// are excluded from all interaction terms that don't touch a free atom.
pub const MAX_MINIMIZE_FREE_ATOMS: usize = 2000;

/// Maximum *total* atoms (frozen included). Bounds the O(N) topology
/// build, UFF typing, and the transient interaction-list memory.
pub const MAX_MINIMIZE_TOTAL_ATOMS: usize = 500_000;

/// Validates the atom counts and vdW mode before minimization starts.
///
/// Checks, in order: the hard total-atom cap, the free-atom limit, and the
/// `AllPairs` guard (the O(N²) nonbonded enumeration is not frozen-aware, so
/// it is gated on *total* atoms). Factored out of `minimize_energy` so the
/// limits can be unit-tested without allocating huge structures.
pub fn check_minimize_limits(
    num_atoms: usize,
    num_free: usize,
    vdw_mode: &VdwMode,
) -> Result<(), String> {
    if num_atoms > MAX_MINIMIZE_TOTAL_ATOMS {
        return Err(format!(
            "Structure has {} atoms, which exceeds the total minimization limit of {} \
             (frozen atoms included). Reduce the structure size.",
            num_atoms, MAX_MINIMIZE_TOTAL_ATOMS
        ));
    }
    if num_free > MAX_MINIMIZE_FREE_ATOMS {
        return Err(format!(
            "Structure has {} unfrozen atoms, which exceeds the minimization limit of {} \
             free atoms. Freeze the atoms that should not move (freeze node, optionally \
             with a region input) or reduce the structure size.",
            num_free, MAX_MINIMIZE_FREE_ATOMS
        ));
    }
    if matches!(vdw_mode, VdwMode::AllPairs) && num_atoms > MAX_MINIMIZE_FREE_ATOMS {
        return Err(format!(
            "Structure has {} atoms; minimizing more than {} atoms requires the \
             van der Waals distance cutoff. Enable 'Use vdW distance cutoff for \
             energy minimization' in Preferences (Simulation section) and try again.",
            num_atoms, MAX_MINIMIZE_FREE_ATOMS
        ));
    }
    Ok(())
}

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
/// Returns an error if the structure exceeds `MAX_MINIMIZE_FREE_ATOMS`
/// unfrozen atoms or `MAX_MINIMIZE_TOTAL_ATOMS` total atoms, to prevent the
/// UI from freezing on large inputs (issue #271), or if `AllPairs` mode is
/// requested above the free-atom limit (the O(N²) pair enumeration is not
/// frozen-aware).
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
    let num_free = structure
        .iter_atoms()
        .filter(|(_, atom)| !atom.is_frozen())
        .count();
    check_minimize_limits(num_atoms, num_free, &vdw_mode)?;

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
