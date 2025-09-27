use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::data_type::FunctionType;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
  pub input_type: DataType,
  pub output_type: DataType,
}

impl NodeData for MapData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
      let mut custom_node_type = base_node_type.clone();

      custom_node_type.parameters[0].data_type = DataType::Array(Box::new(self.input_type.clone()));
      custom_node_type.parameters[1].data_type = DataType::Function(FunctionType {
        parameter_types: vec![self.input_type.clone()],
        output_type: Box::new(self.output_type.clone()),
      });
      custom_node_type.output_type = DataType::Array(Box::new(self.output_type.clone()));

      Some(custom_node_type)
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
      let xs_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        0,
      );
    
      if let NetworkResult::Error(_) = xs_val {
        return xs_val;
      }
    
      // Extract the array elements from xs_val
      let xs = if let NetworkResult::Array(array_elements) = xs_val {
        array_elements
      } else {
        return NetworkResult::Error("Expected array of elements".to_string());
      };
      
      let f_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        1,
      );

      if let NetworkResult::Error(_) = f_val {
        return f_val;
      }

      // Extract the f closure from f_val
      let f = if let NetworkResult::Function(closure) = f_val {
        closure
      } else {
        return NetworkResult::Error("Expected a closure".to_string());
      };

      // Create a function evaluator for the closure
      let mut function_evaluator = FunctionEvaluator::new(f, registry);
      
      // Iterate through all elements in the xs array
      let mut results = Vec::new();
      for element in xs {
        // Set the current element as the first input pin of function_evaluator
        function_evaluator.set_argument_value(0, element);
        
        // Call the evaluate method on function_evaluator
        let result = function_evaluator.evaluate(network_evaluator, registry);
        
        // If there's an error in evaluation, propagate it immediately
        if let NetworkResult::Error(_) = result {
          return result;
        }
        
        // Collect the result
        results.push(result);
      }
      
      // Return the collected results as a NetworkResult::Array
      return NetworkResult::Array(results);
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}
