use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::crystolecule::serialization_utils::dvec3_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::common_constants::CONNECTED_PIN_SYMBOL;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vec3Data {
  #[serde(with = "dvec3_serializer")]
  pub value: DVec3,
}

impl NodeData for Vec3Data {
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
    
      let z = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        self.value.z, 
        NetworkResult::extract_float
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      return NetworkResult::Vec3(DVec3{x, y, z});  
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let x_connected = connected_input_pins.contains("x");
        let y_connected = connected_input_pins.contains("y");
        let z_connected = connected_input_pins.contains("z");
        
        if x_connected && y_connected && z_connected {
            None
        } else {
            let x_display = if x_connected { CONNECTED_PIN_SYMBOL.to_string() } else { format!("{:.2}", self.value.x) };
            let y_display = if y_connected { CONNECTED_PIN_SYMBOL.to_string() } else { format!("{:.2}", self.value.y) };
            let z_display = if z_connected { CONNECTED_PIN_SYMBOL.to_string() } else { format!("{:.2}", self.value.z) };
            Some(format!("({},{},{})", x_display, y_display, z_display))
        }
    }
    
}





