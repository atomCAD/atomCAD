use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use glam::f64::DQuat;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffData {
}

impl NodeData for DiffData {
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
    context: &mut NetworkEvaluationContext,
  ) -> NetworkResult {
    //let _timer = Timer::new("eval_diff");
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let base_input_name = registry.get_parameter_name(&node, 0);
    let sub_input_name = registry.get_parameter_name(&node, 1);
  
    if node.arguments[0].is_empty() {
      return input_missing_error(&base_input_name);
    }
  
    let (mut geometry, mut frame_translation) = helper_union(
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
  
    if !node.arguments[1].is_empty() {
      let (sub_geometry, sub_frame_translation) = helper_union(
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
  
      geometry = Some(GeoNode::Difference3D { base: Box::new(geometry.unwrap()), sub: Box::new(sub_geometry.unwrap()) });
  
      frame_translation += sub_frame_translation;
      frame_translation *= 0.5;
    }
  
    return NetworkResult::Geometry(GeometrySummary { 
      frame_transform: Transform::new(
        frame_translation,
        DQuat::IDENTITY,
      ),
      geo_tree_root: geometry.unwrap(),
    });
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }
}

fn helper_union<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  parameter_index: usize,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> (Option<GeoNode>, DVec3) {

  let mut shapes: Vec<GeoNode> = Vec::new();
  let mut frame_translation = DVec3::ZERO;

  let shapes_val = network_evaluator.evaluate_arg_required(
    network_stack,
    node_id,
    registry,
    context,
    parameter_index,
  );

  if let NetworkResult::Error(_) = shapes_val {
    return (None, DVec3::ZERO);
  }

  // Extract the array elements from shapes_val
  let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
    array_elements
  } else {
    return (None, DVec3::ZERO);
  };

  let shape_count = shape_results.len();

  for shape_val in shape_results {
    if let NetworkResult::Geometry(shape) = shape_val {
      shapes.push(shape.geo_tree_root); 
      frame_translation += shape.frame_transform.translation;
    }
  }

  frame_translation /= shape_count as f64;

  return (Some(GeoNode::Union3D { shapes }), frame_translation);
}
