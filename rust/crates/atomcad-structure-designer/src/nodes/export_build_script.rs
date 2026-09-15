//! `export_build_script` — writes a `[BuildStep]` array back out as a build
//! JSON file.
//!
//! The inverse of `build_script`, and the way an authored or reshaped block
//! leaves the application. Like `export_atoms` it is a `Unit`-returning
//! **effect node**: the central skip rule in `evaluate_all_outputs`
//! (`doc/design_node_execution.md`) guarantees this `eval` runs only under the
//! Execute action, so an ordinary evaluation never writes a file and there is
//! no `if context.execute` guard here.
//!
//! **What it omits is part of the format, not a size optimization.** An
//! identity `r`, an empty `note` / `phase` and a `-1` `layer` /
//! `site` are exactly the values `parse_build_script` produces for an absent
//! key, so omitting them makes `build_script → export_build_script` a
//! round trip rather than a re-write.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::{NetworkResult, dmat3_to_rows};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::build_step::{BUILD_STEP_RECORD, is_identity_rotation, steps_from_array};
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::mechanosynth::{BUILD_FORMAT, NO_LAYER, NO_SITE, Step};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::io;

/// Stored data of an `export_build_script` node.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportBuildScriptData {
    /// Empty until a file name is given.
    pub file_name: String,
}

impl ExportBuildScriptData {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NodeData for ExportBuildScriptData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

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
        let steps_value =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if steps_value.is_error() {
            return EvalOutput::single(steps_value);
        }
        let steps = match steps_from_array(&steps_value) {
            Ok(steps) => steps,
            Err(message) => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "export_build_script: {message}"
                )));
            }
        };

        let file_name = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.file_name.clone(),
            NetworkResult::extract_string,
        ) {
            Ok(value) => value,
            Err(propagated) => return EvalOutput::single(propagated),
        };
        if file_name.is_empty() {
            return EvalOutput::single(NetworkResult::Error(
                "export_build_script: missing export file name".to_string(),
            ));
        }

        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));
        let resolved_path = match resolve_path(&file_name, design_dir.as_deref()) {
            Ok((path, _was_relative)) => path,
            Err(_) => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "export_build_script: failed to resolve export path: {file_name}"
                )));
            }
        };

        // The optional `metadata` record rides along in the document, the way
        // `export_atoms` writes a sidecar: a build file already has a header,
        // so there is nowhere better and no second file to keep paired.
        let metadata = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 2);
        let metadata = match metadata {
            NetworkResult::None => None,
            NetworkResult::Error(err) => return EvalOutput::single(NetworkResult::Error(err)),
            record => Some(record),
        };

        let document = build_document(&steps, metadata.as_ref());
        let json_text = match serde_json::to_string_pretty(&document) {
            Ok(text) => text,
            Err(err) => {
                return EvalOutput::single(NetworkResult::Error(format!(
                    "export_build_script: failed to render '{file_name}': {err}"
                )));
            }
        };
        if let Err(err) = std::fs::write(&resolved_path, json_text) {
            return EvalOutput::single(NetworkResult::Error(format!(
                "export_build_script: failed to write '{file_name}': {err}"
            )));
        }

        EvalOutput::single(NetworkResult::Unit)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    /// Eager feedback for the Execute-deferred checks, the way `export_atoms`
    /// recovers it: under the central skip rule `eval` does not run on a
    /// display pass, so a missing file name would otherwise be invisible until
    /// the user pressed Execute.
    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("file_name") {
            None
        } else if self.file_name.is_empty() {
            Some("(no file name)".to_string())
        } else {
            Some(self.file_name.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![(
            "file_name".to_string(),
            TextValue::String(self.file_name.clone()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(value) = props.get("file_name") {
            self.file_name = value
                .as_string()
                .ok_or_else(|| "file_name must be a string".to_string())?
                .to_string();
        }
        Ok(())
    }
}

/// The build JSON document for a list of steps.
fn build_document(steps: &[Step], metadata: Option<&NetworkResult>) -> Value {
    let mut document = Map::new();
    document.insert(
        "format".to_string(),
        Value::String(BUILD_FORMAT.to_string()),
    );
    if let Some(metadata) = metadata {
        document.insert("metadata".to_string(), metadata_to_json(metadata));
    }
    document.insert(
        "steps".to_string(),
        Value::Array(steps.iter().map(step_to_json).collect()),
    );
    Value::Object(document)
}

fn step_to_json(step: &Step) -> Value {
    let mut object = Map::new();
    object.insert("op".to_string(), Value::String(step.op.clone()));
    object.insert(
        "t".to_string(),
        Value::from(vec![step.t.x, step.t.y, step.t.z]),
    );
    if !is_identity_rotation(&step.r) {
        // Three **rows**, matching the reader: `p = r * p_local + t` is written
        // that way on paper, and `parse_build_script` transposes at the
        // boundary.
        let rows = dmat3_to_rows(&step.r);
        object.insert(
            "r".to_string(),
            Value::from(rows.iter().map(|row| row.to_vec()).collect::<Vec<_>>()),
        );
    }
    if let Some(note) = step.note.as_deref().filter(|note| !note.is_empty()) {
        object.insert("note".to_string(), Value::String(note.to_string()));
    }
    if !step.phase.is_empty() {
        object.insert("phase".to_string(), Value::String(step.phase.clone()));
    }
    if step.layer != NO_LAYER {
        object.insert("layer".to_string(), Value::from(step.layer));
    }
    if step.site != NO_SITE {
        object.insert("site".to_string(), Value::from(step.site));
    }
    Value::Object(object)
}

/// Best-effort, total conversion of the optional metadata record to JSON.
/// Mirrors `export_atoms`'s sidecar conversion: structural where a value has a
/// clean JSON shape, its display string otherwise, never an error.
fn metadata_to_json(value: &NetworkResult) -> Value {
    match value {
        NetworkResult::Bool(b) => Value::Bool(*b),
        NetworkResult::Int(i) => Value::from(*i),
        NetworkResult::Float(f) => Value::from(*f),
        NetworkResult::String(s) => Value::String(s.clone()),
        NetworkResult::Vec2(v) => Value::from(vec![v.x, v.y]),
        NetworkResult::Vec3(v) => Value::from(vec![v.x, v.y, v.z]),
        NetworkResult::IVec2(v) => Value::from(vec![v.x, v.y]),
        NetworkResult::IVec3(v) => Value::from(vec![v.x, v.y, v.z]),
        NetworkResult::Mat3(m) => {
            let rows = dmat3_to_rows(m);
            Value::from(rows.iter().map(|r| r.to_vec()).collect::<Vec<_>>())
        }
        NetworkResult::IMat3(m) => Value::from(m.iter().map(|r| r.to_vec()).collect::<Vec<_>>()),
        NetworkResult::Array(elements) => {
            Value::Array(elements.iter().map(metadata_to_json).collect())
        }
        NetworkResult::Record(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), metadata_to_json(value)))
                .collect::<Map<String, Value>>(),
        ),
        other => Value::String(other.to_display_string()),
    }
}

/// Relativizes the stored path before saving, so projects stay portable.
pub fn export_build_script_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    let Some(data) = node_data
        .as_any_mut()
        .downcast_mut::<ExportBuildScriptData>()
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for export_build_script",
        ));
    };

    if let (Some(design_dir), false) = (design_dir, data.file_name.is_empty()) {
        let (potentially_relative_path, should_update) =
            try_make_relative(&data.file_name, Some(design_dir));
        if should_update {
            data.file_name = potentially_relative_path;
        }
    }

    serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn export_build_script_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let data: ExportBuildScriptData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Box::new(data))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "export_build_script".to_string(),
        description: "Writes the `[BuildStep]` array on its `steps` input to a mechanosynthesis \
            build JSON file — the format `build_script` reads.\n\
            \n\
            Like `export_atoms` this node exists for its side effect: it returns `Unit` and runs \
            only when you invoke **Execute** on it, so an ordinary evaluation never touches the \
            disk.\n\
            \n\
            Per-step fields that hold their default are omitted, which is what makes a load and \
            a re-export a round trip rather than a re-write: an identity rotation, an empty \
            `note` or `phase`, and a `layer` or `site` of `-1`. A record wired into \
            the optional `metadata` pin is written into the file's header.\n\
            \n\
            The path is stored relative to the project file whenever possible."
            .to_string(),
        summary: Some("Write a build script file".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "steps".to_string(),
                data_type: DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    BUILD_STEP_RECORD.to_string(),
                )))),
            },
            Parameter {
                id: None,
                name: "file_name".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "metadata".to_string(),
                // Empty anonymous record: any record value flows in via width
                // subtyping, carrying all its fields.
                data_type: DataType::Record(RecordType::anonymous(vec![])),
            },
        ],
        output_pins: OutputPinDefinition::single_fixed(DataType::Unit),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ExportBuildScriptData::new()),
        node_data_saver: export_build_script_data_saver,
        node_data_loader: export_build_script_data_loader,
    }
}
