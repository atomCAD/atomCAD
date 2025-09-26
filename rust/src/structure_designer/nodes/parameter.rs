use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterData {
  pub param_index: usize,
  pub param_name: String,
  pub data_type: DataType,
  pub sort_order: i32,
  pub data_type_str: Option<String>,
  #[serde(skip)]
  pub error: Option<String>,
}

impl NodeData for ParameterData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
      let mut custom_node_type = base_node_type.clone();

      custom_node_type.parameters[0].data_type = self.data_type.clone();
      custom_node_type.output_type = self.data_type.clone();

      Some(custom_node_type)
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      _decorate: bool,
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      let node = NetworkStackElement::get_top_node(network_stack, node_id);
      let evaled_in_isolation = network_stack.len() < 2;
    
      if evaled_in_isolation {
        return eval_default(network_evaluator, network_stack, node_id, registry, context);
      }
    
      let parent_node_id = network_stack.last().unwrap().node_id;
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();
    
      // evaluate all the arguments of the parent node if any
      if parent_node.arguments[self.param_index].is_empty() {
        return eval_default(network_evaluator, network_stack, node_id, registry, context);
      }
    
      return network_evaluator.evaluate_arg_required(
        network_stack,
        parent_node_id,
        registry,
        context,
        self.param_index);
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
    
}

fn eval_default<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {  
  return network_evaluator.evaluate_arg_required(
    network_stack,
    node_id,
    registry,
    context,
    0);
}
