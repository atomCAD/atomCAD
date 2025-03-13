use crate::kernel::node_data::node_data::NodeData;
use crate::kernel::gadgets::gadget::Gadget;

#[derive(Debug)]
pub struct ParameterData {
  pub param_index: usize,
}

impl NodeData for ParameterData {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>> {
      None
    }
}
