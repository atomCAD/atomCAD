use std::collections::HashSet;

use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use crate::structure_designer::evaluator::network_evaluator::{GeometrySummary2D, NetworkEvaluationContext};
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::input_missing_error;
use crate::structure_designer::evaluator::network_evaluator::error_in_input;
use crate::common::csg_types::CSG;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationCache;

pub fn implicit_eval_diff_2d<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  invocation_cache: &NodeInvocationCache,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {

  let ubase = node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval_2d(network_stack, *node_id, sample_point, registry, invocation_cache)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX);

  let usub = node.arguments[1].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval_2d(network_stack, *node_id, sample_point, registry, invocation_cache)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX);

  return f64::max(ubase, -usub)
}

pub fn eval_diff_2d<'a>(
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

  if node.arguments[0].is_empty() {
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

  if !node.arguments[1].argument_node_ids.is_empty() {
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

    geometry = Some(geometry.unwrap().difference(&sub_geometry.unwrap()));
    //geometry = Some(geometry.unwrap().intersection(&sub_geometry.unwrap().inverse()));

    frame_translation += sub_frame_translation;
    frame_translation *= 0.5;
  }

  return NetworkResult::Geometry2D(GeometrySummary2D { 
    frame_transform: Transform2D::new(
      frame_translation,
      0.0,
    ),
    csg: geometry.unwrap(),
  });
}

fn helper_union<'a>(network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  argument_node_ids: &HashSet<u64>,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> (Option<CSG>, DVec2) {
  let mut geometry = None;
  let mut frame_translation = DVec2::ZERO;
  for input_node_id in argument_node_ids.iter() {
    let shape_val = network_evaluator.evaluate(
      network_stack,
      *input_node_id,
      registry, 
      false,
      context
    )[0].clone();
    if let NetworkResult::Error(_error) = shape_val {
      return (None, DVec2::ZERO);
    }
    else if let NetworkResult::Geometry2D(shape) = shape_val {
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
  frame_translation /= argument_node_ids.len() as f64;
  return (geometry, frame_translation);
}