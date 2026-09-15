//! `mechanosynth` — replays a mechanosynthetic build sequence onto a workpiece.
//!
//! The whole rewrite engine lives in `atomcad_crystolecule::mechanosynth`; this
//! node is the thin wrapper that gives it a place in the network. See
//! `design_mechanosynth_node.md` (external, in the mechanosynth working folder),
//! Track A P2, and `doc/design_mechanosynth_editor.md` for the wired-value
//! conversion.
//!
//! **The library and the steps arrive on wires** (`ops: OpLibrary`,
//! `steps: [BuildStep]`), produced by `ops_library` and `build_script` or by
//! any array plumbing. The two file properties are **deprecated** and kept as
//! the fallback for a project saved before the pins existed: with the pin
//! unwired and the property set, the node reads the file exactly as it used
//! to. A wired pin wins and the panel hides the property's field. Nothing
//! converts a legacy node automatically — the panel offers a **Convert to
//! nodes** button, because graph surgery the user did not ask for is worse
//! than a stale property.
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
use crate::evaluator::network_result::{NetworkResult, first_array_element_error};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::build_step::{BUILD_STEP_RECORD, steps_from_array};
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::mechanosynth::{
    BuildScript, HighlightTags, MechanosynthError, NO_LAYER, NO_SITE, OpLibrary, Scene,
    load_build_script, load_library, replay_scene, steps_applied,
};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};
use glam::DMat3;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};

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

/// The atom tag every atom of every wired tool molecule carries in `scene`.
/// Tool and feedstock atoms are told apart by these rather than by `ms_added`,
/// which means "what the build created **on the workpiece**"
/// (`doc/design_mechanosynth_tools.md`).
pub const MS_TOOL_TAG: &str = "ms_tool";

/// The atom tag every atom of every wired reservoir carries in `scene`.
pub const MS_FEEDSTOCK_TAG: &str = "ms_feedstock";

/// The named record type of the `step` output pin. Registered in
/// `node_type_registry.rs` beside `Patch` and `MaterializeRegion`.
pub const MECHANOSYNTH_STEP_RECORD: &str = "MechanosynthStep";

/// Index of the `step` output pin. **Appended** (pin 1), never inserted, so
/// saved projects and existing wires keep their pin indices
/// (`doc/design_multi_output_pins.md`).
pub const STEP_OUTPUT_PIN: usize = 1;

/// Index of the `scene` output pin — the merged scene (base, feedstocks,
/// tools) after the step. **Appended** (pin 2) for the same reason: making it
/// pin 0 would have got the display default for free and silently rewired
/// every existing file's `result` consumers to the scene.
pub const SCENE_OUTPUT_PIN: usize = 2;

/// Input pin indices. The two file-name pins that used to sit at 1 and 2 are
/// gone; the *properties* behind them are not (see the module doc), so a saved
/// project keeps replaying while a wire into either slot now carries a value
/// instead of a path.
pub const OPS_PIN: usize = 1;
pub const STEPS_PIN: usize = 2;
pub const STEP_PIN: usize = 3;
/// **Appended** (pin 4 / pin 5), both optional and wire-only.
pub const FEEDSTOCKS_PIN: usize = 4;
pub const TOOLS_PIN: usize = 5;

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
#[derive(Debug, Serialize, Deserialize)]
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
    /// The scene the most recent successful evaluation produced.
    ///
    /// The panel's *Tools* and *Feedstocks* readouts are facts about the tool
    /// bindings and the participant map, and neither survives the trip through
    /// a pin — so `eval` parks the whole scene here and the readout reads it
    /// back. Deliberately **not** forced: the panel reads whatever the last
    /// evaluation left, so a panel rebuild never costs a replay, and a node
    /// that has not been evaluated simply reports no tools.
    #[serde(skip)]
    pub last_scene: Mutex<Option<Scene>>,
}

impl Clone for MechanosynthData {
    fn clone(&self) -> Self {
        Self {
            ops_file: self.ops_file.clone(),
            build_file: self.build_file.clone(),
            step: self.step,
            library: self.library.clone(),
            script: self.script.clone(),
            load_error: self.load_error.clone(),
            last_scene: Mutex::new(self.last_scene.lock().ok().and_then(|slot| slot.clone())),
        }
    }
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
            last_scene: Mutex::new(None),
        }
    }

    fn record_last_scene(&self, scene: Option<Scene>) {
        if let Ok(mut slot) = self.last_scene.lock() {
            *slot = scene;
        }
    }

    /// The scene the most recent successful evaluation produced, for the
    /// panel's *Tools* and *Feedstocks* readouts.
    pub fn last_scene(&self) -> Option<Scene> {
        self.last_scene.lock().ok().and_then(|slot| slot.clone())
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

    /// The library to replay with: the one that arrived on the `ops` wire,
    /// else the deprecated property's parsed cache, else that property's file
    /// re-read from disk.
    fn resolve_library(
        &self,
        wired: Option<Arc<OpLibrary>>,
        design_dir: Option<&str>,
    ) -> Result<Cow<'_, OpLibrary>, String> {
        if let Some(library) = wired {
            // One clone per evaluation of a value the `Arc` otherwise shares.
            // `replay` wants a `&OpLibrary` outliving the call, and a library
            // is small; the alternative is threading the `Arc` through `Cow`,
            // which buys nothing here.
            return Ok(Cow::Owned((*library).clone()));
        }
        if let Some(library) = &self.library {
            return Ok(Cow::Borrowed(library));
        }
        match &self.ops_file {
            Some(name) => load_library_at(name, design_dir).map(Cow::Owned),
            None => Err("no operation library (wire the ops pin)".to_string()),
        }
    }

    /// The steps to replay. Same precedence as [`Self::resolve_library`], with
    /// one difference: a node with neither a wire nor a property replays
    /// **nothing** rather than failing, so a half-wired node still displays its
    /// base instead of going red while the user is still building the graph.
    fn resolve_script(
        &self,
        wired: Option<Vec<atomcad_crystolecule::mechanosynth::Step>>,
        design_dir: Option<&str>,
    ) -> Result<Cow<'_, BuildScript>, String> {
        if let Some(steps) = wired {
            return Ok(Cow::Owned(BuildScript {
                // The label errors name. A wired array has no file, and
                // "steps" is what the pin is called.
                file: "steps".to_string(),
                tolerance: None,
                steps,
            }));
        }
        if let Some(script) = &self.script {
            return Ok(Cow::Borrowed(script));
        }
        match &self.build_file {
            Some(name) => load_script_at(name, design_dir).map(Cow::Owned),
            None => Ok(Cow::Owned(BuildScript {
                file: "steps".to_string(),
                tolerance: None,
                steps: Vec::new(),
            })),
        }
    }

    /// Whether either deprecated file property is set. The panel shows its
    /// **Convert to nodes** button exactly when this is true.
    pub fn has_legacy_files(&self) -> bool {
        self.ops_file.is_some() || self.build_file.is_some()
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
            return all_pins(input_val);
        }
        let mut wrapper = match input_val {
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_) => input_val,
            other => {
                return all_pins(error(format!(
                    "expected atomic input, got {:?}",
                    other.infer_data_type()
                )));
            }
        };

        let design_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|design_path| get_parent_directory(design_path));

        let wired_ops = match network_evaluator.evaluate_arg(
            network_stack,
            node_id,
            registry,
            context,
            OPS_PIN,
        ) {
            NetworkResult::None => None,
            NetworkResult::OpLibrary(library) => Some(library),
            propagated @ NetworkResult::Error(_) => return all_pins(propagated),
            other => {
                return all_pins(error(format!(
                    "expected an OpLibrary on the ops pin, got {:?}",
                    other.infer_data_type()
                )));
            }
        };
        let wired_steps = match network_evaluator.evaluate_arg(
            network_stack,
            node_id,
            registry,
            context,
            STEPS_PIN,
        ) {
            NetworkResult::None => None,
            propagated @ NetworkResult::Error(_) => return all_pins(propagated),
            array => match steps_from_array(&array) {
                Ok(steps) => Some(steps),
                Err(message) => return all_pins(error(message)),
            },
        };
        let step = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            STEP_PIN,
            self.step,
            NetworkResult::extract_int,
        ) {
            Ok(step) => step,
            Err(propagated) => return all_pins(propagated),
        };

        // The two participant pins. Unwired is an empty list, which is the
        // workpiece-only replay: no binding runs and no tool state is tracked.
        let feedstocks = match participants(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            FEEDSTOCKS_PIN,
            "feedstocks",
        ) {
            Ok(structures) => structures,
            Err(failure) => return all_pins(*failure),
        };
        let tools = match participants(
            network_evaluator,
            network_stack,
            node_id,
            registry,
            context,
            TOOLS_PIN,
            "tools",
        ) {
            Ok(structures) => structures,
            Err(failure) => return all_pins(*failure),
        };

        let library = match self.resolve_library(wired_ops, design_dir.as_deref()) {
            Ok(library) => library,
            Err(message) => return all_pins(error(message)),
        };
        let script = match self.resolve_script(wired_steps, design_dir.as_deref()) {
            Ok(script) => script,
            Err(message) => return all_pins(error(message)),
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
            tool: Some(MS_TOOL_TAG),
            feedstock: Some(MS_FEEDSTOCK_TAG),
        };
        match replay_scene(atoms, &feedstocks, &tools, &library, &script, step, tags) {
            Ok(scene) => {
                // `result` is the **workpiece alone**; `scene` is everything.
                // Both keep `base`'s variant, the way `atom_union` does.
                let mut scene_wrapper = wrapper.clone();
                *atoms_of(&mut wrapper) = scene.workpiece();
                *atoms_of(&mut scene_wrapper) = scene.structure.clone();
                // The record is built from the same script and the same clamp
                // the replay just used, so the second pin costs no second
                // replay.
                let record = step_record(&script, &library, step, &scene);
                self.record_last_scene(Some(scene));
                EvalOutput::multi(vec![wrapper, record, scene_wrapper])
            }
            Err(failure) => {
                self.record_last_scene(None);
                all_pins(error(failure.to_string()))
            }
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    /// **`scene`, not `result`.** `result` and the base part of `scene` are the
    /// same atoms, so showing both draws the workpiece twice; what a user
    /// scrubbing a build with tools wants to look at is the scene. With nothing
    /// wired to the two participant pins it is the same atoms as `result`, so
    /// the default costs a node without tools nothing.
    fn default_displayed_output_pins(&self) -> Option<HashSet<i32>> {
        Some(HashSet::from([SCENE_OUTPUT_PIN as i32]))
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        let mut parts = Vec::new();
        if !connected_input_pins.contains("steps")
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

/// The same result on **all three** output pins.
///
/// The outputs are one evaluation, so whatever stops the workpiece from being
/// produced stops the record and the scene too: a `MechanosynthStep` whose
/// `index` described a replay that failed would be a lie, and `None` on the pin
/// would be a silent one.
fn all_pins(result: NetworkResult) -> EvalOutput {
    EvalOutput::multi(vec![result.clone(), result.clone(), result])
}

/// The atoms inside a `Crystal` / `Molecule` wrapper.
fn atoms_of(wrapper: &mut NetworkResult) -> &mut AtomicStructure {
    match wrapper {
        NetworkResult::Crystal(crystal) => &mut crystal.atoms,
        NetworkResult::Molecule(molecule) => &mut molecule.atoms,
        _ => unreachable!("only the two atomic variants reach here"),
    }
}

/// One of the two participant pins as a list of structures.
///
/// Both are `[HasAtoms]`, optional and wire-only: `None` is the empty list.
/// The single-structure broadcast to a one-element array is the evaluator's own
/// rule, so nothing here has to know about it. An element that is itself an
/// error forwards verbatim rather than being replaced by a type complaint
/// (`nodes/AGENTS.md` §Errors).
pub(crate) fn participants<'a>(
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
    pin: usize,
    pin_name: &str,
) -> Result<Vec<AtomicStructure>, Box<NetworkResult>> {
    // The error is boxed: `NetworkResult` is a wide enum (a `Crystal` variant
    // alone is over a kilobyte), so an unboxed `Err` would make every `Ok`
    // return pay for it.
    match network_evaluator.evaluate_arg(network_stack, node_id, registry, context, pin) {
        NetworkResult::None => Ok(Vec::new()),
        propagated @ NetworkResult::Error(_) => Err(Box::new(propagated)),
        NetworkResult::Array(elements) => {
            if let Some(failure) = first_array_element_error(pin_name, &elements) {
                return Err(Box::new(failure));
            }
            let mut structures = Vec::with_capacity(elements.len());
            for element in elements {
                match element.extract_atomic() {
                    Some(structure) => structures.push(structure),
                    None => {
                        return Err(Box::new(error(format!(
                            "every {pin_name} entry must be an atomic structure"
                        ))));
                    }
                }
            }
            Ok(structures)
        }
        other => Err(Box::new(error(format!(
            "expected an array of atomic structures on the {pin_name} pin, got {:?}",
            other.infer_data_type()
        )))),
    }
}

/// The `MechanosynthStep` record describing the **last step applied** — the
/// same "current step" the property panel names.
///
/// At `index = 0` nothing has run, so the step-specific fields take their
/// absent-field defaults rather than describing `steps[0]`, which has *not*
/// been applied yet.
fn step_record(
    script: &BuildScript,
    library: &OpLibrary,
    step: i32,
    scene: &Scene,
) -> NetworkResult {
    let count = script.steps.len();
    let index = steps_applied(step, count);
    let current = index.checked_sub(1).and_then(|last| script.steps.get(last));
    // `method` is the **operation's** kind now, not a string the step typed:
    // a step names a reaction, and how the reaction is performed is a fact
    // about the reaction. See `doc/design_mechanosynth_tools.md`.
    let operation = current.and_then(|step| library.get(&step.op));

    let text = |value: Option<&str>| NetworkResult::String(value.unwrap_or_default().to_string());
    // The instrument is the operation's, and only a `tip` operation has one.
    let tool_type = operation
        .and_then(|op| op.tool.as_ref())
        .map(|tool| tool.tool_type.as_str());

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
            text(operation.map(|op| op.method.as_str())),
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
        // So a downstream network can *orient* a gadget at the reaction site
        // and not only place it. The identity when no step has been applied,
        // matching the absent-`r` default everywhere else.
        (
            "r".to_string(),
            NetworkResult::Mat3(current.map_or(DMat3::IDENTITY, |s| s.r)),
        ),
        // Appended: the tool model's three facts about the step. `tool_type`
        // and `tool_state` are the `tip` operation's instrument and the state
        // it is in **after** the step; `agent` is a `bulk` operation's species.
        // Empty strings where absent, as for every other field.
        ("tool_type".to_string(), text(tool_type)),
        (
            "tool_state".to_string(),
            text(tool_type.and_then(|name| {
                scene
                    .binding_of(name)
                    .and_then(|binding| binding.state.as_deref())
            })),
        ),
        (
            "agent".to_string(),
            text(operation.and_then(|op| op.agent.as_deref())),
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
            Two values drive it. The **operation library** on the `ops` pin (from `ops_library`) \
            names before/after atom patterns in a local frame; comparing them by pattern id *is* \
            the rewrite. The **steps** on the `steps` pin are an array of `BuildStep` records, \
            each naming an operation and a rigid transform placing it into workpiece \
            coordinates — from `build_script`, from the array nodes, or from both concatenated. \
            With `steps` unwired the node replays nothing and emits the base.\n\
            \n\
            The match tolerance is the library's own, so a generated library pins it in one \
            place.\n\
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
            **last step applied** — `index`, `count`, `op`, `note`, the operation's `method` \
            (`tip` / `bulk` / `spontaneous`), `phase`, `layer` and `site` metadata, the \
            placement point `t` and rotation `r`, and the tool model's `tool_type`, \
            `tool_state` (after the step) and `agent` — so a `switch`, an `expr` or a \
            `record_destructure` downstream can act on the step rather than parse its note.\n\
            \n\
            The `feedstocks` pin takes the reservoirs a build draws on and dumps to, and the \
            `tools` pin the tool molecules that perform it — each identified by the atom tag \
            naming its type in the library, and posed by the four atoms tagged with the type's \
            frame tags. Both are optional: with `tools` unwired no tool is bound and no tool \
            state is tracked, which is how a library is developed before its instruments exist. \
            Every wired entry must have the phase of `base`; wire one through `enter_structure` \
            or `exit_structure` otherwise.\n\
            \n\
            The third output pin, `scene`, is everything at once — the workpiece, the reservoirs \
            and the tools as one structure, with tool atoms tagged `ms_tool` and reservoir atoms \
            `ms_feedstock`. `result` stays the **workpiece alone**, so an export or a count \
            downstream of it never picks up an atom sitting on a tip. A freshly placed node \
            shows `scene`.\n\
            \n\
            The `ops_file` and `build_file` **properties are deprecated**. A project saved \
            before the pins existed keeps replaying from them, and the panel offers a **Convert \
            to nodes** button that builds the `ops_library` / `build_script` nodes, wires them \
            in and clears the properties. A wired pin always wins over the property."
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
                name: "ops".to_string(),
                data_type: DataType::OpLibrary,
            },
            Parameter {
                id: None,
                name: "steps".to_string(),
                data_type: DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    BUILD_STEP_RECORD.to_string(),
                )))),
            },
            Parameter {
                id: None,
                name: "step".to_string(),
                data_type: DataType::Int,
            },
            // Appended, both optional and wire-only. Every wired entry must
            // have the phase of `base` — the node states that rule itself
            // (`network_validator.rs`), because neither pin takes its output
            // type from the array's elements.
            Parameter {
                id: None,
                name: "feedstocks".to_string(),
                data_type: DataType::Array(Box::new(DataType::HasAtoms)),
            },
            Parameter {
                id: None,
                name: "tools".to_string(),
                data_type: DataType::Array(Box::new(DataType::HasAtoms)),
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
            // The merged scene, keeping `base`'s variant the way `atom_union`
            // does. Appended rather than made pin 0 for the same reason: wires
            // and displayed-pin sets are persisted by index, and a bare `build`
            // in the text format would silently become the scene.
            OutputPinDefinition::same_as_input("scene", "base"),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(MechanosynthData::new()),
        node_data_saver: mechanosynth_data_saver,
        node_data_loader: mechanosynth_data_loader,
    }
}
