use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::common::atomic_structure::AtomicStructure;
use std::collections::HashMap;
use glam::i32::IVec3;
use glam::f64::DVec3;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::box_subdivision::subdivide_box_float;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::evaluator::motif::Motif;
use crate::structure_designer::common_constants::{REAL_IMPLICIT_VOLUME_MIN, REAL_IMPLICIT_VOLUME_MAX};
use crate::structure_designer::common_constants::DEFAULT_ZINCBLENDE_MOTIF;
use crate::structure_designer::evaluator::motif_parser::parse_parameter_element_values;
use crate::structure_designer::node_network::ValidationError;
use crate::common::serialization_utils::dvec3_serializer;
use crate::common::common_constants::ATOM_INFO;
use crate::structure_designer::evaluator::motif::SiteSpecifier;

const CRYSTAL_SAMPLE_THRESHOLD: f64 = 0.01;
const SMALLEST_FILL_BOX_SIZE: f64 = 4.9;
const CONSERVATIVE_EPSILON: f64 = 0.001;

/// Standard C-H bond length in Angstroms
const C_H_BOND_LENGTH: f64 = 1.09;

#[derive(Debug, Clone)]
pub struct AtomFillStatistics {
  pub fill_box_calls: i32,
  pub do_fill_box_calls: i32,
  pub do_fill_box_total_size: DVec3,
  pub motif_cells_processed: i32,
  pub atoms: i32,
  pub bonds: i32,
  pub total_depth: f64,
  pub max_depth: f64,
}

impl AtomFillStatistics {
  pub fn new() -> Self {
    AtomFillStatistics {
      fill_box_calls: 0,
      do_fill_box_calls: 0,
      do_fill_box_total_size: DVec3::ZERO,
      motif_cells_processed: 0,
      atoms: 0,
      bonds: 0,
      total_depth: 0.0,
      max_depth: f64::NEG_INFINITY,
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
    println!("AtomFill Statistics:");
    println!("  fill_box calls: {}", self.fill_box_calls);
    println!("  do_fill_box calls: {}", self.do_fill_box_calls);
    let avg_size = self.get_average_do_fill_box_size();
    println!("  average do_fill_box size: ({:.3}, {:.3}, {:.3})", avg_size.x, avg_size.y, avg_size.z);
    println!("  motif cells processed: {}", self.motif_cells_processed);
    println!("  atoms added: {}", self.atoms);
    println!("  bonds created: {}", self.bonds);
    if self.atoms > 0 {
      println!("  average depth: {:.3} Å", self.get_average_depth());
      println!("  max depth: {:.3} Å", self.max_depth);
    }
  }
}

#[derive(Debug, Clone)]
pub struct PlacedAtomTracker {
  // Primary storage: maps (motif_space_pos, site_index) -> atom_id
  atom_map: HashMap<(IVec3, usize), u64>,
}

impl PlacedAtomTracker {
  pub fn new() -> Self {
    PlacedAtomTracker {
      atom_map: HashMap::new(),
    }
  }
  
  /// Records that an atom was placed at the given motif space position and site index
  pub fn record_atom(&mut self, motif_space_pos: IVec3, site_index: usize, atom_id: u64) {
    self.atom_map.insert((motif_space_pos, site_index), atom_id);
  }
  
  /// Looks up the atom ID for a given motif space position and site index
  pub fn get_atom_id(&self, motif_space_pos: IVec3, site_index: usize) -> Option<u64> {
    self.atom_map.get(&(motif_space_pos, site_index)).copied()
  }
  
  /// Gets atom ID for a site specifier (handles relative cell offsets)
  pub fn get_atom_id_for_specifier(
    &self, 
    base_motif_space_pos: IVec3, 
    site_specifier: &crate::structure_designer::evaluator::motif::SiteSpecifier
  ) -> Option<u64> {
    let target_motif_space_pos = base_motif_space_pos + site_specifier.relative_cell;
    self.get_atom_id(target_motif_space_pos, site_specifier.site_index)
  }
  
  /// Returns an iterator over all placed atoms: (lattice_pos, site_index, atom_id)
  pub fn iter_atoms(&self) -> impl Iterator<Item = (IVec3, usize, u64)> + '_ {
    self.atom_map.iter().map(|((motif_space_pos, site_index), &atom_id)| (*motif_space_pos, *site_index, atom_id))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomFillData {
  pub parameter_element_value_definition: String,
  #[serde(with = "dvec3_serializer")]
  pub motif_offset: DVec3,
  pub hydrogen_passivation: bool,
  #[serde(skip)]
  pub error: Option<String>,
  #[serde(skip)]
  pub parameter_element_values: HashMap<String, i32>,
}

impl AtomFillData {
  /// Converts from motif space coordinates to real space coordinates
  /// Motif space is fractional lattice space offset by motif_offset
  fn motif_to_real(&self, unit_cell: &UnitCellStruct, motif_coords: &DVec3) -> DVec3 {
    // Convert from motif space to canonical lattice space
    let lattice_coords = motif_coords + self.motif_offset;
    // Convert from lattice space to real space
    unit_cell.dvec3_lattice_to_real(&lattice_coords)
  }
  
  /// Converts from real space coordinates to motif space coordinates
  /// Motif space is fractional lattice space offset by motif_offset
  fn real_to_motif(&self, unit_cell: &UnitCellStruct, real_coords: &DVec3) -> DVec3 {
    // Convert from real space to canonical lattice space
    let lattice_coords = unit_cell.real_to_dvec3_lattice(real_coords);
    // Convert from canonical lattice space to motif space
    lattice_coords - self.motif_offset
  }

  /// Parses and validates the parameter element definition and returns any validation errors
  pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    
    // Clear previous state
    self.parameter_element_values.clear();
    self.error = None;
    
    // Skip validation if definition is empty
    if self.parameter_element_value_definition.trim().is_empty() {
      return errors;
    }
    
    // Parse the parameter element value definition
    match parse_parameter_element_values(&self.parameter_element_value_definition) {
      Ok(values) => {
        self.parameter_element_values = values;
      },
      Err(parse_error) => {
        let error_msg = format!("Parameter element parse error: {}", parse_error);
        self.error = Some(error_msg.clone());
        errors.push(ValidationError::new(error_msg, Some(node_id)));
      }
    }
    
    errors
  }
}

impl NodeData for AtomFillData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      _decorate: bool,
      context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
      let shape_val = network_evaluator.evaluate_arg_required(&network_stack, node_id, registry, context, 0);
      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }

      let mesh = match shape_val {
        NetworkResult::Geometry(mesh) => mesh,
        _ => return NetworkResult::Atomic(AtomicStructure::new()),
      };
      
      let motif = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1,
        DEFAULT_ZINCBLENDE_MOTIF.clone(),
        NetworkResult::extract_motif
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let mut atomic_structure = AtomicStructure::new();
      let mut statistics = AtomFillStatistics::new();
      let mut atom_tracker = PlacedAtomTracker::new();

      // Calculate effective parameter element values (fill in defaults for missing values)
      let effective_parameter_values = motif.get_effective_parameter_element_values(&self.parameter_element_values);

      self.fill_box(
        &mesh.unit_cell,
        &mesh.geo_tree_root,
        &motif,
        &REAL_IMPLICIT_VOLUME_MIN,
        &(REAL_IMPLICIT_VOLUME_MAX - REAL_IMPLICIT_VOLUME_MIN),
        &mut atomic_structure,
        &mut statistics,
        &effective_parameter_values,
        &mut atom_tracker);

      // Create bonds after all atoms have been placed
      self.create_bonds(&motif, &atom_tracker, &mut atomic_structure, &mut statistics);
      
      // Remove lone atoms before hydrogen passivation (passivation will bond them)
      atomic_structure.remove_lone_atoms();
      
      // Apply hydrogen passivation after bonds are created and lone atoms removed
      if self.hydrogen_passivation {
        self.hydrogen_passivate(&mesh.unit_cell, &motif, &atom_tracker, &mut atomic_structure, &mut statistics);
      }

      // TODO: Log or use statistics for debugging/optimization
      statistics.log_statistics();

      NetworkResult::Atomic(atomic_structure)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        None
    }
}

impl AtomFillData {
  // Fills the specified box with atoms
  // uses subdivision optimization to avoid processing huge empty spaces
  fn fill_box(
    &self,
    unit_cell: &UnitCellStruct,
    geo_tree_root: &GeoNode,
    motif: &Motif,
    start_pos: &DVec3,
    size: &DVec3,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics,
    parameter_element_values: &HashMap<String, i32>,
    atom_tracker: &mut PlacedAtomTracker) {
    
    statistics.fill_box_calls += 1;
    let box_center = start_pos + size / 2.0;

    // Evaluate SDF at the box center
    let sdf_value = geo_tree_root.implicit_eval_3d(&box_center);

    let half_diagonal = size.length() / 2.0;

    // If SDF value is greater than half diagonal plus a treshold, there is no atom in this box.
    if sdf_value > half_diagonal + CRYSTAL_SAMPLE_THRESHOLD + CONSERVATIVE_EPSILON {
      return;
    }

    // If SDF value is less than -half diagonal, the whole box is filled
    let filled = sdf_value < (-half_diagonal - CONSERVATIVE_EPSILON);

    // Determine if we should subdivide in each dimension (size >= 4)
    let should_subdivide_x = size.x >= 2.0 * SMALLEST_FILL_BOX_SIZE;
    let should_subdivide_y = size.y >= 2.0 * SMALLEST_FILL_BOX_SIZE;
    let should_subdivide_z = size.z >= 2.0 * SMALLEST_FILL_BOX_SIZE;

    // If the whole box is filled or we can't subdivide in any direction,
    // we need to actually do the filling for this box
    if filled || (!should_subdivide_x && !should_subdivide_y && !should_subdivide_z) {
      self.do_fill_box(
        unit_cell,
        geo_tree_root,
        motif,
        start_pos,
        size,
        atomic_structure,
        statistics,
        parameter_element_values,
        atom_tracker
      );
      return;
    }

    // Otherwise, subdivide the box and recursively process each subdivision
    let subdivisions = subdivide_box_float(
      start_pos,
      size,
      should_subdivide_x,
      should_subdivide_y,
      should_subdivide_z
    );
    
    // Process each subdivision recursively
    for (sub_start, sub_size) in subdivisions {
      self.fill_box(
        unit_cell,
        geo_tree_root,
        motif,
        &sub_start,
        &sub_size,
        atomic_structure,
        statistics,
        parameter_element_values,
        atom_tracker
      );
    }
  }

  // Fills the specified box with atoms
  // Called by fill_box. It does the actual filling.
  // No longer uses subdivision optimization
  fn do_fill_box(
    &self,
    unit_cell: &UnitCellStruct,
    geo_tree_root: &GeoNode,
    motif: &Motif,
    start_pos: &DVec3,
    size: &DVec3,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics,
    parameter_element_values: &HashMap<String, i32>,
    atom_tracker: &mut PlacedAtomTracker) {
    
    statistics.do_fill_box_calls += 1;
    statistics.do_fill_box_total_size += *size;
    
    // Calculate the motif-space box that completely covers the real-space box
    let (motif_min, motif_size) = self.calculate_motif_space_box(unit_cell, start_pos, size);
    
    // Iterate through all motif cells in the calculated box
    for i in 0..motif_size.x {
      for j in 0..motif_size.y {
        for k in 0..motif_size.z {
          let motif_pos = motif_min + IVec3::new(i, j, k);
          
          // Convert motif position to real space to check if this cell overlaps with our box
          // First convert IVec3 to DVec3, then use motif space conversion
          let motif_pos_dvec3 = DVec3::new(motif_pos.x as f64, motif_pos.y as f64, motif_pos.z as f64);
          let cell_real_pos = self.motif_to_real(unit_cell, &motif_pos_dvec3);
          
          // Check if this motif cell has any overlap with the real-space box
          if self.cell_overlaps_with_box(&cell_real_pos, unit_cell, start_pos, size) {
            statistics.motif_cells_processed += 1;
            
            // Fill this motif cell with atoms from the motif
            self.fill_cell(
              unit_cell,
              geo_tree_root,
              motif,
              &motif_pos,
              &cell_real_pos,
              atomic_structure,
              statistics,
              parameter_element_values,
              atom_tracker
            );
            
            // Commented out for testing - can be uncommented anytime
            // let cell_center = cell_real_pos + (unit_cell.a + unit_cell.b + unit_cell.c) / 2.0;
            // atomic_structure.add_atom(6, cell_center, 0);
          }
        }
      }
    }
  }

  // Fills a single motif cell with atoms from the motif
  fn fill_cell(
    &self,
    unit_cell: &UnitCellStruct,
    geo_tree_root: &GeoNode,
    motif: &Motif,
    motif_pos: &IVec3,
    cell_real_pos: &DVec3,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics,
    parameter_element_values: &HashMap<String, i32>,
    atom_tracker: &mut PlacedAtomTracker
  ) {
    // Step 1: Place all atoms in this cell and record them in the tracker
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
      let absolute_real_pos = self.motif_to_real(unit_cell, &site_motif_pos);
      
      // Do implicit evaluation at this position
      let sdf_value = geo_tree_root.implicit_eval_3d(&absolute_real_pos);
      
      // Add atom if we are within the geometry
      if sdf_value <= CRYSTAL_SAMPLE_THRESHOLD {
        let atom_id = atomic_structure.add_atom(effective_atomic_number, absolute_real_pos, 0);
        
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
        
        atom_tracker.record_atom(*motif_pos, site_index, atom_id);
        statistics.atoms += 1;
      }
    }
  }

  // Creates bonds between atoms based on motif bond definitions
  // This is called after all atoms have been placed
  fn create_bonds(
    &self,
    motif: &Motif,
    atom_tracker: &PlacedAtomTracker,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics
  ) {
    // Iterate through all placed atoms
    for (lattice_pos, site_index, atom_id) in atom_tracker.iter_atoms() {
      // For each atom, check all bonds in the motif to see if this atom is involved
      for bond in &motif.bonds {
        // Check if this atom matches the first site of the bond
        // (assuming first site is always in relative cell (0,0,0))
        if bond.site_1.site_index == site_index && bond.site_1.relative_cell == IVec3::ZERO {
          // This atom is the first site of the bond, try to find the second site
          let atom_id_2 = atom_tracker.get_atom_id_for_specifier(lattice_pos, &bond.site_2);
          
          if let Some(id2) = atom_id_2 {
            // Both atoms exist, create the bond
            atomic_structure.add_bond(atom_id, id2, bond.multiplicity);
            statistics.bonds += 1;
          }
          // If second atom doesn't exist, skip the bond (will be handled by hydrogen passivation if enabled)
        }
      }
    }
  }

  // Applies hydrogen passivation to dangling bonds
  // This is called after bonds have been created
  fn hydrogen_passivate(
    &self,
    unit_cell: &UnitCellStruct,
    motif: &Motif,
    atom_tracker: &PlacedAtomTracker,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics
  ) {
    //println!("hydrogen_passivate called");

    // Iterate through all placed atoms
    for (lattice_pos, site_index, atom_id) in atom_tracker.iter_atoms() {
      // For each atom, check all bonds in the motif to see if this atom is involved
      for bond in &motif.bonds {
        // Case 1: Check if this atom matches the first site of the bond
        // (assuming first site is always in relative cell (0,0,0))
        if bond.site_1.site_index == site_index && bond.site_1.relative_cell == IVec3::ZERO {
          // This atom is the first site of the bond, try to find the second site
          let atom_id_2 = atom_tracker.get_atom_id_for_specifier(lattice_pos, &bond.site_2);
          
          if atom_id_2.is_none() {
            // Second atom doesn't exist - this is a dangling bond that needs to be passivated
            //println!("dangling bond found (first site exists, second doesn't)");
            
            self.hydrogen_passivate_dangling_bond(
              unit_cell,
              motif,
              &bond.site_1,
              &bond.site_2,
              atom_id,
              atomic_structure,
              statistics
            );
          }
        }
        
        // Case 2: Check if this atom matches the second site of the bond
        // We need to calculate where this atom would be if it were the second site
        let second_site_base_pos = lattice_pos - bond.site_2.relative_cell;
        if bond.site_2.site_index == site_index {
          // This atom is the second site of the bond, try to find the first site
          let atom_id_1 = atom_tracker.get_atom_id_for_specifier(second_site_base_pos, &bond.site_1);
          
          if atom_id_1.is_none() {
            // First atom doesn't exist - this is a dangling bond that needs to be passivated
            //println!("dangling bond found (second site exists, first doesn't)");
            
            self.hydrogen_passivate_dangling_bond(
              unit_cell,
              motif,
              &bond.site_2,
              &bond.site_1,
              atom_id,
              atomic_structure,
              statistics
            );
          }
        }
      }
    }
  }

  // Helper method to passivate a single dangling bond with hydrogen
  // found_site: the site that exists in the crystal
  // not_found_site: the site that is missing and needs to be passivated
  // found_atom_id: the atom ID of the existing atom
  fn hydrogen_passivate_dangling_bond(
    &self,
    unit_cell: &UnitCellStruct,
    motif: &Motif,
    found_site: &SiteSpecifier,
    not_found_site: &SiteSpecifier,
    found_atom_id: u64,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics
  ) {
    // Get the position of the found atom in real space
    if let Some(found_atom) = atomic_structure.get_atom(found_atom_id) {
      // Calculate the relative position of not_found_site relative to found_site in motif space
      let found_site_pos = motif.sites[found_site.site_index].position + 
        found_site.relative_cell.as_dvec3();
      let not_found_site_pos = motif.sites[not_found_site.site_index].position + 
        not_found_site.relative_cell.as_dvec3();

      let relative_motif_pos = not_found_site_pos - found_site_pos;
      
      // Convert the relative position from motif space to real space direction
      let real_space_direction = unit_cell.dvec3_lattice_to_real(&relative_motif_pos);
      
      // Calculate proper bond length based on atomic radii
      let bond_length = if found_atom.atomic_number == 6 {
        // Special case for C-H bonds
        C_H_BOND_LENGTH
      } else {
        // General case: sum of covalent radii
        let atom_1_radius = ATOM_INFO.get(&found_atom.atomic_number)
          .map(|info| info.radius)
          .unwrap_or(0.7); // Default radius if not found
        let hydrogen_radius = ATOM_INFO.get(&1)
          .map(|info| info.radius)
          .unwrap_or(0.31); // Default hydrogen radius
        atom_1_radius + hydrogen_radius
      };
      
      // Normalize the direction and place hydrogen at proper bond length
      let normalized_direction = real_space_direction.normalize();
      let hydrogen_pos = found_atom.position + normalized_direction * bond_length;

      // Add hydrogen atom (atomic number 1) - depth remains 0.0 by default
      let hydrogen_id = atomic_structure.add_atom(1, hydrogen_pos, 0);
      
      // Update depth statistics for hydrogen (depth = 0.0)
      statistics.total_depth += 0.0;
      // Note: max_depth doesn't need updating since hydrogen depth is 0.0
      
      // Create bond between original atom and hydrogen
      atomic_structure.add_bond(found_atom_id, hydrogen_id, 1); // Single bond
      
      statistics.bonds += 1;
      statistics.atoms += 1; // Count the hydrogen atom
    }
  }

  // Helper method to calculate the motif-space box that covers the real-space box
  fn calculate_motif_space_box(
    &self,
    unit_cell: &UnitCellStruct,
    start_pos: &DVec3,
    size: &DVec3
  ) -> (IVec3, IVec3) {
    let end_pos = start_pos + size;
    
    // Convert the corners of the real-space box to motif coordinates
    let start_motif = self.real_to_motif(unit_cell, start_pos);
    let end_motif = self.real_to_motif(unit_cell, &end_pos);
    
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

  // Helper method to check if a motif cell overlaps with the real-space box
  fn cell_overlaps_with_box(
    &self,
    cell_real_pos: &DVec3,
    unit_cell: &UnitCellStruct,
    box_start: &DVec3,
    box_size: &DVec3
  ) -> bool {
    let box_end = box_start + box_size;
    
    // Calculate the bounds of the motif unit cell in real space
    // A unit cell at motif space position (i,j,k) spans from that position to (i+1,j+1,k+1)
    let cell_end = cell_real_pos + &unit_cell.a + &unit_cell.b + &unit_cell.c;
    
    // Check for overlap using axis-aligned bounding box intersection
    // Two boxes overlap if they overlap in all three dimensions
    // Be conservative by adding epsilon to ensure we don't miss cells due to numerical errors
    let overlaps_x = cell_real_pos.x < box_end.x + CONSERVATIVE_EPSILON && 
                     cell_end.x > box_start.x - CONSERVATIVE_EPSILON;
    let overlaps_y = cell_real_pos.y < box_end.y + CONSERVATIVE_EPSILON && 
                     cell_end.y > box_start.y - CONSERVATIVE_EPSILON;
    let overlaps_z = cell_real_pos.z < box_end.z + CONSERVATIVE_EPSILON && 
                     cell_end.z > box_start.z - CONSERVATIVE_EPSILON;
    
    overlaps_x && overlaps_y && overlaps_z
  }

}
