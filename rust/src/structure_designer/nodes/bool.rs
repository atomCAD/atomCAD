use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoolData {
  pub value: bool,
}

impl NodeData for BoolData {
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
      return NetworkResult::Bool(self.value);
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(self.value.to_string())
    }
}



pub fn get_node_type() -> NodeType {
  NodeType {
      name: "bool".to_string(),
      description: "Outputs a bool value.".to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::Bool,
      public: true,
      node_data_creator: || Box::new(BoolData {
        value: false
      }),
      node_data_saver: generic_node_data_saver::<BoolData>,
      node_data_loader: generic_node_data_loader::<BoolData>,
  }
}











