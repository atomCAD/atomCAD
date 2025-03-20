use crate::common::atomic_structure::AtomicStructure;
use crate::common::common_constants::ATOM_INFO;
use crate::common::common_constants::DEFAULT_ATOM_INFO;
use crate::common::common_constants::MAX_SUPPORTED_ATOMIC_RADIUS;

use glam::f64::DVec3;
use std::collections::HashSet;

// Bond distance multiplier - slightly larger than 1.0 to account for variations in bond distances
const BOND_DISTANCE_MULTIPLIER: f64 = 1.15;

pub fn auto_create_bonds(structure: &mut AtomicStructure) {
    // Track bonds we've already created to avoid duplicates
    let mut processed_pairs: HashSet<(u64, u64)> = HashSet::new();

    let atom_ids: Vec<u64> = structure.atoms.keys().cloned().collect();
    
    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            let atom_pos = atom.position;
            let atom_radius = ATOM_INFO.get(&atom.atomic_number)
                .unwrap_or(&DEFAULT_ATOM_INFO)
                .radius;
            
            // Get maximum possible bond distance for this atom
            // We need to use a larger radius to find all potential bonds
            // since we don't know the radius of the other atoms yet
            let max_search_radius = (atom_radius + MAX_SUPPORTED_ATOMIC_RADIUS ) * BOND_DISTANCE_MULTIPLIER;
            
            let nearby_atoms = structure.get_atoms_in_radius(&atom_pos, max_search_radius);
            
            // Check each nearby atom
            for &nearby_atom_id in &nearby_atoms {
                // Skip self
                if nearby_atom_id == atom_id {
                    continue;
                }
                
                // Create a canonical pair representation to avoid processing the same bond twice
                let bond_pair = if atom_id < nearby_atom_id {
                    (atom_id, nearby_atom_id)
                } else {
                    (nearby_atom_id, atom_id)
                };
                
                // Skip if we've already processed this pair
                if processed_pairs.contains(&bond_pair) {
                    continue;
                }
                
                // Process the nearby atom
                if let Some(nearby_atom) = structure.get_atom(nearby_atom_id) {
                    let nearby_atom_radius = ATOM_INFO.get(&nearby_atom.atomic_number)
                        .unwrap_or(&DEFAULT_ATOM_INFO)
                        .radius;
                    
                    let distance = DVec3::distance(atom_pos, nearby_atom.position);                    
                    let max_bond_distance = (atom_radius + nearby_atom_radius) * BOND_DISTANCE_MULTIPLIER;
                    
                    // If the atoms are close enough, create a single bond
                    if distance <= max_bond_distance {
                        structure.add_bond(atom_id, nearby_atom_id, 1);                        
                        processed_pairs.insert(bond_pair);
                    }
                }
            }
        }
    }
}