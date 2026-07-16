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
use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};
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

/// The literal seeded into a freshly added (or reset) element of a simple
/// type: the same type zeros the Flutter literal panel renders for an unset row
/// (`literal_fields_editor.dart::_typeZero`) — numeric zeros, `false`, the empty
/// string, zero vectors, **identity** matrices.
///
/// `None` for a non-simple type; every caller has already established
/// literal-capability.
pub fn default_simple_literal(data_type: &DataType) -> Option<TextValue> {
    Some(match data_type {
        DataType::Bool => TextValue::Bool(false),
        DataType::Int => TextValue::Int(0),
        DataType::Float => TextValue::Float(0.0),
        DataType::String => TextValue::String(String::new()),
        DataType::IVec2 => TextValue::IVec2(IVec2::ZERO),
        DataType::IVec3 => TextValue::IVec3(IVec3::ZERO),
        DataType::Vec2 => TextValue::Vec2(DVec2::ZERO),
        DataType::Vec3 => TextValue::Vec3(DVec3::ZERO),
        DataType::IMat3 => TextValue::IMat3([[1, 0, 0], [0, 1, 0], [0, 0, 1]]),
        DataType::Mat3 => TextValue::Mat3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        _ => return None,
    })
}

/// The element seeded by `add_array_element` / a reset, so a new element
/// **evaluates immediately** rather than erroring until first edited: a simple
/// `element_type` gets that type's [`default_simple_literal`]; a record
/// `element_type` gets its **required** fields seeded with those same defaults
/// and its `Optional` fields left unset (an absent entry is "unset", which eval
/// emits as an explicit `None`).
///
/// A non-literal-capable `element_type` — unreachable through the guarded
/// setters — seeds an empty object, which eval localizes as a per-element
/// mismatch like any other stale literal.
pub fn seed_element(element_type: &DataType, registry: &NodeTypeRegistry) -> TextValue {
    if let DataType::Record(RecordType::Named(schema)) = element_type {
        let mut entries: Vec<(String, TextValue)> = Vec::new();
        if let Some(def) = registry.lookup_record_type_def(schema) {
            for field in &def.fields {
                if field.data_type.is_optional() {
                    continue;
                }
                if let Some(seed) = default_simple_literal(&field.data_type.record_field_pin_type())
                {
                    entries.push((field.name.clone(), seed));
                }
            }
        }
        return TextValue::Object(entries);
    }
    default_simple_literal(element_type).unwrap_or_else(|| TextValue::Object(Vec::new()))
}

impl ArrayData {
    /// Bounds-check helper shared by the element mutators. Errors carry the
    /// index and length so an out-of-range API call from a stale Flutter panel
    /// is reported rather than silently ignored.
    fn check_index(&self, index: usize) -> Result<(), String> {
        if index >= self.elements.len() {
            return Err(format!(
                "element index {} out of range ({} elements)",
                index,
                self.elements.len()
            ));
        }
        Ok(())
    }

    /// Insert a freshly seeded element at `index` (`index == len` appends).
    pub fn insert_element(
        &mut self,
        index: usize,
        registry: &NodeTypeRegistry,
    ) -> Result<(), String> {
        if index > self.elements.len() {
            return Err(format!(
                "element index {} out of range ({} elements)",
                index,
                self.elements.len()
            ));
        }
        self.elements
            .insert(index, seed_element(&self.element_type, registry));
        Ok(())
    }

    pub fn remove_element(&mut self, index: usize) -> Result<(), String> {
        self.check_index(index)?;
        self.elements.remove(index);
        Ok(())
    }

    /// Move the element at `from` to `to`, where `to` is its index in the
    /// **resulting** list (so a move-down by one is `from + 1`).
    pub fn move_element(&mut self, from: usize, to: usize) -> Result<(), String> {
        self.check_index(from)?;
        self.check_index(to)?;
        if from == to {
            return Ok(());
        }
        let element = self.elements.remove(from);
        self.elements.insert(to, element);
        Ok(())
    }

    /// Replace one element's whole literal. The value is stored **raw** — no
    /// coercion check — per the stale-literal rule: eval reports per-element
    /// problems, so nothing is silently dropped or rewritten here.
    pub fn set_element_literal(&mut self, index: usize, value: TextValue) -> Result<(), String> {
        self.check_index(index)?;
        self.elements[index] = value;
        Ok(())
    }

    /// Reset one element back to its seeded default — the "clear" action a
    /// stale row offers. For a record element this re-seeds the required fields
    /// and drops every `Optional` back to unset.
    pub fn reset_element(
        &mut self,
        index: usize,
        registry: &NodeTypeRegistry,
    ) -> Result<(), String> {
        self.check_index(index)?;
        self.elements[index] = seed_element(&self.element_type, registry);
        Ok(())
    }

    /// Set one field of a record element, keyed by field **name** (the storage
    /// key — see the rename cascade in `node_type_registry.rs`). An element
    /// that is not a record literal is rejected rather than being silently
    /// replaced: the panel renders such an element as a stale row offering a
    /// reset, so it never reaches this path.
    pub fn set_element_field_literal(
        &mut self,
        index: usize,
        field_name: &str,
        value: TextValue,
    ) -> Result<(), String> {
        self.check_index(index)?;
        let entries = Self::element_entries_mut(&mut self.elements[index], index)?;
        match entries.iter_mut().find(|(key, _)| key == field_name) {
            Some((_, slot)) => *slot = value,
            None => entries.push((field_name.to_string(), value)),
        }
        Ok(())
    }

    /// Unset one field of a record element (removes the entry). For an
    /// `Optional` field that means "inherit"; for a required field, eval
    /// localizes it as `array[i].field is unset`.
    pub fn clear_element_field_literal(
        &mut self,
        index: usize,
        field_name: &str,
    ) -> Result<(), String> {
        self.check_index(index)?;
        let entries = Self::element_entries_mut(&mut self.elements[index], index)?;
        entries.retain(|(key, _)| key != field_name);
        Ok(())
    }

    fn element_entries_mut(
        element: &mut TextValue,
        index: usize,
    ) -> Result<&mut Vec<(String, TextValue)>, String> {
        match element {
            TextValue::Object(entries) => Ok(entries),
            _ => Err(format!("array[{}] is not a record literal", index)),
        }
    }

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
