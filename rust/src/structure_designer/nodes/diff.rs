use std::collections::HashSet;

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

pub fn eval_diff<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  //let _timer = Timer::new("eval_diff");
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let base_input_name = registry.get_parameter_name(&node.node_type_name, 0);
  let sub_input_name = registry.get_parameter_name(&node.node_type_name, 1);

  if node.arguments[0].argument_node_ids.is_empty() {
    return input_missing_error(&base_input_name);
  }

  let (mut geometry, mut frame_translation) = helper_union(
    network_evaluator,
    network_stack,
    &node.arguments[0].argument_node_ids,
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
      &node.arguments[1].argument_node_ids,
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

fn helper_union<'a>(network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  argument_node_ids: &HashSet<u64>,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> (Option<GeoNode>, DVec3) {
  let mut shapes: Vec<GeoNode> = Vec::new();
  let mut frame_translation = DVec3::ZERO;
  for input_node_id in argument_node_ids.iter() {
    let shape_val = network_evaluator.evaluate(
      network_stack,
      *input_node_id,
      registry, 
      false,
      context
    )[0].clone();
    if let NetworkResult::Error(_error) = shape_val {
      return (None, DVec3::ZERO);
    }
    else if let NetworkResult::Geometry(shape) = shape_val {
      shapes.push(shape.geo_tree_root);
      frame_translation += shape.frame_transform.translation;
    }
  }
  frame_translation /= argument_node_ids.len() as f64;
  return (Some(GeoNode::Union3D { shapes }), frame_translation);
}
