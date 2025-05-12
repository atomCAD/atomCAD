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
    let mut processed_pairs: HashSet<(u64, u64)> = HashSet::new();

    let atom_ids: Vec<u64> = structure.atoms.keys().cloned().collect();
    
    let mut max_atom_radius = 0.0;
    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            let atom_radius = ATOM_INFO.get(&atom.atomic_number)
                .unwrap_or(&DEFAULT_ATOM_INFO)
                .radius;
            
            if atom_radius > max_atom_radius {
                max_atom_radius = atom_radius;
            }
        }
    }

    for &atom_id in &atom_ids {
        if let Some(atom) = structure.get_atom(atom_id) {
            let atom_pos = atom.position;
            let atom_radius = ATOM_INFO.get(&atom.atomic_number)
                .unwrap_or(&DEFAULT_ATOM_INFO)
                .radius;
            
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

/// Detects bonded substructures (connected components) in an AtomicStructure and organizes them into clusters.
/// Each connected component of atoms (where atoms are connected by bonds) will be placed in its own cluster.
///
/// # Parameters
///
/// * `structure` - The atomic structure to analyze and modify
///
pub fn detect_bonded_substructures(structure: &mut AtomicStructure) {

    let mut visited: HashSet<u64> = HashSet::new();
    let mut new_cluster_ids: Vec<u64> = Vec::new();
    
    let all_atom_ids: Vec<u64> = structure.atoms.keys().cloned().collect();
    
    // For each atom that hasn't been visited yet
    for &start_atom_id in &all_atom_ids {
        if visited.contains(&start_atom_id) {
            continue;
        }

        // Create a new cluster for this connected component if necessary
        let mut cluster_id: u64 = 1;
        if structure.clusters.len() == 1 && structure.clusters.values().next().unwrap().name == "default" {
            structure.clusters.values_mut().next().unwrap().name = format!("Cluster_1");
        } else {
            cluster_id = structure.obtain_next_cluster_id();
            structure.add_cluster_with_id(cluster_id, &format!("Cluster_{}", cluster_id));
        }
        
        // Perform depth-first search to find all connected atoms
        let mut stack: Vec<u64> = vec![start_atom_id];
        
        while let Some(atom_id) = stack.pop() {
            if visited.contains(&atom_id) {
                continue;
            }
            visited.insert(atom_id);
            structure.move_atom_to_cluster(atom_id, cluster_id);
            
            // Get the atom to access its bonds
            if let Some(atom) = structure.atoms.get(&atom_id) {
                // For each bond of the current atom
                for &bond_id in &atom.bond_ids {
                    if let Some(bond) = structure.bonds.get(&bond_id) {
                        let connected_atom_id = if bond.atom_id1 == atom_id {
                            bond.atom_id2
                        } else {
                            bond.atom_id1
                        };

                        if !visited.contains(&connected_atom_id) {
                            stack.push(connected_atom_id);
                        }
                    }
                }
            }
        }
    }

    structure.remove_empty_clusters();
    structure.calculate_all_clusters_default_frame_transforms();
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
    let selected_atom_ids: Vec<u64> = structure.atoms.iter()
        .filter(|(_, atom)| atom.selected)
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

