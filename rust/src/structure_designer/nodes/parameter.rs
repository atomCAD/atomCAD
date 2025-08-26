use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::DataType;

#[derive(Debug, Serialize, Deserialize)]
pub struct ParameterData {
  pub param_index: usize,
  pub param_name: String,
  pub data_type: DataType,
  pub multi: bool,
  pub sort_order: i32,
}

impl NodeData for ParameterData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}
