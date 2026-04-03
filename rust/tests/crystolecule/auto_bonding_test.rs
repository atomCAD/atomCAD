use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure_utils::{
    auto_create_bonds, auto_create_bonds_with_tolerance,
};
use glam::f64::DVec3;

/// Helper: create a simple two-carbon structure at a given distance.
fn two_carbons(distance: f64) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO); // Carbon
    structure.add_atom(6, DVec3::new(distance, 0.0, 0.0));
    structure
}

#[test]
fn auto_create_bonds_default_still_works() {
    // C-C covalent radii sum ≈ 0.77 + 0.77 = 1.54 Å
    // Default multiplier 1.15 → max bond distance ≈ 1.771 Å
    let mut structure = two_carbons(1.54);
    auto_create_bonds(&mut structure);
    assert_eq!(structure.get_num_of_bonds(), 1);
}

#[test]
fn auto_create_bonds_default_no_bond_beyond_threshold() {
    // Distance well beyond 1.15x covalent sum
    let mut structure = two_carbons(2.0);
    auto_create_bonds(&mut structure);
    assert_eq!(structure.get_num_of_bonds(), 0);
}

#[test]
fn higher_tolerance_creates_more_bonds() {
    // Place two carbons at 1.8 Å — just beyond default 1.771 Å threshold
    let mut structure = two_carbons(1.8);
    auto_create_bonds_with_tolerance(&mut structure, 1.15);
    assert_eq!(structure.get_num_of_bonds(), 0, "default tolerance should NOT bond at 1.8 Å");

    let mut structure = two_carbons(1.8);
    auto_create_bonds_with_tolerance(&mut structure, 1.20);
    // 1.54 * 1.20 = 1.848 → 1.8 < 1.848, bond should form
    assert_eq!(structure.get_num_of_bonds(), 1, "higher tolerance should bond at 1.8 Å");
}

#[test]
fn lower_tolerance_creates_fewer_bonds() {
    // Place two carbons at 1.5 Å — within default but outside 0.95x
    let mut structure = two_carbons(1.5);
    auto_create_bonds_with_tolerance(&mut structure, 1.15);
    assert_eq!(structure.get_num_of_bonds(), 1, "default tolerance should bond at 1.5 Å");

    let mut structure = two_carbons(1.5);
    auto_create_bonds_with_tolerance(&mut structure, 0.95);
    // 1.54 * 0.95 = 1.463 → 1.5 > 1.463, bond should NOT form
    assert_eq!(structure.get_num_of_bonds(), 0, "lower tolerance should NOT bond at 1.5 Å");
}

#[test]
fn tolerance_with_mixed_elements() {
    // Carbon (r=0.77) and Oxygen (r=0.73) — sum = 1.50 Å
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO); // Carbon
    structure.add_atom(8, DVec3::new(1.6, 0.0, 0.0)); // Oxygen

    // Default: 1.50 * 1.15 = 1.725 → 1.6 < 1.725, should bond
    auto_create_bonds_with_tolerance(&mut structure, 1.15);
    assert_eq!(structure.get_num_of_bonds(), 1);

    // With lower tolerance: 1.50 * 1.05 = 1.575 → 1.6 > 1.575, should NOT bond
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);
    structure.add_atom(8, DVec3::new(1.6, 0.0, 0.0));
    auto_create_bonds_with_tolerance(&mut structure, 1.05);
    assert_eq!(structure.get_num_of_bonds(), 0);
}

#[test]
fn wrapper_matches_explicit_default() {
    // auto_create_bonds() should produce identical results to
    // auto_create_bonds_with_tolerance(_, 1.15)
    let mut s1 = two_carbons(1.54);
    auto_create_bonds(&mut s1);

    let mut s2 = two_carbons(1.54);
    auto_create_bonds_with_tolerance(&mut s2, 1.15);

    assert_eq!(s1.get_num_of_bonds(), s2.get_num_of_bonds());
}
