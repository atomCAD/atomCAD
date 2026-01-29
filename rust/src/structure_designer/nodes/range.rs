use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;

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

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let start_connected = connected_input_pins.contains("start");
        let step_connected = connected_input_pins.contains("step");
        let count_connected = connected_input_pins.contains("count");

        if start_connected && step_connected && count_connected {
            None
        } else {
            let start_display = if start_connected { CONNECTED_PIN_SYMBOL } else { &self.start.to_string() };
            let step_display = if step_connected { CONNECTED_PIN_SYMBOL } else { &self.step.to_string() };
            let count_display = if count_connected { CONNECTED_PIN_SYMBOL } else { &self.count.to_string() };
            Some(format!("[{}:{}:{}]", start_display, step_display, count_display))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("start".to_string(), TextValue::Int(self.start)),
            ("step".to_string(), TextValue::Int(self.step)),
            ("count".to_string(), TextValue::Int(self.count)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("start") {
            self.start = v.as_int().ok_or_else(|| "start must be an integer".to_string())?;
        }
        if let Some(v) = props.get("step") {
            self.step = v.as_int().ok_or_else(|| "step must be an integer".to_string())?;
        }
        if let Some(v) = props.get("count") {
            self.count = v.as_int().ok_or_else(|| "count must be an integer".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "range".to_string(),
      description: "Creates an array of integers starting from an integer value and having a specified step between them. The number of integers in the array can also be specified (count).".to_string(),
      summary: None,
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "start".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "step".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "count".to_string(),
            data_type: DataType::Int,
        },        
      ],
      output_type: DataType::Array(Box::new(DataType::Int)),
      public: true,
      node_data_creator: || Box::new(RangeData {
        start: 0,
        step: 1,
        count: 1,
      }),
      node_data_saver: generic_node_data_saver::<RangeData>,
      node_data_loader: generic_node_data_loader::<RangeData>,
    }
}
