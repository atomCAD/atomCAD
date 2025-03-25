use glam::f64::DVec3;
use crate::structure_designer::node_data::node_data::NodeData;
use crate::common::gadget::Gadget;

pub trait NodeNetworkGadget: Gadget {
    // Syncs the gadget's state into the node data
    fn sync_data(&self, data: &mut dyn NodeData);
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget>;    
}
