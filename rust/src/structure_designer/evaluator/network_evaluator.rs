use glam::i32::IVec3;
use crate::common::surface_point_cloud::SurfacePoint;
use crate::common::surface_point_cloud::SurfacePointCloud;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type::DataType;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::util::transform::Transform;
use crate::structure_designer::nodes::parameter::ParameterData;
use lru::LruCache;
use crate::util::timer::Timer;
use crate::util::box_subdivision::subdivide_box;
use crate::structure_designer::nodes::geo_to_atom::eval_geo_to_atom;
use crate::structure_designer::nodes::sphere::eval_sphere;
use crate::structure_designer::nodes::cuboid::eval_cuboid;
use crate::structure_designer::nodes::half_space::eval_half_space;
use crate::structure_designer::nodes::anchor::eval_anchor;
use crate::structure_designer::nodes::atom_trans::eval_atom_trans;
use crate::structure_designer::nodes::edit_atom::edit_atom::eval_edit_atom;
use crate::structure_designer::nodes::stamp::eval_stamp;

const SAMPLES_PER_UNIT: i32 = 4;

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
    pub implicit_evaluator: ImplicitEvaluator,
}

/*
 * Node network evaluator.
 * The node network evaluator is able to generate displayable representation for a node in a node network.
 * It delegates implicit geometry evaluation to ImplicitEvaluator.
 * It delegates node related evaluation to functions in node specific modules.
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

      let result = &self.evaluate(&network_stack, node_id, registry, network_stack.last().unwrap().node_network.selected_node_id == Some(node_id))[0];
      if let NetworkResult::Atomic(atomic_structure) = result {
        let cloned_atomic_structure = atomic_structure.clone();
        scene.atomic_structures.push(cloned_atomic_structure);
      };

      return scene;
    }

    return StructureDesignerScene::new();
  }

  pub fn evaluate<'a>(&self, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry, decorate: bool) -> Vec<NetworkResult> {

    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    if node.node_type_name == "parameter" {
      let parent_node_id = network_stack.last().unwrap().node_id;

      let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
      let args : Vec<Vec<NetworkResult>> = parent_node.arguments[param_data.param_index].argument_node_ids.iter().map(|&arg_node_id| {
        self.evaluate(&parent_network_stack, arg_node_id, registry, false)
      }).collect();
      return args.concat();
    }
    if node.node_type_name == "sphere" {
      return vec![eval_sphere(network_stack, node_id, registry)];
    }
    if node.node_type_name == "cuboid" {
      return vec![eval_cuboid(network_stack, node_id, registry)];
    }
    if node.node_type_name == "half_space" {
      return vec![eval_half_space(network_stack, node_id, registry)];
    }
    if node.node_type_name == "geo_to_atom" {
      return vec![eval_geo_to_atom(&self.implicit_evaluator, network_stack, node_id, registry)];
    }
    if node.node_type_name == "edit_atom" {
      return vec![eval_edit_atom(&self, network_stack, node_id, registry, decorate)];
    }
    if node.node_type_name == "atom_trans" {
      return vec![eval_atom_trans(&self, network_stack, node_id, registry)];
    }
    if node.node_type_name == "anchor" {
      return vec![eval_anchor(&self, network_stack, node_id, registry)];
    }    
    if node.node_type_name == "stamp" {
      return vec![eval_stamp(&self, network_stack, node_id, registry)];
    }
    if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
      let mut child_network_stack = network_stack.clone();
      child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
      return self.evaluate(&child_network_stack, child_network.return_node_id.unwrap(), registry, false);
    }
    return vec![NetworkResult::None];
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
      eval_cache: &mut LruCache<IVec3, f64>,
      point_cloud: &mut SurfacePointCloud,) {

    let spu = SAMPLES_PER_UNIT as f64;
    let epsilon = 0.001;

    // Calculate the center point of the box
    let center_point = (start_pos.as_dvec3() + size.as_dvec3() / 2.0) / spu;

    // Evaluate SDF at the center point
    let sdf_value = self.implicit_evaluator.eval(network, node_id, &center_point, registry)[0];
    
    let half_diagonal = size.as_dvec3().length() / spu / 2.0;
    
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
    let subdivisions = subdivide_box(
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
    eval_cache: &mut LruCache<IVec3, f64>,
    point_cloud: &mut SurfacePointCloud) {
      let spu = SAMPLES_PER_UNIT as f64;

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
      let values: Vec<f64> = corner_points.iter().map(|ip| {
        if let Some(&cached_value) = eval_cache.get(ip) {
          cached_value
        } else {
          let p = ip.as_dvec3() / spu;
          let value = self.implicit_evaluator.eval(network, node_id, &p, registry)[0];
          //println!("Evaluating point: {:?}, value: {}", ip, value);
          eval_cache.put(*ip, value);
          value
        }
      }).collect();

      if values.iter().any(|&v| v >= 0.0) && values.iter().any(|&v| v < 0.0) {
          let center_point = (corner_points[0].as_dvec3() + 0.5) / spu;
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
