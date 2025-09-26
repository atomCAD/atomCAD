use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::{NetworkResult, error_in_input, input_missing_error};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::expr::validation::{get_function_signatures, get_function_implementations};
use std::collections::HashMap;
use crate::structure_designer::expr::parser::parse;
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::expr::expr::Expr;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::node_type::Parameter;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExprParameter {
    pub name: String,
    pub data_type: DataType,
    pub data_type_str: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExprData {
  pub parameters: Vec<ExprParameter>,
  pub expression: String,
  #[serde(skip)]
  pub expr: Option<Expr>,
  #[serde(skip)]
  pub error: Option<String>,
  #[serde(skip)]
  pub output_type: Option<DataType>,
}

impl ExprData {
    /// Parses and validates the expression and returns any validation errors
    pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        // Clear previous state
        self.expr = None;
        self.output_type = None;
        
        // Skip validation if expression is empty
        if self.expression.trim().is_empty() {
            return errors;
        }
        
        // Parse the expression
        let parsed_expr = match parse(&self.expression) {
            Ok(expr) => {
                self.expr = Some(expr.clone());
                expr
            },
            Err(parse_error) => {
                let error_msg = format!("Parse error: {}", parse_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
                return errors;
            }
        };
        
        // Create variables map for validation
        let mut variables = HashMap::new();
        
        // Add parameters as variables
        for param in &self.parameters {
            variables.insert(param.name.clone(), param.data_type.clone());
        }
        
        // Validate the parsed expression using global function registry
        match parsed_expr.validate(&variables, get_function_signatures()) {
            Ok(output_type) => {
                // Expression is valid - set the output type
                self.output_type = Some(output_type);
            }, 
            Err(validation_error) => {
                let error_msg = format!("Validation error: {}", validation_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
            }
        }
        
        errors
    }
}

impl NodeData for ExprData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
      let mut custom_node_type = base_node_type.clone();
            
      // Update the output type - use DataType::None if self.output_type is None
      custom_node_type.output_type = self.output_type.clone().unwrap_or(DataType::None);
      
      // Convert ExprParameter to Parameter
      custom_node_type.parameters = self.parameters.iter()
        .map(|expr_param| Parameter {
          name: expr_param.name.clone(),
          data_type: expr_param.data_type.clone(),
        })
        .collect();
      
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
    
      // Collect variable values for evaluation
      let mut variables: HashMap<String, NetworkResult> = HashMap::new();
      
      // Go through all parameter indices and evaluate them
      for (param_index, param) in self.parameters.iter().enumerate() {
        if let Some(result) = network_evaluator.evaluate_arg(
          network_stack,
          node_id,
          registry,
          context,
          param_index,
        ) {
          // Check if the result is an error
          if let NetworkResult::Error(_) = result {
            return error_in_input(&param.name);
          }
          
          // Add the variable to our collection
          variables.insert(param.name.clone(), result);
        } else {
          // Input pin is not connected
          return input_missing_error(&param.name);
        }
      }
      
      // If we have a parsed expression, evaluate it
      if let Some(ref expr) = self.expr {
        let function_implementations = get_function_implementations();
        expr.evaluate(&variables, function_implementations)
      } else {
        NetworkResult::Error("Expression not parsed".to_string())
      }
    }
    
}
