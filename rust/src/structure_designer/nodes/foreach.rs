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

/// Side-effect counterpart of `map`: iterates `xs`, applies `f` to each element
/// purely for its side effects, and returns `Unit`. Unlike `map` the inner
/// sequence is never produced — the body's return value is discarded.
///
/// Display-pass cost is **zero**: because the output pin is `Unit`, the central
/// skip rule in the evaluator short-circuits this node entirely on non-execute
/// passes (see `doc/design_node_execution.md` Phase 2 — Central skip rule).
/// `eval` only runs under `context.execute == true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeachData {
    pub input_type: DataType,
}

impl Default for ForeachData {
    fn default() -> Self {
        Self {
            input_type: DataType::Float,
        }
    }
}

impl NodeData for ForeachData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        custom.parameters[0].data_type = DataType::Iterator(Box::new(self.input_type.clone()));
        custom.parameters[1].data_type = DataType::Function(FunctionType {
            parameter_types: vec![self.input_type.clone()],
            output_type: Box::new(DataType::Unit),
        });
        custom.output_pins = OutputPinDefinition::single_fixed(DataType::Unit);
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
        // No `if !context.execute { return Unit; }` guard — `foreach`'s output
        // is `Unit`, so the central skip rule short-circuits this node before
        // `eval` is ever called on display passes. When this body runs,
        // `context.execute == true`. See `doc/design_node_execution.md`.

        let xs_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        let mut walker = match xs_val {
            NetworkResult::Iterator(w) => w,
            // Belt-and-braces: the implicit `[T] → Iter[T]` wire conversion
            // normally wraps any incoming array as `Iterator(_)` already.
            NetworkResult::Array(items) => Walker::from_array(items),
            err @ NetworkResult::Error(_) => return EvalOutput::single(err),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "foreach: xs is not an iterator (got {})",
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
                    "foreach: f is not a function".to_string(),
                ));
            }
        };

        let mut function_evaluator = match FunctionEvaluator::try_build(closure, registry) {
            Ok(fe) => fe,
            Err(msg) => {
                return EvalOutput::single(NetworkResult::Error(format!("foreach: {}", msg)));
            }
        };

        loop {
            match walker.next(network_evaluator, registry, context) {
                None => break,
                Some(NetworkResult::Error(e)) => {
                    return EvalOutput::single(NetworkResult::Error(e));
                }
                Some(elem) => {
                    function_evaluator.set_argument_value(0, elem);
                    let result = function_evaluator.evaluate(network_evaluator, registry, context);
                    if let NetworkResult::Error(_) = result {
                        // Fail-fast: surface the first body error as `foreach`'s
                        // output and halt the loop. Continuing past errors
                        // during a batch export silently produces a partial
                        // result set with no visible signal — the worst of
                        // all worlds.
                        return EvalOutput::single(result);
                    }
                    // Successful results are dropped — body return type is
                    // discarded into Unit (universal `T → Unit` widening).
                }
            }
        }

        EvalOutput::single(NetworkResult::Unit)
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
            "input_type".to_string(),
            TextValue::DataType(self.input_type.clone()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("input_type") {
            self.input_type = v
                .as_data_type()
                .ok_or_else(|| "input_type must be a DataType".to_string())?
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
        // Mirrors `map`'s adapter: peel `Iter[T]` / `Array[T]` / `T`. The
        // over-promised case is caught by the popup filter's static-match
        // verification, so adapters can be loose.
        let elem = source_type.drag_element_type_from_output()?;
        Some(Box::new(ForeachData { input_type: elem }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "foreach".to_string(),
        description: "Iterates `xs` and runs the side-effecting `f` on every element, returning \
                      `Unit`. The body's return value is discarded. On normal display passes the \
                      whole pipeline is skipped — `foreach` only iterates when the user invokes \
                      Execute on it (or one of its descendants in the evaluation tree)."
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
                    output_type: Box::new(DataType::Unit),
                }),
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Unit),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ForeachData::default()),
        node_data_saver: generic_node_data_saver::<ForeachData>,
        node_data_loader: generic_node_data_loader::<ForeachData>,
    }
}
