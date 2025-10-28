use crate::structure_designer::node_data::NodeData;
use crate::common::gadget::Gadget;

pub trait NodeNetworkGadget: Gadget {
    // Syncs the gadget's state into the node data
    // called every frame, node data is refreshed every frame
    // (tough we only do a lightweight scene refresh when dragging, so no network evaluation each frame.)
    fn sync_data(&self, data: &mut dyn NodeData);
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget>;    
}
