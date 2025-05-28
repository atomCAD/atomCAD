use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::util::transform::Transform2D;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use glam::f64::DVec2;

#[derive(Debug, Serialize, Deserialize)]
pub struct CircleData {
  #[serde(with = "ivec2_serializer")]
  pub center: IVec2,
  pub radius: i32,
}

impl NodeData for CircleData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_circle<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let circle_data = &node.data.as_any_ref().downcast_ref::<CircleData>().unwrap();

  return NetworkResult::Geometry2D(GeometrySummary2D { frame_transform: Transform2D::new(
    circle_data.center.as_dvec2() * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
    0.0,
  ) });
}

pub fn implicit_eval_circle<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {
  let sphere_data = &node.data.as_any_ref().downcast_ref::<CircleData>().unwrap();

  return (sample_point - DVec2::new(sphere_data.center.x as f64, sphere_data.center.y as f64)).length() 
    - (sphere_data.radius as f64);
}
