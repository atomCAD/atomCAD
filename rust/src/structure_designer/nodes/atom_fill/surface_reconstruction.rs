use crate::common::atomic_structure::{AtomicStructure, Atom};
use crate::structure_designer::evaluator::motif::Motif;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::common_constants::{DEFAULT_ZINCBLENDE_MOTIF, DIAMOND_UNIT_CELL_SIZE_ANGSTROM};
use crate::common::atomic_structure_utils::remove_single_bond_atoms;
use crate::common::common_constants::{
  DEBUG_CARBON_GRAY, DEBUG_CARBON_RED, DEBUG_CARBON_GREEN, DEBUG_CARBON_BLUE,
  DEBUG_CARBON_YELLOW, DEBUG_CARBON_MAGENTA, DEBUG_CARBON_CYAN, DEBUG_CARBON_ORANGE
};
use std::collections::HashMap;
use rustc_hash::FxHashMap;

/// Surface orientation classification for atoms.
/// 
/// This enum categorizes atoms based on their position and bonding:
/// - Bulk: Deep interior atoms (depth > threshold)
/// - Unknown: Surface atoms that don't fit standard (100) reconstruction pattern
/// - Surface variants: Atoms on {100} facets with specific surface normals
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceOrientation {
    Bulk,
    Unknown,
    Surface100,      // (100)
    SurfaceNeg100,   // (-100)
    Surface010,      // (010)
    SurfaceNeg010,   // (0-10)
    Surface001,      // (001)
    SurfaceNeg001,   // (00-1)
}

/// Depth threshold in Ångströms for classifying atoms as bulk.
/// Atoms deeper than this are not near any surface.
const BULK_DEPTH_THRESHOLD: f32 = 0.5;

/// Minimum magnitude for a bond component on an axis to be considered aligned.
/// Components below this threshold are considered insignificant.
const AXIS_ALIGNMENT_THRESHOLD: f64 = 0.5;

/// Debug flag: when true, replaces atoms with colored elements based on surface orientation.
/// This allows visual inspection of the classification in the 3D viewer.
/// Set to false for normal reconstruction operation.
const DEBUG_SURFACE_ORIENTATION: bool = true;

/// Maps each surface orientation to an atomic number for visual debugging.
/// Uses custom debug carbon elements with carbon radii but distinct colors.
/// All atoms will have the same size for consistent visualization.
fn get_debug_atomic_number(orientation: SurfaceOrientation) -> i32 {
  match orientation {
    SurfaceOrientation::Bulk => DEBUG_CARBON_GRAY,
    SurfaceOrientation::Unknown => DEBUG_CARBON_YELLOW,
    SurfaceOrientation::Surface100 => DEBUG_CARBON_RED,
    SurfaceOrientation::SurfaceNeg100 => DEBUG_CARBON_GREEN,
    SurfaceOrientation::Surface010 => DEBUG_CARBON_BLUE,
    SurfaceOrientation::SurfaceNeg010 => DEBUG_CARBON_ORANGE,
    SurfaceOrientation::Surface001 => DEBUG_CARBON_CYAN,
    SurfaceOrientation::SurfaceNeg001 => DEBUG_CARBON_MAGENTA,
  }
}

/// Classifies the surface orientation of a single atom.
/// 
/// This function determines whether an atom is bulk, on a surface, or has
/// an unknown/ambiguous orientation based on its depth and bonding pattern.
/// 
/// # Arguments
/// * `atom` - The atom to classify
/// * `structure` - The atomic structure (needed to check neighboring atoms)
/// 
/// # Returns
/// * The surface orientation classification for this atom
fn classify_atom_surface_orientation(
  atom: &Atom,
  structure: &AtomicStructure
) -> SurfaceOrientation {
  // Check if atom is deep in the bulk (> 0.5 Å from surface)
  if atom.in_crystal_depth > BULK_DEPTH_THRESHOLD {
    return SurfaceOrientation::Bulk;
  }

  // Surface atoms on {100} facets should have exactly 2 bonds (2 bulk neighbors, 2 dangling)
  if atom.bond_ids.len() != 2 {
    return SurfaceOrientation::Unknown;
  }

  // Get the two bonded neighbors
  let bond1 = structure.bonds.get(&atom.bond_ids[0]);
  let bond2 = structure.bonds.get(&atom.bond_ids[1]);
  
  if bond1.is_none() || bond2.is_none() {
    return SurfaceOrientation::Unknown;
  }
  
  let bond1 = bond1.unwrap();
  let bond2 = bond2.unwrap();
  
  // Get neighbor atom positions
  let neighbor1_id = if bond1.atom_id1 == atom.id { bond1.atom_id2 } else { bond1.atom_id1 };
  let neighbor2_id = if bond2.atom_id1 == atom.id { bond2.atom_id2 } else { bond2.atom_id1 };
  
  let neighbor1 = structure.atoms.get(&neighbor1_id);
  let neighbor2 = structure.atoms.get(&neighbor2_id);
  
  if neighbor1.is_none() || neighbor2.is_none() {
    return SurfaceOrientation::Unknown;
  }
  
  // Calculate normalized bond directions
  let dir1 = (neighbor1.unwrap().position - atom.position).normalize();
  let dir2 = (neighbor2.unwrap().position - atom.position).normalize();
  
  // Check each axis (x, y, z) for consistent alignment
  // Both bonds should point in the same direction (same sign) on the dominant axis
  
  // X-axis check
  if dir1.x.abs() > AXIS_ALIGNMENT_THRESHOLD && dir2.x.abs() > AXIS_ALIGNMENT_THRESHOLD {
    if dir1.x > 0.0 && dir2.x > 0.0 {
      return SurfaceOrientation::Surface100; // Both bonds point +X, surface normal is -X
    } else if dir1.x < 0.0 && dir2.x < 0.0 {
      return SurfaceOrientation::SurfaceNeg100; // Both bonds point -X, surface normal is +X
    }
  }
  
  // Y-axis check
  if dir1.y.abs() > AXIS_ALIGNMENT_THRESHOLD && dir2.y.abs() > AXIS_ALIGNMENT_THRESHOLD {
    if dir1.y > 0.0 && dir2.y > 0.0 {
      return SurfaceOrientation::Surface010;
    } else if dir1.y < 0.0 && dir2.y < 0.0 {
      return SurfaceOrientation::SurfaceNeg010;
    }
  }
  
  // Z-axis check
  if dir1.z.abs() > AXIS_ALIGNMENT_THRESHOLD && dir2.z.abs() > AXIS_ALIGNMENT_THRESHOLD {
    if dir1.z > 0.0 && dir2.z > 0.0 {
      return SurfaceOrientation::Surface001;
    } else if dir1.z < 0.0 && dir2.z < 0.0 {
      return SurfaceOrientation::SurfaceNeg001;
    }
  }
  
  // No axis has consistent alignment - probably edge, corner, or non-{100} surface
  SurfaceOrientation::Unknown
}

/// Classifies surface orientations for all atoms in the structure.
/// 
/// This function iterates through all atoms and determines their surface
/// orientation category, returning a map from atom ID to classification.
/// 
/// # Arguments
/// * `structure` - The atomic structure to analyze
/// 
/// # Returns
/// * HashMap mapping atom IDs to their surface orientation classifications
fn classify_all_surface_orientations(
  structure: &AtomicStructure
) -> FxHashMap<u32, SurfaceOrientation> {
  let mut orientations: FxHashMap<u32, SurfaceOrientation> = FxHashMap::default();

  // Iterate through all atoms and classify each one
  for (&atom_id, atom) in &structure.atoms {
    let orientation = classify_atom_surface_orientation(atom, structure);
    orientations.insert(atom_id, orientation);
  }

  orientations
}

/// Applies visual debugging by replacing atom atomic numbers with colored elements.
/// 
/// This function modifies the atomic structure to display surface orientations
/// as different colored elements in the 3D viewer. Each orientation category
/// gets a distinct element/color for easy visual inspection.
/// 
/// # Arguments
/// * `structure` - The atomic structure to modify
/// * `orientations` - Map of atom IDs to their surface orientations
fn apply_debug_visualization(
  structure: &mut AtomicStructure,
  orientations: &FxHashMap<u32, SurfaceOrientation>
) {
  // Replace each atom's atomic number based on its classification
  for (&atom_id, &orientation) in orientations {
    if let Some(atom) = structure.atoms.get_mut(&atom_id) {
      atom.atomic_number = get_debug_atomic_number(orientation);
    }
  }
}

/// Determines if the current structure is cubic diamond suitable for (100) reconstruction.
/// 
/// This function checks three conditions:
/// 1. The motif matches the built-in zincblende motif structure
/// 2. The unit cell is approximately cubic
/// 3. The unit cell size matches the diamond lattice parameter
/// 4. Both PRIMARY and SECONDARY element parameters are set to carbon (atomic number 6)
/// 
/// # Arguments
/// * `motif` - The motif to check
/// * `unit_cell` - The unit cell to check
/// * `parameter_element_values` - The parameter element values (PRIMARY, SECONDARY, etc.)
/// 
/// # Returns
/// * `true` if all conditions are met for cubic diamond
/// * `false` otherwise
pub fn is_cubic_diamond(
  motif: &Motif,
  unit_cell: &UnitCellStruct,
  parameter_element_values: &HashMap<String, i32>
) -> bool {
  // Check if motif matches the built-in zincblende motif
  if !motif.is_structurally_equal(&DEFAULT_ZINCBLENDE_MOTIF) {
    return false;
  }

  // Check if unit cell is approximately cubic
  if !unit_cell.is_approximately_cubic() {
    return false;
  }

  // Check if the unit cell size matches diamond lattice parameter
  const EPSILON: f64 = 1e-5;
  let cell_size = unit_cell.a.length();
  if (cell_size - DIAMOND_UNIT_CELL_SIZE_ANGSTROM).abs() > EPSILON {
    return false;
  }

  // Get effective parameter values (with defaults filled in)
  let effective_params = motif.get_effective_parameter_element_values(parameter_element_values);

  // Check if both PRIMARY and SECONDARY are set to carbon (atomic number 6)
  const CARBON_ATOMIC_NUMBER: i32 = 6;
  let primary_is_carbon = effective_params.get("PRIMARY") == Some(&CARBON_ATOMIC_NUMBER);
  let secondary_is_carbon = effective_params.get("SECONDARY") == Some(&CARBON_ATOMIC_NUMBER);

  primary_is_carbon && secondary_is_carbon
}

/// Performs (100) 2×1 dimer reconstruction for cubic diamond.
/// 
/// This function implements the surface reconstruction algorithm described in
/// surface_reconstructions.md for (100) surfaces of cubic diamond.
/// 
/// # Arguments
/// * `structure` - The atomic structure to apply reconstruction to
/// * `_motif` - The motif defining the crystal structure
/// * `_unit_cell` - The unit cell defining the lattice
/// * `_parameter_element_values` - Map of parameter element names to atomic numbers
/// * `single_bond_atoms_already_removed` - Whether single-bond atoms were already removed
pub fn reconstruct_surface_100_diamond(
  structure: &mut AtomicStructure,
  _motif: &Motif,
  _unit_cell: &UnitCellStruct,
  _parameter_element_values: &HashMap<String, i32>,
  single_bond_atoms_already_removed: bool
) {
  // Remove single-bond atoms if they haven't been removed yet
  // This is necessary for proper surface reconstruction
  if !single_bond_atoms_already_removed {
    remove_single_bond_atoms(structure, true);
  }
  
  // Step 1: Classify surface orientation for each atom
  let surface_orientations = classify_all_surface_orientations(structure);
  
  // Debug mode: visualize classifications and return early
  if DEBUG_SURFACE_ORIENTATION {
    apply_debug_visualization(structure, &surface_orientations);
    return;
  }
  
  // TODO: Step 2 - Identify dimer pairs efficiently
  // TODO: Step 3 - Apply reconstruction to dimer pairs
}

/// Performs surface reconstruction on the atomic structure.
/// 
/// This function will be expanded to support various reconstruction types,
/// starting with (100) 2×1 dimer reconstruction for cubic diamond.
/// 
/// # Arguments
/// * `structure` - The atomic structure to apply reconstruction to
/// * `motif` - The motif defining the crystal structure
/// * `unit_cell` - The unit cell defining the lattice
/// * `parameter_element_values` - Map of parameter element names to atomic numbers
/// * `single_bond_atoms_already_removed` - Whether single-bond atoms were already removed
pub fn reconstruct_surface(
  structure: &mut AtomicStructure,
  motif: &Motif,
  unit_cell: &UnitCellStruct,
  parameter_element_values: &HashMap<String, i32>,
  single_bond_atoms_already_removed: bool
) {
  // Check if we're dealing with cubic diamond - if not, do nothing for now
  if !is_cubic_diamond(motif, unit_cell, parameter_element_values) {
    return;
  }

  // Perform (100) 2×1 dimer reconstruction for cubic diamond
  reconstruct_surface_100_diamond(structure, motif, unit_cell, parameter_element_values, single_bond_atoms_already_removed);
}
