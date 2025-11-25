use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::motif::Motif;
use crate::geo_tree::GeoNode;
use glam::f64::DVec3;
use std::collections::HashMap;

// ============================================================================
// Configuration Structures
// ============================================================================

/// Configuration for lattice filling operation.
/// Contains all the input data needed to perform the fill.
pub struct LatticeFillConfig {
  /// The unit cell defining the crystal lattice
  pub unit_cell: UnitCellStruct,
  
  /// The motif defining atomic positions and bonds within each unit cell
  pub motif: Motif,
  
  /// Map of parameter names to atomic numbers (e.g., "PRIMARY" -> 6 for carbon)
  pub parameter_element_values: HashMap<String, i16>,
  
  /// The geometry to fill with atoms (implicit surface)
  pub geometry: GeoNode,
  
  /// Offset applied in motif space (fractional lattice coordinates)
  pub motif_offset: DVec3,
}

/// Options controlling the filling behavior
pub struct LatticeFillOptions {
  /// Whether to add hydrogen atoms to dangling bonds
  pub hydrogen_passivation: bool,
  
  /// Whether to remove single-bond atoms before passivation
  pub remove_single_bond_atoms: bool,
  
  /// Whether to perform surface reconstruction (e.g., diamond (100) 2×1 dimers)
  pub reconstruct_surface: bool,
}

/// Results from lattice filling operation
pub struct LatticeFillResult {
  /// The resulting atomic structure
  pub atomic_structure: crate::crystolecule::atomic_structure::AtomicStructure,
}

#[derive(Debug, Clone)]
pub struct LatticeFillStatistics {
  pub fill_box_calls: i32,
  pub do_fill_box_calls: i32,
  pub do_fill_box_total_size: DVec3,
  pub motif_cells_processed: i32,
  pub atoms: i32,
  pub bonds: i32,
  pub total_depth: f64,
  pub max_depth: f64,
  pub non_batched_evaluations: i32,
  pub batched_evaluations: i32,
  pub surface_reconstructions: i32,
}

impl LatticeFillStatistics {
  pub fn new() -> Self {
    LatticeFillStatistics {
      fill_box_calls: 0,
      do_fill_box_calls: 0,
      do_fill_box_total_size: DVec3::ZERO,
      motif_cells_processed: 0,
      atoms: 0,
      bonds: 0,
      total_depth: 0.0,
      max_depth: f64::NEG_INFINITY,
      non_batched_evaluations: 0,
      batched_evaluations: 0,
      surface_reconstructions: 0,
    }
  }

  pub fn get_average_do_fill_box_size(&self) -> DVec3 {
    if self.do_fill_box_calls > 0 {
      self.do_fill_box_total_size / (self.do_fill_box_calls as f64)
    } else {
      DVec3::ZERO
    }
  }

  pub fn get_average_depth(&self) -> f64 {
    if self.atoms > 0 {
      self.total_depth / (self.atoms as f64)
    } else {
      0.0
    }
  }

  pub fn log_statistics(&self) {
    println!("LatticeFill Statistics:");
    println!("  fill_box calls: {}", self.fill_box_calls);
    println!("  do_fill_box calls: {}", self.do_fill_box_calls);
    let avg_size = self.get_average_do_fill_box_size();
    println!("  average do_fill_box size: ({:.3}, {:.3}, {:.3})", avg_size.x, avg_size.y, avg_size.z);
    println!("  motif cells processed: {}", self.motif_cells_processed);
    println!("  atoms added: {}", self.atoms);
    println!("  bonds created: {}", self.bonds);
    if self.surface_reconstructions > 0 {
      println!("  surface reconstructions: {}", self.surface_reconstructions);
    }
    println!("  evaluations: {} non-batched, {} batched", self.non_batched_evaluations, self.batched_evaluations);
    if self.atoms > 0 {
      println!("  average depth: {:.3} Å", self.get_average_depth());
      println!("  max depth: {:.3} Å", self.max_depth);
    }
  }
}
