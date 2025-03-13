use crate::kernel::node_data::node_data::NodeData;
use crate::kernel::gadgets::gadget::Gadget;
use glam::i32::IVec3;

#[derive(Debug)]
pub struct GeoTransData {
  pub translation: IVec3,
  pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees.
}

impl NodeData for GeoTransData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      None
    }
}
