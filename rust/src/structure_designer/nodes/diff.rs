use std::collections::HashSet;

use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_evaluator::input_missing_error;
use crate::structure_designer::evaluator::network_evaluator::error_in_input;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use glam::f64::DQuat;
use crate::common::csg_types::CSG;

pub fn implicit_eval_diff<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {

  let ubase = node.arguments[0].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX);

  let usub = node.arguments[1].argument_node_ids.iter().map(|node_id| {
    evaluator.implicit_eval(network_stack, *node_id, sample_point, registry)[0]
  }).reduce(f64::min).unwrap_or(f64::MAX);

  return f64::max(ubase, -usub)
}

pub fn eval_diff<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  //let _timer = Timer::new("eval_diff");
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  if node.arguments[0].argument_node_ids.is_empty() {
    return input_missing_error("base");
  }

  let (mut geometry, mut frame_translation) = helper_union(
    network_evaluator,
    network_stack,
    &node.arguments[0].argument_node_ids,
    registry,
    context
  );

  if geometry.is_none() {
    return error_in_input("base");
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
      return error_in_input("sub");
    }

    geometry = Some(geometry.unwrap().difference(&sub_geometry.unwrap()));
    //geometry = Some(geometry.unwrap().intersection(&sub_geometry.unwrap().inverse()));

    frame_translation += sub_frame_translation;
    frame_translation *= 0.5;
  }

  //let cube1 = CSG::cube(3.0, 3.0, 3.0, None).translate(1.0, 1.0, 1.0);  // 2×2×2 cube at origin, no metadata
  //let cube2 = cube1.translate(2.0, 2.0, 2.0); 
  //let difference_result = cube1.difference(&cube2);

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
      frame_translation,
      DQuat::IDENTITY,
    ),
    csg: geometry.unwrap(), // difference_result,
  });
}

fn helper_union<'a>(network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  argument_node_ids: &HashSet<u64>,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> (Option<CSG>, DVec3) {
  let mut geometry = None;
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