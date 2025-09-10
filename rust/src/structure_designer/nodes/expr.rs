use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::error_in_input;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExprParameter {
  pub name: String,
  pub data_type: APIDataType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExprData {
  pub parameters: Vec<ExprParameter>,
}

impl NodeData for ExprData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_expr<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> Vec<NetworkResult> {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let expr_data = &node.data.as_any_ref().downcast_ref::<ExprData>().unwrap();
  return Vec::new();  
}
