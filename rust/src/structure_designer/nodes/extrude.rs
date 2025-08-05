use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use glam::{DQuat, Vec3Swizzles};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::input_missing_error;
use crate::structure_designer::evaluator::network_evaluator::error_in_input;
use crate::common::csg_types::CSG;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationCache;
use crate::structure_designer::structure_designer::StructureDesigner;


#[derive(Debug, Serialize, Deserialize)]
pub struct ExtrudeData {
  pub height: i32,
}

impl NodeData for ExtrudeData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_extrude<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  //let _timer = Timer::new("eval_extrude");
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let shape_input_name = registry.get_parameter_name(&node.node_type_name, 0);
  let extrude_data = &node.data.as_any_ref().downcast_ref::<ExtrudeData>().unwrap();

  if node.arguments[0].is_empty() {
    return input_missing_error(&shape_input_name);
  }

  let input_node_id = node.arguments[0].get_node_id().unwrap();
  let shape_val = &network_evaluator.evaluate(
    network_stack,
    input_node_id,
    registry,
    false,
    context
  )[0];

  if let NetworkResult::Error(_error) = shape_val {
    return error_in_input(&shape_input_name);
  }
  if let NetworkResult::Geometry2D(shape) = shape_val {
    let extruded_geometry = if context.explicit_geo_eval_needed {
      let mut extruded = shape.csg.extrude(extrude_data.height as f64);

      // swap y and z coordinates
      for polygon in &mut extruded.polygons {        
        for vertex in &mut polygon.vertices {
            let tmp = vertex.pos.y;
            vertex.pos.y = vertex.pos.z;
            vertex.pos.z = tmp;

            let tmp_norm = vertex.normal.y;
            vertex.normal.y = vertex.normal.z;
            vertex.normal.z = tmp_norm;
        }
      }

      extruded.inverse()
    } else {
      CSG::new()
    };
    let frame_translation_2d = shape.frame_transform.translation;

    return NetworkResult::Geometry(GeometrySummary { 
      frame_transform: Transform::new(
        DVec3::new(frame_translation_2d.x, 0.0, frame_translation_2d.y),
        DQuat::from_rotation_y(shape.frame_transform.rotation),
      ),
      csg: extruded_geometry,
    });
  } else {
    return error_in_input(&shape_input_name);
  }
}

pub fn implicit_eval_extrude<'a>(
  evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  invocation_cache: &NodeInvocationCache,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
  let extrude_data = &node.data.as_any_ref().downcast_ref::<ExtrudeData>().unwrap();

  let y_val = f64::max(-sample_point.y, sample_point.y - (extrude_data.height as f64));

  let input_val = match node.arguments[0].get_node_id() {
    Some(node_id) => evaluator.implicit_eval_2d(
        network_stack,
        node_id, 
        &sample_point.xz(),
        registry,
        invocation_cache)[0],
    None => f64::MAX
  };

  return f64::max(y_val, input_val);
}
