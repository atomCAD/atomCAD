use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::util::serialization_utils::ivec2_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;
use crate::structure_designer::text_format::TextValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IVec2Data {
  #[serde(with = "ivec2_serializer")]
  pub value: IVec2,
}

impl NodeData for IVec2Data {
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
      let x = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        self.value.x, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let y = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        self.value.y, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      return NetworkResult::IVec2(IVec2{x, y});
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let x_connected = connected_input_pins.contains("x");
        let y_connected = connected_input_pins.contains("y");

        if x_connected && y_connected {
            None
        } else {
            let x_display = if x_connected { CONNECTED_PIN_SYMBOL } else { &self.value.x.to_string() };
            let y_display = if y_connected { CONNECTED_PIN_SYMBOL } else { &self.value.y.to_string() };
            Some(format!("({},{})", x_display, y_display))
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("x".to_string(), TextValue::Int(self.value.x)),
            ("y".to_string(), TextValue::Int(self.value.y)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("x") {
            self.value.x = v.as_int().ok_or_else(|| "x must be an integer".to_string())?;
        }
        if let Some(v) = props.get("y") {
            self.value.y = v.as_int().ok_or_else(|| "y must be an integer".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "ivec2".to_string(),
      description: "Outputs an IVec2 value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: DataType::Int,
        },
        Parameter {
            name: "y".to_string(),
            data_type: DataType::Int,
        },        
      ],
      output_type: DataType::IVec2,
      public: true,
      node_data_creator: || Box::new(IVec2Data {
        value: IVec2::new(0, 0)
      }),
      node_data_saver: generic_node_data_saver::<IVec2Data>,
      node_data_loader: generic_node_data_loader::<IVec2Data>,
  }
}