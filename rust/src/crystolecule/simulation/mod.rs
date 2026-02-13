pub mod force_field;
pub mod minimize;
pub mod topology;
pub mod uff;

use crate::crystolecule::atomic_structure::AtomicStructure;

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
pub fn minimize_energy(_structure: &mut AtomicStructure) -> Result<MinimizationResult, String> {
    Err("Energy minimization not yet implemented (UFF port in progress)".to_string())
}
