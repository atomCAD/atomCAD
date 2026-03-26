use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceData {
    /// The element type for all input pins and the output array.
    pub element_type: DataType,
    /// Number of input pins (minimum 1).
    pub input_count: usize,
}

impl Default for SequenceData {
    fn default() -> Self {
        Self {
            element_type: DataType::Atomic,
            input_count: 2,
        }
    }
}

impl NodeData for SequenceData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();

        custom.parameters = (0..self.input_count)
            .map(|i| Parameter {
                id: Some(i as u64),
                name: format!("{}", i),
                data_type: self.element_type.clone(),
            })
            .collect();

        custom.output_pins =
            OutputPinDefinition::single(DataType::Array(Box::new(self.element_type.clone())));

        Some(custom)
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
        let mut items = Vec::new();

        for i in 0..self.input_count {
            let val = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, i);
            match val {
                NetworkResult::None => {} // unconnected pin — skip
                other => items.push(other),
            }
        }

        EvalOutput::single(NetworkResult::Array(items))
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

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "element_type".to_string(),
                TextValue::DataType(self.element_type.clone()),
            ),
            ("count".to_string(), TextValue::Int(self.input_count as i32)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element_type") {
            self.element_type = v
                .as_data_type()
                .ok_or_else(|| "element_type must be a DataType".to_string())?
                .clone();
        }
        if let Some(v) = props.get("count") {
            let n = v
                .as_int()
                .ok_or_else(|| "count must be an integer".to_string())?
                as usize;
            if n < 1 {
                return Err("sequence requires at least 1 input".into());
            }
            self.input_count = n;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        HashMap::new()
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "sequence".to_string(),
        description: "Collects inputs into an ordered array.".to_string(),
        summary: Some("Ordered array from numbered pins".to_string()),
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Array(Box::new(DataType::None))),
        public: true,
        node_data_creator: || Box::new(SequenceData::default()),
        node_data_saver: generic_node_data_saver::<SequenceData>,
        node_data_loader: generic_node_data_loader::<SequenceData>,
    }
}
