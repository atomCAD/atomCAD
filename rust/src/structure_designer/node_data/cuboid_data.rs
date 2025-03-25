use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;

#[derive(Debug)]
pub struct CuboidData {
  pub min_corner: IVec3,
  pub extent: IVec3,
}

impl NodeData for CuboidData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
