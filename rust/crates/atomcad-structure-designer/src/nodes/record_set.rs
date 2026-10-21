//! `record_set` — update record fields by path, at any depth
//! (`doc/design_record_get_set.md`).
//!
//! One `record` input (pin 0, so the `-1` function pin wired into `map.f`
//! updates every record of a stream); one input pin per user-chosen **path**
//! into the `schema` def, each assigning that field; one output, the updated
//! record. Every field no path names passes through untouched, including
//! extra runtime fields beyond the schema. Update only: the output type is
//! the schema.
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
    NodeType, NodeTypeCategory, OutputPinDefinition, Parameter, generic_node_data_loader,
    generic_node_data_saver,
};
use crate::node_type_registry::{NodeTypeRegistry, RecordDefs};
use crate::nodes::comment::ANCHOR_PROPERTY;
use crate::nodes::record_path_rows::{
    RecordPathRow, dangling_row_problems, parse_paths, paths_text_value, paths_text_value_stored,
    replace_rows, row_pins, rows_path_defs,
};
use crate::record_path::format_path;
use crate::structure_designer::StructureDesigner;
use crate::text_format::{PIN_ROLES_PROPERTY, TextValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Names a row pin may not take: `record_set`'s row pins are inputs, so they
/// share the text format's `{ … }` block with the fixed `record` pin, the
/// node's own properties and the network-level special properties. A derived
/// name equal to one of these gets a `__2` suffix.
pub const RESERVED_PIN_NAMES: [&str; 6] = [
    "record",
    "schema",
    "paths",
    "visible",
    ANCHOR_PROPERTY,
    PIN_ROLES_PROPERTY,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordSetData {
    /// Name of the record def the paths start at, and the output type.
    /// Empty = none chosen yet.
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub rows: Vec<RecordSetRow>,
    /// Monotonic id source for `rows`; never max(existing)+1
    /// (`doc/design_parameter_wire_stability.md`).
    #[serde(default)]
    pub next_row_id: u64,
}

impl RecordSetData {
    /// Replaces the rows wholesale from a `paths` text value (fresh ids, no
    /// literals). The Phase 1/2 minimal setter; `merge_rows` replaces it.
    fn set_paths(
        &mut self,
        schema: &str,
        paths: &TextValue,
        defs: RecordDefs<'_>,
    ) -> Result<Vec<RecordSetRow>, String> {
        let specs = parse_paths(paths)?;
        let mut next_row_id = self.next_row_id;
        let rows = replace_rows(schema, &specs, defs, &mut next_row_id)?;
        self.next_row_id = next_row_id;
        Ok(rows
            .into_iter()
            .map(|row| RecordSetRow { row, literal: None })
            .collect())
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
        if registry.lookup_record_type_def(&self.schema).is_none() {
            return EvalOutput::single(NetworkResult::None);
        }
        // The same pins the layout was built with: row pin `k` is input `k + 1`.
        let pins = row_pins(
            &self.schema,
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
        vec![
            ("schema".to_string(), TextValue::String(self.schema.clone())),
            ("paths".to_string(), paths_text_value_stored(&self.rows)),
        ]
    }

    fn get_text_properties_with_defs(&self, defs: RecordDefs<'_>) -> Vec<(String, TextValue)> {
        vec![
            ("schema".to_string(), TextValue::String(self.schema.clone())),
            (
                "paths".to_string(),
                paths_text_value(&self.schema, &self.rows, defs),
            ),
        ]
    }

    /// Paths need the defs to resolve; without them only `schema` can be set.
    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if props.contains_key("paths") {
            return Err("paths can only be set with the record defs at hand".to_string());
        }
        if let Some(v) = props.get("schema") {
            self.schema = v
                .as_string()
                .ok_or_else(|| "schema must be a String".to_string())?
                .to_string();
        }
        Ok(())
    }

    /// `schema` first, then `paths` resolved against it. Like `record_get`'s
    /// Phase 1 setter, the paths replace the rows wholesale with fresh ids and
    /// no literals; a `!`-prefixed (dangling) path is rejected.
    fn set_text_properties_with_defs(
        &mut self,
        props: &HashMap<String, TextValue>,
        defs: RecordDefs<'_>,
    ) -> Result<(), String> {
        let schema = match props.get("schema") {
            Some(v) => v
                .as_string()
                .ok_or_else(|| "schema must be a String".to_string())?
                .to_string(),
            None => self.schema.clone(),
        };
        let rows = match props.get("paths") {
            Some(v) => Some(self.set_paths(&schema, v, defs)?),
            None => None,
        };
        self.schema = schema;
        if let Some(rows) = rows {
            self.rows = rows;
        }
        Ok(())
    }

    /// The `record` input and the output are both the record, so a record
    /// dragged from either side can connect: adopt its schema. Anonymous
    /// records have no def to name.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        _direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        let DataType::Record(RecordType::Named(name)) = source_type else {
            return None;
        };
        registry.lookup_record_type_def(name)?;
        Some(Box::new(RecordSetData {
            schema: name.clone(),
            ..Default::default()
        }))
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
        let pins = row_pins(&self.schema, &self.rows, defs, &RESERVED_PIN_NAMES, missing)?;
        let record_type = DataType::Record(RecordType::Named(self.schema.clone()));
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
        dangling_row_problems(&self.schema, &self.rows, defs)
    }

    fn record_path_defs(&self, defs: RecordDefs<'_>) -> Vec<String> {
        rows_path_defs(&self.schema, &self.rows, defs)
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_set".to_string(),
        description: "Updates chosen fields of a record by path, at any depth: one input pin per \
            path (`p.x` sets field `x` of the nested record in field `p`), and the updated \
            record as output. Every other field passes through unchanged. An unwired path pin \
            uses its stored value, or leaves the field as it is. Set nested field values \
            without destructuring and rebuilding every level."
            .to_string(),
        summary: Some("Set record fields by (nested) path".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Placeholders; the registry-aware cache populator replaces them with
        // `record` plus one pin per row through `build_record_node_type`.
        parameters: vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::Named(String::new())),
        }],
        output_pins: OutputPinDefinition::single_fixed(DataType::Record(RecordType::Named(
            String::new(),
        ))),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordSetData::default()),
        node_data_saver: generic_node_data_saver::<RecordSetData>,
        node_data_loader: generic_node_data_loader::<RecordSetData>,
    }
}
