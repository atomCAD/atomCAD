use crate::structure_designer::node_data::node_data::NodeData;
use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;

#[derive(Debug)]
pub struct NoData {
}

impl NodeData for NoData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
