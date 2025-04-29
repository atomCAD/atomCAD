use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::gadgets::half_space_gadget::HalfSpaceGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;

#[derive(Debug, Serialize, Deserialize)]
pub struct HalfSpaceData {
  #[serde(with = "ivec3_serializer")]
  pub miller_index: IVec3,
  pub shift: i32,
}

impl NodeData for HalfSpaceData {

    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      return Some(Box::new(HalfSpaceGadget::new(&self.miller_index, self.shift)));
    }
  
}
