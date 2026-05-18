use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::{
    CaptureKey, NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
use crate::structure_designer::node_network::{IncomingWire, NodeNetwork, SourcePin};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

        // b. Grab the body.
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let body = match node.zone.as_ref() {
            Some(b) => Arc::clone(b),
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "fold: missing zone body".to_string(),
                ));
            }
        };

        // The zone-output pin must have at least one incoming wire — otherwise
        // the body cannot deliver a new accumulator value.
        let zone_output_arg_wires: Vec<IncomingWire> = match node.zone_output_arguments.first() {
            Some(arg) if !arg.incoming_wires.is_empty() => arg.incoming_wires.clone(),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "fold: body has no incoming wire on `new_acc` zone-output pin".to_string(),
                ));
            }
        };

        // c. Push the body onto the stack and pre-evaluate captures.
        // Captures are pre-evaluated against the outer context so nested
        // captures relying on outer captures still resolve.
        let mut body_stack = network_stack.to_vec();
        body_stack.push(NetworkStackElement {
            node_network: body.as_ref(),
            node_id,
        });

        let captures = match build_captures(
            network_evaluator,
            &body_stack,
            registry,
            context,
            body.as_ref(),
            &zone_output_arg_wires,
        ) {
            Ok(c) => c,
            Err(err) => return EvalOutput::single(NetworkResult::Error(format!("fold: {}", err))),
        };

        // d. Build an inner context for the body's iterations (mirrors the
        // FunctionEvaluator inherit-vs-fresh policy). Inherits `execute`,
        // `use_vdw_cutoff`, and `current_zone_input_values` (so ancestor
        // HOFs' iteration frames remain visible to nested captures); gets
        // fresh per-pass scratch state and the sealed captures map.
        let mut inner_ctx = context.fresh_inner_for_eager_body();
        inner_ctx.captured_source_values = captures;

        // e. Push one frame for this fold call. `acc` slot starts with init,
        // `element` slot starts with a placeholder that's overwritten on
        // every iteration. Captures must NOT be re-evaluated per iteration —
        // they're already sealed onto `inner_ctx` above.
        let mut acc = init_val;
        let placeholder = NetworkResult::None;
        inner_ctx.push_zone_input_frame(node_id, vec![acc.clone(), placeholder]);

        // Drain the source walker eagerly, evaluating the body's `new_acc`
        // wire each step.
        let result = loop {
            match walker.next(network_evaluator, registry, &mut inner_ctx) {
                None => break Ok(acc),
                Some(NetworkResult::Error(e)) => break Err(NetworkResult::Error(e)),
                Some(elem) => {
                    inner_ctx.write_zone_input_pin(node_id, 0, acc.clone());
                    inner_ctx.write_zone_input_pin(node_id, 1, elem);
                    let new_acc = network_evaluator.evaluate_zone_output(
                        &body_stack,
                        node_id,
                        0,
                        registry,
                        &mut inner_ctx,
                    );
                    if let NetworkResult::Error(_) = new_acc {
                        break Err(new_acc);
                    }
                    acc = new_acc;
                }
            }
        };

        inner_ctx.pop_zone_input_frame(node_id);
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

/// Walk the body for capture wires and pre-evaluate them once at body entry.
/// Mirrors `map.rs::build_captures` — see that file for the discipline.
#[allow(clippy::arc_with_non_send_sync)]
fn build_captures<'a>(
    evaluator: &NetworkEvaluator,
    body_stack: &[NetworkStackElement<'a>],
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    body: &NodeNetwork,
    zone_output_wires: &[IncomingWire],
) -> Result<Arc<HashMap<CaptureKey, NetworkResult>>, String> {
    let mut seen: HashSet<CaptureKey> = HashSet::new();
    let mut captures: HashMap<CaptureKey, NetworkResult> = HashMap::new();

    let mut body_node_ids: Vec<u64> = body.nodes.keys().copied().collect();
    body_node_ids.sort();

    let mut wires_to_check: Vec<IncomingWire> = Vec::new();
    for nid in body_node_ids {
        if let Some(node) = body.nodes.get(&nid) {
            for arg in &node.arguments {
                for w in &arg.incoming_wires {
                    wires_to_check.push(w.clone());
                }
            }
        }
    }
    for w in zone_output_wires {
        wires_to_check.push(w.clone());
    }

    for incoming in &wires_to_check {
        if !is_capture(incoming) {
            continue;
        }
        let key = CaptureKey::from_incoming(incoming);
        if !seen.insert(key.clone()) {
            continue;
        }
        let value = resolve_capture_source(evaluator, body_stack, registry, context, incoming);
        if let NetworkResult::Error(e) = &value {
            return Err(format!("failed to pre-evaluate capture: {}", e));
        }
        captures.insert(key, value);
    }

    Ok(Arc::new(captures))
}

fn is_capture(w: &IncomingWire) -> bool {
    match w.source_pin {
        SourcePin::NodeOutput { .. } => w.source_scope_depth > 0,
        SourcePin::ZoneInput { .. } => w.source_scope_depth > 1,
    }
}

fn resolve_capture_source<'a>(
    evaluator: &NetworkEvaluator,
    body_stack: &[NetworkStackElement<'a>],
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    incoming: &IncomingWire,
) -> NetworkResult {
    let depth = incoming.source_scope_depth as usize;
    let stack_len = body_stack.len();
    match incoming.source_pin {
        SourcePin::NodeOutput { pin_index } => {
            let source_frame_idx = stack_len.saturating_sub(1 + depth);
            let source_slice = &body_stack[..=source_frame_idx];
            evaluator.evaluate(
                source_slice,
                incoming.source_node_id,
                pin_index,
                registry,
                false,
                context,
            )
        }
        SourcePin::ZoneInput { pin_index } => context
            .current_zone_input(incoming.source_node_id, pin_index)
            .clone(),
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
