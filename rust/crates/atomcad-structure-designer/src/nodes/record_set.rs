//! `record_set` — update record fields by path, at any depth
//! (`doc/design_record_get_set.md`).
//!
//! One `record` input (pin 0, so the `-1` function pin wired into `map.f`
//! updates every record of a stream); one input pin per user-chosen **path**
//! into the node's root record type, each assigning that field; one output,
//! the updated record. Every field no path names passes through untouched,
//! including extra runtime fields beyond the root. Update only: the output
//! type is the root.
//!
//! The root follows the node's mode, exactly as on `record_get`
//! (`doc/design_anonymous_record_readers.md`): the `schema` def, the
//! anonymous type taken from the wire (`fields`), or `{}` when unset.
//!
//! Rows share their identity and dangling-row rules with `record_get`
//! ([`crate::nodes::record_path_rows`]); a row additionally stores a literal,
//! used when its pin is unwired. A row that is unwired and has no usable
//! literal leaves its field unchanged.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{DragDirection, EvalOutput, MissingDefs, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::{
    NodeType, NodeTypeCategory, OutputPinDefinition, Parameter, generic_node_data_saver,
};
use crate::node_type_registry::{NodeTypeRegistry, RecordDefs};
use crate::nodes::comment::ANCHOR_PROPERTY;
use crate::nodes::local_record_fields::{
    self, LocalRecordField, adopted_reader_fields, is_zero, reader_fields_from_text,
};
use crate::nodes::record_path_rows::{
    RecordPathRow, dangling_row_problems, heal_rows, is_unset_root, literal_fits, merge_rows,
    parse_paths, paths_text_value, paths_text_value_stored, reader_root, row_pins, rows_path_defs,
    text_pin_names,
};
use crate::record_path::{PathSpec, format_path};
use crate::record_typed_input::RecordTypedInput;
use crate::structure_designer::StructureDesigner;
use crate::text_format::{PIN_ROLES_PROPERTY, TextValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;

/// Names a row pin may not take: `record_set`'s row pins are inputs, so they
/// share the text format's `{ … }` block with the fixed `record` pin, the
/// node's own properties and the network-level special properties. A derived
/// name equal to one of these gets a `__2` suffix.
pub const RESERVED_PIN_NAMES: [&str; 7] = [
    "record",
    "schema",
    "fields",
    "paths",
    "visible",
    ANCHOR_PROPERTY,
    PIN_ROLES_PROPERTY,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordSetRow {
    #[serde(flatten)]
    pub row: RecordPathRow,
    /// Stored literal, consulted when the pin is unwired. Lives on the row,
    /// **not** in a name-keyed map: name-keyed literals are the casualty
    /// `doc/design_record_field_identity.md` §4.5 had to repair on
    /// `record_construct`.
    #[serde(default)]
    pub literal: Option<TextValue>,
}

impl AsRef<RecordPathRow> for RecordSetRow {
    fn as_ref(&self) -> &RecordPathRow {
        &self.row
    }
}

impl AsMut<RecordPathRow> for RecordSetRow {
    fn as_mut(&mut self) -> &mut RecordPathRow {
        &mut self.row
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RecordSetData {
    /// Name of the record def the paths start at, and the output type.
    /// Empty = anonymous mode when [`Self::fields`] is non-empty, unset
    /// otherwise.
    #[serde(default)]
    pub schema: String,
    /// Anonymous mode only (`schema` empty): the fields of the root record
    /// type, adopted from a wire. Must be empty while `schema` is set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<LocalRecordField>,
    /// Allocator floor for [`LocalRecordField::id`]; never decreases.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_field_id: u64,
    #[serde(default)]
    pub rows: Vec<RecordSetRow>,
    /// Monotonic id source for `rows`; never max(existing)+1
    /// (`doc/design_parameter_wire_stability.md`).
    #[serde(default)]
    pub next_row_id: u64,
}

impl RecordSetData {
    /// A named-mode node updating the def `schema`, with no rows.
    pub fn named(schema: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            ..Default::default()
        }
    }

    /// Anonymous mode: no schema, and the root's fields are the node's own.
    pub fn is_anonymous(&self) -> bool {
        self.schema.is_empty() && !self.fields.is_empty()
    }

    /// The record type the paths start at and the node outputs: the def, the
    /// adopted anonymous type, or `{}` when unset.
    pub fn root(&self) -> RecordType {
        reader_root(&self.schema, &self.fields)
    }

    /// Sets the schema and edits the rows from by-name specs
    /// ([`merge_rows`]): ids are kept so wires follow their rows, and a
    /// schema change re-resolves the rows by their current names. A row keeps
    /// the literal of the old row whose id it inherited, if the literal still
    /// converts to the row's pin type; a dangling row keeps everything. The
    /// current schema (`""` on an anonymous node) keeps the root, local
    /// fields included; any other clears the fields. `specs: None` keeps the
    /// rows. On error nothing changes.
    pub fn edit_rows(
        &mut self,
        schema: String,
        specs: Option<&[PathSpec]>,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let fields = if schema == self.schema {
            self.fields.clone()
        } else {
            Vec::new()
        };
        self.retarget(schema, fields, self.next_field_id, specs, defs)
    }

    /// Moves the node onto the root `schema` / `fields` and edits its rows
    /// ([`merge_rows`]), carrying literals as [`Self::edit_rows`] describes.
    /// All or nothing.
    fn retarget(
        &mut self,
        schema: String,
        fields: Vec<LocalRecordField>,
        next_field_id: u64,
        specs: Option<&[PathSpec]>,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let root = reader_root(&schema, &fields);
        let mut next_row_id = self.next_row_id;
        let merged = merge_rows(
            &self.root(),
            &self.rows,
            &root,
            specs,
            defs,
            &mut next_row_id,
        )?;
        let rows = merged
            .rows
            .into_iter()
            .zip(merged.sources)
            .map(|(row, source)| {
                let literal = source
                    .and_then(|oi| self.rows[oi].literal.clone())
                    .filter(|literal| literal_fits(&root, &row, literal, defs));
                RecordSetRow { row, literal }
            })
            .collect();
        self.schema = schema;
        self.fields = fields;
        self.next_field_id = next_field_id;
        self.rows = rows;
        self.next_row_id = next_row_id;
        Ok(())
    }

    /// `schema` always, and `fields` in anonymous mode.
    fn root_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![("schema".to_string(), TextValue::String(self.schema.clone()))];
        if self.is_anonymous() {
            props.push((
                "fields".to_string(),
                local_record_fields::fields_text_value(&self.fields),
            ));
        }
        props
    }

    /// The pin names the text format writes the rows' literals under
    /// ([`text_pin_names`]), `None` for a dangling row.
    fn literal_keys(&self, defs: RecordDefs<'_>) -> Vec<Option<String>> {
        text_pin_names(&self.root(), &self.rows, defs, &RESERVED_PIN_NAMES)
    }

    /// Stores the literals in `props` whose keys name a row pin. A literal
    /// that does not convert to its row's pin type is an error; keys that
    /// name no row are left to the caller (the text editor warns).
    fn set_literals(
        &mut self,
        props: &HashMap<String, TextValue>,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let keys = self.literal_keys(defs);
        let root = self.root();
        for (index, key) in keys.iter().enumerate() {
            let Some((key, literal)) = key
                .as_ref()
                .and_then(|key| props.get(key).map(|literal| (key, literal)))
            else {
                continue;
            };
            let row = &mut self.rows[index];
            if !literal_fits(&root, &row.row, literal, defs) {
                return Err(format!(
                    "`{}`: {} does not fit the field's type",
                    key,
                    literal.to_text()
                ));
            }
            row.literal = Some(literal.clone());
        }
        Ok(())
    }

    /// Restores the invariants the setters keep after a raw deserialize
    /// ([`heal_rows`]). Idempotent.
    pub fn heal(&mut self) {
        heal_rows(&mut self.rows, &mut self.next_row_id);
    }
}

impl RecordTypedInput for RecordSetData {
    fn record_input_type(&self) -> Option<RecordType> {
        let root = self.root();
        (!is_unset_root(&root)).then_some(root)
    }

    /// The rows move onto the new root by their current names; a row whose
    /// names do not resolve there goes dangling, and keeps its literal.
    fn set_record_input_type(
        &mut self,
        t: &RecordType,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let (schema, fields, next_field_id) =
            adopted_reader_fields(t, &self.fields, self.next_field_id)?;
        self.retarget(schema, fields, next_field_id, None, defs)
    }
}

impl NodeData for RecordSetData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    /// The pins come from the defs, through `build_record_node_type`.
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
        let root = self.root();
        let named_root_missing =
            !self.schema.is_empty() && registry.lookup_record_type_def(&self.schema).is_none();
        if named_root_missing || is_unset_root(&root) {
            return EvalOutput::single(NetworkResult::None);
        }
        // The same pins the layout was built with: row pin `k` is input `k + 1`.
        let pins = row_pins(
            &root,
            &self.rows,
            registry.record_defs(),
            &RESERVED_PIN_NAMES,
            MissingDefs::Dangle,
        )
        .unwrap_or_default();

        let mut record =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        if matches!(record, NetworkResult::None | NetworkResult::Error(_)) {
            return EvalOutput::single(record);
        }
        if pins.is_empty() {
            return EvalOutput::single(record);
        }
        let Some(node) = network_stack
            .last()
            .and_then(|frame| frame.node_network.nodes.get(&node_id))
        else {
            return EvalOutput::single(NetworkResult::None);
        };

        // Rows apply top to bottom, so `p` then `p.x` adjusts the new `p`.
        for (k, pin) in pins.iter().enumerate() {
            let row = &self.rows[pin.row_index];
            // Wired > literal > leave the field unchanged.
            let Some(value) = network_evaluator.wired_or_literal(
                network_stack,
                node_id,
                registry,
                context,
                node,
                k + 1,
                row.literal.as_ref(),
                &pin.pin_type(),
            ) else {
                continue;
            };
            match &value {
                NetworkResult::Error(_) => return EvalOutput::single(value),
                // On an `Optional` leaf a `None` is stored: that is how a
                // field is unset. On a required leaf the record collapses,
                // `record_construct`'s all-or-nothing rule.
                NetworkResult::None if !pin.leaf_type.is_optional() => {
                    return EvalOutput::single(NetworkResult::None);
                }
                _ => {}
            }
            let path: Vec<&str> = pin.names.iter().map(String::as_str).collect();
            record = match record.with_record_path_replaced(&path, value) {
                Ok(updated) => updated,
                // Only a malformed value gets here: the static type
                // guarantees every record on the path.
                Err(depth) => {
                    return EvalOutput::single(NetworkResult::Error(format!(
                        "record_set: cannot set `{}`: {} is not a record in the input value",
                        format_path(&pin.names),
                        if depth == 0 {
                            "the input".to_string()
                        } else {
                            format!("`{}`", format_path(&pin.names[..depth]))
                        }
                    )));
                }
            };
        }
        EvalOutput::single(record)
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

    /// Without the defs the current field names are unknown: the stored ones.
    /// The text format uses [`Self::get_text_properties_with_defs`].
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = self.root_text_properties();
        props.push(("paths".to_string(), paths_text_value_stored(&self.rows)));
        props
    }

    /// `schema`, `fields` in anonymous mode, `paths`, then each stored literal
    /// under its row's pin name (`p_x: 3.0`), so it reads like any pin
    /// literal. The serializer leaves out a literal whose pin is wired.
    fn get_text_properties_with_defs(&self, defs: RecordDefs<'_>) -> Vec<(String, TextValue)> {
        let mut props = self.root_text_properties();
        props.push((
            "paths".to_string(),
            paths_text_value(&self.root(), &self.rows, defs),
        ));
        for (row, key) in self.rows.iter().zip(self.literal_keys(defs)) {
            if let (Some(key), Some(literal)) = (key, &row.literal) {
                props.push((key, literal.clone()));
            }
        }
        props
    }

    /// Every row pin takes a literal, stored or not; `fields` is a key in
    /// every mode, so a fresh (unset) node takes it.
    fn text_literal_keys_with_defs(&self, defs: RecordDefs<'_>) -> Vec<String> {
        std::iter::once("fields".to_string())
            .chain(self.literal_keys(defs).into_iter().flatten())
            .collect()
    }

    /// Paths need the defs to resolve; without them only `schema` and
    /// `fields` can be set.
    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if props.contains_key("paths") {
            return Err("paths can only be set with the record defs at hand".to_string());
        }
        let (schema, fields, next_field_id) = reader_fields_from_text(
            props,
            &self.schema,
            &self.fields,
            self.next_field_id,
            "record_set",
        )?;
        self.schema = schema;
        self.fields = fields;
        self.next_field_id = next_field_id;
        Ok(())
    }

    /// `schema`, then `fields`, then `paths` resolved against the resulting
    /// root ([`Self::edit_rows`]), and only then the literal keys, against the
    /// **new** pin names: one block can change all four. All or nothing.
    fn set_text_properties_with_defs(
        &mut self,
        props: &HashMap<String, TextValue>,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let (schema, fields, next_field_id) = reader_fields_from_text(
            props,
            &self.schema,
            &self.fields,
            self.next_field_id,
            "record_set",
        )?;
        let specs = props.get("paths").map(parse_paths).transpose()?;
        let mut edited = self.clone();
        edited.retarget(schema, fields, next_field_id, specs.as_deref(), defs)?;
        edited.set_literals(props, defs)?;
        *self = edited;
        Ok(())
    }

    /// The `record` input and the output are both the record, so a record
    /// dragged from either side can connect: the node adopts its type exactly
    /// as a wire would — a named record's def, an anonymous record's fields.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        let DataType::Record(record_type) = source_type else {
            return None;
        };
        if let RecordType::Named(name) = record_type {
            registry.lookup_record_type_def(name)?;
        }
        let mut data = RecordSetData::default();
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

    fn local_record_fields(&self) -> Option<(&[LocalRecordField], u64)> {
        Some((&self.fields, self.next_field_id))
    }

    fn local_record_fields_mut(&mut self) -> Option<&mut [LocalRecordField]> {
        Some(&mut self.fields)
    }

    fn record_schema_ref(&self) -> Option<&str> {
        Some(&self.schema)
    }

    fn set_record_schema_ref(&mut self, name: String) {
        self.schema = name;
    }

    fn build_record_node_type(
        &self,
        base: &NodeType,
        defs: RecordDefs<'_>,
        missing: MissingDefs,
    ) -> Option<NodeType> {
        let root = self.root();
        let pins = row_pins(&root, &self.rows, defs, &RESERVED_PIN_NAMES, missing)?;
        let record_type = DataType::Record(root);
        let mut custom = base.clone();
        custom.parameters = std::iter::once(Parameter {
            id: None,
            name: "record".to_string(),
            data_type: record_type.clone(),
        })
        .chain(pins.iter().map(|pin| Parameter {
            // The row id, so `set_custom_node_type` keeps each row's wire
            // when other rows dangle or are removed.
            id: pin.id,
            name: pin.name.clone(),
            data_type: pin.pin_type(),
        }))
        .collect();
        custom.output_pins = OutputPinDefinition::single_fixed(record_type);
        Some(custom)
    }

    fn record_node_problems(&self, defs: RecordDefs<'_>) -> Vec<String> {
        let mut problems = local_record_fields::local_field_problems(&self.fields, defs);
        problems.extend(dangling_row_problems(&self.root(), &self.rows, defs));
        problems
    }

    fn record_path_defs(&self, defs: RecordDefs<'_>) -> Vec<String> {
        rows_path_defs(&self.root(), &self.rows, defs)
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_set".to_string(),
        description: "Updates chosen fields of a record by path, at any depth: one input pin per \
            path (`p.x` sets field `x` of the nested record in field `p`), and the updated \
            record as output. Every other field passes through unchanged. An unwired path pin \
            uses its stored value, or leaves the field as it is. Set nested field values \
            without destructuring and rebuilding every level. The paths start at a schema (a \
            record type def) or, without one, at the type of the record wired into it — taken \
            when the node is placed from a record pin, or when a record is wired into it while \
            nothing depends on it."
            .to_string(),
        summary: Some("Set record fields by (nested) path".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // The unset node's; the registry-aware cache populator replaces them
        // with `record` plus one pin per row through `build_record_node_type`.
        parameters: vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::anonymous(vec![])),
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::Record(RecordType::anonymous(
            vec![],
        ))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordSetData::default()),
        node_data_saver: generic_node_data_saver::<RecordSetData>,
        node_data_loader: record_set_node_data_loader,
    }
}

/// Loader that heals persisted row state (missing / zero `next_row_id`,
/// id-less rows, duplicate or empty paths) after the generic deserialize.
fn record_set_node_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: RecordSetData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.heal();
    Ok(Box::new(data))
}
