use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use std::any::Any;
use crate::util::as_any::AsAny;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;


pub trait NodeData: Any + AsAny  {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoData {
}

impl NodeData for NoData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
