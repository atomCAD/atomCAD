use crate::common::atomic_structure::AtomicStructure;
use crate::common::common_constants::ATOM_INFO;
use crate::common::common_constants::DEFAULT_ATOM_INFO;
use crate::util::transform::Transform;

use glam::f64::{DVec3, DQuat};
use std::collections::HashSet;

// Bond distance multiplier - slightly larger than 1.0 to account for variations in bond distances
const BOND_DISTANCE_MULTIPLIER: f64 = 1.15;

pub fn auto_create_bonds(structure: &mut AtomicStructure) {
    // Track bonds we've already created to avoid duplicates
    let mut processed_pairs: HashSet<(u32, u32)> = HashSet::new();

    let atom_ids: Vec<u32> = structure.atom_ids().cloned().collect();
    
    let mut max_atom_radius = 0.0;
    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            let atom_radius = ATOM_INFO.get(&(atom.atomic_number as i32))
                .unwrap_or(&DEFAULT_ATOM_INFO)
                .covalent_radius;
            
            if atom_radius > max_atom_radius {
                max_atom_radius = atom_radius;
            }
        }
    }

    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            let atom_pos = atom.position;
            let atom_radius = ATOM_INFO.get(&(atom.atomic_number as i32))
                .unwrap_or(&DEFAULT_ATOM_INFO)
                .covalent_radius;
            
            // MAke the maximum possible bond distance for this atom the search radius.
            // We need to use the max atom radius
            // since we don't know the radius of the other atom here
            let search_radius = (atom_radius + max_atom_radius + 0.01) * BOND_DISTANCE_MULTIPLIER;
            
            let nearby_atoms = structure.get_atoms_in_radius(&atom_pos, search_radius);
            
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
                    let nearby_atom_radius = ATOM_INFO.get(&(nearby_atom.atomic_number as i32))
                        .unwrap_or(&DEFAULT_ATOM_INFO)
                        .covalent_radius;
                    
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


/// Calculates a transform that represents a local coordinate system aligned to selected atoms.
/// This transform can be used as a basis for translation/rotation gadgets.
///
/// # Parameters
///
/// * `structure` - The atomic structure containing selected atoms
///
/// # Returns
///
/// * `Option<Transform>` - A transform if atoms are selected, None otherwise
///   - Translation is set to the average position of selected atoms
///   - Rotation is set based on the number of selected atoms:
///     - 1 atom: Identity rotation
///     - 2 atoms: X-axis aligned with the vector between atoms
///     - 3 atoms: X-Z plane containing all three atom centers
///     - 4+ atoms: Identity rotation
///
pub fn calc_selection_transform(structure: &AtomicStructure) -> Option<Transform> {
    // Get selected atom IDs
    let selected_atom_ids: Vec<u32> = structure.iter_atoms()
        .filter(|(_, atom)| atom.is_selected())
        .map(|(id, _)| *id)
        .collect();

    if selected_atom_ids.is_empty() {
        return None;
    }

    // Get atom positions
    let mut atom_positions: Vec<DVec3> = Vec::new();
    for atom_id in &selected_atom_ids {
        if let Some(atom) = structure.get_atom(*atom_id) {
            atom_positions.push(atom.position);
        }
    }

    if atom_positions.is_empty() {
        return None;
    }

    // Initialize transform with default values
    let mut transform = Transform::default();

    // Calculate the average position of selected atoms for translation
    let avg_position = atom_positions.iter().fold(DVec3::ZERO, |acc, pos| acc + *pos) / atom_positions.len() as f64;
    transform.translation = avg_position;

    // Special case: for 4 or more atoms, use identity rotation
    if atom_positions.len() >= 4 || atom_positions.len() == 1 {
        transform.rotation = DQuat::IDENTITY;
    }
    // For 2 or 3 atoms, calculate a meaningful orientation
    else if atom_positions.len() >= 2 {
        // For two or more atoms, align X axis with the vector between first two atoms
        let x_axis = (atom_positions[1] - atom_positions[0]).normalize();
        
        // Default X axis in local space is (1,0,0)
        let local_x_axis = DVec3::new(1.0, 0.0, 0.0);
        
        // Calculate rotation to align local X axis with desired X axis
        transform.rotation = DQuat::from_rotation_arc(local_x_axis, x_axis);

        if atom_positions.len() == 3 {
            // Get the current X axis in global coordinates after the first rotation
            let global_x_axis = transform.rotation.mul_vec3(local_x_axis);
            
            // Vector from atom1 to atom3
            let atom1_to_atom3 = atom_positions[2] - atom_positions[0];
            
            // Project atom1_to_atom3 onto the X axis to get the component along X
            let projection = atom1_to_atom3.dot(global_x_axis) * global_x_axis;
            
            // Get the perpendicular component (this will be in the X-Z plane)
            let perpendicular = atom1_to_atom3 - projection;
            
            // Only proceed if the perpendicular component is significant
            if perpendicular.length_squared() > 0.00001 {
                // This will be our desired Z axis
                let new_z_axis = perpendicular.normalize();
                
                // Get the current Z axis in global coordinates
                let global_z_axis = transform.rotation.mul_vec3(DVec3::new(0.0, 0.0, 1.0));
                
                // Calculate angle between current Z and desired Z
                let angle = global_z_axis.angle_between(new_z_axis);
                
                // Determine rotation direction using cross product
                let cross = global_z_axis.cross(new_z_axis);
                let sign = if cross.dot(global_x_axis) < 0.0 { -1.0 } else { 1.0 };
                
                // Create rotation around X axis
                let x_rotation = DQuat::from_axis_angle(global_x_axis, sign * angle);
                
                // Apply this rotation to align Z axis properly
                transform.rotation = x_rotation * transform.rotation;
            }
        }
    }

    Some(transform)
}

/// Prints detailed information about all atoms in the AtomicStructure for debugging purposes.
/// Shows index, atom ID, atomic number, and number of bonds for each atom.
///
/// # Parameters
///
/// * `structure` - The atomic structure to analyze and print
///
pub fn print_atom_info(structure: &AtomicStructure) {
    println!("=== Atomic Structure Info ===");
    println!("Total atoms: {}", structure.get_num_of_atoms());
    println!("Total bonds: {}", structure.get_num_of_bonds());
    println!();
    
    // Collect atom IDs for consistent ordering
    let mut atom_ids: Vec<u32> = structure.atom_ids().cloned().collect();
    atom_ids.sort(); // Sort for consistent output
    
    println!("{:<6} {:<8} {:<12} {:<10}", "Index", "Atom ID", "Atomic Num", "Bond Count");
    println!("{:-<40}", "");
    
    for (index, &atom_id) in atom_ids.iter().enumerate() {
        if let Some(atom) = structure.get_atom(atom_id) {
            let bond_count = atom.bonds.len();
            println!("{:<6} {:<8} {:<12} {:<10}", 
                     index, 
                     atom_id, 
                     atom.atomic_number, 
                     bond_count);
        }
    }
    
    println!();
    
    // Print summary of bond count distribution
    let mut bond_count_distribution = std::collections::HashMap::new();
    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            let bond_count = atom.bonds.len();
            *bond_count_distribution.entry(bond_count).or_insert(0) += 1;
        }
    }
    
    println!("Bond count distribution:");
    let mut bond_counts: Vec<_> = bond_count_distribution.keys().cloned().collect();
    bond_counts.sort();
    
    for bond_count in bond_counts {
        let atom_count = bond_count_distribution[&bond_count];
        println!("  {} bonds: {} atoms", bond_count, atom_count);
    }
    
    println!("=============================");
}

pub fn remove_lone_atoms(structure: &mut AtomicStructure) {
    let lone_atoms: Vec<u32> = structure.atoms_values()
      .filter(|atom| atom.bonds.is_empty())
      .map(|atom| atom.id)
      .collect();

    for atom_id in lone_atoms {
      structure.delete_lone_atom(atom_id);
    }
}

/// Helper function that deletes a batch of atoms with at most one bond
/// and returns the neighbors that need to be checked in the next iteration.
/// 
/// IMPORTANT: Atoms in the list are assumed to have ≤1 bond at the START of this call.
/// During deletion, some atoms might go from 1 bond to 0 bonds, but they still get deleted.
/// This is why we do NOT re-check bond counts inside this function.
/// 
/// Returns: list of neighbor IDs to check next
fn delete_atoms_with_at_most_one_bond(
    structure: &mut AtomicStructure,
    atoms_to_delete: Vec<u32>,
) -> Vec<u32> {
    let mut all_neighbors = Vec::new();
    
    for atom_id in atoms_to_delete {
        // Get the atom
        let Some(atom) = structure.get_atom(atom_id) else {
            continue;
        };
        
        // Collect neighbor IDs before deletion
        for bond in &atom.bonds {
            all_neighbors.push(bond.other_atom_id());
        }
        
        // Delete the atom (this also removes all its bonds)
        structure.delete_atom(atom_id);
    }
    
    all_neighbors
}

pub fn remove_single_bond_atoms(structure: &mut AtomicStructure, recursive: bool) {
    // First iteration: find ALL atoms with at most one bond (0 or 1 bonds)
    let mut atoms_with_at_most_one_bond: Vec<u32> = structure.atoms_values()
        .filter(|atom| atom.bonds.len() <= 1)
        .map(|atom| atom.id)
        .collect();
    
    while !atoms_with_at_most_one_bond.is_empty() {
        // Delete the batch and get neighbors to check
        let all_neighbors = delete_atoms_with_at_most_one_bond(
            structure,
            atoms_with_at_most_one_bond,
        );
        
        // If not recursive, stop after first iteration
        if !recursive {
            break;
        }
        
        // For next iteration: check which neighbors now have at most one bond
        atoms_with_at_most_one_bond = all_neighbors.into_iter()
            .filter(|&neighbor_id| {
                structure.get_atom(neighbor_id)
                    .map(|atom| atom.bonds.len() <= 1)
                    .unwrap_or(false)
            })
            .collect();
    }
}