use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::data_type::FunctionType;
use crate::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
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
pub struct FoldData {
    pub element_type: DataType,
    pub accumulator_type: DataType,
}

impl Default for FoldData {
    fn default() -> Self {
        Self {
            element_type: DataType::Float,
            accumulator_type: DataType::Float,
        }
    }
}

impl NodeData for FoldData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        custom.parameters[0].data_type = DataType::Array(Box::new(self.element_type.clone()));
        custom.parameters[1].data_type = self.accumulator_type.clone();
        custom.parameters[2].data_type = DataType::Function(FunctionType {
            parameter_types: vec![self.accumulator_type.clone(), self.element_type.clone()],
            output_type: Box::new(self.accumulator_type.clone()),
        });
        custom.output_pins = OutputPinDefinition::single_fixed(self.accumulator_type.clone());
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
        if let NetworkResult::Error(_) = xs_val {
            return EvalOutput::single(xs_val);
        }
        let xs = if let NetworkResult::Array(items) = xs_val {
            items
        } else {
            return EvalOutput::single(NetworkResult::Error(
                "Expected array of elements".to_string(),
            ));
        };

        let init_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = init_val {
            return EvalOutput::single(init_val);
        }

        let f_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 2);
        if let NetworkResult::Error(_) = f_val {
            return EvalOutput::single(f_val);
        }
        let closure = if let NetworkResult::Function(c) = f_val {
            c
        } else {
            return EvalOutput::single(NetworkResult::Error("Expected a closure".to_string()));
        };

        let mut function_evaluator = FunctionEvaluator::new(closure, registry);
        let mut acc = init_val;
        for elem in xs {
            function_evaluator.set_argument_value(0, acc);
            function_evaluator.set_argument_value(1, elem);
            let next = function_evaluator.evaluate(network_evaluator, registry);
            if let NetworkResult::Error(_) = next {
                return EvalOutput::single(next);
            }
            acc = next;
        }

        EvalOutput::single(acc)
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
            (
                "accumulator_type".to_string(),
                TextValue::DataType(self.accumulator_type.clone()),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element_type") {
            self.element_type = v
                .as_data_type()
                .ok_or_else(|| "element_type must be a DataType".to_string())?
                .clone();
        }
        if let Some(v) = props.get("accumulator_type") {
            self.accumulator_type = v
                .as_data_type()
                .ok_or_else(|| "accumulator_type must be a DataType".to_string())?
                .clone();
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("xs".to_string(), (true, None));
        m.insert("init".to_string(), (true, None));
        m.insert("f".to_string(), (true, None));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "fold".to_string(),
        description: "Reduces `xs` to a single value by repeatedly applying `f(acc, elem)`, \
                      starting from `init`, left-to-right."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "xs".to_string(),
                data_type: DataType::Array(Box::new(DataType::Float)),
            },
            Parameter {
                id: None,
                name: "init".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "f".to_string(),
                data_type: DataType::Function(FunctionType {
                    parameter_types: vec![DataType::Float, DataType::Float],
                    output_type: Box::new(DataType::Float),
                }),
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Float),
        public: true,
        node_data_creator: || Box::new(FoldData::default()),
        node_data_saver: generic_node_data_saver::<FoldData>,
        node_data_loader: generic_node_data_loader::<FoldData>,
    }
}
