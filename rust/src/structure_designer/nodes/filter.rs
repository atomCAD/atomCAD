use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::data_type::FunctionType;
use crate::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
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
pub struct FilterData {
    pub element_type: DataType,
}

impl Default for FilterData {
    fn default() -> Self {
        Self {
            element_type: DataType::Float,
        }
    }
}

impl NodeData for FilterData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        let iter_ty = DataType::Iterator(Box::new(self.element_type.clone()));
        custom.parameters[0].data_type = iter_ty.clone();
        custom.parameters[1].data_type = DataType::Function(FunctionType {
            parameter_types: vec![self.element_type.clone()],
            output_type: Box::new(DataType::Bool),
        });
        custom.output_pins = OutputPinDefinition::single_fixed(iter_ty);
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
        let xs_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        let xs_walker = match xs_val {
            NetworkResult::Iterator(w) => w,
            NetworkResult::Array(items) => Walker::from_array(items),
            err @ NetworkResult::Error(_) => return EvalOutput::single(err),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "filter: xs is not an iterator (got {})",
                    other.to_display_string()
                )));
            }
        };

        let f_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);

        let closure = match f_val {
            NetworkResult::Function(c) => c,
            err @ NetworkResult::Error(_) => return EvalOutput::single(err),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "filter: f is not a function".to_string(),
                ));
            }
        };

        let fe = match FunctionEvaluator::try_build(closure, registry) {
            Ok(fe) => fe,
            Err(msg) => {
                return EvalOutput::single(NetworkResult::Error(format!("filter: {}", msg)));
            }
        };

        EvalOutput::single(NetworkResult::Iterator(Walker::filter(xs_walker, fe)))
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

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("xs".to_string(), (true, None));
        m.insert("f".to_string(), (true, None));
        m
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // filter preserves type — both directions extract the same way.
        let elem = source_type.drag_element_type_from_output()?;
        Some(Box::new(FilterData { element_type: elem }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "filter".to_string(),
        description:
            "Lazily yields each element pulled from `xs` for which the predicate `f` returns `true`, preserving order. The intermediate sequence is never materialised."
                .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "xs".to_string(),
                data_type: DataType::Iterator(Box::new(DataType::Float)),
            },
            Parameter {
                id: None,
                name: "f".to_string(),
                data_type: DataType::Function(FunctionType {
                    parameter_types: vec![DataType::Float],
                    output_type: Box::new(DataType::Bool),
                }),
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Iterator(Box::new(
            DataType::Float,
        ))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(FilterData::default()),
        node_data_saver: generic_node_data_saver::<FilterData>,
        node_data_loader: generic_node_data_loader::<FilterData>,
    }
}
