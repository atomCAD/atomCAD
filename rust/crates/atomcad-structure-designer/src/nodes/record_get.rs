//! `record_get` — read record fields by path, at any depth
//! (`doc/design_record_get_set.md`).
//!
//! One `record` input; one output pin per user-chosen **path** into the
//! node's root record type (`p.x`, `size`, …). Rows are stored by `FieldId` where the
//! record type is a named def ([`crate::record_path`]), so a field rename
//! anywhere along the path renames the pin and keeps its wire. A row whose
//! path stops resolving is dangling: it keeps its place in the data, loses its
//! pin, and is reported as a non-blocking warning.
//!
//! The root is decided by the node's mode, as on `record_destructure`
//! (`doc/design_anonymous_record_readers.md`): **named** (`schema` set — the
//! def), **anonymous** (`schema` empty, `fields` non-empty — the type the node
//! took from its wire; rows below it resolve by name down to the first named
//! def) or **unset** (both empty — the input is `{}` and no path can start).
//! The type is adopted through [`RecordTypedInput`], like the destructure's.
//!
//! A record node in the `NodeData` hook sense: its pins come from the defs
//! (`build_record_node_type`), never from `calculate_custom_node_type`.

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
use crate::nodes::local_record_fields::{
    self, LocalRecordField, adopted_reader_fields, is_zero, reader_fields_from_text,
};
use crate::nodes::record_path_rows::{
    RecordPathRow, dangling_row_problems, heal_rows, is_unset_root, merge_rows, parse_paths,
    paths_text_value, paths_text_value_stored, reader_root, row_pins, rows_path_defs,
};
use crate::record_path::PathSpec;
use crate::record_typed_input::RecordTypedInput;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RecordGetData {
    /// Name of the record def the paths start at. Empty = anonymous mode when
    /// [`Self::fields`] is non-empty, unset otherwise.
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
    pub rows: Vec<RecordPathRow>,
    /// Monotonic id source for `rows`; never max(existing)+1
    /// (`doc/design_parameter_wire_stability.md`).
    #[serde(default)]
    pub next_row_id: u64,
}

impl RecordGetData {
    /// A named-mode node reading the def `schema`, with no rows.
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

    /// The record type the paths start at: the def, the adopted anonymous
    /// type, or `{}` when unset.
    pub fn root(&self) -> RecordType {
        reader_root(&self.schema, &self.fields)
    }

    /// Sets the schema and edits the rows from by-name specs
    /// ([`merge_rows`]): ids are kept so wires follow their rows, and a
    /// schema change re-resolves the rows by their current names. The
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
    /// ([`merge_rows`]). All or nothing.
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
        self.schema = schema;
        self.fields = fields;
        self.next_field_id = next_field_id;
        self.rows = merged.rows;
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

    /// Restores the invariants the setters keep after a raw deserialize
    /// ([`heal_rows`]). Idempotent.
    pub fn heal(&mut self) {
        heal_rows(&mut self.rows, &mut self.next_row_id);
    }
}

impl RecordTypedInput for RecordGetData {
    fn record_input_type(&self) -> Option<RecordType> {
        let root = self.root();
        (!is_unset_root(&root)).then_some(root)
    }

    /// The rows move onto the new root by their current names; a row whose
    /// names do not resolve there goes dangling.
    fn set_record_input_type(
        &mut self,
        t: &RecordType,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let (schema, fields, next_field_id) =
            adopted_reader_fields(t, &self.fields, self.next_field_id)?;
        self.retarget(schema, fields, next_field_id, None, defs)
    }

    /// Output pins carry row ids, which do not depend on the root.
    fn output_pin_ids_survive_type_change(&self) -> bool {
        true
    }
}

impl NodeData for RecordGetData {
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
        // The same pins the layout was built with, so values land on them.
        let pins = row_pins(
            &self.root(),
            &self.rows,
            registry.record_defs(),
            &[],
            MissingDefs::Dangle,
        )
        .unwrap_or_default();
        let named_root_missing =
            !self.schema.is_empty() && registry.lookup_record_type_def(&self.schema).is_none();
        if named_root_missing || pins.is_empty() {
            // The placeholder pin.
            return EvalOutput::single(NetworkResult::None);
        }

        let record_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        match &record_val {
            NetworkResult::None => return EvalOutput::multi(vec![NetworkResult::None; pins.len()]),
            NetworkResult::Error(_) => return EvalOutput::multi(vec![record_val; pins.len()]),
            _ => {}
        }

        // Records pass through without projection, so the value may carry
        // extra fields; they are ignored. A field missing at runtime, or a
        // `None` part-way down, reads as `None`.
        EvalOutput::multi(
            pins.iter()
                .map(|pin| {
                    let path: Vec<&str> = pin.names.iter().map(String::as_str).collect();
                    record_val
                        .extract_record_path(&path)
                        .cloned()
                        .unwrap_or(NetworkResult::None)
                })
                .collect(),
        )
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

    /// `schema`, `fields` in anonymous mode (as on `record_destructure`), then
    /// `paths`. An unset node writes `schema: ""`, which keeps its statement
    /// from adopting the wire's type on the way back in.
    fn get_text_properties_with_defs(&self, defs: RecordDefs<'_>) -> Vec<(String, TextValue)> {
        let mut props = self.root_text_properties();
        props.push((
            "paths".to_string(),
            paths_text_value(&self.root(), &self.rows, defs),
        ));
        props
    }

    /// `fields` is a key in every mode, so a fresh (unset) node takes it.
    fn text_literal_keys_with_defs(&self, _defs: RecordDefs<'_>) -> Vec<String> {
        vec!["fields".to_string()]
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
            "record_get",
        )?;
        self.schema = schema;
        self.fields = fields;
        self.next_field_id = next_field_id;
        Ok(())
    }

    /// `schema`, then `fields`, then `paths` resolved against the resulting
    /// root ([`merge_rows`]: rows keep their ids, a `!`-prefixed path keeps
    /// its dangling row, a root change re-resolves rows by name). Live paths
    /// on a node left with no root are an error naming both properties.
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
            "record_get",
        )?;
        let specs = props.get("paths").map(parse_paths).transpose()?;
        self.retarget(schema, fields, next_field_id, specs.as_deref(), defs)
    }

    /// The `record` input takes a record, so a record dragged off an output
    /// pin (`FromOutput`) can feed it: the node adopts its type exactly as a
    /// wire would — a named record's def, an anonymous record's fields.
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
        if let RecordType::Named(name) = record_type {
            registry.lookup_record_type_def(name)?;
        }
        let mut data = RecordGetData::default();
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
        let pins = row_pins(&root, &self.rows, defs, &[], missing)?;
        let mut custom = base.clone();
        custom.parameters = vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(root),
        }];
        custom.output_pins = if pins.is_empty() {
            vec![OutputPinDefinition::fixed("result", DataType::None)]
        } else {
            pins.iter()
                .map(|pin| {
                    let def = OutputPinDefinition::fixed(&pin.name, pin.pin_type());
                    match pin.id {
                        // The row id, so repair remaps output wires by row
                        // (`repair_node_network`'s R3 block).
                        Some(id) => def.with_id(id),
                        None => def,
                    }
                })
                .collect()
        };
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
        name: "record_get".to_string(),
        description: "Reads chosen fields of a record by path, at any depth: one output pin per \
            path (`p.x` reads field `x` of the nested record in field `p`). Get nested field \
            values without destructuring every level. The paths start at a schema (a record \
            type def) or, without one, at the type of the record wired into it — taken when \
            the node is placed from a record output, or when a record is wired into it while \
            nothing uses its outputs. Pins follow renamed fields of a def; a path whose field \
            was deleted is kept as a broken row with a warning."
            .to_string(),
        summary: Some("Get record fields by (nested) path".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // The unset node's; the registry-aware cache populator replaces them
        // with one pin per row through `build_record_node_type`.
        parameters: vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::anonymous(vec![])),
        }],
        output_pins: vec![OutputPinDefinition::fixed("result", DataType::None)],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordGetData::default()),
        node_data_saver: generic_node_data_saver::<RecordGetData>,
        node_data_loader: record_get_node_data_loader,
    }
}

/// Loader that heals persisted row state (missing / zero `next_row_id`,
/// id-less rows, duplicate or empty paths) after the generic deserialize.
fn record_get_node_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: RecordGetData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.heal();
    Ok(Box::new(data))
}
