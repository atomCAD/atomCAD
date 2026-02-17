// L-BFGS optimizer for molecular geometry optimization.
//
// Implements the Limited-memory BFGS (L-BFGS) algorithm with:
// - Two-loop recursion for inverse Hessian approximation
// - Backtracking line search with Armijo condition
// - Frozen atom support (gradient zeroed for fixed atoms)
//
// Reference: Nocedal & Wright, "Numerical Optimization", 2nd ed., Algorithm 7.4/7.5

use crate::crystolecule::simulation::force_field::ForceField;

/// Configuration for the L-BFGS minimizer.
#[derive(Debug, Clone)]
pub struct MinimizationConfig {
    /// Maximum number of L-BFGS iterations.
    pub max_iterations: u32,
    /// Convergence tolerance on the RMS gradient (kcal/(mol*Angstrom)).
    /// Default: 1e-4, matching RDKit's default forceTol.
    pub gradient_rms_tolerance: f64,
    /// Number of (s, y) vector pairs to store for L-BFGS memory.
    /// Higher values use more memory but converge faster. Default: 8.
    pub memory_size: usize,
    /// Armijo condition parameter (c1) for backtracking line search.
    /// Sufficient decrease condition: f(x + a*d) <= f(x) + c1*a*g^T*d.
    pub line_search_c1: f64,
    /// Minimum step size before the line search gives up.
    pub line_search_min_step: f64,
    /// Maximum number of backtracking steps in the line search.
    pub line_search_max_iter: u32,
    /// Maximum displacement for any single atom per step, in Angstroms.
    /// The initial line search step is scaled down so that no atom moves
    /// more than this distance. Prevents divergence when initial forces
    /// are very large (e.g., close atom contacts from hydrogen passivation).
    /// Default: 0.3 Å.
    pub max_displacement: f64,
}

impl Default for MinimizationConfig {
    fn default() -> Self {
        Self {
            max_iterations: 500,
            gradient_rms_tolerance: 1e-4,
            memory_size: 8,
            line_search_c1: 1e-4,
            line_search_min_step: 1e-16,
            line_search_max_iter: 40,
            max_displacement: 0.3,
        }
    }
}

/// Result of the L-BFGS minimization.
#[derive(Debug)]
pub struct LbfgsResult {
    /// Final energy (kcal/mol).
    pub energy: f64,
    /// Number of iterations performed.
    pub iterations: u32,
    /// Whether the optimizer converged within the gradient tolerance.
    pub converged: bool,
}

/// Minimizes a force field's energy with respect to atomic positions using L-BFGS.
///
/// Positions are modified in place. Frozen atoms have their gradient components
/// zeroed so they do not move during optimization.
///
/// # Arguments
///
/// * `ff` - Force field that computes energy and gradients
/// * `positions` - Flat coordinate array [x0, y0, z0, x1, y1, z1, ...], modified in place
/// * `config` - Minimization parameters (iterations, tolerances, etc.)
/// * `frozen` - Topology indices of frozen atoms (their 3 coordinate components are fixed)
///
/// # Returns
///
/// `LbfgsResult` with final energy, iteration count, and convergence status.
pub fn minimize_with_force_field(
    ff: &dyn ForceField,
    positions: &mut [f64],
    config: &MinimizationConfig,
    frozen: &[usize],
) -> LbfgsResult {
    let n = positions.len();
    if n == 0 {
        return LbfgsResult {
            energy: 0.0,
            iterations: 0,
            converged: true,
        };
    }

    let m = config.memory_size;

    // L-BFGS history: circular buffer of (s, y, rho) triplets.
    let mut s_history: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut y_history: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut rho_history: Vec<f64> = Vec::with_capacity(m);

    // Evaluate energy and gradient at initial positions.
    let mut energy = 0.0;
    let mut grad = vec![0.0; n];
    ff.energy_and_gradients(positions, &mut energy, &mut grad);
    zero_frozen(&mut grad, frozen);

    // Scratch space (allocated once, reused each iteration).
    let mut pos_new = vec![0.0; n];
    let mut grad_new = vec![0.0; n];
    let mut d = vec![0.0; n];

    let num_free_coords = n - frozen.len() * 3;
    let mut iterations = 0u32;
    let mut converged = false;

    for _ in 0..config.max_iterations {
        // Convergence check: RMS gradient over free coordinates.
        let grad_rms = if num_free_coords > 0 {
            (grad.iter().map(|g| g * g).sum::<f64>() / num_free_coords as f64).sqrt()
        } else {
            0.0
        };
        if grad_rms < config.gradient_rms_tolerance {
            converged = true;
            break;
        }

        // Compute search direction via L-BFGS two-loop recursion.
        // d = -H_k * g_k  where H_k is the inverse Hessian approximation.
        d.copy_from_slice(&grad);
        let k = s_history.len();
        let mut alpha_vec = vec![0.0; k];

        // First loop: most recent to oldest.
        for i in (0..k).rev() {
            alpha_vec[i] = rho_history[i] * dot(&s_history[i], &d);
            for j in 0..n {
                d[j] -= alpha_vec[i] * y_history[i][j];
            }
        }

        // Initial Hessian scaling: H_0 = gamma * I where gamma = (s^T y) / (y^T y).
        if k > 0 {
            let sy = dot(&s_history[k - 1], &y_history[k - 1]);
            let yy = dot(&y_history[k - 1], &y_history[k - 1]);
            if yy > 0.0 {
                let gamma = sy / yy;
                for dj in d.iter_mut() {
                    *dj *= gamma;
                }
            }
        }

        // Second loop: oldest to most recent.
        for i in 0..k {
            let beta = rho_history[i] * dot(&y_history[i], &d);
            for j in 0..n {
                d[j] += (alpha_vec[i] - beta) * s_history[i][j];
            }
        }

        // Negate for descent direction.
        for dj in d.iter_mut() {
            *dj = -*dj;
        }
        zero_frozen(&mut d, frozen);

        // Verify descent direction. If not, reset L-BFGS memory and use steepest descent.
        let dg = dot(&d, &grad);
        if dg >= 0.0 {
            s_history.clear();
            y_history.clear();
            rho_history.clear();
            for j in 0..n {
                d[j] = -grad[j];
            }
            zero_frozen(&mut d, frozen);
        }

        // Backtracking line search with Armijo sufficient decrease condition.
        let dg_armijo = dot(&d, &grad);

        // Limit the initial step so no atom moves more than max_displacement.
        // This prevents divergence when forces are very large (e.g., close
        // contacts from hydrogen passivation produce gradients of ~10^6).
        let mut step = if config.max_displacement > 0.0 {
            let max_atom_disp = max_per_atom_displacement(&d);
            if max_atom_disp > config.max_displacement {
                config.max_displacement / max_atom_disp
            } else {
                1.0
            }
        } else {
            1.0
        };

        let mut e_new = 0.0;
        let mut ls_found = false;

        for _ in 0..config.line_search_max_iter {
            for j in 0..n {
                pos_new[j] = positions[j] + step * d[j];
            }
            ff.energy_and_gradients(&pos_new, &mut e_new, &mut grad_new);
            zero_frozen(&mut grad_new, frozen);

            if e_new <= energy + config.line_search_c1 * step * dg_armijo {
                ls_found = true;
                break;
            }
            step *= 0.5;
            if step < config.line_search_min_step {
                break;
            }
        }

        if !ls_found {
            // Line search failed to find a step satisfying Armijo condition.
            // Accept the last evaluated step (smallest) to make some progress.
            for j in 0..n {
                pos_new[j] = positions[j] + step * d[j];
            }
            ff.energy_and_gradients(&pos_new, &mut e_new, &mut grad_new);
            zero_frozen(&mut grad_new, frozen);
        }

        // Compute s = x_new - x_old and y = g_new - g_old for Hessian update.
        let s: Vec<f64> = (0..n).map(|j| pos_new[j] - positions[j]).collect();
        let y: Vec<f64> = (0..n).map(|j| grad_new[j] - grad[j]).collect();
        let sy = dot(&s, &y);

        // Update positions and gradient.
        positions.copy_from_slice(&pos_new);
        energy = e_new;
        grad.copy_from_slice(&grad_new);

        // Store (s, y, rho) if curvature condition s^T y > 0 holds.
        // This ensures the Hessian approximation stays positive definite.
        if sy > 1e-10 {
            if s_history.len() == m {
                s_history.remove(0);
                y_history.remove(0);
                rho_history.remove(0);
            }
            s_history.push(s);
            y_history.push(y);
            rho_history.push(1.0 / sy);
        }

        iterations += 1;
    }

    LbfgsResult {
        energy,
        iterations,
        converged,
    }
}

/// Dot product of two equal-length slices.
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(ai, bi)| ai * bi).sum()
}

/// Computes the maximum per-atom displacement magnitude from a direction vector.
///
/// The direction vector is a flat [x0,y0,z0,x1,y1,z1,...] array. Returns the
/// maximum Euclidean displacement over all atoms (i.e., max |d_i| where d_i
/// is the 3D displacement vector for atom i).
fn max_per_atom_displacement(d: &[f64]) -> f64 {
    let mut max_disp = 0.0_f64;
    for chunk in d.chunks_exact(3) {
        let disp = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        if disp > max_disp {
            max_disp = disp;
        }
    }
    max_disp
}

/// Zeros gradient components for frozen atoms.
///
/// Each frozen atom index corresponds to 3 consecutive entries in the gradient
/// array (x, y, z). Zeroing these prevents the optimizer from moving frozen atoms.
fn zero_frozen(grad: &mut [f64], frozen: &[usize]) {
    for &atom_idx in frozen {
        let base = atom_idx * 3;
        if base + 2 < grad.len() {
            grad[base] = 0.0;
            grad[base + 1] = 0.0;
            grad[base + 2] = 0.0;
        }
    }
}
