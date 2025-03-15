use crate::structure_editor::node_data::node_data::NodeData;
use crate::structure_editor::gadgets::gadget::Gadget;
use glam::i32::IVec3;

#[derive(Debug)]
pub struct GeoTransData {
  pub translation: IVec3,
  pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees.
  pub transform_only_frame: bool, // If true, only the reference frame is transformed, the geometry remains in place.
}

impl NodeData for GeoTransData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      None
    }
}
