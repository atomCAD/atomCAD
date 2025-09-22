use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use glam::f64::DQuat;

pub fn eval_union<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  //let _timer = Timer::new("eval_union");

  let mut shapes: Vec<GeoNode> = Vec::new();
  let mut frame_translation = DVec3::ZERO;

  let shapes_val = network_evaluator.evaluate_arg_required(
    network_stack,
    node_id,
    registry,
    context,
    0,
  );

  if let NetworkResult::Error(_) = shapes_val {
    return shapes_val;
  }

  // Extract the array elements from shapes_val
  let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
    array_elements
  } else {
    return NetworkResult::Error("Expected array of geometry shapes".to_string());
  };

  let shape_count = shape_results.len();

  for shape_val in shape_results {
    if let NetworkResult::Geometry(shape) = shape_val {
      shapes.push(shape.geo_tree_root); 
      frame_translation += shape.frame_transform.translation;
    }
  }

  frame_translation /= shape_count as f64;

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
      frame_translation,
      DQuat::IDENTITY,
    ),
    geo_tree_root: GeoNode::Union3D { shapes },
  });
}