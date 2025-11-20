use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::motif::Motif;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::common_constants::{DEFAULT_ZINCBLENDE_MOTIF, DIAMOND_UNIT_CELL_SIZE_ANGSTROM};
use std::collections::HashMap;

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
/// * `_structure` - The atomic structure to apply reconstruction to
/// * `_motif` - The motif defining the crystal structure
/// * `_unit_cell` - The unit cell defining the lattice
/// * `_parameter_element_values` - Map of parameter element names to atomic numbers
pub fn reconstruct_surface_100_diamond(
  _structure: &mut AtomicStructure,
  _motif: &Motif,
  _unit_cell: &UnitCellStruct,
  _parameter_element_values: &HashMap<String, i32>
) {
  // TODO: Implement (100) 2×1 dimer reconstruction algorithm
  // Steps:
  // 1. Classify surface orientation for each atom
  // 2. Identify dimer pairs efficiently
  // 3. Apply reconstruction to dimer pairs
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
pub fn reconstruct_surface(
  structure: &mut AtomicStructure,
  motif: &Motif,
  unit_cell: &UnitCellStruct,
  parameter_element_values: &HashMap<String, i32>
) {
  // Check if we're dealing with cubic diamond - if not, do nothing for now
  if !is_cubic_diamond(motif, unit_cell, parameter_element_values) {
    return;
  }

  // Perform (100) 2×1 dimer reconstruction for cubic diamond
  reconstruct_surface_100_diamond(structure, motif, unit_cell, parameter_element_values);
}
