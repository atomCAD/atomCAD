// Tests for atom count limits in minimize_energy (issue #271, extended by
// doc/design_relax_frozen_atoms.md).
//
// The limits are frozen-aware: MAX_MINIMIZE_FREE_ATOMS caps *unfrozen* atoms
// (they drive per-iteration cost), MAX_MINIMIZE_TOTAL_ATOMS caps total atoms
// (O(N) setup and transient interaction-list memory), and AllPairs mode is
// rejected above the free-atom limit because its O(N²) nonbonded enumeration
// is not frozen-aware.

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::simulation::uff::VdwMode;
use rust_lib_flutter_cad::crystolecule::simulation::{
    MAX_MINIMIZE_FREE_ATOMS, MAX_MINIMIZE_TOTAL_ATOMS, check_minimize_limits, minimize_energy,
};

/// Creates an atomic structure with `n` disconnected carbon atoms on a grid.
fn create_large_structure(n: usize) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    let spacing = 2.0; // Angstroms apart to avoid auto-bonding
    let side = (n as f64).cbrt().ceil() as usize;
    let mut count = 0;
    for ix in 0..side {
        for iy in 0..side {
            for iz in 0..side {
                if count >= n {
                    return structure;
                }
                let pos = DVec3::new(
                    ix as f64 * spacing,
                    iy as f64 * spacing,
                    iz as f64 * spacing,
                );
                structure.add_atom(6, pos); // Carbon
                count += 1;
            }
        }
    }
    structure
}

/// A frozen bulk of `n_frozen` grid carbons plus one free, stretched, bonded
/// C–C pair placed well outside the bulk's 6 Å vdW shell.
fn create_frozen_bulk_with_free_pair(n_frozen: usize) -> (AtomicStructure, u32, u32) {
    let mut structure = create_large_structure(n_frozen);
    let frozen_ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    for id in frozen_ids {
        structure.set_atom_frozen(id, true);
    }
    // Free pair, stretched past the ~1.54 Å C-C equilibrium, far from the bulk.
    let a1 = structure.add_atom(6, DVec3::new(-30.0, 0.0, 0.0));
    let a2 = structure.add_atom(6, DVec3::new(-28.0, 0.0, 0.0));
    structure.add_bond(a1, a2, BOND_SINGLE);
    (structure, a1, a2)
}

// ============================================================================
// Free-atom limit
// ============================================================================

#[test]
fn minimize_energy_rejects_too_many_free_atoms_allpairs() {
    let n = MAX_MINIMIZE_FREE_ATOMS + 1;
    let mut structure = create_large_structure(n);
    assert_eq!(structure.get_num_of_atoms(), n);

    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    assert!(
        result.is_err(),
        "Expected error for {} free atoms with AllPairs mode",
        n
    );

    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains(&n.to_string()),
        "Error should mention atom count {}: {}",
        n,
        err_msg
    );
    assert!(
        err_msg.contains(&MAX_MINIMIZE_FREE_ATOMS.to_string()),
        "Error should mention limit {}: {}",
        MAX_MINIMIZE_FREE_ATOMS,
        err_msg
    );
    assert!(
        err_msg.contains("Freeze"),
        "Free-atom error should suggest freezing: {}",
        err_msg
    );
}

#[test]
fn minimize_energy_rejects_too_many_free_atoms_cutoff() {
    let n = MAX_MINIMIZE_FREE_ATOMS + 1;
    let mut structure = create_large_structure(n);

    let result = minimize_energy(&mut structure, VdwMode::Cutoff(6.0));
    assert!(
        result.is_err(),
        "Expected error for {} free atoms with Cutoff mode",
        n
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("unfrozen"),
        "Error should be the free-atom message: {}",
        err_msg
    );
}

// ============================================================================
// Frozen bulk over the old total limit
// ============================================================================

/// The reported scenario: a large frozen bulk (over the old 2000 total-atom
/// limit) with a small free region minimizes successfully in Cutoff mode.
/// Frozen atoms must not move; the stretched free pair must relax inward.
#[test]
fn minimize_energy_accepts_large_frozen_bulk_with_cutoff() {
    let n_frozen = MAX_MINIMIZE_FREE_ATOMS + 500;
    let (mut structure, a1, a2) = create_frozen_bulk_with_free_pair(n_frozen);
    assert_eq!(structure.get_num_of_atoms(), n_frozen + 2);

    let frozen_positions: Vec<(u32, DVec3)> = structure
        .iter_atoms()
        .filter(|(_, a)| a.is_frozen())
        .map(|(id, a)| (*id, a.position))
        .collect();
    assert_eq!(frozen_positions.len(), n_frozen);

    let result = minimize_energy(&mut structure, VdwMode::Cutoff(6.0));
    let res = result.expect("frozen bulk over the old limit must minimize");
    assert!(
        res.converged,
        "minimization should converge: {}",
        res.message
    );

    for (id, original) in frozen_positions {
        let atom = structure.get_atom(id).expect("frozen atom still present");
        assert_eq!(atom.position, original, "frozen atom {} must not move", id);
    }

    let x1 = structure.get_atom(a1).unwrap().position.x;
    let x2 = structure.get_atom(a2).unwrap().position.x;
    let dist = (x2 - x1).abs();
    assert!(
        (dist - 2.0).abs() > 1e-3 && dist < 2.0,
        "stretched free pair should relax inward from 2.0 Å (got {} Å)",
        dist
    );
}

/// The same structure in AllPairs mode is rejected with the error that points
/// at the vdW cutoff preference.
#[test]
fn minimize_energy_rejects_large_frozen_bulk_with_allpairs() {
    let n_frozen = MAX_MINIMIZE_FREE_ATOMS + 500;
    let (mut structure, _, _) = create_frozen_bulk_with_free_pair(n_frozen);

    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    let err_msg = result.expect_err("AllPairs above the limit must be rejected");
    assert!(
        err_msg.contains("van der Waals distance cutoff"),
        "Error should name the cutoff: {}",
        err_msg
    );
    assert!(
        err_msg.contains("Use vdW distance cutoff for energy minimization"),
        "Error should name the preference checkbox: {}",
        err_msg
    );
}

// ============================================================================
// check_minimize_limits — exhaustive unit tests (incl. the 500k total cap,
// which is impractical to exercise with real atoms)
// ============================================================================

#[test]
fn check_limits_accepts_small_structure_all_modes() {
    assert!(check_minimize_limits(100, 100, &VdwMode::AllPairs).is_ok());
    assert!(check_minimize_limits(100, 100, &VdwMode::Cutoff(6.0)).is_ok());
}

#[test]
fn check_limits_accepts_exactly_at_free_limit() {
    let n = MAX_MINIMIZE_FREE_ATOMS;
    assert!(check_minimize_limits(n, n, &VdwMode::AllPairs).is_ok());
    assert!(check_minimize_limits(n, n, &VdwMode::Cutoff(6.0)).is_ok());
}

#[test]
fn check_limits_rejects_free_atoms_over_limit_in_both_modes() {
    let n = MAX_MINIMIZE_FREE_ATOMS + 1;
    for mode in [VdwMode::AllPairs, VdwMode::Cutoff(6.0)] {
        let err = check_minimize_limits(n, n, &mode).unwrap_err();
        assert!(
            err.contains("unfrozen"),
            "expected free-atom error: {}",
            err
        );
    }
}

#[test]
fn check_limits_mostly_frozen_ok_with_cutoff_rejected_with_allpairs() {
    // 175k total / 500 free — the reported use case.
    let total = 175_400;
    let free = 500;
    assert!(check_minimize_limits(total, free, &VdwMode::Cutoff(6.0)).is_ok());

    let err = check_minimize_limits(total, free, &VdwMode::AllPairs).unwrap_err();
    assert!(
        err.contains("van der Waals distance cutoff"),
        "expected AllPairs guard error: {}",
        err
    );
}

#[test]
fn check_limits_allpairs_allowed_at_total_equal_free_limit() {
    // AllPairs guard is on *total* atoms and only fires strictly above the limit.
    let n = MAX_MINIMIZE_FREE_ATOMS;
    assert!(check_minimize_limits(n, 10, &VdwMode::AllPairs).is_ok());
    let err = check_minimize_limits(n + 1, 10, &VdwMode::AllPairs).unwrap_err();
    assert!(
        err.contains("van der Waals"),
        "expected AllPairs guard: {}",
        err
    );
}

#[test]
fn check_limits_rejects_total_over_cap_in_both_modes() {
    let n = MAX_MINIMIZE_TOTAL_ATOMS + 1;
    for mode in [VdwMode::AllPairs, VdwMode::Cutoff(6.0)] {
        let err = check_minimize_limits(n, 0, &mode).unwrap_err();
        assert!(
            err.contains("total minimization limit"),
            "expected total-cap error: {}",
            err
        );
        assert!(
            err.contains(&MAX_MINIMIZE_TOTAL_ATOMS.to_string()),
            "total-cap error should mention the cap: {}",
            err
        );
    }
}

#[test]
fn check_limits_accepts_exactly_at_total_cap_with_cutoff() {
    assert!(check_minimize_limits(MAX_MINIMIZE_TOTAL_ATOMS, 100, &VdwMode::Cutoff(6.0)).is_ok());
}

#[test]
fn check_limits_total_cap_takes_precedence_over_free_limit() {
    // Both violated → the total-cap message (hard structural cap first).
    let n = MAX_MINIMIZE_TOTAL_ATOMS + 1;
    let err = check_minimize_limits(n, n, &VdwMode::Cutoff(6.0)).unwrap_err();
    assert!(
        err.contains("total minimization limit"),
        "total cap should fire first: {}",
        err
    );
}

#[test]
fn check_limits_free_limit_takes_precedence_over_allpairs_guard() {
    // Both violated → the free-atom message (actionable "freeze more" advice
    // before the mode advice).
    let n = MAX_MINIMIZE_FREE_ATOMS + 1;
    let err = check_minimize_limits(n, n, &VdwMode::AllPairs).unwrap_err();
    assert!(
        err.contains("unfrozen"),
        "free-atom check should fire before the AllPairs guard: {}",
        err
    );
}

// ============================================================================
// Small-structure and edge-case behavior (unchanged from the old limit)
// ============================================================================

#[test]
fn minimize_energy_accepts_small_structure() {
    let mut structure = AtomicStructure::new();
    let a1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let a2 = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure.add_bond(a1, a2, BOND_SINGLE);

    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    assert!(
        result.is_ok(),
        "Small structure should be accepted: {:?}",
        result.err()
    );
}

#[test]
fn minimize_energy_accepts_empty_structure() {
    let mut structure = AtomicStructure::new();
    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    assert!(result.is_ok());
    let res = result.unwrap();
    assert_eq!(res.iterations, 0);
    assert!(res.converged);
}

#[test]
fn minimize_energy_limit_values_are_reasonable() {
    // The free-atom limit should be between 500 and 5000 — large enough for
    // typical use but small enough to prevent UI freezes.
    assert!(
        MAX_MINIMIZE_FREE_ATOMS >= 500,
        "Limit {} is too low for practical use",
        MAX_MINIMIZE_FREE_ATOMS
    );
    assert!(
        MAX_MINIMIZE_FREE_ATOMS <= 5000,
        "Limit {} may be too high to prevent UI freezes",
        MAX_MINIMIZE_FREE_ATOMS
    );
    assert!(
        MAX_MINIMIZE_TOTAL_ATOMS > MAX_MINIMIZE_FREE_ATOMS,
        "Total cap must exceed the free-atom limit"
    );
}

#[test]
fn minimize_energy_rejects_7000_free_atoms() {
    // Reproduces issue #271: ~7000 unfrozen atoms should be rejected immediately.
    let mut structure = create_large_structure(7000);
    assert_eq!(structure.get_num_of_atoms(), 7000);

    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    assert!(
        result.is_err(),
        "7000 free atoms must be rejected to prevent UI freeze (issue #271)"
    );
}
