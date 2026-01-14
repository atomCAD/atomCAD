use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use crate::util::as_any::AsAny;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::text_format::TextValue;


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

    /// Returns the properties to serialize for text format output.
    ///
    /// Keys are property names as they appear in text format.
    /// These should match parameter names where applicable.
    /// Only returns properties that have stored values (not input-only params).
    ///
    /// Default implementation returns an empty list.
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![]
    }

    /// Updates node data from parsed text properties.
    ///
    /// Only properties present in the map are updated.
    /// Returns error if a property value has wrong type or is invalid.
    ///
    /// Default implementation does nothing and returns Ok.
    fn set_text_properties(&mut self, _props: &HashMap<String, TextValue>) -> Result<(), String> {
        Ok(())
    }
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
















