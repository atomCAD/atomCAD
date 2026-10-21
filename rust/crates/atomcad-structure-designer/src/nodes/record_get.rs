//! `record_get` — read record fields by path, at any depth
//! (`doc/design_record_get_set.md`).
//!
//! One `record` input; one output pin per user-chosen **path** into the
//! `schema` def (`p.x`, `size`, …). Rows are stored by `FieldId` where the
//! record type is a named def ([`crate::record_path`]), so a field rename
//! anywhere along the path renames the pin and keeps its wire. A row whose
//! path stops resolving is dangling: it keeps its place in the data, loses its
//! pin, and is reported as a non-blocking warning.
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
    NodeType, NodeTypeCategory, OutputPinDefinition, Parameter, generic_node_data_loader,
    generic_node_data_saver,
};
use crate::node_type_registry::{NodeTypeRegistry, RecordDefs};
use crate::nodes::record_path_rows::{
    RecordPathRow, dangling_row_problems, parse_paths, paths_text_value, paths_text_value_stored,
    replace_rows, row_pins, rows_path_defs,
};
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordGetData {
    /// Name of the record def the paths start at. Empty = none chosen yet.
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub rows: Vec<RecordPathRow>,
    /// Monotonic id source for `rows`; never max(existing)+1
    /// (`doc/design_parameter_wire_stability.md`).
    #[serde(default)]
    pub next_row_id: u64,
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
            &self.schema,
            &self.rows,
            registry.record_defs(),
            &[],
            MissingDefs::Dangle,
        )
        .unwrap_or_default();
        if registry.lookup_record_type_def(&self.schema).is_none() || pins.is_empty() {
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

    /// `schema` first, then `paths` resolved against it. Phase 1: the paths
    /// replace the rows wholesale with fresh ids (`replace_rows`); a
    /// `!`-prefixed (dangling) path is rejected.
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
            Some(v) => {
                let specs = parse_paths(v)?;
                let mut next_row_id = self.next_row_id;
                let rows = replace_rows(&schema, &specs, defs, &mut next_row_id)?;
                self.next_row_id = next_row_id;
                Some(rows)
            }
            None => None,
        };
        self.schema = schema;
        if let Some(rows) = rows {
            self.rows = rows;
        }
        Ok(())
    }

    /// The `record` input takes a record, so a record dragged off an output
    /// pin (`FromOutput`) can feed it: adopt its schema. Anonymous records
    /// have no def to name.
    fn adapt_for_drag_source(
        &self,
        source_type: &DataType,
        direction: DragDirection,
        registry: &NodeTypeRegistry,
    ) -> Option<Box<dyn NodeData>> {
        if direction != DragDirection::FromOutput {
            return None;
        }
        let DataType::Record(RecordType::Named(name)) = source_type else {
            return None;
        };
        registry.lookup_record_type_def(name)?;
        Some(Box::new(RecordGetData {
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
        let pins = row_pins(&self.schema, &self.rows, defs, &[], missing)?;
        let mut custom = base.clone();
        custom.parameters = vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::Named(self.schema.clone())),
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
        dangling_row_problems(&self.schema, &self.rows, defs)
    }

    fn record_path_defs(&self, defs: RecordDefs<'_>) -> Vec<String> {
        rows_path_defs(&self.schema, &self.rows, defs)
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "record_get".to_string(),
        description: "Reads chosen fields of a record by path, at any depth: one output pin per \
            path (`p.x` reads field `x` of the nested record in field `p`). Get nested field \
            values without destructuring every level. Pins follow renamed fields; a path whose \
            field was deleted is kept as a broken row with a warning."
            .to_string(),
        summary: Some("Get record fields by (nested) path".to_string()),
        category: NodeTypeCategory::MathAndProgramming,
        // Placeholders; the registry-aware cache populator replaces them with
        // one pin per row through `build_record_node_type`.
        parameters: vec![Parameter {
            id: None,
            name: "record".to_string(),
            data_type: DataType::Record(RecordType::Named(String::new())),
        }],
        output_pins: vec![OutputPinDefinition::fixed("result", DataType::None)],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(RecordGetData::default()),
        node_data_saver: generic_node_data_saver::<RecordGetData>,
        node_data_loader: generic_node_data_loader::<RecordGetData>,
    }
}
