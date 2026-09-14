//! `ops_library` — loads a mechanosynthesis operation library from a JSON file.
//!
//! The one node that produces an `OpLibrary` value. Everything that interprets
//! a step against a workpiece — the replayer today, the editor and the
//! placement engine next — takes such a value on an `ops` pin, so an operation
//! library is a wired value rather than global design state. Two libraries that
//! both define `dimerize` coexist because they are two wires, which is exactly
//! what a design-level registry would have needed qualified names for. See
//! `doc/design_mechanosynth_editor.md` §Decisions.
//!
//! Conventions it inherits from the other file-reading nodes:
//!
//! - **The parsed library is `#[serde(skip)]` payload**, reloaded by
//!   [`ops_library_data_loader`] after deserialization, and the stored path is
//!   relativized on save so projects stay portable.
//! - **A no-op property write must not wipe the payload.** The file-name setter
//!   fires on every focus loss of a path field, so [`OpsLibraryData::with_file`]
//!   keeps the parsed cache when the name did not actually change
//!   (`project_import_node_payload_wipe`).
//! - **`eval` re-reads a file whose cache is empty**, the way `import_cif`
//!   does: a library is small JSON, and the text-format edit path drops the
//!   cache without a design directory to reload from.

use crate::data_type::DataType;
use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData, NodeDataError};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::mechanosynth::{MechanosynthError, OpLibrary, load_library};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::Arc;

/// Stored data of an `ops_library` node.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpsLibraryData {
    /// `None` until a file is chosen.
    pub file: Option<String>,

    #[serde(skip)]
    pub library: Option<Arc<OpLibrary>>,
    /// The parse failure from the most recent load attempt, surfaced at
    /// evaluation (the `import_cif` pattern: the node loads, the failure shows
    /// when it is evaluated).
    #[serde(skip)]
    pub load_error: Option<String>,
}

impl OpsLibraryData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses the stored file when there is no cache yet, and recomputes
    /// [`Self::load_error`].
    ///
    /// A cached library is never re-read, which is what makes this safe to call
    /// after every property write: only a name that actually changed has had
    /// its cache dropped (see [`Self::with_file`]).
    pub fn reload_missing(&mut self, design_dir: Option<&str>) {
        if self.library.is_some() {
            return;
        }
        let Some(name) = self.file.clone() else {
            self.load_error = None;
            return;
        };
        match load_library_at(&name, design_dir) {
            Ok(library) => {
                self.library = Some(Arc::new(library));
                self.load_error = None;
            }
            Err(message) => self.load_error = Some(message),
        }
    }

    /// Node data for a `file` edit that **keeps the parsed library when the
    /// name has not actually changed** — the `project_import_node_payload_wipe`
    /// pitfall.
    pub fn with_file(&self, file: Option<String>) -> Self {
        let unchanged = file == self.file;
        Self {
            file,
            library: if unchanged {
                self.library.clone()
            } else {
                None
            },
            load_error: if unchanged {
                self.load_error.clone()
            } else {
                None
            },
        }
    }

    /// The library to emit: the wired file if one arrived, else the parsed
    /// cache, else the stored file re-read from disk.
    fn resolve(
        &self,
        wired: Option<String>,
        design_dir: Option<&str>,
    ) -> Result<Arc<OpLibrary>, String> {
        if let Some(name) = wired {
            return load_library_at(&name, design_dir).map(Arc::new);
        }
        if let Some(library) = &self.library {
            return Ok(library.clone());
        }
        match &self.file {
            Some(name) => load_library_at(name, design_dir).map(Arc::new),
            None => Err("no operation library (set the file property)".to_string()),
        }
    }
}

/// Resolves `name` against the design directory and reads a library from it.
/// The error already names the file, so callers only prefix the node.
pub fn load_library_at(name: &str, design_dir: Option<&str>) -> Result<OpLibrary, String> {
    let path = resolve_path(name, design_dir)
        .map(|(resolved, _was_relative)| resolved)
        .map_err(|_| format!("failed to resolve path: {name}"))?;
    load_library(Path::new(&path)).map_err(|e: MechanosynthError| e.to_string())
}

impl NodeData for OpsLibraryData {
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
            Ok(library) => EvalOutput::single(NetworkResult::OpLibrary(library)),
            Err(message) => {
                EvalOutput::single(NetworkResult::Error(format!("ops_library: {message}")))
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
        match &self.library {
            Some(library) => Some(format!("{file}  {} ops", library.ops.len())),
            None => Some(file),
        }
    }

    /// The library's own load-time warnings — today the origin convention —
    /// surfaced as a **non-blocking** error so they reach the unified error
    /// list without stopping evaluation. The node still produces a usable
    /// library, so they fail the blocking litmus in
    /// `doc/design_error_management.md`.
    fn get_data_error(&self, _connected_input_pins: &HashSet<String>) -> Option<NodeDataError> {
        self.library
            .as_ref()
            .filter(|library| !library.warnings.is_empty())
            .map(|library| NodeDataError::warning(library.warnings.join("\n")))
    }

    /// **Total**, deliberately: a property the node omits here is treated as
    /// wire-only by the text editor and a literal for it is silently dropped,
    /// so the file name could never be *set* from the text on a node that has
    /// none yet (`project_text_format_roundtrip`).
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![(
            "file".to_string(),
            TextValue::String(self.file.clone().unwrap_or_default()),
        )]
    }

    /// A name that actually changes drops its parsed cache; `eval` re-reads it,
    /// because this path has no design directory to reload from.
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
                self.library = None;
                self.load_error = None;
            }
        }
        Ok(())
    }
}

/// Pre-parses the library after deserializing, mirroring
/// `mechanosynth_data_loader`.
pub fn ops_library_data_loader(
    value: &Value,
    design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: OpsLibraryData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.reload_missing(design_dir);
    Ok(Box::new(data))
}

/// Relativizes the stored path before saving, so projects stay portable.
pub fn ops_library_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    let Some(data) = node_data.as_any_mut().downcast_mut::<OpsLibraryData>() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for ops_library",
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
        name: "ops_library".to_string(),
        description: "Loads a mechanosynthesis **operation library** from a JSON file and emits \
            it as a value.\n\
            \n\
            An operation is a before/after pair of small atom lists in a local frame; comparing \
            the two by pattern id *is* the rewrite. A build script's steps name operations from \
            such a library, so `mechanosynth` needs one on its `ops` pin. Wire the same node into \
            as many consumers as you like — the library is parsed once and shared.\n\
            \n\
            The library's `tolerance` is the match tolerance for every replay against it; a \
            generated library states a tight one (0.05 Å), and a file that states none gets that \
            same default. An operation may state `\"chiral\": true`, which the replayer ignores \
            and the interactive placement tool reads as \"a mirrored placement is a different \
            reaction\".\n\
            \n\
            By convention the `before` atom with id 1 sits at the origin and is the atom the \
            operation acts on. A library that breaks the convention still loads and still \
            replays; the node shows a warning naming the operation.\n\
            \n\
            The path is stored relative to the project file whenever possible, so a copied \
            project keeps working."
            .to_string(),
        summary: Some("Load an operation library".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "file".to_string(),
            data_type: DataType::String,
        }],
        output_pins: vec![OutputPinDefinition::fixed("ops", DataType::OpLibrary)],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(OpsLibraryData::new()),
        node_data_saver: ops_library_data_saver,
        node_data_loader: ops_library_data_loader,
    }
}
