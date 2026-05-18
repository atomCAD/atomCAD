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

/// Side-effect counterpart of `map`: iterates `xs`, evaluates the inline zone
/// body for each element purely for its side effects, and returns `Unit`.
/// Unlike `map` the inner sequence is never produced — the body's `out` pin
/// is discarded into `Unit` via the universal `T → Unit` widening.
///
/// Display-pass cost is **zero**: because the output pin is `Unit`, the
/// central skip rule in the evaluator short-circuits this node entirely on
/// non-execute passes (see `doc/design_node_execution.md` Phase 2 — Central
/// skip rule). `eval` only runs under `context.execute == true`.
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

        // External: only `xs` remains. The side-effecting body lives inside
        // the zone.
        custom.parameters[0].data_type = DataType::Iterator(Box::new(self.input_type.clone()));
        custom.output_pins = OutputPinDefinition::single_fixed(DataType::Unit);

        // Inside-facing pins: one element source, one Unit destination. The
        // body wires whatever it wants into `out` — the universal `T → Unit`
        // widening discards the value.
        custom.zone_input_pins = vec![OutputPinDefinition::fixed(
            "element",
            self.input_type.clone(),
        )];
        custom.zone_output_pins = vec![Parameter {
            id: None,
            name: "out".to_string(),
            data_type: DataType::Unit,
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
        // No `if !context.execute { return Unit; }` guard — `foreach`'s output
        // is `Unit`, so the central skip rule short-circuits this node before
        // `eval` is ever called on display passes. When this body runs,
        // `context.execute == true`. See `doc/design_node_execution.md`.

        // a. Resolve `xs` first — runs against the HOF's containing network
        // scope, before the body is pushed.
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

        // b. Grab the body.
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let body = match node.zone.as_ref() {
            Some(b) => Arc::clone(b),
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "foreach: missing zone body".to_string(),
                ));
            }
        };

        // The zone-output pin must have at least one incoming wire — otherwise
        // the body produces no side effect.
        let zone_output_arg_wires: Vec<IncomingWire> = match node.zone_output_arguments.first() {
            Some(arg) if !arg.incoming_wires.is_empty() => arg.incoming_wires.clone(),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "foreach: body has no incoming wire on `out` zone-output pin".to_string(),
                ));
            }
        };

        // c. Push the body and pre-evaluate captures.
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
            Err(err) => {
                return EvalOutput::single(NetworkResult::Error(format!("foreach: {}", err)));
            }
        };

        // d. Build inner context for the body's iterations. Inherits
        // `execute` (true here by construction — see the comment at the top
        // of this function) so nested `export_xyz`/`print` actually fire.
        let mut inner_ctx = context.fresh_inner_for_eager_body();
        inner_ctx.captured_source_values = captures;

        // e. Push one frame for this foreach call; rewrite the element slot
        // per iteration.
        let placeholder = NetworkResult::None;
        inner_ctx.push_zone_input_frame(node_id, vec![placeholder]);

        let result = loop {
            match walker.next(network_evaluator, registry, &mut inner_ctx) {
                None => break Ok(()),
                Some(NetworkResult::Error(e)) => break Err(NetworkResult::Error(e)),
                Some(elem) => {
                    inner_ctx.write_zone_input_pin(node_id, 0, elem);
                    let out = network_evaluator.evaluate_zone_output(
                        &body_stack,
                        node_id,
                        0,
                        registry,
                        &mut inner_ctx,
                    );
                    if let NetworkResult::Error(_) = out {
                        // Fail-fast: surface the first body error as
                        // `foreach`'s output and halt the loop. Continuing
                        // past errors during a batch export silently produces
                        // a partial result set with no visible signal — the
                        // worst of all worlds.
                        break Err(out);
                    }
                    // Successful results are dropped — body return type is
                    // discarded into Unit (universal `T → Unit` widening).
                }
            }
        };

        inner_ctx.pop_zone_input_frame(node_id);
        context.drain_inner_context(inner_ctx);

        EvalOutput::single(match result {
            Ok(()) => NetworkResult::Unit,
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
        name: "foreach".to_string(),
        description: "Iterates `xs` and evaluates the inline zone body on every element for its side effects, returning `Unit`. The body delivers a value to the inside-facing `out` destination pin — the universal `T → Unit` widening discards it. On normal display passes the whole pipeline is skipped — `foreach` only iterates when the user invokes Execute on it (or one of its descendants in the evaluation tree).".to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![Parameter {
            id: None,
            name: "xs".to_string(),
            data_type: DataType::Iterator(Box::new(DataType::Float)),
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::Unit),
        zone_input_pins: vec![OutputPinDefinition::fixed("element", DataType::Float)],
        zone_output_pins: vec![Parameter {
            id: None,
            name: "out".to_string(),
            data_type: DataType::Unit,
        }],
        public: true,
        node_data_creator: || Box::new(ForeachData::default()),
        node_data_saver: generic_node_data_saver::<ForeachData>,
        node_data_loader: generic_node_data_loader::<ForeachData>,
    }
}
