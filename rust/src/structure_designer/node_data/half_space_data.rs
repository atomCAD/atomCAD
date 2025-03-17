use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::gadget::Gadget;
use crate::structure_designer::gadgets::half_space_gadget::HalfSpaceGadget;
use glam::i32::IVec3;

#[derive(Debug)]
pub struct HalfSpaceData {
  pub miller_index: IVec3,
  pub shift: i32,
}

impl NodeData for HalfSpaceData {

    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      return Some(Box::new(HalfSpaceGadget::new(&self.miller_index, self.shift)));
    }
  
}
