use crate::structure_editor::node_data::node_data::NodeData;
use crate::structure_editor::gadgets::gadget::Gadget;
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
