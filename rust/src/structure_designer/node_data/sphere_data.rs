use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::gadget::Gadget;
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
