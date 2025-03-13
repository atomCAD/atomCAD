use crate::kernel::node_data::node_data::NodeData;
use crate::kernel::gadgets::gadget::Gadget;
use glam::i32::IVec3;

#[derive(Debug)]
pub struct CuboidData {
  pub min_corner: IVec3,
  pub extent: IVec3,
}

impl NodeData for CuboidData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      None
    }
}
