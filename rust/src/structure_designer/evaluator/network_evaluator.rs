use std::collections::HashMap;
use std::any::Any;

use glam::f64::DVec3;
use crate::structure_designer::node_network::NodeDisplayType;
use crate::structure_designer::node_network::NodeNetwork;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::half_plane::eval_half_plane;
use crate::structure_designer::nodes::polygon::eval_polygon;
use crate::structure_designer::nodes::reg_poly::eval_reg_poly;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::evaluator::implicit_evaluator::NodeEvaluator;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::nodes::geo_to_atom::eval_geo_to_atom;
use crate::structure_designer::nodes::geo_trans::eval_geo_trans;
use crate::structure_designer::nodes::sphere::eval_sphere;
use crate::structure_designer::nodes::cuboid::eval_cuboid;
use crate::structure_designer::nodes::intersect::eval_intersect;
use crate::structure_designer::nodes::union::eval_union;
use crate::structure_designer::nodes::parameter::eval_parameter;
use crate::structure_designer::nodes::diff::eval_diff;
use crate::structure_designer::nodes::intersect_2d::eval_intersect_2d;
use crate::structure_designer::nodes::union_2d::eval_union_2d;
use crate::structure_designer::nodes::diff_2d::eval_diff_2d;
use crate::structure_designer::nodes::extrude::eval_extrude;
use crate::structure_designer::nodes::half_space::eval_half_space;
use crate::structure_designer::nodes::facet_shell::FacetShellData;
use crate::structure_designer::nodes::anchor::eval_anchor;
use crate::structure_designer::nodes::atom_trans::eval_atom_trans;
use crate::structure_designer::nodes::edit_atom::edit_atom::eval_edit_atom;
use crate::structure_designer::nodes::stamp::eval_stamp;
use crate::structure_designer::nodes::circle::eval_circle;
use crate::structure_designer::nodes::rect::eval_rect;
use crate::structure_designer::nodes::facet_shell::eval_facet_shell;
use crate::structure_designer::nodes::relax::eval_relax;
use crate::structure_designer::implicit_eval::surface_splatting_2d::generate_2d_point_cloud_scene;
use crate::structure_designer::implicit_eval::surface_splatting_3d::generate_point_cloud_scene;
use crate::structure_designer::implicit_eval::dual_contour_3d::generate_dual_contour_3d_scene;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualization;
use crate::common::csg_utils::convert_csg_to_poly_mesh;
use crate::structure_designer::geo_tree::GeoNode;

#[derive(Clone)]
pub struct GeometrySummary2D {
  pub frame_transform: Transform2D,
  pub geo_tree_root: GeoNode,
}

#[derive(Clone)]
pub struct GeometrySummary {
  pub frame_transform: Transform,
  pub geo_tree_root: GeoNode,
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

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct NodeInvocationId {
    root_network_name: String,
    node_id_stack: Vec<u64>,
}

impl NodeInvocationId {
    pub fn new<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64) -> Self {
        let root_network_name = network_stack.first()
            .map(|element| element.node_network.node_type.name.clone())
            .unwrap_or_default();
        
        let mut node_id_stack = Vec::new();
        
        // Add node_id from all elements except the first one
        for element in network_stack.iter().skip(1) {
            node_id_stack.push(element.node_id);
        }
        
        // Add the parameter node_id at the end
        node_id_stack.push(node_id);
        
        NodeInvocationId {
            root_network_name,
            node_id_stack,
        }
    }
}

pub type NodeInvocationCache = HashMap<NodeInvocationId, Vec<NetworkResult>>;

pub struct NetworkEvaluationContext {
  pub node_errors: HashMap<u64, String>,
  pub explicit_geo_eval_needed: bool,
  pub record_invocations: bool,
  pub node_invocation_cache: NodeInvocationCache,
  pub selected_node_eval_cache: Option<Box<dyn Any>>,
}

impl NetworkEvaluationContext {
  pub fn new(explicit_geo_eval_needed: bool, record_invocations: bool) -> Self {
    Self {
      node_errors: HashMap::new(),
      explicit_geo_eval_needed,
      record_invocations,
      node_invocation_cache: HashMap::new(),
      selected_node_eval_cache: None,
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

  // traces a ray into all displayed geometry nodes in the network and returns the closest intersection distance if any
  pub fn raytrace_geometry(&self, network_name: &str, registry: &NodeTypeRegistry, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<f64> {
    let network = match registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return None,
    };
    
    let mut min_distance: Option<f64> = None;
    
    for node_entry in &network.displayed_node_ids {
      let node = match network.nodes.get(&node_entry.0) {
        Some(node) => node,
        None => return None,
      };
  
      if registry.get_node_output_type(node) != APIDataType::Geometry {
        continue; // Skip non-geometry nodes
      }
      
      // Raytrace the current geometry node
      if let Some(distance) = self.raytrace_geometry_node(network, *node_entry.0, registry, ray_origin, ray_direction) {
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

  // traces a ray for a given geometry node
  pub fn raytrace_geometry_node(&self, network: &NodeNetwork, node_id: u64, registry: &NodeTypeRegistry, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<f64> {
    // Constants for ray marching algorithm
    const MAX_STEPS: usize = 100;
    const MAX_DISTANCE: f64 = 5000.0;
    const SURFACE_THRESHOLD: f64 = 0.01;
    
    let normalized_dir = ray_direction.normalize();
    let mut current_distance: f64 = 0.0;

    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    let invocation_cache = self.pre_eval_geometry_node(network_stack, node_id, registry).0;

    // Perform ray marching
    for _ in 0..MAX_STEPS {
      // Calculate current position along the ray
      let current_pos = *ray_origin + normalized_dir * current_distance;
      
      // Scale the position by dividing by DIAMOND_UNIT_CELL_SIZE_ANGSTROM to match the scale used in rendering
      let scaled_pos = current_pos / common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
      
      // Evaluate SDF at the scaled position
      let sdf_value = self.implicit_evaluator.eval(network, node_id, &scaled_pos, registry, &invocation_cache)[0];
      
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

  // Creates the Scene that will be displayed for the given node by the Renderer, and is retained
  // for interaction purposes
  pub fn generate_scene(
    &self,
    network_name: &str,
    node_id: u64,
    _display_type: NodeDisplayType, //TODO: use display_type
    registry: &NodeTypeRegistry,
    geometry_visualization_preferences: &GeometryVisualizationPreferences,
  ) -> StructureDesignerScene {
    //let _timer = Timer::new("generate_scene");

    let network = match registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return StructureDesignerScene::new(),
    };

    let mut context = NetworkEvaluationContext::new(
      geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh,
      false
    );

    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    let node = match network.nodes.get(&node_id) {
      Some(node) => node,
      None => return StructureDesignerScene::new(),
    };

    let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);

    let mut scene = if registry.get_node_output_type(node) == APIDataType::Geometry2D {
      // Create a NodeEvaluator instance to abstract SDF evaluation
      let node_evaluator = NodeEvaluator {
        network,
        node_id,
        registry,
        implicit_evaluator: &self.implicit_evaluator,
        invocation_cache: self.pre_eval_geometry_node(network_stack.clone(), node_id, registry).0,
      };

      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting ||
         geometry_visualization_preferences.geometry_visualization == GeometryVisualization::DualContouring {
        let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context)[0];
        if let NetworkResult::Geometry2D(geometry_summary_2d) = result {
          generate_2d_point_cloud_scene(&geometry_summary_2d.geo_tree_root, &mut context, geometry_visualization_preferences)
        } else {
          StructureDesignerScene::new()
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        self.generate_explicit_mesh_scene(&network_stack, node_id, registry, &mut context, geometry_visualization_preferences)
      } else {
        StructureDesignerScene::new()
      }
    }
    else if registry.get_node_output_type(node) == APIDataType::Geometry {
      // Create a NodeEvaluator instance to abstract SDF evaluation
      let node_evaluator = NodeEvaluator {
        network,
        node_id,
        registry,
        implicit_evaluator: &self.implicit_evaluator,
        invocation_cache: self.pre_eval_geometry_node(network_stack.clone(), node_id, registry).0,
      };
      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting {
        let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context)[0];
        if let NetworkResult::Geometry(geometry_summary) = result {
          generate_point_cloud_scene(&geometry_summary.geo_tree_root, &mut context, geometry_visualization_preferences) 
        } else {
          StructureDesignerScene::new()
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::DualContouring {
        let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context)[0];
        if let NetworkResult::Geometry(geometry_summary) = result {
          generate_dual_contour_3d_scene(&geometry_summary.geo_tree_root, geometry_visualization_preferences)
        } else {
          StructureDesignerScene::new()
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        self.generate_explicit_mesh_scene(&network_stack, node_id, registry, &mut context, geometry_visualization_preferences)
      } else {
        StructureDesignerScene::new()
      }
    }
    else if registry.get_node_output_type(node) == APIDataType::Atomic {
      //let atomic_structure = self.generate_atomic_structure(network, node, registry);

      let mut scene = StructureDesignerScene::new();

      let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context)[0];
      if let NetworkResult::Atomic(atomic_structure) = result {
        let mut cloned_atomic_structure = atomic_structure.clone();
        cloned_atomic_structure.from_selected_node = from_selected_node;
        scene.atomic_structures.push(cloned_atomic_structure);
      };
      scene
    } else {
      StructureDesignerScene::new()
    };

    // Copy the collected errors to the scene
    scene.node_errors = context.node_errors.clone();
    scene.selected_node_eval_cache = context.selected_node_eval_cache.take();

    return scene;
  }

  fn generate_explicit_mesh_scene<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64, registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    geometry_visualization_preferences: &GeometryVisualizationPreferences) -> StructureDesignerScene {
      let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);
      let mut scene = StructureDesignerScene::new();
      let result = &self.evaluate(&network_stack, node_id, registry, from_selected_node, context)[0];
      
      // Extract CSG from either geometry type (3D or 2D)
      let csg = match result {
        NetworkResult::Geometry(geometry_summary) => Some(geometry_summary.geo_tree_root.to_csg()),
        NetworkResult::Geometry2D(geometry_summary_2d) => Some(geometry_summary_2d.geo_tree_root.to_csg()),
        _ => None,
      };

      // Process the CSG if it was found
      if let Some(csg) = csg {
        let scale_factor = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
        let scaled_csg = csg.scale(scale_factor, scale_factor, scale_factor);
        let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();
        let is_half_space = node.node_type_name == "half_space";
        let mut poly_mesh = convert_csg_to_poly_mesh(
          &scaled_csg, 
          !geometry_visualization_preferences.wireframe_geometry,
          is_half_space,
          is_half_space);
        poly_mesh.detect_sharp_edges(
          geometry_visualization_preferences.sharpness_angle_threshold_degree,
          true
        );
        // Highlight faces if the last node is facet_shell and it's selected
        if node.node_type_name == "facet_shell" && from_selected_node {
          // Downcast the node data to FacetShellData
          if let Some(facet_shell_data) = node.data.as_any_ref().downcast_ref::<FacetShellData>() {
            // Call the highlight method
            facet_shell_data.highlight_selected_facets(&mut poly_mesh);
          }
        }
        scene.poly_meshes.push(poly_mesh);
      }
  
      scene.node_errors = context.node_errors.clone();
      return scene;
  }

  // Pre-evaluates a geometry node with explicit mesh generation turned off
  // just to cache transforms for each node invocation which will be used
  // in implicit evaluation of the geometry.
  // Returns the pre evaluation context which contains the transformation
  // outputs for each node invocation.
  pub fn pre_eval_geometry_node(
    &self,
    network_stack: Vec<NetworkStackElement>,
    node_id: u64,
    registry: &NodeTypeRegistry) -> (NodeInvocationCache, NetworkResult) {
    // Create evaluation context to record transformation outputs for invocations
    let mut context = NetworkEvaluationContext::new(
      false,
      true
    );
    let result = self.evaluate(&network_stack, node_id, registry, false, &mut context)[0].clone();

    return (context.node_invocation_cache.clone(), result);
  }

  pub fn evaluate<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64, registry: &NodeTypeRegistry,
    decorate: bool,
    context: &mut NetworkEvaluationContext) -> Vec<NetworkResult> {

    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    let results = if node.node_type_name == "parameter" {
      eval_parameter(&self, network_stack, node_id, registry, context)
    } else if node.node_type_name == "circle" {
      vec![eval_circle(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "rect" {
      vec![eval_rect(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "reg_poly" {
      vec![eval_reg_poly(network_stack, node_id, registry)]
    } else if node.node_type_name == "polygon" {
      vec![eval_polygon(network_stack, node_id, registry)]
    } else if node.node_type_name == "half_plane" {
      vec![eval_half_plane(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "intersect_2d" {
      vec![eval_intersect_2d(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "union_2d" {
      vec![eval_union_2d(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "diff_2d" {
      vec![eval_diff_2d(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "extrude" {
      vec![eval_extrude(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "sphere" {
      vec![eval_sphere(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "cuboid" {
      vec![eval_cuboid(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "half_space" {
      vec![eval_half_space(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "facet_shell" {
      vec![eval_facet_shell(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "intersect" {
      vec![eval_intersect(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "union" {
      vec![eval_union(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "diff" {
      vec![eval_diff(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "geo_trans" {
      vec![eval_geo_trans(&self, network_stack, node_id, registry, context)]
    }else if node.node_type_name == "geo_to_atom" {
      vec![eval_geo_to_atom(&self, network_stack, node_id, registry)]
    } else if node.node_type_name == "edit_atom" {
      vec![eval_edit_atom(&self, network_stack, node_id, registry, decorate, context)]
    } else if node.node_type_name == "atom_trans" {
      vec![eval_atom_trans(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "anchor" {
      vec![eval_anchor(&self, network_stack, node_id, registry, context)]
    } else if node.node_type_name == "stamp" {
      vec![eval_stamp(&self, network_stack, node_id, registry, decorate, context)]
    } else if node.node_type_name == "relax" {
      vec![eval_relax(&self, network_stack, node_id, registry, context)]
    } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) { // custom node
      let mut child_network_stack = network_stack.clone();
      child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
      let result = self.evaluate(&child_network_stack, child_network.return_node_id.unwrap(), registry, false, context);
      if let NetworkResult::Error(_error) = &result[0] {
        vec![NetworkResult::Error(format!("Error in {}", node.node_type_name))]
      } else { result }
    } else {
      vec![NetworkResult::None]
    };
    
    if context.record_invocations {
      let node_invocation_id = NodeInvocationId::new(network_stack, node_id);
      context.node_invocation_cache.insert(node_invocation_id, results.clone());
    }

    // Check for errors and store them in the context
    for result in &results {
      if let NetworkResult::Error(error_message) = result {
        context.node_errors.insert(node_id, error_message.clone());
      }
    }
    
    results
  }



}
