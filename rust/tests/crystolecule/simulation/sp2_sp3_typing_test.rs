// Tests for GitHub issue #228: sp2 structures being minimized as sp3.
//
// Validates that the UFF typer correctly identifies sp2 nitrogen (and carbon)
// in conjugated systems where the atom's own bonds are all single but
// neighbor context indicates sp2 hybridization.
//
// Code paths tested:
// 1. Atom type assignment (typer.rs): amide N, pyrrole-type N, enamine N
// 2. Topology inversion enumeration: sp2 centers get inversion terms
// 3. Angle bend parameters: sp2 centers use trigonal order (θ₀=120°)
// 4. End-to-end minimization: sp2 geometry preserved after minimize

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, InlineBond,
};
use rust_lib_flutter_cad::crystolecule::simulation::minimize::{
    MinimizationConfig, minimize_with_force_field,
};
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;
use rust_lib_flutter_cad::crystolecule::simulation::uff::typer::{
    assign_uff_type, assign_uff_types, hybridization_from_label,
};
use rust_lib_flutter_cad::crystolecule::simulation::uff::UffForceField;

// ============================================================================
// Helpers
// ============================================================================

fn single_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_SINGLE)
}

fn double_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_DOUBLE)
}

fn aromatic_bond(to: u32) -> InlineBond {
    InlineBond::new(to, BOND_AROMATIC)
}

fn build_structure(
    atoms: &[(i16, [f64; 3])],
    bonds: &[(usize, usize, u8)],
) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    for &(z, pos) in atoms {
        structure.add_atom(z, DVec3::new(pos[0], pos[1], pos[2]));
    }
    for &(a, b, order) in bonds {
        structure.add_bond((a + 1) as u32, (b + 1) as u32, order);
    }
    structure
}

/// Measures planarity of 4 atoms: returns the maximum absolute distance
/// of any atom from the best-fit plane through the first 3 atoms.
fn max_out_of_plane_distance(positions: &[f64], indices: &[usize]) -> f64 {
    assert!(indices.len() >= 3);
    let p = |i: usize| -> DVec3 {
        DVec3::new(
            positions[i * 3],
            positions[i * 3 + 1],
            positions[i * 3 + 2],
        )
    };
    let a = p(indices[0]);
    let b = p(indices[1]);
    let c = p(indices[2]);
    let normal = (b - a).cross(c - a).normalize();
    let mut max_dist = 0.0f64;
    for &idx in indices {
        let d = (p(idx) - a).dot(normal).abs();
        max_dist = max_dist.max(d);
    }
    max_dist
}

// ============================================================================
// Path 1: Typer — amide nitrogen (N-C(=O)) should be sp2
// ============================================================================

#[test]
fn test_sp2_sp3_amide_nitrogen_typing() {
    // Formamide: H2N-C(=O)-H
    // Atom 0: N (3 single bonds to C, H, H)
    // Atom 1: C (1 double bond to O, 1 single to N, 1 single to H)
    // Atom 2: O (1 double bond to C)
    // Atom 3: H (bonded to N)
    // Atom 4: H (bonded to N)
    // Atom 5: H (bonded to C)
    //
    // N has 3 single bonds → naive typer assigns N_3 (sp3)
    // But N is adjacent to C_2 (C=O carbon) → should be N_2 (sp2, amide)
    let atomic_numbers: Vec<i16> = vec![7, 6, 8, 1, 1, 1];
    let bonds_n: Vec<InlineBond> = vec![single_bond(1), single_bond(3), single_bond(4)];
    let bonds_c: Vec<InlineBond> = vec![single_bond(0), double_bond(2), single_bond(5)];
    let bonds_o: Vec<InlineBond> = vec![double_bond(1)];
    let bonds_h3: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h4: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h5: Vec<InlineBond> = vec![single_bond(1)];

    let bond_lists: Vec<&[InlineBond]> = vec![
        &bonds_n, &bonds_c, &bonds_o, &bonds_h3, &bonds_h4, &bonds_h5,
    ];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();

    // Carbon with C=O should be C_2
    assert_eq!(result.labels[1], "C_2", "carbonyl carbon should be C_2");
    // Oxygen with C=O should be O_2
    assert_eq!(result.labels[2], "O_2", "carbonyl oxygen should be O_2");
    // Nitrogen adjacent to C_2 should be N_2 (sp2, amide), NOT N_3
    assert_eq!(
        result.labels[0], "N_2",
        "amide nitrogen should be N_2 (sp2), not N_3 (sp3)"
    );
}

#[test]
fn test_sp2_sp3_nitrogen_adjacent_to_aromatic_carbon() {
    // N with 3 single bonds, one neighbor is aromatic carbon (C_R)
    // This models pyrrole-type nitrogen or aniline
    // Atom 0: N (single bonds to atoms 1, 4, 5)
    // Atom 1: C (aromatic bonds to 2, 6; single bond to 0)
    // Atom 2: C (aromatic bonds to 1, 3)
    // Atom 3: C (aromatic bonds to 2, 6; single to 7)
    // Atoms 4,5: H (bonded to N)
    // Atom 6: C (aromatic bonds to 1, 3)
    // Atom 7: H
    let atomic_numbers: Vec<i16> = vec![7, 6, 6, 6, 1, 1, 6, 1];
    let bonds_n: Vec<InlineBond> = vec![single_bond(1), single_bond(4), single_bond(5)];
    let bonds_c1: Vec<InlineBond> = vec![single_bond(0), aromatic_bond(2), aromatic_bond(6)];
    let bonds_c2: Vec<InlineBond> = vec![aromatic_bond(1), aromatic_bond(3)];
    let bonds_c3: Vec<InlineBond> = vec![aromatic_bond(2), aromatic_bond(6), single_bond(7)];
    let bonds_h4: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h5: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_c6: Vec<InlineBond> = vec![aromatic_bond(1), aromatic_bond(3)];
    let bonds_h7: Vec<InlineBond> = vec![single_bond(3)];

    let bond_lists: Vec<&[InlineBond]> = vec![
        &bonds_n, &bonds_c1, &bonds_c2, &bonds_c3, &bonds_h4, &bonds_h5, &bonds_c6, &bonds_h7,
    ];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();

    // Neighbor C should be C_R (aromatic)
    assert_eq!(result.labels[1], "C_R", "aromatic carbon should be C_R");
    // N adjacent to C_R should be N_2 (sp2), not N_3
    assert_eq!(
        result.labels[0], "N_2",
        "nitrogen adjacent to aromatic carbon should be N_2 (sp2)"
    );
}

#[test]
fn test_sp2_sp3_nitrogen_adjacent_to_double_bond_carbon() {
    // Enamine: N-C=C pattern
    // Atom 0: N (single bonds to C1, H, H)
    // Atom 1: C (single bond to N, double bond to C2, single to H)
    // Atom 2: C (double bond to C1, single to H, single to H)
    let atomic_numbers: Vec<i16> = vec![7, 6, 6, 1, 1, 1, 1, 1];
    let bonds_n: Vec<InlineBond> = vec![single_bond(1), single_bond(3), single_bond(4)];
    let bonds_c1: Vec<InlineBond> = vec![single_bond(0), double_bond(2), single_bond(5)];
    let bonds_c2: Vec<InlineBond> = vec![double_bond(1), single_bond(6), single_bond(7)];
    let bonds_h3: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h4: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h5: Vec<InlineBond> = vec![single_bond(1)];
    let bonds_h6: Vec<InlineBond> = vec![single_bond(2)];
    let bonds_h7: Vec<InlineBond> = vec![single_bond(2)];

    let bond_lists: Vec<&[InlineBond]> = vec![
        &bonds_n, &bonds_c1, &bonds_c2, &bonds_h3, &bonds_h4, &bonds_h5, &bonds_h6, &bonds_h7,
    ];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();

    assert_eq!(result.labels[1], "C_2", "vinyl carbon should be C_2");
    assert_eq!(
        result.labels[0], "N_2",
        "enamine nitrogen should be N_2 (sp2)"
    );
}

// ============================================================================
// Path 2: Topology — amide N should have inversion terms
// ============================================================================

#[test]
fn test_sp2_sp3_amide_nitrogen_inversions() {
    // Formamide with planar geometry: N should get inversion terms
    // since it's sp2 (amide).
    let structure = build_structure(
        &[
            (7, [0.0, 0.0, 0.0]),    // N
            (6, [1.35, 0.0, 0.0]),   // C
            (8, [2.0, 1.1, 0.0]),    // O (C=O)
            (1, [-0.5, 0.87, 0.0]),  // H1 on N
            (1, [-0.5, -0.87, 0.0]), // H2 on N
            (1, [1.8, -0.9, 0.0]),   // H on C
        ],
        &[
            (0, 1, BOND_SINGLE), // N-C
            (1, 2, BOND_DOUBLE), // C=O
            (0, 3, BOND_SINGLE), // N-H
            (0, 4, BOND_SINGLE), // N-H
            (1, 5, BOND_SINGLE), // C-H
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    // C (idx 1) has a double bond → sp2 → 3 inversions (it has 3 bonds)
    let c_inversions: Vec<_> = topo.inversions.iter().filter(|inv| inv.idx2 == 1).collect();
    assert_eq!(c_inversions.len(), 3, "carbonyl C should have 3 inversions");

    // N (idx 0) has 3 single bonds but is adjacent to C_2 → should also be sp2
    // → should have 3 inversions
    let n_inversions: Vec<_> = topo.inversions.iter().filter(|inv| inv.idx2 == 0).collect();
    assert_eq!(
        n_inversions.len(),
        3,
        "amide nitrogen should have 3 inversions (sp2)"
    );
}

// ============================================================================
// Path 3: Force field angle params — amide N should use trigonal order
// ============================================================================

#[test]
fn test_sp2_sp3_amide_nitrogen_angle_order() {
    // Build formamide and check that N angles use order=3 (trigonal, 120°)
    let structure = build_structure(
        &[
            (7, [0.0, 0.0, 0.0]),    // N
            (6, [1.35, 0.0, 0.0]),   // C
            (8, [2.0, 1.1, 0.0]),    // O
            (1, [-0.5, 0.87, 0.0]),  // H1
            (1, [-0.5, -0.87, 0.0]), // H2
            (1, [1.8, -0.9, 0.0]),   // H3
        ],
        &[
            (0, 1, BOND_SINGLE),
            (1, 2, BOND_DOUBLE),
            (0, 3, BOND_SINGLE),
            (0, 4, BOND_SINGLE),
            (1, 5, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology(&topo).unwrap();

    // Check angles centered on N (topology index 0)
    let n_angles: Vec<_> = ff.angle_params.iter().filter(|a| a.idx2 == 0).collect();
    assert!(!n_angles.is_empty(), "N should have angle parameters");

    for angle in &n_angles {
        assert_eq!(
            angle.order, 3,
            "amide nitrogen angles should use trigonal order (3), not general (0)"
        );
    }
}

// ============================================================================
// Path 4: End-to-end minimization — sp2 geometry preserved
// ============================================================================

#[test]
fn test_sp2_sp3_formamide_minimization_preserves_planarity() {
    // Build formamide with all atoms in a plane (sp2 geometry).
    // After minimization, the nitrogen and its neighbors should remain planar.
    let structure = build_structure(
        &[
            (7, [0.0, 0.0, 0.0]),     // N (idx 0)
            (6, [1.35, 0.0, 0.0]),    // C (idx 1)
            (8, [2.03, 1.17, 0.0]),   // O (idx 2)
            (1, [-0.47, 0.88, 0.0]),  // H1 (idx 3)
            (1, [-0.47, -0.88, 0.0]), // H2 (idx 4)
            (1, [1.82, -0.94, 0.0]),  // H3 (idx 5)
        ],
        &[
            (0, 1, BOND_SINGLE), // N-C
            (1, 2, BOND_DOUBLE), // C=O
            (0, 3, BOND_SINGLE), // N-H
            (0, 4, BOND_SINGLE), // N-H
            (1, 5, BOND_SINGLE), // C-H
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology(&topo).unwrap();
    let mut positions = topo.positions.clone();
    let config = MinimizationConfig::default();
    let _result = minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // Check that N and its neighbors remain approximately planar.
    // Indices: N=0, C=1, H1=3, H2=4
    let oop = max_out_of_plane_distance(&positions, &[0, 1, 3, 4]);
    assert!(
        oop < 0.15,
        "amide nitrogen should remain planar after minimization (out-of-plane: {:.4} Å)",
        oop
    );
}

#[test]
fn test_sp2_sp3_initially_nonplanar_amide_minimizes_to_planar() {
    // Build formamide with N deliberately pushed out of plane (sp3-like).
    // After minimization with correct sp2 typing, it should flatten.
    let structure = build_structure(
        &[
            (7, [0.0, 0.0, 0.5]),     // N pushed 0.5 Å out of plane
            (6, [1.35, 0.0, 0.0]),    // C
            (8, [2.03, 1.17, 0.0]),   // O
            (1, [-0.47, 0.88, 0.3]),  // H1 also out of plane
            (1, [-0.47, -0.88, 0.3]), // H2 also out of plane
            (1, [1.82, -0.94, 0.0]),  // H3
        ],
        &[
            (0, 1, BOND_SINGLE), // N-C
            (1, 2, BOND_DOUBLE), // C=O
            (0, 3, BOND_SINGLE), // N-H
            (0, 4, BOND_SINGLE), // N-H
            (1, 5, BOND_SINGLE), // C-H
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    let ff = UffForceField::from_topology(&topo).unwrap();
    let mut positions = topo.positions.clone();
    let config = MinimizationConfig::default();
    let _result = minimize_with_force_field(&ff, &mut positions, &config, &[]);

    // After minimization, the amide should be approximately planar
    let oop = max_out_of_plane_distance(&positions, &[0, 1, 3, 4]);
    assert!(
        oop < 0.2,
        "non-planar amide should minimize toward planarity (out-of-plane: {:.4} Å)",
        oop
    );
}

// ============================================================================
// Path 5: Carbon typing — C adjacent to aromatic should stay sp2 (regression)
// ============================================================================

#[test]
fn test_sp2_sp3_carbon_with_double_bond_stays_c2() {
    // Verify that carbon WITH an explicit double bond is correctly typed C_2.
    // This is a regression test — should always pass.
    let bonds = [double_bond(1), single_bond(2), single_bond(3)];
    assert_eq!(assign_uff_type(6, &bonds).unwrap(), "C_2");
}

#[test]
fn test_sp2_sp3_nitrogen_with_double_bond_stays_n2() {
    // N with explicit double bond → N_2. Regression test.
    let bonds = [double_bond(1), single_bond(2)];
    assert_eq!(assign_uff_type(7, &bonds).unwrap(), "N_2");
}

#[test]
fn test_sp2_sp3_nitrogen_with_aromatic_bonds_stays_nr() {
    // N with aromatic bonds → N_R. Regression test.
    let bonds = [aromatic_bond(1), aromatic_bond(2)];
    assert_eq!(assign_uff_type(7, &bonds).unwrap(), "N_R");
}

// ============================================================================
// Path 6: Pure sp3 nitrogen should NOT be affected by fix
// ============================================================================

#[test]
fn test_sp2_sp3_ammonia_stays_sp3() {
    // NH3: N with 3 single bonds to H only → should remain N_3
    // No sp2 neighbors.
    let atomic_numbers: Vec<i16> = vec![7, 1, 1, 1];
    let bonds_n: Vec<InlineBond> = vec![single_bond(1), single_bond(2), single_bond(3)];
    let bonds_h1: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h2: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h3: Vec<InlineBond> = vec![single_bond(0)];

    let bond_lists: Vec<&[InlineBond]> = vec![&bonds_n, &bonds_h1, &bonds_h2, &bonds_h3];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();
    assert_eq!(
        result.labels[0], "N_3",
        "ammonia nitrogen should remain N_3 (sp3)"
    );
}

#[test]
fn test_sp2_sp3_trimethylamine_stays_sp3() {
    // N(CH3)3: N with 3 single bonds to sp3 carbons → N_3
    let atomic_numbers: Vec<i16> = vec![7, 6, 6, 6];
    let bonds_n: Vec<InlineBond> = vec![single_bond(1), single_bond(2), single_bond(3)];
    let bonds_c1: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_c2: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_c3: Vec<InlineBond> = vec![single_bond(0)];

    let bond_lists: Vec<&[InlineBond]> = vec![&bonds_n, &bonds_c1, &bonds_c2, &bonds_c3];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();
    assert_eq!(
        result.labels[0], "N_3",
        "trimethylamine N with all sp3 neighbors should remain N_3"
    );
}

// ============================================================================
// Path 7: Caffeine-like structure (the original issue scenario)
// ============================================================================

#[test]
fn test_sp2_sp3_caffeine_like_nitrogen_typing() {
    // Simplified caffeine fragment: N in a ring adjacent to C=O
    // This models the exact issue scenario: N assembled with single bonds
    // but adjacent to carbonyl carbons.
    //
    //     O
    //     ‖
    // H₃C-N—C—N-CH₃
    //       ‖
    //       O
    //
    // We model just the N-C(=O)-N fragment:
    // Atom 0: N (single bonds to C1, C3, H)  → should be N_2 (amide)
    // Atom 1: C (single bond to N0, double bond to O2, single to N4)
    // Atom 2: O (double bond to C1)
    // Atom 3: C (single bond to N0) — methyl
    // Atom 4: N (single bonds to C1, C5, H)  → should be N_2 (amide)
    // Atom 5: C (single bond to N4) — methyl
    // Atom 6: H (bonded to N0)
    // Atom 7: H (bonded to N4)
    let atomic_numbers: Vec<i16> = vec![7, 6, 8, 6, 7, 6, 1, 1];
    let bonds_n0: Vec<InlineBond> = vec![single_bond(1), single_bond(3), single_bond(6)];
    let bonds_c1: Vec<InlineBond> = vec![single_bond(0), double_bond(2), single_bond(4)];
    let bonds_o2: Vec<InlineBond> = vec![double_bond(1)];
    let bonds_c3: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_n4: Vec<InlineBond> = vec![single_bond(1), single_bond(5), single_bond(7)];
    let bonds_c5: Vec<InlineBond> = vec![single_bond(4)];
    let bonds_h6: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h7: Vec<InlineBond> = vec![single_bond(4)];

    let bond_lists: Vec<&[InlineBond]> = vec![
        &bonds_n0, &bonds_c1, &bonds_o2, &bonds_c3,
        &bonds_n4, &bonds_c5, &bonds_h6, &bonds_h7,
    ];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();

    assert_eq!(result.labels[1], "C_2", "carbonyl C should be C_2");
    assert_eq!(
        result.labels[0], "N_2",
        "caffeine-like N0 (amide) should be N_2, not N_3"
    );
    assert_eq!(
        result.labels[4], "N_2",
        "caffeine-like N4 (amide) should be N_2, not N_3"
    );
}

// ============================================================================
// Path 8: Urea — both nitrogens are amide sp2
// ============================================================================

#[test]
fn test_sp2_sp3_urea_nitrogen_typing() {
    // Urea: H2N-C(=O)-NH2
    // Both nitrogens have 3 single bonds but are adjacent to C_2 → both N_2
    let atomic_numbers: Vec<i16> = vec![7, 6, 8, 7, 1, 1, 1, 1];
    let bonds_n0: Vec<InlineBond> = vec![single_bond(1), single_bond(4), single_bond(5)];
    let bonds_c1: Vec<InlineBond> = vec![single_bond(0), double_bond(2), single_bond(3)];
    let bonds_o2: Vec<InlineBond> = vec![double_bond(1)];
    let bonds_n3: Vec<InlineBond> = vec![single_bond(1), single_bond(6), single_bond(7)];
    let bonds_h4: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h5: Vec<InlineBond> = vec![single_bond(0)];
    let bonds_h6: Vec<InlineBond> = vec![single_bond(3)];
    let bonds_h7: Vec<InlineBond> = vec![single_bond(3)];

    let bond_lists: Vec<&[InlineBond]> = vec![
        &bonds_n0, &bonds_c1, &bonds_o2, &bonds_n3,
        &bonds_h4, &bonds_h5, &bonds_h6, &bonds_h7,
    ];

    let result = assign_uff_types(&atomic_numbers, &bond_lists).unwrap();

    assert_eq!(result.labels[0], "N_2", "urea N0 should be N_2 (amide sp2)");
    assert_eq!(result.labels[3], "N_2", "urea N3 should be N_2 (amide sp2)");
}
