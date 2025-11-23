use rust_lib_flutter_cad::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use rust_lib_flutter_cad::api::common_api_types::SelectModifier;
use glam::f64::DVec3;

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a simple test molecule (ethane-like structure)
fn create_test_molecule() -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let h1 = structure.add_atom(1, DVec3::new(-0.5, 0.5, 0.0));
    let h2 = structure.add_atom(1, DVec3::new(2.0, 0.5, 0.0));
    
    structure.add_bond(c1, c2, 1);
    structure.add_bond(c1, h1, 1);
    structure.add_bond(c2, h2, 1);
    
    structure
}

/// Verifies grid consistency - all atoms in grid match their actual positions
fn verify_grid_consistency(structure: &AtomicStructure) -> bool {
    for (_, atom) in structure.iter_atoms() {
        let nearby = structure.get_atoms_in_radius(&atom.position, 0.1);
        if !nearby.contains(&atom.id) {
            return false;
        }
    }
    true
}

/// Verifies all bonds are bidirectional
fn verify_bonds_bidirectional(structure: &AtomicStructure) -> bool {
    for (_, atom) in structure.iter_atoms() {
        for bond in &atom.bonds {
            let other_id = bond.other_atom_id();
            if let Some(other_atom) = structure.get_atom(other_id) {
                let has_reverse_bond = other_atom.bonds.iter()
                    .any(|b| b.other_atom_id() == atom.id);
                if !has_reverse_bond {
                    return false;
                }
            } else {
                return false; // Bond points to non-existent atom
            }
        }
    }
    true
}

// ============================================================================
// Group 1: Basic Atom CRUD Operations
// ============================================================================

#[test]
fn test_add_atom_returns_unique_ids() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

#[test]
fn test_add_and_retrieve_atom() {
    let mut structure = AtomicStructure::new();
    let pos = DVec3::new(1.5, 2.5, 3.5);
    let id = structure.add_atom(8, pos);
    
    let atom = structure.get_atom(id).expect("Atom should exist");
    assert_eq!(atom.atomic_number, 8);
    assert_eq!(atom.position, pos);
    assert_eq!(atom.id, id);
}

#[test]
fn test_delete_atom_removes_from_structure() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    assert!(structure.get_atom(id).is_some());
    structure.delete_atom(id);
    assert!(structure.get_atom(id).is_none());
}

#[test]
fn test_delete_atom_removes_from_grid() {
    let mut structure = AtomicStructure::new();
    let pos = DVec3::new(5.0, 5.0, 5.0);
    let id = structure.add_atom(6, pos);
    
    // Verify atom is in grid
    let nearby_before = structure.get_atoms_in_radius(&pos, 1.0);
    assert!(nearby_before.contains(&id));
    
    structure.delete_atom(id);
    
    // Verify atom is removed from grid
    let nearby_after = structure.get_atoms_in_radius(&pos, 1.0);
    assert!(!nearby_after.contains(&id));
}

#[test]
fn test_set_atom_position_updates_grid() {
    let mut structure = AtomicStructure::new();
    let old_pos = DVec3::new(0.0, 0.0, 0.0);
    let new_pos = DVec3::new(10.0, 10.0, 10.0);
    let id = structure.add_atom(6, old_pos);
    
    structure.set_atom_position(id, new_pos);
    
    // Should not be found at old position
    let at_old = structure.get_atoms_in_radius(&old_pos, 1.0);
    assert!(!at_old.contains(&id));
    
    // Should be found at new position
    let at_new = structure.get_atoms_in_radius(&new_pos, 1.0);
    assert!(at_new.contains(&id));
}

#[test]
fn test_get_atoms_in_radius() {
    let mut structure = AtomicStructure::new();
    let center = DVec3::new(0.0, 0.0, 0.0);
    
    let id1 = structure.add_atom(6, DVec3::new(0.5, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    structure.add_atom(6, DVec3::new(10.0, 0.0, 0.0));
    
    let nearby = structure.get_atoms_in_radius(&center, 2.0);
    assert_eq!(nearby.len(), 1);
    assert!(nearby.contains(&id1));
    
    let medium = structure.get_atoms_in_radius(&center, 6.0);
    assert_eq!(medium.len(), 2);
    assert!(medium.contains(&id1));
    assert!(medium.contains(&id2));
}

#[test]
fn test_operations_on_empty_structure() {
    let structure = AtomicStructure::new();
    
    assert_eq!(structure.get_num_of_atoms(), 0);
    assert_eq!(structure.get_num_of_bonds(), 0);
    assert!(structure.get_atom(999).is_none());
    assert!(!structure.has_selected_atoms());
    assert!(!structure.has_selection());
}

#[test]
fn test_get_num_of_atoms() {
    let mut structure = AtomicStructure::new();
    assert_eq!(structure.get_num_of_atoms(), 0);
    
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    assert_eq!(structure.get_num_of_atoms(), 1);
    
    structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    structure.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    assert_eq!(structure.get_num_of_atoms(), 3);
}

#[test]
fn test_iter_atoms() {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    structure.add_atom(8, DVec3::new(1.0, 0.0, 0.0));
    
    let count = structure.iter_atoms().count();
    assert_eq!(count, 2);
    
    let atomic_numbers: Vec<i16> = structure.iter_atoms()
        .map(|(_, atom)| atom.atomic_number)
        .collect();
    assert!(atomic_numbers.contains(&6));
    assert!(atomic_numbers.contains(&8));
}

// ============================================================================
// Group 2: Bond Management
// ============================================================================

#[test]
fn test_add_bond_creates_bidirectional_link() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    
    structure.add_bond(id1, id2, 1);
    
    // Check atom1 has bond to atom2
    let atom1 = structure.get_atom(id1).unwrap();
    assert!(atom1.bonds.iter().any(|b| b.other_atom_id() == id2));
    
    // Check atom2 has bond to atom1
    let atom2 = structure.get_atom(id2).unwrap();
    assert!(atom2.bonds.iter().any(|b| b.other_atom_id() == id1));
}

#[test]
fn test_add_bond_prevents_duplicates() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    
    structure.add_bond_checked(id1, id2, 1);
    structure.add_bond_checked(id1, id2, 1); // Try to add duplicate
    
    let atom1 = structure.get_atom(id1).unwrap();
    let bonds_to_atom2 = atom1.bonds.iter().filter(|b| b.other_atom_id() == id2).count();
    assert_eq!(bonds_to_atom2, 1, "Should only have one bond between atoms");
}

#[test]
fn test_delete_atom_removes_all_bonds() {
    let mut structure = create_test_molecule();
    let initial_bonds = structure.get_num_of_bonds();
    
    // Get first atom (should have bonds)
    let first_id = structure.iter_atoms().next().unwrap().0;
    structure.delete_atom(*first_id);
    
    // Bond count should decrease
    assert!(structure.get_num_of_bonds() < initial_bonds);
    
    // All remaining atoms should have valid bonds
    assert!(verify_bonds_bidirectional(&structure));
}

#[test]
fn test_delete_bond_removes_from_both_atoms() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    structure.add_bond(id1, id2, 1);
    
    let bond_ref = BondReference { atom_id1: id1, atom_id2: id2 };
    structure.delete_bond(&bond_ref);
    
    // Check bond removed from atom1
    let atom1 = structure.get_atom(id1).unwrap();
    assert!(!atom1.bonds.iter().any(|b| b.other_atom_id() == id2));
    
    // Check bond removed from atom2
    let atom2 = structure.get_atom(id2).unwrap();
    assert!(!atom2.bonds.iter().any(|b| b.other_atom_id() == id1));
}

#[test]
fn test_bond_reference_equality() {
    let ref1 = BondReference { atom_id1: 1, atom_id2: 2 };
    let ref2 = BondReference { atom_id1: 2, atom_id2: 1 };
    let ref3 = BondReference { atom_id1: 1, atom_id2: 3 };
    
    assert_eq!(ref1, ref2, "Bond references should be equal regardless of order");
    assert_ne!(ref1, ref3, "Different bonds should not be equal");
}

#[test]
fn test_get_num_of_bonds_no_duplicates() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let id3 = structure.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    
    structure.add_bond(id1, id2, 1);
    structure.add_bond(id2, id3, 1);
    
    assert_eq!(structure.get_num_of_bonds(), 2, "Should count each bond only once");
}

#[test]
fn test_bonds_with_multiplicity() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    
    structure.add_bond(id1, id2, 2); // Double bond
    
    let atom1 = structure.get_atom(id1).unwrap();
    let bond = atom1.bonds.iter().find(|b| b.other_atom_id() == id2).unwrap();
    assert_eq!(bond.bond_order(), 2);
}

// ============================================================================
// Group 3: Selection Management
// ============================================================================

#[test]
fn test_select_and_deselect_atoms() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    assert!(!structure.has_selected_atoms());
    
    // Select atom using the select() API
    structure.select(&vec![id], &vec![], SelectModifier::Replace);
    assert!(structure.has_selected_atoms());
    
    let atom = structure.get_atom(id).unwrap();
    assert!(atom.is_selected());
    
    // Deselect by replacing with empty selection
    structure.select(&vec![], &vec![], SelectModifier::Replace);
    assert!(!structure.has_selected_atoms());
}

#[test]
fn test_select_bond_via_decorator() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    structure.add_bond(id1, id2, 1);
    
    let bond_ref = BondReference { atom_id1: id1, atom_id2: id2 };
    structure.select_bond(&bond_ref);
    
    assert!(structure.decorator().is_bond_selected(&bond_ref));
    assert!(structure.decorator().has_selected_bonds());
}

#[test]
fn test_delete_selected_atoms_clears_selection() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    structure.select(&vec![id], &vec![], SelectModifier::Replace);
    assert!(structure.has_selected_atoms());
    
    structure.delete_atom(id);
    assert!(!structure.has_selected_atoms());
}

#[test]
fn test_has_selection_includes_bonds() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    structure.add_bond(id1, id2, 1);
    
    assert!(!structure.has_selection());
    
    let bond_ref = BondReference { atom_id1: id1, atom_id2: id2 };
    structure.select_bond(&bond_ref);
    
    assert!(structure.has_selection());
    assert!(!structure.has_selected_atoms());
}

#[test]
fn test_clear_bond_selection() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    structure.add_bond(id1, id2, 1);
    
    let bond_ref = BondReference { atom_id1: id1, atom_id2: id2 };
    structure.select_bond(&bond_ref);
    assert!(structure.decorator().has_selected_bonds());
    
    structure.decorator_mut().clear_bond_selection();
    assert!(!structure.decorator().has_selected_bonds());
}

#[test]
fn test_clear_selection_clears_all() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    structure.add_bond(id1, id2, 1);
    
    structure.select(&vec![id1], &vec![], SelectModifier::Replace);
    let bond_ref = BondReference { atom_id1: id1, atom_id2: id2 };
    structure.select_bond(&bond_ref);
    
    assert!(structure.has_selection());
    
    // Clear selection by selecting empty sets
    structure.select(&vec![], &vec![], SelectModifier::Replace);
    
    assert!(!structure.has_selection());
    assert!(!structure.has_selected_atoms());
    assert!(!structure.decorator().has_selected_bonds());
}

// ============================================================================
// Group 4: Decorator Functionality
// ============================================================================

#[test]
fn test_decorator_atom_display_states() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomDisplayState;
    
    structure.decorator_mut().set_atom_display_state(id, AtomDisplayState::Marked);
    let state = structure.decorator().get_atom_display_state(id);
    
    // Check state using matches! since AtomDisplayState doesn't derive PartialEq
    assert!(matches!(state, AtomDisplayState::Marked));
}

#[test]
fn test_decorator_from_selected_node() {
    let mut structure = AtomicStructure::new();
    
    assert!(!structure.decorator().from_selected_node);
    
    structure.decorator_mut().from_selected_node = true;
    assert!(structure.decorator().from_selected_node);
}

#[test]
fn test_decorator_selection_transform() {
    let mut structure = AtomicStructure::new();
    
    assert!(structure.decorator().selection_transform.is_none());
    
    use rust_lib_flutter_cad::util::transform::Transform;
    use glam::f64::DQuat;
    
    let transform = Transform::new(DVec3::new(1.0, 2.0, 3.0), DQuat::IDENTITY);
    structure.decorator_mut().selection_transform = Some(transform.clone());
    
    assert!(structure.decorator().selection_transform.is_some());
}

#[test]
fn test_set_atom_hydrogen_passivation() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    let atom = structure.get_atom(id).unwrap();
    assert!(!atom.is_hydrogen_passivation());
    
    structure.set_atom_hydrogen_passivation(id, true);
    
    let atom = structure.get_atom(id).unwrap();
    assert!(atom.is_hydrogen_passivation());
}

// ============================================================================
// Group 5: Atom Field Setters
// ============================================================================

#[test]
fn test_set_atom_atomic_number() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    structure.set_atom_atomic_number(id, 8);
    
    let atom = structure.get_atom(id).unwrap();
    assert_eq!(atom.atomic_number, 8);
}

#[test]
fn test_set_atom_in_crystal_depth() {
    let mut structure = AtomicStructure::new();
    let id = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    
    structure.set_atom_in_crystal_depth(id, 5.5);
    
    let atom = structure.get_atom(id).unwrap();
    assert_eq!(atom.in_crystal_depth, 5.5);
}

#[test]
fn test_set_atom_position_with_small_change() {
    let mut structure = AtomicStructure::new();
    let pos = DVec3::new(1.0, 2.0, 3.0);
    let id = structure.add_atom(6, pos);
    
    // Very small change (below epsilon)
    let new_pos = DVec3::new(1.0, 2.0, 3.0 + 1e-6);
    let result = structure.set_atom_position(id, new_pos);
    
    // Should still return true but position might not change
    assert!(result);
}

#[test]
fn test_setters_with_invalid_id() {
    let mut structure = AtomicStructure::new();
    
    // These should not panic, just silently do nothing
    structure.set_atom_atomic_number(999, 8);
    structure.set_atom_in_crystal_depth(999, 5.0);
    structure.set_atom_hydrogen_passivation(999, true);
    
    // set_atom_position returns bool
    let result = structure.set_atom_position(999, DVec3::new(0.0, 0.0, 0.0));
    assert!(!result);
}

// ============================================================================
// Group 6: Complex Scenarios
// ============================================================================

#[test]
fn test_create_molecule_workflow() {
    let mut structure = AtomicStructure::new();
    
    // Add atoms
    let c1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let h1 = structure.add_atom(1, DVec3::new(-0.5, 0.5, 0.0));
    
    // Add bonds
    structure.add_bond(c1, c2, 1);
    structure.add_bond(c1, h1, 1);
    
    // Select some atoms
    structure.select(&vec![c1], &vec![], SelectModifier::Replace);
    
    // Delete selected
    structure.delete_atom(c1);
    
    // Verify structure is still valid
    assert_eq!(structure.get_num_of_atoms(), 2);
    assert!(verify_bonds_bidirectional(&structure));
    assert!(verify_grid_consistency(&structure));
}

#[test]
fn test_transform_selected_atoms() {
    let mut structure = AtomicStructure::new();
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id2 = structure.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    
    structure.select(&vec![id1, id2], &vec![], SelectModifier::Replace);
    
    use rust_lib_flutter_cad::util::transform::Transform;
    use glam::f64::DQuat;
    
    let translation = DVec3::new(5.0, 0.0, 0.0);
    structure.transform_atom(id1, &DQuat::IDENTITY, &translation);
    structure.transform_atom(id2, &DQuat::IDENTITY, &translation);
    
    let atom1 = structure.get_atom(id1).unwrap();
    assert_eq!(atom1.position.x, 5.0);
    
    assert!(verify_grid_consistency(&structure));
}

#[test]
fn test_rebuild_after_deletions() {
    let mut structure = create_test_molecule();
    let initial_count = structure.get_num_of_atoms();
    
    // Delete some atoms
    let ids_to_delete: Vec<u32> = structure.iter_atoms()
        .take(2)
        .map(|(id, _)| *id)
        .collect();
    
    for id in ids_to_delete {
        structure.delete_atom(id);
    }
    
    assert!(structure.get_num_of_atoms() < initial_count);
    assert!(verify_bonds_bidirectional(&structure));
    assert!(verify_grid_consistency(&structure));
}

#[test]
fn test_large_structure_performance() {
    let mut structure = AtomicStructure::new();
    
    // Create a grid of atoms
    for i in 0..10 {
        for j in 0..10 {
            for k in 0..10 {
                let pos = DVec3::new(i as f64 * 2.0, j as f64 * 2.0, k as f64 * 2.0);
                structure.add_atom(6, pos);
            }
        }
    }
    
    assert_eq!(structure.get_num_of_atoms(), 1000);
    
    // Test spatial queries are fast
    let center = DVec3::new(10.0, 10.0, 10.0);
    let nearby = structure.get_atoms_in_radius(&center, 5.0);
    assert!(nearby.len() > 0);
    assert!(nearby.len() < 1000);
}

// ============================================================================
// Group 7: Invariant Validation
// ============================================================================

#[test]
fn test_invariant_all_bonds_bidirectional() {
    let structure = create_test_molecule();
    assert!(verify_bonds_bidirectional(&structure), "All bonds must be bidirectional");
}

#[test]
fn test_invariant_grid_matches_positions() {
    let structure = create_test_molecule();
    assert!(verify_grid_consistency(&structure), "Grid must match atom positions");
}

#[test]
fn test_invariant_no_dangling_bonds() {
    let structure = create_test_molecule();
    
    for (_, atom) in structure.iter_atoms() {
        for bond in &atom.bonds {
            let other_id = bond.other_atom_id();
            assert!(structure.get_atom(other_id).is_some(), 
                "Bond references atom {} which doesn't exist", other_id);
        }
    }
}

#[test]
fn test_deleted_ids_not_reused() {
    let mut structure = AtomicStructure::new();
    
    // Add first atom (should get ID 1)
    let id1 = structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    assert_eq!(id1, 1, "First atom should have ID 1");
    assert!(structure.get_atom(id1).is_some(), "Atom 1 should exist");
    
    // Delete first atom
    structure.delete_atom(id1);
    assert!(structure.get_atom(id1).is_none(), "Atom 1 should be deleted");
    
    // Add second atom (should get ID 2, NOT reuse ID 1)
    let id2 = structure.add_atom(8, DVec3::new(1.0, 0.0, 0.0));
    assert_eq!(id2, 2, "Second atom should have ID 2, not reuse ID 1");
    
    // Verify ID 1 is still None (deleted, not reused)
    assert!(structure.get_atom(id1).is_none(), 
        "Deleted atom ID 1 should remain None, not be reused");
    
    // Verify ID 2 exists
    assert!(structure.get_atom(id2).is_some(), "Atom 2 should exist");
    
    // Verify atom count is 1 (only the second atom exists)
    assert_eq!(structure.get_num_of_atoms(), 1, "Should have 1 atom after delete and add");
}




