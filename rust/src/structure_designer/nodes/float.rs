use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::text_format::TextValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatData {
  pub value: f64,
}

impl NodeData for FloatData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }

    fn eval<'a>(
      &self,
      _network_evaluator: &NetworkEvaluator,
      _network_stack: &Vec<NetworkStackElement<'a>>,
      _node_id: u64,
      _registry: &NodeTypeRegistry,
      _decorate: bool,
      _context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
      NetworkResult::Float(self.value)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("{:.2}", self.value))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("value".to_string(), TextValue::Float(self.value)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("value") {
            self.value = v.as_float().ok_or_else(|| "value must be a float".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "float".to_string(),
      description: "Outputs a float value.".to_string(),
      summary: None,
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::Float,
      public: true,
      node_data_creator: || Box::new(FloatData {
        value: 0.0
      }),
      node_data_saver: generic_node_data_saver::<FloatData>,
      node_data_loader: generic_node_data_loader::<FloatData>,
    }
}












