use crate::common::atomic_structure::AtomicStructure;
use glam::DVec3;

/// Standard C-H bond length in Angstroms
const C_H_BOND_LENGTH: f64 = 1.09;

/// Tolerance for determining bond directions (to handle floating point precision)
const DIRECTION_TOLERANCE: f64 = 0.1;

/// Bond directions for primary atoms in diamond/zincblende structure
/// These are the 4 tetrahedral directions from primary atoms
const PRIMARY_DIRECTIONS: [DVec3; 4] = [
    DVec3::new(1.0, -1.0, -1.0),   // +1 -1 -1
    DVec3::new(-1.0, 1.0, -1.0),   // -1 +1 -1
    DVec3::new(-1.0, -1.0, 1.0),   // -1 -1 +1
    DVec3::new(1.0, 1.0, 1.0),     // +1 +1 +1
];

/// Bond directions for secondary atoms in diamond/zincblende structure
/// These are the 4 tetrahedral directions from secondary atoms
const SECONDARY_DIRECTIONS: [DVec3; 4] = [
    DVec3::new(-1.0, 1.0, 1.0),    // -1 +1 +1
    DVec3::new(1.0, -1.0, 1.0),    // +1 -1 +1
    DVec3::new(1.0, 1.0, -1.0),    // +1 +1 -1
    DVec3::new(-1.0, -1.0, -1.0),  // -1 -1 -1
];

/// Performs hydrogen passivation on a diamond or zincblende crystal structure.
/// 
/// This function assumes the AtomicStructure contains a diamond or zincblende crystal
/// without any existing hydrogen passivation. It adds hydrogen atoms to complete
/// the tetrahedral coordination of under-coordinated carbon atoms.
/// 
/// # Arguments
/// 
/// * `structure` - A mutable reference to the AtomicStructure to passivate
/// 
/// # Algorithm
/// 
/// 1. For each atom in the structure:
///    - Determine if it's a primary or secondary atom based on existing bond directions
///    - Find which tetrahedral directions are missing bonds
///    - Add hydrogen atoms in those missing directions
/// 
pub fn hydrogen_passivate_diamond(structure: &mut AtomicStructure) {
    // Collect all atom IDs to avoid borrowing issues during modification
    let atom_ids: Vec<u64> = structure.atoms.keys().cloned().collect();
    
    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            // Only passivate carbon atoms
            if atom.atomic_number != 6 {
                continue;
            }
            
            let atom_position = atom.position;
            
            // Get existing bond directions
            let existing_directions = get_existing_bond_directions(structure, atom_id);
            
            // Determine atom type (primary or secondary) and get expected directions
            let expected_directions = determine_atom_type_and_directions(&existing_directions);
            
            // Find missing directions
            let missing_directions = find_missing_directions(&expected_directions, &existing_directions);
            
            // Add hydrogen atoms in missing directions
            for direction in missing_directions {
                let hydrogen_position = atom_position + direction.normalize() * C_H_BOND_LENGTH;
                let hydrogen_id = structure.add_atom(1, hydrogen_position); // Atomic number 1 = Hydrogen
                structure.add_bond(atom_id, hydrogen_id, 1); // Single bond
            }
        }
    }
}

/// Gets the normalized directions of all bonds from a given atom
fn get_existing_bond_directions(structure: &AtomicStructure, atom_id: u64) -> Vec<DVec3> {
    let mut directions = Vec::new();
    
    if let Some(atom) = structure.get_atom(atom_id) {
        let atom_position = atom.position;
        
        // Check all bonds connected to this atom
        for &bond_id in &atom.bond_ids {
            if let Some(bond) = structure.get_bond(bond_id) {
                // Determine the other atom in the bond
                let other_atom_id = if bond.atom_id1 == atom_id {
                    bond.atom_id2
                } else {
                    bond.atom_id1
                };
                
                // Get the direction to the other atom
                if let Some(other_atom) = structure.get_atom(other_atom_id) {
                    let direction = (other_atom.position - atom_position).normalize();
                    directions.push(direction);
                }
            }
        }
    }
    
    directions
}

/// Determines whether an atom is primary or secondary based on its existing bond directions
/// Returns the expected directions for that atom type
fn determine_atom_type_and_directions(existing_directions: &[DVec3]) -> &'static [DVec3; 4] {
    if existing_directions.is_empty() {
        // If no existing bonds, default to primary (this shouldn't happen in a proper crystal)
        return &PRIMARY_DIRECTIONS;
    }
    
    // Check if any existing direction matches primary directions better than secondary
    let mut primary_matches = 0;
    let mut secondary_matches = 0;
    
    for existing_dir in existing_directions {
        // Find best match in primary directions
        let best_primary_match = PRIMARY_DIRECTIONS.iter()
            .map(|dir| existing_dir.dot(dir.normalize()))
            .fold(f64::NEG_INFINITY, f64::max);
        
        // Find best match in secondary directions
        let best_secondary_match = SECONDARY_DIRECTIONS.iter()
            .map(|dir| existing_dir.dot(dir.normalize()))
            .fold(f64::NEG_INFINITY, f64::max);
        
        if best_primary_match > best_secondary_match {
            primary_matches += 1;
        } else {
            secondary_matches += 1;
        }
    }
    
    // Return the directions for the atom type with more matches
    if primary_matches >= secondary_matches {
        &PRIMARY_DIRECTIONS
    } else {
        &SECONDARY_DIRECTIONS
    }
}

/// Finds which expected directions are missing based on existing bond directions
fn find_missing_directions(expected_directions: &[DVec3; 4], existing_directions: &[DVec3]) -> Vec<DVec3> {
    let mut missing = Vec::new();
    
    for &expected_dir in expected_directions {
        let expected_normalized = expected_dir.normalize();
        
        // Check if this direction is already occupied by an existing bond
        let is_occupied = existing_directions.iter().any(|&existing_dir| {
            // Check if the directions are similar (within tolerance)
            let dot_product = expected_normalized.dot(existing_dir);
            dot_product > (1.0 - DIRECTION_TOLERANCE) // Close to parallel
        });
        
        if !is_occupied {
            missing.push(expected_dir);
        }
    }
    
    missing
}
