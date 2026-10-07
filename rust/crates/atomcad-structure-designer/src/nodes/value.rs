use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network::NodeNetwork;
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ValueData {
    #[serde(skip)]
    pub value: NetworkResult,
    /// The type the output pin declares. `None` = inferred from `value`.
    /// Set it to inject a value the validator would otherwise reject — an
    /// empty or malformed array of records, or a deliberately wrong-typed
    /// value — so a test can reach the consuming node's runtime checks.
    #[serde(skip)]
    pub declared_type: Option<DataType>,
}

impl std::fmt::Debug for ValueData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueData")
            .field("value", &"<NetworkResult>")
            .finish()
    }
}

impl NodeData for ValueData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// The output pin carries the declared type, else the held value's
    /// inferred type, so a `value` node wired into a typed pin passes
    /// validation. A value whose type can't be inferred (`None`, `Error`, an
    /// empty array, …) keeps the base `None` output.
    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let data_type = self
            .declared_type
            .clone()
            .or_else(|| infer_value_type(&self.value))?;
        let mut custom = base_node_type.clone();
        custom.output_pins = OutputPinDefinition::single(data_type);
        Some(custom)
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        EvalOutput::single(self.value.clone())
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "value".to_string(),
        description: "".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::None),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: false,
        node_data_creator: || {
            Box::new(ValueData {
                value: NetworkResult::None,
                declared_type: None,
            })
        },
        node_data_saver: generic_node_data_saver::<ValueData>,
        node_data_loader: generic_node_data_loader::<ValueData>,
    }
}

/// `infer_data_type`, plus arrays: a non-empty array whose elements share one
/// inferred type is `Array[T]`.
fn infer_value_type(value: &NetworkResult) -> Option<DataType> {
    if let NetworkResult::Array(items) = value {
        let mut element: Option<DataType> = None;
        for item in items {
            let t = infer_value_type(item)?;
            match &element {
                None => element = Some(t),
                Some(e) if *e == t => {}
                _ => return None,
            }
        }
        return element.map(|t| DataType::Array(Box::new(t)));
    }
    value.infer_data_type()
}

/// Adds a `value` node holding `value` to `network`, with its custom node type
/// (the held value's inferred type on the output pin) already cached. A bare
/// `network.add_node("value", …)` skips the cache — validation then sees a
/// `None` output and flags every typed pin it feeds — so tests that poke a
/// network directly should add `value` nodes through this.
pub fn add_value_node(network: &mut NodeNetwork, position: DVec2, value: NetworkResult) -> u64 {
    insert(
        network,
        position,
        ValueData {
            value,
            declared_type: None,
        },
    )
}

/// [`add_value_node`] with an explicitly declared output type (see
/// [`ValueData::declared_type`]).
pub fn add_typed_value_node(
    network: &mut NodeNetwork,
    position: DVec2,
    value: NetworkResult,
    declared_type: DataType,
) -> u64 {
    insert(
        network,
        position,
        ValueData {
            value,
            declared_type: Some(declared_type),
        },
    )
}

fn insert(network: &mut NodeNetwork, position: DVec2, data: ValueData) -> u64 {
    let custom_node_type = data.calculate_custom_node_type(&get_node_type());
    let node_id = network.add_node("value", position, 0, Box::new(data));
    if let Some(node) = network.nodes.get_mut(&node_id) {
        node.set_custom_node_type(custom_node_type, false);
    }
    node_id
}
