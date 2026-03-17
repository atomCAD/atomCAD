use rust_lib_flutter_cad::crystolecule::simulation::uff::energy::{
    TorsionAngleParams, calculate_cos_torsion, torsion_energy, torsion_energy_and_gradient,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::params::{
    Hybridization, calc_torsion_params, get_uff_params, is_in_group6,
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
// Test: is_in_group6
// ============================================================================

#[test]
fn test_is_in_group6() {
    // Group 6 elements: O(8), S(16), Se(34), Te(52), Po(84)
    assert!(is_in_group6(8), "oxygen");
    assert!(is_in_group6(16), "sulfur");
    assert!(is_in_group6(34), "selenium");
    assert!(is_in_group6(52), "tellurium");
    assert!(is_in_group6(84), "polonium");

    assert!(!is_in_group6(6), "carbon");
    assert!(!is_in_group6(7), "nitrogen");
    assert!(!is_in_group6(1), "hydrogen");
    assert!(!is_in_group6(15), "phosphorus");
}

// ============================================================================
// Test: Torsion parameter calculation
// ============================================================================

#[test]
fn test_torsion_params_sp3_sp3_general() {
    // General sp3-sp3: V = sqrt(V1_i * V1_j), n=3, cosTerm=-1
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    // V = sqrt(2.119 * 2.119) = 2.119
    assert_approx_eq(tp.force_constant, 2.119, 1e-4, "sp3-sp3 V");
    assert_eq!(tp.order, 3, "sp3-sp3 order");
    assert_approx_eq(tp.cos_term, -1.0, 1e-10, "sp3-sp3 cos_term");
}

#[test]
fn test_torsion_params_sp3_sp3_group6_oxygen() {
    // Special case: O-O single bond (group 6 - group 6)
    // V = sqrt(2.0 * 2.0) = 2.0, n=2, cosTerm=-1
    let o3 = get_uff_params("O_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        8,
        Hybridization::SP3,
        Hybridization::SP3,
        o3,
        o3,
        false,
    );
    assert_approx_eq(tp.force_constant, 2.0, 1e-4, "O-O V");
    assert_eq!(tp.order, 2, "O-O order");
    assert_approx_eq(tp.cos_term, -1.0, 1e-10, "O-O cos_term");
}

#[test]
fn test_torsion_params_sp3_sp3_group6_sulfur() {
    // S-S single bond: V = sqrt(6.8 * 6.8) = 6.8, n=2
    let s3 = get_uff_params("S_3+2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        16,
        16,
        Hybridization::SP3,
        Hybridization::SP3,
        s3,
        s3,
        false,
    );
    assert_approx_eq(tp.force_constant, 6.8, 1e-4, "S-S V");
    assert_eq!(tp.order, 2, "S-S order");
}

#[test]
fn test_torsion_params_sp3_sp3_group6_mixed_os() {
    // O-S single bond: V = sqrt(2.0 * 6.8), n=2
    let o3 = get_uff_params("O_3").unwrap();
    let s3 = get_uff_params("S_3+2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        16,
        Hybridization::SP3,
        Hybridization::SP3,
        o3,
        s3,
        false,
    );
    let expected_v = (2.0_f64 * 6.8).sqrt();
    assert_approx_eq(tp.force_constant, expected_v, 1e-4, "O-S V");
    assert_eq!(tp.order, 2, "O-S order");
}

#[test]
fn test_torsion_params_sp3_sp3_group6_not_single_bond() {
    // Group 6 - group 6 but NOT single bond: falls through to general case
    let o3 = get_uff_params("O_3").unwrap();
    let tp = calc_torsion_params(
        2.0,
        8,
        8,
        Hybridization::SP3,
        Hybridization::SP3,
        o3,
        o3,
        false,
    );
    // General case: V = sqrt(V1_O * V1_O) = sqrt(0.018 * 0.018) = 0.018
    assert_approx_eq(tp.force_constant, 0.018, 1e-4, "O=O general V");
    assert_eq!(tp.order, 3, "O=O general order");
}

#[test]
fn test_torsion_params_sp2_sp2() {
    // sp2-sp2: uses equation 17
    // V = 5 * sqrt(U1_i * U1_j) * (1 + 4.18*ln(bondOrder))
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        2.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP2,
        c2,
        c2,
        false,
    );
    // V = 5 * sqrt(2.0 * 2.0) * (1 + 4.18 * ln(2.0))
    //   = 5 * 2.0 * (1 + 4.18 * 0.6931) = 10 * (1 + 2.8973) = 10 * 3.8973 = 38.973
    let expected_v = 5.0 * (c2.u1 * c2.u1).sqrt() * (1.0 + 4.18 * 2.0_f64.ln());
    assert_approx_eq(tp.force_constant, expected_v, 1e-2, "sp2-sp2 V");
    assert_eq!(tp.order, 2, "sp2-sp2 order");
    assert_approx_eq(tp.cos_term, 1.0, 1e-10, "sp2-sp2 cos_term");
}

#[test]
fn test_torsion_params_sp2_sp2_single_bond() {
    // sp2-sp2 with single bond order
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP2,
        c2,
        c2,
        false,
    );
    // V = 5 * sqrt(2.0 * 2.0) * (1 + 4.18 * ln(1.0)) = 5 * 2.0 * 1.0 = 10.0
    let expected_v = 5.0 * (c2.u1 * c2.u1).sqrt() * (1.0 + 4.18 * 1.0_f64.ln());
    assert_approx_eq(tp.force_constant, expected_v, 1e-4, "sp2-sp2 single V");
    assert_approx_eq(expected_v, 10.0, 1e-4, "equation 17 single bond");
}

#[test]
fn test_torsion_params_sp2_sp3_default() {
    // Default sp2-sp3: V=1, n=6, cosTerm=1
    let c3 = get_uff_params("C_3").unwrap();
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP3,
        c2,
        c3,
        false,
    );
    assert_approx_eq(tp.force_constant, 1.0, 1e-10, "sp2-sp3 default V");
    assert_eq!(tp.order, 6, "sp2-sp3 default order");
    assert_approx_eq(tp.cos_term, 1.0, 1e-10, "sp2-sp3 default cos_term");
}

#[test]
fn test_torsion_params_sp3_group6_sp2_non_group6() {
    // Special case: sp3 group6 (O) with sp2 non-group6 (C)
    // Uses equation 17, n=2, cosTerm=-1
    let o3 = get_uff_params("O_3").unwrap();
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        6,
        Hybridization::SP3,
        Hybridization::SP2,
        o3,
        c2,
        false,
    );
    let expected_v = 5.0 * (o3.u1 * c2.u1).sqrt() * (1.0 + 4.18 * 1.0_f64.ln());
    assert_approx_eq(tp.force_constant, expected_v, 1e-4, "O_sp3-C_sp2 V");
    assert_eq!(tp.order, 2, "O_sp3-C_sp2 order");
    assert_approx_eq(tp.cos_term, -1.0, 1e-10, "O_sp3-C_sp2 cos_term");
}

#[test]
fn test_torsion_params_sp2_sp3_end_atom_sp2() {
    // Special case: sp2-sp3 with end atom also sp2 (propene-like)
    // V=2, n=3, cosTerm=-1
    let c3 = get_uff_params("C_3").unwrap();
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP3,
        c2,
        c3,
        true,
    );
    assert_approx_eq(tp.force_constant, 2.0, 1e-10, "sp2-sp3 endSP2 V");
    assert_eq!(tp.order, 3, "sp2-sp3 endSP2 order");
    assert_approx_eq(tp.cos_term, -1.0, 1e-10, "sp2-sp3 endSP2 cos_term");
}

// ============================================================================
// Test: calculate_cos_torsion
// ============================================================================

#[test]
fn test_cos_torsion_trans() {
    // Trans configuration: dihedral = 180°, cos = -1
    // Atoms: p1=(0,1,0), p2=(0,0,0), p3=(1,0,0), p4=(1,-1,0)
    let cos_phi = calculate_cos_torsion(
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, -1.0, 0.0],
    );
    assert_approx_eq(cos_phi, -1.0, 1e-10, "trans cos_phi");
}

#[test]
fn test_cos_torsion_cis() {
    // Cis configuration: dihedral = 0°, cos = 1
    // Atoms: p1=(0,1,0), p2=(0,0,0), p3=(1,0,0), p4=(1,1,0)
    let cos_phi = calculate_cos_torsion(
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    );
    assert_approx_eq(cos_phi, 1.0, 1e-10, "cis cos_phi");
}

#[test]
fn test_cos_torsion_90_degrees() {
    // 90° dihedral: cos = 0
    // Atoms: p1=(0,1,0), p2=(0,0,0), p3=(1,0,0), p4=(1,0,1)
    let cos_phi = calculate_cos_torsion(
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 1.0],
    );
    assert_approx_eq(cos_phi, 0.0, 1e-10, "90deg cos_phi");
}

#[test]
fn test_cos_torsion_60_degrees() {
    // 60° dihedral: cos = 0.5
    // Atoms: p1=(0,1,0), p2=(0,0,0), p3=(1,0,0), p4=(1,0.5,sqrt(3)/2)
    let cos_phi = calculate_cos_torsion(
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.5, 3.0_f64.sqrt() / 2.0],
    );
    assert_approx_eq(cos_phi, 0.5, 1e-6, "60deg cos_phi");
}

// ============================================================================
// Test: Torsion energy at specific configurations
// ============================================================================

#[test]
fn test_torsion_energy_at_equilibrium_sp3_sp3() {
    // sp3-sp3: n=3, cosTerm=-1 (phi0=60°)
    // At phi=60°: cos(phi)=0.5, cos(3*60°)=cos(180°)=-1
    // E = V/2 * (1 - (-1)*(-1)) = V/2 * 0 = 0
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    // Place atoms so dihedral = 60°
    // p1=(0,1,0), p2=(0,0,0), p3=(1,0,0), p4=(1, cos(60°), sin(60°)) = (1, 0.5, sqrt(3)/2)
    let positions = [
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        1.0,
        0.5,
        3.0_f64.sqrt() / 2.0,
    ];
    let energy = torsion_energy(&params, &positions);
    assert_approx_eq(energy, 0.0, 1e-6, "sp3-sp3 energy at equilibrium (60°)");
}

#[test]
fn test_torsion_energy_at_eclipsed_sp3_sp3() {
    // sp3-sp3: at phi=0° (eclipsed), cos(0)=1, cos(3*0)=1
    // E = V/2 * (1 - (-1)*1) = V/2 * 2 = V
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    // Cis configuration: phi=0°
    let positions = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0];
    let energy = torsion_energy(&params, &positions);
    // E = V = 2.119 (for C_3 - C_3)
    assert_approx_eq(energy, tp.force_constant, 1e-4, "sp3-sp3 eclipsed energy");
}

#[test]
fn test_torsion_energy_at_equilibrium_sp2_sp2() {
    // sp2-sp2: n=2, cosTerm=1 (phi0=180°)
    // At phi=180°: cos(180°)=-1, cos(2*180°)=cos(360°)=1
    // E = V/2 * (1 - 1*1) = 0
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP2,
        c2,
        c2,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    // Trans configuration: phi=180°
    let positions = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, -1.0, 0.0];
    let energy = torsion_energy(&params, &positions);
    assert_approx_eq(energy, 0.0, 1e-6, "sp2-sp2 energy at equilibrium (180°)");
}

#[test]
fn test_torsion_energy_only_matches_gradient_version() {
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.3, 1.2, -0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.4, 1.3];

    let e1 = torsion_energy(&params, &positions);
    let mut gradients = vec![0.0; 12];
    let e2 = torsion_energy_and_gradient(&params, &positions, &mut gradients);

    assert_approx_eq(e1, e2, 1e-12, "energy-only vs energy-and-gradient");
}

// ============================================================================
// Test A4/A6: Numerical vs analytical gradient for torsion
// ============================================================================

fn numerical_gradient_torsion(params: &TorsionAngleParams, positions: &[f64], h: f64) -> Vec<f64> {
    let n = positions.len();
    let mut grad = vec![0.0; n];

    for i in 0..n {
        let mut pos_plus = positions.to_vec();
        let mut pos_minus = positions.to_vec();
        pos_plus[i] += h;
        pos_minus[i] -= h;

        let e_plus = torsion_energy(params, &pos_plus);
        let e_minus = torsion_energy(params, &pos_minus);
        grad[i] = (e_plus - e_minus) / (2.0 * h);
    }

    grad
}

fn assert_torsion_gradient_matches_numerical(
    params: &TorsionAngleParams,
    positions: &[f64],
    label: &str,
) {
    let h = 1e-5;
    let rel_tol = 0.01; // 1% relative error
    let abs_tol = 1e-6;

    let mut analytical = vec![0.0; positions.len()];
    torsion_energy_and_gradient(params, positions, &mut analytical);

    let numerical = numerical_gradient_torsion(params, positions, h);

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
fn test_numerical_gradient_sp3_sp3_basic() {
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.0, 1.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "sp3-sp3 basic");
}

#[test]
fn test_numerical_gradient_sp2_sp2() {
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP2,
        c2,
        c2,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "sp2-sp2");
}

#[test]
fn test_numerical_gradient_sp2_sp3() {
    let c2 = get_uff_params("C_2").unwrap();
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP3,
        c2,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "sp2-sp3 default");
}

#[test]
fn test_numerical_gradient_group6_group6() {
    // O-O torsion (group 6 special case, n=2)
    let o3 = get_uff_params("O_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        8,
        Hybridization::SP3,
        Hybridization::SP3,
        o3,
        o3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "group6-group6 O-O");
}

#[test]
fn test_numerical_gradient_sp2_sp3_end_sp2() {
    // Propene-like: sp2-sp3 with end atom sp2, n=3
    let c2 = get_uff_params("C_2").unwrap();
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP3,
        c2,
        c3,
        true,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "sp2-sp3 endSP2");
}

#[test]
fn test_numerical_gradient_off_axis_3d() {
    // General 3D configuration with all atoms off-axis
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [
        0.3, 1.2, -0.5, -0.1, 0.1, 0.2, 1.3, -0.2, 0.1, 1.8, 0.7, 1.1,
    ];
    assert_torsion_gradient_matches_numerical(&params, &positions, "sp3-sp3 3D off-axis");
}

#[test]
fn test_numerical_gradient_near_trans() {
    // Near trans (phi ≈ 180°) — tests sin(phi) ≈ 0 degenerate handling
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP2,
        c2,
        c2,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    // Almost exactly trans
    let positions = [
        0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, -1.0,
        0.01, // slight z perturbation from exact trans
    ];
    assert_torsion_gradient_matches_numerical(&params, &positions, "near-trans sp2-sp2");
}

#[test]
fn test_numerical_gradient_non_origin() {
    // Translation invariance: all atoms away from origin
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [5.0, 4.5, 3.0, 5.0, 3.0, 3.0, 6.5, 3.0, 3.0, 6.5, 3.0, 4.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "sp3-sp3 non-origin");
}

#[test]
fn test_numerical_gradient_group6_sp3_sp2() {
    // O(sp3) - C(sp2) special case, n=2
    let o3 = get_uff_params("O_3").unwrap();
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        6,
        Hybridization::SP3,
        Hybridization::SP2,
        o3,
        c2,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    assert_torsion_gradient_matches_numerical(&params, &positions, "O_sp3-C_sp2");
}

#[test]
fn test_gradient_sum_zero() {
    // For an isolated torsion, the sum of gradients on all four atoms should be zero
    // (force balance / translational invariance)
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [
        0.3, 1.2, -0.5, -0.1, 0.1, 0.2, 1.3, -0.2, 0.1, 1.8, 0.7, 1.1,
    ];
    let mut gradients = vec![0.0; 12];
    torsion_energy_and_gradient(&params, &positions, &mut gradients);

    // Sum of x-gradients, y-gradients, z-gradients should each be zero
    let sum_x = gradients[0] + gradients[3] + gradients[6] + gradients[9];
    let sum_y = gradients[1] + gradients[4] + gradients[7] + gradients[10];
    let sum_z = gradients[2] + gradients[5] + gradients[8] + gradients[11];

    assert_approx_eq(sum_x, 0.0, 1e-8, "gradient sum x");
    assert_approx_eq(sum_y, 0.0, 1e-8, "gradient sum y");
    assert_approx_eq(sum_z, 0.0, 1e-8, "gradient sum z");
}

#[test]
fn test_gradient_accumulation() {
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.0, 1.5];

    let mut grad1 = vec![0.0; 12];
    torsion_energy_and_gradient(&params, &positions, &mut grad1);

    let mut grad2 = grad1.clone();
    torsion_energy_and_gradient(&params, &positions, &mut grad2);

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
// Test A6: Equilibrium torsion angles by hybridization (RDKit testUFF7)
// ============================================================================
// These tests use steepest descent minimization to find equilibrium angles.
// Reference: RDKit testUFF7 expects specific cos(phi) values at equilibrium.

/// Simple steepest descent minimizer for torsion angle.
/// Returns the positions after minimization.
fn minimize_torsion(params: &TorsionAngleParams, positions: &[f64], max_iter: usize) -> Vec<f64> {
    let mut pos = positions.to_vec();
    let mut step_size = 0.01;

    for _ in 0..max_iter {
        let mut grad = vec![0.0; pos.len()];
        let _energy = torsion_energy_and_gradient(params, &pos, &mut grad);

        let grad_norm: f64 = grad.iter().map(|g| g * g).sum::<f64>().sqrt();
        if grad_norm < 1e-8 {
            break;
        }

        // Backtracking line search
        let current_e = torsion_energy(params, &pos);
        let mut new_pos = pos.clone();
        let mut found = false;
        for _ in 0..20 {
            for i in 0..pos.len() {
                new_pos[i] = pos[i] - step_size * grad[i];
            }
            let new_e = torsion_energy(params, &new_pos);
            if new_e < current_e {
                pos = new_pos.clone();
                step_size *= 1.2;
                found = true;
                break;
            }
            step_size *= 0.5;
        }
        if !found {
            break;
        }
    }
    pos
}

fn cos_torsion_from_positions(positions: &[f64]) -> f64 {
    calculate_cos_torsion(
        [positions[0], positions[1], positions[2]],
        [positions[3], positions[4], positions[5]],
        [positions[6], positions[7], positions[8]],
        [positions[9], positions[10], positions[11]],
    )
}

#[test]
fn test_a6_equilibrium_sp3_sp3() {
    // RDKit testUFF7: sp3-sp3 C-C → cos(phi) = 0.5 (60° dihedral)
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP3,
        Hybridization::SP3,
        c3,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.0, 1.5];
    let result = minimize_torsion(&params, &positions, 500);
    let cos_phi = cos_torsion_from_positions(&result);
    assert_approx_eq(cos_phi, 0.5, 1e-2, "sp3-sp3 equilibrium cos(phi)");
}

#[test]
fn test_a6_equilibrium_sp2_sp2() {
    // RDKit testUFF7: sp2-sp2 → cos(phi) = 1.0 (0° or 180° dihedral)
    // With cosTerm=1 (phi0=180°), equilibrium is at cos(phi)=±1
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP2,
        c2,
        c2,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    let result = minimize_torsion(&params, &positions, 500);
    let cos_phi = cos_torsion_from_positions(&result);
    // Should converge to cos(phi) = 1.0 (trans) or -1.0 (cis)
    assert!(
        (cos_phi - 1.0).abs() < 0.02 || (cos_phi + 1.0).abs() < 0.02,
        "sp2-sp2 should minimize to cos(phi)=±1.0, got {cos_phi}"
    );
}

#[test]
fn test_a6_equilibrium_sp2_sp3() {
    // RDKit testUFF7: sp2-sp3 (default) → cos(phi) = 0.5
    // n=6, cosTerm=1, phi0=0, minima at phi = 60° multiples
    let c2 = get_uff_params("C_2").unwrap();
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP3,
        c2,
        c3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    let result = minimize_torsion(&params, &positions, 500);
    let cos_phi = cos_torsion_from_positions(&result);
    // For n=6 with cosTerm=1, minima are at phi = k*60° for k=0,1,2,...
    // The starting configuration should minimize to the nearest minimum
    // RDKit testUFF7 checks cos(phi) = 0.5 (phi=60°)
    assert_approx_eq(cos_phi, 0.5, 0.02, "sp2-sp3 equilibrium cos(phi)");
}

#[test]
fn test_a6_equilibrium_group6_group6() {
    // RDKit testUFF7: group6-group6 (O-O) → cos(phi) = 0.0 (90° dihedral)
    // n=2, cosTerm=-1, phi0=90°
    let o3 = get_uff_params("O_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        8,
        Hybridization::SP3,
        Hybridization::SP3,
        o3,
        o3,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    let result = minimize_torsion(&params, &positions, 500);
    let cos_phi = cos_torsion_from_positions(&result);
    assert_approx_eq(cos_phi, 0.0, 0.02, "group6-group6 equilibrium cos(phi)");
}

#[test]
fn test_a6_equilibrium_group6_sp3_sp2() {
    // RDKit testUFF7: group6(sp3)-non_group6(sp2) → cos(phi) = 0.0 (90°)
    // Uses equation 17, n=2, cosTerm=-1
    let o3 = get_uff_params("O_3").unwrap();
    let c2 = get_uff_params("C_2").unwrap();
    let tp = calc_torsion_params(
        1.0,
        8,
        6,
        Hybridization::SP3,
        Hybridization::SP2,
        o3,
        c2,
        false,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    let result = minimize_torsion(&params, &positions, 500);
    let cos_phi = cos_torsion_from_positions(&result);
    assert_approx_eq(cos_phi, 0.0, 0.02, "group6_sp3-sp2 equilibrium cos(phi)");
}

#[test]
fn test_a6_equilibrium_sp2_sp3_end_sp2() {
    // RDKit testUFF7: (SP2-)SP2-SP3 with endAtomIsSP2 → cos(phi) = 0.5
    // V=2, n=3, cosTerm=-1 (phi0=60°)
    let c2 = get_uff_params("C_2").unwrap();
    let c3 = get_uff_params("C_3").unwrap();
    let tp = calc_torsion_params(
        1.0,
        6,
        6,
        Hybridization::SP2,
        Hybridization::SP3,
        c2,
        c3,
        true,
    );
    let params = TorsionAngleParams {
        idx1: 0,
        idx2: 1,
        idx3: 2,
        idx4: 3,
        params: tp,
    };

    let positions = [0.0, 1.5, 0.1, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 1.5, 0.2, 1.5];
    let result = minimize_torsion(&params, &positions, 500);
    let cos_phi = cos_torsion_from_positions(&result);
    assert_approx_eq(cos_phi, 0.5, 0.02, "sp2-sp3_endSP2 equilibrium cos(phi)");
}
