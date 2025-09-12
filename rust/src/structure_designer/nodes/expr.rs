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
use crate::structure_designer::expr::validation::get_function_signatures;
use std::collections::HashMap;
use crate::structure_designer::expr::parser::parse;
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::expr::expr::Expr;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExprParameter {
  pub name: String,
  pub data_type: APIDataType,
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
  pub output_type: Option<APIDataType>,
}

impl ExprData {
    /// Parses and validates the expression and returns any validation errors
    pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        // Clear previous state
        self.expr = None;
        self.error = None;
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
            variables.insert(param.name.clone(), param.data_type);
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

//evaluate(&self, context: &EvaluationContext) -> NetworkResult {


  return Vec::new();  
}
