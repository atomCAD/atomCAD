use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeData {
  pub start: i32,
  pub step: i32,
  pub count: i32,
}

impl NodeData for RangeData {
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
      context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
      let node = NetworkStackElement::get_top_node(network_stack, node_id);
      let range_data = &node.data.as_any_ref().downcast_ref::<RangeData>().unwrap();
    
      let start = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        range_data.start, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let step = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        range_data.step, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let count = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        range_data.count, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
    
      // Create a vector of integers from the range
      let mut result_vec = Vec::new();
      
      for i in 0..count {
        let value = start + (i * step);
        result_vec.push(NetworkResult::Int(value));
      }
      
      return NetworkResult::Array(result_vec);
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}

