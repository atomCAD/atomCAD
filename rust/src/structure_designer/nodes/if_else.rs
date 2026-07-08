use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
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

/// The `if` node: selects one of two values based on a boolean condition.
///
/// Pins: `cond: Bool`, `then: T`, `else: T` → output `T`, where `T` is the
/// user-selected `value_type` property (default `Float`), expanded onto the
/// pins by `calculate_custom_node_type` (same idiom as `parameter` /
/// `array_at`).
///
/// Two properties distinguish it from an `expr` conditional:
///   - It can select **structural** values (Crystal / Molecule / Blueprint /
///     Geometry / Function …), not just the scalar/vector types the `expr`
///     language handles.
///   - It is **lazy**: `eval` pulls `cond` first and then evaluates *only* the
///     taken branch, so the untaken branch's upstream cone is never computed
///     (and an error in it never poisons the output). `expr` eagerly evaluates
///     every wired input before running the expression.
///
/// All three pins are optional, consistent with the node network's "every pin
/// type is implicitly optional" model:
///   - unwired `cond` → the node is inert and outputs `None`;
///   - the taken branch unwired → outputs `None` (a downstream required
///     consumer surfaces "input missing"; an optional one falls back to its
///     default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfData {
    /// Type of the `then` / `else` value pins and of the output pin.
    pub value_type: DataType,
}

impl Default for IfData {
    fn default() -> Self {
        Self {
            value_type: DataType::Float,
        }
    }
}

impl NodeData for IfData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        // cond (pin 0) stays Bool; then (pin 1) / else (pin 2) take value_type.
        custom.parameters[1].data_type = self.value_type.clone();
        custom.parameters[2].data_type = self.value_type.clone();
        custom.output_pins = OutputPinDefinition::single_fixed(self.value_type.clone());
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
        // Evaluate the condition (pin 0). Unwired → inert (None); error → propagate.
        let cond_val = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &cond_val {
            NetworkResult::None => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Error(_) => return EvalOutput::single(cond_val),
            _ => {}
        }
        let cond = match cond_val {
            NetworkResult::Bool(b) => b,
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "if.cond: expected Bool, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        // Lazily pull *only* the taken branch: pin 1 = then, pin 2 = else.
        // The other branch's upstream cone is never evaluated. An unwired taken
        // branch yields `None` (evaluate_arg's disconnected result), which flows
        // through unchanged.
        let branch_index = if cond { 1 } else { 2 };
        let branch_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, branch_index);
        EvalOutput::single(branch_val)
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
            "value_type".to_string(),
            TextValue::DataType(self.value_type.clone()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("value_type") {
            self.value_type = v
                .as_data_type()
                .ok_or_else(|| "value_type must be a DataType".to_string())?
                .clone();
        }
        Ok(())
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // The value pins (`then`/`else`) and the output are all plain `T`, so
        // in both drag directions the useful adaptation is `value_type = T`
        // (no peeling — the pins aren't `Array[T]`/`Iter[T]`):
        //   - FromOutput: source plugs into `then`/`else` (a `T` value pin).
        //     A `Bool` source also matches the static `cond: Bool` pin, so the
        //     popup will still offer `if` for a boolean drag.
        //   - FromInput: source is the consumer pin's declared type, which
        //     equals `if`'s output `T`.
        // Reject types that can't be a clean concrete value pin: abstract phase
        // supertypes (no abstract → concrete downcasts) and `Iter[T]` (an
        // iterator value can't be meaningfully branched/stored here). The
        // popup re-verifies via the static-pin check, so this only prunes
        // clearly-wrong candidates.
        if source_type.is_abstract() || matches!(source_type, DataType::Iterator(_)) {
            return None;
        }
        Some(Box::new(IfData {
            value_type: source_type.clone(),
        }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "if".to_string(),
        description:
            "Selects one of two values based on a boolean condition. The `cond` input picks the \
             `then` value when true and the `else` value when false. Only the taken branch is \
             evaluated (the other branch's inputs are never computed). All pins are optional: an \
             unwired `cond` makes the node inert, and an unwired taken branch produces no value. \
             The value type is selectable and can be any concrete type, including structural \
             types like Crystal, Molecule, or Geometry."
                .to_string(),
        summary: Some("Select a value by a boolean condition".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "cond".to_string(),
                data_type: DataType::Bool,
            },
            Parameter {
                id: None,
                name: "then".to_string(),
                data_type: DataType::Float,
            },
            Parameter {
                id: None,
                name: "else".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Float),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(IfData::default()),
        node_data_saver: generic_node_data_saver::<IfData>,
        node_data_loader: generic_node_data_loader::<IfData>,
    }
}
