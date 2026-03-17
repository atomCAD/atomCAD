// Tests for steepest descent minimizer (Phase 1 of continuous minimization).
//
// Validates:
// - Steepest descent convergence on simple functions and real molecules
// - Frozen atom support
// - Early exit when already at minimum
// - Empty positions edge case

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::simulation::force_field::ForceField;
use rust_lib_flutter_cad::crystolecule::simulation::minimize::steepest_descent_steps;
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;
use rust_lib_flutter_cad::crystolecule::simulation::uff::UffForceField;

// ============================================================================
// Test force fields
// ============================================================================

/// Quadratic bowl: f(x) = 0.5 * sum(a_i * (x_i - c_i)^2)
/// Minimum at x = c with f = 0.
struct QuadraticFF {
    coeffs: Vec<f64>,
    center: Vec<f64>,
}

impl QuadraticFF {
    fn new(coeffs: Vec<f64>, center: Vec<f64>) -> Self {
        assert_eq!(coeffs.len(), center.len());
        Self { coeffs, center }
    }

    fn isotropic(n: usize, k: f64, center: Vec<f64>) -> Self {
        Self::new(vec![k; n], center)
    }
}

impl ForceField for QuadraticFF {
    fn energy_and_gradients(&self, positions: &[f64], energy: &mut f64, gradients: &mut [f64]) {
        *energy = 0.0;
        for (i, (&x, (&a, &c))) in positions
            .iter()
            .zip(self.coeffs.iter().zip(self.center.iter()))
            .enumerate()
        {
            let dx = x - c;
            *energy += 0.5 * a * dx * dx;
            gradients[i] = a * dx;
        }
    }
}

// ============================================================================
// Molecule helpers
// ============================================================================

/// Build a distorted methane where one hydrogen is pulled away.
fn build_distorted_methane() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let d = 1.09;
    let a = d / 3.0_f64.sqrt();
    // First hydrogen pulled out to 2x distance
    let h1 = s.add_atom(1, DVec3::new(2.0 * a, 2.0 * a, 2.0 * a));
    let h2 = s.add_atom(1, DVec3::new(a, -a, -a));
    let h3 = s.add_atom(1, DVec3::new(-a, a, -a));
    let h4 = s.add_atom(1, DVec3::new(-a, -a, a));
    s.add_bond(c, h1, BOND_SINGLE);
    s.add_bond(c, h2, BOND_SINGLE);
    s.add_bond(c, h3, BOND_SINGLE);
    s.add_bond(c, h4, BOND_SINGLE);
    s
}

fn build_ff_and_positions(structure: &AtomicStructure) -> (UffForceField, Vec<f64>) {
    let topology = MolecularTopology::from_structure(structure);
    let ff = UffForceField::from_topology(&topology).expect("Failed to build UFF");
    (ff, topology.positions)
}

// ============================================================================
// Steepest descent: convergence tests
// ============================================================================

#[test]
fn sd_quadratic_converges() {
    let ff = QuadraticFF::isotropic(6, 1.0, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let mut pos = vec![0.0; 6];
    // Steepest descent converges slower than L-BFGS; use many steps
    let energy = steepest_descent_steps(&ff, &mut pos, &[], 500, 0.3);
    assert!(energy < 0.1, "energy should be near zero: {energy}");
    for (i, &expected) in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].iter().enumerate() {
        assert!(
            (pos[i] - expected).abs() < 0.2,
            "coord {i}: {} != {expected}",
            pos[i]
        );
    }
}

#[test]
fn sd_distorted_methane_energy_decreases() {
    let structure = build_distorted_methane();
    let (ff, mut positions) = build_ff_and_positions(&structure);

    let mut initial_energy = 0.0;
    let mut grad = vec![0.0; positions.len()];
    ff.energy_and_gradients(&positions, &mut initial_energy, &mut grad);

    let final_energy = steepest_descent_steps(&ff, &mut positions, &[], 20, 0.1);

    assert!(
        final_energy < initial_energy,
        "energy should decrease: {final_energy} >= {initial_energy}"
    );
}

#[test]
fn sd_ethane_energy_decreases() {
    // Use a distorted ethane (stretch the C-C bond) so there's clear room to improve
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::new(-1.2, 0.0, 0.0)); // stretched from -0.77
    let c2 = s.add_atom(6, DVec3::new(1.2, 0.0, 0.0)); // stretched from 0.77
    s.add_bond(c1, c2, BOND_SINGLE);
    let h1 = s.add_atom(1, DVec3::new(-1.6, 1.03, 0.0));
    let h2 = s.add_atom(1, DVec3::new(-1.6, -0.51, 0.89));
    let h3 = s.add_atom(1, DVec3::new(-1.6, -0.51, -0.89));
    s.add_bond(c1, h1, BOND_SINGLE);
    s.add_bond(c1, h2, BOND_SINGLE);
    s.add_bond(c1, h3, BOND_SINGLE);
    let h4 = s.add_atom(1, DVec3::new(1.6, -1.03, 0.0));
    let h5 = s.add_atom(1, DVec3::new(1.6, 0.51, -0.89));
    let h6 = s.add_atom(1, DVec3::new(1.6, 0.51, 0.89));
    s.add_bond(c2, h4, BOND_SINGLE);
    s.add_bond(c2, h5, BOND_SINGLE);
    s.add_bond(c2, h6, BOND_SINGLE);

    let (ff, mut positions) = build_ff_and_positions(&s);

    let mut initial_energy = 0.0;
    let mut grad = vec![0.0; positions.len()];
    ff.energy_and_gradients(&positions, &mut initial_energy, &mut grad);

    let final_energy = steepest_descent_steps(&ff, &mut positions, &[], 50, 0.1);

    assert!(
        final_energy < initial_energy,
        "energy should decrease: {final_energy} >= {initial_energy}"
    );
}

// ============================================================================
// Steepest descent: frozen atoms
// ============================================================================

#[test]
fn sd_frozen_atoms_stay_fixed() {
    let ff = QuadraticFF::isotropic(6, 1.0, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let mut pos = vec![0.0; 6];

    // Freeze atom 0 (coords 0-2)
    steepest_descent_steps(&ff, &mut pos, &[0], 500, 0.3);

    // Atom 0 should stay at origin
    assert!(pos[0].abs() < 1e-12, "frozen x moved: {}", pos[0]);
    assert!(pos[1].abs() < 1e-12, "frozen y moved: {}", pos[1]);
    assert!(pos[2].abs() < 1e-12, "frozen z moved: {}", pos[2]);

    // Atom 1 should move toward minimum (SD converges slowly, use looser tolerance)
    assert!(
        (pos[3] - 4.0).abs() < 0.2,
        "free x should be ~4: {}",
        pos[3]
    );
    assert!(
        (pos[4] - 5.0).abs() < 0.2,
        "free y should be ~5: {}",
        pos[4]
    );
    assert!(
        (pos[5] - 6.0).abs() < 0.2,
        "free z should be ~6: {}",
        pos[5]
    );
}

#[test]
fn sd_frozen_atom_on_real_molecule() {
    let structure = build_distorted_methane();
    let (ff, mut positions) = build_ff_and_positions(&structure);

    let original_pos = positions.clone();

    // Freeze atom 0 (carbon)
    steepest_descent_steps(&ff, &mut positions, &[0], 20, 0.1);

    // Carbon should not have moved
    assert!(
        (positions[0] - original_pos[0]).abs() < 1e-12,
        "frozen carbon x moved"
    );
    assert!(
        (positions[1] - original_pos[1]).abs() < 1e-12,
        "frozen carbon y moved"
    );
    assert!(
        (positions[2] - original_pos[2]).abs() < 1e-12,
        "frozen carbon z moved"
    );

    // At least one hydrogen should have moved
    let any_moved = (3..positions.len()).any(|i| (positions[i] - original_pos[i]).abs() > 1e-6);
    assert!(any_moved, "no non-frozen atoms moved");
}

// ============================================================================
// Steepest descent: early exit
// ============================================================================

#[test]
fn sd_already_at_minimum_exits_early() {
    let ff = QuadraticFF::isotropic(6, 1.0, vec![0.0; 6]);
    let mut pos = vec![0.0; 6];
    let energy = steepest_descent_steps(&ff, &mut pos, &[], 100, 0.1);
    assert!(energy.abs() < 1e-12, "energy should be zero: {energy}");
    // Positions should be unchanged
    for (i, &p) in pos.iter().enumerate() {
        assert!(p.abs() < 1e-12, "coord {i} moved: {p}");
    }
}

// ============================================================================
// Steepest descent: empty positions
// ============================================================================

#[test]
fn sd_empty_positions() {
    let ff = QuadraticFF::new(vec![], vec![]);
    let mut pos: Vec<f64> = vec![];
    let energy = steepest_descent_steps(&ff, &mut pos, &[], 10, 0.1);
    assert!((energy - 0.0).abs() < 1e-12);
}

// ============================================================================
// Steepest descent: many steps on real molecule converges well
// ============================================================================

#[test]
fn sd_many_steps_converges_on_distorted_methane() {
    let structure = build_distorted_methane();
    let (ff, mut positions) = build_ff_and_positions(&structure);

    let mut initial_energy = 0.0;
    let mut grad = vec![0.0; positions.len()];
    ff.energy_and_gradients(&positions, &mut initial_energy, &mut grad);

    // 200 steps should make significant progress
    let final_energy = steepest_descent_steps(&ff, &mut positions, &[], 200, 0.1);

    // Energy should decrease significantly
    assert!(
        final_energy < initial_energy * 0.1,
        "energy should decrease significantly: {final_energy} vs {initial_energy}"
    );
}
