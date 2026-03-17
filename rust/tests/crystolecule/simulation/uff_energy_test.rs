use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
    BondStretchParams, bond_stretch_energy, bond_stretch_energy_and_gradient,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::params::{
    calc_bond_force_constant, calc_bond_rest_length, get_uff_params,
};

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
// Test A3: Bond stretch energy at specific distances
// ============================================================================
// Reference: RDKit testUFF2 (testUFFForceField.cpp)
//   Two C_3 atoms at distance 1.814 (r0 = 1.514):
//   E = 31.4816, gradient = ±209.8775

#[test]
fn test_a3_bond_stretch_energy_at_specific_distance() {
    // Set up two C_3 atoms along the x-axis, separated by 1.814 Å
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    // Verify r0 and kb match what testUFF2 uses
    assert_approx_eq(r0, 1.514, 1e-3, "C_3-C_3 r0");
    assert_approx_eq(kb, 699.5918, 0.1, "C_3-C_3 kb");

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Two atoms along x-axis at distance 1.814
    let positions = [0.0, 0.0, 0.0, 1.814, 0.0, 0.0];
    let energy = bond_stretch_energy(&params, &positions);

    // RDKit testUFF2: energy at r=1.814 should be 31.4816
    // E = 0.5 * 699.5918 * (1.814 - 1.514)^2 = 0.5 * 699.5918 * 0.09 = 31.4816
    assert_approx_eq(energy, 31.4816, 0.01, "bond stretch energy at r=1.814");
}

#[test]
fn test_a3_bond_stretch_energy_at_equilibrium() {
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Two atoms at equilibrium distance r0
    let positions = [0.0, 0.0, 0.0, r0, 0.0, 0.0];
    let energy = bond_stretch_energy(&params, &positions);

    // At equilibrium: E = 0
    assert_approx_eq(energy, 0.0, 1e-10, "bond stretch energy at equilibrium");
}

#[test]
fn test_a3_bond_stretch_gradient_at_specific_distance() {
    // RDKit testUFF2: gradient at r=1.814 should be ±209.8775
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Two atoms along x-axis at distance 1.814
    let positions = [0.0, 0.0, 0.0, 1.814, 0.0, 0.0];
    let mut gradients = [0.0; 6];
    let energy = bond_stretch_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(energy, 31.4816, 0.01, "energy with gradient");

    // Gradient on atom 0 should be -209.8775 (pointing towards negative x,
    // since the bond is stretched and the force pulls atom 0 toward atom 1)
    // dE/dx0 = kb * (r - r0) * (x0 - x1) / r = 699.5918 * 0.3 * (-1.814) / 1.814 = -209.8775
    assert_approx_eq(gradients[0], -209.8775, 0.01, "gradient atom 0 x");
    assert_approx_eq(gradients[1], 0.0, 1e-10, "gradient atom 0 y");
    assert_approx_eq(gradients[2], 0.0, 1e-10, "gradient atom 0 z");

    // Gradient on atom 1 should be +209.8775
    assert_approx_eq(gradients[3], 209.8775, 0.01, "gradient atom 1 x");
    assert_approx_eq(gradients[4], 0.0, 1e-10, "gradient atom 1 y");
    assert_approx_eq(gradients[5], 0.0, 1e-10, "gradient atom 1 z");
}

#[test]
fn test_a3_bond_stretch_gradient_at_equilibrium() {
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // At equilibrium: gradient should be zero
    let positions = [0.0, 0.0, 0.0, r0, 0.0, 0.0];
    let mut gradients = [0.0; 6];
    let energy = bond_stretch_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(energy, 0.0, 1e-10, "energy at equilibrium");
    for (i, g) in gradients.iter().enumerate() {
        assert_approx_eq(*g, 0.0, 1e-10, &format!("gradient[{i}] at equilibrium"));
    }
}

#[test]
fn test_a3_bond_stretch_compressed_bond() {
    // Test that compressed bonds also work correctly
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Compressed bond: r = 1.214 (0.3 shorter than equilibrium)
    let r = 1.214;
    let positions = [0.0, 0.0, 0.0, r, 0.0, 0.0];
    let mut gradients = [0.0; 6];
    let energy = bond_stretch_energy_and_gradient(&params, &positions, &mut gradients);

    // Energy should be same as stretched by same amount (harmonic)
    let dist_term = r - r0;
    let expected_energy = 0.5 * kb * dist_term * dist_term;
    assert_approx_eq(energy, expected_energy, 1e-6, "compressed bond energy");

    // For compressed bond, gradient on atom 0 should be positive (pushes apart)
    // dE/dx0 = kb * (r - r0) * (x0 - x1) / r = kb * (-0.3) * (-1.214) / 1.214 > 0
    assert!(
        gradients[0] > 0.0,
        "compressed bond: atom 0 gradient should push in +x direction"
    );
    assert!(
        gradients[3] < 0.0,
        "compressed bond: atom 1 gradient should push in -x direction"
    );
}

#[test]
fn test_a3_bond_stretch_diagonal_direction() {
    // Test bond not aligned with axis
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Atoms at (0,0,0) and (1,1,1) — distance = sqrt(3)
    let positions = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let mut gradients = [0.0; 6];
    let energy = bond_stretch_energy_and_gradient(&params, &positions, &mut gradients);

    let dist = 3.0_f64.sqrt();
    let expected_energy = 0.5 * kb * (dist - r0) * (dist - r0);
    assert_approx_eq(energy, expected_energy, 1e-6, "diagonal bond energy");

    // All three gradient components should be equal (by symmetry)
    assert_approx_eq(
        gradients[0],
        gradients[1],
        1e-10,
        "grad x == grad y for atom 0",
    );
    assert_approx_eq(
        gradients[1],
        gradients[2],
        1e-10,
        "grad y == grad z for atom 0",
    );

    // Atom 1 gradients should be negative of atom 0
    assert_approx_eq(
        gradients[3],
        -gradients[0],
        1e-10,
        "Newton's third law: grad x",
    );
    assert_approx_eq(
        gradients[4],
        -gradients[1],
        1e-10,
        "Newton's third law: grad y",
    );
    assert_approx_eq(
        gradients[5],
        -gradients[2],
        1e-10,
        "Newton's third law: grad z",
    );
}

#[test]
fn test_a3_energy_only_matches_energy_and_gradient() {
    // bond_stretch_energy() should return same value as bond_stretch_energy_and_gradient()
    let c3 = get_uff_params("C_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, h);
    let kb = calc_bond_force_constant(r0, c3, h);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [0.5, -0.3, 0.8, -0.1, 0.7, -0.2];
    let e1 = bond_stretch_energy(&params, &positions);
    let mut gradients = [0.0; 6];
    let e2 = bond_stretch_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(e1, e2, 1e-12, "energy-only vs energy-and-gradient");
}

// ============================================================================
// Test A4: Numerical vs analytical gradient for bond stretch
// ============================================================================
// Central difference: dE/dx ≈ (E(x+h) - E(x-h)) / (2h)
// Step size h = 1e-5, tolerance: relative error < 1%

fn numerical_gradient_bond_stretch(
    params: &BondStretchParams,
    positions: &[f64],
    h: f64,
) -> Vec<f64> {
    let n = positions.len();
    let mut grad = vec![0.0; n];

    for i in 0..n {
        let mut pos_plus = positions.to_vec();
        let mut pos_minus = positions.to_vec();
        pos_plus[i] += h;
        pos_minus[i] -= h;

        let e_plus = bond_stretch_energy(params, &pos_plus);
        let e_minus = bond_stretch_energy(params, &pos_minus);
        grad[i] = (e_plus - e_minus) / (2.0 * h);
    }

    grad
}

fn assert_gradient_matches_numerical(params: &BondStretchParams, positions: &[f64], label: &str) {
    let h = 1e-5;
    let rel_tol = 0.01; // 1% relative error
    let abs_tol = 1e-6; // for near-zero components

    let mut analytical = vec![0.0; positions.len()];
    bond_stretch_energy_and_gradient(params, positions, &mut analytical);

    let numerical = numerical_gradient_bond_stretch(params, positions, h);

    for i in 0..positions.len() {
        let a = analytical[i];
        let n = numerical[i];
        let diff = (a - n).abs();

        if a.abs() < abs_tol && n.abs() < abs_tol {
            // Both near zero — absolute check
            assert!(
                diff < abs_tol,
                "{label}: gradient[{i}] analytical={a}, numerical={n}, diff={diff} (near-zero check)"
            );
        } else {
            // Relative check
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
fn test_a4_numerical_gradient_c3_c3_stretched() {
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Stretched bond along x
    let positions = [0.0, 0.0, 0.0, 1.814, 0.0, 0.0];
    assert_gradient_matches_numerical(&params, &positions, "C3-C3 stretched x-axis");
}

#[test]
fn test_a4_numerical_gradient_c3_c3_compressed() {
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Compressed bond along x
    let positions = [0.0, 0.0, 0.0, 1.2, 0.0, 0.0];
    assert_gradient_matches_numerical(&params, &positions, "C3-C3 compressed");
}

#[test]
fn test_a4_numerical_gradient_c3_h_diagonal() {
    let c3 = get_uff_params("C_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, h);
    let kb = calc_bond_force_constant(r0, c3, h);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Arbitrary off-axis positions
    let positions = [0.3, -0.5, 0.1, 0.8, 0.4, -0.3];
    assert_gradient_matches_numerical(&params, &positions, "C3-H diagonal");
}

#[test]
fn test_a4_numerical_gradient_c2_c2_double() {
    let c2 = get_uff_params("C_2").unwrap();
    let r0 = calc_bond_rest_length(2.0, c2, c2);
    let kb = calc_bond_force_constant(r0, c2, c2);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [-0.5, 0.2, 0.7, 0.8, -0.3, 0.1];
    assert_gradient_matches_numerical(&params, &positions, "C2=C2 double bond");
}

#[test]
fn test_a4_numerical_gradient_cr_cr_aromatic() {
    let cr = get_uff_params("C_R").unwrap();
    let r0 = calc_bond_rest_length(1.5, cr, cr);
    let kb = calc_bond_force_constant(r0, cr, cr);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [0.0, 0.5, -0.3, 1.2, -0.1, 0.4];
    assert_gradient_matches_numerical(&params, &positions, "CR-CR aromatic");
}

#[test]
fn test_a4_numerical_gradient_o3_h() {
    let o3 = get_uff_params("O_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, o3, h);
    let kb = calc_bond_force_constant(r0, o3, h);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [0.1, 0.2, 0.3, 0.7, 0.9, 0.5];
    assert_gradient_matches_numerical(&params, &positions, "O3-H water bond");
}

#[test]
fn test_a4_numerical_gradient_n3_h() {
    let n3 = get_uff_params("N_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, n3, h);
    let kb = calc_bond_force_constant(r0, n3, h);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [-0.2, 0.6, 0.1, 0.5, -0.4, 0.8];
    assert_gradient_matches_numerical(&params, &positions, "N3-H ammonia bond");
}

#[test]
fn test_a4_numerical_gradient_c3_s_thiol() {
    let c3 = get_uff_params("C_3").unwrap();
    let s = get_uff_params("S_3+2").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, s);
    let kb = calc_bond_force_constant(r0, c3, s);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [0.0, 0.0, 0.0, 1.5, 0.8, 0.3];
    assert_gradient_matches_numerical(&params, &positions, "C3-S thiol bond");
}

#[test]
fn test_a4_numerical_gradient_near_equilibrium() {
    // Near equilibrium the gradients are very small — tests accuracy at small magnitudes
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    // Slightly perturbed from equilibrium
    let positions = [0.0, 0.0, 0.0, r0 + 0.001, 0.0, 0.0];
    assert_gradient_matches_numerical(&params, &positions, "C3-C3 near equilibrium");
}

#[test]
fn test_a4_numerical_gradient_non_origin() {
    // Both atoms away from origin — tests translation invariance
    let c3 = get_uff_params("C_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, h);
    let kb = calc_bond_force_constant(r0, c3, h);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [5.0, 3.0, -2.0, 5.8, 3.4, -1.7];
    assert_gradient_matches_numerical(&params, &positions, "C3-H non-origin");
}

#[test]
fn test_a4_gradient_accumulation() {
    // Verify that gradients are accumulated, not overwritten
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [0.0, 0.0, 0.0, 1.814, 0.0, 0.0];

    // First call
    let mut grad1 = [0.0; 6];
    bond_stretch_energy_and_gradient(&params, &positions, &mut grad1);

    // Second call should add to existing gradients
    let mut grad2 = grad1;
    bond_stretch_energy_and_gradient(&params, &positions, &mut grad2);

    for i in 0..6 {
        assert_approx_eq(
            grad2[i],
            2.0 * grad1[i],
            1e-10,
            &format!("gradient accumulation [{i}]"),
        );
    }
}

#[test]
fn test_a4_degenerate_zero_distance() {
    // Two atoms at the same position — should not panic
    let c3 = get_uff_params("C_3").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, c3);
    let kb = calc_bond_force_constant(r0, c3, c3);

    let params = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [1.0, 2.0, 3.0, 1.0, 2.0, 3.0];
    let mut gradients = [0.0; 6];
    let energy = bond_stretch_energy_and_gradient(&params, &positions, &mut gradients);

    // Energy should be 0.5 * kb * r0^2 (dist = 0, dist_term = -r0)
    let expected = 0.5 * kb * r0 * r0;
    assert_approx_eq(energy, expected, 1e-6, "degenerate zero-distance energy");

    // Gradient should provide a nudge, not NaN
    assert!(gradients[0].is_finite(), "gradient x should be finite");
    assert!(gradients[3].is_finite(), "gradient x should be finite");
}

// ============================================================================
// Test: Bond stretch energy for multiple bonds in a molecule
// ============================================================================
// Use methane (CH4) as a multi-bond test case with known positions

#[test]
fn test_a3_bond_stretch_multi_bond_consistency() {
    // Set up a 3-atom molecule: H-C-H (part of methane)
    // Atom 0 = C at origin, Atom 1 = H at (+1,0,0), Atom 2 = H at (0,+1,0)
    let c3 = get_uff_params("C_3").unwrap();
    let h = get_uff_params("H_").unwrap();
    let r0 = calc_bond_rest_length(1.0, c3, h);
    let kb = calc_bond_force_constant(r0, c3, h);

    let bond_ch1 = BondStretchParams {
        idx1: 0,
        idx2: 1,
        rest_length: r0,
        force_constant: kb,
    };
    let bond_ch2 = BondStretchParams {
        idx1: 0,
        idx2: 2,
        rest_length: r0,
        force_constant: kb,
    };

    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let mut gradients = [0.0; 9];

    let e1 = bond_stretch_energy_and_gradient(&bond_ch1, &positions, &mut gradients);
    let e2 = bond_stretch_energy_and_gradient(&bond_ch2, &positions, &mut gradients);

    // Both bonds have the same length (1.0 Å) so energies should be equal
    assert_approx_eq(e1, e2, 1e-10, "symmetric bond energies should match");

    // The carbon (atom 0) gradient should be sum of contributions from both bonds
    // Bond 1 pushes along -x, Bond 2 pushes along -y
    // (both are compressed from r0 ≈ 1.109 to 1.0, so they push outward from C)
    // Actually: for compressed bond, dE/dx0 = kb * (r-r0) * (x0-x1)/r
    //   Bond 1: kb * (1.0-1.109) * (0-1)/1 = kb * (-0.109) * (-1) = positive
    //   So atom 0's x-gradient from bond 1 is positive
    assert!(
        gradients[0] > 0.0,
        "carbon x-grad should be positive (bond 1 pushes in +x)"
    );
    assert!(
        gradients[1] > 0.0,
        "carbon y-grad should be positive (bond 2 pushes in +y)"
    );
    assert_approx_eq(gradients[2], 0.0, 1e-10, "carbon z-grad should be zero");
}
