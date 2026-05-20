use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::iterator_walker::Walker;
use crate::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::zone_closure::build_inline_closure;
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

        // b. Build the closure from this node's own inline zone: grab the
        // body, freeze captures once, and collect the zone-output wire(s).
        let closure = match build_inline_closure(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            "filter",
        ) {
            Ok(c) => c,
            Err(e) => return EvalOutput::single(e),
        };

        // c. Construct the walker. The closure travels via the walker;
        // subsequent iterations run it once per element via `run_closure_once`.
        let walker = Walker::filter_zone(xs_walker, closure);
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
