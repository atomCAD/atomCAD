
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;

#[derive(Debug, Serialize, Deserialize)]
pub struct StampPlacement {
  #[serde(with = "ivec3_serializer")]
  pub position: IVec3,
  pub primary_orientation: i32, // 0 - 7
  pub secondary_orientation: i32, // 0 - 2
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StampData {
  pub stamp_placements: Vec<StampPlacement>,
}

impl NodeData for StampData {
  fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }
}

impl StampData {
  pub fn new() -> Self {
      Self {
          stamp_placements: Vec::new(),
      }
  }
}

pub fn eval_stamp<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {  
  return NetworkResult::None;
  //TODO
}
