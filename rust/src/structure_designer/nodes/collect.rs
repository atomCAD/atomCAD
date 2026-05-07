use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
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
pub struct CollectData {
    /// Element type T. Drives the input pin's `Iter[T]` declared type and the
    /// output pin's `Array[T]` declared type.
    pub element_type: DataType,
    /// Optional cap on the number of elements collected. `Some(n)` collects
    /// at most `n` elements; `None` exhausts the stream. Overridden by the
    /// wired `limit` input pin when connected. See
    /// `doc/design_iter_display_via_collect.md`.
    #[serde(default)]
    pub limit: Option<i32>,
}

impl Default for CollectData {
    fn default() -> Self {
        Self {
            element_type: DataType::Int,
            limit: None,
        }
    }
}

impl NodeData for CollectData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        custom.parameters[0].data_type = DataType::Iterator(Box::new(self.element_type.clone()));
        custom.output_pins =
            OutputPinDefinition::single_fixed(DataType::Array(Box::new(self.element_type.clone())));
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
        let v = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);

        // Resolve effective limit: pin overrides stored when it provides a
        // concrete Int. A disconnected pin (or one yielding `None`) falls
        // through to the stored field. Errors propagate.
        let limit_arg =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let effective_limit: Option<i32> = match limit_arg {
            NetworkResult::Int(n) => Some(n),
            NetworkResult::Error(_) => return EvalOutput::single(limit_arg),
            _ => self.limit,
        };

        if let Some(n) = effective_limit {
            if n < 0 {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "collect: limit must be non-negative, got {}",
                    n
                )));
            }
        }

        let mut walker = match v {
            NetworkResult::None => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Iterator(w) => w,
            // No-op pass-through. The implicit `[T] → Iter[T]` wire conversion
            // will normally have wrapped any incoming array as
            // `NetworkResult::Iterator(Walker::from_array(_))` already; this
            // arm handles edge cases (e.g. a pin whose declared type is
            // `[T]` rather than `Iter[T]`, which validation permits).
            NetworkResult::Array(items) => Walker::from_array(items),
            NetworkResult::Error(_) => return EvalOutput::single(v),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "collect: input is not an iterator".to_string(),
                ));
            }
        };

        let cap = effective_limit.map(|n| n as usize);
        let mut out: Vec<NetworkResult> = Vec::new();
        let mut cap_hit = false;

        loop {
            let at_cap = matches!(cap, Some(n) if out.len() >= n);
            if at_cap {
                // We've collected the cap. Peek once to decide whether the
                // walker had more (cap-hit) or exhausted exactly at the cap.
                match walker.next(network_evaluator, registry, context) {
                    None => {}
                    Some(NetworkResult::Error(e)) => {
                        return EvalOutput::single(NetworkResult::Error(e));
                    }
                    Some(_) => {
                        cap_hit = true;
                    }
                }
                break;
            }
            match walker.next(network_evaluator, registry, context) {
                None => break,
                Some(NetworkResult::Error(e)) => {
                    return EvalOutput::single(NetworkResult::Error(e));
                }
                Some(elem) => out.push(elem),
            }
        }

        // Surface the live element count in the node-graph UI in place of
        // the raw array dump (which would be unreadable for any nontrivial
        // stream). The `pin_subtitles` channel is honored by the evaluator's
        // post-eval display-string clobber.
        let subtitle = if cap_hit {
            format!("(stopped at limit {})", effective_limit.unwrap())
        } else {
            format!("({} elements)", out.len())
        };
        let mut output = EvalOutput::single(NetworkResult::Array(out));
        output.pin_subtitles.insert(0, subtitle);
        output
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
        let mut props = vec![(
            "element_type".to_string(),
            TextValue::DataType(self.element_type.clone()),
        )];
        if let Some(n) = self.limit {
            props.push(("limit".to_string(), TextValue::Int(n)));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element_type") {
            self.element_type = v
                .as_data_type()
                .ok_or_else(|| "element_type must be a DataType".to_string())?
                .clone();
        }
        if let Some(v) = props.get("limit") {
            self.limit = Some(
                v.as_int()
                    .ok_or_else(|| "limit must be an Int".to_string())?,
            );
        }
        Ok(())
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        // collect is meant for streams — no scalar-broadcast on either side.
        // FromOutput: input pin is `Iter[T]`; we need `Iter[T]` / `Array[T]`.
        // FromInput:  output pin is `Array[T]`; we need `Array[T]`.
        let elem = match direction {
            DragDirection::FromOutput => source_type.drag_element_type_from_input_strict()?,
            DragDirection::FromInput => match source_type {
                DataType::Array(t) => (**t).clone(),
                _ => return None,
            },
        };
        Some(Box::new(CollectData {
            element_type: elem,
            limit: None,
        }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "collect".to_string(),
        description:
            "Materializes a lazy iterator into an array by exhausting the stream. The escape hatch when a downstream array consumer really does want the whole vector. The configured element type drives both the iterator-input pin and the array-output pin. Optional `limit` (stored on the node or wired as an Int input) caps the number of elements collected — useful for previewing long or unbounded streams. The wired pin overrides the stored value when connected."
                .to_string(),
        summary: Some("Materialize an iterator into an array".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "iter".to_string(),
                data_type: DataType::Iterator(Box::new(DataType::Int)),
            },
            Parameter {
                id: None,
                name: "limit".to_string(),
                data_type: DataType::Int,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Array(Box::new(DataType::Int))),
        public: true,
        node_data_creator: || Box::new(CollectData::default()),
        node_data_saver: generic_node_data_saver::<CollectData>,
        node_data_loader: generic_node_data_loader::<CollectData>,
    }
}
