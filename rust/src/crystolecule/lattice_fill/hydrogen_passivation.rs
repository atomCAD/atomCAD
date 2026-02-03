use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_constants::ATOM_INFO;
use crate::crystolecule::motif::SiteSpecifier;
use glam::f64::DVec3;
use super::config::{LatticeFillConfig, LatticeFillStatistics};
use super::placed_atom_tracker::PlacedAtomTracker;

/// Standard C-H bond length in angstroms
const C_H_BOND_LENGTH: f64 = 1.09;

// Applies hydrogen passivation to dangling bonds
// This is called after bonds have been created
pub fn hydrogen_passivate(
  config: &LatticeFillConfig,
  atom_tracker: &PlacedAtomTracker,
  atomic_structure: &mut AtomicStructure,
  statistics: &mut LatticeFillStatistics
) {
    //println!("hydrogen_passivate called");

    // Iterate through all placed atoms
    for (address, atom_id) in atom_tracker.iter_atoms() {
      let lattice_pos = address.motif_space_pos;
      let site_index = address.site_index;
      
      // Check if this atom exists and isn''t already hydrogen passivated
      // (it might have been removed by remove_single_bond_atoms or passivated by surface reconstruction)
      // Extract needed fields immediately to avoid borrow checker conflicts
      let (atom_position, atom_atomic_number, actual_bond_count) = match atomic_structure.get_atom(atom_id) {
        None => continue, // Early exit - atom was removed, skip it
        Some(atom) => {
          if atom.is_hydrogen_passivation() {
            continue; // Skip - atom already passivated by surface reconstruction
          }
          (atom.position, atom.atomic_number, atom.bonds.len())
        }
      };
      
      // Optimization: Check if atom has all expected bonds from motif
      // If so, skip expensive dangling bond checking
      let expected_bond_count = config.motif.bonds_by_site1_index[site_index].len() 
                              + config.motif.bonds_by_site2_index[site_index].len();
      if actual_bond_count == expected_bond_count {
        continue; // All bonds present - no passivation needed
      }
      
      // Case 1: Check bonds where this atom is the first site (optimized with precomputed index)
      // Use precomputed bonds_by_site1_index to only check bonds that start from this site
      for &bond_index in &config.motif.bonds_by_site1_index[site_index] {
        let bond = &config.motif.bonds[bond_index];
        
        // This atom is the first site of the bond, try to find the second site
        let atom_id_2 = atom_tracker.get_atom_id_for_specifier(lattice_pos, &bond.site_2);
        
        // Check if second atom is missing (not in tracker OR in tracker but removed from structure)
        let is_dangling = match atom_id_2 {
          None => true, // Not in tracker - definitely dangling
          Some(id) => atomic_structure.get_atom(id).is_none(), // In tracker but removed from structure
        };
        
        if is_dangling {
          // Second atom doesn''t exist - this is a dangling bond that needs to be passivated
          //println!("dangling bond found (first site exists, second doesn''t)");
          
          hydrogen_passivate_dangling_bond(
            config,
            &bond.site_1,
            &bond.site_2,
            atom_id,
            atom_position,
            atom_atomic_number,
            atomic_structure,
            statistics
          );
        }
      }
      
      // Case 2: Check bonds where this atom is the second site (optimized with precomputed index)
      // Use precomputed bonds_by_site2_index to only check bonds that end at this site
      for &bond_index in &config.motif.bonds_by_site2_index[site_index] {
        let bond = &config.motif.bonds[bond_index];
        
        // We need to calculate where this atom would be if it were the second site
        let second_site_base_pos = lattice_pos - bond.site_2.relative_cell;
        
        // This atom is the second site of the bond, try to find the first site
        let atom_id_1 = atom_tracker.get_atom_id_for_specifier(second_site_base_pos, &bond.site_1);
        
        // Check if first atom is missing (not in tracker OR in tracker but removed from structure)
        let is_dangling = match atom_id_1 {
          None => true, // Not in tracker - definitely dangling
          Some(id) => atomic_structure.get_atom(id).is_none(), // In tracker but removed from structure
        };
        
        if is_dangling {
          // First atom doesn''t exist - this is a dangling bond that needs to be passivated
          //println!("dangling bond found (second site exists, first doesn''t)");
          
          hydrogen_passivate_dangling_bond(
            config,
            &bond.site_2,
            &bond.site_1,
            atom_id,
            atom_position,
            atom_atomic_number,
            atomic_structure,
            statistics
          );
        }
      }
    }
}

// Helper method to passivate a single dangling bond with hydrogen
// found_site: the site that exists in the crystal
// not_found_site: the site that is missing and needs to be passivated
// found_atom_id: ID of the existing atom
// found_atom_position: position of the existing atom
// found_atom_atomic_number: atomic number of the existing atom
#[allow(clippy::too_many_arguments)]
fn hydrogen_passivate_dangling_bond(
  config: &LatticeFillConfig,
  found_site: &SiteSpecifier,
  not_found_site: &SiteSpecifier,
  found_atom_id: u32,
  found_atom_position: DVec3,
  found_atom_atomic_number: i16,
  atomic_structure: &mut AtomicStructure,
  statistics: &mut LatticeFillStatistics
) {
    // Calculate the relative position of not_found_site relative to found_site in motif space
    let found_site_pos = config.motif.sites[found_site.site_index].position + 
      found_site.relative_cell.as_dvec3();
    let not_found_site_pos = config.motif.sites[not_found_site.site_index].position + 
      not_found_site.relative_cell.as_dvec3();

    let relative_motif_pos = not_found_site_pos - found_site_pos;
    
    // Convert the relative position from motif space to real space direction
    let real_space_direction = config.unit_cell.dvec3_lattice_to_real(&relative_motif_pos);
    
    // Calculate proper bond length based on atomic radii
    let bond_length = if found_atom_atomic_number == 6 {
      // Special case for C-H bonds
      C_H_BOND_LENGTH
    } else {
      // General case: sum of covalent radii
      let atom_1_radius = ATOM_INFO.get(&(found_atom_atomic_number as i32))
        .map(|info| info.covalent_radius)
        .unwrap_or(0.7); // Default radius if not found
      let hydrogen_radius = ATOM_INFO.get(&1)
        .map(|info| info.covalent_radius)
        .unwrap_or(0.31); // Default hydrogen radius
      atom_1_radius + hydrogen_radius
    };
    
    // Normalize the direction and place hydrogen at proper bond length
    let normalized_direction = real_space_direction.normalize();
    let hydrogen_pos = found_atom_position + normalized_direction * bond_length;

    // Add hydrogen atom (atomic number 1) - depth remains 0.0 by default
    let hydrogen_id = atomic_structure.add_atom(1, hydrogen_pos);
    
    // Update depth statistics for hydrogen (depth = 0.0)
    statistics.total_depth += 0.0;
    // Note: max_depth doesn''t need updating since hydrogen depth is 0.0
    
    // Create bond between original atom and hydrogen
    atomic_structure.add_bond(found_atom_id, hydrogen_id, 1); // Single bond
    
    statistics.bonds += 1;
    statistics.atoms += 1; // Count the hydrogen atom
}
