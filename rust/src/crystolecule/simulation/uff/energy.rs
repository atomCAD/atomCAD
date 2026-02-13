// UFF energy terms and analytical gradients.
//
// Each energy term (bond stretch, angle bend, torsion, inversion) is implemented
// as a function that computes both the energy contribution and its gradient
// with respect to atomic positions.
//
// Ported from RDKit's BondStretch.cpp, AngleBend.cpp, TorsionAngle.cpp,
// and Inversion.cpp, cross-referenced with OpenBabel's forcefielduff.cpp.

use super::params::ANGLE_CORRECTION_THRESHOLD;

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

// ============================================================================
// Angle bend
// ============================================================================
//
// Energy for coordination order 0 (general, Fourier expansion):
//   E_term = C0 + C1*cos(theta) + C2*cos(2*theta)
//
// Energy for coordination orders 1-4 (special geometries):
//   E_term = [1 - cos(n*theta)] / n^2
//   where n = order (1=linear, 2=linear-cos2, 3=trigonal, 4=square)
//
// Total energy: E = force_constant * E_term
//
// Gradient via chain rule: dE/dx = dE/dTheta * dTheta/dx
//
// Angle correction for small angles (order > 0, theta < ~30 degrees):
//   penalty = exp(-20*(theta - theta0 + 0.25))
//   Borrowed from OpenBabel to prevent atom overlap.
//
// Ported from RDKit's AngleBend.cpp (BSD-3-Clause).

/// Pre-computed parameters for a single angle bend interaction.
#[derive(Debug, Clone)]
pub struct AngleBendParams {
    /// Index of atom 1 (end atom).
    pub idx1: usize,
    /// Index of atom 2 (vertex atom).
    pub idx2: usize,
    /// Index of atom 3 (end atom).
    pub idx3: usize,
    /// Force constant in kcal/(mol*rad^2).
    pub force_constant: f64,
    /// Coordination order: 0=general, 1=linear, 2=linear-cos2, 3=trigonal, 4=square.
    pub order: u32,
    /// Equilibrium angle in radians.
    pub theta0: f64,
    /// Fourier coefficient C0 (only used for order=0).
    pub c0: f64,
    /// Fourier coefficient C1 (only used for order=0).
    pub c1: f64,
    /// Fourier coefficient C2 (only used for order=0).
    pub c2: f64,
}

impl AngleBendParams {
    /// Creates new angle bend parameters with pre-computed Fourier coefficients.
    ///
    /// For order=0, computes C0, C1, C2 from theta0 using the UFF Fourier expansion.
    /// For orders 1-4 (special geometries), C0/C1/C2 are unused (set to 0).
    pub fn new(
        idx1: usize,
        idx2: usize,
        idx3: usize,
        force_constant: f64,
        theta0: f64,
        order: u32,
    ) -> Self {
        let (c0, c1, c2) = if order == 0 {
            let sin_theta0 = theta0.sin();
            let cos_theta0 = theta0.cos();
            let c2 = 1.0 / (4.0 * (sin_theta0 * sin_theta0).max(1e-8));
            let c1 = -4.0 * c2 * cos_theta0;
            let c0 = c2 * (2.0 * cos_theta0 * cos_theta0 + 1.0);
            (c0, c1, c2)
        } else {
            (0.0, 0.0, 0.0)
        };

        Self {
            idx1,
            idx2,
            idx3,
            force_constant,
            order,
            theta0,
            c0,
            c1,
            c2,
        }
    }
}

/// Computes the angle bend energy term (without force constant multiplier).
fn angle_energy_term(params: &AngleBendParams, cos_theta: f64, sin_theta_sq: f64) -> f64 {
    // cos(2x) = cos^2(x) - sin^2(x)
    let cos2theta = cos_theta * cos_theta - sin_theta_sq;

    match params.order {
        0 => params.c0 + params.c1 * cos_theta + params.c2 * cos2theta,
        1 => {
            // E_term = 1 + cos(theta)
            1.0 + cos_theta
        }
        2 => {
            // E_term = (1 - cos(2*theta)) / 4
            (1.0 - cos2theta) / 4.0
        }
        3 => {
            // cos(3x) = cos^3(x) - 3*cos(x)*sin^2(x)
            let cos3theta = cos_theta * (cos_theta * cos_theta - 3.0 * sin_theta_sq);
            (1.0 - cos3theta) / 9.0
        }
        4 => {
            // cos(4x) = cos^4(x) - 6*cos^2(x)*sin^2(x) + sin^4(x)
            let cos4theta = cos_theta * cos_theta * cos_theta * cos_theta
                - 6.0 * cos_theta * cos_theta * sin_theta_sq
                + sin_theta_sq * sin_theta_sq;
            (1.0 - cos4theta) / 16.0
        }
        _ => 0.0,
    }
}

/// Computes dE/dTheta for the angle bend term (includes force constant).
fn angle_theta_deriv(params: &AngleBendParams, cos_theta: f64, sin_theta: f64) -> f64 {
    let sin2theta = 2.0 * sin_theta * cos_theta;

    match params.order {
        0 => {
            // dE/dTheta = -k * (C1*sin(theta) + 2*C2*sin(2*theta))
            -params.force_constant * (params.c1 * sin_theta + 2.0 * params.c2 * sin2theta)
        }
        n @ 1..=4 => {
            // E = k/n^2 * [1 - cos(n*theta)]
            // dE/dTheta = k/n * sin(n*theta)
            let sin_n_theta = match n {
                1 => {
                    // d(1+cosTheta)/dTheta = -sinTheta
                    -sin_theta
                }
                2 => {
                    // sin(2x) = 2*sin(x)*cos(x)
                    sin2theta
                }
                3 => {
                    // sin(3x) = 3*sin(x) - 4*sin^3(x)
                    sin_theta * (3.0 - 4.0 * sin_theta * sin_theta)
                }
                4 => {
                    // sin(4x) = cos(x)*(4*sin(x) - 8*sin^3(x))
                    cos_theta * sin_theta * (4.0 - 8.0 * sin_theta * sin_theta)
                }
                _ => unreachable!(),
            };
            sin_n_theta * params.force_constant / n as f64
        }
        _ => 0.0,
    }
}

/// Computes angle bend energy for a single angle.
///
/// Atom 2 (idx2) is the vertex of the angle.
/// Positions are a flat array: [x0, y0, z0, x1, y1, z1, ...].
/// Returns energy in kcal/mol.
pub fn angle_bend_energy(params: &AngleBendParams, positions: &[f64]) -> f64 {
    let i3 = params.idx1 * 3;
    let j3 = params.idx2 * 3;
    let k3 = params.idx3 * 3;

    // Vectors from vertex to end atoms
    let p12x = positions[i3] - positions[j3];
    let p12y = positions[i3 + 1] - positions[j3 + 1];
    let p12z = positions[i3 + 2] - positions[j3 + 2];
    let p32x = positions[k3] - positions[j3];
    let p32y = positions[k3 + 1] - positions[j3 + 1];
    let p32z = positions[k3 + 2] - positions[j3 + 2];

    let dist1 = (p12x * p12x + p12y * p12y + p12z * p12z).sqrt();
    let dist2 = (p32x * p32x + p32y * p32y + p32z * p32z).sqrt();

    let cos_theta = ((p12x * p32x + p12y * p32y + p12z * p32z) / (dist1 * dist2)).clamp(-1.0, 1.0);
    let sin_theta_sq = 1.0 - cos_theta * cos_theta;

    let angle_term = angle_energy_term(params, cos_theta, sin_theta_sq);
    let mut energy = params.force_constant * angle_term;

    // Angle correction for near-zero angles (from OpenBabel)
    if params.order > 0 && params.order < 5 && cos_theta > ANGLE_CORRECTION_THRESHOLD {
        let theta = cos_theta.acos();
        energy += (-20.0 * (theta - params.theta0 + 0.25)).exp();
    }

    energy
}

/// Computes angle bend energy and accumulates gradients for a single angle.
///
/// Atom 2 (idx2) is the vertex of the angle.
/// Positions and gradients are flat arrays: [x0, y0, z0, x1, y1, z1, ...].
/// Gradients are **accumulated** (added to existing values).
/// Returns energy in kcal/mol.
pub fn angle_bend_energy_and_gradient(
    params: &AngleBendParams,
    positions: &[f64],
    gradients: &mut [f64],
) -> f64 {
    let i3 = params.idx1 * 3;
    let j3 = params.idx2 * 3;
    let k3 = params.idx3 * 3;

    // Vectors from vertex to end atoms
    let p12x = positions[i3] - positions[j3];
    let p12y = positions[i3 + 1] - positions[j3 + 1];
    let p12z = positions[i3 + 2] - positions[j3 + 2];
    let p32x = positions[k3] - positions[j3];
    let p32y = positions[k3 + 1] - positions[j3 + 1];
    let p32z = positions[k3 + 2] - positions[j3 + 2];

    let dist1 = (p12x * p12x + p12y * p12y + p12z * p12z).sqrt();
    let dist2 = (p32x * p32x + p32y * p32y + p32z * p32z).sqrt();

    // Unit vectors from vertex toward end atoms
    let r1x = p12x / dist1;
    let r1y = p12y / dist1;
    let r1z = p12z / dist1;
    let r2x = p32x / dist2;
    let r2y = p32y / dist2;
    let r2z = p32z / dist2;

    let cos_theta = (r1x * r2x + r1y * r2y + r1z * r2z).clamp(-1.0, 1.0);
    let sin_theta_sq = 1.0 - cos_theta * cos_theta;
    let sin_theta = sin_theta_sq.sqrt().max(1e-8);

    // Energy
    let angle_term = angle_energy_term(params, cos_theta, sin_theta_sq);
    let mut energy = params.force_constant * angle_term;

    // dE/dTheta (includes force constant)
    let mut de_dtheta = angle_theta_deriv(params, cos_theta, sin_theta);

    // Angle correction for near-zero angles (from OpenBabel)
    if params.order > 0 && params.order < 5 && cos_theta > ANGLE_CORRECTION_THRESHOLD {
        let theta = cos_theta.acos();
        let penalty = (-20.0 * (theta - params.theta0 + 0.25)).exp();
        energy += penalty;
        de_dtheta += -20.0 * penalty;
    }

    // Cartesian gradient via chain rule: dE/dx = dE/dTheta * dTheta/dx
    // dTheta/dx = -(1/sinTheta) * dcos/dx

    // dcos/dS for atom 1 (idx1)
    let dcos_ds1x = (r2x - cos_theta * r1x) / dist1;
    let dcos_ds1y = (r2y - cos_theta * r1y) / dist1;
    let dcos_ds1z = (r2z - cos_theta * r1z) / dist1;

    // dcos/dS for atom 3 (idx3)
    let dcos_ds3x = (r1x - cos_theta * r2x) / dist2;
    let dcos_ds3y = (r1y - cos_theta * r2y) / dist2;
    let dcos_ds3z = (r1z - cos_theta * r2z) / dist2;

    let factor = de_dtheta / (-sin_theta);

    // Atom 1 gradient
    gradients[i3] += factor * dcos_ds1x;
    gradients[i3 + 1] += factor * dcos_ds1y;
    gradients[i3 + 2] += factor * dcos_ds1z;

    // Atom 2 (vertex) gradient: negative sum of atom 1 and atom 3 contributions
    gradients[j3] += factor * (-dcos_ds1x - dcos_ds3x);
    gradients[j3 + 1] += factor * (-dcos_ds1y - dcos_ds3y);
    gradients[j3 + 2] += factor * (-dcos_ds1z - dcos_ds3z);

    // Atom 3 gradient
    gradients[k3] += factor * dcos_ds3x;
    gradients[k3 + 1] += factor * dcos_ds3y;
    gradients[k3 + 2] += factor * dcos_ds3z;

    energy
}
