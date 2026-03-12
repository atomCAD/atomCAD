// Tests for steepest descent minimizer and RestrainedForceField (Phase 1 of
// continuous minimization).
//
// Validates:
// - Steepest descent convergence on simple functions and real molecules
// - Frozen atom support
// - Early exit when already at minimum
// - Empty positions edge case
// - RestrainedForceField gradient correctness (numerical finite difference)
// - RestrainedForceField with k=0 (no restraint contribution)
// - RestrainedForceField with very large k (effectively frozen)
// - Large k spring produces same result as frozen constraint

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::simulation::force_field::{
    ForceField, RestrainedForceField,
};
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

/// Build a methane molecule (CH4) with tetrahedral geometry.
/// Carbon at origin, 4 hydrogens at tetrahedral positions.
fn build_methane() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    // Carbon at origin
    let c = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    // Hydrogens at tetrahedral positions (bond length ~1.09 Å)
    let d = 1.09;
    let a = d / 3.0_f64.sqrt();
    let h1 = s.add_atom(1, DVec3::new(a, a, a));
    let h2 = s.add_atom(1, DVec3::new(a, -a, -a));
    let h3 = s.add_atom(1, DVec3::new(-a, a, -a));
    let h4 = s.add_atom(1, DVec3::new(-a, -a, a));
    s.add_bond(c, h1, BOND_SINGLE);
    s.add_bond(c, h2, BOND_SINGLE);
    s.add_bond(c, h3, BOND_SINGLE);
    s.add_bond(c, h4, BOND_SINGLE);
    s
}

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
// RestrainedForceField: gradient numerical check
// ============================================================================

#[test]
fn restrained_ff_gradient_matches_numerical() {
    let structure = build_methane();
    let (ff, positions) = build_ff_and_positions(&structure);

    let restrained = RestrainedForceField {
        base: &ff,
        // Restrain atom 1 (first hydrogen) to a shifted position
        restraints: vec![(
            1,
            positions[3] + 0.5,
            positions[4] - 0.3,
            positions[5] + 0.2,
        )],
        spring_constant: 200.0,
    };

    let n = positions.len();
    let mut energy = 0.0;
    let mut analytical_grad = vec![0.0; n];
    restrained.energy_and_gradients(&positions, &mut energy, &mut analytical_grad);

    // Finite difference check
    let eps = 1e-5;
    for i in 0..n {
        let mut pos_plus = positions.clone();
        let mut pos_minus = positions.clone();
        pos_plus[i] += eps;
        pos_minus[i] -= eps;

        let mut e_plus = 0.0;
        let mut e_minus = 0.0;
        let mut dummy_grad = vec![0.0; n];
        restrained.energy_and_gradients(&pos_plus, &mut e_plus, &mut dummy_grad);
        restrained.energy_and_gradients(&pos_minus, &mut e_minus, &mut dummy_grad);

        let numerical = (e_plus - e_minus) / (2.0 * eps);
        let analytical = analytical_grad[i];

        let abs_diff = (analytical - numerical).abs();
        let rel_err = if analytical.abs() > 1e-8 {
            abs_diff / analytical.abs()
        } else {
            abs_diff
        };

        assert!(
            rel_err < 0.01,
            "coord {i}: analytical={analytical:.8}, numerical={numerical:.8}, rel_err={rel_err:.6}"
        );
    }
}

#[test]
fn restrained_ff_multiple_restraints_gradient_check() {
    let structure = build_methane();
    let (ff, positions) = build_ff_and_positions(&structure);

    // Restrain all 4 hydrogens (atoms 1-4) to shifted positions
    let restrained = RestrainedForceField {
        base: &ff,
        restraints: vec![
            (1, positions[3] + 0.3, positions[4], positions[5]),
            (2, positions[6], positions[7] + 0.4, positions[8]),
            (3, positions[9], positions[10], positions[11] - 0.2),
            (4, positions[12] - 0.1, positions[13] + 0.1, positions[14]),
        ],
        spring_constant: 150.0,
    };

    let n = positions.len();
    let mut energy = 0.0;
    let mut analytical_grad = vec![0.0; n];
    restrained.energy_and_gradients(&positions, &mut energy, &mut analytical_grad);

    let eps = 1e-5;
    for i in 0..n {
        let mut pos_plus = positions.clone();
        let mut pos_minus = positions.clone();
        pos_plus[i] += eps;
        pos_minus[i] -= eps;

        let mut e_plus = 0.0;
        let mut e_minus = 0.0;
        let mut dummy_grad = vec![0.0; n];
        restrained.energy_and_gradients(&pos_plus, &mut e_plus, &mut dummy_grad);
        restrained.energy_and_gradients(&pos_minus, &mut e_minus, &mut dummy_grad);

        let numerical = (e_plus - e_minus) / (2.0 * eps);
        let analytical = analytical_grad[i];

        let abs_diff = (analytical - numerical).abs();
        let rel_err = if analytical.abs() > 1e-8 {
            abs_diff / analytical.abs()
        } else {
            abs_diff
        };

        assert!(
            rel_err < 0.01,
            "coord {i}: analytical={analytical:.8}, numerical={numerical:.8}, rel_err={rel_err:.6}"
        );
    }
}

// ============================================================================
// RestrainedForceField: k=0 matches base
// ============================================================================

#[test]
fn restrained_ff_k_zero_matches_base() {
    let structure = build_methane();
    let (ff, positions) = build_ff_and_positions(&structure);

    let restrained = RestrainedForceField {
        base: &ff,
        restraints: vec![
            (1, 10.0, 20.0, 30.0), // far target, but k=0 so no effect
        ],
        spring_constant: 0.0,
    };

    let n = positions.len();
    let mut base_energy = 0.0;
    let mut base_grad = vec![0.0; n];
    ff.energy_and_gradients(&positions, &mut base_energy, &mut base_grad);

    let mut restrained_energy = 0.0;
    let mut restrained_grad = vec![0.0; n];
    restrained.energy_and_gradients(&positions, &mut restrained_energy, &mut restrained_grad);

    assert!(
        (base_energy - restrained_energy).abs() < 1e-12,
        "energies differ: {base_energy} vs {restrained_energy}"
    );
    for i in 0..n {
        assert!(
            (base_grad[i] - restrained_grad[i]).abs() < 1e-12,
            "grad {i} differs: {} vs {}",
            base_grad[i],
            restrained_grad[i]
        );
    }
}

// ============================================================================
// RestrainedForceField: very large k keeps atom near target
// ============================================================================

#[test]
fn restrained_ff_large_k_keeps_atom_near_target() {
    let structure = build_distorted_methane();
    let (ff, mut positions) = build_ff_and_positions(&structure);

    // Target for atom 1 (the distorted hydrogen) at a specific position
    let target = (positions[3], positions[4], positions[5]);

    let restrained = RestrainedForceField {
        base: &ff,
        restraints: vec![(1, target.0, target.1, target.2)],
        spring_constant: 10000.0,
    };

    steepest_descent_steps(&restrained, &mut positions, &[], 50, 0.1);

    // Atom 1 should remain very close to its target
    let dx = positions[3] - target.0;
    let dy = positions[4] - target.1;
    let dz = positions[5] - target.2;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    assert!(
        dist < 0.05,
        "atom with k=10000 moved {dist:.4} Å from target"
    );
}

// ============================================================================
// Large k spring produces same result as frozen constraint
// ============================================================================

#[test]
fn large_k_spring_converges_to_frozen_result() {
    let structure = build_distorted_methane();

    // Method 1: steepest descent with atom 1 frozen
    let (ff_frozen, mut pos_frozen) = build_ff_and_positions(&structure);
    steepest_descent_steps(&ff_frozen, &mut pos_frozen, &[1], 100, 0.1);

    // Method 2: steepest descent with atom 1 spring-restrained at very large k
    let (ff_spring, mut pos_spring) = build_ff_and_positions(&structure);
    let target = (pos_spring[3], pos_spring[4], pos_spring[5]);
    let restrained = RestrainedForceField {
        base: &ff_spring,
        restraints: vec![(1, target.0, target.1, target.2)],
        spring_constant: 10000.0,
    };
    steepest_descent_steps(&restrained, &mut pos_spring, &[], 100, 0.1);

    // Non-selected atoms (0, 2, 3, 4) should end up at approximately the same positions
    for atom in [0, 2, 3, 4] {
        let base = atom * 3;
        for c in 0..3 {
            let idx = base + c;
            let diff = (pos_frozen[idx] - pos_spring[idx]).abs();
            assert!(
                diff < 0.2,
                "atom {atom} coord {c}: frozen={:.4} spring={:.4} diff={diff:.4}",
                pos_frozen[idx],
                pos_spring[idx]
            );
        }
    }

    // Selected atom (1) in spring method should be close to its target
    // (with finite k, there's a small deviation due to force field forces)
    let dx = pos_spring[3] - target.0;
    let dy = pos_spring[4] - target.1;
    let dz = pos_spring[5] - target.2;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    assert!(
        dist < 0.1,
        "spring-restrained atom moved {dist:.4} Å from target"
    );
}

// ============================================================================
// Steepest descent with RestrainedForceField: energy decreases
// ============================================================================

#[test]
fn sd_with_spring_restraint_energy_decreases() {
    let structure = build_distorted_methane();
    let (ff, mut positions) = build_ff_and_positions(&structure);

    // Pull the distorted hydrogen back toward a more reasonable position
    let restrained = RestrainedForceField {
        base: &ff,
        restraints: vec![(1, 0.6, 0.6, 0.6)],
        spring_constant: 200.0,
    };

    let mut initial_energy = 0.0;
    let mut grad = vec![0.0; positions.len()];
    restrained.energy_and_gradients(&positions, &mut initial_energy, &mut grad);

    let final_energy = steepest_descent_steps(&restrained, &mut positions, &[], 20, 0.1);

    assert!(
        final_energy < initial_energy,
        "energy should decrease: {final_energy} >= {initial_energy}"
    );
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

// ============================================================================
// RestrainedForceField: empty restraints list
// ============================================================================

#[test]
fn restrained_ff_empty_restraints_matches_base() {
    let structure = build_methane();
    let (ff, positions) = build_ff_and_positions(&structure);

    let restrained = RestrainedForceField {
        base: &ff,
        restraints: vec![],
        spring_constant: 500.0,
    };

    let n = positions.len();
    let mut base_energy = 0.0;
    let mut base_grad = vec![0.0; n];
    ff.energy_and_gradients(&positions, &mut base_energy, &mut base_grad);

    let mut restrained_energy = 0.0;
    let mut restrained_grad = vec![0.0; n];
    restrained.energy_and_gradients(&positions, &mut restrained_energy, &mut restrained_grad);

    assert!((base_energy - restrained_energy).abs() < 1e-12);
    for i in 0..n {
        assert!((base_grad[i] - restrained_grad[i]).abs() < 1e-12);
    }
}

// ============================================================================
// RestrainedForceField: restraint energy is correct
// ============================================================================

#[test]
fn restrained_ff_energy_contribution_is_correct() {
    // Use a zero-energy base force field so we can isolate the spring contribution
    let base_ff = QuadraticFF::isotropic(6, 0.0, vec![0.0; 6]);

    let restrained = RestrainedForceField {
        base: &base_ff,
        restraints: vec![(0, 1.0, 0.0, 0.0)], // atom 0 target at (1,0,0)
        spring_constant: 100.0,
    };

    let positions = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // atom 0 at origin
    let mut energy = 0.0;
    let mut grad = vec![0.0; 6];
    restrained.energy_and_gradients(&positions, &mut energy, &mut grad);

    // E = 0.5 * 100 * (1^2 + 0 + 0) = 50.0
    assert!((energy - 50.0).abs() < 1e-10, "expected 50.0, got {energy}");

    // Gradient at coord 0: k * (0 - 1) = -100
    assert!(
        (grad[0] - (-100.0)).abs() < 1e-10,
        "expected grad[0]=-100, got {}",
        grad[0]
    );
    // Gradient at coords 1,2: 0 (no displacement in y,z)
    assert!(grad[1].abs() < 1e-10);
    assert!(grad[2].abs() < 1e-10);
    // Atom 1 gradients: 0 (not restrained, zero base)
    assert!(grad[3].abs() < 1e-10);
    assert!(grad[4].abs() < 1e-10);
    assert!(grad[5].abs() < 1e-10);
}
