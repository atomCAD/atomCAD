pub mod force_field;
pub mod minimize;
pub mod topology;
pub mod uff;

use crate::crystolecule::atomic_structure::AtomicStructure;
use glam::DVec3;
use minimize::{MinimizationConfig, minimize_with_force_field};
use topology::MolecularTopology;
use uff::UffForceField;

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
/// All atoms are free to move (no frozen atoms).
///
/// # Arguments
///
/// * `structure` - A mutable reference to the atomic structure to minimize
///
/// # Returns
///
/// Returns `Ok(MinimizationResult)` with final energy, iteration count, and convergence info,
/// or `Err` with a description of what went wrong.
pub fn minimize_energy(structure: &mut AtomicStructure) -> Result<MinimizationResult, String> {
    let topology = MolecularTopology::from_structure(structure);
    if topology.num_atoms == 0 {
        return Ok(MinimizationResult {
            energy: 0.0,
            iterations: 0,
            converged: true,
            message: "No atoms to minimize".to_string(),
        });
    }

    let ff = UffForceField::from_topology(&topology)?;
    let mut positions = topology.positions.clone();
    let config = MinimizationConfig::default();
    let frozen: &[usize] = &[];

    let result = minimize_with_force_field(&ff, &mut positions, &config, frozen);

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
