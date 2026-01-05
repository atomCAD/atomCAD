use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::simulation::minimize_energy;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;

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
    context: &mut NetworkEvaluationContext) -> NetworkResult {  
  
    let input_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
  
    if let NetworkResult::Error(_) = input_val {
      return input_val;
    }
  
    if let NetworkResult::Atomic(mut atomic_structure) = input_val {
  
      match minimize_energy(&mut atomic_structure) {
        Ok(result) => {
          // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
          // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
          if network_stack.len() == 1 {
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

  fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      None
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "relax".to_string(),
      description: "".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
          },
      ],
      output_type: DataType::Atomic,
      public: false,
      node_data_creator: || Box::new(RelaxData {}),
      node_data_saver: generic_node_data_saver::<RelaxData>,
      node_data_loader: generic_node_data_loader::<RelaxData>,
    }
}
