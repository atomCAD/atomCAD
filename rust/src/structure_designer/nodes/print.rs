use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::PrintLogEntry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Debug node — passthrough on `text` with a side effect of appending an entry
/// to the per-CAD-instance print log buffer (surfaced in the Flutter Console
/// panel). Output type is `String`, so the central skip rule does **not** apply
/// here: `eval` runs on every pass that reaches this node, including normal
/// display passes. The `execute_only` flag inside `eval` is what gates the
/// actual buffer push when the user wants prints only on Execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintData {
    /// When `true`, the side-effect push fires only under
    /// `context.execute == true` (i.e. an explicit Execute pass). When
    /// `false` (default), it fires on every evaluation — useful for "what
    /// is flowing through this wire" debugging.
    pub execute_only: bool,
}

impl Default for PrintData {
    fn default() -> Self {
        Self {
            execute_only: false,
        }
    }
}

impl NodeData for PrintData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let text = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            String::new(),
            NetworkResult::extract_string,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let should_print = !self.execute_only || context.execute;
        if should_print {
            let top = network_stack.last().unwrap();
            let network_name = top.node_network.node_type.name.clone();
            let node = NetworkStackElement::get_top_node(network_stack, node_id);
            let node_label = node
                .custom_name
                .clone()
                .unwrap_or_else(|| node.node_type_name.clone());
            context.print_buffer.push(PrintLogEntry {
                timestamp: SystemTime::now(),
                network_name,
                node_id,
                node_label,
                text: text.clone(),
                from_execute: context.execute,
            });
        }

        EvalOutput::single(NetworkResult::String(text))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if self.execute_only {
            Some("execute only".to_string())
        } else {
            None
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![(
            "execute_only".to_string(),
            TextValue::Bool(self.execute_only),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("execute_only") {
            self.execute_only = v
                .as_bool()
                .ok_or_else(|| "execute_only must be a bool".to_string())?;
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "print".to_string(),
        description: "Passes its `text` input through unchanged. As a side effect, appends the \
                      text to the in-app Console panel. Set `execute_only` to true to gate the \
                      side effect to explicit Execute passes only."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![Parameter {
            id: None,
            name: "text".to_string(),
            data_type: DataType::String,
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::String),
        public: true,
        node_data_creator: || Box::new(PrintData::default()),
        node_data_saver: generic_node_data_saver::<PrintData>,
        node_data_loader: generic_node_data_loader::<PrintData>,
    }
}
