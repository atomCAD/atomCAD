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
pub struct ArrayConcatData {
    /// Element type shared by both inputs and the output array.
    pub element_type: DataType,
}

impl Default for ArrayConcatData {
    fn default() -> Self {
        Self {
            element_type: DataType::Int,
        }
    }
}

impl NodeData for ArrayConcatData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        let array_ty = DataType::Array(Box::new(self.element_type.clone()));
        custom.parameters[0].data_type = array_ty.clone();
        custom.parameters[1].data_type = array_ty.clone();
        custom.output_pins = OutputPinDefinition::single_fixed(array_ty);
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
        let a_val = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &a_val {
            NetworkResult::None => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Error(_) => return EvalOutput::single(a_val),
            _ => {}
        }

        let b_val = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        match &b_val {
            NetworkResult::None => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Error(_) => return EvalOutput::single(b_val),
            _ => {}
        }

        let mut combined = match a_val {
            NetworkResult::Array(items) => items,
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "array_concat: input a is not an array".to_string(),
                ));
            }
        };

        match b_val {
            NetworkResult::Array(items) => combined.extend(items),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "array_concat: input b is not an array".to_string(),
                ));
            }
        }

        EvalOutput::single(NetworkResult::Array(combined))
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
        vec![(
            "element_type".to_string(),
            TextValue::DataType(self.element_type.clone()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element_type") {
            self.element_type = v
                .as_data_type()
                .ok_or_else(|| "element_type must be a DataType".to_string())?
                .clone();
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "array_concat".to_string(),
        description:
            "Concatenates two arrays of the same element type into a single array. Both inputs share the configured element type."
                .to_string(),
        summary: Some("Concatenate two arrays".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "a".to_string(),
                data_type: DataType::Array(Box::new(DataType::Int)),
            },
            Parameter {
                id: None,
                name: "b".to_string(),
                data_type: DataType::Array(Box::new(DataType::Int)),
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Array(Box::new(DataType::Int))),
        public: true,
        node_data_creator: || Box::new(ArrayConcatData::default()),
        node_data_saver: generic_node_data_saver::<ArrayConcatData>,
        node_data_loader: generic_node_data_loader::<ArrayConcatData>,
    }
}
