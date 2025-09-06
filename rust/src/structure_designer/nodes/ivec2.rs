use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;

#[derive(Debug, Serialize, Deserialize)]
pub struct IVec2Data {
  #[serde(with = "ivec2_serializer")]
  pub value: IVec2,
}

impl NodeData for IVec2Data {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_ivec2<'a>(
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  _context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let ivec2_data = &node.data.as_any_ref().downcast_ref::<IVec2Data>().unwrap();

  return NetworkResult::IVec2(ivec2_data.value);
}
