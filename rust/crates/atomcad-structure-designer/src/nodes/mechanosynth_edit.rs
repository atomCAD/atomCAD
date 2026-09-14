//! `mechanosynth_edit` — authors a build script by clicking atoms.
//!
//! `mechanosynth` replays; this node edits. The split is the same one
//! `atom_edit` / `apply_diff` makes, and it exists because the two jobs give
//! the `result` pin different meanings — "the state after N steps, with replay
//! highlights" against "the state at the cursor, with ghost previews" — which
//! inside one node would have to become a mode flag that changes what a wire
//! carries. See `doc/design_mechanosynth_editor.md` §Decisions.
//!
//! Four things about the data model are load-bearing:
//!
//! - **The authored block comes *after* the wired prefix.** The `steps` pin is
//!   a prefix, the stored [`MechanosynthEditData::authored`] block follows it,
//!   and the cursor can only sit inside the block. To author between two
//!   generated blocks, chain two editors through `result`. The rejected
//!   alternative — an edit list applied *onto* the wired steps, "insert at
//!   index k" — breaks silently whenever the upstream block changes length,
//!   which is the same drift problem absolute-coordinate diffs have.
//! - **The cursor is not undoable**, exactly as the replayer's slider is not.
//!   It is node data so it persists, but a scrub is navigation, not an edit.
//! - **Each authored step stores its fit `residual` and `approximate` flag**,
//!   and neither travels on the `steps` wire: they are the editor's evidence
//!   about how the step was placed, not part of the step. A design whose steps
//!   are all exact is exact to file rounding (§Exactness).
//! - **The placement state is transient.** The armed operation, the last offer
//!   list and the pending candidates are `#[serde(skip)]`: a saved project
//!   carries the block and the cursor and nothing about the tool.
//!
//! `ms_current` is the only tag this node paints, on the cursor step's touched
//! atoms. `ms_added` / `ms_layer` are the replayer's concern — they describe a
//! whole build, and the editor's result is a work in progress.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::{NetworkResult, dmat3_to_rows, rows_to_dmat3};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::build_step::{
    BUILD_STEP_RECORD, build_step_record, is_identity_rotation, steps_from_array,
};
use crate::nodes::mechanosynth::MS_CURRENT_TAG;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::mechanosynth::{
    Applicability, BuildScript, Candidate, EXACT_FIT_RESIDUAL, HighlightTags, NO_LAYER, NO_SITE,
    OpLibrary, Step, replay, steps_applied,
};
use glam::{DMat3, DVec3};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex};

/// Input pin indices.
pub const BASE_PIN: usize = 0;
pub const OPS_PIN: usize = 1;
pub const STEPS_PIN: usize = 2;

/// Index of the `steps` output pin. **Appended**, never inserted, so pin 0
/// stays the workpiece (`doc/design_multi_output_pins.md`).
pub const STEPS_OUTPUT_PIN: usize = 1;

/// The label errors use for a step array that arrived on a wire. A wired array
/// has no file, and `steps` is what the pin is called.
const PREFIX_LABEL: &str = "steps";

/// The label errors use for the stored block.
const AUTHORED_LABEL: &str = "authored";

/// One step of the authored block: the step itself plus the evidence about how
/// it was placed.
///
/// The two extra fields are **node data, not `BuildStep` fields**. They say what
/// the fit was worth, which is a fact about this editing session rather than
/// about the reaction, so `export_build_script` cannot see them and does not
/// try to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "AuthoredStepJson", from = "AuthoredStepJson")]
pub struct AuthoredStep {
    pub step: Step,
    /// Max per-atom residual of the fit that placed it, Å. `0.0` for a step
    /// typed by hand — the author's assertion, which has the same standing a
    /// generated file's step has.
    pub residual: f64,
    /// The orientation came from the host's bonds rather than from the
    /// library's frame atoms.
    pub approximate: bool,
}

impl AuthoredStep {
    /// A step placed by the tool from one of `place`'s candidates.
    pub fn from_candidate(candidate: &Candidate) -> Self {
        Self {
            step: candidate.step.clone(),
            residual: candidate.residual,
            approximate: candidate.approximate,
        }
    }

    /// Whether the fit is exact to the generator's own congruence threshold.
    /// A step with no residual counts as exact, which is what makes a
    /// hand-typed step exact.
    pub fn is_exact(&self) -> bool {
        self.residual < EXACT_FIT_RESIDUAL
    }

    /// Copies the build metadata — not the note, which is about one step — from
    /// `source`. This is what a commit does with the previous step.
    pub(crate) fn inherit_metadata_from(&mut self, source: &Step) {
        self.step.method = source.method.clone();
        self.step.phase = source.phase.clone();
        self.step.layer = source.layer;
        self.step.site = source.site;
    }
}

/// The persisted shape of an [`AuthoredStep`]. `Step` itself derives no serde
/// (the engine keeps "what a file may say" separate from "what the engine may
/// assume"), so the conversion lives here, and it omits every absent field so a
/// short step stays short in the `.cnnd`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthoredStepJson {
    op: String,
    t: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    r: Option<[[f64; 3]; 3]>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    note: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    method: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    phase: String,
    #[serde(default = "absent_layer", skip_serializing_if = "is_absent_layer")]
    layer: i32,
    #[serde(default = "absent_site", skip_serializing_if = "is_absent_site")]
    site: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    residual: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    approximate: bool,
}

fn absent_layer() -> i32 {
    NO_LAYER
}
fn absent_site() -> i32 {
    NO_SITE
}
fn is_absent_layer(layer: &i32) -> bool {
    *layer == NO_LAYER
}
fn is_absent_site(site: &i32) -> bool {
    *site == NO_SITE
}
fn is_zero(value: &f64) -> bool {
    *value == 0.0
}
fn is_false(value: &bool) -> bool {
    !*value
}

impl From<AuthoredStep> for AuthoredStepJson {
    fn from(authored: AuthoredStep) -> Self {
        let step = authored.step;
        Self {
            op: step.op,
            t: step.t.to_array(),
            r: (!is_identity_rotation(&step.r)).then(|| dmat3_to_rows(&step.r)),
            note: step.note.unwrap_or_default(),
            method: step.method,
            phase: step.phase,
            layer: step.layer,
            site: step.site,
            residual: authored.residual,
            approximate: authored.approximate,
        }
    }
}

impl From<AuthoredStepJson> for AuthoredStep {
    fn from(json: AuthoredStepJson) -> Self {
        Self {
            step: Step {
                op: json.op,
                t: DVec3::from_array(json.t),
                r: json.r.map_or(DMat3::IDENTITY, |rows| rows_to_dmat3(&rows)),
                note: (!json.note.is_empty()).then_some(json.note),
                method: json.method,
                phase: json.phase,
                layer: json.layer,
                site: json.site,
            },
            residual: json.residual,
            approximate: json.approximate,
        }
    }
}

/// Which of the placement tool's states the node is in. Derived from
/// [`PlacementState`] rather than stored beside it, so the two cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Idle,
    Armed,
    Offers,
    Candidates,
}

impl ToolState {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolState::Idle => "idle",
            ToolState::Armed => "armed",
            ToolState::Offers => "offers",
            ToolState::Candidates => "candidates",
        }
    }
}

/// The placement tool's transient state. Never serialized: a saved project
/// carries the authored block and the cursor, and nothing about a click.
#[derive(Debug, Clone, Default)]
pub struct PlacementState {
    /// The operation the tool is armed with, so a run of identical placements
    /// is one click each.
    pub armed: Option<String>,
    /// The atom the current offer list or candidate list was taken on — the
    /// popup's anchor.
    pub anchor: Option<u32>,
    /// The last applicability sweep, kept whole so highlighting a row previews
    /// it without a second `place` call.
    pub offers: Vec<Applicability>,
    /// The candidates of the chosen row, awaiting a choice.
    pub candidates: Vec<Candidate>,
    /// Which operation [`Self::candidates`] belongs to.
    pub candidates_op: Option<String>,
}

impl PlacementState {
    pub fn state(&self) -> ToolState {
        if !self.candidates.is_empty() {
            ToolState::Candidates
        } else if !self.offers.is_empty() {
            ToolState::Offers
        } else if self.armed.is_some() {
            ToolState::Armed
        } else {
            ToolState::Idle
        }
    }

    /// Back to Idle: no anchor, no offers, no pending candidates. The armed
    /// operation is cleared too, because Escape means "stop".
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Clears everything a click produced but keeps the armed operation, which
    /// is what a commit does.
    pub fn clear_query(&mut self) {
        self.anchor = None;
        self.offers.clear();
        self.candidates.clear();
        self.candidates_op = None;
    }
}

/// Stored data of a `mechanosynth_edit` node.
#[derive(Debug, Serialize, Deserialize)]
pub struct MechanosynthEditData {
    /// The authored block, applied after whatever arrives on the `steps` pin.
    #[serde(default)]
    pub authored: Vec<AuthoredStep>,
    /// How many authored steps `result` shows. `-1` means "all" and follows the
    /// block as it grows; anything past the end clamps, exactly as the
    /// replayer's slider does.
    #[serde(default = "default_cursor")]
    pub cursor: i32,

    #[serde(skip)]
    pub placement: PlacementState,
    /// The most recent evaluation failure, for the panel. Behind a `Mutex`
    /// because `eval` takes `&self` (the `atom_edit` cache pattern).
    #[serde(skip)]
    pub last_error: Mutex<Option<String>>,
}

/// A fresh editor shows its whole block, which is the useful default: the
/// cursor only stops following the end when the user scrubs it.
pub fn default_cursor() -> i32 {
    -1
}

/// **Not** `#[derive(Default)]`: the derive would give `cursor` a `0`, and the
/// `#[serde(default)]` attribute above only covers *deserialization*. A node
/// created from the palette would then show its prefix and none of its block,
/// for no reason a user could see.
impl Default for MechanosynthEditData {
    fn default() -> Self {
        Self {
            authored: Vec::new(),
            cursor: default_cursor(),
            placement: PlacementState::default(),
            last_error: Mutex::new(None),
        }
    }
}

impl Clone for MechanosynthEditData {
    fn clone(&self) -> Self {
        Self {
            authored: self.authored.clone(),
            cursor: self.cursor,
            placement: self.placement.clone(),
            last_error: Mutex::new(self.last_error.lock().ok().and_then(|slot| slot.clone())),
        }
    }
}

impl MechanosynthEditData {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many authored steps the stored cursor applies.
    pub fn applied(&self) -> usize {
        steps_applied(self.cursor, self.authored.len())
    }

    /// The authored block as engine steps.
    pub fn authored_steps(&self) -> Vec<Step> {
        self.authored
            .iter()
            .map(|authored| authored.step.clone())
            .collect()
    }

    /// How many authored steps were placed with a residual above the exact
    /// threshold, and how many came from the bond-derived fallback — the two
    /// counts the panel reports above the list.
    pub fn inexact_counts(&self) -> (usize, usize) {
        (
            self.authored
                .iter()
                .filter(|authored| !authored.is_exact())
                .count(),
            self.authored
                .iter()
                .filter(|authored| authored.approximate)
                .count(),
        )
    }

    fn record_error(&self, message: Option<String>) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = message;
        }
    }

    /// The most recent evaluation failure, if the last evaluation failed.
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|slot| slot.clone())
    }
}

/// A one-off script wrapping a step list, for [`replay`].
fn script(file: &str, steps: Vec<Step>) -> BuildScript {
    BuildScript {
        file: file.to_string(),
        tolerance: None,
        steps,
    }
}

/// Replays `prefix` and then the first `cursor` steps of `authored` onto
/// `base`, painting `ms_current` on the last **authored** step's touched atoms.
///
/// Two replays rather than one concatenated script, and that is the whole point:
/// a single call would paint the highlight on the last *prefix* step whenever
/// the cursor sits at 0, which is exactly the state that must show no highlight
/// at all.
pub fn replay_prefix_and_block(
    base: &atomcad_crystolecule::atomic_structure::AtomicStructure,
    library: &OpLibrary,
    prefix: Vec<Step>,
    authored: Vec<Step>,
    cursor: i32,
) -> Result<atomcad_crystolecule::atomic_structure::AtomicStructure, String> {
    let after_prefix = replay(
        base,
        library,
        &script(PREFIX_LABEL, prefix),
        -1,
        HighlightTags::default(),
    )
    .map_err(|failure| failure.to_string())?;
    replay(
        &after_prefix,
        library,
        &script(AUTHORED_LABEL, authored),
        cursor,
        HighlightTags {
            current: Some(MS_CURRENT_TAG),
            added: None,
            layer: None,
        },
    )
    .map_err(|failure| failure.to_string())
}

impl NodeData for MechanosynthEditData {
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
        let input_val = network_evaluator.evaluate_arg_required(
            network_stack,
            node_id,
            registry,
            context,
            BASE_PIN,
        );
        if input_val.is_error() {
            return both(input_val);
        }
        let mut wrapper = match input_val {
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_) => input_val,
            other => {
                return self.fail(format!(
                    "expected atomic input, got {:?}",
                    other.infer_data_type()
                ));
            }
        };

        let library: Arc<OpLibrary> = match network_evaluator.evaluate_arg(
            network_stack,
            node_id,
            registry,
            context,
            OPS_PIN,
        ) {
            NetworkResult::OpLibrary(library) => library,
            NetworkResult::None => {
                return self.fail("no operation library (wire the ops pin)".to_string());
            }
            propagated @ NetworkResult::Error(_) => return both(propagated),
            other => {
                return self.fail(format!(
                    "expected an OpLibrary on the ops pin, got {:?}",
                    other.infer_data_type()
                ));
            }
        };

        let prefix = match network_evaluator.evaluate_arg(
            network_stack,
            node_id,
            registry,
            context,
            STEPS_PIN,
        ) {
            NetworkResult::None => Vec::new(),
            propagated @ NetworkResult::Error(_) => return both(propagated),
            array => match steps_from_array(&array) {
                Ok(steps) => steps,
                Err(message) => return self.fail(message),
            },
        };

        let atoms = match &mut wrapper {
            NetworkResult::Crystal(crystal) => &mut crystal.atoms,
            NetworkResult::Molecule(molecule) => &mut molecule.atoms,
            _ => unreachable!("the match above admitted only these two"),
        };

        // The `steps` output is the whole block whatever the cursor: the cursor
        // says what to *show*, never what the node hands downstream.
        let all_steps: Vec<Step> = prefix
            .iter()
            .cloned()
            .chain(self.authored_steps())
            .collect();
        let steps_output = NetworkResult::Array(all_steps.iter().map(build_step_record).collect());

        match replay_prefix_and_block(atoms, &library, prefix, self.authored_steps(), self.cursor) {
            Ok(result) => {
                *atoms = result;
                self.record_error(None);
                EvalOutput::multi(vec![wrapper, steps_output])
            }
            Err(message) => self.fail(message),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        if self.authored.is_empty() {
            return None;
        }
        Some(format!("{} / {}", self.applied(), self.authored.len()))
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut metadata = HashMap::new();
        metadata.insert("base".to_string(), (true, None));
        metadata.insert("ops".to_string(), (true, None));
        metadata
    }

    /// **Total**, deliberately: a property omitted here is treated as wire-only
    /// by the text editor and a literal for it is silently dropped, so an empty
    /// block must still be written as `[]` (`project_text_format_roundtrip`).
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("cursor".to_string(), TextValue::Int(self.cursor)),
            (
                "authored".to_string(),
                TextValue::Array(self.authored.iter().map(step_to_text).collect()),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(value) = props.get("cursor") {
            self.cursor = value
                .as_int()
                .ok_or_else(|| "cursor must be an integer".to_string())?;
        }
        if let Some(value) = props.get("authored") {
            let TextValue::Array(elements) = value else {
                return Err("authored must be an array of step literals".to_string());
            };
            self.authored = elements
                .iter()
                .enumerate()
                .map(|(index, element)| step_from_text(element, index + 1))
                .collect::<Result<Vec<_>, String>>()?;
        }
        Ok(())
    }
}

impl MechanosynthEditData {
    /// Records `message` as the node's last error and returns it on both pins.
    fn fail(&self, message: String) -> EvalOutput {
        self.record_error(Some(message.clone()));
        both(NetworkResult::Error(format!(
            "mechanosynth_edit: {message}"
        )))
    }
}

/// The same result on both output pins: the two outputs are one evaluation, so
/// whatever stops the workpiece stops the step array too.
fn both(result: NetworkResult) -> EvalOutput {
    EvalOutput::multi(vec![result.clone(), result])
}

// ============================================================================
// Text format
// ============================================================================

/// One authored step as a record literal. An identity `r`, empty metadata, an
/// exact residual and `approximate: false` are omitted, so a short step stays
/// short and a step typed by hand counts as exact.
fn step_to_text(authored: &AuthoredStep) -> TextValue {
    let step = &authored.step;
    let mut fields = vec![
        ("op".to_string(), TextValue::String(step.op.clone())),
        ("t".to_string(), TextValue::Vec3(step.t)),
    ];
    if !is_identity_rotation(&step.r) {
        fields.push(("r".to_string(), TextValue::Mat3(dmat3_to_rows(&step.r))));
    }
    if let Some(note) = step.note.as_ref().filter(|note| !note.is_empty()) {
        fields.push(("note".to_string(), TextValue::String(note.clone())));
    }
    if !step.method.is_empty() {
        fields.push(("method".to_string(), TextValue::String(step.method.clone())));
    }
    if !step.phase.is_empty() {
        fields.push(("phase".to_string(), TextValue::String(step.phase.clone())));
    }
    if step.layer != NO_LAYER {
        fields.push(("layer".to_string(), TextValue::Int(step.layer)));
    }
    if step.site != NO_SITE {
        fields.push(("site".to_string(), TextValue::Int(step.site)));
    }
    if !authored.is_exact() {
        fields.push(("residual".to_string(), TextValue::Float(authored.residual)));
    }
    if authored.approximate {
        fields.push(("approximate".to_string(), TextValue::Bool(true)));
    }
    TextValue::Object(fields)
}

/// The inverse of [`step_to_text`]. An unknown field is a parse error rather
/// than a silent drop: the block is the design's own record of a build, and a
/// misspelled `methdo:` that vanished would be worse than a message.
fn step_from_text(value: &TextValue, index: usize) -> Result<AuthoredStep, String> {
    let TextValue::Object(fields) = value else {
        return Err(format!("step {index}: expected a record literal"));
    };
    let get = |name: &str| fields.iter().find(|(key, _)| key == name).map(|(_, v)| v);
    let bad =
        |name: &str, expected: &str| format!("step {index}: field \"{name}\" must be {expected}");

    for (key, _) in fields {
        if !matches!(
            key.as_str(),
            "op" | "t"
                | "r"
                | "note"
                | "method"
                | "phase"
                | "layer"
                | "site"
                | "residual"
                | "approximate"
        ) {
            return Err(format!("step {index}: unknown field \"{key}\""));
        }
    }

    let op = match get("op") {
        Some(TextValue::String(op)) => op.clone(),
        Some(_) => return Err(bad("op", "a string")),
        None => return Err(format!("step {index}: missing field \"op\"")),
    };
    // An all-integer vector literal lexes as `IVec3`; the wire rule coerces it
    // to `Vec3` everywhere else, and a hand-typed `t: (0, 0, 0)` deserves the
    // same treatment rather than a type complaint.
    let t = match get("t") {
        Some(TextValue::Vec3(t)) => *t,
        Some(TextValue::IVec3(t)) => DVec3::new(t.x as f64, t.y as f64, t.z as f64),
        Some(_) => return Err(bad("t", "a Vec3")),
        None => return Err(format!("step {index}: missing field \"t\"")),
    };
    let r = match get("r") {
        None => DMat3::IDENTITY,
        Some(TextValue::Mat3(rows)) => rows_to_dmat3(rows),
        Some(TextValue::IMat3(rows)) => rows_to_dmat3(&rows.map(|row| row.map(|v| v as f64))),
        Some(_) => return Err(bad("r", "a Mat3")),
    };

    let text = |name: &str| -> Result<String, String> {
        match get(name) {
            None => Ok(String::new()),
            Some(TextValue::String(value)) => Ok(value.clone()),
            Some(_) => Err(bad(name, "a string")),
        }
    };
    let number = |name: &str, absent: i32| -> Result<i32, String> {
        match get(name) {
            None => Ok(absent),
            Some(TextValue::Int(value)) => Ok(*value),
            Some(_) => Err(bad(name, "an integer")),
        }
    };

    let note = text("note")?;
    let residual = match get("residual") {
        None => 0.0,
        Some(TextValue::Float(value)) => *value,
        Some(TextValue::Int(value)) => *value as f64,
        Some(_) => return Err(bad("residual", "a number")),
    };
    let approximate = match get("approximate") {
        None => false,
        Some(TextValue::Bool(value)) => *value,
        Some(_) => return Err(bad("approximate", "a boolean")),
    };

    Ok(AuthoredStep {
        step: Step {
            op,
            t,
            r,
            note: (!note.is_empty()).then_some(note),
            method: text("method")?,
            phase: text("phase")?,
            layer: number("layer", NO_LAYER)?,
            site: number("site", NO_SITE)?,
        },
        residual,
        approximate,
    })
}

// ============================================================================
// Serialization
// ============================================================================

pub fn mechanosynth_edit_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let data: MechanosynthEditData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Box::new(data))
}

pub fn mechanosynth_edit_data_saver(
    node_data: &mut dyn NodeData,
    _design_dir: Option<&str>,
) -> io::Result<Value> {
    let Some(data) = node_data
        .as_any_mut()
        .downcast_mut::<MechanosynthEditData>()
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch for mechanosynth_edit",
        ));
    };
    serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "mechanosynth_edit".to_string(),
        description: "Authors a mechanosynthetic build script by clicking atoms, and replays it \
            like `mechanosynth`.\n\
            \n\
            The `steps` input pin is a **prefix** — a generated block from `build_script`, or \
            another editor's output — and the node's own authored block comes after it. Choose an \
            operation from the wired library, click one atom of the workpiece, and the step is \
            placed exactly: the rigid transform comes from fitting the operation's `before` \
            pattern onto the atoms that are actually there, so no coordinate is typed and no \
            orientation is guessed. Click an atom with nothing armed and the library answers with \
            the operations that fit *that atom*, ranked, with the ones that nearly fit listed \
            below with how far off they are.\n\
            \n\
            `result` is the workpiece after the prefix and the authored steps **up to the \
            cursor**, with the cursor step's atoms tagged `ms_current`. The `steps` output is the \
            prefix followed by the whole authored block, whatever the cursor says, so it can be \
            wired into `mechanosynth`, `export_build_script` or the array nodes.\n\
            \n\
            Each authored step records the residual of the fit that placed it. A step whose \
            residual is above 1e-4 Å carries a warning chip, because the library's patterns did \
            not quite match the environment they were applied to; a step whose orientation had to \
            be derived from the host's bonds is flagged separately. A design with no chips \
            reproduces a generator's replay to file rounding.\n\
            \n\
            The cursor is navigation, not an edit: scrubbing it is not undoable, exactly as \
            `mechanosynth`'s `step` slider is not."
            .to_string(),
        summary: Some("Author a build sequence".to_string()),
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
        ],
        output_pins: vec![
            OutputPinDefinition::same_as_input("result", "base"),
            OutputPinDefinition::fixed(
                "steps",
                DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    BUILD_STEP_RECORD.to_string(),
                )))),
            ),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(MechanosynthEditData::new()),
        node_data_saver: mechanosynth_edit_data_saver,
        node_data_loader: mechanosynth_edit_data_loader,
    }
}
