use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec3_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;

#[derive(Debug, Serialize, Deserialize)]
pub struct Vec3Data {
  #[serde(with = "dvec3_serializer")]
  pub value: DVec3,
}

impl NodeData for Vec3Data {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_vec3<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let vec3_data = &node.data.as_any_ref().downcast_ref::<Vec3Data>().unwrap();

  let x = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 0, 
    vec3_data.value.x, 
    NetworkResult::extract_float
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let y = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, 
    vec3_data.value.y, 
    NetworkResult::extract_float
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let z = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 2, 
    vec3_data.value.z, 
    NetworkResult::extract_float
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  return NetworkResult::Vec3(DVec3{x, y, z});  
}
