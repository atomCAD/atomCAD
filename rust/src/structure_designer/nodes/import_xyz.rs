use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::common::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;


#[derive(Serialize, Deserialize)]
pub struct ImportXYZData {
  pub file_name: Option<String>, // If none, nothing has been imported yet.

  #[serde(skip)]
  pub atomic_structure: Option<AtomicStructure>,
}

impl NodeData for ImportXYZData {
  fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }
}

impl ImportXYZData {
  pub fn new() -> Self {
      Self {
          file_name: None,
          atomic_structure: None,
      }
  }
}

pub fn eval_import_xyz<'a>(
  _network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  _context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let node_data = &node.data.as_any_ref().downcast_ref::<ImportXYZData>().unwrap();

  return match &node_data.atomic_structure {
      Some(atomic_structure) => NetworkResult::Atomic(atomic_structure.clone()),
      None => NetworkResult::Error("No atomic structure imported".to_string()),
  };
}
