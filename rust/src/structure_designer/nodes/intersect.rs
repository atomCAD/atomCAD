use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use glam::f64::DQuat;
use crate::structure_designer::geo_tree::GeoNode;

pub fn eval_intersect<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  //let _timer = Timer::new("eval_intersect");
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let shapes_input_name = registry.get_parameter_name(&node, 0);

  if node.arguments[0].is_empty() {
    return input_missing_error(&shapes_input_name);
  }

  let mut shapes: Vec<GeoNode> = Vec::new();
  let mut frame_translation = DVec3::ZERO;
  for input_node_id in node.arguments[0].argument_node_ids.iter() {
    let shape_val = network_evaluator.evaluate(
      network_stack,
      *input_node_id,
      registry, 
      false,
      context
    );
    if let NetworkResult::Error(_error) = shape_val {
      return error_in_input(&shapes_input_name);
    }
    else if let NetworkResult::Geometry(shape) = shape_val {
      shapes.push(shape.geo_tree_root);
      frame_translation += shape.frame_transform.translation;
    }
  }

  frame_translation /= node.arguments[0].argument_node_ids.len() as f64;

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
      frame_translation,
      DQuat::IDENTITY,
    ),
    geo_tree_root: GeoNode::Intersection3D { shapes },
  });
}