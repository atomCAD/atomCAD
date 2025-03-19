use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::gadget::Gadget;
use crate::structure_designer::gadgets::atom_trans_gadget::AtomTransGadget;
use glam::f64::DVec3;

#[derive(Debug)]
pub struct AtomTransData {
  pub translation: DVec3,
  pub rotation: DVec3, // intrinsic euler angles in radians
}

impl NodeData for AtomTransData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      return Some(Box::new(AtomTransGadget::new(self.translation, self.rotation)));
    }
}
