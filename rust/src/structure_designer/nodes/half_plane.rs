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
  pub miller_index: IVec2,
  pub shift: i32,
}

impl NodeData for HalfPlaneData {

    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
  
}

pub fn eval_half_plane<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let half_plane_data = &node.data.as_any_ref().downcast_ref::<HalfPlaneData>().unwrap();

  let dir = half_plane_data.miller_index.as_dvec2().normalize();
  let shift_handle_offset = ((half_plane_data.shift as f64) / half_plane_data.miller_index.as_dvec2().length()) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

  return NetworkResult::Geometry2D(GeometrySummary2D { frame_transform: Transform2D::new(
    dir * shift_handle_offset,
    dir.x.atan2(dir.y), // Angle from Y direction to dir in radians
  )});
}

pub fn implicit_eval_half_plane<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {
  let half_plane_data = &node.data.as_any_ref().downcast_ref::<HalfPlaneData>().unwrap();
  let float_miller = half_plane_data.miller_index.as_dvec2();
  let miller_magnitude = float_miller.length();
  return (float_miller.dot(sample_point.clone()) - (half_plane_data.shift as f64)) / miller_magnitude;
}
