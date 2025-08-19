use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::network_evaluator::input_missing_error;
use crate::structure_designer::evaluator::network_evaluator::error_in_input;
use crate::common::simulation::minimize_energy;

#[derive(Debug, Clone)]
pub struct RelaxEvalCache {
  pub relax_message: String,
}

pub fn eval_relax<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let molecule_input_name = registry.get_parameter_name(&node.node_type_name, 0);

  if node.arguments[0].is_empty() {
    return input_missing_error(&molecule_input_name);
  }

  let input_node_id = node.arguments[0].get_node_id().unwrap();
  let input_val = network_evaluator.evaluate(network_stack, input_node_id, registry, false, context)[0].clone();

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
