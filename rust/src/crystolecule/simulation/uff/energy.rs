// UFF energy terms and analytical gradients.
//
// Each energy term (bond stretch, angle bend, torsion, inversion) is implemented
// as a function that computes both the energy contribution and its gradient
// with respect to atomic positions.
//
// Ported from RDKit's BondStretch.cpp, AngleBend.cpp, TorsionAngle.cpp,
// and Inversion.cpp, cross-referenced with OpenBabel's forcefielduff.cpp.

// ============================================================================
// Bond stretch
// ============================================================================
//
// E = 0.5 * kb * (r - r0)^2
//
// where r is the current distance, r0 is the equilibrium rest length, and
// kb is the force constant.
//
// Gradient (for atom i, atom j is symmetric with opposite sign):
//   dE/d(xi) = kb * (r - r0) * (xi - xj) / r
//
// Ported from RDKit's BondStretch.cpp (BSD-3-Clause).

/// Pre-computed parameters for a single bond stretch interaction.
#[derive(Debug, Clone)]
pub struct BondStretchParams {
    /// Index of the first atom.
    pub idx1: usize,
    /// Index of the second atom.
    pub idx2: usize,
    /// Equilibrium rest length in Angstroms.
    pub rest_length: f64,
    /// Force constant in kcal/(mol·Å²).
    pub force_constant: f64,
}

/// Computes bond stretch energy for a single bond.
///
/// Positions are a flat array: [x0, y0, z0, x1, y1, z1, ...].
/// Returns energy in kcal/mol.
pub fn bond_stretch_energy(params: &BondStretchParams, positions: &[f64]) -> f64 {
    let i3 = params.idx1 * 3;
    let j3 = params.idx2 * 3;

    let dx = positions[i3] - positions[j3];
    let dy = positions[i3 + 1] - positions[j3 + 1];
    let dz = positions[i3 + 2] - positions[j3 + 2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    let dist_term = dist - params.rest_length;
    0.5 * params.force_constant * dist_term * dist_term
}

/// Computes bond stretch energy and accumulates gradients for a single bond.
///
/// Positions and gradients are flat arrays: [x0, y0, z0, x1, y1, z1, ...].
/// Gradients are **accumulated** (added to existing values).
/// Returns energy in kcal/mol.
pub fn bond_stretch_energy_and_gradient(
    params: &BondStretchParams,
    positions: &[f64],
    gradients: &mut [f64],
) -> f64 {
    let i3 = params.idx1 * 3;
    let j3 = params.idx2 * 3;

    let dx = positions[i3] - positions[j3];
    let dy = positions[i3 + 1] - positions[j3 + 1];
    let dz = positions[i3 + 2] - positions[j3 + 2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    let dist_term = dist - params.rest_length;
    let energy = 0.5 * params.force_constant * dist_term * dist_term;

    // Gradient: dE/d(xi) = kb * (r - r0) * (xi - xj) / r
    if dist > 0.0 {
        let pre_factor = params.force_constant * dist_term / dist;
        gradients[i3] += pre_factor * dx;
        gradients[i3 + 1] += pre_factor * dy;
        gradients[i3 + 2] += pre_factor * dz;
        gradients[j3] -= pre_factor * dx;
        gradients[j3 + 1] -= pre_factor * dy;
        gradients[j3 + 2] -= pre_factor * dz;
    } else {
        // Degenerate case: atoms at the same position.
        // Move a small amount in an arbitrary direction (matches RDKit).
        let nudge = params.force_constant * 0.01;
        gradients[i3] += nudge;
        gradients[j3] -= nudge;
    }

    energy
}
