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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayAtData {
    /// Element type of the input array (and of the output).
    pub element_type: DataType,
    /// Stored index used when the `index` input pin is not connected.
    /// `0` (the default) reads the first element. Overridden by the wired
    /// `index` input pin when connected. Negative values produce an
    /// out-of-bounds evaluation error, same as a wired negative index.
    #[serde(default)]
    pub index: i32,
}

impl Default for ArrayAtData {
    fn default() -> Self {
        Self {
            element_type: DataType::Int,
            index: 0,
        }
    }
}

impl NodeData for ArrayAtData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        custom.parameters[0].data_type = DataType::Array(Box::new(self.element_type.clone()));
        // index pin stays Int.
        custom.output_pins = OutputPinDefinition::single_fixed(self.element_type.clone());
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
        let array_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &array_val {
            NetworkResult::None => return EvalOutput::single(NetworkResult::None),
            NetworkResult::Error(_) => return EvalOutput::single(array_val),
            _ => {}
        }

        // Same override pattern as `collect.limit`: a wired Int pin replaces
        // the stored field; a disconnected pin (or one yielding `None`) falls
        // through to `self.index`. Errors propagate.
        let index_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let index: i32 = match index_val {
            NetworkResult::Int(n) => n,
            NetworkResult::Error(_) => return EvalOutput::single(index_val),
            _ => self.index,
        };

        let items = match array_val {
            NetworkResult::Array(items) => items,
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "array_at: array input is not an array".to_string(),
                ));
            }
        };

        if index < 0 || (index as usize) >= items.len() {
            return EvalOutput::single(NetworkResult::Error(format!(
                "array index {} out of bounds for array of length {}",
                index,
                items.len()
            )));
        }

        EvalOutput::single(items.into_iter().nth(index as usize).unwrap())
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
        if self.index != 0 {
            props.push(("index".to_string(), TextValue::Int(self.index)));
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
        if let Some(v) = props.get("index") {
            self.index = v
                .as_int()
                .ok_or_else(|| "index must be an Int".to_string())?;
        }
        Ok(())
    }

    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        _registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        let elem = match direction {
            // FromOutput: source plugs into the `array: Array[T]` input.
            //             Peel `Array[T]`/`Iter[T]` or broadcast scalar `T`
            //             (the implicit `T → [T]` rule); static-match catches
            //             over-promised cases (e.g. `Iter[T]` source).
            DragDirection::FromOutput => source_type.drag_element_type_from_output()?,
            // FromInput: source is the consumer pin's declared type, which
            //            equals array_at's output (the element type T).
            //            Direct copy — peeling would mis-type the output.
            DragDirection::FromInput => {
                if matches!(source_type, DataType::Function(_)) || source_type.is_abstract() {
                    return None;
                }
                source_type.clone()
            }
        };
        // Reject Iter element types: Array[Iter[T]] is technically valid
        // but virtually never the user's intent, and surfacing array_at for
        // an Iter[T] drag-from-input renders the output pin as Iter[T],
        // misleading users into thinking array_at is iterator-aware.
        if matches!(elem, DataType::Iterator(_)) {
            return None;
        }
        Some(Box::new(ArrayAtData {
            element_type: elem,
            index: 0,
        }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "array_at".to_string(),
        description:
            "Reads one element from an array at a given integer index. The index can be stored on the node (default 0) or supplied by the wired `index` input pin — the wired pin overrides the stored value when connected. Out-of-bounds indices produce an evaluation error."
                .to_string(),
        summary: Some("Element access on an array".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter {
                id: None,
                name: "array".to_string(),
                data_type: DataType::Array(Box::new(DataType::Int)),
            },
            Parameter {
                id: None,
                name: "index".to_string(),
                data_type: DataType::Int,
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Int),
        public: true,
        node_data_creator: || Box::new(ArrayAtData::default()),
        node_data_saver: generic_node_data_saver::<ArrayAtData>,
        node_data_loader: generic_node_data_loader::<ArrayAtData>,
    }
}
