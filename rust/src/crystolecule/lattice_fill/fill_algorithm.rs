// Core algorithm imports
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::motif::Motif;
use crate::geo_tree::GeoNode;
use crate::util::daabox::DAABox;
use glam::f64::DVec3;
use std::collections::HashMap;
use crate::geo_tree::batched_implicit_evaluator::BatchedImplicitEvaluator;
use std::collections::HashSet;
use glam::i32::IVec3;
use crate::util::box_subdivision::subdivide_daabox;
use super::placed_atom_tracker::PlacedAtomTracker;

use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use crate::crystolecule::atomic_structure_utils::{remove_lone_atoms, remove_single_bond_atoms};
use super::surface_reconstruction::reconstruct_surface;
use super::hydrogen_passivation::{hydrogen_passivate};
use crate::util::timer::Timer;

// Import configuration types
use super::config::{LatticeFillConfig, LatticeFillOptions, LatticeFillResult, LatticeFillStatistics};

// ============================================================================
// Constants
// ============================================================================

/// Threshold for SDF sampling - atoms placed where SDF <= this value
const CRYSTAL_SAMPLE_THRESHOLD: f64 = 0.01;

/// Conservative epsilon for numerical stability in box overlaps
const CONSERVATIVE_EPSILON: f64 = 0.001;

/// Minimum size for fill box before stopping subdivision
const SMALLEST_FILL_BOX_SIZE: f64 = 4.9;

// ============================================================================
// Main Algorithm Entry Point
// ============================================================================

/// Fills a crystal geometry with atoms based on a periodic motif.
/// 
/// This is the main entry point for the lattice filling algorithm.
/// It takes configuration and options, and returns the resulting atomic structure.
/// 
/// # Arguments
/// * `config` - Configuration containing unit cell, motif, geometry, etc.
/// * `options` - Options controlling behavior (passivation, reconstruction, etc.)
/// * `fill_region` - The region in real space to fill (passed separately as it changes during recursion)
/// 
/// # Returns
/// * `LatticeFillResult` containing the filled atomic structure
pub fn fill_lattice(
  config: &LatticeFillConfig,
  options: &LatticeFillOptions,
  fill_region: &DAABox,
) -> LatticeFillResult {
  let _timer = Timer::new("LatticeFill total");

  let mut atomic_structure = AtomicStructure::new();
  let mut statistics = LatticeFillStatistics::new();
  let mut atom_tracker = PlacedAtomTracker::new();

  // Create a set to track which motif cells have been processed to avoid duplicates
  let mut processed_cells = HashSet::new();

  // Create batched evaluator with multi-threading enabled and pending atom data storage
  let mut batched_evaluator = BatchedImplicitEvaluator::new_with_threading(&config.geometry, true);
  let mut pending_atoms = Vec::new();

  {
    let _fill_timer = Timer::new("LatticeFill geometry filling");
    fill_box(
      &config.unit_cell,
      &config.geometry,
      &config.motif,
      &config.motif_offset,
      fill_region,
      &mut atomic_structure,
      &mut statistics,
      &config.parameter_element_values,
      &mut atom_tracker,
      &mut processed_cells,
      &mut batched_evaluator,
      &mut pending_atoms);
  }

  {
    let _batch_timer = Timer::new("LatticeFill batch evaluation");
    // Process all batched evaluations
    let sdf_results = batched_evaluator.flush();
    statistics.batched_evaluations += sdf_results.len() as i32;
    
    // Process results and add atoms
    for (i, &sdf_value) in sdf_results.iter().enumerate() {
      let atom_data = &pending_atoms[i];
      
      // Add atom if we are within the geometry
      if sdf_value <= CRYSTAL_SAMPLE_THRESHOLD {
        let atom_id = atomic_structure.add_atom(atom_data.atomic_number, atom_data.position);
        
        // Set the depth value based on SDF (negative SDF means inside the geometry)
        // Convert to f32 for memory efficiency and negate to make depth positive inside geometry
        let depth = (-sdf_value) as f32;
        atomic_structure.set_atom_depth(atom_id, depth);
        
        // Update depth statistics
        let depth_f64 = depth as f64;
        statistics.total_depth += depth_f64;
        if depth_f64 > statistics.max_depth {
          statistics.max_depth = depth_f64;
        }
        
        atom_tracker.record_atom(atom_data.motif_pos, atom_data.site_index, atom_id);
        statistics.atoms += 1;
      }
    }
  }

  {
    let _bond_timer = Timer::new("LatticeFill bond creation");
    // Create bonds after all atoms have been placed
    create_bonds(config, &atom_tracker, &mut atomic_structure, &mut statistics);
  }
  
  {
    let _cleanup_timer = Timer::new("LatticeFill cleanup and passivation");
    // Remove lone atoms before hydrogen passivation (passivation will bond them)
    remove_lone_atoms(&mut atomic_structure);

    // Remove single bond atoms before hydrogen passivation if enabled
    // This is useful for removing methyl groups on crystal surfaces
    // Recursive removal: keeps removing until no more single-bond atoms exist
    if options.remove_single_bond_atoms {
      remove_single_bond_atoms(&mut atomic_structure, true);
    }
    
    // Apply surface reconstruction if enabled (before hydrogen passivation)
    if options.reconstruct_surface {
      let _reconstruction_timer = Timer::new("LatticeFill surface reconstruction");
      let reconstruction_count = reconstruct_surface(
        &mut atomic_structure,
        &atom_tracker,
        &config.motif, 
        &config.unit_cell, 
        &config.parameter_element_values,
        options.remove_single_bond_atoms,
        options.hydrogen_passivation,
        options.invert_phase
      );
      statistics.surface_reconstructions = reconstruction_count as i32;
    }
    
    // Apply hydrogen passivation after bonds are created and lone atoms removed
    if options.hydrogen_passivation {
      hydrogen_passivate(config, &atom_tracker, &mut atomic_structure, &mut statistics);
    }
  }

  statistics.log_statistics();

  LatticeFillResult {
    atomic_structure,
  }
}

// ============================================================================
// Helper Functions  
// ============================================================================

// Placeholder for PendingAtomData - will be properly defined later
struct PendingAtomData {
  position: DVec3,
  atomic_number: i16,
  motif_pos: IVec3,
  site_index: usize,
}

/// Converts from motif space coordinates to real space coordinates.
/// Motif space is fractional lattice space offset by motif_offset.
fn motif_to_real(unit_cell: &UnitCellStruct, motif_offset: &DVec3, motif_coords: &DVec3) -> DVec3 {
  // Convert from motif space to canonical lattice space
  let lattice_coords = motif_coords + motif_offset;
  // Convert from lattice space to real space
  unit_cell.dvec3_lattice_to_real(&lattice_coords)
}

/// Converts from real space coordinates to motif space coordinates.
/// Motif space is fractional lattice space offset by motif_offset.
fn real_to_motif(unit_cell: &UnitCellStruct, motif_offset: &DVec3, real_coords: &DVec3) -> DVec3 {
  // Convert from real space to canonical lattice space
  let lattice_coords = unit_cell.real_to_dvec3_lattice(real_coords);
  // Convert from canonical lattice space to motif space
  lattice_coords - motif_offset
}

// Helper method to calculate the motif-space box that covers the real-space box
fn calculate_motif_space_box(
  unit_cell: &UnitCellStruct,
  motif_offset: &DVec3,
  start_pos: &DVec3,
  size: &DVec3
) -> (IVec3, IVec3) {
  let end_pos = start_pos + size;
  
  // Convert the corners of the real-space box to motif coordinates
  let start_motif = real_to_motif(unit_cell, motif_offset, start_pos);
  let end_motif = real_to_motif(unit_cell, motif_offset, &end_pos);
  
  // Find the minimum and maximum motif coordinates in each dimension
  // Be conservative by expanding the range slightly to account for numerical errors
  let min_x = (start_motif.x.min(end_motif.x) - CONSERVATIVE_EPSILON).floor() as i32;
  let max_x = (start_motif.x.max(end_motif.x) + CONSERVATIVE_EPSILON).ceil() as i32;
  let min_y = (start_motif.y.min(end_motif.y) - CONSERVATIVE_EPSILON).floor() as i32;
  let max_y = (start_motif.y.max(end_motif.y) + CONSERVATIVE_EPSILON).ceil() as i32;
  let min_z = (start_motif.z.min(end_motif.z) - CONSERVATIVE_EPSILON).floor() as i32;
  let max_z = (start_motif.z.max(end_motif.z) + CONSERVATIVE_EPSILON).ceil() as i32;
  
  let motif_min = IVec3::new(min_x, min_y, min_z);
  let motif_size = IVec3::new(
    max_x - min_x + 1,
    max_y - min_y + 1,
    max_z - min_z + 1
  );
  
  (motif_min, motif_size)
}

// Helper method to calculate the axis-aligned bounding box of a unit cell
// A unit cell is a parallelepiped defined by vectors a, b, c from a base position
// We need to find the AABB that contains all 8 corners of this parallelepiped
fn calculate_unit_cell_aabb(
  cell_real_pos: &DVec3,
  unit_cell: &UnitCellStruct
) -> DAABox {
  // Calculate all 8 corners of the unit cell parallelepiped
  let corners = [
    *cell_real_pos,                                           // (0,0,0)
    *cell_real_pos + unit_cell.a,                            // (1,0,0)
    *cell_real_pos + unit_cell.b,                            // (0,1,0)
    *cell_real_pos + unit_cell.c,                            // (0,0,1)
    *cell_real_pos + unit_cell.a + unit_cell.b,              // (1,1,0)
    *cell_real_pos + unit_cell.a + unit_cell.c,              // (1,0,1)
    *cell_real_pos + unit_cell.b + unit_cell.c,              // (0,1,1)
    *cell_real_pos + unit_cell.a + unit_cell.b + unit_cell.c // (1,1,1)
  ];

  // Find the min and max coordinates across all corners
  let mut min = corners[0];
  let mut max = corners[0];
  
  for corner in &corners[1..] {
    min = DVec3::new(
      min.x.min(corner.x),
      min.y.min(corner.y),
      min.z.min(corner.z)
    );
    max = DVec3::new(
      max.x.max(corner.x),
      max.y.max(corner.y),
      max.z.max(corner.z)
    );
  }

  DAABox::from_min_max(min, max)
}

// Helper method to check if a motif cell overlaps with the real-space box
fn cell_overlaps_with_box(
  cell_real_pos: &DVec3,
  unit_cell: &UnitCellStruct,
  query_box: &DAABox
) -> bool {
  // Calculate the axis-aligned bounding box of the unit cell
  // This correctly handles rotated/skewed unit cells by considering all 8 corners
  let cell_aabb = calculate_unit_cell_aabb(cell_real_pos, unit_cell);
  
  // Use conservative overlap to ensure we don't miss cells due to numerical errors
  cell_aabb.conservative_overlap(query_box, CONSERVATIVE_EPSILON)
}

/// Fills the specified box with atoms using subdivision optimization.
/// Recursively subdivides the box to avoid processing huge empty spaces.
#[allow(clippy::too_many_arguments)]
fn fill_box(
  unit_cell: &UnitCellStruct,
  geo_tree_root: &GeoNode,
  motif: &Motif,
  motif_offset: &DVec3,
  box_to_fill: &DAABox,
  atomic_structure: &mut AtomicStructure,
  statistics: &mut LatticeFillStatistics,
  parameter_element_values: &HashMap<String, i16>,
  atom_tracker: &mut PlacedAtomTracker,
  processed_cells: &mut HashSet<IVec3>,
  batched_evaluator: &mut BatchedImplicitEvaluator,
  pending_atoms: &mut Vec<PendingAtomData>,
) {
    
  statistics.fill_box_calls += 1;
  let box_center = box_to_fill.center();

  // Evaluate SDF at the box center
  let sdf_value = geo_tree_root.implicit_eval_3d(&box_center);
  statistics.non_batched_evaluations += 1;

  let box_size = box_to_fill.size();
  let half_diagonal = box_size.length() / 2.0;

  // If SDF value is greater than half diagonal plus a threshold, there is no atom in this box
  if sdf_value > half_diagonal + CRYSTAL_SAMPLE_THRESHOLD + CONSERVATIVE_EPSILON {
    return;
  }

  // If SDF value is less than -half diagonal, the whole box is filled
  let filled = sdf_value < (-half_diagonal - CONSERVATIVE_EPSILON);

  // Determine if we should subdivide in each dimension
  let should_subdivide_x = box_size.x >= 2.0 * SMALLEST_FILL_BOX_SIZE;
  let should_subdivide_y = box_size.y >= 2.0 * SMALLEST_FILL_BOX_SIZE;
  let should_subdivide_z = box_size.z >= 2.0 * SMALLEST_FILL_BOX_SIZE;

  // If the whole box is filled or we can't subdivide in any direction,
  // we need to actually do the filling for this box
  if filled || (!should_subdivide_x && !should_subdivide_y && !should_subdivide_z) {
    do_fill_box(
      unit_cell,
      motif,
      motif_offset,
      box_to_fill,
      atomic_structure,
      statistics,
      parameter_element_values,
      atom_tracker,
      processed_cells,
      batched_evaluator,
      pending_atoms
    );
    return;
  }

  // Otherwise, subdivide the box and recursively process each subdivision
  let subdivisions = subdivide_daabox(
    box_to_fill,
    should_subdivide_x,
    should_subdivide_y,
    should_subdivide_z
  );
  
  // Process each subdivision recursively
  for sub_box in subdivisions {
    fill_box(
      unit_cell,
      geo_tree_root,
      motif,
      motif_offset,
      &sub_box,
      atomic_structure,
      statistics,
      parameter_element_values,
      atom_tracker,
      processed_cells,
      batched_evaluator,
      pending_atoms
    );
  }
}

/// Fills the specified box with atoms.
/// Called by fill_box. It does the actual filling by iterating through motif cells.
#[allow(clippy::too_many_arguments)]
fn do_fill_box(
  unit_cell: &UnitCellStruct,
  motif: &Motif,
  motif_offset: &DVec3,
  box_to_fill: &DAABox,
  atomic_structure: &mut AtomicStructure,
  statistics: &mut LatticeFillStatistics,
  parameter_element_values: &HashMap<String, i16>,
  atom_tracker: &mut PlacedAtomTracker,
  processed_cells: &mut HashSet<IVec3>,
  batched_evaluator: &mut BatchedImplicitEvaluator,
  pending_atoms: &mut Vec<PendingAtomData>,
) {
  statistics.do_fill_box_calls += 1;
  let box_size = box_to_fill.size();
  statistics.do_fill_box_total_size += box_size;
  
  // Calculate the motif-space box that completely covers the real-space box
  let (motif_min, motif_size) = calculate_motif_space_box(unit_cell, motif_offset, &box_to_fill.min, &box_size);
  
  // Iterate through all motif cells in the calculated box
  for i in 0..motif_size.x {
    for j in 0..motif_size.y {
      for k in 0..motif_size.z {
        let motif_pos = motif_min + IVec3::new(i, j, k);
        
        // Convert motif position to real space to check if this cell overlaps with our box
        let motif_pos_dvec3 = DVec3::new(motif_pos.x as f64, motif_pos.y as f64, motif_pos.z as f64);
        let cell_real_pos = motif_to_real(unit_cell, motif_offset, &motif_pos_dvec3);
        
        // Check if this motif cell has any overlap with the real-space box
        if cell_overlaps_with_box(&cell_real_pos, unit_cell, box_to_fill) {
          // Check if this motif cell has already been processed
          if !processed_cells.contains(&motif_pos) {
            // Mark this cell as processed
            processed_cells.insert(motif_pos);
            statistics.motif_cells_processed += 1;
            
            // Fill this motif cell with atoms from the motif
            fill_cell(
              unit_cell,
              motif,
              motif_offset,
              &motif_pos,
              &cell_real_pos,
              atomic_structure,
              statistics,
              parameter_element_values,
              atom_tracker,
              batched_evaluator,
              pending_atoms
            );
          }
        }
      }
    }
  }
}

/// Fills a single motif cell with atoms from the motif.
/// Adds atoms to the batched evaluator for later SDF evaluation.
#[allow(clippy::too_many_arguments)]
fn fill_cell(
  unit_cell: &UnitCellStruct,
  motif: &Motif,
  motif_offset: &DVec3,
  motif_pos: &IVec3,
  _cell_real_pos: &DVec3,
  _atomic_structure: &mut AtomicStructure,
  _statistics: &mut LatticeFillStatistics,
  parameter_element_values: &HashMap<String, i16>,
  _atom_tracker: &mut PlacedAtomTracker,
  batched_evaluator: &mut BatchedImplicitEvaluator,
  pending_atoms: &mut Vec<PendingAtomData>,
) {
  // Iterate through all sites in the motif
  for (site_index, site) in motif.sites.iter().enumerate() {
    // Determine the effective atomic number
    let effective_atomic_number = if site.atomic_number > 0 {
      // Positive atomic number - use directly
      site.atomic_number
    } else {
      // Negative atomic number - this is a parameter element
      // Find the parameter element by index (first parameter is -1, second is -2, etc.)
      let param_index = (-site.atomic_number - 1) as usize;
      if param_index < motif.parameters.len() {
        let param_name = &motif.parameters[param_index].name;
        match parameter_element_values.get(param_name) {
          Some(&atomic_number) => atomic_number,
          None => {
            // This should not happen if get_effective_parameter_element_values worked correctly
            // but use the default as fallback
            motif.parameters[param_index].default_atomic_number
          }
        }
      } else {
        // Invalid parameter index - skip this site
        continue;
      }
    };
    
    // Convert motif space position to real coordinates
    // The site position is in motif space relative to the motif cell
    let motif_pos_dvec3 = DVec3::new(motif_pos.x as f64, motif_pos.y as f64, motif_pos.z as f64);
    let site_motif_pos = motif_pos_dvec3 + site.position;
    let absolute_real_pos = motif_to_real(unit_cell, motif_offset, &site_motif_pos);
    
    // Add this point to the batch for later evaluation
    batched_evaluator.add_point(absolute_real_pos);
    
    // Store the associated data for this evaluation point
    pending_atoms.push(PendingAtomData {
      position: absolute_real_pos,
      atomic_number: effective_atomic_number,
      motif_pos: *motif_pos,
      site_index,
    });
  }
}

// Creates bonds between atoms based on motif bond definitions
// This is called after all atoms have been placed
fn create_bonds(
  config: &LatticeFillConfig,
  atom_tracker: &PlacedAtomTracker,
  atomic_structure: &mut AtomicStructure,
  statistics: &mut LatticeFillStatistics
) {
    // Iterate through all placed atoms
    for (address, atom_id) in atom_tracker.iter_atoms() {
      let lattice_pos = address.motif_space_pos;
      let site_index = address.site_index;
      
      // Use precomputed bonds_by_site1_index to only check bonds that start from this site
      // This is O(k) where k is the number of bonds per site, instead of O(N) where N is total bonds
      for &bond_index in &config.motif.bonds_by_site1_index[site_index] {
        let bond = &config.motif.bonds[bond_index];
        
        // This atom is the first site of the bond, try to find the second site
        let atom_id_2 = atom_tracker.get_atom_id_for_specifier(lattice_pos, &bond.site_2);
        
        if let Some(id2) = atom_id_2 {
          // Both atoms exist, create the bond using fast method
          // We can use add_bond vs. add_bond_checked because:
          // - Both atoms are guaranteed to exist (we just got their IDs from atom_tracker)
          // - No bond exists yet (this is the initial bond creation phase)
          // - We're creating new bonds, not updating existing ones
          atomic_structure.add_bond(atom_id, id2, bond.multiplicity as u8);
          statistics.bonds += 1;
        }
        // If second atom doesn't exist, skip the bond (will be handled by hydrogen passivation if enabled)
      }
    }
  }
