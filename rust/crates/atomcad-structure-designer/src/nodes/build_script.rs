//! `build_script` — loads a build JSON file into `[BuildStep]`.
//!
//! The steps then travel the network as an ordinary array of an ordinary
//! record, so a loaded block, a generated block and a hand-authored one
//! concatenate with `array_concat` and are reshaped with `map`, `filter` and
//! `collect` like any other array. See `doc/design_mechanosynth_editor.md`.
//!
//! Whether a step names an operation the library actually has cannot be
//! answered here — this node sees no library — so an unknown op passes through
//! and the consumer reports it.
//!
//! The file-payload conventions are `ops_library`'s (and `import_cif`'s): the
//! parsed script is `#[serde(skip)]`, a no-op property write keeps the cache,
//! and `eval` re-reads a file whose cache is empty.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::build_step::{BUILD_STEP_RECORD, build_step_record};
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::mechanosynth::{BuildScript, MechanosynthError, load_build_script};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

/// Stored data of a `build_script` node.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildScriptData {
    /// `None` until a file is chosen.
    pub file: Option<String>,

    #[serde(skip)]
    pub script: Option<BuildScript>,
    #[serde(skip)]
    pub load_error: Option<String>,
}

impl BuildScriptData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses the stored file when there is no cache yet. See
    /// `OpsLibraryData::reload_missing`.
    pub fn reload_missing(&mut self, design_dir: Option<&str>) {
        if self.script.is_some() {
            return;
        }
        let Some(name) = self.file.clone() else {
            self.load_error = None;
            return;
        };
        match load_script_at(&name, design_dir) {
            Ok(script) => {
                self.script = Some(script);
                self.load_error = None;
            }
            Err(message) => self.load_error = Some(message),
        }
    }

    /// Node data for a `file` edit that keeps the parsed script when the name
    /// has not actually changed (`project_import_node_payload_wipe`).
    pub fn with_file(&self, file: Option<String>) -> Self {
        let unchanged = file == self.file;
        Self {
            file,
            script: if unchanged { self.script.clone() } else { None },
            load_error: if unchanged {
                self.load_error.clone()
            } else {
                None
            },
        }
    }

    fn resolve(
        &self,
        wired: Option<String>,
        design_dir: Option<&str>,
    ) -> Result<BuildScript, String> {
        if let Some(name) = wired {
            return load_script_at(&name, design_dir);
        }
        if let Some(script) = &self.script {
            return Ok(script.clone());
        }
        match &self.file {
            Some(name) => load_script_at(name, design_dir),
            None => Err("no build script (set the file property)".to_string()),
        }
    }
}

/// Resolves `name` against the design directory and reads a build script from
/// it. The error already names the file, so callers only prefix the node.
pub fn load_script_at(name: &str, design_dir: Option<&str>) -> Result<BuildScript, String> {
    let path = resolve_path(name, design_dir)
        .map(|(resolved, _was_relative)| resolved)
        .map_err(|_| format!("failed to resolve path: {name}"))?;
    load_build_script(Path::new(&path)).map_err(|e: MechanosynthError| e.to_string())
}

impl NodeData for BuildScriptData {
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
        let wired = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            None,
            |result| result.extract_string().map(Some),
        ) {
            Ok(name) => name,
            Err(propagated) => return EvalOutput::single(propagated),
        };

        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));

        match self.resolve(wired, design_dir.as_deref()) {
            Ok(script) => EvalOutput::single(NetworkResult::Array(
                script.steps.iter().map(build_step_record).collect(),
            )),
            Err(message) => {
                EvalOutput::single(NetworkResult::Error(format!("build_script: {message}")))
            }
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        if connected_input_pins.contains("file") {
            return None;
        }
        let file = self.file.clone()?;
        match &self.script {
            Some(script) => Some(format!("{file}  {} steps", script.steps.len())),
            None => Some(file),
        }
    }

    /// Total, for the same reason `ops_library`'s is.
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![(
            "file".to_string(),
            TextValue::String(self.file.clone().unwrap_or_default()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(value) = props.get("file") {
            let name = value
                .as_string()
                .ok_or_else(|| "file must be a string".to_string())?;
            let file = if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            };
            if file != self.file {
                self.file = file;
                self.script = None;
                self.load_error = None;
            }
        }
        Ok(())
    }
}

/// Pre-parses the script after deserializing.
pub fn build_script_data_loader(
    value: &Value,
    design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: BuildScriptData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.reload_missing(design_dir);
    Ok(Box::new(data))
}

/// Relativizes the stored path before saving, so projects stay portable.
pub fn build_script_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    let Some(data) = node_data.as_any_mut().downcast_mut::<BuildScriptData>() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for build_script",
        ));
    };

    if let (Some(file_name), Some(design_dir)) = (data.file.as_deref(), design_dir) {
        let (potentially_relative_path, should_update) =
            try_make_relative(file_name, Some(design_dir));
        if should_update {
            data.file = Some(potentially_relative_path);
        }
    }

    serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "build_script".to_string(),
        description: "Loads a mechanosynthesis **build script** from a JSON file and emits its \
            steps as an array of `BuildStep` records.\n\
            \n\
            Each step names an operation and gives a rigid transform placing that operation's \
            local frame into workpiece coordinates. Because the steps are an ordinary array, a \
            loaded block concatenates with a generated or hand-authored one through \
            `array_concat`, and `map`, `filter` and `collect` reshape it like any other array. \
            Wire the result into `mechanosynth`'s `steps` pin to replay it.\n\
            \n\
            Absent per-step fields take the file format's own defaults: an identity rotation, \
            empty `note` / `method` / `phase`, and `-1` for `layer` and `site`. The \
            file's `tolerance` is **not** read — a step array has no header, so the match \
            tolerance comes from the operation library.\n\
            \n\
            Whether a step names an operation that exists cannot be checked here, since this \
            node sees no library; an unknown operation is reported by whatever consumes the \
            steps. The path is stored relative to the project file whenever possible."
            .to_string(),
        summary: Some("Load a build script".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "file".to_string(),
            data_type: DataType::String,
        }],
        output_pins: vec![OutputPinDefinition::fixed(
            "steps",
            DataType::Array(Box::new(DataType::Record(RecordType::Named(
                BUILD_STEP_RECORD.to_string(),
            )))),
        )],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(BuildScriptData::new()),
        node_data_saver: build_script_data_saver,
        node_data_loader: build_script_data_loader,
    }
}
