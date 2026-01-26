use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_data::CustomNodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

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
      let evaled_in_isolation = network_stack.len() < 2;

      if evaled_in_isolation {
        // Check if CLI parameter is provided (has precedence over default pin)
        if let Some(cli_value) = context.top_level_parameters.get(&self.param_name) {
          return cli_value.clone();
        }
        // Fall back to default pin
        return eval_default(network_evaluator, network_stack, node_id, registry, context);
      }

      let parent_node_id = network_stack.last().unwrap().node_id;
      let mut parent_network_stack = network_stack.clone();
      parent_network_stack.pop();
      let parent_node = parent_network_stack.last().unwrap().node_network.nodes.get(&parent_node_id).unwrap();

      // If wire is connected, evaluate it (highest priority)
      if !parent_node.arguments[self.param_index].is_empty() {
        return network_evaluator.evaluate_arg_required(
          &parent_network_stack,
          parent_node_id,
          registry,
          context,
          self.param_index);
      }

      // No wire connected - check for stored literal value in CustomNodeData
      if let Some(custom_data) = parent_node.data.as_any_ref().downcast_ref::<CustomNodeData>() {
        if let Some(text_value) = custom_data.literal_values.get(&self.param_name) {
          if let Some(result) = text_value.to_network_result(&self.data_type) {
            return result;
          }
        }
      }

      // Fall back to default pin (lowest priority)
      eval_default(network_evaluator, network_stack, node_id, registry, context)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(self.param_name.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![
            ("param_index".to_string(), TextValue::Int(self.param_index as i32)),
            ("param_name".to_string(), TextValue::String(self.param_name.clone())),
            ("data_type".to_string(), TextValue::DataType(self.data_type.clone())),
            ("sort_order".to_string(), TextValue::Int(self.sort_order)),
        ];
        if let Some(ref dt_str) = self.data_type_str {
            props.push(("data_type_str".to_string(), TextValue::String(dt_str.clone())));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("param_index") {
            self.param_index = v.as_int().ok_or_else(|| "param_index must be an integer".to_string())? as usize;
        }
        if let Some(v) = props.get("param_name") {
            self.param_name = v.as_string().ok_or_else(|| "param_name must be a string".to_string())?.to_string();
        }
        if let Some(v) = props.get("data_type") {
            self.data_type = v.as_data_type().ok_or_else(|| "data_type must be a DataType".to_string())?.clone();
        }
        if let Some(v) = props.get("sort_order") {
            self.sort_order = v.as_int().ok_or_else(|| "sort_order must be an integer".to_string())?;
        }
        if let Some(v) = props.get("data_type_str") {
            self.data_type_str = Some(v.as_string().ok_or_else(|| "data_type_str must be a string".to_string())?.to_string());
        }
        Ok(())
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

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "parameter".to_string(),
      description: "To set up an input pin (parameter) of your custom node you need to use a parameter node in your subnetwork.
The sort order property of a parameter determines the order of the parameters in the resulting custom node.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
          Parameter {
              name: "default".to_string(),
              data_type: DataType::Int, // will change based on  ParameterData::data_type.
          },
      ],
      output_type: DataType::Int, // will change based on ParameterData::data_type.
      public: true,
      node_data_creator: || Box::new(ParameterData {
        param_index: 0,
        param_name: "param".to_string(),
        data_type: DataType::Int,
        sort_order: 0,
        data_type_str: None,
        error: None,
      }),
      node_data_saver: generic_node_data_saver::<ParameterData>,
      node_data_loader: generic_node_data_loader::<ParameterData>,
    }
}
