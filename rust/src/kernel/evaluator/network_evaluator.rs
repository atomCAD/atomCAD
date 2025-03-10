use glam::i32::IVec3;
use glam::f32::Vec3;
use crate::kernel::surface_point_cloud::SurfacePoint;
use crate::kernel::surface_point_cloud::SurfacePointCloud;
use crate::kernel::node_network::NodeNetwork;
use crate::kernel::node_network::Node;
use crate::kernel::node_type::NodeData;
use crate::kernel::node_type::ParameterData;
use crate::kernel::node_type::SphereData;
use crate::kernel::node_type::CuboidData;
use crate::kernel::node_type::HalfSpaceData;
use crate::kernel::node_type::GeoTransData;
use crate::kernel::node_type::DataType;
use crate::kernel::node_type_registry::NodeTypeRegistry;
use crate::kernel::scene::Scene;
use crate::kernel::atomic_structure::AtomicStructure;
use crate::util::timer::Timer;
use std::collections::HashMap;
use lru::LruCache;
use crate::kernel::common_constants;

const SAMPLES_PER_UNIT: i32 = 4;
const DIAMOND_SAMPLE_THRESHOLD: f32 = 0.01;
const CARBON: i32 = 6;

#[derive(Clone)]
pub struct NetworkStackElement<'a> {
  pub node_network: &'a NodeNetwork,
  pub node_id: u64,
}

fn eval_cuboid<'a>(
  _evaluator: &NetworkEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
  let cuboid_data = &node.data.as_any_ref().downcast_ref::<CuboidData>().unwrap();

  let max_corner = cuboid_data.min_corner + cuboid_data.extent;
  let x_val = f32::max((cuboid_data.min_corner.x as f32) - sample_point.x, sample_point.x - (max_corner.x as f32));
  let y_val = f32::max((cuboid_data.min_corner.y as f32) - sample_point.y, sample_point.y - (max_corner.y as f32));
  let z_val = f32::max((cuboid_data.min_corner.z as f32) - sample_point.z, sample_point.z - (max_corner.z as f32));

  return f32::max(f32::max(x_val, y_val), z_val);
}

fn eval_sphere<'a>(
  _evaluator: &NetworkEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
  let sphere_data = &node.data.as_any_ref().downcast_ref::<SphereData>().unwrap();

  return (sample_point - Vec3::new(sphere_data.center.x as f32, sphere_data.center.y as f32, sphere_data.center.z as f32)).length() 
    - (sphere_data.radius as f32);
}

fn eval_half_space<'a>(
  _evaluator: &NetworkEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
  let half_space_data = &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();
  let float_miller = half_space_data.miller_index.as_vec3();
  let miller_magnitude = float_miller.length();
  return (float_miller.dot(sample_point.clone()) - (half_space_data.shift as f32)) / miller_magnitude;
}

/*
fn eval_geo_trans(node_data: &dyn NodeData, args: Vec<Vec<f32>>, sample_point: &Vec3) -> f32 {
  let geo_trans_data = &node_data.as_any_ref().downcast_ref::<GeoTransData>().unwrap();

}
*/

fn eval_union<'a>(
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node: &Node,
    sample_point: &Vec3) -> f32 {
  node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f32::min).unwrap_or(f32::MAX)
}

fn eval_intersect<'a>(
  evaluator: &NetworkEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {
    node.arguments[0].argument_node_ids.iter().map(|node_id| {
      evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
    }).reduce(f32::max).unwrap_or(f32::MIN)
}

fn eval_diff<'a>(
  evaluator: &NetworkEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &Vec3) -> f32 {

  let ubase = node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f32::min).unwrap_or(f32::MAX);

  let usub = node.arguments[1].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f32::min).unwrap_or(f32::MAX);

  return f32::max(ubase, -usub)
}

pub struct NetworkEvaluator {
  built_in_functions: HashMap<String,fn(&NetworkEvaluator, &NodeTypeRegistry, &Vec<NetworkStackElement>, &Node, &Vec3) -> f32>,
}

/*
 * Node network evaluator that uses implicits for geometry modeling.
 * A node network evaluator is able to generate displayable representation for a node in a node network. This evaluator
 * does this by treating the abstract operators (nodes) in the node network as implicit geometry functions. 
 * Currently this is the only network evaluator in our codebase, but it should be possible to create other evaluators
 * (like evaluator based on polygon meshes or evaluator based on voxels.)
 * TODO: probably should be refactored into an Evaluator and an ImplicitGeometry evaluator,
 * as nodes related to atomic representation is not specific to implicits. 
 */
impl NetworkEvaluator {

  pub fn new() -> Self {
    let mut ret = Self {
      built_in_functions: HashMap::new(),    
    };

    ret.built_in_functions.insert("cuboid".to_string(), eval_cuboid);
    ret.built_in_functions.insert("sphere".to_string(), eval_sphere);
    ret.built_in_functions.insert("half_space".to_string(), eval_half_space);
    ret.built_in_functions.insert("union".to_string(), eval_union);
    ret.built_in_functions.insert("intersect".to_string(), eval_intersect);
    ret.built_in_functions.insert("diff".to_string(), eval_diff);

    return ret;
  }

  // Creates the Scene that will be displayed for the given node
  // Currently creates it from scratch, no caching is used.
  pub fn generate_scene(&self, network_name: &str, node_id: u64, registry: &NodeTypeRegistry) -> Scene {
    let _timer = Timer::new("generate_scene");

    let network = match registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return Scene::new(),
    };

    let node = match network.nodes.get(&node_id) {
      Some(node) => node,
      None => return Scene::new(),
    };

    let node_type = registry.get_node_type(&node.node_type_name).unwrap();

    if node.node_type_name == "geo_to_atom" {
      return self.generate_geo_to_atomic_scene(network, node, registry);
    }
    if node_type.output_type == DataType::Geometry {
      return self.generate_point_cloud_scene(network, node_id, registry);
    }

    return Scene::new();
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

  // generates diamond molecule from geometry
  pub fn generate_geo_to_atomic_scene(&self, network: &NodeNetwork, node: &Node, registry: &NodeTypeRegistry) -> Scene {

    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    if node.arguments[0].argument_node_ids.is_empty() {
      return Scene::new();
    }

    let geo_node_id = *node.arguments[0].argument_node_ids.iter().next().unwrap();

    let mut atomic_structure = AtomicStructure::new();

    // id:0 means there is no atom there
    let mut atom_pos_to_id: HashMap<IVec3, u64> = HashMap::new();

    // relative in-cell positions of the carbon atoms that are part of a cell
    // a position can be part of multiple cells (corner positions are part of 8 cells,
    // face center positions are part of 2 cells, other positions are part of 1 cell).
    // in one cell coordinates go from 0 to 4. (a cell can be thought of 4x4x4 mini cells)
    let in_cell_carbon_positions = [
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

    // Iterate over voxel grid
    for x in common_constants::IMPLICIT_VOLUME_MIN.x..common_constants::IMPLICIT_VOLUME_MAX.x {
      for y in common_constants::IMPLICIT_VOLUME_MIN.y..common_constants::IMPLICIT_VOLUME_MAX.y {
        for z in common_constants::IMPLICIT_VOLUME_MIN.z..common_constants::IMPLICIT_VOLUME_MAX.z {
          let cell_start_position = IVec3::new(x, y, z) * 4;

          let mut carbon_atom_ids = Vec::new();
          for pos in &in_cell_carbon_positions {
            let absolute_pos = cell_start_position + *pos;
            if let Some(id) = atom_pos_to_id.get(&absolute_pos) {
              carbon_atom_ids.push(*id);
            } else {
              let crystal_space_pos = absolute_pos.as_vec3() / 4.0;
              let value = self.implicit_eval(&network_stack, geo_node_id, &crystal_space_pos, registry)[0];
              let atom_id = if value < DIAMOND_SAMPLE_THRESHOLD {
                let id = atomic_structure.add_atom(CARBON, crystal_space_pos * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM);
                atom_pos_to_id.insert(absolute_pos, id);
                id
              } else { 0 };
              carbon_atom_ids.push(atom_id);
            }
          }

          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 14, 0);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 14, 8);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 14, 10);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 14, 12);

          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 15, 6);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 15, 9);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 15, 11);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 15, 12);

          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 16, 5);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 16, 9);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 16, 10);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 16, 13);

          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 17, 4);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 17, 8);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 17, 11);
          self.add_bond(&mut atomic_structure, &carbon_atom_ids, 17, 13);
        }
      }
    }
    atomic_structure.remove_lone_atoms();

    let mut scene = Scene::new();
    scene.atomic_structures.push(atomic_structure);
    return scene;
  }

  pub fn generate_point_cloud_scene(&self, network: &NodeNetwork, node_id: u64, registry: &NodeTypeRegistry) -> Scene {
    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    let mut point_cloud = SurfacePointCloud::new();
    let cache_size = (common_constants::IMPLICIT_VOLUME_MAX.z - common_constants::IMPLICIT_VOLUME_MIN.z + 1) *
    (common_constants::IMPLICIT_VOLUME_MAX.y - common_constants::IMPLICIT_VOLUME_MIN.y + 1) *
    SAMPLES_PER_UNIT * SAMPLES_PER_UNIT * 2;

    let mut eval_cache = LruCache::new(std::num::NonZeroUsize::new(cache_size as usize).unwrap());

    let spu = SAMPLES_PER_UNIT as f32;

    // Iterate over voxel grid
    for x in common_constants::IMPLICIT_VOLUME_MIN.x*SAMPLES_PER_UNIT..common_constants::IMPLICIT_VOLUME_MAX.x*SAMPLES_PER_UNIT {
      for y in common_constants::IMPLICIT_VOLUME_MIN.y*SAMPLES_PER_UNIT..common_constants::IMPLICIT_VOLUME_MAX.y*SAMPLES_PER_UNIT {
        for z in common_constants::IMPLICIT_VOLUME_MIN.z*SAMPLES_PER_UNIT..common_constants::IMPLICIT_VOLUME_MAX.z*SAMPLES_PER_UNIT {
          // Define the corner points for the current cube
          let corner_points = [
            IVec3::new(x, y, z),
            IVec3::new(x + 1, y, z),
            IVec3::new(x, y + 1, z),
            IVec3::new(x, y, z + 1),
            IVec3::new(x + 1, y + 1, z),
            IVec3::new(x + 1, y, z + 1),
            IVec3::new(x, y + 1, z + 1),
            IVec3::new(x + 1, y + 1, z + 1),
          ];

          // Evaluate corner points using cache
          let values: Vec<f32> = corner_points.iter().map(|ip| {
            if let Some(&cached_value) = eval_cache.get(ip) {
              cached_value
            } else {
              let p = ip.as_vec3() / spu;
              let value = self.implicit_eval(&network_stack, node_id, &p, registry)[0];
              //println!("Evaluating point: {:?}, value: {}", ip, value);
              eval_cache.put(*ip, value);
              value
            }
          }).collect();

          if values.iter().any(|&v| v >= 0.0) && values.iter().any(|&v| v < 0.0) {
            let center_point = (corner_points[0].as_vec3() + 0.5) / spu;
            let value = self.implicit_eval(&network_stack, node_id, &center_point, registry)[0];
            let gradient = self.get_gradient(network, node_id, &center_point, registry);
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
    }
    let mut scene = Scene::new();
    scene.surface_point_clouds.push(point_cloud);
    scene
  }

  pub fn get_gradient(&self, network: &NodeNetwork, node_id: u64, sample_point: &Vec3, registry: &NodeTypeRegistry) -> Vec3 {
    let epsilon = 0.0001; // Small value for finite difference approximation
    
    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    // Calculate partial derivatives using central differences
    let dx = (
      self.implicit_eval(&network_stack, node_id, &(sample_point + Vec3::new(epsilon, 0.0, 0.0)), registry)[0] -
      self.implicit_eval(&network_stack, node_id, &(sample_point - Vec3::new(epsilon, 0.0, 0.0)), registry)[0]
    ) / (2.0 * epsilon);
    
    let dy = (
      self.implicit_eval(&network_stack, node_id, &(sample_point + Vec3::new(0.0, epsilon, 0.0)), registry)[0] -
      self.implicit_eval(&network_stack, node_id, &(sample_point - Vec3::new(0.0, epsilon, 0.0)), registry)[0]
    ) / (2.0 * epsilon);
    
    let dz = (
      self.implicit_eval(&network_stack, node_id, &(sample_point + Vec3::new(0.0, 0.0, epsilon)), registry)[0] -
      self.implicit_eval(&network_stack, node_id, &(sample_point - Vec3::new(0.0, 0.0, epsilon)), registry)[0]
    ) / (2.0 * epsilon);

    let gradient = Vec3::new(dx, dy, dz);
    
    // Normalize the gradient vector
    if gradient.length_squared() > 0.0 {
      gradient.normalize()
    } else {
      gradient
    }
  }

  /*
   * This is a naive but simple way to evaluate the implicit function. We need this now
   * for rapid development and later to have a correct reference implementation.
   * Future possible optimizations:
   * - Do not refer to node types by string: use an internal id
   * - Do not do this recursion per sampled point, but do it for a cubic array at a time, and work with
   * cubic array of f32 values at once.
   * - Do not sample everywhere. If we know the max gradient length we can infer that there is no sign change in big ranges.
   * - Ultimatly to achieve very high performance we can consider generating GPU code so that evaluation can be done
   * per sampled point again, but massively paralelly in compute shader using generated GPU shader code.
   * The GPU compute shader needs to be regenerated on node network edit operations though, the cost of which
   * needs to be investigated. If partial recompilation of shader code is possible that would be a huge win.
   * Not all optimizations fit all use cases or even compatible with each other, so we might use multiple approaches
   * in different cases.
   */
  pub fn implicit_eval<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, sample_point: &Vec3, registry: &NodeTypeRegistry) -> Vec<f32> {
    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    if node.node_type_name == "parameter" {
      let parent_node_id = network_stack.last().unwrap().node_id;

      let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
      let args : Vec<Vec<f32>> = parent_node.arguments[param_data.param_index].argument_node_ids.iter().map(|&arg_node_id| {
        self.implicit_eval(&parent_network_stack, arg_node_id, sample_point, registry)
      }).collect();
      return args.concat();
    }
    if let Some(built_in_function) = self.built_in_functions.get(&node.node_type_name) {
      let ret = built_in_function(self, registry, network_stack, node, sample_point);
      return vec![ret];
    }
    if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
      let mut child_network_stack = network_stack.clone();
      child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
      return self.implicit_eval(&child_network_stack, child_network.return_node_id.unwrap(), sample_point, registry);
    }
    return vec![0.0];
  }
}
