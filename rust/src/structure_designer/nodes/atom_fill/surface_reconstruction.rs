use crate::common::atomic_structure::{AtomicStructure, Atom};
use crate::structure_designer::evaluator::motif::Motif;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::common_constants::{DEFAULT_ZINCBLENDE_MOTIF, DIAMOND_UNIT_CELL_SIZE_ANGSTROM};
use crate::common::atomic_structure_utils::remove_single_bond_atoms;
use crate::common::common_constants::{
  DEBUG_CARBON_GRAY, DEBUG_CARBON_RED, DEBUG_CARBON_GREEN, DEBUG_CARBON_BLUE,
  DEBUG_CARBON_YELLOW, DEBUG_CARBON_MAGENTA, DEBUG_CARBON_CYAN, DEBUG_CARBON_ORANGE
};
use crate::structure_designer::nodes::atom_fill::placed_atom_tracker::PlacedAtomTracker;
use std::collections::HashMap;
use rustc_hash::FxHashMap;
use glam::IVec3;

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

/// A dimer pair candidate for surface reconstruction.
/// 
/// This represents a primary atom and its potential dimer partner.
/// The partner orientation has not been validated yet.
#[derive(Debug, Clone, Copy)]
struct DimerPair {
  /// Atom ID of the primary dimer atom
  primary_atom_id: u32,
  
  /// Atom ID of the dimer partner
  partner_atom_id: u32,
  
  /// Surface orientation of the primary atom
  primary_orientation: SurfaceOrientation,
}

/// Results from processing atoms for dimer reconstruction.
/// 
/// This struct contains the dimer candidates and orientation data needed
/// for validating and applying surface reconstruction.
#[derive(Debug)]
struct DimerCandidateData {
  /// Map of potential dimer partner atom IDs to their surface orientations.
  /// Only includes atoms that are not primary dimer atoms (i.e., potential partners).
  /// Excludes bulk and unknown orientation atoms.
  partner_orientations: FxHashMap<u32, SurfaceOrientation>,
  
  /// Vector of dimer pair candidates.
  /// The partner orientation has not been validated yet - validation happens later.
  dimer_pairs: Vec<DimerPair>,
}

/// Determines if an atom at the given lattice position is a primary dimer atom.
/// 
/// A primary dimer atom is the designated "first" atom in a dimer pair.
/// Each dimer has exactly one primary atom to avoid double-counting.
/// 
/// # Arguments
/// * `lattice_coords` - Lattice coordinates (motif space position) of the atom
/// * `basis_index` - Basis index (site index) within the unit cell
/// * `orientation` - Surface orientation of the atom
/// 
/// # Returns
/// * `true` if this is a primary dimer atom, `false` otherwise
fn is_primary_dimer_atom(
  _lattice_coords: IVec3,
  _basis_index: usize,
  _orientation: SurfaceOrientation
) -> bool {
  // TODO: Implement pattern matching for primary atoms based on phase
  false
}

/// Computes the lattice address of the dimer partner for a primary atom.
/// 
/// Given a primary dimer atom, this function calculates the crystallographic
/// address of its dimer partner.
/// 
/// # Arguments
/// * `lattice_coords` - Lattice coordinates of the primary atom
/// * `basis_index` - Basis index of the primary atom
/// * `orientation` - Surface orientation of the primary atom
/// 
/// # Returns
/// * `Some((partner_lattice_coords, partner_basis_index))` if a partner exists
/// * `None` if no partner pattern is defined for this configuration
fn get_dimer_partner(
  _lattice_coords: IVec3,
  _basis_index: usize,
  _orientation: SurfaceOrientation
) -> Option<(IVec3, usize)> {
  // TODO: Implement partner offset calculation based on surface orientation and phase
  None
}

/// Processes all atoms to classify orientations and identify dimer pair candidates.
/// 
/// This function iterates through the PlacedAtomTracker (which has lattice coordinates)
/// and performs all necessary processing in a single efficient pass:
/// - Classifies surface orientation
/// - Applies debug visualization if enabled
/// - Identifies primary dimer atoms
/// - Finds dimer partner candidates
/// 
/// # Arguments
/// * `structure` - The atomic structure (mutable for debug visualization)
/// * `atom_tracker` - Tracker with lattice coordinate mappings
/// 
/// # Returns
/// * `DimerCandidateData` containing partner orientations and dimer pair candidates
fn process_atoms(
  structure: &mut AtomicStructure,
  atom_tracker: &PlacedAtomTracker
) -> DimerCandidateData {
  let mut partner_orientations: FxHashMap<u32, SurfaceOrientation> = FxHashMap::default();
  let mut dimer_pairs: Vec<DimerPair> = Vec::new();

  // Iterate through all placed atoms (which have lattice coordinates)
  for (lattice_coords, basis_index, atom_id) in atom_tracker.iter_atoms() {
    // Get the atom from the structure
    let atom = match structure.atoms.get(&atom_id) {
      Some(a) => a,
      None => continue, // Atom doesn't exist in structure, skip
    };

    // Classify surface orientation
    let orientation = classify_atom_surface_orientation(atom, structure);
    
    // Apply debug visualization if enabled
    if DEBUG_SURFACE_ORIENTATION {
      if let Some(atom_mut) = structure.atoms.get_mut(&atom_id) {
        atom_mut.atomic_number = get_debug_atomic_number(orientation);
      }
    }

    // Skip bulk and unknown atoms - we only care about potential dimer candidates
    if orientation == SurfaceOrientation::Bulk || orientation == SurfaceOrientation::Unknown {
      continue;
    }

    // Check if this is a primary dimer atom
    if is_primary_dimer_atom(lattice_coords, basis_index, orientation) {
      // Get the dimer partner location
      if let Some((partner_lattice, partner_basis)) = get_dimer_partner(lattice_coords, basis_index, orientation) {
        // Look up the partner atom ID
        if let Some(partner_atom_id) = atom_tracker.get_atom_id(partner_lattice, partner_basis) {
          // Add to dimer candidates (partner orientation not validated yet)
          dimer_pairs.push(DimerPair {
            primary_atom_id: atom_id,
            partner_atom_id,
            primary_orientation: orientation,
          });
        }
      }
    } else {
      partner_orientations.insert(atom_id, orientation);
    }

  }

  DimerCandidateData {
    partner_orientations,
    dimer_pairs,
  }
}

/// Applies surface reconstruction to a validated dimer pair.
/// 
/// This function performs the actual reconstruction on a pair of atoms that
/// have been validated to be proper dimer partners on the same surface facet.
/// 
/// # Arguments
/// * `structure` - The atomic structure to modify
/// * `dimer_pair` - The validated dimer pair to reconstruct
fn apply_dimer_reconstruction(
  _structure: &mut AtomicStructure,
  _dimer_pair: &DimerPair
) {
  // TODO: Implement the actual reconstruction:
  // - Create a bond between the two atoms
  // - Adjust atom positions to form the dimer
  // - Update any other structural properties
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
/// * `atom_tracker` - Tracker with lattice coordinate mappings for all placed atoms
/// * `_motif` - The motif defining the crystal structure
/// * `_unit_cell` - The unit cell defining the lattice
/// * `_parameter_element_values` - Map of parameter element names to atomic numbers
/// * `single_bond_atoms_already_removed` - Whether single-bond atoms were already removed
pub fn reconstruct_surface_100_diamond(
  structure: &mut AtomicStructure,
  atom_tracker: &PlacedAtomTracker,
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
  
  // Step 1: Process atoms - classify orientations and identify dimer candidates
  // Debug visualization is applied during processing if DEBUG_SURFACE_ORIENTATION is true
  let candidate_data = process_atoms(structure, atom_tracker);

  // Step 2: Process dimer candidates - validate and apply reconstruction
  for dimer_pair in &candidate_data.dimer_pairs {
    // Validate: check that the partner has the same orientation as the primary
    if let Some(&partner_orientation) = candidate_data.partner_orientations.get(&dimer_pair.partner_atom_id) {
      // Only reconstruct if both atoms have the same surface orientation
      if partner_orientation == dimer_pair.primary_orientation {
        apply_dimer_reconstruction(structure, dimer_pair);
      }
    }
  }
}

/// Performs surface reconstruction on the atomic structure.
/// 
/// This function will be expanded to support various reconstruction types,
/// starting with (100) 2×1 dimer reconstruction for cubic diamond.
/// 
/// # Arguments
/// * `structure` - The atomic structure to apply reconstruction to
/// * `atom_tracker` - Tracker with lattice coordinate mappings for all placed atoms
/// * `motif` - The motif defining the crystal structure
/// * `unit_cell` - The unit cell defining the lattice
/// * `parameter_element_values` - Map of parameter element names to atomic numbers
/// * `single_bond_atoms_already_removed` - Whether single-bond atoms were already removed
pub fn reconstruct_surface(
  structure: &mut AtomicStructure,
  atom_tracker: &PlacedAtomTracker,
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
  reconstruct_surface_100_diamond(structure, atom_tracker, motif, unit_cell, parameter_element_values, single_bond_atoms_already_removed);
}
