use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::common::simulation::minimize_energy;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaxData {
}

#[derive(Debug, Clone)]
pub struct RelaxEvalCache {
  pub relax_message: String,
}

impl NodeData for RelaxData {
  fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }

  fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
    None
  }

  fn eval<'a>(
    &self,
    network_evaluator: &NetworkEvaluator,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    _decorate: bool,
    context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let molecule_input_name = registry.get_parameter_name(&node, 0);
  
    if node.arguments[0].is_empty() {
      return input_missing_error(&molecule_input_name);
    }
  
    let input_node_id = node.arguments[0].get_node_id().unwrap();
    let input_val = network_evaluator.evaluate(network_stack, input_node_id, 0, registry, false, context);
  
    if let NetworkResult::Error(_error) = input_val {
      return error_in_input(&molecule_input_name);
    }
  
    if let NetworkResult::Atomic(mut atomic_structure) = input_val {
  
      match minimize_energy(&mut atomic_structure) {
        Ok(result) => {
          // Store evaluation cache for selected node
          if NetworkStackElement::is_node_selected_in_root_network(network_stack, node_id) {
            let eval_cache = RelaxEvalCache {
              relax_message: result.message.clone(),
            };
            context.selected_node_eval_cache = Some(Box::new(eval_cache));
          }
          
          return NetworkResult::Atomic(atomic_structure);
        }
        Err(error_msg) => {
          return NetworkResult::Error(error_msg);
        }
      }
    }
    return NetworkResult::Atomic(AtomicStructure::new());
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }
}

