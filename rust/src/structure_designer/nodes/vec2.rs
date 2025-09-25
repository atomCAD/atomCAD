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
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Serialize, Deserialize)]
pub struct Vec2Data {
  #[serde(with = "dvec2_serializer")]
  pub value: DVec2,
}

impl NodeData for Vec2Data {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }
}

pub fn eval_vec2<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let vec2_data = &node.data.as_any_ref().downcast_ref::<Vec2Data>().unwrap();

  let x = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 0, 
    vec2_data.value.x, 
    NetworkResult::extract_float
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let y = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, 
    vec2_data.value.y, 
    NetworkResult::extract_float
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  return NetworkResult::Vec2(DVec2{x, y});
}
