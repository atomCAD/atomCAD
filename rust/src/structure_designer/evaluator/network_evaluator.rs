use std::collections::HashMap;
use std::any::Any;

use crate::structure_designer::node_network::NodeDisplayType;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::half_plane::eval_half_plane;
use crate::structure_designer::nodes::polygon::eval_polygon;
use crate::structure_designer::nodes::reg_poly::eval_reg_poly;
use crate::structure_designer::structure_designer_scene::StructureDesignerScene;
use crate::structure_designer::common_constants;
use crate::structure_designer::nodes::int::eval_int;
use crate::structure_designer::nodes::float::eval_float;
use crate::structure_designer::nodes::ivec2::eval_ivec2;
use crate::structure_designer::nodes::ivec3::eval_ivec3;
use crate::structure_designer::nodes::vec2::eval_vec2;
use crate::structure_designer::nodes::vec3::eval_vec3;
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
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::error_in_input;

#[derive(Clone)]
pub struct NetworkStackElement<'a> {
  pub node_network: &'a NodeNetwork,
  pub node_id: u64,
}

impl<'a> NetworkStackElement<'a> {
  pub fn get_top_node(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64) -> &'a Node {
    return network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();
  }

  pub fn is_node_selected_in_root_network(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64) -> bool {
    return network_stack.first().unwrap().node_network.selected_node_id == Some(node_id);
  }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct NodeInvocationId {
    root_network_name: String,
    node_id_stack: Vec<u64>,
}

pub struct NetworkEvaluationContext {
  pub node_errors: HashMap<u64, String>,
  pub node_output_strings: HashMap<u64, String>,
  pub selected_node_eval_cache: Option<Box<dyn Any>>,
}

impl NetworkEvaluationContext {
  pub fn new() -> Self {
    Self {
      node_errors: HashMap::new(),
      node_output_strings: HashMap::new(),
      selected_node_eval_cache: None,
    }
  }
}

pub struct NetworkEvaluator {
}

/*
 * Node network evaluator.
 * The node network evaluator is able to generate displayable representation for a node in a node network.
 * It delegates node related evaluation to functions in node specific modules.
 */
impl NetworkEvaluator {
  pub fn new() -> Self {
    Self {
    }
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

    let mut context = NetworkEvaluationContext::new();

    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    let node = match network.nodes.get(&node_id) {
      Some(node) => node,
      None => return StructureDesignerScene::new(),
    };

    let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);

    let result = self.evaluate(&network_stack, node_id, registry, from_selected_node, &mut context).into_iter().next().unwrap();

    let mut scene = 
    if registry.get_node_output_type(node) == APIDataType::Geometry2D {
      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting ||
         geometry_visualization_preferences.geometry_visualization == GeometryVisualization::DualContouring {
        if let NetworkResult::Geometry2D(geometry_summary_2d) = result {
          let mut ret = generate_2d_point_cloud_scene(&geometry_summary_2d.geo_tree_root, &mut context, geometry_visualization_preferences);
          ret.geo_trees.push(geometry_summary_2d.geo_tree_root);
          ret
        } else {
          StructureDesignerScene::new()
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        self.generate_explicit_mesh_scene(result, &network_stack, node_id, registry, &mut context, geometry_visualization_preferences)
      } else {
        StructureDesignerScene::new()
      }
    }
    else if registry.get_node_output_type(node) == APIDataType::Geometry {
      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting {
        if let NetworkResult::Geometry(geometry_summary) = result {
          let mut ret = generate_point_cloud_scene(&geometry_summary.geo_tree_root, &mut context, geometry_visualization_preferences);
          ret.geo_trees.push(geometry_summary.geo_tree_root);
          ret
        } else {
          StructureDesignerScene::new()
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::DualContouring {
        if let NetworkResult::Geometry(geometry_summary) = result {
          let mut ret = generate_dual_contour_3d_scene(&geometry_summary.geo_tree_root, geometry_visualization_preferences);
          ret.geo_trees.push(geometry_summary.geo_tree_root);
          ret
        } else {
          StructureDesignerScene::new()
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        self.generate_explicit_mesh_scene(result, &network_stack, node_id, registry, &mut context, geometry_visualization_preferences)
      } else {
        StructureDesignerScene::new()
      }
    }
    else if registry.get_node_output_type(node) == APIDataType::Atomic {
      //let atomic_structure = self.generate_atomic_structure(network, node, registry);

      let mut scene = StructureDesignerScene::new();

      if let NetworkResult::Atomic(atomic_structure) = result {
        let mut cloned_atomic_structure = atomic_structure.clone();
        cloned_atomic_structure.from_selected_node = from_selected_node;
        scene.atomic_structures.push(cloned_atomic_structure);
      };
      scene
    } else {
      StructureDesignerScene::new()
    };

    // Copy the collected errors and output strings to the scene
    scene.node_errors = context.node_errors.clone();
    scene.node_output_strings = context.node_output_strings.clone();
    scene.selected_node_eval_cache = context.selected_node_eval_cache.take();

    return scene;
  }

  fn generate_explicit_mesh_scene<'a>(
    &self,
    result: NetworkResult,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64, registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    geometry_visualization_preferences: &GeometryVisualizationPreferences) -> StructureDesignerScene {
      let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);
      let mut scene = StructureDesignerScene::new();
      
      // Extract CSG from either geometry type (3D or 2D)
      let csg = match &result {
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
      
      // Extract geo_tree_root from the result based on its type
      match result {
        NetworkResult::Geometry(geometry_summary) => {
          scene.geo_trees.push(geometry_summary.geo_tree_root);
        },
        NetworkResult::Geometry2D(geometry_summary_2d) => {
          scene.geo_trees.push(geometry_summary_2d.geo_tree_root);
        },
        _ => {
          // No geo_tree_root for other result types
        }
      }
      
      return scene;
  }


  // Convenience helper method for the most common evaluation scenario:
  // evaluates a single argument and returns a single element of the result.
  // Returns None if the input was not connected.
  // Can return an Error NetworkResult, or a valid NetworkResult.
  pub fn evaluate_single_arg<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
  ) -> Option<NetworkResult> {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);

    let input_name = registry.get_parameter_name(&node.node_type_name, parameter_index);
    if let Some(input_node_id) = node.arguments[parameter_index].get_node_id() {
      let result = self.evaluate(
        network_stack,
        input_node_id,
        registry, 
        false,
        context
      )[0].clone();
      if let NetworkResult::Error(_error) = result {
        return Some(error_in_input(&input_name));
      }
      return Some(result);
    } else {
      return None;
    }
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
    } else if node.node_type_name == "int" {
      vec![eval_int(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "float" {
      vec![eval_float(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "ivec2" {
      vec![eval_ivec2(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "ivec3" {
      vec![eval_ivec3(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "vec2" {
      vec![eval_vec2(network_stack, node_id, registry, context)]
    } else if node.node_type_name == "vec3" {
      vec![eval_vec3(network_stack, node_id, registry, context)]
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
      vec![eval_cuboid(&self, network_stack, node_id, registry, context)]
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
      vec![eval_geo_to_atom(&self, network_stack, node_id, registry, context)]
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

    // Check for errors and store them in the context
    for result in &results {
      if let NetworkResult::Error(error_message) = result {
        context.node_errors.insert(node_id, error_message.clone());
      }
    }
    
    // Process results for display strings
    let display_strings: Vec<String> = results
      .iter()
      .filter_map(|result| result.to_display_string())
      .collect();
    
    if !display_strings.is_empty() {
      let output_string = if display_strings.len() == 1 {
        display_strings[0].clone()
      } else {
        format!("[{}]", display_strings.join(", "))
      };
      context.node_output_strings.insert(node_id, output_string);
    }
    

    
    results
  }

}
