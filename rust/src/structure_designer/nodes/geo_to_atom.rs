use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::{input_missing_error, NetworkResult};
use crate::common::atomic_structure::AtomicStructure;
use std::collections::HashMap;
use glam::i32::IVec3;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::common_constants;
use crate::util::box_subdivision::subdivide_box;
use crate::common::crystal_utils::in_crystal_pos_to_id;
use crate::common::common_constants::ATOM_INFO;
use crate::structure_designer::common_constants::CrystalTypeInfo;
use crate::common::atomic_structure::CrystalMetaData;
use crate::common::crystal_utils::ZincBlendeAtomType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationCache;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::diamond_hydrogen_passivation::hydrogen_passivate_diamond;
use crate::structure_designer::geo_tree::GeoNode;

const DIAMOND_SAMPLE_THRESHOLD: f64 = 0.01;

// Relative in-cell positions of the atoms that are part of a cell
// A position can be part of multiple cells (corner positions are part of 8 cells,
// face center positions are part of 2 cells, other positions are part of 1 cell).
// In one cell coordinates go from 0 to 4. (a cell can be thought of 4x4x4 mini cells)
const IN_CELL_ATOM_POSITIONS: [IVec3; 18] = [
  // corner positions
  IVec3::new(0, 0, 0),
  IVec3::new(4, 0, 0),
  IVec3::new(0, 4, 0),
  IVec3::new(0, 0, 4),
  IVec3::new(4, 4, 0),
  IVec3::new(4, 0, 4),
  IVec3::new(0, 4, 4),
  IVec3::new(4, 4, 4),

  // face center positions
  IVec3::new(2, 2, 0),
  IVec3::new(2, 2, 4),
  IVec3::new(2, 0, 2),
  IVec3::new(2, 4, 2),
  IVec3::new(0, 2, 2),
  IVec3::new(4, 2, 2),

  // other positions
  IVec3::new(1, 1, 1),
  IVec3::new(1, 3, 3),
  IVec3::new(3, 1, 3),
  IVec3::new(3, 3, 1),
];

const IN_CELL_ZINCBLENDE_TYPES: [ZincBlendeAtomType; 18] = [
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,

  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,
  ZincBlendeAtomType::Primary,

  ZincBlendeAtomType::Secondary,
  ZincBlendeAtomType::Secondary,
  ZincBlendeAtomType::Secondary,
  ZincBlendeAtomType::Secondary,
];

#[derive(Debug, Serialize, Deserialize)]
pub struct GeoToAtomData {
  pub primary_atomic_number: i32,
  pub secondary_atomic_number: i32,
  pub hydrogen_passivation: bool,
}

impl NodeData for GeoToAtomData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

// generates diamond molecule from geometry in an optimized way
pub fn eval_geo_to_atom<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  if node.arguments[0].argument_node_ids.is_empty() {
    return input_missing_error("shape");
  }

  let geo_node_id = node.arguments[0].get_node_id().unwrap();

  let (invocation_cache, pre_eval_result) = network_evaluator.pre_eval_geometry_node(network_stack.clone(), geo_node_id, registry);

  let mesh = match pre_eval_result {
    NetworkResult::Geometry(mesh) => mesh,
    _ => return NetworkResult::Atomic(AtomicStructure::new()),
  };

  let mut atomic_structure = AtomicStructure::new();
  atomic_structure.frame_transform = mesh.frame_transform.scale(common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM);

  // id:0 means there is no atom there
  let mut atom_pos_to_id: HashMap<IVec3, u64> = HashMap::new();

  let geo_to_atom_data = node.data.as_any_ref().downcast_ref::<GeoToAtomData>().unwrap();

  atomic_structure.crystal_meta_data = CrystalMetaData {
    primary_atomic_number: geo_to_atom_data.primary_atomic_number,
    secondary_atomic_number: geo_to_atom_data.secondary_atomic_number,
    unit_cell_size: get_unit_cell_size(geo_to_atom_data.primary_atomic_number, geo_to_atom_data.secondary_atomic_number),
    stamped_by_anchor_atom_type: None,
  };

  process_box_for_atomic(
    &mesh.geo_tree_root,
    &invocation_cache,
    geo_to_atom_data,
    network_stack,
    geo_node_id,
    registry,
    &common_constants::IMPLICIT_VOLUME_MIN,
    &(common_constants::IMPLICIT_VOLUME_MAX - common_constants::IMPLICIT_VOLUME_MIN),
    &mut atom_pos_to_id,
    &mut atomic_structure
  );

  atomic_structure.remove_lone_atoms();

  if geo_to_atom_data.hydrogen_passivation {
    hydrogen_passivate_diamond(&mut atomic_structure);
  }

  return NetworkResult::Atomic(atomic_structure);
}

fn process_box_for_atomic<'a>(
  geo_tree_root: &GeoNode,
  invocation_cache: &NodeInvocationCache,
  geo_to_atom_data: &GeoToAtomData,
  network_stack: &Vec<NetworkStackElement<'a>>,
  geo_node_id: u64,
  registry: &NodeTypeRegistry,
  start_pos: &IVec3,
  size: &IVec3,
  atom_pos_to_id: &mut HashMap<IVec3, u64>,
  atomic_structure: &mut AtomicStructure) {

  let epsilon: f64 = 0.001;

  // Calculate the center point of the box
  let center_point = start_pos.as_dvec3() + size.as_dvec3() / 2.0;

  // Evaluate SDF at the center point
  let sdf_value = geo_tree_root.implicit_eval_3d(&center_point);

  let half_diagonal = size.as_dvec3().length() / 2.0;

  // If SDF value is greater than half diagonal plus a treshold, there is no atom in this box.
  if sdf_value > half_diagonal + DIAMOND_SAMPLE_THRESHOLD + epsilon {
    return;
  }

  // If SDF value is less than -half diagonal, the whole box is filled
  let filled = sdf_value < (-half_diagonal - epsilon);
  
  // Determine if we should subdivide in each dimension (size >= 4)
  let should_subdivide_x = size.x >= 2;
  let should_subdivide_y = size.y >= 2;
  let should_subdivide_z = size.z >= 2;

  // If the whole box is filled or we can't subdivide in any direction, process each cell individually
  if filled || (!should_subdivide_x && !should_subdivide_y && !should_subdivide_z) {
      // Process each cell within the box
      for x in 0..size.x {
          for y in 0..size.y {
              for z in 0..size.z {
                  let cell_pos = IVec3::new(
                      start_pos.x + x,
                      start_pos.y + y,
                      start_pos.z + z
                  );
                  process_cell_for_atomic(
                      geo_tree_root,
                      invocation_cache,
                      geo_to_atom_data,
                      network_stack,
                      geo_node_id,
                      registry,
                      &cell_pos,
                      atom_pos_to_id,
                      atomic_structure,
                      filled,
                  );
              }
          }
      }
      return;
  }

  // Otherwise, subdivide the box and recursively process each subdivision
  let subdivisions = subdivide_box(
      start_pos,
      size,
      should_subdivide_x,
      should_subdivide_y,
      should_subdivide_z
  );
    
  // Process each subdivision recursively
  for (sub_start, sub_size) in subdivisions {
      process_box_for_atomic(
          geo_tree_root,
          invocation_cache,
          geo_to_atom_data,
          network_stack,
          geo_node_id,
          registry,
          &sub_start,
          &sub_size,
          atom_pos_to_id,
          atomic_structure
      );
  }
}

fn process_cell_for_atomic<'a>(
  geo_tree_root: &GeoNode,
  invocation_cache: &NodeInvocationCache,
  geo_to_atom_data: &GeoToAtomData,
  network_stack: &Vec<NetworkStackElement<'a>>,
  geo_node_id: u64,
  registry: &NodeTypeRegistry,
  int_pos: &IVec3,
  atom_pos_to_id: &mut HashMap<IVec3, u64>,
  atomic_structure: &mut AtomicStructure,
  filled: bool,) {
    let cell_start_position = int_pos * 4;

    let mut atom_ids = Vec::new();
    for i in 0..IN_CELL_ATOM_POSITIONS.len() {
      let pos = &IN_CELL_ATOM_POSITIONS[i];
      let atom_type = &IN_CELL_ZINCBLENDE_TYPES[i];
      let absolute_pos = cell_start_position + *pos;
      if let Some(id) = atom_pos_to_id.get(&absolute_pos) {
        atom_ids.push(*id);
      } else {
        let crystal_space_pos = absolute_pos.as_dvec3() / 4.0;
        let mut has_atom = filled;
        if !has_atom {
          let value = geo_tree_root.implicit_eval_3d( &crystal_space_pos);
          has_atom = value < DIAMOND_SAMPLE_THRESHOLD;
        }

        let atom_id = if has_atom {
          let id = in_crystal_pos_to_id(&absolute_pos);
          let atomic_number = match atom_type {
            ZincBlendeAtomType::Primary => geo_to_atom_data.primary_atomic_number,
            ZincBlendeAtomType::Secondary => geo_to_atom_data.secondary_atomic_number,
          };
          let unit_cell_size = get_unit_cell_size(geo_to_atom_data.primary_atomic_number, geo_to_atom_data.secondary_atomic_number);
          atomic_structure.add_atom_with_id(id, atomic_number, crystal_space_pos * unit_cell_size, 1);
          atom_pos_to_id.insert(absolute_pos, id);
          id
        } else { 0 };
        atom_ids.push(atom_id);
      }
    }

    add_bond(atomic_structure, &atom_ids, 14, 0);
    add_bond(atomic_structure, &atom_ids, 14, 8);
    add_bond(atomic_structure, &atom_ids, 14, 10);
    add_bond(atomic_structure, &atom_ids, 14, 12);

    add_bond(atomic_structure, &atom_ids, 15, 6);
    add_bond(atomic_structure, &atom_ids, 15, 9);
    add_bond(atomic_structure, &atom_ids, 15, 11);
    add_bond(atomic_structure, &atom_ids, 15, 12);

    add_bond(atomic_structure, &atom_ids, 16, 5);
    add_bond(atomic_structure, &atom_ids, 16, 9);
    add_bond(atomic_structure, &atom_ids, 16, 10);
    add_bond(atomic_structure, &atom_ids, 16, 13);

    add_bond(atomic_structure, &atom_ids, 17, 4);
    add_bond(atomic_structure, &atom_ids, 17, 8);
    add_bond(atomic_structure, &atom_ids, 17, 11);
    add_bond(atomic_structure, &atom_ids, 17, 13);
}

fn add_bond(
  atomic_structure: &mut AtomicStructure,
  atom_ids: &Vec<u64>,
  atom_index_1: usize,
  atom_index_2: usize) {
    if atom_ids[atom_index_1] == 0 || atom_ids[atom_index_2] == 0 { return; }
    atomic_structure.add_bond(atom_ids[atom_index_1], atom_ids[atom_index_2], 1);    
}

// Returns the unit cell size for a given element pair
// If the pair has a known measured unit cell size, returns that
// Otherwise, estimates based on covalent radii
pub fn get_unit_cell_size(primary_atomic_number: i32, secondary_atomic_number: i32) -> f64 {
  // Check if we have measured data for this pair
  if let Some(info) = common_constants::CRYSTAL_INFO_MAP.get(&(primary_atomic_number, secondary_atomic_number)) {
    return info.unit_cell_size;
  }
    
  // Check if we have measured data with elements in reverse order
  if let Some(info) = common_constants::CRYSTAL_INFO_MAP.get(&(secondary_atomic_number, primary_atomic_number)) {
    return info.unit_cell_size;
  }
    
  // If no measured data, estimate based on covalent radii
  estimate_unit_cell_size(primary_atomic_number, secondary_atomic_number)
}
  
// Estimates unit cell size based on covalent radii of elements
pub fn estimate_unit_cell_size(primary_atomic_number: i32, secondary_atomic_number: i32) -> f64 {
  // Get covalent radii from atom info using direct HashMap access
  let primary_info = ATOM_INFO.get(&primary_atomic_number);
  let secondary_info = ATOM_INFO.get(&secondary_atomic_number);
    
  if let (Some(primary), Some(secondary)) = (primary_info, secondary_info) {
    // Calculate estimated bond length based on covalent radii
    // For zinc blende structures, the bond length is approximately the sum of the covalent radii
    let bond_length = primary.radius + secondary.radius;
      
    // In zinc blende/diamond structures, the unit cell size is approximately 4 times the bond length
    // between two adjacent atoms, divided by sqrt(3)
    // This is a simplification based on crystal geometry
    let estimated_cell_size = (4.0 * bond_length) / (3.0_f64).sqrt();
      
    return estimated_cell_size;
  }
    
  // Fallback to diamond unit cell size if atom info not found
  common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM
}

// Returns whether the unit cell size for a given element pair is estimated or measured
pub fn is_unit_cell_size_estimated(primary_atomic_number: i32, secondary_atomic_number: i32) -> bool {
  // Check if we have measured data for this pair or with reverse order
  !common_constants::CRYSTAL_INFO_MAP.contains_key(&(primary_atomic_number, secondary_atomic_number)) &&
  !common_constants::CRYSTAL_INFO_MAP.contains_key(&(secondary_atomic_number, primary_atomic_number))
}

pub fn get_crystal_types() -> Vec<CrystalTypeInfo> {
  common_constants::CRYSTAL_INFO_VEC.clone()
}
