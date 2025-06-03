use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use glam::i32::IVec2;
use glam::f64::DVec2;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;

#[derive(Debug, Serialize, Deserialize)]
pub struct HalfPlaneData {
  #[serde(with = "ivec2_serializer")]
  pub point1: IVec2,
  #[serde(with = "ivec2_serializer")]
  pub point2: IVec2,
}

impl NodeData for HalfPlaneData {

    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
  
}

pub fn eval_half_plane<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let half_plane_data = &node.data.as_any_ref().downcast_ref::<HalfPlaneData>().unwrap();

  // Convert point1 to double precision for calculations
  let point1 = half_plane_data.point1.as_dvec2();
  
  // Calculate direction vector from point1 to point2
  let dir_vector = (half_plane_data.point2 - half_plane_data.point1).as_dvec2();
  let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
  
  // Use point1 as the position and calculate the angle for the transform
  return NetworkResult::Geometry2D(GeometrySummary2D { frame_transform: Transform2D::new(
    point1,
    normal.x.atan2(normal.y), // Angle from Y direction to normal in radians
  )});
}

pub fn implicit_eval_half_plane<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {
  let half_plane_data = &node.data.as_any_ref().downcast_ref::<HalfPlaneData>().unwrap();
  
  // Convert points to double precision for calculations
  let point1 = half_plane_data.point1.as_dvec2();
  let point2 = half_plane_data.point2.as_dvec2();
  
  // Calculate line direction and normal
  let dir_vector = point2 - point1;
  let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
  
  // Calculate signed distance from sample_point to the line
  // Formula: distance = normal·(sample_point - point1)
  return normal.dot(*sample_point - point1);
}
