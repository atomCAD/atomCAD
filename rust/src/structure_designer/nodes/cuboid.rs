use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::util::transform::Transform;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::common::csg_types::CSG;
use std::collections::HashMap;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationId;

#[derive(Debug, Serialize, Deserialize)]
pub struct CuboidData {
  #[serde(with = "ivec3_serializer")]
  pub min_corner: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub extent: IVec3,
}

impl NodeData for CuboidData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_cuboid<'a>(
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let cuboid_data = &node.data.as_any_ref().downcast_ref::<CuboidData>().unwrap();

  let min_corner = cuboid_data.min_corner.as_dvec3();
  let extent = cuboid_data.extent.as_dvec3();
  let center = min_corner + extent / 2.0;

  let geometry = if context.explicit_geo_eval_needed { CSG::cube(extent.x, extent.y, extent.z, None)
    .translate(min_corner.x, min_corner.y, min_corner.z) } else { CSG::new() };

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
      center,
      DQuat::IDENTITY,
    ),
    csg: geometry,
  });
}

pub fn implicit_eval_cuboid<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _invocation_cache: &HashMap<NodeInvocationId, NetworkResult>,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
  let cuboid_data = &node.data.as_any_ref().downcast_ref::<CuboidData>().unwrap();

  let max_corner = cuboid_data.min_corner + cuboid_data.extent;
  let x_val = f64::max((cuboid_data.min_corner.x as f64) - sample_point.x, sample_point.x - (max_corner.x as f64));
  let y_val = f64::max((cuboid_data.min_corner.y as f64) - sample_point.y, sample_point.y - (max_corner.y as f64));
  let z_val = f64::max((cuboid_data.min_corner.z as f64) - sample_point.z, sample_point.z - (max_corner.z as f64));

  return f64::max(f64::max(x_val, y_val), z_val);
}