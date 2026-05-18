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

        // External: only `xs` remains. The predicate body lives inside the zone.
        custom.parameters[0].data_type = iter_ty.clone();
        custom.output_pins = OutputPinDefinition::single_fixed(iter_ty);

        // Inside-facing pins: one element source, one Bool destination.
        custom.zone_input_pins = vec![OutputPinDefinition::fixed(
            "element",
            self.element_type.clone(),
        )];
        custom.zone_output_pins = vec![Parameter {
            id: None,
            name: "keep".to_string(),
            data_type: DataType::Bool,
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
        // a. Resolve `xs` first — runs against the HOF's containing network
        // scope, before the body is pushed.
        let xs_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        let xs_walker = match xs_val {
            NetworkResult::Iterator(w) => w,
            // Belt-and-braces: the implicit `[T] → Iter[T]` wire conversion
            // normally wraps any incoming array as `Iterator(_)` already.
            NetworkResult::Array(items) => Walker::from_array(items),
            err @ NetworkResult::Error(_) => return EvalOutput::single(err),
            other => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "filter: xs is not an iterator (got {})",
                    other.to_display_string()
                )));
            }
        };

        // b. Grab the body. A `filter` node without a populated zone is
        // ill-formed — populate runs at add_node time.
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let body = match node.zone.as_ref() {
            Some(b) => Arc::clone(b),
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "filter: missing zone body".to_string(),
                ));
            }
        };

        // The zone-output pin must have at least one incoming wire — otherwise
        // the body cannot deliver a per-iteration Bool. Reports as eval-time
        // error; Phase 6 will surface this at validation time as well.
        let zone_output_arg_wires: Vec<IncomingWire> = match node.zone_output_arguments.first() {
            Some(arg) if !arg.incoming_wires.is_empty() => arg.incoming_wires.clone(),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "filter: body has no incoming wire on `keep` zone-output pin".to_string(),
                ));
            }
        };

        // c. Pre-evaluate captures. Push the body so source_scope_depth walks
        // land correctly, pre-evaluate, then pop.
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
                return EvalOutput::single(NetworkResult::Error(format!("filter: {}", err)));
            }
        };

        // d. Construct the walker. Body, captures, and the zone-output wires
        // travel via the walker; subsequent iterations re-build the body
        // stack in `next()` and use the cached captures.
        let walker = Walker::filter_zone(
            xs_walker,
            body,
            captures,
            Arc::new(zone_output_arg_wires),
            node_id,
        );
        EvalOutput::single(NetworkResult::Iterator(walker))
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

/// A wire is a capture iff its source is outside the body that contains its
/// destination.
fn is_capture(w: &IncomingWire) -> bool {
    match w.source_pin {
        SourcePin::NodeOutput { .. } => w.source_scope_depth > 0,
        // `ZoneInput { depth = 1 }` references the immediately-enclosing
        // HOF's iteration values — per-iteration, not a capture. Deeper
        // references go through the cache.
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
        name: "filter".to_string(),
        description:
            "Lazily yields each element pulled from `xs` for which the inline zone body's `keep` pin evaluates to `true`, preserving order. The intermediate sequence is never materialised. The body reads the per-element value from the inside-facing `element` source pin and delivers a Bool to the inside-facing `keep` destination pin."
                .to_string(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![Parameter {
            id: None,
            name: "xs".to_string(),
            data_type: DataType::Iterator(Box::new(DataType::Float)),
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::Iterator(Box::new(
            DataType::Float,
        ))),
        zone_input_pins: vec![OutputPinDefinition::fixed("element", DataType::Float)],
        zone_output_pins: vec![Parameter {
            id: None,
            name: "keep".to_string(),
            data_type: DataType::Bool,
        }],
        public: true,
        node_data_creator: || Box::new(FilterData::default()),
        node_data_saver: generic_node_data_saver::<FilterData>,
        node_data_loader: generic_node_data_loader::<FilterData>,
    }
}
