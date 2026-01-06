use std::collections::HashMap;
use std::any::Any;

use crate::structure_designer::node_network::NodeDisplayType;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer_scene::{NodeSceneData, NodeOutput};
use crate::structure_designer::nodes::facet_shell::FacetShellData;
use crate::structure_designer::implicit_eval::surface_splatting_2d::generate_2d_point_cloud;
use crate::structure_designer::implicit_eval::surface_splatting_3d::generate_point_cloud;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualization;
use crate::display::csg_to_poly_mesh::convert_csg_mesh_to_poly_mesh;
use crate::display::csg_to_poly_mesh::convert_csg_sketch_to_poly_mesh;
use crate::structure_designer::node_network::NodeNetwork;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::data_type::DataType;
use crate::geo_tree::csg_cache::CsgConversionCache;
use crate::geo_tree::GeoNode;

use super::network_result::input_missing_error;
use super::network_result::Closure;

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
  pub top_level_parameters: HashMap<String, NetworkResult>,
}

impl NetworkEvaluationContext {
  pub fn new() -> Self {
    Self {
      node_errors: HashMap::new(),
      node_output_strings: HashMap::new(),
      selected_node_eval_cache: None,
      top_level_parameters: HashMap::new(),
    }
  }
}

pub struct NetworkEvaluator {
  csg_conversion_cache: CsgConversionCache,
}

/*
 * Node network evaluator.
 * The node network evaluator is able to generate displayable representation for a node in a node network.
 * It delegates node related evaluation to functions in node specific modules.
 */
impl NetworkEvaluator {
  pub fn new() -> Self {
    Self {
      csg_conversion_cache: CsgConversionCache::with_defaults(),
    }
  }

  /// Clear the CSG conversion cache
  pub fn clear_csg_cache(&mut self) {
    self.csg_conversion_cache.clear();
  }

  /// Get cache statistics
  pub fn get_csg_cache_stats(&self) -> crate::geo_tree::csg_cache::CacheStats {
    self.csg_conversion_cache.stats()
  }

  // Creates the Scene that will be displayed for the given node by the Renderer, and is retained
  // for interaction purposes
  pub fn generate_scene(
    &mut self,
    network_name: &str,
    node_id: u64,
    _display_type: NodeDisplayType, //TODO: use display_type
    registry: &NodeTypeRegistry,
    geometry_visualization_preferences: &GeometryVisualizationPreferences,
    top_level_parameters: Option<HashMap<String, NetworkResult>>,
  ) -> NodeSceneData {
    //let _timer = Timer::new("generate_scene");
    
    let network = match registry.node_networks.get(network_name) {
      Some(network) => network,
      None => return NodeSceneData::new(NodeOutput::None),
    };
    
    // Do not evaluate invalid networks
    if !network.valid {
      return NodeSceneData::new(NodeOutput::None);
    }

    let mut context = NetworkEvaluationContext::new();
    if let Some(params) = top_level_parameters {
      context.top_level_parameters = params;
    }

    let mut network_stack = Vec::new();
    // We assign the root node network zero node id. It is not used in the evaluation.
    network_stack.push(NetworkStackElement { node_network: network, node_id: 0 });

    let node = match network.nodes.get(&node_id) {
      Some(node) => node,
      None => return NodeSceneData::new(NodeOutput::None),
    };

    let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);
    let result = {
      //let _timer = Timer::new("evaluate inside generate_scene");
      self.evaluate(&network_stack, node_id, 0, registry, from_selected_node, &mut context)
    };
    
    // Get the unit cell before the result is potentially moved
    let unit_cell = result.get_unit_cell();

    // Determine output and geo_tree based on node type and visualization preferences
    let (output, geo_tree) = 
    if registry.get_node_type_for_node(node).unwrap().output_type == DataType::DrawingPlane {
      if let NetworkResult::DrawingPlane(drawing_plane) = result {
        (NodeOutput::DrawingPlane(drawing_plane), None)
      } else {
        (NodeOutput::None, None)
      }
    }
    else if registry.get_node_type_for_node(node).unwrap().output_type == DataType::Geometry2D {
      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting  {
        if let NetworkResult::Geometry2D(geometry_summary_2d) = result {
          let point_cloud = generate_2d_point_cloud(&geometry_summary_2d.geo_tree_root, &mut context, geometry_visualization_preferences);
          (NodeOutput::SurfacePointCloud2D(point_cloud), Some(geometry_summary_2d.geo_tree_root))
        } else {
          (NodeOutput::None, None)
        }
      } else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        self.generate_explicit_mesh_output(result, &network_stack, node_id, registry, &mut context, geometry_visualization_preferences)
      } else {
        (NodeOutput::None, None)
      }
    }
    else if registry.get_node_type_for_node(node).unwrap().output_type == DataType::Geometry {
      if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::SurfaceSplatting {
        if let NetworkResult::Geometry(geometry_summary) = result {
          let point_cloud = generate_point_cloud(&geometry_summary.geo_tree_root, &mut context, geometry_visualization_preferences);
          (NodeOutput::SurfacePointCloud(point_cloud), Some(geometry_summary.geo_tree_root))
        } else {
          (NodeOutput::None, None)
        }
} else if geometry_visualization_preferences.geometry_visualization == GeometryVisualization::ExplicitMesh {
        self.generate_explicit_mesh_output(result, &network_stack, node_id, registry, &mut context, geometry_visualization_preferences)
      } else {
        (NodeOutput::None, None)
      }
    }
    else if registry.get_node_type_for_node(node).unwrap().output_type == DataType::Atomic {
      if let NetworkResult::Atomic(atomic_structure) = result {
        let mut cloned_atomic_structure = atomic_structure.clone();
        cloned_atomic_structure.decorator_mut().from_selected_node = from_selected_node;
        (NodeOutput::Atomic(cloned_atomic_structure), None)
      } else {
        (NodeOutput::None, None)
      }
    } else {
      (NodeOutput::None, None)
    };

    // Build NodeSceneData
    let mut node_data = NodeSceneData {
      output,
      geo_tree,
      node_errors: context.node_errors.clone(),
      node_output_strings: context.node_output_strings.clone(),
      unit_cell,
      selected_node_eval_cache: context.selected_node_eval_cache,
    };

    return node_data;
  }

  fn generate_explicit_mesh_output<'a>(
    &mut self,
    result: NetworkResult,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64, _registry: &NodeTypeRegistry,
    _context: &mut NetworkEvaluationContext,
    geometry_visualization_preferences: &GeometryVisualizationPreferences) -> (NodeOutput, Option<GeoNode>) {
      //let _timer = Timer::new("generate_explicit_mesh_output");
      let from_selected_node = network_stack.last().unwrap().node_network.selected_node_id == Some(node_id);
      
      let poly_mesh = match &result {
        NetworkResult::Geometry(geometry_summary) => { 
          if let Some(csg_mesh) = geometry_summary.geo_tree_root.to_csg_mesh_cached(Some(&mut self.csg_conversion_cache)) {
            let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();
            let is_half_space = node.node_type_name == "half_space";
            let mut poly_mesh = convert_csg_mesh_to_poly_mesh(
              &csg_mesh,
              is_half_space,
              is_half_space,
            );
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
            Some(poly_mesh)
          } else {
            None
          }
        },
        NetworkResult::Geometry2D(geometry_summary_2d) => {
          if let Some(csg_sketch) = geometry_summary_2d.geo_tree_root.to_csg_sketch_cached(Some(&mut self.csg_conversion_cache)) {
            let mut poly_mesh = convert_csg_sketch_to_poly_mesh(
              csg_sketch, 
              !geometry_visualization_preferences.wireframe_geometry,
              &geometry_summary_2d.drawing_plane,
            );
            poly_mesh.detect_sharp_edges(
              geometry_visualization_preferences.sharpness_angle_threshold_degree,
              true
            );
            Some(poly_mesh)
          } else {
            None
          }
        },
        _ => None,
      };

      // Extract geo_tree_root from the result based on its type
      let geo_tree = match result {
        NetworkResult::Geometry(geometry_summary) => Some(geometry_summary.geo_tree_root),
        NetworkResult::Geometry2D(geometry_summary_2d) => Some(geometry_summary_2d.geo_tree_root),
        _ => None,
      };
      
      // Return output and geo_tree
      let output = if let Some(mesh) = poly_mesh {
        NodeOutput::PolyMesh(mesh)
      } else {
        NodeOutput::None
      };
      
      return (output, geo_tree);
  }


  /// Helper method for the common pattern: get value from node data, or override with input pin
  /// Returns the input pin value if connected, otherwise returns the default value
  /// If the input pin evaluation results in an error, returns that error
  pub fn evaluate_or_default<'a, T>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
    default_value: T,
    extractor: impl FnOnce(NetworkResult) -> Option<T>,
  ) -> Result<T, NetworkResult> {
    let result = self.evaluate_arg(network_stack, node_id, registry, context, parameter_index);
    
    if let NetworkResult::None = result {
      return Ok(default_value);
    }
    
    // Check for error first
    if result.is_error() {
      return Err(result);
    }
    
    // Try to extract the value
    if let Some(value) = extractor(result) {
      Ok(value)
    } else {
      Ok(default_value)
    }
  }

  /// Helper method for the common pattern: get value from required input pin
  /// Returns the input pin value if connected, otherwise returns the missing input error
  /// If the input pin evaluation results in an error, returns that error
  pub fn evaluate_required<'a, T>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
    extractor: impl FnOnce(NetworkResult) -> Option<T>,
  ) -> Result<T, NetworkResult> {
    let result = self.evaluate_arg_required(network_stack, node_id, registry, context, parameter_index);
    
    // Check for error first
    if result.is_error() {
      return Err(result);
    }
    
    // Try to extract the value
    if let Some(value) = extractor(result.clone()) {
      Ok(value)
    } else {
      Err(result)
    }
  }

  // Evaluates an argument of a node.
  // Can return an Error NetworkResult, or a valid NetworkResult.
  // If the atgument is not connected that is an error.
  // If the return value is not an Error, it is guaranteed to be converted to the
  // type of the parameter.
  pub fn evaluate_arg_required<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
  ) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let input_name = registry.get_parameter_name(&node, parameter_index);
    let result = self.evaluate_arg(network_stack, node_id, registry, context, parameter_index);
    if let NetworkResult::None = result {
      input_missing_error(&input_name)
    } else {
      result
    }
  }

  // Evaluates an argument of a node.
  // Can return a NetworkResult::None, NetworkResult::Error, or a valid NetworkResult.
  // Returns NetworkResult::None if the input was not connected.
  // If the return value is not an Error or None, it is guaranteed to be converted to the
  // type of the parameter.
  pub fn evaluate_arg<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
  ) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let input_name = registry.get_parameter_name(&node, parameter_index);

    // Get the expected input type for this parameter
    let expected_type = registry.get_node_param_data_type(node, parameter_index);

    if expected_type.is_array() {
      let input_output_pins = &node.arguments[parameter_index].argument_output_pins;

      if input_output_pins.is_empty() {
        return NetworkResult::None; // Nothing is connected
      }

      let mut merged_items = Vec::new();

      // Sort by node ID to ensure deterministic evaluation order
      // (HashMap iteration order is non-deterministic)
      let mut sorted_pins: Vec<_> = input_output_pins.iter().collect();
      sorted_pins.sort_by_key(|&(&node_id, _)| node_id);

      for (&input_node_id, &input_node_output_pin_index) in sorted_pins {
        let result = self.evaluate(
          network_stack,
          input_node_id,
          input_node_output_pin_index,
          registry,
          false,
          context,
        );

        if let NetworkResult::Error(_) = result {
          return error_in_input(&input_name);
        }

        let input_node = NetworkStackElement::get_top_node(network_stack, input_node_id);
        let input_node_output_type = registry.get_node_type_for_node(input_node).unwrap().output_type.clone();

        // convert_to handles conversion to array types, so we can convert directly.
        // The result is guaranteed to be an array, containing one or more elements.
        let converted_result = result.convert_to(&input_node_output_type, &expected_type.clone());

        if let NetworkResult::Array(array_data) = converted_result {
          merged_items.extend(array_data);
        } else {
          // This should not happen based on the logic of convert_to, but we handle it just in case.
          return error_in_input(&input_name);
        }
      }

      NetworkResult::Array(merged_items)
    }
    else { // single argument evaluation
      if let Some((input_node_id, input_node_output_pin_index)) = node.arguments[parameter_index].get_node_id_and_pin() {
        let result = self.evaluate(
          network_stack,
          input_node_id,
          input_node_output_pin_index,
          registry, 
          false,
          context
        );
        if let NetworkResult::Error(_error) = result {
          return error_in_input(&input_name);
        }

        let input_node = NetworkStackElement::get_top_node(network_stack, input_node_id);
        let input_node_type = registry.get_node_type_for_node(input_node);
        let input_node_output_type = input_node_type.unwrap().get_output_pin_type(input_node_output_pin_index);

        // Convert the result to the expected type
        let converted_result = result.convert_to(&input_node_output_type, &expected_type);

        return converted_result;
      } else {
        return NetworkResult::None; // Nothing is connected
      }      
    }
  }

  // Evaluates the specified node (calculates the NetworkResult on its output pin).
  pub fn evaluate<'a>(
    &self,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    output_pin_index: i32,
    registry: &NodeTypeRegistry,
    decorate: bool,
    context: &mut NetworkEvaluationContext) -> NetworkResult {

    let node = network_stack.last().unwrap().node_network.nodes.get(&node_id).unwrap();

    let result = if output_pin_index == (-1) {
      let node_type = registry.get_node_type_for_node(node);
      let num_of_params = node_type.unwrap().parameters.len();
      let mut captured_argument_values: Vec<NetworkResult> = Vec::new();

      for i in 0..num_of_params {
        let result = self.evaluate_arg(network_stack, node_id, registry, context, i);
        captured_argument_values.push(result);
      }

      NetworkResult::Function(Closure {
        node_network_name: network_stack.last().unwrap().node_network.node_type.name.clone(),
        node_id,
        captured_argument_values,
      })
    } else {
      let node = NetworkStackElement::get_top_node(network_stack, node_id);
      if registry.built_in_node_types.contains_key(&node.node_type_name) {
        node.data.eval(&self, network_stack, node_id, registry, decorate, context)
      } else if let Some(child_network) = registry.node_networks.get(&node.node_type_name) { // custom node{
        // Do not evaluate invalid child networks
        if !child_network.valid {
          return NetworkResult::Error(format!("{} is invalid", node.node_type_name));
        }
        let mut child_network_stack = network_stack.clone();
        child_network_stack.push(NetworkStackElement { node_network: child_network, node_id });
        if child_network.return_node_id.is_none() {
          return NetworkResult::Error(format!("{} has no return node", node.node_type_name));
        }
        let result = self.evaluate(&child_network_stack, child_network.return_node_id.unwrap(), 0, registry, false, context);
        if let NetworkResult::Error(_error) = &result {
          NetworkResult::Error(format!("Error in {}", node.node_type_name))
        } else { result }        
      } else {
        NetworkResult::Error(format!("Unknown node type: {}", node.node_type_name))
      }
    };

    // Check for error and store it in the context
    if let NetworkResult::Error(error_message) = &result {
      context.node_errors.insert(node_id, error_message.clone());
    }
    
    context.node_output_strings.insert(node_id, result.to_display_string());   
    
    result
  }

}


