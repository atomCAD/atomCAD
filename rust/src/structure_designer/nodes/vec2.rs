use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec2_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;

#[derive(Debug, Serialize, Deserialize)]
pub struct Vec2Data {
  #[serde(with = "dvec2_serializer")]
  pub value: DVec2,
}

impl NodeData for Vec2Data {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_vec2<'a>(
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  _context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let vec2_data = &node.data.as_any_ref().downcast_ref::<Vec2Data>().unwrap();

  return NetworkResult::Vec2(vec2_data.value);
}
