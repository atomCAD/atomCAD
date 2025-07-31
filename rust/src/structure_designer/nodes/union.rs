use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::evaluator::network_evaluator::error_in_input;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_evaluator::input_missing_error;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use glam::f64::DQuat;
use crate::common::csg_types::CSG;
use std::collections::HashMap;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationId;

pub fn implicit_eval_union<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  invocation_cache: &HashMap<NodeInvocationId, NetworkResult>,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
node.arguments[0].argument_node_ids.iter().map(|node_id| {
  evaluator.implicit_eval(network_stack, *node_id, sample_point, registry, invocation_cache)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX)
}

pub fn eval_union<'a>(
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

  let mut geometry = None;
  let mut frame_translation = DVec3::ZERO;
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
    else if let NetworkResult::Geometry(shape) = shape_val {
      if context.explicit_geo_eval_needed {
        if geometry.is_none() {
          geometry = Some(shape.csg);
        } else {
          geometry = Some(geometry.unwrap().union(&shape.csg));
        } 
      }
      frame_translation += shape.frame_transform.translation;
    }
  }

  frame_translation /= node.arguments[0].argument_node_ids.len() as f64;

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
      frame_translation,
      DQuat::IDENTITY,
    ),
    csg: geometry.unwrap_or(CSG::new()),
  });
}