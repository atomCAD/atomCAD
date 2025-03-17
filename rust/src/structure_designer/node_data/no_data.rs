use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::gadget::Gadget;

#[derive(Debug)]
pub struct NoData {
}

impl NodeData for NoData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      None
    }
}
