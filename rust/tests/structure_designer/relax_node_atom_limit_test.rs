// Tests for relax node atom count limit (issue #271).
//
// The relax node delegates to minimize_energy(), which enforces MAX_MINIMIZE_ATOMS.
// The relax node also has its own MAX_RELAX_ATOMS guard for early rejection before
// any expensive computation.

use rust_lib_flutter_cad::crystolecule::simulation::MAX_MINIMIZE_ATOMS;
use rust_lib_flutter_cad::structure_designer::nodes::relax::MAX_RELAX_ATOMS;

#[test]
fn relax_node_max_atoms_constant_is_reasonable() {
    // The limit should be between 500 and 5000 — large enough for typical use
    // but small enough to prevent UI freezes with O(N²) computation.
    assert!(
        MAX_RELAX_ATOMS >= 500,
        "MAX_RELAX_ATOMS {} is too low for practical use",
        MAX_RELAX_ATOMS
    );
    assert!(
        MAX_RELAX_ATOMS <= 5000,
        "MAX_RELAX_ATOMS {} may be too high to prevent UI freezes",
        MAX_RELAX_ATOMS
    );
}

#[test]
fn relax_node_limit_matches_minimize_energy_limit() {
    // Both guards should use the same limit value to ensure consistent behavior.
    assert_eq!(
        MAX_RELAX_ATOMS, MAX_MINIMIZE_ATOMS,
        "Relax node limit ({}) should match minimize_energy limit ({})",
        MAX_RELAX_ATOMS, MAX_MINIMIZE_ATOMS
    );
}
