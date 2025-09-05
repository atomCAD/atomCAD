use crate::util::transform::Transform2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec2;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::geo_tree::GeoNode;

pub fn eval_union_2d<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  //let _timer = Timer::new("eval_union");
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let shapes_input_name = registry.get_parameter_name(&node.node_type_name, 0);

  if node.arguments[0].is_empty() {
    return input_missing_error(&shapes_input_name);
  }

  let mut shapes: Vec<GeoNode> = Vec::new();
  let mut frame_translation = DVec2::ZERO;
  for input_node_id in node.arguments[0].argument_node_ids.iter() {
    let shape_val = network_evaluator.evaluate(
      network_stack,
      *input_node_id,
      registry, 
      false,
      context
    )[0].clone();
    if let NetworkResult::Error(_error) = shape_val {
      return error_in_input(&shapes_input_name);
    }
    else if let NetworkResult::Geometry2D(shape) = shape_val {
      shapes.push(shape.geo_tree_root);
      frame_translation += shape.frame_transform.translation;
    }
  }

  frame_translation /= node.arguments[0].argument_node_ids.len() as f64;

  return NetworkResult::Geometry2D(GeometrySummary2D { 
    frame_transform: Transform2D::new(
      frame_translation,
      0.0,
    ),
    geo_tree_root: GeoNode::Union2D { shapes },
  });
}
