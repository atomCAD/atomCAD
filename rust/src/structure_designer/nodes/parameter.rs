use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::DataType;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;

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

pub fn eval_parameter<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> Vec<NetworkResult> {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  let evaled_in_isolation = network_stack.len() < 2;

  if evaled_in_isolation {
    return vec![NetworkResult::Error("input is missing".to_string())];
  }

  let parent_node_id = network_stack.last().unwrap().node_id;
  let param_data = &(*node.data).as_any_ref().downcast_ref::<ParameterData>().unwrap();
  let mut parent_network_stack = network_stack.clone();
  parent_network_stack.pop();
  let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();

  // evaluate all the arguments of the parent node if any
  let argument_node_ids = &parent_node.arguments[param_data.param_index].argument_node_ids;
  if argument_node_ids.is_empty() {
    return vec![NetworkResult::Error("input is missing".to_string())];
  }
  let args : Vec<Vec<NetworkResult>> = argument_node_ids.iter().map(|&arg_node_id| {
    network_evaluator.evaluate(&parent_network_stack, arg_node_id, registry, false, context)
  }).collect();
  args.concat()
}
