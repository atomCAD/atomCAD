/// Trait for force field implementations.
///
/// A force field computes the potential energy and its gradient (negative force)
/// for a set of atomic positions, given pre-computed interaction parameters.
pub trait ForceField {
    /// Compute the total energy and per-atom gradients for the given positions.
    ///
    /// # Arguments
    ///
    /// * `positions` - Flat array of atomic coordinates [x0, y0, z0, x1, y1, z1, ...]
    /// * `energy` - Output: total potential energy (kcal/mol)
    /// * `gradients` - Output: gradient array (same layout as positions), dE/dx_i
    fn energy_and_gradients(&self, positions: &[f64], energy: &mut f64, gradients: &mut [f64]);
}
