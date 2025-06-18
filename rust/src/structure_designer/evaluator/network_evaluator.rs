use std::collections::HashMap;

use glam::f64::DVec3;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_type::DataType;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::half_plane::eval_half_plane;
use crate::structure_designer::nodes::polygon::eval_polygon;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::evaluator::implicit_evaluator::NodeEvaluator;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::util::timer::Timer;
use crate::structure_designer::nodes::geo_to_atom::eval_geo_to_atom;
use crate::structure_designer::nodes::sphere::eval_sphere;
use crate::structure_designer::nodes::cuboid::eval_cuboid;
use crate::structure_designer::nodes::intersect::eval_intersect;
use crate::structure_designer::nodes::union::eval_union;
use crate::structure_designer::nodes::half_space::eval_half_space;
use crate::structure_designer::nodes::anchor::eval_anchor;
use crate::structure_designer::nodes::atom_trans::eval_atom_trans;
use crate::structure_designer::nodes::edit_atom::edit_atom::eval_edit_atom;
use crate::structure_designer::nodes::stamp::eval_stamp;
use crate::structure_designer::nodes::circle::eval_circle;
use crate::structure_designer::nodes::rect::eval_rect;
use super::surface_splatting_2d::generate_2d_point_cloud_scene;
use super::surface_splatting_3d::generate_point_cloud_scene;
use super::dual_contour_3d::generate_dual_contour_3d_scene;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualization;
use crate::common::csg_types::CSG;
use crate::common::csg_utils::convert_csg_to_poly_mesh;

#[derive(Clone)]
pub struct GeometrySummary2D {
  pub frame_transform: Transform2D,
}

#[derive(Clone)]
pub struct GeometrySummary {
  pub frame_transform: Transform,
  pub csg: CSG,
}

#[derive(Clone)]
pub enum NetworkResult {
  None,
  Geometry2D(GeometrySummary2D),
  Geometry(GeometrySummary),
  Atomic(AtomicStructure),
  Error(String),
}

/// Creates a consistent error message for missing input in node evaluation
/// 
/// # Arguments
/// * `input_name` - The name of the missing input (e.g., 'molecule', 'shape')
/// 
/// # Returns
/// * `NetworkResult::Error` with a formatted error message
pub fn input_missing_error(input_name: &str) -> NetworkResult {
  NetworkResult::Error(format!("{} input is missing", input_name))
}

pub fn error_in_input(input_name: &str) -> NetworkResult {
  NetworkResult::Error(format!("error in {} input", input_name))
}

pub struct NetworkEvaluationContext {
  pub node_errors: HashMap<u64, String>,
  pub explicit_geo_eval_needed: bool,
}

impl NetworkEvaluationContext {
  pub fn new(explicit_geo_eval_needed: bool) -> Self {
    Self {
      node_errors: HashMap::new(),
      explicit_geo_eval_needed,
    }
  }
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

  // traces a ray for a given geometry node
  pub fn raytrace_geometry(&self, network_name: &str, registry: &NodeTypeRegistry, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<f64> {
    let network = match registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return None,
    };
    
    let mut min_distance: Option<f64> = None;
    
    for node_id in &network.displayed_node_ids {
      let node = match network.nodes.get(&node_id) {
        Some(node) => node,
        None => return None,
      };
  
      let node_type = registry.get_node_type(&node.node_type_name).unwrap();
      if node_type.output_type != DataType::Geometry {
        continue; // Skip non-geometry nodes
      }
      
      // Raytrace the current geometry node
      if let Some(distance) = self.raytrace_geometry_node(network, *node_id, registry, ray_origin, ray_direction) {
        // Update minimum distance if this is the first hit or closer than previous hits
        min_distance = match min_distance {
          None => Some(distance),
          Some(current_min) if distance < current_min => Some(distance),
          _ => min_distance,
        };
      }
    }
    
    min_distance
  } 

  pub fn raytrace_geometry_node(&self, network: &NodeNetwork, node_id: u64, registry: &NodeTypeRegistry, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<f64> {
    // Constants for ray marching algorithm
    const MAX_STEPS: usize = 100;
    const MAX_DISTANCE: f64 = 5000.0;
    const SURFACE_THRESHOLD: f64 = 0.01;
    
    let normalized_dir = ray_direction.normalize();
    let mut current_distance: f64 = 0.0;
    
    // Perform ray marching
    for _ in 0..MAX_STEPS {
      // Calculate current position along the ray
      let current_pos = *ray_origin + normalized_dir * current_distance;
      
      // Scale the position by dividing by DIAMOND_UNIT_CELL_SIZE_ANGSTROM to match the scale used in rendering
      let scaled_pos = current_pos / common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
      
      // Evaluate SDF at the scaled position
      let sdf_value = self.implicit_evaluator.eval(network, node_id, &scaled_pos, registry)[0];
      
      //println!("Current position: {:?}", current_pos);
      //println!("Scaled position: {:?}", scaled_pos);
      //println!("SDF value: {}", sdf_value);

      // If we're close enough to the surface, return the distance
      if sdf_value.abs() < SURFACE_THRESHOLD {
        return Some(current_distance);
      }
      
      // If we've gone too far, give up
      if current_distance > MAX_DISTANCE {
        return None;
      }
      
      // Step forward by the SDF value - this is safe because
      // the absolute value of the gradient of an SDF cannot be bigger than 1
      // This means the SDF value tells us how far we can safely march without missing the surface
      // We need to scale the SDF value back to world space by multiplying by DIAMOND_UNIT_CELL_SIZE_ANGSTROM
      current_distance += sdf_value * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
    }

    // No intersection found within the maximum number of steps
    None
  }

  // Creates the Scene that will be displayed for the given node
  // Currently creates it from scratch, no caching is used.
  pub fn generate_scene(
    &self,
    network_name: &str,
    node_id: u64,
    registry: &NodeTypeRegistry,
    geometry_visualization_preferences: &GeometryVisualizationPreferences
  ) -> StructureDesignerScene {
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
    
    // Create evaluation context to track errors
    let mut context = NetworkEvaluationContext::new(geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh);

    // Create a NodeEvaluator instance to abstract SDF evaluation
    let node_evaluator = NodeEvaluator {
      network,
      node_id,
      registry,
      implicit_evaluator: &self.implicit_evaluator,
    };

    let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);

    if node_type.output_type == DataType::Geometry2D {
      return generate_2d_point_cloud_scene(&node_evaluator, &mut context, geometry_visualization_preferences);
    }
    if node_type.output_type == DataType::Geometry {
      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting {
        return generate_point_cloud_scene(&node_evaluator, &mut context, geometry_visualization_preferences);
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::DualContouring {
        return generate_dual_contour_3d_scene(&node_evaluator, geometry_visualization_preferences);
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        let mut scene = StructureDesignerScene::new();
        let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context)[0];
        if let NetworkResult::Geometry(geometry_summary) = result {
          let mut poly_mesh = convert_csg_to_poly_mesh(&geometry_summary.csg);
          poly_mesh.detect_sharp_edges(
            geometry_visualization_preferences.sharpness_angle_threshold_degree,
            true
          );
          scene.poly_meshes.push(poly_mesh);
        }
        scene.node_errors = context.node_errors;
        return scene;
      }
    }
    if node_type.output_type == DataType::Atomic {
      //let atomic_structure = self.generate_atomic_structure(network, node, registry);

      let mut scene = StructureDesignerScene::new();

      let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context)[0];
      if let NetworkResult::Atomic(atomic_structure) = result {
        let mut cloned_atomic_structure = atomic_structure.clone();
        cloned_atomic_structure.from_selected_node = from_selected_node;
        scene.atomic_structures.push(cloned_atomic_structure);
      };

      // Copy the collected errors to the scene
      scene.node_errors = context.node_errors;

      return scene;
    }

    return StructureDesignerScene::new();
  }

  pub fn evaluate<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64, registry: &NodeTypeRegistry,
    decorate: bool,
    context: &mut NetworkEvaluationContext) -> Vec<NetworkResult> {

    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    let results = if node.node_type_name == "parameter" {
      let parent_node_id = network_stack.last().unwrap().node_id;

      let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
      let args : Vec<Vec<NetworkResult>> = parent_node.arguments[param_data.param_index].argument_node_ids.iter().map(|&arg_node_id| {
        self.evaluate(&parent_network_stack, arg_node_id, registry, false, context)
      }).collect();
      args.concat()
    } else if node.node_type_name == "circle" {
      vec![eval_circle(network_stack, node_id, registry)]
    } else if node.node_type_name == "rect" {
      vec![eval_rect(network_stack, node_id, registry)]
    } else if node.node_type_name == "polygon" {
      vec![eval_polygon(network_stack, node_id, registry)]
    } else if node.node_type_name == "half_plane" {
      vec![eval_half_plane(network_stack, node_id, registry)]
    }else if node.node_type_name == "sphere" {
      vec![eval_sphere(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "cuboid" {
      vec![eval_cuboid(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "half_space" {
      vec![eval_half_space(network_stack, node_id, registry)]
    } else if node.node_type_name == "intersect" {
      vec![eval_intersect(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "union" {
      vec![eval_union(&self, network_stack, node_id, registry, context)]
    }else if node.node_type_name == "geo_to_atom" {
      vec![eval_geo_to_atom(&self.implicit_evaluator, network_stack, node_id, registry)]
    } else if node.node_type_name == "edit_atom" {
      vec![eval_edit_atom(&self, network_stack, node_id, registry, decorate, context)]
    } else if node.node_type_name == "atom_trans" {
      vec![eval_atom_trans(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "anchor" {
      vec![eval_anchor(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "stamp" {
      vec![eval_stamp(&self, network_stack, node_id, registry, decorate, context)]
    } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) {
      let mut child_network_stack = network_stack.clone();
      child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
      self.evaluate(&child_network_stack, child_network.return_node_id.unwrap(), registry, false, context)
    } else {
      vec![NetworkResult::None]
    };
    
    // Check for errors and store them in the context
    for result in &results {
      if let NetworkResult::Error(error_message) = result {
        context.node_errors.insert(node_id, error_message.clone());
      }
    }
    
    results
  }



}
