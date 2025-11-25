use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use std::any::Any;
use std::collections::HashSet;
use crate::util::as_any::AsAny;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;


pub trait NodeData: Any + AsAny  {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>>;

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType>;

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut NetworkEvaluationContext
    ) -> NetworkResult;

    // Method to clone the trait object
    fn clone_box(&self) -> Box<dyn NodeData>;

    // Method to provide an optional subtitle for the node
    // connected_input_pins contains the names of input pins that are connected
    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoData {
}

impl NodeData for NoData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        NetworkResult::Error(format!("eval not implemented for node {}", node.node_type_name))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(NoData {})
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}
















