//! `record_construct`: builds a record value from one input pin per field.
//!
//! Two modes, decided by `schema` (`doc/design_wire_declared_inputs.md`):
//!
//! - **Named** (`schema` set): the fields are a record def's, the pins carry
//!   the def's `FieldId`s, and the output is `Record(Named(schema))`.
//! - **Anonymous** (`schema` empty): the fields are the node's own
//!   ([`RecordConstructData::fields`]), the pins carry
//!   `ANON_FIELD_PIN_ID_BASE | field.id`, and the output is the anonymous
//!   record of those fields. Only this mode has declared inputs (the open
//!   port and *Take type from wire*).

use crate::data_type::{DataType, RecordType, walk_data_type_record_names};
use crate::declared_inputs::DeclaredInputs;
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
use crate::node_type_registry::{NodeTypeRegistry, RecordDefs, is_valid_record_field_type};
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Anonymous field pins carry `ANON_FIELD_PIN_ID_BASE | field.id`. `FieldId`s
/// are counters allocated from a per-file floor and never reach this range, so
/// `Node::set_custom_node_type`'s id-first matching can never pair an
/// anonymous field with an unrelated def field on a mode switch: it falls
/// through to name matching, which is the intended correspondence.
pub const ANON_FIELD_PIN_ID_BASE: u64 = 1 << 62;

/// The property keys of the node in the text format. A field may not take one
/// of these names: `r = record_construct { schema: x }` would be ambiguous.
const RESERVED_NAMES: &[&str] = &["schema", "fields", "literal_values"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordConstructData {
    /// Name of a record type def in the project's registry. Wrapped as
    /// `RecordType::Named(self.schema.clone())` at use time. An empty string
    /// means anonymous mode: the fields are [`Self::fields`].
    #[serde(default)]
    pub schema: String,

    /// Anonymous mode only (`schema` empty): the locally declared fields, in
    /// authored order. Must be empty while `schema` is set (the invariant
    /// checker reports a node that has both).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<LocalRecordField>,

    /// Allocator floor for [`LocalRecordField::id`]; never decreases, so a
    /// deleted field's id is never handed out again.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_field_id: u64,

    /// Per-field stored literal values, consulted in `eval` when the
    /// corresponding input pin is unwired. Keyed by field name, in both modes.
    /// Entries whose key isn't a current field are inert (orphan-tolerant):
    /// `eval` ignores them and the property-panel getter does not surface
    /// them. The edits of this node never leave one behind on purpose: a
    /// schema switch, a field delete and a retype drop the literals they
    /// orphan, so none can reappear on a later field of the same name.
    #[serde(default)]
    pub literal_values: HashMap<String, TextValue>,
}

/// One field of an anonymous `record_construct`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRecordField {
    /// Stable within the node; the pin id is `ANON_FIELD_PIN_ID_BASE | id`.
    pub id: u64,
    pub name: String,
    /// Never `Optional`: the type comes from a pin, and `Optional` is never a
    /// pin type. A promoted def can make the field optional later.
    pub data_type: DataType,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// The identifier rule anonymous record field names follow: `Display` prints
/// them unquoted and the type parser reads them back as identifiers.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl RecordConstructData {
    /// Anonymous mode: no schema, the fields are the node's own.
    pub fn is_anonymous(&self) -> bool {
        self.schema.is_empty()
    }

    /// The type gate of an anonymous field: any type a record def field may
    /// hold ([`is_valid_record_field_type`]), except the ones a declared field
    /// can never have — `None`, `Unit`, `AnyFunction` (not a value type) and
    /// `Optional` (never a pin type).
    pub fn accepts_field_type(data_type: &DataType) -> Result<(), String> {
        match data_type {
            DataType::None => Err("A record field cannot have type None".to_string()),
            DataType::Unit => Err("A record field cannot have type Unit".to_string()),
            DataType::AnyFunction { .. } => {
                Err("A record field needs a concrete function type".to_string())
            }
            DataType::Optional(_) => {
                Err("An anonymous record field cannot be Optional".to_string())
            }
            _ => is_valid_record_field_type(data_type),
        }
    }

    /// A fresh local field id: at or above `next_field_id` and above every
    /// live id.
    fn allocate_field_id(&mut self) -> u64 {
        let floor = self.fields.iter().map(|f| f.id + 1).max().unwrap_or(0);
        let id = self.next_field_id.max(floor);
        self.next_field_id = id + 1;
        id
    }

    /// The local field a pin id names, if any.
    fn field_index_for_pin(&self, pin_id: u64) -> Option<usize> {
        if pin_id & ANON_FIELD_PIN_ID_BASE == 0 {
            return None;
        }
        let id = pin_id & !ANON_FIELD_PIN_ID_BASE;
        self.fields.iter().position(|f| f.id == id)
    }

    /// Replace the anonymous field list with `new_fields` (names and types in
    /// authored order): the one edit behind the panel's rename / retype /
    /// delete / reorder / add and the text format's `fields`.
    ///
    /// Ids are kept by **name first, then position** (`set_expr_data`'s rule),
    /// so a reorder keeps every field's id and a rename in place keeps its
    /// position's. A position is only inherited from an old field whose name
    /// is gone, so a reorder never hands one field's id to another. New fields
    /// take fresh ids from `next_field_id`. A field that keeps its id and its
    /// type keeps its literal (under its new name); every other literal is
    /// dropped.
    pub fn set_local_fields(&mut self, new_fields: Vec<(String, DataType)>) -> Result<(), String> {
        if !self.is_anonymous() {
            return Err(format!(
                "The fields of record '{}' are edited in its def",
                self.schema
            ));
        }
        let mut seen = HashSet::new();
        for (name, data_type) in &new_fields {
            if !is_identifier(name) {
                return Err(format!(
                    "Field name '{}' must be a letter or '_' followed by letters, digits or '_'",
                    name
                ));
            }
            if RESERVED_NAMES.contains(&name.as_str()) {
                return Err(format!("Field name '{}' is reserved", name));
            }
            if !seen.insert(name.as_str()) {
                return Err(format!("Duplicate field name '{}'", name));
            }
            Self::accepts_field_type(data_type)
                .map_err(|reason| format!("Field '{}': {}", name, reason))?;
        }

        let old_fields = std::mem::take(&mut self.fields);
        let mut old_literals = std::mem::take(&mut self.literal_values);
        let mut used: HashSet<u64> = HashSet::new();
        let mut kept: Vec<Option<&LocalRecordField>> = Vec::with_capacity(new_fields.len());
        for (index, (name, _)) in new_fields.iter().enumerate() {
            let by_name = old_fields.iter().find(|f| &f.name == name);
            let by_position = old_fields
                .get(index)
                .filter(|old| !seen.contains(old.name.as_str()));
            let old = by_name
                .or(by_position)
                .filter(|old| !used.contains(&old.id));
            if let Some(old) = old {
                used.insert(old.id);
            }
            kept.push(old);
        }

        // Ids of the kept fields stay above the floor already; fresh ones are
        // allocated against the kept set.
        self.fields = kept
            .iter()
            .zip(&new_fields)
            .filter_map(|(old, (name, data_type))| {
                old.map(|old| LocalRecordField {
                    id: old.id,
                    name: name.clone(),
                    data_type: data_type.clone(),
                })
            })
            .collect();
        let mut result = Vec::with_capacity(new_fields.len());
        for (old, (name, data_type)) in kept.into_iter().zip(new_fields) {
            let id = match old {
                Some(old) => {
                    if old.data_type == data_type
                        && let Some(literal) = old_literals.remove(&old.name)
                    {
                        self.literal_values.insert(name.clone(), literal);
                    }
                    old.id
                }
                None => self.allocate_field_id(),
            };
            result.push(LocalRecordField {
                id,
                name,
                data_type,
            });
        }
        self.fields = result;
        Ok(())
    }

    /// Point the node at `schema` (empty = anonymous mode). Local fields are
    /// cleared, and only the literals named in `kept_literal_names` (the new
    /// def's field names) survive, so none is left behind to reappear on a
    /// later field of the same name. Setting the current schema is a no-op.
    /// Data only: the caller re-derives the pins, which keeps the wires whose
    /// field has a same-named counterpart and drops the rest.
    pub fn set_schema(&mut self, schema: String, kept_literal_names: &HashSet<String>) {
        if schema == self.schema {
            return;
        }
        self.schema = schema;
        self.fields.clear();
        self.literal_values
            .retain(|name, _| kept_literal_names.contains(name));
    }

    /// `(name, pin type, is optional)` per field, in pin order: the def's
    /// fields in named mode (`None` when the def is missing), the local ones
    /// in anonymous mode.
    fn field_slots(&self, defs: RecordDefs<'_>) -> Option<Vec<(String, DataType, bool)>> {
        if self.is_anonymous() {
            return Some(
                self.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.data_type.clone(), false))
                    .collect(),
            );
        }
        let def = defs.lookup(&self.schema)?;
        // An `Optional[T]` field is exposed as a plain `T` pin (the value /
        // wire layer never sees `Optional`), so literals coerce against the
        // inner `T`, not `Optional[T]`. See `doc/design_optional_type.md` §5.
        Some(
            def.fields
                .iter()
                .map(|f| {
                    (
                        f.name.clone(),
                        f.data_type.record_field_pin_type(),
                        f.data_type.is_optional(),
                    )
                })
                .collect(),
        )
    }
}

impl DeclaredInputs for RecordConstructData {
    fn accepts_type(&self, data_type: &DataType) -> Result<(), String> {
        Self::accepts_field_type(data_type)
    }

    fn reserved_names(&self) -> &'static [&'static str] {
        RESERVED_NAMES
    }

    fn fallback_name(&self) -> &'static str {
        "field"
    }

    fn declaring_property(&self) -> &'static str {
        "fields"
    }

    fn declares_pin(&self, pin_id: u64) -> bool {
        self.field_index_for_pin(pin_id).is_some()
    }

    fn append(&mut self, name: String, data_type: DataType) -> Result<u64, String> {
        Self::accepts_field_type(&data_type)?;
        let id = self.allocate_field_id();
        self.fields.push(LocalRecordField {
            id,
            name,
            data_type,
        });
        Ok(ANON_FIELD_PIN_ID_BASE | id)
    }

    /// Also drops the field's literal: a retype happens only on a wired field,
    /// whose literal is not in use, and only to a type the literal was not
    /// written for. Undo brings it back.
    fn retype(&mut self, pin_id: u64, data_type: DataType) -> Result<(), String> {
        Self::accepts_field_type(&data_type)?;
        let index = self
            .field_index_for_pin(pin_id)
            .ok_or_else(|| format!("record_construct has no field with pin id {}", pin_id))?;
        let field = &mut self.fields[index];
        field.data_type = data_type;
        self.literal_values.remove(&field.name);
        Ok(())
    }
}

impl NodeData for RecordConstructData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// `record_construct`'s parameters and output pin depend on the registry,
    /// not on data alone; they are built by `build_record_node_type`.
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
        let Some(slots) = self.field_slots(registry.record_defs()) else {
            // Dangling schema. The output type fails subtyping against any
            // consumer — produce None so downstream destructure / consumer
            // nodes don't see a partial value.
            return EvalOutput::single(NetworkResult::None);
        };

        // Resolve `node` for the wire-state check below. Should always succeed
        // — we are mid-eval on this node — but bail defensively.
        let Some(node) = network_stack
            .last()
            .and_then(|frame| frame.node_network.nodes.get(&node_id))
        else {
            return EvalOutput::single(NetworkResult::None);
        };

        // Walk the fields in pin order (the def's authored order, or the local
        // fields' order); the parameter pin layout matches it.
        let mut fields: Vec<(String, NetworkResult)> = Vec::with_capacity(slots.len());
        for (param_index, (field_name, pin_type, is_optional)) in slots.into_iter().enumerate() {
            // Wired > literal > pass-None-through. A wired pin always wins;
            // an unwired pin with a stored literal uses that literal (if it
            // still coerces to the field type); otherwise we fall through to
            // `evaluate_arg`, which on an unwired pin yields None — which then
            // collapses the record for a required field, or is kept as an
            // explicit `None` for an `Optional` field (below).
            let value = network_evaluator
                .wired_or_literal(
                    network_stack,
                    node_id,
                    registry,
                    context,
                    node,
                    param_index,
                    self.literal_values.get(&field_name),
                    &pin_type,
                )
                .unwrap_or_else(|| {
                    network_evaluator.evaluate_arg(
                        network_stack,
                        node_id,
                        registry,
                        context,
                        param_index,
                    )
                });
            match &value {
                // A `None` on a required field still collapses the whole record
                // to `None` (pre-Optional all-or-nothing behavior). A `None` on
                // an `Optional[T]` field is **kept** as an explicit `None` in
                // that slot — the record is emitted with all fields, "unset"
                // being an explicit `None` value. `Error` short-circuits either
                // way. See `doc/design_optional_type.md` §5.
                NetworkResult::None if !is_optional => {
                    return EvalOutput::single(NetworkResult::None);
                }
                NetworkResult::Error(_) => return EvalOutput::single(value),
                _ => {}
            }
            fields.push((field_name, value));
        }

        // `record(..)` re-sorts into canonical order.
        EvalOutput::single(NetworkResult::record(fields))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        if self.schema.is_empty() {
            None
        } else {
            Some(self.schema.clone())
        }
    }

    /// `fields` is written whenever the node is anonymous, even empty, so a
    /// fresh node lists it (the editor and `describe` read a fresh node's
    /// properties to learn its keys; `text_format/AGENTS.md`). A named-mode
    /// node writes `schema` alone, as it always did.
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

    /// `fields` is a key in both modes (a named-mode node takes `fields: []`).
    fn text_literal_keys_with_defs(&self, _defs: RecordDefs<'_>) -> Vec<String> {
        vec!["fields".to_string()]
    }

    /// `schema` first, then `fields`, so a statement can switch a node to
    /// anonymous mode and declare its fields at once. Setting a schema clears
    /// the local fields; `fields` is refused on a named-mode node.
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
            let mut new_fields = Vec::with_capacity(entries.len());
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
                new_fields.push((name, data_type));
            }
            if self.is_anonymous() {
                self.set_local_fields(new_fields)?;
            } else if !new_fields.is_empty() {
                return Err(
                    "fields can only be declared on an anonymous record (empty schema)".to_string(),
                );
            }
        }
        Ok(())
    }

    /// `record_construct`'s output pin is the record value, so it can satisfy
    /// a consumer that wants a record (`FromInput` — the user dragged off an
    /// input pin expecting something that *produces* the record). A named
    /// record adopts the schema name; an anonymous one becomes an anonymous
    /// node with those fields (in the type's canonical order). `FromOutput` is
    /// not adapted: the new node starts with zero fields, and the drop's
    /// auto-connect targets its open port. See
    /// `doc/design_drag_aware_add_node.md`, issue #312 and
    /// `doc/design_wire_declared_inputs.md`.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        if direction != DragDirection::FromInput {
            return None;
        }
        match source_type {
            DataType::Record(RecordType::Named(name)) => {
                // Don't pre-set a dangling schema — require the def to resolve.
                registry.lookup_record_type_def(name)?;
                Some(Box::new(RecordConstructData {
                    schema: name.clone(),
                    ..Default::default()
                }))
            }
            DataType::Record(RecordType::Anonymous(source_fields)) => {
                let mut data = RecordConstructData::default();
                for (name, data_type) in source_fields {
                    if !is_identifier(name) || RESERVED_NAMES.contains(&name.as_str()) {
                        return None;
                    }
                    data.append(name.clone(), data_type.clone()).ok()?;
                }
                Some(Box::new(data))
            }
            _ => None,
        }
    }

    fn declared_inputs(&self) -> Option<&dyn DeclaredInputs> {
        self.is_anonymous().then_some(self as &dyn DeclaredInputs)
    }

    fn declared_inputs_mut(&mut self) -> Option<&mut dyn DeclaredInputs> {
        if self.is_anonymous() {
            Some(self)
        } else {
            None
        }
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
        if self.is_anonymous() {
            // Needs no defs, so it never refuses.
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

    /// One warning per local field type that names a record def which does
    /// not exist (a hand-edited file): the field's pin is typed by a dangling
    /// name, and this is what tells the user so. A name under a library mount
    /// freezes the node instead and never gets here.
    fn record_node_problems(&self, defs: RecordDefs<'_>) -> Vec<String> {
        let mut problems = Vec::new();
        for field in &self.fields {
            walk_data_type_record_names(&field.data_type, &mut |name| {
                if defs.lookup(name).is_none() {
                    problems.push(format!(
                        "Field '{}' has unknown record type '{}'",
                        field.name, name
                    ));
                }
            });
        }
        problems
    }
}

/// The `NodeType` of an anonymous `record_construct`: one pin per local field
/// in authored order, each carrying `ANON_FIELD_PIN_ID_BASE | id`, and the
/// anonymous record of the fields as output (`{}` with none).
pub fn build_anonymous_node_type(
    base_node_type: &NodeType,
    fields: &[LocalRecordField],
) -> NodeType {
    let mut custom = base_node_type.clone();
    custom.parameters = fields
        .iter()
        .map(|field| Parameter {
            id: Some(ANON_FIELD_PIN_ID_BASE | field.id),
            name: field.name.clone(),
            data_type: field.data_type.clone(),
        })
        .collect();
    custom.output_pins =
        OutputPinDefinition::single_fixed(DataType::Record(RecordType::anonymous(
            fields
                .iter()
                .map(|f| (f.name.clone(), f.data_type.clone()))
                .collect(),
        )));
    custom
}

/// Build the `NodeType` for a named-mode `record_construct` node bound to the
/// given schema name. Returns the bare base type (with no parameters and a
/// dangling-named output) when the schema is missing from the registry. Wires
/// out of it are kept: `validate_wires` reports each one whose consumer the
/// dangling output no longer satisfies, and repair does not drop them.
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
    let mut custom = base_node_type.clone();
    custom.output_pins =
        OutputPinDefinition::single_fixed(DataType::Record(RecordType::Named(schema.to_string())));
    if let Some(def) = defs.lookup(schema) {
        // Pin order = authored field order. An `Optional[T]` field is exposed
        // as a plain `T` input pin (the value/wire layer never sees `Optional`;
        // the optional behavior lives in `eval`'s collapse exemption). See
        // `doc/design_optional_type.md` §5.
        // R2 (the load-bearing change of `doc/design_record_field_identity.md`):
        // stamp the field's stable `FieldId` onto the pin so
        // `set_custom_node_type`'s id-first matching preserves the wire across a
        // rename / reorder — at top level and inside every HOF body.
        custom.parameters = def
            .fields
            .iter()
            .map(|field| Parameter {
                id: Some(field.id.0),
                name: field.name.clone(),
                data_type: field.data_type.record_field_pin_type(),
            })
            .collect();
    } else {
        custom.parameters = Vec::new();
    }
    custom
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_construct".to_string(),
        description:
            "Builds a record value from one input per field. With a schema (a record type def), \
            the fields and their order are the def's. Without one, the node is an anonymous \
            record whose fields it declares itself — add one by dropping a wire on the node — \
            and its output is the anonymous record type of those fields. The runtime value is \
            stored in canonical (sorted) order."
                .to_string(),
        summary: Some("Construct a record value".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Base parameters/pins are placeholders; the registry-aware cache
        // populator replaces them with per-field pins (`build_record_node_type`).
        parameters: vec![],
        output_pins: OutputPinDefinition::single_fixed(DataType::Record(RecordType::anonymous(
            vec![],
        ))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordConstructData::default()),
        node_data_saver: generic_node_data_saver::<RecordConstructData>,
        node_data_loader: generic_node_data_loader::<RecordConstructData>,
    }
}
