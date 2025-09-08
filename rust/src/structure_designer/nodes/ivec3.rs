use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;

#[derive(Debug, Serialize, Deserialize)]
pub struct IVec3Data {
  #[serde(with = "ivec3_serializer")]
  pub value: IVec3,
}

impl NodeData for IVec3Data {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_ivec3<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let ivec3_data = &node.data.as_any_ref().downcast_ref::<IVec3Data>().unwrap();

  let x = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 0, 
    ivec3_data.value.x, 
    NetworkResult::extract_int
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let y = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, 
    ivec3_data.value.y, 
    NetworkResult::extract_int
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let z = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 2, 
    ivec3_data.value.z, 
    NetworkResult::extract_int
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  return NetworkResult::IVec3(IVec3{x, y, z});
}
