// Tests for atom count limits in minimize_energy (issue #271).
//
// Verifies that minimize_energy returns an error for structures exceeding
// MAX_MINIMIZE_ATOMS, preventing UI freezes from O(N²) computation.

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::simulation::uff::VdwMode;
use rust_lib_flutter_cad::crystolecule::simulation::{MAX_MINIMIZE_ATOMS, minimize_energy};

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

#[test]
fn minimize_energy_rejects_structure_exceeding_limit_allpairs() {
    let n = MAX_MINIMIZE_ATOMS + 1;
    let mut structure = create_large_structure(n);
    assert_eq!(structure.get_num_of_atoms(), n);

    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    assert!(
        result.is_err(),
        "Expected error for {} atoms with AllPairs mode",
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
        err_msg.contains(&MAX_MINIMIZE_ATOMS.to_string()),
        "Error should mention limit {}: {}",
        MAX_MINIMIZE_ATOMS,
        err_msg
    );
}

#[test]
fn minimize_energy_rejects_structure_exceeding_limit_cutoff() {
    let n = MAX_MINIMIZE_ATOMS + 1;
    let mut structure = create_large_structure(n);

    let result = minimize_energy(&mut structure, VdwMode::Cutoff(6.0));
    assert!(
        result.is_err(),
        "Expected error for {} atoms with Cutoff mode",
        n
    );
}

#[test]
fn minimize_energy_accepts_structure_at_limit() {
    // A structure exactly at the limit should be accepted (not rejected).
    // Use a small molecule to keep test fast.
    let mut structure = AtomicStructure::new();
    let a1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let a2 = structure.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    structure.add_bond(a1, a2, 1);

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
fn minimize_energy_limit_value_is_reasonable() {
    // The limit should be between 500 and 5000 — large enough for typical use
    // but small enough to prevent UI freezes with O(N²) computation.
    assert!(
        MAX_MINIMIZE_ATOMS >= 500,
        "Limit {} is too low for practical use",
        MAX_MINIMIZE_ATOMS
    );
    assert!(
        MAX_MINIMIZE_ATOMS <= 5000,
        "Limit {} may be too high to prevent UI freezes",
        MAX_MINIMIZE_ATOMS
    );
}

#[test]
fn minimize_energy_rejects_7000_atoms() {
    // Reproduces issue #271: ~7000 atoms should be rejected immediately.
    let mut structure = create_large_structure(7000);
    assert_eq!(structure.get_num_of_atoms(), 7000);

    let result = minimize_energy(&mut structure, VdwMode::AllPairs);
    assert!(
        result.is_err(),
        "7000 atoms must be rejected to prevent UI freeze (issue #271)"
    );
}
