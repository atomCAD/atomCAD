//! `mechanosynth` — replays a mechanosynthetic build sequence onto a workpiece.
//!
//! The whole rewrite engine lives in `atomcad_crystolecule::mechanosynth`; this
//! node is the thin wrapper that gives it two file properties, a step number
//! and a place in the network. See `design_mechanosynth_node.md` (external, in
//! the mechanosynth working folder), Track A P2.
//!
//! Three conventions this node inherits and must not break:
//!
//! - **The parsed files are `#[serde(skip)]` payload**, reloaded by
//!   [`mechanosynth_data_loader`] after deserialization exactly as
//!   `ImportXYZData` does, and the stored paths are relativized on save so
//!   projects stay portable.
//! - **A no-op property write must not wipe the payload.** The file-name
//!   setters fire on every focus loss of a path field, so
//!   [`MechanosynthData::with_ops_file`] / [`MechanosynthData::with_build_file`]
//!   keep the parsed cache when the name did not actually change
//!   (`project_import_node_payload_wipe`).
//! - **`eval` re-reads a file whose cache is empty**, the way `import_cif`
//!   does. Both files are small JSON, and the text-format edit path drops the
//!   cache without a design directory to reload from, so this fallback is what
//!   keeps a `query`/`edit` round trip evaluating.
//!
//! The highlight tags are passed in from here rather than baked into the
//! engine: `replay`'s [`HighlightTags`] argument names all three for the node
//! and is `HighlightTags::default()` — nothing painted, no tag name interned —
//! for the engine tests and any non-UI caller.
//!
//! The node has **two** output pins: the workpiece, and a `MechanosynthStep`
//! record describing the last step applied. They are one evaluation, so an
//! error reaches both (`both`), and the record is read off the same script and
//! the same clamp the replay used rather than replaying a second time. See
//! `doc/design_mechanosynth_step_metadata.md`.

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
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::mechanosynth::{
    BuildScript, HighlightTags, MechanosynthError, NO_LAYER, NO_SITE, OpLibrary, load_build_script,
    load_library, replay, steps_applied,
};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

/// The atom tag the node paints on the atoms of the current step. An ordinary
/// tag (`doc/design_atom_tags.md`), so `apply_style` can colour it and the tag
/// panel lists it — at the cost of one of the 32 tag slots.
pub const MS_CURRENT_TAG: &str = "ms_current";

/// The atom tag the node paints on every atom *created* by an applied step —
/// "what this build has put down so far", as against the base it started from.
pub const MS_ADDED_TAG: &str = "ms_added";

/// The atom tag the node paints on the atoms created by the terrace under
/// construction: every atom an applied step whose `layer` matches the current
/// step's created. Empty when the current step names no layer.
pub const MS_LAYER_TAG: &str = "ms_layer";

/// The named record type of the `step` output pin. Registered in
/// `node_type_registry.rs` beside `Patch` and `MaterializeRegion`.
pub const MECHANOSYNTH_STEP_RECORD: &str = "MechanosynthStep";

/// Index of the `step` output pin. **Appended** (pin 1), never inserted, so
/// saved projects and existing wires keep their pin indices
/// (`doc/design_multi_output_pins.md`).
pub const STEP_OUTPUT_PIN: usize = 1;

/// `step = -1` means "every step", which is the useful default: a freshly wired
/// node shows the finished build.
pub fn default_step() -> i32 {
    -1
}

/// Stored data of a `mechanosynth` node.
///
/// Only the three properties are serialized. The parsed library and script are
/// payload — reloaded from the files on project load, on a property edit, and
/// (as a fallback) at evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MechanosynthData {
    pub ops_file: Option<String>,
    pub build_file: Option<String>,
    /// Number of steps to apply; negative means "all". See
    /// [`steps_applied`] for the clamping rule.
    #[serde(default = "default_step")]
    pub step: i32,

    #[serde(skip)]
    pub library: Option<OpLibrary>,
    #[serde(skip)]
    pub script: Option<BuildScript>,
    /// The parse failure from the most recent load attempt, surfaced at
    /// evaluation (the `import_xyz` pattern: the node loads, the failure shows
    /// when it is evaluated).
    #[serde(skip)]
    pub load_error: Option<String>,
}

impl MechanosynthData {
    pub fn new() -> Self {
        Self {
            ops_file: None,
            build_file: None,
            step: default_step(),
            library: None,
            script: None,
            load_error: None,
        }
    }

    /// Parses whichever of the two files has no cache yet, from the stored
    /// names, and recomputes [`Self::load_error`].
    ///
    /// A cached file is never re-read, which is what makes this safe to call
    /// after every property write: only a name that actually changed has had
    /// its cache dropped (see [`Self::with_ops_file`]). A file whose cache is
    /// empty is either new, or failed last time and is worth retrying.
    pub fn reload_missing(&mut self, design_dir: Option<&str>) {
        let mut errors = Vec::new();

        if self.library.is_none()
            && let Some(name) = self.ops_file.clone()
        {
            match load_library_at(&name, design_dir) {
                Ok(library) => self.library = Some(library),
                Err(message) => errors.push(message),
            }
        }
        if self.script.is_none()
            && let Some(name) = self.build_file.clone()
        {
            match load_script_at(&name, design_dir) {
                Ok(script) => self.script = Some(script),
                Err(message) => errors.push(message),
            }
        }

        self.load_error = if errors.is_empty() {
            None
        } else {
            Some(errors.join("\n"))
        };
    }

    /// Node data for an `ops_file` edit that **keeps the parsed library when
    /// the name has not actually changed** — the
    /// `project_import_node_payload_wipe` pitfall.
    pub fn with_ops_file(&self, ops_file: Option<String>) -> Self {
        let unchanged = ops_file == self.ops_file;
        Self {
            ops_file,
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
            ..self.clone()
        }
    }

    /// The `build_file` counterpart of [`Self::with_ops_file`].
    pub fn with_build_file(&self, build_file: Option<String>) -> Self {
        let unchanged = build_file == self.build_file;
        Self {
            build_file,
            script: if unchanged { self.script.clone() } else { None },
            load_error: if unchanged {
                self.load_error.clone()
            } else {
                None
            },
            ..self.clone()
        }
    }

    /// How many steps the stored `step` asks for, and how many the script has.
    /// `None` when no script is loaded.
    pub fn step_counts(&self) -> Option<(usize, usize)> {
        let count = self.script.as_ref()?.steps.len();
        Some((steps_applied(self.step, count), count))
    }

    /// The library to replay with: the wired file if one arrived, else the
    /// parsed cache, else the stored file re-read from disk.
    fn resolve_library(
        &self,
        wired: Option<String>,
        design_dir: Option<&str>,
    ) -> Result<Cow<'_, OpLibrary>, String> {
        if let Some(name) = wired {
            return load_library_at(&name, design_dir).map(Cow::Owned);
        }
        if let Some(library) = &self.library {
            return Ok(Cow::Borrowed(library));
        }
        match &self.ops_file {
            Some(name) => load_library_at(name, design_dir).map(Cow::Owned),
            None => Err("no operation library (set the ops_file property)".to_string()),
        }
    }

    /// The build script to replay. Same precedence as [`Self::resolve_library`].
    fn resolve_script(
        &self,
        wired: Option<String>,
        design_dir: Option<&str>,
    ) -> Result<Cow<'_, BuildScript>, String> {
        if let Some(name) = wired {
            return load_script_at(&name, design_dir).map(Cow::Owned);
        }
        if let Some(script) = &self.script {
            return Ok(Cow::Borrowed(script));
        }
        match &self.build_file {
            Some(name) => load_script_at(name, design_dir).map(Cow::Owned),
            None => Err("no build script (set the build_file property)".to_string()),
        }
    }
}

impl Default for MechanosynthData {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolves `name` against the design directory and reads an operation library
/// from it. The error already names the file, so callers only prefix the node.
fn load_library_at(name: &str, design_dir: Option<&str>) -> Result<OpLibrary, String> {
    let path = resolve(name, design_dir)?;
    load_library(Path::new(&path)).map_err(|e: MechanosynthError| e.to_string())
}

/// The build-script counterpart of [`load_library_at`]. Public because the
/// property panel's readout (`mechanosynth_api::mechanosynth_info`) must read
/// a script that arrives on the wired `build_file` pin exactly the way `eval`
/// does, or the slider has no range whenever the file name is switched in.
pub fn load_script_at(name: &str, design_dir: Option<&str>) -> Result<BuildScript, String> {
    let path = resolve(name, design_dir)?;
    load_build_script(Path::new(&path)).map_err(|e: MechanosynthError| e.to_string())
}

fn resolve(name: &str, design_dir: Option<&str>) -> Result<String, String> {
    resolve_path(name, design_dir)
        .map(|(resolved, _was_relative)| resolved)
        .map_err(|_| format!("failed to resolve path: {name}"))
}

/// A wired file-name pin: `None` when nothing is connected, the string when one
/// is. Anything else is a type error the evaluator reports for us, so a
/// non-string simply falls back to the stored property.
#[allow(clippy::result_large_err)]
fn wired_name(
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'_>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    parameter_index: usize,
) -> Result<Option<String>, NetworkResult> {
    network_evaluator.evaluate_or_default(
        network_stack,
        node_id,
        registry,
        context,
        parameter_index,
        None,
        |result| result.extract_string().map(Some),
    )
}

impl NodeData for MechanosynthData {
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
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if input_val.is_error() {
            return both(input_val);
        }
        let mut wrapper = match input_val {
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_) => input_val,
            other => {
                return both(error(format!(
                    "expected atomic input, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));

        let wired_ops = match wired_name(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            1,
        ) {
            Ok(name) => name,
            Err(propagated) => return both(propagated),
        };
        let wired_build = match wired_name(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            2,
        ) {
            Ok(name) => name,
            Err(propagated) => return both(propagated),
        };
        let step = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.step,
            NetworkResult::extract_int,
        ) {
            Ok(step) => step,
            Err(propagated) => return both(propagated),
        };

        let library = match self.resolve_library(wired_ops, design_dir.as_deref()) {
            Ok(library) => library,
            Err(message) => return both(error(message)),
        };
        let script = match self.resolve_script(wired_build, design_dir.as_deref()) {
            Ok(script) => script,
            Err(message) => return both(error(message)),
        };

        let atoms = match &mut wrapper {
            NetworkResult::Crystal(crystal) => &mut crystal.atoms,
            NetworkResult::Molecule(molecule) => &mut molecule.atoms,
            _ => unreachable!("the match above admitted only these two"),
        };
        let tags = HighlightTags {
            current: Some(MS_CURRENT_TAG),
            added: Some(MS_ADDED_TAG),
            layer: Some(MS_LAYER_TAG),
        };
        match replay(atoms, &library, &script, step, tags) {
            Ok(result) => {
                *atoms = result;
                // The record is built from the same script and the same clamp
                // the replay just used, so the second pin costs no second
                // replay.
                let record = step_record(&script, step);
                EvalOutput::multi(vec![wrapper, record])
            }
            Err(failure) => both(error(failure.to_string())),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        let mut parts = Vec::new();
        if !connected_input_pins.contains("build_file")
            && let Some(build_file) = &self.build_file
        {
            parts.push(build_file.clone());
        }
        if let Some((applied, count)) = self.step_counts() {
            // A wired `step` overrides the stored one, so the stored count would
            // be a lie; show the script's length alone in that case.
            if connected_input_pins.contains("step") {
                parts.push(format!("{count} steps"));
            } else {
                parts.push(format!("{applied} / {count}"));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("  "))
        }
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut metadata = HashMap::new();
        metadata.insert("base".to_string(), (true, None));
        metadata
    }

    /// **Total**, deliberately: a property the node omits here is treated as
    /// wire-only by the text editor and a literal for it is silently dropped
    /// with a warning, so a file name could never be *set* from the text on a
    /// node that has none yet (`project_text_format_roundtrip`). An absent name
    /// is therefore the empty string in both directions.
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "ops_file".to_string(),
                TextValue::String(self.ops_file.clone().unwrap_or_default()),
            ),
            (
                "build_file".to_string(),
                TextValue::String(self.build_file.clone().unwrap_or_default()),
            ),
            ("step".to_string(), TextValue::Int(self.step)),
        ]
    }

    /// A name that actually changes drops its parsed cache; `eval` re-reads it,
    /// because this path has no design directory to reload from. A property the
    /// text omits keeps its stored value.
    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(value) = props.get("ops_file") {
            let ops_file = optional_name(value, "ops_file")?;
            if ops_file != self.ops_file {
                self.ops_file = ops_file;
                self.library = None;
                self.load_error = None;
            }
        }
        if let Some(value) = props.get("build_file") {
            let build_file = optional_name(value, "build_file")?;
            if build_file != self.build_file {
                self.build_file = build_file;
                self.script = None;
                self.load_error = None;
            }
        }
        if let Some(value) = props.get("step") {
            self.step = value
                .as_int()
                .ok_or_else(|| "step must be an integer".to_string())?;
        }
        Ok(())
    }
}

/// A file-name text property: the empty string is "no file", matching what
/// [`MechanosynthData::get_text_properties`] writes for a `None`.
fn optional_name(value: &TextValue, property: &str) -> Result<Option<String>, String> {
    let name = value
        .as_string()
        .ok_or_else(|| format!("{property} must be a string"))?;
    Ok(if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    })
}

/// Wraps a message as the node's evaluation error. One prefix, applied in one
/// place, so the engine's own wording reaches the user unchanged behind it.
fn error(message: impl std::fmt::Display) -> NetworkResult {
    NetworkResult::Error(format!("mechanosynth: {message}"))
}

/// The same result on both output pins.
///
/// The two outputs are one evaluation, so whatever stops the workpiece from
/// being produced stops the record too: a `MechanosynthStep` whose `index`
/// described a replay that failed would be a lie, and `None` on the pin would
/// be a silent one.
fn both(result: NetworkResult) -> EvalOutput {
    EvalOutput::multi(vec![result.clone(), result])
}

/// The `MechanosynthStep` record describing the **last step applied** — the
/// same "current step" the property panel names.
///
/// At `index = 0` nothing has run, so the step-specific fields take their
/// absent-field defaults rather than describing `steps[0]`, which has *not*
/// been applied yet.
fn step_record(script: &BuildScript, step: i32) -> NetworkResult {
    let count = script.steps.len();
    let index = steps_applied(step, count);
    let current = index.checked_sub(1).and_then(|last| script.steps.get(last));

    let text = |value: Option<&str>| NetworkResult::String(value.unwrap_or_default().to_string());

    NetworkResult::record(vec![
        ("index".to_string(), NetworkResult::Int(index as i32)),
        ("count".to_string(), NetworkResult::Int(count as i32)),
        ("op".to_string(), text(current.map(|s| s.op.as_str()))),
        (
            "note".to_string(),
            text(current.and_then(|s| s.note.as_deref())),
        ),
        (
            "method".to_string(),
            text(current.map(|s| s.method.as_str())),
        ),
        ("phase".to_string(), text(current.map(|s| s.phase.as_str()))),
        (
            "layer".to_string(),
            NetworkResult::Int(current.map_or(NO_LAYER, |s| s.layer)),
        ),
        (
            "site".to_string(),
            NetworkResult::Int(current.map_or(NO_SITE, |s| s.site)),
        ),
        (
            "t".to_string(),
            NetworkResult::Vec3(current.map_or(DVec3::ZERO, |s| s.t)),
        ),
    ])
}

/// Pre-parses both files after deserializing, mirroring
/// `import_xyz_data_loader`.
///
/// A failure leaves the cache empty and the message in `load_error`, so the
/// node exists and reports the problem when evaluated rather than failing the
/// whole project load.
pub fn mechanosynth_data_loader(
    value: &Value,
    design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: MechanosynthData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    data.reload_missing(design_dir);
    Ok(Box::new(data))
}

/// Relativizes both stored paths before saving, so projects stay portable.
pub fn mechanosynth_data_saver(
    node_data: &mut dyn NodeData,
    design_dir: Option<&str>,
) -> io::Result<Value> {
    let Some(data) = node_data.as_any_mut().downcast_mut::<MechanosynthData>() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for mechanosynth",
        ));
    };

    if let Some(design_dir) = design_dir {
        for slot in [&mut data.ops_file, &mut data.build_file] {
            if let Some(file_name) = slot.as_deref() {
                let (potentially_relative_path, should_update) =
                    try_make_relative(file_name, Some(design_dir));
                if should_update {
                    *slot = Some(potentially_relative_path);
                }
            }
        }
    }

    serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "mechanosynth".to_string(),
        description: "Replays a mechanosynthetic build sequence onto a workpiece: the structure \
            after the first `step` positionally controlled reactions of a build script. Scrubbing \
            `step` shows the structure being built.\n\
            \n\
            Two JSON files drive it. The **operation library** (`ops_file`) names before/after \
            atom patterns in a local frame; comparing them by pattern id *is* the rewrite. The \
            **build script** (`build_file`) lists steps, each naming an operation and a rigid \
            transform placing it into workpiece coordinates. Both are written by generators, not \
            by hand.\n\
            \n\
            Matching is nearest-atom-within-tolerance on position and element; bonds take no part \
            in it. A step that fails to match aborts evaluation with a message naming the step, \
            the operation and the nearest atom — the partial state is reachable by setting `step` \
            one lower. Added atoms land exactly where the operation says: the node never relaxes, \
            so wire `relax` downstream if a settled geometry is wanted.\n\
            \n\
            The atoms of the current step carry the `ms_current` tag, which `apply_style` can \
            colour; when a step deletes an atom, the atoms it was bonded to carry the tag in its \
            place. Every atom the applied steps created carries `ms_added`, and those created by \
            the terrace under construction carry `ms_layer`.\n\
            \n\
            The second output pin, `step`, carries a `MechanosynthStep` record describing the \
            **last step applied** — `index`, `count`, `op`, `note`, the script's own `method`, \
            `phase`, `layer` and `site` metadata, and the placement point `t` — so a `switch`, an \
            `expr` or a `record_destructure` downstream can act on the step rather than parse its \
            note. File paths are stored relative to the project file whenever possible so a \
            copied project keeps working."
            .to_string(),
        summary: Some("Replay a build sequence".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "base".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "ops_file".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "build_file".to_string(),
                data_type: DataType::String,
            },
            Parameter {
                id: None,
                name: "step".to_string(),
                data_type: DataType::Int,
            },
        ],
        output_pins: vec![
            OutputPinDefinition::same_as_input("result", "base"),
            // Appended, never inserted: pin 0 stays the workpiece so saved
            // projects keep their wires (`doc/design_multi_output_pins.md`).
            OutputPinDefinition::fixed(
                "step",
                DataType::Record(RecordType::Named(MECHANOSYNTH_STEP_RECORD.to_string())),
            ),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(MechanosynthData::new()),
        node_data_saver: mechanosynth_data_saver,
        node_data_loader: mechanosynth_data_loader,
    }
}
