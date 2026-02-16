// Tests for molecular topology enumeration (Phase 14).
//
// Validates that MolecularTopology::from_structure() produces correct
// interaction counts for all 9 reference molecules from uff_reference.json,
// plus hand-built test cases and edge cases.

use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::simulation::topology::MolecularTopology;

// ============================================================================
// Helper: build AtomicStructure from reference JSON data
// ============================================================================

#[derive(serde::Deserialize)]
struct ReferenceData {
    molecules: Vec<ReferenceMolecule>,
}

#[derive(serde::Deserialize)]
struct ReferenceMolecule {
    name: String,
    atoms: Vec<ReferenceAtom>,
    bonds: Vec<ReferenceBond>,
    input_positions: Vec<[f64; 3]>,
    interaction_counts: InteractionCounts,
}

#[derive(serde::Deserialize)]
struct ReferenceAtom {
    atomic_number: i16,
}

#[derive(serde::Deserialize)]
struct ReferenceBond {
    atom1: usize,
    atom2: usize,
    order: f64,
}

#[derive(serde::Deserialize)]
struct InteractionCounts {
    bonds: usize,
    angles: usize,
    torsions: usize,
    inversions: usize,
}

fn load_reference_data() -> ReferenceData {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/crystolecule/simulation/test_data/uff_reference.json"
    );
    let content = std::fs::read_to_string(path).expect("Failed to read uff_reference.json");
    serde_json::from_str(&content).expect("Failed to parse uff_reference.json")
}

fn bond_order_from_f64(order: f64) -> u8 {
    if (order - 1.0).abs() < 0.01 {
        BOND_SINGLE
    } else if (order - 1.5).abs() < 0.01 {
        BOND_AROMATIC
    } else if (order - 2.0).abs() < 0.01 {
        BOND_DOUBLE
    } else if (order - 3.0).abs() < 0.01 {
        BOND_TRIPLE
    } else {
        BOND_SINGLE
    }
}

fn build_structure_from_reference(mol: &ReferenceMolecule) -> AtomicStructure {
    let mut structure = AtomicStructure::new();

    // Add atoms
    for (i, atom) in mol.atoms.iter().enumerate() {
        let pos = DVec3::new(
            mol.input_positions[i][0],
            mol.input_positions[i][1],
            mol.input_positions[i][2],
        );
        let id = structure.add_atom(atom.atomic_number, pos);
        // Atom IDs are 1-based sequential, so id should equal i+1
        assert_eq!(id, (i + 1) as u32);
    }

    // Add bonds (reference uses 0-based indices, AtomicStructure uses 1-based IDs)
    for bond in &mol.bonds {
        let order = bond_order_from_f64(bond.order);
        structure.add_bond((bond.atom1 + 1) as u32, (bond.atom2 + 1) as u32, order);
    }

    structure
}

/// Helper to build a simple molecule from atom list and bond list.
fn build_simple_structure(
    atoms: &[(i16, [f64; 3])],
    bonds: &[(usize, usize, u8)],
) -> AtomicStructure {
    let mut structure = AtomicStructure::new();

    for &(atomic_number, pos) in atoms {
        structure.add_atom(atomic_number, DVec3::new(pos[0], pos[1], pos[2]));
    }

    for &(a1, a2, order) in bonds {
        structure.add_bond((a1 + 1) as u32, (a2 + 1) as u32, order);
    }

    structure
}

// ============================================================================
// Test B8: Interaction counts for all 9 reference molecules
// ============================================================================

fn assert_topology_counts(name: &str, topo: &MolecularTopology, expected: &InteractionCounts) {
    assert_eq!(
        topo.bonds.len(),
        expected.bonds,
        "{}: bond count mismatch (got {}, expected {})",
        name,
        topo.bonds.len(),
        expected.bonds
    );
    assert_eq!(
        topo.angles.len(),
        expected.angles,
        "{}: angle count mismatch (got {}, expected {})",
        name,
        topo.angles.len(),
        expected.angles
    );
    assert_eq!(
        topo.torsions.len(),
        expected.torsions,
        "{}: torsion count mismatch (got {}, expected {})",
        name,
        topo.torsions.len(),
        expected.torsions
    );
    assert_eq!(
        topo.inversions.len(),
        expected.inversions,
        "{}: inversion count mismatch (got {}, expected {})",
        name,
        topo.inversions.len(),
        expected.inversions
    );
}

#[test]
fn test_b8_methane_counts() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "methane").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 5);
    assert_topology_counts("methane", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_ethylene_counts() {
    let data = load_reference_data();
    let mol = data
        .molecules
        .iter()
        .find(|m| m.name == "ethylene")
        .unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 6);
    assert_topology_counts("ethylene", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_ethane_counts() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "ethane").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 8);
    assert_topology_counts("ethane", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_benzene_counts() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "benzene").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 12);
    assert_topology_counts("benzene", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_butane_counts() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "butane").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 14);
    assert_topology_counts("butane", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_water_counts() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "water").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 3);
    assert_topology_counts("water", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_ammonia_counts() {
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "ammonia").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 4);
    assert_topology_counts("ammonia", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_adamantane_counts() {
    let data = load_reference_data();
    let mol = data
        .molecules
        .iter()
        .find(|m| m.name == "adamantane")
        .unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 26);
    assert_topology_counts("adamantane", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_methanethiol_counts() {
    let data = load_reference_data();
    let mol = data
        .molecules
        .iter()
        .find(|m| m.name == "methanethiol")
        .unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 6);
    assert_topology_counts("methanethiol", &topo, &mol.interaction_counts);
}

#[test]
fn test_b8_all_molecules_summary() {
    // Run all 9 molecules in one test for a quick summary
    let data = load_reference_data();

    for mol in &data.molecules {
        let structure = build_structure_from_reference(mol);
        let topo = MolecularTopology::from_structure(&structure);

        assert_eq!(
            topo.num_atoms,
            mol.atoms.len(),
            "{}: atom count mismatch",
            mol.name
        );
        assert_topology_counts(&mol.name, &topo, &mol.interaction_counts);
    }
}

// ============================================================================
// Test: RDKit testUFFBuilder1 reference cases (heavy atoms only)
// ============================================================================

#[test]
fn test_builder1_cc_o_c() {
    // CC(O)C → 4 heavy atoms: C1-C2(-O)-C3
    // RDKit reference: 3 bonds, 3 angles, 0 torsions
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]), // C1
            (6, [1.5, 0.0, 0.0]), // C2
            (8, [1.5, 1.5, 0.0]), // O
            (6, [3.0, 0.0, 0.0]), // C3
        ],
        &[
            (0, 1, BOND_SINGLE), // C1-C2
            (1, 2, BOND_SINGLE), // C2-O
            (1, 3, BOND_SINGLE), // C2-C3
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.bonds.len(), 3, "CC(O)C bonds");
    assert_eq!(topo.angles.len(), 3, "CC(O)C angles");
    assert_eq!(topo.torsions.len(), 0, "CC(O)C torsions");
}

#[test]
fn test_builder1_ccoc() {
    // CCOC → 4 heavy atoms: C1-C2-O-C3
    // RDKit reference: 3 bonds, 2 angles, 1 torsion
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]), // C1
            (6, [1.5, 0.0, 0.0]), // C2
            (8, [3.0, 0.0, 0.0]), // O
            (6, [4.5, 0.0, 0.0]), // C3
        ],
        &[
            (0, 1, BOND_SINGLE), // C1-C2
            (1, 2, BOND_SINGLE), // C2-O
            (2, 3, BOND_SINGLE), // O-C3
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.bonds.len(), 3, "CCOC bonds");
    assert_eq!(topo.angles.len(), 2, "CCOC angles");
    assert_eq!(topo.torsions.len(), 1, "CCOC torsions");
}

// ============================================================================
// Test: Edge cases
// ============================================================================

#[test]
fn test_empty_structure() {
    let structure = AtomicStructure::new();
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 0);
    assert_eq!(topo.bonds.len(), 0);
    assert_eq!(topo.angles.len(), 0);
    assert_eq!(topo.torsions.len(), 0);
    assert_eq!(topo.inversions.len(), 0);
}

#[test]
fn test_single_atom() {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::ZERO);

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 1);
    assert_eq!(topo.bonds.len(), 0);
    assert_eq!(topo.angles.len(), 0);
    assert_eq!(topo.torsions.len(), 0);
    assert_eq!(topo.inversions.len(), 0);
}

#[test]
fn test_two_atoms_one_bond() {
    let structure = build_simple_structure(
        &[(6, [0.0, 0.0, 0.0]), (6, [1.5, 0.0, 0.0])],
        &[(0, 1, BOND_SINGLE)],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.num_atoms, 2);
    assert_eq!(topo.bonds.len(), 1);
    assert_eq!(topo.angles.len(), 0);
    assert_eq!(topo.torsions.len(), 0);
    assert_eq!(topo.inversions.len(), 0);
}

#[test]
fn test_three_membered_ring() {
    // Cyclopropane (heavy atoms only): C1-C2-C3-C1
    // 3 bonds, 3 angles, 0 torsions (all i-j-k-l where i==l are skipped)
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),
            (6, [1.5, 0.0, 0.0]),
            (6, [0.75, 1.3, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (1, 2, BOND_SINGLE),
            (0, 2, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 3, "3-ring bonds");
    assert_eq!(topo.angles.len(), 3, "3-ring angles");
    // All potential torsions have i == l (e.g., C3-C1-C2-C3), so 0 torsions
    assert_eq!(topo.torsions.len(), 0, "3-ring torsions (degenerate)");
}

#[test]
fn test_four_membered_ring() {
    // Cyclobutane (heavy atoms only): C1-C2-C3-C4-C1
    // 4 bonds, 4 angles (each C has 2 bonds → 1 angle each), 4 torsions
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),
            (6, [1.5, 0.0, 0.0]),
            (6, [1.5, 1.5, 0.0]),
            (6, [0.0, 1.5, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (1, 2, BOND_SINGLE),
            (2, 3, BOND_SINGLE),
            (3, 0, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 4, "4-ring bonds");
    assert_eq!(topo.angles.len(), 4, "4-ring angles");
    assert_eq!(topo.torsions.len(), 4, "4-ring torsions");
}

// ============================================================================
// Test: Inversion-specific cases
// ============================================================================

#[test]
fn test_sp2_carbon_inversions() {
    // A single sp2 carbon: C=C with 2 single bonds from each C
    // C1(=C0)(-H2)(-H3) — C0 has 3 bonds (1 double, 2 single) → 3 inversions
    // C1 same → 3 inversions. Total: 6
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),   // C0
            (6, [1.3, 0.0, 0.0]),   // C1
            (1, [-0.5, 0.9, 0.0]),  // H2
            (1, [-0.5, -0.9, 0.0]), // H3
            (1, [1.8, 0.9, 0.0]),   // H4
            (1, [1.8, -0.9, 0.0]),  // H5
        ],
        &[
            (0, 1, BOND_DOUBLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
            (1, 4, BOND_SINGLE),
            (1, 5, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 6, "ethylene-like inversions");
}

#[test]
fn test_sp3_carbon_no_inversions() {
    // Methane: sp3 carbon with 4 single bonds → 0 inversions
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (1, [0.0, 1.0, 0.0]),
            (1, [0.0, 0.0, 1.0]),
            (1, [-1.0, 0.0, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
            (0, 4, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 0, "sp3 carbon: no inversions");
}

#[test]
fn test_sp3_nitrogen_no_inversions() {
    // Ammonia: sp3 nitrogen with 3 single bonds → 0 inversions
    let structure = build_simple_structure(
        &[
            (7, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (1, [0.0, 1.0, 0.0]),
            (1, [0.0, 0.0, 1.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 0, "sp3 nitrogen: no inversions");
}

#[test]
fn test_sp2_nitrogen_inversions() {
    // N with 1 double + 2 single bonds (e.g., N=C(-H)(-H)) → 3 inversions
    let structure = build_simple_structure(
        &[
            (7, [0.0, 0.0, 0.0]),   // N
            (6, [1.3, 0.0, 0.0]),   // C
            (1, [-0.5, 0.9, 0.0]),  // H1
            (1, [-0.5, -0.9, 0.0]), // H2
        ],
        &[
            (0, 1, BOND_DOUBLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 3, "sp2 nitrogen: 3 inversions");
}

#[test]
fn test_aromatic_carbon_inversions() {
    // 3 aromatic bonds on a carbon → sp2-like → 3 inversions
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),   // C (center)
            (6, [1.4, 0.0, 0.0]),   // C
            (6, [-0.7, 1.2, 0.0]),  // C
            (1, [-0.7, -1.2, 0.0]), // H
        ],
        &[
            (0, 1, BOND_AROMATIC),
            (0, 2, BOND_AROMATIC),
            (0, 3, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 3, "aromatic carbon: 3 inversions");
}

#[test]
fn test_phosphorus_inversions() {
    // Phosphorus with 3 single bonds → pyramidal inversion (group 15)
    let structure = build_simple_structure(
        &[
            (15, [0.0, 0.0, 0.0]), // P
            (1, [1.0, 0.0, 0.0]),  // H1
            (1, [0.0, 1.0, 0.0]),  // H2
            (1, [0.0, 0.0, 1.0]),  // H3
        ],
        &[
            (0, 1, BOND_SINGLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(
        topo.inversions.len(),
        3,
        "phosphorus: 3 inversions (group 15)"
    );
}

#[test]
fn test_phosphorus_4_bonds_no_inversions() {
    // Phosphorus with 4 bonds → not exactly 3, no inversions
    let structure = build_simple_structure(
        &[
            (15, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (1, [0.0, 1.0, 0.0]),
            (1, [0.0, 0.0, 1.0]),
            (8, [-1.0, 0.0, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
            (0, 4, BOND_DOUBLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 0, "P with 4 bonds: no inversions");
}

// ============================================================================
// Test: Topology structure integrity
// ============================================================================

#[test]
fn test_index_validity() {
    // All indices in interactions should be within [0, num_atoms)
    let data = load_reference_data();

    for mol in &data.molecules {
        let structure = build_structure_from_reference(mol);
        let topo = MolecularTopology::from_structure(&structure);
        let n = topo.num_atoms;

        for (i, bond) in topo.bonds.iter().enumerate() {
            assert!(
                bond.idx1 < n && bond.idx2 < n,
                "{}: bond {} has out-of-range index",
                mol.name,
                i
            );
            assert!(
                bond.idx1 < bond.idx2,
                "{}: bond {} has idx1 >= idx2",
                mol.name,
                i
            );
        }

        for (i, angle) in topo.angles.iter().enumerate() {
            assert!(
                angle.idx1 < n && angle.idx2 < n && angle.idx3 < n,
                "{}: angle {} has out-of-range index",
                mol.name,
                i
            );
            assert_ne!(
                angle.idx1, angle.idx2,
                "{}: angle {} has duplicate atoms",
                mol.name, i
            );
            assert_ne!(
                angle.idx2, angle.idx3,
                "{}: angle {} has duplicate atoms",
                mol.name, i
            );
        }

        for (i, torsion) in topo.torsions.iter().enumerate() {
            assert!(
                torsion.idx1 < n && torsion.idx2 < n && torsion.idx3 < n && torsion.idx4 < n,
                "{}: torsion {} has out-of-range index",
                mol.name,
                i
            );
            assert_ne!(
                torsion.idx1, torsion.idx4,
                "{}: torsion {} has idx1 == idx4 (degenerate)",
                mol.name, i
            );
        }

        for (i, inv) in topo.inversions.iter().enumerate() {
            assert!(
                inv.idx1 < n && inv.idx2 < n && inv.idx3 < n && inv.idx4 < n,
                "{}: inversion {} has out-of-range index",
                mol.name,
                i
            );
        }
    }
}

#[test]
fn test_positions_match_input() {
    // Verify that topology positions match the input positions
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "methane").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    for (i, pos) in mol.input_positions.iter().enumerate() {
        let x = topo.positions[i * 3];
        let y = topo.positions[i * 3 + 1];
        let z = topo.positions[i * 3 + 2];
        assert!(
            (x - pos[0]).abs() < 1e-10 && (y - pos[1]).abs() < 1e-10 && (z - pos[2]).abs() < 1e-10,
            "atom {}: position mismatch",
            i
        );
    }
}

#[test]
fn test_atom_ids_mapping() {
    // Verify atom_ids maps topology indices back to structure IDs
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "ethane").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.atom_ids.len(), mol.atoms.len());
    for (i, &id) in topo.atom_ids.iter().enumerate() {
        // IDs should be 1-based sequential
        assert_eq!(id, (i + 1) as u32, "atom_ids[{}] should be {}", i, i + 1);
    }
}

#[test]
fn test_atomic_numbers_preserved() {
    // Verify atomic numbers are correctly preserved
    let data = load_reference_data();
    let mol = data.molecules.iter().find(|m| m.name == "water").unwrap();
    let structure = build_structure_from_reference(mol);
    let topo = MolecularTopology::from_structure(&structure);

    for (i, atom) in mol.atoms.iter().enumerate() {
        assert_eq!(
            topo.atomic_numbers[i], atom.atomic_number,
            "atom {}: atomic number mismatch",
            i
        );
    }
}

#[test]
fn test_bond_orders_preserved() {
    // Verify bond orders are preserved correctly
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),
            (6, [1.3, 0.0, 0.0]),
            (6, [2.6, 0.0, 0.0]),
        ],
        &[(0, 1, BOND_DOUBLE), (1, 2, BOND_SINGLE)],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.bonds.len(), 2);

    // Find each bond and check order
    let bond_01 = topo.bonds.iter().find(|b| b.idx1 == 0 && b.idx2 == 1);
    let bond_12 = topo.bonds.iter().find(|b| b.idx1 == 1 && b.idx2 == 2);

    assert!(bond_01.is_some(), "bond 0-1 should exist");
    assert!(bond_12.is_some(), "bond 1-2 should exist");
    assert_eq!(bond_01.unwrap().bond_order, BOND_DOUBLE);
    assert_eq!(bond_12.unwrap().bond_order, BOND_SINGLE);
}

// ============================================================================
// Test: Angle count formula verification
// ============================================================================

#[test]
fn test_angle_count_formula() {
    // Verify that angle count = sum of C(n_bonds, 2) over all atoms
    let data = load_reference_data();

    for mol in &data.molecules {
        let structure = build_structure_from_reference(mol);
        let topo = MolecularTopology::from_structure(&structure);

        // Count bonds per atom from topology
        let mut bond_counts = vec![0usize; topo.num_atoms];
        for bond in &topo.bonds {
            bond_counts[bond.idx1] += 1;
            bond_counts[bond.idx2] += 1;
        }

        // Sum C(n, 2) = n*(n-1)/2
        let expected_angles: usize = bond_counts
            .iter()
            .map(|&n| if n >= 2 { n * (n - 1) / 2 } else { 0 })
            .sum();

        assert_eq!(
            topo.angles.len(),
            expected_angles,
            "{}: angle count should equal sum of C(n_bonds,2)",
            mol.name
        );
    }
}

// ============================================================================
// Test: Linear molecule (no angles at terminal atoms)
// ============================================================================

#[test]
fn test_linear_chain() {
    // C-C-C linear chain: 2 bonds, 1 angle (at middle C), 0 torsions
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),
            (6, [1.5, 0.0, 0.0]),
            (6, [3.0, 0.0, 0.0]),
        ],
        &[(0, 1, BOND_SINGLE), (1, 2, BOND_SINGLE)],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 2);
    assert_eq!(topo.angles.len(), 1);
    assert_eq!(topo.torsions.len(), 0);
}

#[test]
fn test_four_atom_chain() {
    // C-C-C-C: 3 bonds, 2 angles, 1 torsion
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),
            (6, [1.5, 0.0, 0.0]),
            (6, [3.0, 0.0, 0.0]),
            (6, [4.5, 0.0, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (1, 2, BOND_SINGLE),
            (2, 3, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 3);
    assert_eq!(topo.angles.len(), 2);
    assert_eq!(topo.torsions.len(), 1);
}

// ============================================================================
// Test: Inversion permutation structure
// ============================================================================

#[test]
fn test_inversion_permutations() {
    // One sp2 center with 3 neighbors: verify 3 permutations have correct center
    // and each neighbor appears exactly twice as idx4 (out-of-plane)
    let structure = build_simple_structure(
        &[
            (6, [0.0, 0.0, 0.0]),   // C center (sp2)
            (6, [1.3, 0.0, 0.0]),   // C (double bond)
            (1, [-0.5, 0.9, 0.0]),  // H1
            (1, [-0.5, -0.9, 0.0]), // H2
        ],
        &[
            (0, 1, BOND_DOUBLE),
            (0, 2, BOND_SINGLE),
            (0, 3, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.inversions.len(), 3);

    // All inversions should have center = 0
    for inv in &topo.inversions {
        assert_eq!(inv.idx2, 0, "inversion center should be atom 0");
    }

    // Each neighbor should appear exactly once as idx4 (out-of-plane atom)
    let mut idx4_counts = vec![0; 4];
    for inv in &topo.inversions {
        idx4_counts[inv.idx4] += 1;
    }
    // Neighbors are atoms 1, 2, 3 — each should be idx4 exactly once
    assert_eq!(idx4_counts[1], 1, "atom 1 should be idx4 exactly once");
    assert_eq!(idx4_counts[2], 1, "atom 2 should be idx4 exactly once");
    assert_eq!(idx4_counts[3], 1, "atom 3 should be idx4 exactly once");
}

// ============================================================================
// Test: Delete markers excluded
// ============================================================================

#[test]
fn test_delete_markers_excluded() {
    // Build a structure, then simulate a delete marker by setting atomic_number = 0
    // We can't easily do this through the public API, but we can test that
    // having zero-bond atoms doesn't cause issues
    let structure = build_simple_structure(
        &[(6, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])],
        &[(0, 1, BOND_SINGLE)],
    );

    let topo = MolecularTopology::from_structure(&structure);
    assert_eq!(topo.num_atoms, 2);
    assert_eq!(topo.bonds.len(), 1);
}

// ============================================================================
// Test: Five-membered ring (cyclopentane)
// ============================================================================

#[test]
fn test_five_membered_ring() {
    // Cyclopentane heavy atoms: C1-C2-C3-C4-C5-C1
    // 5 bonds, 5 angles (each C has 2 bonds → 1 angle each), 5 torsions
    let structure = build_simple_structure(
        &[
            (6, [1.0, 0.0, 0.0]),
            (6, [0.31, 0.95, 0.0]),
            (6, [-0.81, 0.59, 0.0]),
            (6, [-0.81, -0.59, 0.0]),
            (6, [0.31, -0.95, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (1, 2, BOND_SINGLE),
            (2, 3, BOND_SINGLE),
            (3, 4, BOND_SINGLE),
            (4, 0, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 5, "5-ring bonds");
    assert_eq!(topo.angles.len(), 5, "5-ring angles");
    assert_eq!(topo.torsions.len(), 5, "5-ring torsions");
}

// ============================================================================
// Test: Six-membered ring (cyclohexane vs benzene)
// ============================================================================

#[test]
fn test_six_membered_ring_single_bonds() {
    // Cyclohexane heavy atoms: 6 C's in ring with single bonds
    // 6 bonds, 6 angles, 6 torsions, 0 inversions (all sp3)
    let structure = build_simple_structure(
        &[
            (6, [1.0, 0.0, 0.0]),
            (6, [0.5, 0.87, 0.0]),
            (6, [-0.5, 0.87, 0.0]),
            (6, [-1.0, 0.0, 0.0]),
            (6, [-0.5, -0.87, 0.0]),
            (6, [0.5, -0.87, 0.0]),
        ],
        &[
            (0, 1, BOND_SINGLE),
            (1, 2, BOND_SINGLE),
            (2, 3, BOND_SINGLE),
            (3, 4, BOND_SINGLE),
            (4, 5, BOND_SINGLE),
            (5, 0, BOND_SINGLE),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 6, "cyclohexane bonds");
    assert_eq!(topo.angles.len(), 6, "cyclohexane angles");
    assert_eq!(topo.torsions.len(), 6, "cyclohexane torsions");
    assert_eq!(topo.inversions.len(), 0, "cyclohexane inversions (sp3)");
}

#[test]
fn test_six_membered_ring_aromatic() {
    // Benzene heavy atoms: 6 C's in ring with aromatic bonds
    // 6 bonds, 6 angles, 6 torsions, 18 inversions (6 sp2 × 3 permutations)
    let structure = build_simple_structure(
        &[
            (6, [1.0, 0.0, 0.0]),
            (6, [0.5, 0.87, 0.0]),
            (6, [-0.5, 0.87, 0.0]),
            (6, [-1.0, 0.0, 0.0]),
            (6, [-0.5, -0.87, 0.0]),
            (6, [0.5, -0.87, 0.0]),
        ],
        &[
            (0, 1, BOND_AROMATIC),
            (1, 2, BOND_AROMATIC),
            (2, 3, BOND_AROMATIC),
            (3, 4, BOND_AROMATIC),
            (4, 5, BOND_AROMATIC),
            (5, 0, BOND_AROMATIC),
        ],
    );

    let topo = MolecularTopology::from_structure(&structure);

    assert_eq!(topo.bonds.len(), 6, "benzene heavy atoms bonds");
    assert_eq!(topo.angles.len(), 6, "benzene heavy atoms angles");
    assert_eq!(topo.torsions.len(), 6, "benzene heavy atoms torsions");
    // Each C has 2 aromatic bonds in ring, but total 2 bonds (not 3)
    // So no inversions because C has only 2 bonds, not 3
    assert_eq!(
        topo.inversions.len(),
        0,
        "benzene heavy atoms only: each C has 2 bonds, not 3"
    );
}
