use crate::kernel::node_data::node_data::NodeData;
use crate::kernel::gadgets::gadget::Gadget;
use glam::i32::IVec3;

#[derive(Debug)]
pub struct SphereData {
  pub center: IVec3,
  pub radius: i32,
}

impl NodeData for SphereData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      None
    }
}
