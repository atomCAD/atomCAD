use crate::structure_designer::gadgets::node_network_gadget::NodeNetworkGadget;
use std::any::Any;
use crate::util::as_any::AsAny;

pub trait NodeData: Any + AsAny  {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>>;
}
