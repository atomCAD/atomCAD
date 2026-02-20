use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::measurement::{
    MeasurementResult, SelectedAtomInfo, compute_measurement,
};

// Helper to create a SelectedAtomInfo
fn atom(id: u32, x: f64, y: f64, z: f64) -> SelectedAtomInfo {
    SelectedAtomInfo {
        result_atom_id: id,
        position: DVec3::new(x, y, z),
    }
}

// =============================================================================
// Distance (2 atoms)
// =============================================================================

#[test]
fn test_distance_simple() {
    let structure = AtomicStructure::new();
    let atoms = [atom(0, 0.0, 0.0, 0.0), atom(1, 1.54, 0.0, 0.0)];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Distance { distance } => {
            assert!((distance - 1.54).abs() < 1e-10, "Expected 1.54, got {distance}");
        }
        _ => panic!("Expected Distance, got {result:?}"),
    }
}

#[test]
fn test_distance_3d() {
    let structure = AtomicStructure::new();
    let atoms = [atom(0, 1.0, 2.0, 3.0), atom(1, 4.0, 6.0, 3.0)];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Distance { distance } => {
            let expected = (9.0 + 16.0_f64).sqrt(); // sqrt(25) = 5.0
            assert!((distance - expected).abs() < 1e-10, "Expected {expected}, got {distance}");
        }
        _ => panic!("Expected Distance"),
    }
}

// =============================================================================
// Angle (3 atoms)
// =============================================================================

#[test]
fn test_angle_bonded_chain() {
    // Create structure with bonds: 0-1-2 (atom 1 is the vertex)
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);

    let atoms = [
        atom(id0, 0.0, 0.0, 0.0),
        atom(id1, 1.0, 0.0, 0.0),
        atom(id2, 1.0, 1.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Angle {
            angle_degrees,
            vertex_index,
        } => {
            assert_eq!(vertex_index, 1, "Vertex should be atom 1 (middle of chain)");
            assert!(
                (angle_degrees - 90.0).abs() < 0.1,
                "Expected ~90 degrees, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Angle"),
    }
}

#[test]
fn test_angle_bonded_chain_vertex_first() {
    // Create structure with bonds: 0-1 and 0-2 (atom 0 is the vertex)
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id0, id2, 1);

    let atoms = [
        atom(id0, 0.0, 0.0, 0.0),
        atom(id1, 1.0, 0.0, 0.0),
        atom(id2, 0.0, 1.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Angle {
            angle_degrees,
            vertex_index,
        } => {
            assert_eq!(vertex_index, 0, "Vertex should be atom 0 (bonded to both)");
            assert!(
                (angle_degrees - 90.0).abs() < 0.1,
                "Expected ~90 degrees, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Angle"),
    }
}

#[test]
fn test_angle_no_bonds_geometric_fallback() {
    // No bonds - geometric heuristic: most distant pair = arms
    let structure = AtomicStructure::new();

    // Atom 0 and 2 are the most distant (3.0 apart)
    // Atom 1 is in between - should be detected as vertex
    let atoms = [
        atom(0, 0.0, 0.0, 0.0),
        atom(1, 1.0, 0.5, 0.0),
        atom(2, 3.0, 0.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Angle { vertex_index, .. } => {
            assert_eq!(
                vertex_index, 1,
                "Vertex should be atom 1 (not in the most distant pair)"
            );
        }
        _ => panic!("Expected Angle"),
    }
}

#[test]
fn test_angle_tetrahedral() {
    // Tetrahedral angle: 109.47 degrees
    // Central atom at origin, two neighbors at tetrahedral positions
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // center
    let id1 = structure.add_atom(6, DVec3::new(1.0, 1.0, 1.0));
    let id2 = structure.add_atom(6, DVec3::new(-1.0, -1.0, 1.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id0, id2, 1);

    let atoms = [
        atom(id0, 0.0, 0.0, 0.0),
        atom(id1, 1.0, 1.0, 1.0),
        atom(id2, -1.0, -1.0, 1.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Angle {
            angle_degrees,
            vertex_index,
        } => {
            assert_eq!(vertex_index, 0);
            assert!(
                (angle_degrees - 109.47).abs() < 0.1,
                "Expected ~109.47 degrees, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Angle"),
    }
}

// =============================================================================
// Dihedral (4 atoms)
// =============================================================================

#[test]
fn test_dihedral_bonded_chain() {
    // Create a bonded chain: 0-1-2-3
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(2.0, 1.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(3.0, 1.0, 1.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id2, id3, 1);

    let atoms = [
        atom(id0, 0.0, 0.0, 0.0),
        atom(id1, 1.0, 0.0, 0.0),
        atom(id2, 2.0, 1.0, 0.0),
        atom(id3, 3.0, 1.0, 1.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Dihedral { chain, .. } => {
            // Chain should be identified as 0-1-2-3
            assert_eq!(chain[0], 0, "A should be atom 0");
            assert_eq!(chain[1], 1, "B should be atom 1");
            assert_eq!(chain[2], 2, "C should be atom 2");
            assert_eq!(chain[3], 3, "D should be atom 3");
        }
        _ => panic!("Expected Dihedral"),
    }
}

#[test]
fn test_dihedral_bonded_chain_shuffled_order() {
    // Atoms given in non-chain order, but bonds define chain: 2-0-1-3
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(2.0, 1.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(3.0, 1.0, 1.0));
    structure.add_bond(id2, id0, 1); // 2-0
    structure.add_bond(id0, id1, 1); // 0-1
    structure.add_bond(id1, id3, 1); // 1-3

    let atoms = [
        atom(id0, 1.0, 0.0, 0.0),
        atom(id1, 2.0, 1.0, 0.0),
        atom(id2, 0.0, 0.0, 0.0),
        atom(id3, 3.0, 1.0, 1.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Dihedral { chain, .. } => {
            // Degree analysis: id0=2, id1=2, id2=1, id3=1
            // Ends (degree 1): indices 2, 3
            // Center (degree 2): indices 0, 1
            // Chain: 2-0-1-3 (ends[0]=2 is bonded to center[0]=0)
            assert_eq!(chain[0], 2, "A should be atom index 2 (id2)");
            assert_eq!(chain[1], 0, "B should be atom index 0 (id0)");
            assert_eq!(chain[2], 1, "C should be atom index 1 (id1)");
            assert_eq!(chain[3], 3, "D should be atom index 3 (id3)");
        }
        _ => panic!("Expected Dihedral"),
    }
}

#[test]
fn test_dihedral_no_bonds_geometric_fallback() {
    // No bonds - most distant pair should be the ends
    let structure = AtomicStructure::new();

    // Atoms 0 and 3 are the most distant (10 units apart)
    let atoms = [
        atom(0, 0.0, 0.0, 0.0),
        atom(1, 3.0, 1.0, 0.0),
        atom(2, 7.0, 1.0, 0.0),
        atom(3, 10.0, 0.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Dihedral { chain, .. } => {
            // Most distant: 0 and 3 → ends
            // Center: 1 and 2
            // B (closer to A=0): 1, C (closer to D=3): 2
            assert_eq!(chain[0], 0, "A should be 0");
            assert_eq!(chain[1], 1, "B should be 1");
            assert_eq!(chain[2], 2, "C should be 2");
            assert_eq!(chain[3], 3, "D should be 3");
        }
        _ => panic!("Expected Dihedral"),
    }
}

#[test]
fn test_dihedral_planar_180_degrees() {
    // All atoms in a plane, anti-periplanar (180 degrees)
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(1.0, -1.0, 0.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id2, id3, 1);

    let atoms = [
        atom(id0, 0.0, 1.0, 0.0),
        atom(id1, 0.0, 0.0, 0.0),
        atom(id2, 1.0, 0.0, 0.0),
        atom(id3, 1.0, -1.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Dihedral { angle_degrees, .. } => {
            assert!(
                (angle_degrees.abs() - 180.0).abs() < 0.1,
                "Expected ~180 degrees, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Dihedral"),
    }
}

#[test]
fn test_dihedral_eclipsed_0_degrees() {
    // Eclipsed conformation (0 degrees)
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id2, id3, 1);

    let atoms = [
        atom(id0, 0.0, 1.0, 0.0),
        atom(id1, 0.0, 0.0, 0.0),
        atom(id2, 1.0, 0.0, 0.0),
        atom(id3, 1.0, 1.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Dihedral { angle_degrees, .. } => {
            assert!(
                angle_degrees.abs() < 0.1,
                "Expected ~0 degrees, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Dihedral"),
    }
}

#[test]
fn test_dihedral_gauche_90_degrees() {
    // 90 degree dihedral: A above, D out of plane
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id2, id3, 1);

    let atoms = [
        atom(id0, 0.0, 1.0, 0.0),
        atom(id1, 0.0, 0.0, 0.0),
        atom(id2, 1.0, 0.0, 0.0),
        atom(id3, 1.0, 0.0, 1.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Dihedral { angle_degrees, .. } => {
            assert!(
                (angle_degrees.abs() - 90.0).abs() < 0.1,
                "Expected ~90 degrees, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Dihedral"),
    }
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn test_too_few_atoms_returns_none() {
    let structure = AtomicStructure::new();
    let atoms = [atom(0, 0.0, 0.0, 0.0)];
    assert!(compute_measurement(&atoms, &structure).is_none());
}

#[test]
fn test_too_many_atoms_returns_none() {
    let structure = AtomicStructure::new();
    let atoms = [
        atom(0, 0.0, 0.0, 0.0),
        atom(1, 1.0, 0.0, 0.0),
        atom(2, 2.0, 0.0, 0.0),
        atom(3, 3.0, 0.0, 0.0),
        atom(4, 4.0, 0.0, 0.0),
    ];
    assert!(compute_measurement(&atoms, &structure).is_none());
}

#[test]
fn test_angle_triangle_all_bonded() {
    // All 3 atoms bonded to each other (triangle) - geometric fallback
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 0.1, 0.0)); // close to midpoint
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id0, id2, 1);

    let atoms = [
        atom(id0, 0.0, 0.0, 0.0),
        atom(id1, 2.0, 0.0, 0.0),
        atom(id2, 1.0, 0.1, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Angle { vertex_index, .. } => {
            // All atoms have degree 2, so geometric fallback is used.
            // Most distant pair: 0 and 1 (distance 2.0), so vertex = 2
            assert_eq!(vertex_index, 2, "Vertex should be atom 2 (geometric fallback)");
        }
        _ => panic!("Expected Angle"),
    }
}

#[test]
fn test_dihedral_cycle_geometric_fallback() {
    // 4-cycle: all degree 2, falls to geometric fallback
    let mut structure = AtomicStructure::new();
    let id0 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id1 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(0.0, 1.0, 0.0));
    structure.add_bond(id0, id1, 1);
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id2, id3, 1);
    structure.add_bond(id3, id0, 1);

    let atoms = [
        atom(id0, 0.0, 0.0, 0.0),
        atom(id1, 1.0, 0.0, 0.0),
        atom(id2, 1.0, 1.0, 0.0),
        atom(id3, 0.0, 1.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    // Should return a Dihedral result (the specific chain depends on geometry)
    assert!(
        matches!(result, MeasurementResult::Dihedral { .. }),
        "Expected Dihedral for 4-cycle"
    );
}

#[test]
fn test_angle_linear_180_degrees() {
    // Three collinear atoms: angle should be 180 degrees
    let structure = AtomicStructure::new();
    let atoms = [
        atom(0, 0.0, 0.0, 0.0),
        atom(1, 1.0, 0.0, 0.0),
        atom(2, 2.0, 0.0, 0.0),
    ];

    let result = compute_measurement(&atoms, &structure).unwrap();
    match result {
        MeasurementResult::Angle { angle_degrees, .. } => {
            assert!(
                (angle_degrees - 180.0).abs() < 0.1,
                "Expected ~180 degrees for collinear atoms, got {angle_degrees}"
            );
        }
        _ => panic!("Expected Angle"),
    }
}
