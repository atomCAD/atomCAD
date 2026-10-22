//! `record_destructure`: splits a record into one output pin per field.
//!
//! Three modes, decided by its data (`doc/design_anonymous_record_readers.md`):
//!
//! - **Named** (`schema` set, no `fields`): the fields are a record def's, the
//!   pins carry the def's `FieldId`s, and the input is `Record(Named(schema))`.
//! - **Anonymous** (`schema` empty, `fields` non-empty): the fields are the
//!   node's own, the pins carry `ANON_FIELD_PIN_ID_BASE | field.id`, and the
//!   input is the anonymous record of those fields. The fields always describe
//!   the input type: they are adopted from a wire, never edited by hand.
//! - **Unset** (both empty): the input is `{}`, which every resolvable record
//!   converts to, and there is one placeholder output.
//!
//! The type is adopted through [`RecordTypedInput`] — on drag-place, on
//! connect while the node is uncommitted and by *Take type from wire*; the
//! orchestration is `record_input_type_ops.rs`.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{DragDirection, EvalOutput, MissingDefs, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::{NodeTypeRegistry, RecordDefs};
use crate::nodes::local_record_fields::{
    self, ANON_FIELD_PIN_ID_BASE, LocalRecordField, is_zero, rebuild_fields_by_name,
};
use crate::record_typed_input::RecordTypedInput;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordDestructureData {
    /// Name of a record type def in the project's registry. Wrapped as
    /// `RecordType::Named(self.schema.clone())` at use time. An empty string
    /// means anonymous mode when [`Self::fields`] is non-empty, and unset
    /// otherwise.
    #[serde(default)]
    pub schema: String,

    /// Anonymous mode only (`schema` empty): the fields of the input record
    /// type, adopted from a wire. Must be empty while `schema` is set (the
    /// invariant checker reports a node that has both).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<LocalRecordField>,

    /// Allocator floor for [`LocalRecordField::id`]; never decreases, so a
    /// field that is gone never hands its id to another.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_field_id: u64,
}

impl RecordDestructureData {
    /// A named-mode node reading the def `schema`.
    pub fn named(schema: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            ..Default::default()
        }
    }

    /// Anonymous mode: no schema, and the fields are the node's own.
    pub fn is_anonymous(&self) -> bool {
        self.schema.is_empty() && !self.fields.is_empty()
    }

    /// The field names the output pins stand for, in pin order: the def's in
    /// named mode (`None` when the def is missing), the local ones in
    /// anonymous mode, `None` when unset.
    fn field_names(&self, registry: &NodeTypeRegistry) -> Option<Vec<String>> {
        if self.is_anonymous() {
            return Some(self.fields.iter().map(|f| f.name.clone()).collect());
        }
        let def = registry.lookup_record_type_def(&self.schema)?;
        Some(def.fields.iter().map(|f| f.name.clone()).collect())
    }
}

impl RecordTypedInput for RecordDestructureData {
    fn record_input_type(&self) -> Option<RecordType> {
        if !self.schema.is_empty() {
            Some(RecordType::Named(self.schema.clone()))
        } else if self.fields.is_empty() {
            None
        } else {
            Some(RecordType::anonymous(
                self.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.data_type.clone()))
                    .collect(),
            ))
        }
    }

    fn set_record_input_type(
        &mut self,
        t: &RecordType,
        _defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        match t {
            RecordType::Named(name) => {
                self.schema = name.clone();
                self.fields.clear();
            }
            RecordType::Anonymous(type_fields) => {
                let mut fields = self.fields.clone();
                let mut next_field_id = self.next_field_id;
                rebuild_fields_by_name(&mut fields, &mut next_field_id, type_fields.clone())?;
                self.schema.clear();
                self.fields = fields;
                self.next_field_id = next_field_id;
            }
        }
        Ok(())
    }
}

impl NodeData for RecordDestructureData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// `record_destructure`'s input pin type and per-field output pins are
    /// derived from its mode, through `build_record_node_type`.
    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
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
        let Some(field_names) = self.field_names(registry) else {
            // Unset, or a dangling schema. There are no real per-field output
            // pins (only the placeholder), so emit a single None.
            return EvalOutput::single(NetworkResult::None);
        };

        // Read the input record. Pin 0 of the parameters is the record input
        // (set by the registry-aware cache populator).
        let record_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &record_val {
            NetworkResult::None => {
                // No input — every output pin emits None.
                let nones = vec![NetworkResult::None; field_names.len().max(1)];
                return EvalOutput::multi(nones);
            }
            NetworkResult::Error(_) => {
                let errs = vec![record_val.clone(); field_names.len().max(1)];
                return EvalOutput::multi(errs);
            }
            _ => {}
        }

        // Pass-through: the runtime record may carry extra fields beyond the
        // input type. Look up each field by name (binary search on the
        // canonical field list); fields the type declares but the runtime
        // value lacks — an unset `Optional` field — emit None on their pin.
        if field_names.is_empty() {
            // Match the placeholder output pin (one None) used when the
            // schema declares zero fields.
            return EvalOutput::multi(vec![NetworkResult::None]);
        }
        let outputs = field_names
            .iter()
            .map(|name| {
                record_val
                    .extract_record_field(name)
                    .cloned()
                    .unwrap_or(NetworkResult::None)
            })
            .collect();
        EvalOutput::multi(outputs)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        if self.schema.is_empty() {
            None
        } else {
            Some(self.schema.clone())
        }
    }

    /// `schema` always; `fields` only in anonymous mode, written exactly like
    /// `record_construct`'s. An unset node writes `schema: ""`, which also
    /// keeps its statement from adopting the wire's type on the way back in.
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![("schema".to_string(), TextValue::String(self.schema.clone()))];
        if self.is_anonymous() {
            let fields = self
                .fields
                .iter()
                .map(|f| {
                    TextValue::Object(vec![
                        ("name".to_string(), TextValue::String(f.name.clone())),
                        (
                            "data_type".to_string(),
                            TextValue::DataType(f.data_type.clone()),
                        ),
                    ])
                })
                .collect();
            props.push(("fields".to_string(), TextValue::Array(fields)));
        }
        props
    }

    /// `fields` is a key in every mode, so a fresh (unset) node takes it.
    fn text_literal_keys_with_defs(&self, _defs: RecordDefs<'_>) -> Vec<String> {
        vec!["fields".to_string()]
    }

    /// `schema` first, then `fields`, so a statement can switch a node to
    /// anonymous mode and list its fields at once. Setting a schema clears the
    /// local fields; `fields` is refused on a named-mode node, and
    /// `fields: []` makes the node unset. Ids are kept by name.
    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("schema") {
            let schema = v
                .as_string()
                .ok_or_else(|| "schema must be a String".to_string())?
                .to_string();
            if schema != self.schema {
                self.schema = schema;
                self.fields.clear();
            }
        }
        if let Some(v) = props.get("fields") {
            let TextValue::Array(entries) = v else {
                return Err("fields must be an array".to_string());
            };
            let mut new_fields: Vec<(String, DataType)> = Vec::with_capacity(entries.len());
            for entry in entries {
                let TextValue::Object(obj) = entry else {
                    return Err("each field must be an object { name, data_type }".to_string());
                };
                let name = obj
                    .iter()
                    .find(|(k, _)| k == "name")
                    .and_then(|(_, v)| v.as_string())
                    .ok_or_else(|| "field name must be a string".to_string())?
                    .to_string();
                let data_type = obj
                    .iter()
                    .find(|(k, _)| k == "data_type")
                    .and_then(|(_, v)| v.to_data_type())
                    .ok_or_else(|| "field data_type must be a DataType".to_string())?;
                if !crate::declared_inputs::is_input_identifier(&name) {
                    return Err(format!(
                        "Field name '{}' must be a letter or '_' followed by letters, digits or '_'",
                        name
                    ));
                }
                if new_fields.iter().any(|(n, _)| *n == name) {
                    return Err(format!("Duplicate field name '{}'", name));
                }
                new_fields.push((name, data_type));
            }
            if self.schema.is_empty() {
                rebuild_fields_by_name(&mut self.fields, &mut self.next_field_id, new_fields)?;
            } else if !new_fields.is_empty() {
                return Err(
                    "fields can only be listed on a record_destructure with no schema".to_string(),
                );
            }
        }
        Ok(())
    }

    /// `record_destructure`'s `record` input pin accepts a record, so a record
    /// dragged off an *output* pin (`FromOutput`) can feed it. The node adopts
    /// the dragged type exactly as a wire would: a named record's def, an
    /// anonymous record's fields (in the type's canonical order; the order of
    /// an anonymous `record_construct` source is restored when the drop's
    /// wire is connected). `FromInput` is not adaptable: the node's output
    /// pins are the individual fields, not a record. See
    /// `doc/design_drag_aware_add_node.md`, issue #312 and
    /// `doc/design_anonymous_record_readers.md`.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        if direction != DragDirection::FromOutput {
            return None;
        }
        let DataType::Record(record_type) = source_type else {
            return None;
        };
        // Don't pre-set a dangling schema — require the def to resolve.
        if let RecordType::Named(name) = record_type {
            registry.lookup_record_type_def(name)?;
        }
        let mut data = RecordDestructureData::default();
        data.set_record_input_type(record_type, registry.record_defs())
            .ok()?;
        Some(Box::new(data))
    }

    fn record_typed_input(&self) -> Option<&dyn RecordTypedInput> {
        Some(self)
    }

    fn record_typed_input_mut(&mut self) -> Option<&mut dyn RecordTypedInput> {
        Some(self)
    }

    fn record_schema_ref(&self) -> Option<&str> {
        Some(&self.schema)
    }

    fn set_record_schema_ref(&mut self, name: String) {
        self.schema = name;
    }

    fn local_record_fields(&self) -> Option<(&[LocalRecordField], u64)> {
        Some((&self.fields, self.next_field_id))
    }

    fn local_record_fields_mut(&mut self) -> Option<&mut [LocalRecordField]> {
        Some(&mut self.fields)
    }

    fn build_record_node_type(
        &self,
        base: &NodeType,
        defs: RecordDefs<'_>,
        missing: MissingDefs,
    ) -> Option<NodeType> {
        if self.schema.is_empty() {
            // Anonymous and unset modes need no defs, so they never refuse.
            return Some(build_anonymous_node_type(base, &self.fields));
        }
        if missing == MissingDefs::Refuse && defs.lookup(&self.schema).is_none() {
            return None;
        }
        Some(build_node_type_for_schema_with_defs(
            base,
            &self.schema,
            defs,
        ))
    }

    fn record_node_problems(&self, defs: RecordDefs<'_>) -> Vec<String> {
        local_record_fields::local_field_problems(&self.fields, defs)
    }
}

/// The `NodeType` of an anonymous `record_destructure`: input
/// `record: Record(anonymous(fields))` and one output per local field in
/// authored order, each carrying `ANON_FIELD_PIN_ID_BASE | id`. With no
/// fields it is the unset node: input `{}`, which every resolvable record
/// converts to, and the single placeholder output.
pub fn build_anonymous_node_type(
    base_node_type: &NodeType,
    fields: &[LocalRecordField],
) -> NodeType {
    let mut custom = base_node_type.clone();
    custom.parameters = vec![Parameter {
        id: None,
        name: "record".to_string(),
        data_type: DataType::Record(RecordType::anonymous(
            fields
                .iter()
                .map(|f| (f.name.clone(), f.data_type.clone()))
                .collect(),
        )),
    }];
    custom.output_pins = if fields.is_empty() {
        vec![OutputPinDefinition::fixed("result", DataType::None)]
    } else {
        // A local field is never `Optional`, so its pin type is its type.
        fields
            .iter()
            .map(|field| {
                OutputPinDefinition::fixed(&field.name, field.data_type.clone())
                    .with_id(ANON_FIELD_PIN_ID_BASE | field.id)
            })
            .collect()
    };
    custom
}

/// Build the `NodeType` for a named-mode `record_destructure` node bound to
/// the given schema name. The single input pin is `record:
/// Record(Named(schema))`. The output pins are one per field of the def, in
/// authored order. When the def is missing or has zero fields, the input pin
/// remains `Record(Named(schema))` (dangling and self-evidently broken when
/// missing) and a single placeholder output pin is preserved so the
/// `output_pins.len() >= 1` invariant holds. An empty schema is the unset
/// node ([`build_anonymous_node_type`] with no fields).
pub fn build_node_type_for_schema(
    base_node_type: &NodeType,
    schema: &str,
    registry: &NodeTypeRegistry,
) -> NodeType {
    build_node_type_for_schema_with_defs(base_node_type, schema, registry.record_defs())
}

/// Same as `build_node_type_for_schema`, but takes a [`RecordDefs`] view so
/// the cache populator can call it without conflicting with a concurrent
/// `&mut node_networks` borrow on the registry.
pub fn build_node_type_for_schema_with_defs(
    base_node_type: &NodeType,
    schema: &str,
    defs: RecordDefs<'_>,
) -> NodeType {
    if schema.is_empty() {
        return build_anonymous_node_type(base_node_type, &[]);
    }
    let mut custom = base_node_type.clone();
    custom.parameters = vec![Parameter {
        id: None,
        name: "record".to_string(),
        data_type: DataType::Record(RecordType::Named(schema.to_string())),
    }];
    let resolved = defs.lookup(schema);
    // An `Optional[T]` field projects onto a plain `T` output pin (identity at
    // the value layer — the stored payload or `None` passes straight through).
    // The value/wire layer never sees `Optional`. See
    // `doc/design_optional_type.md` §5.
    let pins: Vec<OutputPinDefinition> = match resolved {
        Some(def) if !def.fields.is_empty() => def
            .fields
            .iter()
            .map(|field| {
                // Stamp the field's stable `FieldId` onto the output pin so
                // `repair_node_network` can remap output wires by identity
                // across a reorder/rename/delete (R3,
                // `doc/design_record_field_identity.md` §4.4).
                OutputPinDefinition::fixed(&field.name, field.data_type.record_field_pin_type())
                    .with_id(field.id.0)
            })
            .collect(),
        _ => vec![OutputPinDefinition::fixed("result", DataType::None)],
    };
    custom.output_pins = pins;
    custom
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_destructure".to_string(),
        description: "Splits a record value into one output per field. With a schema (a record \
            type def), the fields and their order are the def's. Without one, the node takes \
            the type of the record wired into it — when it is placed from a record output, or \
            when a record is wired into it while nothing uses its outputs — and has one output \
            per field of that type. Extra fields on the input record (beyond what the type \
            declares) are ignored (pass-through)."
            .to_string(),
        summary: Some("Read fields of a record".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Base parameters/pins are the unset node's; the registry-aware cache
        // populator replaces them with per-field pins (`build_record_node_type`).
        parameters: vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::anonymous(vec![])),
        }],
        output_pins: vec![OutputPinDefinition::fixed("result", DataType::None)],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordDestructureData::default()),
        node_data_saver: generic_node_data_saver::<RecordDestructureData>,
        node_data_loader: generic_node_data_loader::<RecordDestructureData>,
    }
}
