use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::{build_inline_closure, run_closure_once};
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

        // External: `xs` (Iter[T]) and `init` (A). The combining body lives
        // inside the zone.
        custom.parameters[0].data_type = DataType::Iterator(Box::new(self.element_type.clone()));
        custom.parameters[1].data_type = self.accumulator_type.clone();
        custom.output_pins = OutputPinDefinition::single_fixed(self.accumulator_type.clone());

        // Inside-facing pins: two zone-input sources (acc, element) and one
        // zone-output destination (new_acc).
        custom.zone_input_pins = vec![
            OutputPinDefinition::fixed("acc", self.accumulator_type.clone()),
            OutputPinDefinition::fixed("element", self.element_type.clone()),
        ];
        custom.zone_output_pins = vec![Parameter {
            id: None,
            name: "new_acc".to_string(),
            data_type: self.accumulator_type.clone(),
        }];

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
        // a. Resolve external inputs first — runs against the HOF's
        // containing network scope, before the body is pushed.
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
                    "fold: xs is not an iterator (got {})",
                    other.to_display_string()
                )));
            }
        };

        let init_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 1);
        if let NetworkResult::Error(_) = init_val {
            return EvalOutput::single(init_val);
        }

        // b. Build the closure (body + frozen captures + zone-output wire(s) +
        // type metadata). Captures are pre-evaluated against the outer context
        // so nested captures relying on outer captures still resolve.
        let closure = match build_inline_closure(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            "fold",
        ) {
            Ok(c) => c,
            Err(e) => return EvalOutput::single(e),
        };

        // c. Build an inner context for the body's iterations (the eager-body
        // inherit-vs-fresh policy). Inherits `execute`,
        // `use_vdw_cutoff`, and `current_zone_input_values` (so ancestor
        // HOFs' iteration frames remain visible to nested captures); gets
        // fresh per-pass scratch state; prints drain back at end of call.
        let mut inner_ctx = context.fresh_inner_for_eager_body();

        // d. Drain the source walker eagerly. Each step runs the closure once
        // on `(acc, element)` via `run_closure_once` — the same carried-wires
        // per-step body shared by the lazy walkers and `apply`. The closure's
        // frozen captures are swapped in for the duration of each step.
        let mut acc = init_val;
        let result = loop {
            match walker.next(network_evaluator, registry, &mut inner_ctx) {
                None => break Ok(acc),
                Some(NetworkResult::Error(e)) => break Err(NetworkResult::Error(e)),
                Some(elem) => {
                    let new_acc = run_closure_once(
                        network_evaluator,
                        network_stack,
                        registry,
                        &mut inner_ctx,
                        &closure,
                        vec![acc.clone(), elem],
                    );
                    if let NetworkResult::Error(_) = new_acc {
                        break Err(new_acc);
                    }
                    acc = new_acc;
                }
            }
        };

        context.drain_inner_context(inner_ctx);

        EvalOutput::single(match result {
            Ok(v) => v,
            Err(e) => e,
        })
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
        m
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // FromOutput: source `Iter[T]` / `Array[T]` / `T` → both T.
        // FromInput:  source `T` → both T (fold's output is the accumulator
        //             type, its `init` input is also T).
        let elem = source_type.drag_element_type_from_output()?;
        Some(Box::new(FoldData {
            element_type: elem.clone(),
            accumulator_type: elem,
        }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "fold".to_string(),
        description: "Reduces `xs` to a single value by repeatedly evaluating the inline zone body. The body reads the current accumulator and element from the inside-facing `acc` and `element` source pins and delivers the next accumulator value to the `new_acc` destination pin. Iteration is eager and left-to-right, starting from `init`.".to_string(),
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
                name: "init".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Float),
        zone_input_pins: vec![
            OutputPinDefinition::fixed("acc", DataType::Float),
            OutputPinDefinition::fixed("element", DataType::Float),
        ],
        zone_output_pins: vec![Parameter {
            id: None,
            name: "new_acc".to_string(),
            data_type: DataType::Float,
        }],
        public: true,
        node_data_creator: || Box::new(FoldData::default()),
        node_data_saver: generic_node_data_saver::<FoldData>,
        node_data_loader: generic_node_data_loader::<FoldData>,
    }
}
