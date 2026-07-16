use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{DragDirection, EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The literal array node: an `Array[element_type]` authored entirely from
/// stored literals, with **no input pins** at all.
///
/// It is the complement of `sequence` (which collects *wired* values): `array`
/// = literal data, `sequence` = computed data. Having no input pins is what
/// makes every element edit a pure node-data mutation — no pin-count changes,
/// no wire stability problem, no stable per-element ids. See
/// `doc/design_array_node_and_field_hints.md` Part B, Decision 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayData {
    /// Type of every element, and hence of the output `Array[element_type]`.
    /// Must be **literal-capable** ([`is_literal_capable`]).
    pub element_type: DataType,
    /// The stored elements, one `TextValue` each. For a simple `element_type`
    /// this is the matching scalar `TextValue`; for a record `element_type` it
    /// is a [`TextValue::Object`] holding entries **only for set fields** — an
    /// absent entry means "unset", exactly the semantics of a missing
    /// `literal_values` key on `record_construct`.
    ///
    /// Literals are kept **verbatim** across an `element_type` change or a def
    /// edit — mismatches surface as localized eval errors rather than being
    /// silently dropped (the same no-silent-data-loss stance as switch-case
    /// healing).
    #[serde(default)]
    pub elements: Vec<TextValue>,
}

impl Default for ArrayData {
    fn default() -> Self {
        Self {
            element_type: DataType::Int,
            elements: Vec::new(),
        }
    }
}

/// The simple types the literal panel can edit — the `APISimpleParamType` set.
/// Deliberately excludes `IMat2`, which has no `APISimpleParamType` /
/// `APILiteralValue` member today (add both together if wanted).
pub fn is_simple_literal_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Bool
            | DataType::Int
            | DataType::Float
            | DataType::String
            | DataType::IVec2
            | DataType::IVec3
            | DataType::Vec2
            | DataType::Vec3
            | DataType::IMat3
            | DataType::Mat3
    )
}

/// The registry-free half of the literal-capable predicate: everything
/// [`is_literal_capable`] checks **except** that a named record's fields are
/// themselves simple (which needs a def lookup).
///
/// This exists because `NodeData::set_text_properties` has no registry
/// parameter. It still rejects every registry-independent exclusion —
/// structural types, `Function` / `Iter` / `Unit`, `IMat2`, nested `Array`s,
/// and anonymous records — so the text-format guard is meaningful; a named
/// record with a non-simple field slips past it and is caught at eval as a
/// localized per-element error (the stale-literal rule), which is the same
/// place a def edit would surface it anyway.
pub fn is_literal_capable_shape(data_type: &DataType) -> bool {
    is_simple_literal_type(data_type) || matches!(data_type, DataType::Record(RecordType::Named(_)))
}

/// Can `data_type` be authored as an `array` element? True for the simple
/// literal types, and for a `Record(Named(def))` whose every field — looked at
/// *through* its `Optional[..]` wrapper — is one of those simple types.
///
/// Excluded deliberately: structural types (`Blueprint` / `Crystal` /
/// `Molecule` / `Structure` / …), `Function` / `Iter` / `Unit` (no literal form
/// exists), `IMat2`, **nested arrays** and **record-typed record fields** (each
/// would recurse the element editor into a list-in-list / group-in-group UI),
/// and `Record(Anonymous)` (reserved for expr-inferred types). See
/// `doc/design_array_node_and_field_hints.md` Part B, Decision 2.
pub fn is_literal_capable(data_type: &DataType, registry: &NodeTypeRegistry) -> bool {
    if is_simple_literal_type(data_type) {
        return true;
    }
    let DataType::Record(RecordType::Named(name)) = data_type else {
        return false;
    };
    let Some(def) = registry.lookup_record_type_def(name) else {
        return false;
    };
    def.fields
        .iter()
        .all(|field| is_simple_literal_type(&field.data_type.record_field_pin_type()))
}

impl ArrayData {
    /// Convert one stored element to a `NetworkResult`, or produce a localized
    /// error naming the element index (and, for a record element, the field).
    fn element_to_result(
        &self,
        index: usize,
        element: &TextValue,
        registry: &NodeTypeRegistry,
    ) -> NetworkResult {
        // Record element: emit **all** fields (the emit-all-fields invariant of
        // `doc/design_optional_type.md`), a set field coerced to the field type
        // and an unset `Optional` field as an explicit `None`.
        if let DataType::Record(RecordType::Named(schema)) = &self.element_type {
            let Some(def) = registry.lookup_record_type_def(schema) else {
                return NetworkResult::Error(format!(
                    "array[{}]: record type '{}' not found",
                    index, schema
                ));
            };
            let Some(entries) = element.as_object() else {
                return NetworkResult::Error(format!(
                    "array[{}]: expected a record literal for element type {}",
                    index, self.element_type
                ));
            };

            let mut fields: Vec<(String, NetworkResult)> = Vec::with_capacity(def.fields.len());
            for field in &def.fields {
                // An `Optional[T]` field is edited as a plain `T` (the value /
                // wire layer never sees `Optional`), so literals coerce against
                // the inner `T`. See `doc/design_optional_type.md` §5.
                let pin_type = field.data_type.record_field_pin_type();
                let stored = entries
                    .iter()
                    .find(|(key, _)| *key == field.name)
                    .map(|(_, value)| value);
                let value = match stored {
                    Some(literal) => match literal.to_network_result(&pin_type) {
                        Some(result) => result,
                        None => {
                            return NetworkResult::Error(format!(
                                "array[{}].{}: literal does not match field type {}",
                                index, field.name, field.data_type
                            ));
                        }
                    },
                    None if field.data_type.is_optional() => NetworkResult::None,
                    None => {
                        return NetworkResult::Error(format!(
                            "array[{}].{} is unset",
                            index, field.name
                        ));
                    }
                };
                fields.push((field.name.clone(), value));
            }
            // `record(..)` re-sorts into canonical order.
            return NetworkResult::record(fields);
        }

        // Simple element: the same literal-coercion path `record_construct`
        // applies to an unwired field, so a whole-number literal coerces into a
        // `Float` element.
        match element.to_network_result(&self.element_type) {
            Some(result) => result,
            None => NetworkResult::Error(format!(
                "array[{}]: literal does not match element type {}",
                index, self.element_type
            )),
        }
    }
}

impl NodeData for ArrayData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        // Literal-only: no input pins, ever.
        custom.parameters = Vec::new();
        custom.output_pins =
            OutputPinDefinition::single_fixed(DataType::Array(Box::new(self.element_type.clone())));
        Some(custom)
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        // No input pins, so nothing to evaluate — every element comes from
        // stored data. An empty `elements` list is a valid empty `Array`.
        let mut items = Vec::with_capacity(self.elements.len());
        for (index, element) in self.elements.iter().enumerate() {
            let result = self.element_to_result(index, element, registry);
            // Nothing is partially emitted: one bad element fails the array.
            if matches!(result, NetworkResult::Error(_)) {
                return EvalOutput::single(result);
            }
            items.push(result);
        }
        EvalOutput::single(NetworkResult::Array(items))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        Some(format!("{} × {}", self.elements.len(), self.element_type))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "element_type".to_string(),
                TextValue::DataType(self.element_type.clone()),
            ),
            (
                "elements".to_string(),
                TextValue::Array(self.elements.clone()),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("element_type") {
            let data_type = v
                .as_data_type()
                .ok_or_else(|| "element_type must be a DataType".to_string())?;
            if !is_literal_capable_shape(data_type) {
                return Err(format!(
                    "array element_type {} is not literal-capable: only the simple literal types \
                     (Bool, Int, Float, String, IVec2, IVec3, Vec2, Vec3, IMat3, Mat3) and named \
                     record types can be authored as array elements. Use a `sequence` node to \
                     collect computed values instead.",
                    data_type
                ));
            }
            self.element_type = data_type.clone();
        }
        if let Some(v) = props.get("elements") {
            // Individual literals stay raw: eval reports per-element problems,
            // per the stale-literal rule.
            self.elements = v
                .as_array()
                .ok_or_else(|| "elements must be an array".to_string())?
                .clone();
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        HashMap::new()
    }

    /// `FromInput` — the user dragged backwards off a consumer pin expecting
    /// `Array[T]`: strict-peel `T` and accept iff it is literal-capable, so
    /// `array` surfaces in the drag-add popup for exactly the pins it can feed
    /// (including `atom_replace.rules` and `apply_style.rules`).
    ///
    /// `FromOutput` is never adaptable: the node has no input pins to offer.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        if direction != DragDirection::FromInput {
            return None;
        }
        let element_type = source_type.drag_element_type_from_input_strict()?;
        if !is_literal_capable(&element_type, registry) {
            return None;
        }
        Some(Box::new(ArrayData {
            element_type,
            elements: Vec::new(),
        }))
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "array".to_string(),
        description:
            "An array literal: pick an element type, then author the elements directly on \
            the node. It has no input pins — every element is stored data, so use a `sequence` \
            node instead when the elements are computed by other nodes. Element types are limited \
            to the simple literal types and named record types whose fields are all simple."
                .to_string(),
        summary: Some("Array literal".to_string()),
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![],
        output_pins: OutputPinDefinition::single_fixed(DataType::Array(Box::new(DataType::Int))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ArrayData::default()),
        node_data_saver: generic_node_data_saver::<ArrayData>,
        node_data_loader: generic_node_data_loader::<ArrayData>,
    }
}
