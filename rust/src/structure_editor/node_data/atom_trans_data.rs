use crate::structure_editor::node_data::node_data::NodeData;
use crate::structure_editor::gadgets::gadget::Gadget;
use crate::structure_editor::gadgets::atom_trans_gadget::AtomTransGadget;
use glam::f32::Vec3;

#[derive(Debug)]
pub struct AtomTransData {
  pub translation: Vec3,
  pub rotation: Vec3, // intrinsic euler angles in radians
}

impl NodeData for AtomTransData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      return Some(Box::new(AtomTransGadget::new(self.translation, self.rotation)));
    }
}
