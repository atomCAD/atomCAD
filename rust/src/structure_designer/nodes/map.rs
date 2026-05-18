use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::{
    CaptureKey, NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
use crate::structure_designer::node_network::{Argument, IncomingWire, NodeNetwork, SourcePin};
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
pub struct MapData {
    pub input_type: DataType,
    pub output_type: DataType,
}

impl NodeData for MapData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom_node_type = base_node_type.clone();

        // External: only `xs` remains. The body lives inside the zone.
        custom_node_type.parameters[0].data_type =
            DataType::Iterator(Box::new(self.input_type.clone()));
        custom_node_type.output_pins =
            OutputPinDefinition::single(DataType::Iterator(Box::new(self.output_type.clone())));

        // Inside-facing pins: one element source, one result destination.
        custom_node_type.zone_input_pins = vec![OutputPinDefinition::fixed(
            "element",
            self.input_type.clone(),
        )];
        custom_node_type.zone_output_pins = vec![Parameter {
            id: None,
            name: "result".to_string(),
            data_type: self.output_type.clone(),
        }];

        Some(custom_node_type)
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
                    "map: xs is not an iterator (got {})",
                    other.to_display_string()
                )));
            }
        };

        // b. Grab the body. A `map` node without a populated zone is
        // ill-formed — populate runs at add_node time.
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        let body = match node.zone.as_ref() {
            Some(b) => Arc::clone(b),
            None => {
                return EvalOutput::single(NetworkResult::Error(
                    "map: missing zone body".to_string(),
                ));
            }
        };

        // The zone-output pin must have at least one incoming wire — otherwise
        // the body cannot deliver a per-iteration result. Reports as eval-time
        // error; Phase 6 will surface this at validation time as well.
        let zone_output_arg_wires: Vec<IncomingWire> = match node.zone_output_arguments.first() {
            Some(arg) if !arg.incoming_wires.is_empty() => arg.incoming_wires.clone(),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "map: body has no incoming wire on `result` zone-output pin".to_string(),
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
            Err(err) => return EvalOutput::single(NetworkResult::Error(format!("map: {}", err))),
        };

        // d. Construct the walker. Body, captures, and the zone-output wires
        // travel via the walker; subsequent iterations re-build the body
        // stack in `next()` and use the cached captures.
        let walker = Walker::map_zone(
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
        vec![
            (
                "input_type".to_string(),
                TextValue::DataType(self.input_type.clone()),
            ),
            (
                "output_type".to_string(),
                TextValue::DataType(self.output_type.clone()),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("input_type") {
            self.input_type = v
                .as_data_type()
                .ok_or_else(|| "input_type must be a DataType".to_string())?
                .clone();
        }
        if let Some(v) = props.get("output_type") {
            self.output_type = v
                .as_data_type()
                .ok_or_else(|| "output_type must be a DataType".to_string())?
                .clone();
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("xs".to_string(), (true, None)); // required
        m
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // Both directions accept the same shape: peel `Iter[T]` / `Array[T]` /
        // `T`. The over-promised case (e.g. `FromInput` with a scalar `T`) is
        // caught by the filter's static-match verification step.
        let elem = source_type.drag_element_type_from_output()?;
        Some(Box::new(MapData {
            input_type: elem.clone(),
            output_type: elem,
        }))
    }
}

/// Walk the body for capture wires and pre-evaluate them once at body entry.
///
/// A wire is a *capture* iff its source is outside this body — see
/// [`is_capture`]. Each unique source-side identity (`CaptureKey`) is
/// evaluated once and stored in the cache.
///
/// We do **not** clear the caller's existing `captured_source_values` while
/// building — for a nested HOF, the outer body's captures are already
/// installed there (via the lazy walker's `CapturesGuard`) and would be
/// needed when resolving an inner capture whose source itself reads through
/// the outer body's captures.
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

    // Collect every wire on every body-internal node, plus the wires
    // terminating at the HOF's zone-output arguments. The iteration order is
    // deterministic so capture insertion order is stable across runs.
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
        // Local body-internal wire: not a capture.
        SourcePin::NodeOutput { .. } => w.source_scope_depth > 0,
        // `ZoneInput { depth = 1 }` references the immediately-enclosing
        // HOF's iteration values — per-iteration, not a capture. Deeper
        // references go through the cache.
        SourcePin::ZoneInput { .. } => w.source_scope_depth > 1,
    }
}

/// Resolve a capture wire's source value during pre-evaluation.
///
/// Walk `source_scope_depth` levels up the stack and evaluate via the normal
/// path. `ZoneInput` captures (depth > 1) read from the live
/// `current_zone_input_values` of the enclosing HOF, which is correct
/// because at capture-build time that frame is the outer iteration's
/// current frame.
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
            // depth >= 1 (caller guaranteed via `is_capture`).
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
        SourcePin::ZoneInput { pin_index } => {
            // depth > 1 (caller guaranteed). Read the live iteration value
            // of the referenced outer HOF.
            context
                .current_zone_input(incoming.source_node_id, pin_index)
                .clone()
        }
    }
}

// `Argument` is referenced by the empty-body initialization in `add_node`
// (via `populate_custom_node_type_cache_with_types`). Silence an unused-import
// warning when this file is built standalone.
#[allow(dead_code)]
fn _force_argument_use(_: &Argument) {}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "map".to_string(),
      description: "Lazily applies the inline zone body to every element pulled from `xs`, producing an iterator of the resulting values. The intermediate sequence is never materialised — downstream consumers (`fold`, `collect`, …) drive the stream one element at a time. The body reads the per-element value from the inside-facing `element` source pin and delivers its result to the inside-facing `result` destination pin.".to_string(),
      summary: None,
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![
        Parameter {
          id: None,
          name: "xs".to_string(),
          data_type: DataType::Iterator(Box::new(DataType::Float)), // will change based on  ParameterData::data_type.
        },
      ],
      output_pins: OutputPinDefinition::single(DataType::Iterator(Box::new(DataType::Float))), // will change based on the output type
      zone_input_pins: vec![OutputPinDefinition::fixed("element", DataType::Float)],
      zone_output_pins: vec![Parameter {
        id: None,
        name: "result".to_string(),
        data_type: DataType::Float,
      }],
      public: true,
      node_data_creator: || Box::new(MapData {
        input_type: DataType::Float,
        output_type: DataType::Float,
      }),
      node_data_saver: generic_node_data_saver::<MapData>,
      node_data_loader: generic_node_data_loader::<MapData>,
    }
}
