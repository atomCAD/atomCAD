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

/// Wraps a base ForceField and adds harmonic spring restraints pulling
/// specified atoms toward target positions.
///
/// Used for Method 2 (spring restraint) continuous minimization, where
/// dragged atoms are pulled toward the cursor position by a spring rather
/// than being hard-frozen.
///
/// Energy contribution per restraint: `E = 0.5 * k * |r - r_target|^2`
/// Gradient contribution: `dE/dr_i = k * (r_i - r_target_i)`
pub struct RestrainedForceField<'a> {
    /// The underlying force field (e.g., UFF).
    pub base: &'a dyn ForceField,
    /// Restraints: (topology_index, target_x, target_y, target_z) for each restrained atom.
    pub restraints: Vec<(usize, f64, f64, f64)>,
    /// Spring constant in kcal/(mol*Å²).
    pub spring_constant: f64,
}

impl ForceField for RestrainedForceField<'_> {
    fn energy_and_gradients(&self, positions: &[f64], energy: &mut f64, gradients: &mut [f64]) {
        // Compute base energy and gradients
        self.base
            .energy_and_gradients(positions, energy, gradients);

        // Add restraint terms: E = 0.5 * k * |r - r_target|^2
        // dE/dx_i = k * (x_i - x_target)
        let k = self.spring_constant;
        for &(topo_idx, tx, ty, tz) in &self.restraints {
            let base = topo_idx * 3;
            let dx = positions[base] - tx;
            let dy = positions[base + 1] - ty;
            let dz = positions[base + 2] - tz;

            *energy += 0.5 * k * (dx * dx + dy * dy + dz * dz);
            gradients[base] += k * dx;
            gradients[base + 1] += k * dy;
            gradients[base + 2] += k * dz;
        }
    }
}
