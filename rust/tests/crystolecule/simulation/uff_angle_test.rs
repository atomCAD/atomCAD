use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
    AngleBendParams, BondStretchParams, angle_bend_energy, angle_bend_energy_and_gradient,
    bond_stretch_energy, bond_stretch_energy_and_gradient,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::params::{
    AMIDE_BOND_ORDER, calc_angle_force_constant, calc_bond_force_constant, calc_bond_rest_length,
    get_uff_params,
};

use std::f64::consts::PI;

// ============================================================================
// Helper: assert float equality with tolerance
// ============================================================================

fn assert_approx_eq(actual: f64, expected: f64, tol: f64, msg: &str) {
    let diff = (actual - expected).abs();
    assert!(
        diff < tol,
        "{msg}: expected {expected}, got {actual} (diff={diff}, tol={tol})"
    );
}

// ============================================================================
// Test: Angle force constant (testUFF3 reference values)
// ============================================================================

#[test]
fn test_angle_force_constant_c3_c3_c3() {
    // C_3 - C_3 - C_3, all sp3 carbon
    // Reference: RDKit testUFF3
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0; // 109.47 degrees

    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);

    // RDKit testUFF3 comments out the value: 699.5918 (that's bond stretch k).
    // The angle force constant is computed but the exact reference value isn't
    // given in testUFF3 for C_3-C_3-C_3. Verify it's positive and reasonable.
    assert!(ka > 0.0, "angle force constant should be positive");
    assert!(
        ka > 50.0 && ka < 500.0,
        "angle force constant {ka} should be in reasonable range"
    );
}

#[test]
fn test_angle_force_constant_amide() {
    // C_R - N_R - C_3 (amide bond bend)
    // Reference: RDKit testUFF3: forceConstant = 211.0, tolerance 0.1
    let cr = get_uff_params("C_R").unwrap();
    let nr = get_uff_params("N_R").unwrap();
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = nr.theta0 * PI / 180.0; // 120 degrees for N_R

    let ka = calc_angle_force_constant(theta0, AMIDE_BOND_ORDER, 1.0, cr, nr, c3);

    assert_approx_eq(ka, 211.0, 0.1, "amide angle force constant");
}

#[test]
fn test_angle_force_constant_amide_bond_lengths() {
    // Verify the intermediate bond lengths match testUFF3
    let cr = get_uff_params("C_R").unwrap();
    let nr = get_uff_params("N_R").unwrap();
    let c3 = get_uff_params("C_3").unwrap();

    let r_cr_nr = calc_bond_rest_length(AMIDE_BOND_ORDER, cr, nr);
    assert_approx_eq(r_cr_nr, 1.357, 1e-3, "C_R-N_R amide bond length");

    let r_nr_c3 = calc_bond_rest_length(1.0, nr, c3);
    assert_approx_eq(r_nr_c3, 1.450, 1e-3, "N_R-C_3 bond length");
}

// ============================================================================
// Test: Angle bend energy at specific configurations
// ============================================================================

#[test]
fn test_angle_energy_at_equilibrium_order0() {
    // At equilibrium angle, energy should be 0
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);

    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    // Place 3 atoms with bond lengths ~1.514 and angle = theta0
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let positions = [
        r0,
        0.0,
        0.0, // atom 0
        0.0,
        0.0,
        0.0, // atom 1 (vertex)
        r0 * theta0.cos(),
        r0 * theta0.sin(),
        0.0, // atom 3
    ];

    let energy = angle_bend_energy(&params, &positions);
    assert_approx_eq(energy, 0.0, 1e-6, "angle energy at equilibrium");
}

#[test]
fn test_angle_energy_at_equilibrium_order2() {
    // Order 2 (linear), theta0 = PI: energy should be 0 at 180 degrees
    let c3 = get_uff_params("C_3").unwrap();
    let ka = calc_angle_force_constant(PI, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, PI, 2);

    let r0 = calc_bond_rest_length(1.0, c3, c3);
    // Linear arrangement along x-axis
    let positions = [r0, 0.0, 0.0, 0.0, 0.0, 0.0, -r0, 0.0, 0.0];

    let energy = angle_bend_energy(&params, &positions);
    assert_approx_eq(energy, 0.0, 1e-6, "linear angle energy at equilibrium");
}

#[test]
fn test_angle_energy_at_equilibrium_order3() {
    // Order 3 (trigonal), theta0 = 120 degrees
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = 120.0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 3);

    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let positions = [
        r0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        r0 * theta0.cos(),
        r0 * theta0.sin(),
        0.0,
    ];

    let energy = angle_bend_energy(&params, &positions);
    assert_approx_eq(energy, 0.0, 1e-6, "trigonal angle energy at equilibrium");
}

#[test]
fn test_angle_energy_at_equilibrium_order4() {
    // Order 4 (square planar), theta0 = 90 degrees
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = PI / 2.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 4);

    let r0 = calc_bond_rest_length(1.0, c3, c3);
    // 90 degree angle: one along x, one along y
    let positions = [r0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, r0, 0.0];

    let energy = angle_bend_energy(&params, &positions);
    assert_approx_eq(
        energy,
        0.0,
        1e-6,
        "square planar angle energy at equilibrium",
    );
}

#[test]
fn test_angle_energy_positive_when_displaced() {
    // Displaced from equilibrium: energy should be positive
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    // 90 degree angle (displaced from 109.47 equilibrium)
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let positions = [r0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, r0, 0.0];

    let energy = angle_bend_energy(&params, &positions);
    assert!(
        energy > 0.0,
        "energy should be positive when displaced from equilibrium: {energy}"
    );
}

#[test]
fn test_angle_energy_only_matches_gradient_version() {
    // angle_bend_energy() should return same value as angle_bend_energy_and_gradient()
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [0.5, -0.3, 0.8, -0.1, 0.7, -0.2, 0.9, 0.1, 0.4];
    let e1 = angle_bend_energy(&params, &positions);
    let mut gradients = [0.0; 9];
    let e2 = angle_bend_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(e1, e2, 1e-12, "energy-only vs energy-and-gradient");
}

// ============================================================================
// Test A4: Numerical vs analytical gradient for angle bend
// ============================================================================
// Central difference: dE/dx ~= (E(x+h) - E(x-h)) / (2h)
// Step size h = 1e-5, tolerance: relative error < 1%

fn numerical_gradient_angle(params: &AngleBendParams, positions: &[f64], h: f64) -> Vec<f64> {
    let n = positions.len();
    let mut grad = vec![0.0; n];

    for i in 0..n {
        let mut pos_plus = positions.to_vec();
        let mut pos_minus = positions.to_vec();
        pos_plus[i] += h;
        pos_minus[i] -= h;

        let e_plus = angle_bend_energy(params, &pos_plus);
        let e_minus = angle_bend_energy(params, &pos_minus);
        grad[i] = (e_plus - e_minus) / (2.0 * h);
    }

    grad
}

fn assert_angle_gradient_matches_numerical(
    params: &AngleBendParams,
    positions: &[f64],
    label: &str,
) {
    let h = 1e-5;
    let rel_tol = 0.01; // 1% relative error
    let abs_tol = 1e-6; // for near-zero components

    let mut analytical = vec![0.0; positions.len()];
    angle_bend_energy_and_gradient(params, positions, &mut analytical);

    let numerical = numerical_gradient_angle(params, positions, h);

    for i in 0..positions.len() {
        let a = analytical[i];
        let n = numerical[i];
        let diff = (a - n).abs();

        if a.abs() < abs_tol && n.abs() < abs_tol {
            assert!(
                diff < abs_tol,
                "{label}: gradient[{i}] analytical={a}, numerical={n}, diff={diff} (near-zero check)"
            );
        } else {
            let max_abs = a.abs().max(n.abs());
            let rel_err = diff / max_abs;
            assert!(
                rel_err < rel_tol,
                "{label}: gradient[{i}] analytical={a}, numerical={n}, rel_err={rel_err} (tol={rel_tol})"
            );
        }
    }
}

#[test]
fn test_a4_numerical_gradient_angle_order0_tetrahedral() {
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    // 3 atoms: atom 0 at (1.5, 0, 0), vertex at origin, atom 2 at (0.1, 1.5, 0)
    let positions = [1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 1.5, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order0 tetrahedral C3-C3-C3");
}

#[test]
fn test_a4_numerical_gradient_angle_order0_90deg() {
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = 90.0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [1.514, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 1.5, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order0 90deg");
}

#[test]
fn test_a4_numerical_gradient_angle_order0_120deg() {
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = 120.0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -0.3, -1.3, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order0 120deg");
}

#[test]
fn test_a4_numerical_gradient_angle_order1_linear() {
    let c3 = get_uff_params("C_3").unwrap();
    let ka = calc_angle_force_constant(PI, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, PI, 1);

    let positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -1.3, 0.1, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order1 linear");
}

#[test]
fn test_a4_numerical_gradient_angle_order2_linear() {
    // This is the order used in testUFF4 for linear molecules
    let c3 = get_uff_params("C_3").unwrap();
    let ka = calc_angle_force_constant(PI, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, PI, 2);

    let positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -1.3, 0.1, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order2 linear");
}

#[test]
fn test_a4_numerical_gradient_angle_order3_trigonal() {
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = 120.0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 3);

    let positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -0.3, -1.3, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order3 trigonal");
}

#[test]
fn test_a4_numerical_gradient_angle_order4_square() {
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = PI / 2.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 4);

    let positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -0.3, -1.3, 0.0];
    assert_angle_gradient_matches_numerical(&params, &positions, "order4 square");
}

#[test]
fn test_a4_numerical_gradient_angle_amide() {
    // C_R - N_R - C_3 with amide bond order
    let cr = get_uff_params("C_R").unwrap();
    let nr = get_uff_params("N_R").unwrap();
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = nr.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, AMIDE_BOND_ORDER, 1.0, cr, nr, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [0.8, 0.5, 0.1, -0.1, 0.0, 0.0, 0.3, -1.2, 0.2];
    assert_angle_gradient_matches_numerical(&params, &positions, "amide C_R-N_R-C_3");
}

#[test]
fn test_a4_numerical_gradient_angle_3d_offaxis() {
    // Arbitrary 3D positions (not in a plane)
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [1.3, 0.1, 0.1, -0.1, 0.05, -0.05, 0.1, 1.5, 0.05];
    assert_angle_gradient_matches_numerical(&params, &positions, "3D off-axis positions");
}

#[test]
fn test_a4_numerical_gradient_angle_near_equilibrium() {
    // Near equilibrium: very small gradients
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let r0 = calc_bond_rest_length(1.0, c3, c3);
    // Slightly perturbed from perfect equilibrium
    let positions = [
        r0,
        0.001,
        0.0,
        0.0,
        0.0,
        0.0,
        r0 * theta0.cos() + 0.001,
        r0 * theta0.sin(),
        0.0,
    ];
    assert_angle_gradient_matches_numerical(&params, &positions, "near equilibrium");
}

#[test]
fn test_a4_numerical_gradient_angle_non_origin() {
    // Atoms far from origin (translation invariance)
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [6.5, 3.0, -2.0, 5.0, 3.0, -2.0, 5.1, 4.5, -1.95];
    assert_angle_gradient_matches_numerical(&params, &positions, "non-origin");
}

#[test]
fn test_a4_numerical_gradient_angle_h_c_h() {
    // H - C_3 - H angle (methane-like)
    let c3 = get_uff_params("C_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, h, c3, h);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [1.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.1];
    assert_angle_gradient_matches_numerical(&params, &positions, "H-C3-H");
}

#[test]
fn test_a4_angle_gradient_accumulation() {
    // Verify that gradients are accumulated, not overwritten
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 1.5, 0.0];

    let mut grad1 = [0.0; 9];
    angle_bend_energy_and_gradient(&params, &positions, &mut grad1);

    let mut grad2 = grad1;
    angle_bend_energy_and_gradient(&params, &positions, &mut grad2);

    for i in 0..9 {
        assert_approx_eq(
            grad2[i],
            2.0 * grad1[i],
            1e-10,
            &format!("angle gradient accumulation [{i}]"),
        );
    }
}

#[test]
fn test_a4_angle_gradient_vertex_is_sum() {
    // The vertex atom gradient should be the negative sum of the end atom gradients
    // (force balance: sum of all gradients = 0 for internal coordinate)
    let c3 = get_uff_params("C_3").unwrap();
    let theta0 = c3.theta0 * PI / 180.0;
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);
    let params = AngleBendParams::new(0, 1, 2, ka, theta0, 0);

    let positions = [1.5, 0.2, -0.3, 0.0, 0.0, 0.0, -0.3, 1.4, 0.1];

    let mut gradients = [0.0; 9];
    angle_bend_energy_and_gradient(&params, &positions, &mut gradients);

    // Sum of all forces should be zero (translation invariance)
    for dim in 0..3 {
        let sum = gradients[dim] + gradients[3 + dim] + gradients[6 + dim];
        assert_approx_eq(sum, 0.0, 1e-10, &format!("force balance dim={dim}"));
    }
}

// ============================================================================
// Test A5: Angle minimization to target geometries
// ============================================================================
// Matches RDKit testUFF4: 3 atoms with 2 bond stretches + 1 angle bend,
// minimize and verify convergence to target bond lengths and angle.

/// Simple steepest descent minimizer for testing.
/// Uses backtracking line search (Armijo condition).
fn simple_minimize(
    bond_params: &[BondStretchParams],
    angle_params: &[AngleBendParams],
    positions: &mut [f64],
    max_steps: usize,
) {
    let n = positions.len();
    let mut new_pos = vec![0.0; n];

    for _ in 0..max_steps {
        let mut gradients = vec![0.0; n];
        let mut energy = 0.0;

        for bp in bond_params {
            energy += bond_stretch_energy_and_gradient(bp, positions, &mut gradients);
        }
        for ap in angle_params {
            energy += angle_bend_energy_and_gradient(ap, positions, &mut gradients);
        }

        // Check gradient norm for convergence
        let grad_norm_sq: f64 = gradients.iter().map(|g| g * g).sum();
        if grad_norm_sq < 1e-16 {
            break;
        }

        // Backtracking line search
        let mut alpha = 0.1;
        let c = 1e-4;

        for _ in 0..40 {
            for i in 0..n {
                new_pos[i] = positions[i] - alpha * gradients[i];
            }

            let mut new_energy = 0.0;
            for bp in bond_params {
                new_energy += bond_stretch_energy(bp, &new_pos);
            }
            for ap in angle_params {
                new_energy += angle_bend_energy(ap, &new_pos);
            }

            if new_energy <= energy - c * alpha * grad_norm_sq {
                positions.copy_from_slice(&new_pos);
                break;
            }
            alpha *= 0.5;
        }
    }
}

/// Measures the angle (in radians) between vectors (p1-p2) and (p3-p2).
fn measure_angle(positions: &[f64], idx1: usize, idx2: usize, idx3: usize) -> f64 {
    let i3 = idx1 * 3;
    let j3 = idx2 * 3;
    let k3 = idx3 * 3;

    let v1x = positions[i3] - positions[j3];
    let v1y = positions[i3 + 1] - positions[j3 + 1];
    let v1z = positions[i3 + 2] - positions[j3 + 2];
    let v2x = positions[k3] - positions[j3];
    let v2y = positions[k3 + 1] - positions[j3 + 1];
    let v2z = positions[k3 + 2] - positions[j3 + 2];

    let len1 = (v1x * v1x + v1y * v1y + v1z * v1z).sqrt();
    let len2 = (v2x * v2x + v2y * v2y + v2z * v2z).sqrt();
    let dot = v1x * v2x + v1y * v2y + v1z * v2z;

    (dot / (len1 * len2)).clamp(-1.0, 1.0).acos()
}

/// Measures the distance between two atoms.
fn measure_distance(positions: &[f64], idx1: usize, idx2: usize) -> f64 {
    let i3 = idx1 * 3;
    let j3 = idx2 * 3;

    let dx = positions[i3] - positions[j3];
    let dy = positions[i3 + 1] - positions[j3 + 1];
    let dz = positions[i3 + 2] - positions[j3 + 2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Sets up bond and angle params for the testUFF4 pattern:
/// 3 atoms with sp3 carbon params, 2 bond stretches (0-1, 1-2), 1 angle bend.
fn setup_testuff4_params(
    theta0: f64,
    order: u32,
) -> (Vec<BondStretchParams>, Vec<AngleBendParams>) {
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    // Use theta0 as passed in (already in radians) for force constant calc
    let ka = calc_angle_force_constant(theta0, 1.0, 1.0, c3, c3, c3);

    let bond_params = vec![
        BondStretchParams {
            idx1: 0,
            idx2: 1,
            rest_length: r0,
            force_constant: kb,
        },
        BondStretchParams {
            idx1: 1,
            idx2: 2,
            rest_length: r0,
            force_constant: kb,
        },
    ];

    let angle_params = vec![AngleBendParams::new(0, 1, 2, ka, theta0, order)];

    (bond_params, angle_params)
}

#[test]
fn test_a5_angle_minimize_90deg_order0() {
    // testUFF4 case 1: theta0 = 90 degrees, order 0
    let theta0 = 90.0 * PI / 180.0;
    let (bond_params, angle_params) = setup_testuff4_params(theta0, 0);

    // Initial positions from testUFF4
    let mut positions = [1.514, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 1.5, 0.0];

    simple_minimize(&bond_params, &angle_params, &mut positions, 2000);

    let d01 = measure_distance(&positions, 0, 1);
    let d12 = measure_distance(&positions, 1, 2);
    let theta = measure_angle(&positions, 0, 1, 2);

    assert_approx_eq(d01, 1.514, 1e-3, "bond 0-1 length");
    assert_approx_eq(d12, 1.514, 1e-3, "bond 1-2 length");
    assert_approx_eq(theta, theta0, 1e-4, "angle 90 deg");
}

#[test]
fn test_a5_angle_minimize_90deg_order0_3d() {
    // testUFF4 case 2: same theta0=90, more complicated initial positions (3D)
    let theta0 = 90.0 * PI / 180.0;
    let (bond_params, angle_params) = setup_testuff4_params(theta0, 0);

    let mut positions = [1.3, 0.1, 0.1, -0.1, 0.05, -0.05, 0.1, 1.5, 0.05];

    simple_minimize(&bond_params, &angle_params, &mut positions, 2000);

    let d01 = measure_distance(&positions, 0, 1);
    let d12 = measure_distance(&positions, 1, 2);
    let theta = measure_angle(&positions, 0, 1, 2);

    assert_approx_eq(d01, 1.514, 1e-3, "bond 0-1 length (3D)");
    assert_approx_eq(d12, 1.514, 1e-3, "bond 1-2 length (3D)");
    assert_approx_eq(theta, theta0, 1e-4, "angle 90 deg (3D)");
}

#[test]
fn test_a5_angle_minimize_tetrahedral() {
    // testUFF4 case 3: theta0 = 109.47 degrees (tetrahedral), order 0
    let theta0 = 109.47 * PI / 180.0;
    let (bond_params, angle_params) = setup_testuff4_params(theta0, 0);

    let mut positions = [1.3, 0.1, 0.1, -0.1, 0.05, -0.05, 0.1, 1.5, 0.05];

    simple_minimize(&bond_params, &angle_params, &mut positions, 2000);

    let d01 = measure_distance(&positions, 0, 1);
    let d12 = measure_distance(&positions, 1, 2);
    let theta = measure_angle(&positions, 0, 1, 2);

    assert_approx_eq(d01, 1.514, 1e-3, "bond 0-1 tetrahedral");
    assert_approx_eq(d12, 1.514, 1e-3, "bond 1-2 tetrahedral");
    assert_approx_eq(theta, theta0, 1e-4, "angle 109.47 deg");
}

#[test]
fn test_a5_angle_minimize_linear_order2() {
    // testUFF4 case 4: theta0 = PI (linear), order 2
    let theta0 = PI;
    let (bond_params, angle_params) = setup_testuff4_params(theta0, 2);

    let mut positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -1.3, 0.1, 0.0];

    simple_minimize(&bond_params, &angle_params, &mut positions, 2000);

    let d01 = measure_distance(&positions, 0, 1);
    let d12 = measure_distance(&positions, 1, 2);
    let theta = measure_angle(&positions, 0, 1, 2);

    assert_approx_eq(d01, 1.514, 1e-3, "bond 0-1 linear");
    assert_approx_eq(d12, 1.514, 1e-3, "bond 1-2 linear");
    assert_approx_eq(theta, theta0, 1e-4, "angle 180 deg (linear)");
}

#[test]
fn test_a5_angle_minimize_trigonal_order3() {
    // testUFF4 case 5: theta0 = 120 degrees, order 3
    let theta0 = 120.0 * PI / 180.0;
    let (bond_params, angle_params) = setup_testuff4_params(theta0, 3);

    let mut positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -0.3, -1.3, 0.0];

    simple_minimize(&bond_params, &angle_params, &mut positions, 2000);

    let d01 = measure_distance(&positions, 0, 1);
    let d12 = measure_distance(&positions, 1, 2);
    let theta = measure_angle(&positions, 0, 1, 2);

    assert_approx_eq(d01, 1.514, 1e-3, "bond 0-1 trigonal");
    assert_approx_eq(d12, 1.514, 1e-3, "bond 1-2 trigonal");
    assert_approx_eq(theta, theta0, 1e-4, "angle 120 deg (trigonal)");
}

#[test]
fn test_a5_angle_minimize_square_order4() {
    // testUFF4 case 6: theta0 = PI/2, order 4
    let theta0 = PI / 2.0;
    let (bond_params, angle_params) = setup_testuff4_params(theta0, 4);

    let mut positions = [1.3, 0.1, 0.0, 0.0, 0.0, 0.0, -0.3, -1.3, 0.0];

    simple_minimize(&bond_params, &angle_params, &mut positions, 2000);

    let d01 = measure_distance(&positions, 0, 1);
    let d12 = measure_distance(&positions, 1, 2);
    let theta = measure_angle(&positions, 0, 1, 2);

    assert_approx_eq(d01, 1.514, 1e-3, "bond 0-1 square");
    assert_approx_eq(d12, 1.514, 1e-3, "bond 1-2 square");
    assert_approx_eq(theta, theta0, 1e-4, "angle 90 deg (square planar)");
}
