use crate::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::unit_cell_mismatch_error;
use crate::crystolecule::drawing_plane::DrawingPlane;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff2DData {
}

impl NodeData for Diff2DData {
  fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }

  fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
    None
  }

  fn eval<'a>(
    &self,
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    _decorate: bool,
    context: &mut NetworkEvaluationContext,
  ) -> NetworkResult {
    //let _timer = Timer::new("eval_diff");
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let base_input_name = registry.get_parameter_name(node, 0);
    let sub_input_name = registry.get_parameter_name(node, 1);
  
    if node.arguments[0].is_empty() {
      return input_missing_error(&base_input_name);
    }
  
    let (mut geometry, mut frame_translation, base_drawing_plane) = helper_union(
      network_evaluator,
      network_stack,
      node_id,
      0,
      registry,
      context
    );
  
    if geometry.is_none() {
      return error_in_input(&base_input_name);
    }
    
    if base_drawing_plane.is_none() {
      return unit_cell_mismatch_error();
    }
    
    let result_drawing_plane = base_drawing_plane.unwrap();
  
    if !node.arguments[1].is_empty() {
      let (sub_geometry, sub_frame_translation, sub_drawing_plane) = helper_union(
        network_evaluator,
        network_stack,
        node_id,
        1,
        registry,
        context
      );
    
      if sub_geometry.is_none() {
        return error_in_input(&sub_input_name);
      }
      
      if sub_drawing_plane.is_none() {
        return unit_cell_mismatch_error();
      }
      
      // Check drawing plane compatibility between base and sub
      if !result_drawing_plane.is_compatible(&sub_drawing_plane.unwrap()) {
        return unit_cell_mismatch_error();
      }
  
      geometry = Some(GeoNode::difference_2d(Box::new(geometry.unwrap()), Box::new(sub_geometry.unwrap())));
  
      frame_translation += sub_frame_translation;
      frame_translation *= 0.5;
    }
  
    NetworkResult::Geometry2D(GeometrySummary2D { 
      drawing_plane: result_drawing_plane,
      frame_transform: Transform2D::new(
        frame_translation,
        0.0,
      ),
      geo_tree_root: geometry.unwrap(),
    })
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }

  fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      None
  }

  fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
      let mut m = std::collections::HashMap::new();
      m.insert("base".to_string(), (true, None)); // required
      m.insert("sub".to_string(), (true, None)); // required
      m
  }
}

fn helper_union<'a>(network_evaluator: &NetworkEvaluator,
  network_stack: &[NetworkStackElement<'a>],
  node_id: u64,
  parameter_index: usize,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> (Option<GeoNode>, DVec2, Option<DrawingPlane>) {
  let mut shapes: Vec<GeoNode> = Vec::new();
  let mut frame_translation = DVec2::ZERO;

  let shapes_val = network_evaluator.evaluate_arg_required(
    network_stack,
    node_id,
    registry,
    context,
    parameter_index,
  );

  if let NetworkResult::Error(_) = shapes_val {
    return (None, DVec2::ZERO, None);
  }

  // Extract the array elements from shapes_val
  let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
    array_elements
  } else {
    return (None, DVec2::ZERO, None);
  };

  let shape_count = shape_results.len();
  
  if shape_count == 0 {
    return (None, DVec2::ZERO, None);
  }

  // Extract geometries and check unit cell compatibility
  let mut geometries: Vec<GeometrySummary2D> = Vec::new();
  for shape_val in shape_results {
    if let NetworkResult::Geometry2D(shape) = shape_val {
      geometries.push(shape);
    } else {
      return (None, DVec2::ZERO, None);
    }
  }
  
  // Check drawing plane compatibility - compare all to the first geometry
  if !GeometrySummary2D::all_have_compatible_drawing_planes(&geometries) {
    return (None, DVec2::ZERO, None);
  }
  
  // All drawing planes are compatible, proceed with union
  let first_drawing_plane = geometries[0].drawing_plane.clone();
  for geometry in geometries.into_iter() {
    shapes.push(geometry.geo_tree_root);
    frame_translation += geometry.frame_transform.translation;
  }

  frame_translation /= shape_count as f64;
  (Some(GeoNode::union_2d(shapes)), frame_translation, Some(first_drawing_plane))
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "diff_2d".to_string(),
      description: "Computes the Boolean difference of two 2D geometries.".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
          Parameter {
              id: None,
              name: "base".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)), // A set of shapes to subtract from
          },
          Parameter {
              id: None,
              name: "sub".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry2D)), // A set of shapes to subtract from base
          },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(Diff2DData {}),
      node_data_saver: generic_node_data_saver::<Diff2DData>,
      node_data_loader: generic_node_data_loader::<Diff2DData>,
  }
}













