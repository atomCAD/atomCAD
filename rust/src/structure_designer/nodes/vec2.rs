use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec2;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::dvec2_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vec2Data {
  #[serde(with = "dvec2_serializer")]
  pub value: DVec2,
}

impl NodeData for Vec2Data {
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
        NetworkResult::extract_float
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let y = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        self.value.y, 
        NetworkResult::extract_float
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      return NetworkResult::Vec2(DVec2{x, y});
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
            let x_display = if x_connected { CONNECTED_PIN_SYMBOL.to_string() } else { format!("{:.2}", self.value.x) };
            let y_display = if y_connected { CONNECTED_PIN_SYMBOL.to_string() } else { format!("{:.2}", self.value.y) };
            Some(format!("({},{})", x_display, y_display))
        }
    }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "vec2".to_string(),
      description: "Outputs an Vec2 value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
            name: "x".to_string(),
            data_type: DataType::Float,
        },
        Parameter {
            name: "y".to_string(),
            data_type: DataType::Float,
        },        
      ],
      output_type: DataType::Vec2,
      public: true,
      node_data_creator: || Box::new(Vec2Data {
        value: DVec2::new(0.0, 0.0)
      }),
      node_data_saver: generic_node_data_saver::<Vec2Data>,
      node_data_loader: generic_node_data_loader::<Vec2Data>,
  }
}
