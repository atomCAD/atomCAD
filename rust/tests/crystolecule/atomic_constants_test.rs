//! Tests for the passivation helpers in `atomic_constants`
//! (`doc/design_halogen_passivation.md` Phase 1).

use rust_lib_flutter_cad::crystolecule::atomic_constants::{
    ALLOWED_PASSIVANTS, halogen_bond_length, is_allowed_passivant,
};

const EPS: f64 = 1e-9;

#[test]
fn allowed_passivants_is_h_and_the_four_halogens() {
    assert_eq!(ALLOWED_PASSIVANTS, [1, 9, 17, 35, 53]);
    for z in [1, 9, 17, 35, 53] {
        assert!(is_allowed_passivant(z), "{z} should be allowed");
    }
    // A non-passivant (oxygen) and a non-monovalent halogen-lookalike are rejected.
    assert!(!is_allowed_passivant(8), "oxygen must be rejected");
    assert!(!is_allowed_passivant(6), "carbon must be rejected");
    assert!(!is_allowed_passivant(0), "unknown must be rejected");
}

#[test]
fn halogen_bond_length_table_hits() {
    // Representative entries from the D2 table.
    assert!((halogen_bond_length(6, 9) - 1.35).abs() < EPS, "C–F"); // Carbon–Fluorine
    assert!((halogen_bond_length(6, 17) - 1.77).abs() < EPS, "C–Cl");
    assert!((halogen_bond_length(14, 17) - 2.02).abs() < EPS, "Si–Cl");
    assert!((halogen_bond_length(32, 53) - 2.51).abs() < EPS, "Ge–I");
    assert!((halogen_bond_length(5, 9) - 1.31).abs() < EPS, "B–F");
}

#[test]
fn halogen_bond_length_dash_cell_falls_back_to_radii_sum() {
    // N–Br is a `—` cell (no tabulated value) → covalent radii sum.
    // N covalent radius 0.71 + Br covalent radius 1.20 = 1.91.
    assert!(
        (halogen_bond_length(7, 35) - 1.91).abs() < EPS,
        "N–Br must fall back to the covalent-radii sum (1.91), got {}",
        halogen_bond_length(7, 35)
    );
}

#[test]
fn halogen_bond_length_unknown_host_falls_back_to_radii_sum() {
    // Gold (79, covalent radius 1.36) is not a tabulated host → radii sum with F (0.57).
    assert!(
        (halogen_bond_length(79, 9) - (1.36 + 0.57)).abs() < EPS,
        "unknown host must fall back to the radii sum"
    );
}
