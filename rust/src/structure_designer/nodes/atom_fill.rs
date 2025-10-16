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
use crate::structure_designer::common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::util::box_subdivision::subdivide_box_float;
use crate::common::crystal_utils::in_crystal_pos_to_id;
use crate::common::common_constants::ATOM_INFO;
use crate::structure_designer::common_constants::CrystalTypeInfo;
use crate::common::atomic_structure::CrystalMetaData;
use crate::common::crystal_utils::ZincBlendeAtomType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::diamond_hydrogen_passivation::hydrogen_passivate_diamond;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::common_constants::{REAL_IMPLICIT_VOLUME_MIN, REAL_IMPLICIT_VOLUME_MAX};

const DIAMOND_SAMPLE_THRESHOLD: f64 = 0.01;
const SMALLEST_FILL_BOX_SIZE: f64 = 4.9;
const CONSERVATIVE_EPSILON: f64 = 0.001;

#[derive(Debug, Clone)]
pub struct AtomFillStatistics {
  pub fill_box_calls: i32,
  pub do_fill_box_calls: i32,
  pub do_fill_box_total_size: DVec3,
  pub lattice_cells_processed: i32,
}

impl AtomFillStatistics {
  pub fn new() -> Self {
    AtomFillStatistics {
      fill_box_calls: 0,
      do_fill_box_calls: 0,
      do_fill_box_total_size: DVec3::ZERO,
      lattice_cells_processed: 0,
    }
  }

  pub fn get_average_do_fill_box_size(&self) -> DVec3 {
    if self.do_fill_box_calls > 0 {
      self.do_fill_box_total_size / (self.do_fill_box_calls as f64)
    } else {
      DVec3::ZERO
    }
  }

  pub fn log_statistics(&self) {
    println!("AtomFill Statistics:");
    println!("  fill_box calls: {}", self.fill_box_calls);
    println!("  do_fill_box calls: {}", self.do_fill_box_calls);
    let avg_size = self.get_average_do_fill_box_size();
    println!("  average do_fill_box size: ({:.3}, {:.3}, {:.3})", avg_size.x, avg_size.y, avg_size.z);
    println!("  lattice cells processed: {}", self.lattice_cells_processed);
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomFillData {
  pub parameter_element_values: HashMap<String, i32>,
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
      let shape_val = network_evaluator.evaluate_arg_required(&network_stack.clone(), node_id, registry, context, 0);

      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }

      let mesh = match shape_val {
        NetworkResult::Geometry(mesh) => mesh,
        _ => return NetworkResult::Atomic(AtomicStructure::new()),
      };
    
      let mut atomic_structure = AtomicStructure::new();
      let mut statistics = AtomFillStatistics::new();

      self.fill_box(
        &mesh.unit_cell,
        &mesh.geo_tree_root,
        &REAL_IMPLICIT_VOLUME_MIN,
        &(REAL_IMPLICIT_VOLUME_MAX - REAL_IMPLICIT_VOLUME_MIN),
        &mut atomic_structure,
        &mut statistics);

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
    start_pos: &DVec3,
    size: &DVec3,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics) {
    
    statistics.fill_box_calls += 1;
    let box_center = start_pos + size / 2.0;

    // Evaluate SDF at the box center
    let sdf_value = geo_tree_root.implicit_eval_3d(&box_center);

    let half_diagonal = size.length() / 2.0;

    // If SDF value is greater than half diagonal plus a treshold, there is no atom in this box.
    if sdf_value > half_diagonal + DIAMOND_SAMPLE_THRESHOLD + CONSERVATIVE_EPSILON {
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
        start_pos,
        size,
        atomic_structure,
        statistics
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
        &sub_start,
        &sub_size,
        atomic_structure,
        statistics
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
    start_pos: &DVec3,
    size: &DVec3,
    atomic_structure: &mut AtomicStructure,
    statistics: &mut AtomFillStatistics) {
    
    statistics.do_fill_box_calls += 1;
    statistics.do_fill_box_total_size += *size;
    
    // Calculate the lattice-space box that completely covers the real-space box
    let (lattice_min, lattice_size) = self.calculate_lattice_space_box(unit_cell, start_pos, size);
    
    // Iterate through all lattice cells in the calculated box
    for i in 0..lattice_size.x {
      for j in 0..lattice_size.y {
        for k in 0..lattice_size.z {
          let lattice_pos = lattice_min + IVec3::new(i, j, k);
          
          // Convert lattice position to real space to check if this cell overlaps with our box
          let cell_real_pos = unit_cell.ivec3_lattice_to_real(&lattice_pos);
          
          // Check if this lattice cell has any overlap with the real-space box
          if self.cell_overlaps_with_box(&cell_real_pos, unit_cell, start_pos, size) {
            statistics.lattice_cells_processed += 1;
            
            // Fake filling: add a carbon atom at the center of the unit cell
            let cell_center = cell_real_pos + (unit_cell.a + unit_cell.b + unit_cell.c) / 2.0;
            atomic_structure.add_atom(6, cell_center, 0);
          }
        }
      }
    }
  }

  // Helper method to calculate the lattice-space box that covers the real-space box
  fn calculate_lattice_space_box(
    &self,
    unit_cell: &UnitCellStruct,
    start_pos: &DVec3,
    size: &DVec3
  ) -> (IVec3, IVec3) {
    let end_pos = start_pos + size;
    
    // Convert the corners of the real-space box to lattice coordinates
    let start_lattice = unit_cell.real_to_dvec3_lattice(start_pos);
    let end_lattice = unit_cell.real_to_dvec3_lattice(&end_pos);
    
    // Find the minimum and maximum lattice coordinates in each dimension
    // Be conservative by expanding the range slightly to account for numerical errors
    let min_x = (start_lattice.x.min(end_lattice.x) - CONSERVATIVE_EPSILON).floor() as i32;
    let max_x = (start_lattice.x.max(end_lattice.x) + CONSERVATIVE_EPSILON).ceil() as i32;
    let min_y = (start_lattice.y.min(end_lattice.y) - CONSERVATIVE_EPSILON).floor() as i32;
    let max_y = (start_lattice.y.max(end_lattice.y) + CONSERVATIVE_EPSILON).ceil() as i32;
    let min_z = (start_lattice.z.min(end_lattice.z) - CONSERVATIVE_EPSILON).floor() as i32;
    let max_z = (start_lattice.z.max(end_lattice.z) + CONSERVATIVE_EPSILON).ceil() as i32;
    
    let lattice_min = IVec3::new(min_x, min_y, min_z);
    let lattice_size = IVec3::new(
      max_x - min_x + 1,
      max_y - min_y + 1,
      max_z - min_z + 1
    );
    
    (lattice_min, lattice_size)
  }

  // Helper method to check if a lattice cell overlaps with the real-space box
  fn cell_overlaps_with_box(
    &self,
    cell_real_pos: &DVec3,
    unit_cell: &UnitCellStruct,
    box_start: &DVec3,
    box_size: &DVec3
  ) -> bool {
    let box_end = box_start + box_size;
    
    // Calculate the bounds of the unit cell in real space
    // A unit cell at lattice position (i,j,k) spans from that position to (i+1,j+1,k+1)
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
