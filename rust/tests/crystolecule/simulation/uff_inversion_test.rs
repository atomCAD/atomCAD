use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
    InversionParams, calculate_cos_y, inversion_energy, inversion_energy_and_gradient,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::params::calc_inversion_coefficients_and_force_constant;

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
// Test: calc_inversion_coefficients_and_force_constant
// ============================================================================

#[test]
fn test_inversion_coefficients_carbon_not_bound_to_o() {
    // Carbon sp2 (not bound to O): K=6/3=2, C0=1, C1=-1, C2=0
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    assert_approx_eq(k, 2.0, 1e-10, "C sp2 K");
    assert_approx_eq(c0, 1.0, 1e-10, "C sp2 C0");
    assert_approx_eq(c1, -1.0, 1e-10, "C sp2 C1");
    assert_approx_eq(c2, 0.0, 1e-10, "C sp2 C2");
}

#[test]
fn test_inversion_coefficients_carbon_bound_to_o() {
    // Carbon sp2 bound to O (amide carbonyl): K=50/3≈16.667, C0=1, C1=-1, C2=0
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, true);
    assert_approx_eq(k, 50.0 / 3.0, 1e-10, "C=O K");
    assert_approx_eq(c0, 1.0, 1e-10, "C=O C0");
    assert_approx_eq(c1, -1.0, 1e-10, "C=O C1");
    assert_approx_eq(c2, 0.0, 1e-10, "C=O C2");
}

#[test]
fn test_inversion_coefficients_nitrogen() {
    // Nitrogen sp2: K=6/3=2, C0=1, C1=-1, C2=0
    // Reference: RDKit testUFFParamGetters gives K=2.0 for amide nitrogen
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(7, false);
    assert_approx_eq(k, 2.0, 1e-10, "N sp2 K");
    assert_approx_eq(c0, 1.0, 1e-10, "N sp2 C0");
    assert_approx_eq(c1, -1.0, 1e-10, "N sp2 C1");
    assert_approx_eq(c2, 0.0, 1e-10, "N sp2 C2");
}

#[test]
fn test_inversion_coefficients_oxygen() {
    // Oxygen sp2: K=6/3=2, C0=1, C1=-1, C2=0
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(8, false);
    assert_approx_eq(k, 2.0, 1e-10, "O sp2 K");
    assert_approx_eq(c0, 1.0, 1e-10, "O sp2 C0");
    assert_approx_eq(c1, -1.0, 1e-10, "O sp2 C1");
    assert_approx_eq(c2, 0.0, 1e-10, "O sp2 C2");
}

#[test]
fn test_inversion_coefficients_phosphorus() {
    // Phosphorus: w0=84.4339°
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(15, false);
    // Verify K > 0 and coefficients are correct
    assert!(k > 0.0, "P K should be positive");
    let w0 = 84.4339_f64.to_radians();
    let expected_c2 = 1.0;
    let expected_c1 = -4.0 * w0.cos();
    let expected_c0 = -(expected_c1 * w0.cos() + expected_c2 * (2.0 * w0).cos());
    let expected_k = 22.0 / (expected_c0 + expected_c1 + expected_c2) / 3.0;
    assert_approx_eq(k, expected_k, 1e-8, "P K");
    assert_approx_eq(c0, expected_c0, 1e-8, "P C0");
    assert_approx_eq(c1, expected_c1, 1e-8, "P C1");
    assert_approx_eq(c2, expected_c2, 1e-8, "P C2");
}

#[test]
fn test_inversion_coefficients_arsenic() {
    // Arsenic: w0=86.9735°
    let (k, _c0, _c1, _c2) = calc_inversion_coefficients_and_force_constant(33, false);
    assert!(k > 0.0, "As K should be positive");
}

#[test]
fn test_inversion_coefficients_antimony() {
    // Antimony: w0=87.7047°
    let (k, _c0, _c1, _c2) = calc_inversion_coefficients_and_force_constant(51, false);
    assert!(k > 0.0, "Sb K should be positive");
}

#[test]
fn test_inversion_coefficients_bismuth() {
    // Bismuth: w0=90°
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(83, false);
    assert!(k > 0.0, "Bi K should be positive");
    // At w0=90°: cos(90°)=0, cos(180°)=-1
    // C2=1, C1=-4*0=0, C0=-(0+1*(-1))=1
    assert_approx_eq(c0, 1.0, 1e-10, "Bi C0");
    assert_approx_eq(c1, 0.0, 1e-10, "Bi C1");
    assert_approx_eq(c2, 1.0, 1e-10, "Bi C2");
    // K = 22/(1+0+1)/3 = 22/2/3 ≈ 3.667
    assert_approx_eq(k, 22.0 / 2.0 / 3.0, 1e-10, "Bi K value");
}

// ============================================================================
// Test: calculate_cos_y
// ============================================================================

#[test]
fn test_cos_y_planar() {
    // Central atom at origin, three neighbors in the XY plane, fourth also in plane.
    // cosY should be 0 (L is in the plane, perpendicular to normal).
    let p1 = [1.0, 0.0, 0.0]; // I
    let p2 = [0.0, 0.0, 0.0]; // J (center)
    let p3 = [0.0, 1.0, 0.0]; // K
    let p4 = [-1.0, 0.0, 0.0]; // L (in the XY plane)
    let cos_y = calculate_cos_y(p1, p2, p3, p4);
    assert_approx_eq(cos_y, 0.0, 1e-10, "planar cosY");
}

#[test]
fn test_cos_y_perpendicular() {
    // L is directly above the plane (along the normal).
    // cosY = ±1.
    let p1 = [1.0, 0.0, 0.0]; // I
    let p2 = [0.0, 0.0, 0.0]; // J (center)
    let p3 = [0.0, 1.0, 0.0]; // K
    let p4 = [0.0, 0.0, 1.0]; // L (along +Z, which is rJI×rJK direction)
    let cos_y = calculate_cos_y(p1, p2, p3, p4);
    assert_approx_eq(cos_y.abs(), 1.0, 1e-10, "perpendicular cosY");
}

#[test]
fn test_cos_y_45_degrees() {
    // L is at 45° from the plane.
    // Wilson angle = 45°, so Y = 45° from normal, cosY = cos(45°) = 1/sqrt(2)
    let p1 = [1.0, 0.0, 0.0]; // I
    let p2 = [0.0, 0.0, 0.0]; // J (center)
    let p3 = [0.0, 1.0, 0.0]; // K
    // Normal is along +Z. L at 45° from normal means 45° from Z axis.
    let p4 = [0.0, 1.0, 1.0]; // In YZ plane at 45° from Z
    let cos_y = calculate_cos_y(p1, p2, p3, p4);
    assert_approx_eq(cos_y.abs(), 1.0 / 2.0_f64.sqrt(), 1e-6, "45-degree cosY");
}

#[test]
fn test_cos_y_degenerate_collinear() {
    // I and K are collinear from J → cross product is zero → cosY = 0
    let p1 = [1.0, 0.0, 0.0];
    let p2 = [0.0, 0.0, 0.0];
    let p3 = [-1.0, 0.0, 0.0]; // collinear with p1
    let p4 = [0.0, 1.0, 0.0];
    let cos_y = calculate_cos_y(p1, p2, p3, p4);
    assert_approx_eq(cos_y, 0.0, 1e-10, "collinear cosY");
}

#[test]
fn test_cos_y_degenerate_zero_distance() {
    // Atom at same position as center → cosY = 0
    let p1 = [0.0, 0.0, 0.0];
    let p2 = [0.0, 0.0, 0.0];
    let p3 = [0.0, 1.0, 0.0];
    let p4 = [0.0, 0.0, 1.0];
    let cos_y = calculate_cos_y(p1, p2, p3, p4);
    assert_approx_eq(cos_y, 0.0, 1e-10, "zero distance cosY");
}

// ============================================================================
// Test: Inversion energy at specific configurations
// ============================================================================

#[test]
fn test_inversion_energy_planar_carbon() {
    // Planar sp2 carbon: all atoms in the XY plane → E = 0
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    // Trigonal planar geometry in XY plane
    let positions = [
        1.0, 0.0, 0.0, // atom 0 (I)
        0.0, 0.0, 0.0, // atom 1 (J, center)
        -0.5, 0.866, 0.0, // atom 2 (K)
        -0.5, -0.866, 0.0, // atom 3 (L, in plane)
    ];

    let energy = inversion_energy(&params, &positions);
    assert_approx_eq(energy, 0.0, 1e-6, "planar carbon E=0");
}

#[test]
fn test_inversion_energy_out_of_plane_carbon() {
    // Carbon with L atom displaced out of the IJK plane → E > 0
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, // atom 0 (I)
        0.0, 0.0, 0.0, // atom 1 (J, center)
        -0.5, 0.866, 0.0, // atom 2 (K)
        -0.5, -0.866, 0.5, // atom 3 (L, displaced in Z)
    ];

    let energy = inversion_energy(&params, &positions);
    assert!(
        energy > 0.0,
        "out-of-plane carbon should have E > 0, got {energy}"
    );
}

#[test]
fn test_inversion_energy_carbon_bound_to_o_higher() {
    // Carbon bound to O has higher force constant → larger energy for same displacement
    let (k_no_o, c0_no_o, c1_no_o, c2_no_o) =
        calc_inversion_coefficients_and_force_constant(6, false);
    let (k_with_o, c0_with_o, c1_with_o, c2_with_o) =
        calc_inversion_coefficients_and_force_constant(6, true);

    let params_no_o = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k_no_o,
        c0: c0_no_o,
        c1: c1_no_o,
        c2: c2_no_o,
    };
    let params_with_o = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k_with_o,
        c0: c0_with_o,
        c1: c1_with_o,
        c2: c2_with_o,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.5,
    ];

    let e_no_o = inversion_energy(&params_no_o, &positions);
    let e_with_o = inversion_energy(&params_with_o, &positions);

    assert!(
        e_with_o > e_no_o,
        "C=O should have higher energy: e_with_o={e_with_o}, e_no_o={e_no_o}"
    );
    // K ratio is 50/6 ≈ 8.33, energy should scale proportionally
    assert_approx_eq(e_with_o / e_no_o, 50.0 / 6.0, 1e-6, "C=O energy ratio");
}

#[test]
fn test_inversion_energy_only_matches_gradient_version() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.2, 0.3, -0.1, 0.0, 0.0, 0.0, -0.5, 0.9, 0.1, -0.4, -0.8, 0.6,
    ];

    let e1 = inversion_energy(&params, &positions);
    let mut gradients = vec![0.0; 12];
    let e2 = inversion_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(e1, e2, 1e-10, "energy-only vs energy-and-gradient");
}

// ============================================================================
// Test A4: Numerical vs analytical gradient for inversion
// ============================================================================

fn numerical_gradient_inversion(params: &InversionParams, positions: &[f64], h: f64) -> Vec<f64> {
    let n = positions.len();
    let mut grad = vec![0.0; n];

    for i in 0..n {
        let mut pos_plus = positions.to_vec();
        let mut pos_minus = positions.to_vec();
        pos_plus[i] += h;
        pos_minus[i] -= h;

        let e_plus = inversion_energy(params, &pos_plus);
        let e_minus = inversion_energy(params, &pos_minus);
        grad[i] = (e_plus - e_minus) / (2.0 * h);
    }

    grad
}

fn assert_inversion_gradient_matches_numerical(
    params: &InversionParams,
    positions: &[f64],
    label: &str,
) {
    let h = 1e-5;
    let rel_tol = 0.01; // 1% relative error
    let abs_tol = 1e-6;

    let mut analytical = vec![0.0; positions.len()];
    inversion_energy_and_gradient(params, positions, &mut analytical);

    let numerical = numerical_gradient_inversion(params, positions, h);

    for i in 0..positions.len() {
        let a = analytical[i];
        let n = numerical[i];
        let diff = (a - n).abs();

        if a.abs() < abs_tol && n.abs() < abs_tol {
            assert!(
                diff < abs_tol,
                "{label}: gradient[{i}] analytical={a}, numerical={n}, diff={diff} (near-zero)"
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
fn test_numerical_gradient_carbon_sp2_basic() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    // Basic trigonal geometry with slight out-of-plane displacement
    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.3,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "C sp2 basic");
}

#[test]
fn test_numerical_gradient_carbon_bound_to_o() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, true);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.4,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "C=O sp2");
}

#[test]
fn test_numerical_gradient_nitrogen_sp2() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(7, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.3,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "N sp2");
}

#[test]
fn test_numerical_gradient_phosphorus() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(15, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.4,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "P group 15");
}

#[test]
fn test_numerical_gradient_bismuth() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(83, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.4,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "Bi group 15");
}

#[test]
fn test_numerical_gradient_3d_off_axis() {
    // General 3D configuration with all atoms off-axis
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        0.3, 1.2, -0.5, -0.1, 0.1, 0.2, 1.3, -0.2, 0.1, -0.8, -0.7, 1.1,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "C sp2 3D off-axis");
}

#[test]
fn test_numerical_gradient_non_origin() {
    // Translation invariance: all atoms away from origin
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        6.0, 5.0, 3.0, 5.0, 5.0, 3.0, 4.5, 5.866, 3.0, 4.5, 4.134, 3.3,
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "C sp2 non-origin");
}

#[test]
fn test_numerical_gradient_large_displacement() {
    // Large out-of-plane displacement
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866,
        1.5, // very far out of plane
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "C sp2 large displacement");
}

#[test]
fn test_numerical_gradient_near_planar() {
    // Nearly planar (small out-of-plane displacement)
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866,
        0.05, // tiny Z displacement
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "C sp2 near planar");
}

#[test]
fn test_numerical_gradient_asymmetric() {
    // Asymmetric geometry (bond lengths differ)
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(7, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.5, 0.0, 0.0, // long bond
        0.0, 0.0, 0.0, -0.3, 0.5, 0.0, // short bond
        -0.8, -1.2, 0.4, // medium bond, out of plane
    ];
    assert_inversion_gradient_matches_numerical(&params, &positions, "N sp2 asymmetric");
}

// ============================================================================
// Test: Gradient force balance (translational invariance)
// ============================================================================

#[test]
fn test_gradient_sum_zero() {
    // For an isolated inversion term, sum of gradients on all four atoms = 0
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        0.3, 1.2, -0.5, -0.1, 0.1, 0.2, 1.3, -0.2, 0.1, -0.8, -0.7, 1.1,
    ];
    let mut gradients = vec![0.0; 12];
    inversion_energy_and_gradient(&params, &positions, &mut gradients);

    let sum_x = gradients[0] + gradients[3] + gradients[6] + gradients[9];
    let sum_y = gradients[1] + gradients[4] + gradients[7] + gradients[10];
    let sum_z = gradients[2] + gradients[5] + gradients[8] + gradients[11];

    assert_approx_eq(sum_x, 0.0, 1e-8, "gradient sum x");
    assert_approx_eq(sum_y, 0.0, 1e-8, "gradient sum y");
    assert_approx_eq(sum_z, 0.0, 1e-8, "gradient sum z");
}

#[test]
fn test_gradient_sum_zero_phosphorus() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(15, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        0.3, 1.2, -0.5, -0.1, 0.1, 0.2, 1.3, -0.2, 0.1, -0.8, -0.7, 1.1,
    ];
    let mut gradients = vec![0.0; 12];
    inversion_energy_and_gradient(&params, &positions, &mut gradients);

    let sum_x = gradients[0] + gradients[3] + gradients[6] + gradients[9];
    let sum_y = gradients[1] + gradients[4] + gradients[7] + gradients[10];
    let sum_z = gradients[2] + gradients[5] + gradients[8] + gradients[11];

    assert_approx_eq(sum_x, 0.0, 1e-8, "P gradient sum x");
    assert_approx_eq(sum_y, 0.0, 1e-8, "P gradient sum y");
    assert_approx_eq(sum_z, 0.0, 1e-8, "P gradient sum z");
}

// ============================================================================
// Test: Gradient accumulation
// ============================================================================

#[test]
fn test_gradient_accumulation() {
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.3,
    ];

    let mut grad1 = vec![0.0; 12];
    inversion_energy_and_gradient(&params, &positions, &mut grad1);

    let mut grad2 = grad1.clone();
    inversion_energy_and_gradient(&params, &positions, &mut grad2);

    for i in 0..12 {
        assert_approx_eq(
            grad2[i],
            2.0 * grad1[i],
            1e-10,
            &format!("gradient accumulation [{i}]"),
        );
    }
}

// ============================================================================
// Test: Gradient at planar equilibrium (should be zero for C/N/O)
// ============================================================================

#[test]
fn test_gradient_zero_at_planar_equilibrium() {
    // For C/N/O: equilibrium is planar (E=0), so gradient should be zero
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);
    let params = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    // Perfect trigonal planar geometry
    let positions = [
        1.0, 0.0, 0.0, // I
        0.0, 0.0, 0.0, // J (center)
        -0.5, 0.866025, 0.0, // K (120°)
        -0.5, -0.866025, 0.0, // L (240°, in plane)
    ];

    let mut gradients = vec![0.0; 12];
    let energy = inversion_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(energy, 0.0, 1e-6, "planar equilibrium energy");
    for i in 0..12 {
        assert_approx_eq(
            gradients[i],
            0.0,
            1e-6,
            &format!("planar equilibrium gradient[{i}]"),
        );
    }
}

// ============================================================================
// Test: Multiple inversion permutations (as used in actual force field)
// ============================================================================

#[test]
fn test_three_permutations_sum() {
    // For an sp2 center with 3 neighbors (I=0, K=2, L=3), the force field creates
    // 3 inversion terms by permuting which atom is L. The total energy from all 3
    // permutations should equal the "full" inversion energy.
    let (k, c0, c1, c2) = calc_inversion_coefficients_and_force_constant(6, false);

    // The 3 permutations: L=3 (I=0,K=2), L=0 (I=2,K=3), L=2 (I=3,K=0)
    // Center is always atom 1.
    let p1 = InversionParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        force_constant: k,
        c0,
        c1,
        c2,
    };
    let p2 = InversionParams {
        idx1: 2,
        idx2: 1,
        idx3: 3,
        idx4: 0,
        force_constant: k,
        c0,
        c1,
        c2,
    };
    let p3 = InversionParams {
        idx1: 3,
        idx2: 1,
        idx3: 0,
        idx4: 2,
        force_constant: k,
        c0,
        c1,
        c2,
    };

    let positions = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.866, 0.0, -0.5, -0.866, 0.4,
    ];

    let e1 = inversion_energy(&p1, &positions);
    let e2 = inversion_energy(&p2, &positions);
    let e3 = inversion_energy(&p3, &positions);
    let total = e1 + e2 + e3;

    // Each term gets K/3, so total should be as if K was the full value
    assert!(total > 0.0, "total inversion energy should be > 0");

    // Also test gradients sum correctly
    let mut gradients = vec![0.0; 12];
    inversion_energy_and_gradient(&p1, &positions, &mut gradients);
    inversion_energy_and_gradient(&p2, &positions, &mut gradients);
    inversion_energy_and_gradient(&p3, &positions, &mut gradients);

    // Total gradient should still have zero sum (translational invariance)
    let sum_x = gradients[0] + gradients[3] + gradients[6] + gradients[9];
    let sum_y = gradients[1] + gradients[4] + gradients[7] + gradients[10];
    let sum_z = gradients[2] + gradients[5] + gradients[8] + gradients[11];

    assert_approx_eq(sum_x, 0.0, 1e-7, "3-perm gradient sum x");
    assert_approx_eq(sum_y, 0.0, 1e-7, "3-perm gradient sum y");
    assert_approx_eq(sum_z, 0.0, 1e-7, "3-perm gradient sum z");
}
