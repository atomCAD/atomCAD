use glam::i32::IVec3;
use glam::f32::Quat;
use glam::f32::Vec3;
use crate::common::surface_point_cloud::SurfacePoint;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type::DataType;
use crate::structure_designer::node_data::atom_trans_data::AtomTransData;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::common::atomic_structure::AtomicStructure;
use crate::util::timer::Timer;
use std::collections::HashMap;
use lru::LruCache;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::util::transform::Transform;
use crate::structure_designer::node_data::parameter_data::ParameterData;
use crate::structure_designer::node_data::sphere_data::SphereData;
use crate::structure_designer::node_data::cuboid_data::CuboidData;
use crate::structure_designer::node_data::half_space_data::HalfSpaceData;

const SAMPLES_PER_UNIT: i32 = 4;
const DIAMOND_SAMPLE_THRESHOLD: f32 = 0.01;
const CARBON: i32 = 6;

// Relative in-cell positions of the carbon atoms that are part of a cell
// A position can be part of multiple cells (corner positions are part of 8 cells,
// face center positions are part of 2 cells, other positions are part of 1 cell).
// In one cell coordinates go from 0 to 4. (a cell can be thought of 4x4x4 mini cells)
const IN_CELL_CARBON_POSITIONS: [IVec3; 18] = [
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

#[derive(Clone)]
pub struct GeometrySummary {
  pub frame_transform: Transform,
}

#[derive(Clone)]
pub enum NetworkResult {
  None,
  Geometry(GeometrySummary),
  Atomic(AtomicStructure),
}

pub struct NetworkEvaluator {
    implicit_evaluator: ImplicitEvaluator,
}

/*
 * Node network evaluator.
 * The node network evaluator is able to generate displayable representation for a node in a node network.
 * It delegates implicit geometry evaluation to ImplicitEvaluator.
 */
impl NetworkEvaluator {

  pub fn new() -> Self {
    Self {
      implicit_evaluator: ImplicitEvaluator::new(),
    }
  }

  // Creates the Scene that will be displayed for the given node
  // Currently creates it from scratch, no caching is used.
  pub fn generate_scene(&self, network_name: &str, node_id: u64, registry: &NodeTypeRegistry) -> StructureDesignerScene {
    let _timer = Timer::new("generate_scene");

    let network = match registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return StructureDesignerScene::new(),
    };

    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    let node = match network.nodes.get(&node_id) {
      Some(node) => node,
      None => return StructureDesignerScene::new(),
    };

    let node_type = registry.get_node_type(&node.node_type_name).unwrap();

    if node_type.output_type == DataType::Geometry {
      return self.generate_point_cloud_scene(network, node_id, registry);
    }
    if node_type.output_type == DataType::Atomic {
      //let atomic_structure = self.generate_atomic_structure(network, node, registry);

      let mut scene = StructureDesignerScene::new();

      let result = &self.evaluate(&network_stack, node_id, registry)[0];
      if let NetworkResult::Atomic(atomic_structure) = result {
        scene.atomic_structures.push(atomic_structure.clone());
      };

      return scene;
    }

    return StructureDesignerScene::new();
  }

  fn evaluate<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> Vec<NetworkResult> {

    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    if node.node_type_name == "parameter" {
      let parent_node_id = network_stack.last().unwrap().node_id;

      let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
      let args : Vec<Vec<NetworkResult>> = parent_node.arguments[param_data.param_index].argument_node_ids.iter().map(|&arg_node_id| {
        self.evaluate(&parent_network_stack, arg_node_id, registry)
      }).collect();
      return args.concat();
    }
    if node.node_type_name == "sphere" {
      return vec![self.eval_sphere(network_stack, node_id, registry)];
    }
    if node.node_type_name == "cuboid" {
      return vec![self.eval_cuboid(network_stack, node_id, registry)];
    }
    if node.node_type_name == "half_space" {
      return vec![self.eval_half_space(network_stack, node_id, registry)];
    }
    if node.node_type_name == "geo_to_atom" {
      return vec![self.eval_geo_to_atom(network_stack, node_id, registry)];
    }
    if node.node_type_name == "atom_trans" {
      return vec![self.eval_atom_trans(network_stack, node_id, registry)];
    }
    if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
      let mut child_network_stack = network_stack.clone();
      child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
      return self.evaluate(&child_network_stack, child_network.return_node_id.unwrap(), registry);
    }
    return vec![NetworkResult::None];
  }

  fn add_bond(
    &self,
    atomic_structure: &mut AtomicStructure,
    atom_ids: &Vec<u64>,
    atom_index_1: usize,
    atom_index_2: usize) {
      if atom_ids[atom_index_1] == 0 || atom_ids[atom_index_2] == 0 { return; }
      atomic_structure.add_bond(atom_ids[atom_index_1], atom_ids[atom_index_2], 1);    
  }

  fn eval_sphere<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let sphere_data = &node.data.as_any_ref().downcast_ref::<SphereData>().unwrap();

    return NetworkResult::Geometry(GeometrySummary { frame_transform: Transform::new(
      sphere_data.center.as_vec3() * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
      Quat::IDENTITY,
    ) });
  }

  fn eval_cuboid<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let cuboid_data = &node.data.as_any_ref().downcast_ref::<CuboidData>().unwrap();

    let min_corner = cuboid_data.min_corner.as_vec3() * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
    let extent = cuboid_data.extent.as_vec3() * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
    let center = min_corner + extent / 2.0;

    return NetworkResult::Geometry(GeometrySummary { frame_transform: Transform::new(
      center,
      Quat::IDENTITY,
    ) });
  }

  fn eval_half_space<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let half_space_data = &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();


    let dir = half_space_data.miller_index.as_vec3().normalize();
    let shift_handle_offset = ((half_space_data.shift as f32) / half_space_data.miller_index.as_vec3().length()) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32);

    return NetworkResult::Geometry(GeometrySummary { frame_transform: Transform::new(
      dir * shift_handle_offset,
      Quat::from_rotation_arc(Vec3::Y, dir),
    )});
  }

  fn eval_atom_trans<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {  
    let node = NetworkStackElement::get_top_node(network_stack, node_id);

    if node.arguments[0].argument_node_ids.is_empty() {
      return NetworkResult::Atomic(AtomicStructure::new());
    }
    let input_molecule_node_id = node.arguments[0].get_node_id().unwrap();

    let result = &self.evaluate(network_stack, input_molecule_node_id, registry)[0];
    if let NetworkResult::Atomic(atomic_structure) = result {
      let atom_trans_data = &node.data.as_any_ref().downcast_ref::<AtomTransData>().unwrap();

      let rotation_quat = Quat::from_euler(
        glam::EulerRot::XYX,
        atom_trans_data.rotation.x, 
        atom_trans_data.rotation.y, 
        atom_trans_data.rotation.z);

      let mut result_atomic_structure = atomic_structure.clone();
      result_atomic_structure.transform(&rotation_quat, &atom_trans_data.translation);

      return NetworkResult::Atomic(result_atomic_structure);
    }
    return NetworkResult::None;
  }

  // generates diamond molecule from geometry in an optimized way
  fn eval_geo_to_atom<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);

    if node.arguments[0].argument_node_ids.is_empty() {
      return NetworkResult::Atomic(AtomicStructure::new());
    }

    let geo_node_id = node.arguments[0].get_node_id().unwrap();

    let mut atomic_structure = AtomicStructure::new();

    // id:0 means there is no atom there
    let mut atom_pos_to_id: HashMap<IVec3, u64> = HashMap::new();

    self.process_box_for_atomic(
      network_stack,
      geo_node_id,
      registry,
      &common_constants::IMPLICIT_VOLUME_MIN,
      &(common_constants::IMPLICIT_VOLUME_MAX - common_constants::IMPLICIT_VOLUME_MIN),
      &mut atom_pos_to_id,
      &mut atomic_structure
    );

    atomic_structure.remove_lone_atoms();
    return NetworkResult::Atomic(atomic_structure);
  }

  fn process_box_for_atomic<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    geo_node_id: u64,
    registry: &NodeTypeRegistry,
    start_pos: &IVec3,
    size: &IVec3,
    atom_pos_to_id: &mut HashMap<IVec3, u64>,
    atomic_structure: &mut AtomicStructure) {

    let epsilon = 0.001;

    // Calculate the center point of the box
    let center_point = start_pos.as_vec3() + size.as_vec3() / 2.0;

    // Evaluate SDF at the center point
    let sdf_value = self.implicit_evaluator.implicit_eval(network_stack, geo_node_id, &center_point, registry)[0];
  
    let half_diagonal = size.as_vec3().length() / 2.0;

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
                    self.process_cell_for_atomic(
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
    let subdivisions = self.subdivide_box(
        start_pos,
        size,
        should_subdivide_x,
        should_subdivide_y,
        should_subdivide_z
    );
    
    // Process each subdivision recursively
    for (sub_start, sub_size) in subdivisions {
        self.process_box_for_atomic(
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

  fn subdivide_box(
    &self,
    start_pos: &IVec3,
    size: &IVec3,
    should_subdivide_x: bool,
    should_subdivide_y: bool,
    should_subdivide_z: bool
  ) -> Vec<(IVec3, IVec3)> {
    let mut result = Vec::new();
    
    // Calculate first subdivision sizes
    let sub_size_x_first = if should_subdivide_x { size.x / 2 } else { size.x };
    let sub_size_y_first = if should_subdivide_y { size.y / 2 } else { size.y };
    let sub_size_z_first = if should_subdivide_z { size.z / 2 } else { size.z };
    
    // Calculate second subdivision sizes, accounting for remainder
    let sub_size_x_second = if should_subdivide_x { size.x - sub_size_x_first } else { size.x };
    let sub_size_y_second = if should_subdivide_y { size.y - sub_size_y_first } else { size.y };
    let sub_size_z_second = if should_subdivide_z { size.z - sub_size_z_first } else { size.z };
    
    // Calculate the number of subdivisions in each direction
    let subdivisions_x = if should_subdivide_x { 2 } else { 1 };
    let subdivisions_y = if should_subdivide_y { 2 } else { 1 };
    let subdivisions_z = if should_subdivide_z { 2 } else { 1 };
    
    // Generate all subdivision boxes
    for dx in 0..subdivisions_x {
        let sub_size_x = if dx == 0 { sub_size_x_first } else { sub_size_x_second };
        let offset_x = if dx == 0 { 0 } else { sub_size_x_first };
        
        for dy in 0..subdivisions_y {
            let sub_size_y = if dy == 0 { sub_size_y_first } else { sub_size_y_second };
            let offset_y = if dy == 0 { 0 } else { sub_size_y_first };
            
            for dz in 0..subdivisions_z {
                let sub_size_z = if dz == 0 { sub_size_z_first } else { sub_size_z_second };
                let offset_z = if dz == 0 { 0 } else { sub_size_z_first };
                
                let sub_start = IVec3::new(
                    start_pos.x + offset_x,
                    start_pos.y + offset_y,
                    start_pos.z + offset_z
                );
                
                let sub_size = IVec3::new(
                    sub_size_x,
                    sub_size_y,
                    sub_size_z
                );
                
                result.push((sub_start, sub_size));
            }
        }
    }
    
    result
  }

  fn process_cell_for_atomic<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    geo_node_id: u64,
    registry: &NodeTypeRegistry,
    int_pos: &IVec3,
    atom_pos_to_id: &mut HashMap<IVec3, u64>,
    atomic_structure: &mut AtomicStructure,
    filled: bool,) {
      let cell_start_position = int_pos * 4;

      let mut carbon_atom_ids = Vec::new();
      for pos in &IN_CELL_CARBON_POSITIONS {
        let absolute_pos = cell_start_position + *pos;
        if let Some(id) = atom_pos_to_id.get(&absolute_pos) {
          carbon_atom_ids.push(*id);
        } else {
          let crystal_space_pos = absolute_pos.as_vec3() / 4.0;
          let mut has_atom = filled;
          if !has_atom {
            let value = self.implicit_evaluator.implicit_eval(network_stack, geo_node_id, &crystal_space_pos, registry)[0];
            has_atom = value < DIAMOND_SAMPLE_THRESHOLD;
          }

          let atom_id = if has_atom {
            let id = atomic_structure.add_atom(CARBON, crystal_space_pos * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM, 1);
            atom_pos_to_id.insert(absolute_pos, id);
            id
          } else { 0 };
          carbon_atom_ids.push(atom_id);
        }
      }

      self.add_bond(atomic_structure, &carbon_atom_ids, 14, 0);
      self.add_bond(atomic_structure, &carbon_atom_ids, 14, 8);
      self.add_bond(atomic_structure, &carbon_atom_ids, 14, 10);
      self.add_bond(atomic_structure, &carbon_atom_ids, 14, 12);

      self.add_bond(atomic_structure, &carbon_atom_ids, 15, 6);
      self.add_bond(atomic_structure, &carbon_atom_ids, 15, 9);
      self.add_bond(atomic_structure, &carbon_atom_ids, 15, 11);
      self.add_bond(atomic_structure, &carbon_atom_ids, 15, 12);

      self.add_bond(atomic_structure, &carbon_atom_ids, 16, 5);
      self.add_bond(atomic_structure, &carbon_atom_ids, 16, 9);
      self.add_bond(atomic_structure, &carbon_atom_ids, 16, 10);
      self.add_bond(atomic_structure, &carbon_atom_ids, 16, 13);

      self.add_bond(atomic_structure, &carbon_atom_ids, 17, 4);
      self.add_bond(atomic_structure, &carbon_atom_ids, 17, 8);
      self.add_bond(atomic_structure, &carbon_atom_ids, 17, 11);
      self.add_bond(atomic_structure, &carbon_atom_ids, 17, 13);
    }

  pub fn generate_point_cloud_scene(&self, network: &NodeNetwork, node_id: u64, registry: &NodeTypeRegistry) -> StructureDesignerScene {
    let mut point_cloud = SurfacePointCloud::new();
    let cache_size = (common_constants::IMPLICIT_VOLUME_MAX.z - common_constants::IMPLICIT_VOLUME_MIN.z + 1) *
    (common_constants::IMPLICIT_VOLUME_MAX.y - common_constants::IMPLICIT_VOLUME_MIN.y + 1) *
    (common_constants::IMPLICIT_VOLUME_MAX.x - common_constants::IMPLICIT_VOLUME_MIN.x + 1) *
    SAMPLES_PER_UNIT * SAMPLES_PER_UNIT;

    let mut eval_cache = LruCache::new(std::num::NonZeroUsize::new(cache_size as usize).unwrap());

    self.process_box_for_point_cloud(
        network,
        node_id,
        registry,
        &(common_constants::IMPLICIT_VOLUME_MIN * SAMPLES_PER_UNIT),
        &((common_constants::IMPLICIT_VOLUME_MAX - common_constants::IMPLICIT_VOLUME_MIN) * SAMPLES_PER_UNIT),
        &mut eval_cache,
        &mut point_cloud);

    let mut scene = StructureDesignerScene::new();
    scene.surface_point_clouds.push(point_cloud);
    scene
  }

  fn process_box_for_point_cloud(
      &self,
      network: &NodeNetwork,
      node_id: u64,
      registry: &NodeTypeRegistry,
      start_pos: &IVec3,
      size: &IVec3,
      eval_cache: &mut LruCache<IVec3, f32>,
      point_cloud: &mut SurfacePointCloud,) {

    let spu = SAMPLES_PER_UNIT as f32;
    let epsilon = 0.001;

    // Calculate the center point of the box
    let center_point = (start_pos.as_vec3() + size.as_vec3() / 2.0) / spu;

    // Evaluate SDF at the center point
    let sdf_value = self.implicit_evaluator.eval(network, node_id, &center_point, registry)[0];
    
    let half_diagonal = size.as_vec3().length() / spu / 2.0;
    
    // If absolute SDF value is greater than half diagonal, there's no surface in this box
    if sdf_value.abs() > half_diagonal + epsilon {
        return;
    }

    // Determine if we should subdivide in each dimension (size >= 4)
    let should_subdivide_x = size.x >= 4;
    let should_subdivide_y = size.y >= 4;
    let should_subdivide_z = size.z >= 4;

    // If we can't subdivide in any direction, process each cell individually
    if !should_subdivide_x && !should_subdivide_y && !should_subdivide_z {
        // Process each cell within the box
        for x in 0..size.x {
            for y in 0..size.y {
                for z in 0..size.z {
                    let cell_pos = IVec3::new(
                        start_pos.x + x,
                        start_pos.y + y,
                        start_pos.z + z
                    );
                    self.process_cell_for_point_cloud(
                        network,
                        node_id,
                        registry,
                        &cell_pos,
                        eval_cache,
                        point_cloud
                    );
                }
            }
        }
        return;
    }
    
    // Otherwise, subdivide the box and recursively process each subdivision
    let subdivisions = self.subdivide_box(
        start_pos,
        size,
        should_subdivide_x,
        should_subdivide_y,
        should_subdivide_z
    );
    
    // Process each subdivision recursively
    for (sub_start, sub_size) in subdivisions {
        self.process_box_for_point_cloud(
            network,
            node_id,
            registry,
            &sub_start,
            &sub_size,
            eval_cache,
            point_cloud
        );
    }
  }

  fn process_cell_for_point_cloud(
    &self,
    network: &NodeNetwork,
    node_id: u64,
    registry: &NodeTypeRegistry,
    int_pos: &IVec3,
    eval_cache: &mut LruCache<IVec3, f32>,
    point_cloud: &mut SurfacePointCloud) {
      let spu = SAMPLES_PER_UNIT as f32;

      // Define the corner points for the current cube
      let corner_points = [
          IVec3::new(int_pos.x, int_pos.y, int_pos.z),
          IVec3::new(int_pos.x + 1, int_pos.y, int_pos.z),
          IVec3::new(int_pos.x, int_pos.y + 1, int_pos.z),
          IVec3::new(int_pos.x, int_pos.y, int_pos.z + 1),
          IVec3::new(int_pos.x + 1, int_pos.y + 1, int_pos.z),
          IVec3::new(int_pos.x + 1, int_pos.y, int_pos.z + 1),
          IVec3::new(int_pos.x, int_pos.y + 1, int_pos.z + 1),
          IVec3::new(int_pos.x + 1, int_pos.y + 1, int_pos.z + 1),
      ];

      // Evaluate corner points using cache
      let values: Vec<f32> = corner_points.iter().map(|ip| {
        if let Some(&cached_value) = eval_cache.get(ip) {
          cached_value
        } else {
          let p = ip.as_vec3() / spu;
          let value = self.implicit_evaluator.eval(network, node_id, &p, registry)[0];
          //println!("Evaluating point: {:?}, value: {}", ip, value);
          eval_cache.put(*ip, value);
          value
        }
      }).collect();

      if values.iter().any(|&v| v >= 0.0) && values.iter().any(|&v| v < 0.0) {
          let center_point = (corner_points[0].as_vec3() + 0.5) / spu;
          let gradient_val = self.implicit_evaluator.get_gradient(network, node_id, &center_point, registry);
          let gradient = gradient_val.0;
          let value = gradient_val.1;
          let gradient_magnitude_sq = gradient.length_squared();
          // Avoid division by very small numbers
          let step = if gradient_magnitude_sq > 1e-10 {
              value * gradient / gradient_magnitude_sq
          } else {
              value * gradient // Fallback to SDF assumption if gradient is nearly zero
          };
          point_cloud.points.push(
            SurfacePoint {
              position: (center_point - step) * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
              normal: gradient.normalize(),
            }
          );
      }
  }

}
