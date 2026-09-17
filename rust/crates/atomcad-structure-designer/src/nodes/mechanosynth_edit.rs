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
use crate::nodes::mechanosynth::{MS_CURRENT_TAG, MS_FEEDSTOCK_TAG, MS_TOOL_TAG, participants};
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::atomic_structure_decorator::MechanosynthGhostVisuals;
use atomcad_crystolecule::mechanosynth::{
    Applicability, BuildScript, Candidate, EXACT_FIT_RESIDUAL, GhostAtom, GhostBond, HighlightTags,
    LandingPlan, NO_LAYER, NO_SITE, OpLibrary, Scene, Step, build_scene, replay_steps,
    steps_applied,
};
use glam::{DMat3, DVec3};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex};

/// Input pin indices.
pub const BASE_PIN: usize = 0;
pub const OPS_PIN: usize = 1;
pub const STEPS_PIN: usize = 2;
/// **Appended** (pin 3 / pin 4), both optional and wire-only — the replayer's
/// two participant pins, one index lower because this node has no `step` pin.
pub const FEEDSTOCKS_PIN: usize = 3;
pub const TOOLS_PIN: usize = 4;

/// Index of the `steps` output pin. **Appended**, never inserted, so pin 0
/// stays the workpiece (`doc/design_multi_output_pins.md`).
pub const STEPS_OUTPUT_PIN: usize = 1;

/// Index of the `scene` output pin — the merged scene at the cursor.
pub const SCENE_OUTPUT_PIN: usize = 2;

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
    Offers,
}

impl ToolState {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolState::Idle => "idle",
            ToolState::Offers => "offers",
        }
    }
}

/// The placement tool's transient state. Never serialized: a saved project
/// carries the authored block and the cursor, and nothing about a click.
#[derive(Debug, Clone, Default)]
pub struct PlacementState {
    /// The atom the current offer list or candidate list was taken on — the
    /// popup's anchor.
    pub anchor: Option<u32>,
    /// The last applicability sweep, kept whole: every row carries **all** its
    /// candidates, so the popup lists each orientation inline and choosing one
    /// costs a lookup rather than a second `place` call.
    pub offers: Vec<Applicability>,
    /// The ghost atoms of the row the user has **selected** for preview, ready
    /// for `eval(decorate)` to hand to the decorator.
    ///
    /// Held as atoms rather than as a (row, index) reference because they are
    /// computed against the workpiece the sweep ran on, and `eval` must not
    /// re-derive them: that would put a `place`-shaped cost on every
    /// evaluation, and the whole point of selecting on a *click* rather than on
    /// hover is that the preview is paid for once.
    pub preview_ghosts: Vec<GhostAtom>,
    /// The bonds the selected row would add, delete or re-order. Kept beside
    /// the atoms rather than folded in: a **bond-only** operation has no ghost
    /// atoms at all, and with nothing here it would preview as an empty scene.
    pub preview_bonds: Vec<GhostBond>,
    /// Whether [`Self::preview_ghosts`] previews a near miss, which is drawn in
    /// a warning colour and can never be placed.
    pub preview_near_miss: bool,
}

impl PlacementState {
    pub fn state(&self) -> ToolState {
        if self.offers.is_empty() {
            ToolState::Idle
        } else {
            ToolState::Offers
        }
    }

    /// Back to Idle: no anchor, no offers, no preview.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Drops the preview alone, leaving the list open.
    pub fn clear_preview(&mut self) {
        self.preview_ghosts.clear();
        self.preview_bonds.clear();
        self.preview_near_miss = false;
    }

    /// Whether a row is selected for preview. Not `preview_ghosts.is_empty()`:
    /// a bond-only operation previews with no atoms at all.
    pub fn has_preview(&self) -> bool {
        !self.preview_ghosts.is_empty() || !self.preview_bonds.is_empty()
    }
}

/// The node's three evaluated inputs, kept so an interaction that changes only
/// *this* node does not re-evaluate the chain above it.
///
/// The placement tool's preview is the case that needs it: hovering a row
/// changes nothing upstream, but `base` reaches back through a `mechanosynth`
/// replaying a hundred steps over a few thousand atoms, and paying for that on
/// every hover is what makes the list feel heavy. The evaluator's memo does not
/// help — it is scoped to a single evaluation pass, not across them.
///
/// The same `NodeData::clear_input_cache` contract `atom_edit` uses: the
/// refresh system drops this whenever upstream *may* have changed, so the cache
/// is only ever read when the refresh has vouched for it.
#[derive(Clone)]
struct CachedInputs {
    /// The `base` pin's value, atoms included and **unmutated** — `eval` clones
    /// it and replays into the clone.
    wrapper: NetworkResult,
    library: Arc<OpLibrary>,
    prefix: Vec<Step>,
    feedstocks: Vec<AtomicStructure>,
    tools: Vec<AtomicStructure>,
}

impl std::fmt::Debug for CachedInputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `NetworkResult` has no `Debug`, and a structure's worth of atoms is
        // not something a debug line wants anyway.
        f.debug_struct("CachedInputs")
            .field("prefix_steps", &self.prefix.len())
            .finish_non_exhaustive()
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
    /// Operation names the placement tool's offer sweep does not ask about.
    ///
    /// **A view filter over the wired library and nothing else**
    /// (`doc/design_mechanosynth_op_muting.md`): a muted operation still
    /// replays, still exports and still means what it means — the sweep simply
    /// does not look at it, so `commit_candidate` needs no rule about it. A
    /// name the wired library does not define is kept and ignored, which is
    /// what makes rewiring `ops` safe, and the **muted** set rather than the
    /// enabled set is what makes an operation *added* to the library later
    /// show up rather than silently vanish.
    ///
    /// A `BTreeSet` so the order is the name order: the `.cnnd` diff and the
    /// text dump then do not churn on set operations, and a bulk mute writes a
    /// dozen names at once.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub muted: BTreeSet<String>,

    #[serde(skip)]
    pub placement: PlacementState,
    /// The node's evaluated inputs, reused while the refresh system says
    /// upstream cannot have changed. See [`CachedInputs`].
    #[serde(skip)]
    cached_input: Mutex<Option<CachedInputs>>,
    /// The most recent evaluation failure, for the panel. Behind a `Mutex`
    /// because `eval` takes `&self` (the `atom_edit` cache pattern).
    #[serde(skip)]
    pub last_error: Mutex<Option<String>>,
    /// **The scene after the last successful step** of the most recent
    /// evaluation.
    ///
    /// Two jobs in one field. When the block fails at the cursor step the pins
    /// carry the error — a downstream export must never receive a silently
    /// truncated build — but the *viewport* shows this, because the failing
    /// step is the one being authored and the user has to see where it stands
    /// to fix it. And the placement tool works against it whether or not the
    /// block failed: the whole `Scene` rather than its atoms, because an offer
    /// needs the tool **bindings** and a click needs the **participant** of the
    /// atom it landed on. Evaluation-time state, never persisted.
    #[serde(skip)]
    last_scene: Mutex<Option<Scene>>,
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
            muted: BTreeSet::new(),
            placement: PlacementState::default(),
            cached_input: Mutex::new(None),
            last_error: Mutex::new(None),
            last_scene: Mutex::new(None),
        }
    }
}

impl Clone for MechanosynthEditData {
    fn clone(&self) -> Self {
        Self {
            authored: self.authored.clone(),
            cursor: self.cursor,
            muted: self.muted.clone(),
            placement: self.placement.clone(),
            cached_input: Mutex::new(self.cached_input.lock().ok().and_then(|slot| slot.clone())),
            last_error: Mutex::new(self.last_error.lock().ok().and_then(|slot| slot.clone())),
            last_scene: Mutex::new(self.last_scene.lock().ok().and_then(|slot| slot.clone())),
        }
    }
}

impl MechanosynthEditData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the node's inputs are cached — i.e. whether the next `eval`
    /// will reuse them instead of evaluating the chain above. The predicate the
    /// tests assert on, so `CachedInputs` itself stays private.
    pub fn has_cached_input(&self) -> bool {
        self.cached_input
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// Drops the cached inputs, so the next `eval` goes back to the chain.
    ///
    /// The refresh system calls this through
    /// [`NodeData::clear_input_cache`] whenever upstream may have changed;
    /// anything that reaches into this node's data *without* going through a
    /// refresh must call it too.
    pub fn invalidate_input_cache(&self) {
        if let Ok(mut slot) = self.cached_input.lock() {
            *slot = None;
        }
    }

    /// Whether the offer sweep skips `op`. The one question the mute set is
    /// ever asked — see the field's own note for what it deliberately does not
    /// decide.
    pub fn is_muted(&self, op: &str) -> bool {
        self.muted.contains(op)
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

    fn record_last_scene(&self, state: Option<Scene>) {
        if let Ok(mut slot) = self.last_scene.lock() {
            *slot = state;
        }
    }

    /// The scene after the last successful step of the most recent evaluation.
    /// `None` when nothing has been evaluated, or when the failure happened
    /// before any step could run (a bad library, a mis-tagged tool).
    pub fn last_scene(&self) -> Option<Scene> {
        self.last_scene.lock().ok().and_then(|slot| slot.clone())
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

/// Replays `prefix` and then the first `cursor` steps of `authored` onto the
/// scene built from `base`, `feedstocks` and `tools`, painting `ms_current` on
/// the last **authored** step's touched atoms.
///
/// Two passes over **one** scene rather than one concatenated script, and that
/// is the whole point: a single script would paint the highlight on the last
/// *prefix* step whenever the cursor sits at 0, which is exactly the state that
/// must show no highlight at all. Two *scenes* would be worse still — the
/// second binding would put every tool back in its initial state.
///
/// The outer `Err` is a failure before any step ran (a bad script, a mis-tagged
/// tool, a prefix step that would not replay): there is nothing to show. The
/// `Ok` carries the scene *and* the block's failure, if the block failed — the
/// scene is then the state after the last successful step, which is what the
/// editor displays while the user works on the step that is failing.
pub fn replay_prefix_and_block(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    prefix: Vec<Step>,
    authored: Vec<Step>,
    cursor: i32,
) -> Result<(Scene, Option<String>), String> {
    let mut scene =
        build_scene(base, feedstocks, tools, library).map_err(|failure| failure.to_string())?;
    // The editor's block replay lands every step like the node's and fails for
    // none of them, so the landings go unread here: an authored step on a
    // blocked site is seen in the replayer, not where it was authored.
    let (prefix_failure, _) = replay_steps(
        &mut scene,
        library,
        &script(PREFIX_LABEL, prefix),
        -1,
        HighlightTags::default(),
        LandingPlan::None,
    )
    .map_err(|failure| failure.to_string())?;
    if let Some(failure) = prefix_failure {
        return Err(failure.to_string());
    }
    let (block_failure, _) = replay_steps(
        &mut scene,
        library,
        &script(AUTHORED_LABEL, authored),
        cursor,
        HighlightTags {
            current: Some(MS_CURRENT_TAG),
            tool: Some(MS_TOOL_TAG),
            feedstock: Some(MS_FEEDSTOCK_TAG),
            ..HighlightTags::default()
        },
        LandingPlan::None,
    )
    .map_err(|failure| failure.to_string())?;
    Ok((scene, block_failure.map(|failure| failure.to_string())))
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
        decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        // The three inputs, from the cache when the refresh system has vouched
        // for it (see `CachedInputs`) and from the chain above otherwise. Only
        // a *complete* evaluation is cached: an error path leaves the slot
        // empty rather than storing half of one.
        let cached = self.cached_input.lock().ok().and_then(|slot| slot.clone());

        let (wrapper_source, library, prefix, feedstocks, tools) = match cached {
            Some(cached) => (
                cached.wrapper,
                cached.library,
                cached.prefix,
                cached.feedstocks,
                cached.tools,
            ),
            None => {
                let input_val = network_evaluator.evaluate_arg_required(
                    network_stack,
                    node_id,
                    registry,
                    context,
                    BASE_PIN,
                );
                if input_val.is_error() {
                    return all_pins(input_val);
                }
                let wrapper_source = match input_val {
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
                    propagated @ NetworkResult::Error(_) => return all_pins(propagated),
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
                    propagated @ NetworkResult::Error(_) => return all_pins(propagated),
                    array => match steps_from_array(&array) {
                        Ok(steps) => steps,
                        Err(message) => return self.fail(message),
                    },
                };

                // The two participant pins. Unwired is an empty list, which
                // is the workpiece-only replay.
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

                if let Ok(mut slot) = self.cached_input.lock() {
                    *slot = Some(CachedInputs {
                        wrapper: wrapper_source.clone(),
                        library: library.clone(),
                        prefix: prefix.clone(),
                        feedstocks: feedstocks.clone(),
                        tools: tools.clone(),
                    });
                }
                (wrapper_source, library, prefix, feedstocks, tools)
            }
        };

        // The replay mutates the atoms, so it works on a copy and the cache
        // keeps the pristine input.
        let mut wrapper = wrapper_source;

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

        let (scene, block_failure) = match replay_prefix_and_block(
            atoms,
            &feedstocks,
            &tools,
            &library,
            prefix,
            self.authored_steps(),
            self.cursor,
        ) {
            Ok(replayed) => replayed,
            Err(message) => {
                // Nothing ran, so there is no last good state to fall back to.
                self.record_last_scene(None);
                return self.fail(message);
            }
        };

        // `result` is the workpiece alone, `scene` everything; both keep
        // `base`'s variant.
        let mut scene_wrapper = wrapper.clone();
        *atoms_of(&mut wrapper) = scene.workpiece();
        *atoms_of(&mut scene_wrapper) = scene.structure.clone();

        // The state after the last successful step, for the viewport and the
        // placement tool. Recorded whether or not the block failed: on success
        // it is simply what the pins carry. The **whole scene**, because an
        // offer needs the bindings and a click needs the participant map — the
        // decorator's preview ghosts are deliberately not in it, being display
        // state of one evaluation.
        self.record_last_scene(Some(scene));

        // The placement preview is display-only, so it rides on the decorator
        // and never on the atoms — the same channel guided placement and the
        // guideline use. The ghosts were computed when the row was selected;
        // this only hands them over. **Both** output structures carry them, so
        // a selected row previews whichever pin the user is showing.
        if decorate && self.placement.has_preview() {
            let visuals = MechanosynthGhostVisuals {
                ghosts: self.placement.preview_ghosts.clone(),
                bonds: self.placement.preview_bonds.clone(),
                near_miss: self.placement.preview_near_miss,
            };
            atoms_of(&mut wrapper)
                .decorator_mut()
                .mechanosynth_ghost_visuals = Some(Box::new(visuals.clone()));
            atoms_of(&mut scene_wrapper)
                .decorator_mut()
                .mechanosynth_ghost_visuals = Some(Box::new(visuals));
        }

        match block_failure {
            None => {
                self.record_error(None);
                EvalOutput::multi(vec![wrapper, steps_output, scene_wrapper])
            }
            Some(message) => {
                // **The pins carry the error**, exactly as the replayer's
                // would: a downstream export or style node must never receive
                // a silently truncated build, and *same result, both nodes*
                // has to hold for a failing block as for a good one. What
                // changes is what the editor *displays* — the last good state,
                // through the per-pin display override the scene generator
                // already consults.
                self.record_error(Some(message.clone()));
                let failed = NetworkResult::Error(format!("mechanosynth_edit: {message}"));
                // **`steps` is not a replay product.** The block is stored
                // data and the prefix arrived intact, so the array is exactly
                // as valid as it was a moment ago — and it has to be, because
                // the walk that makes a sequence tool-aware inserts a recharge
                // *while* the block is failing, and a downstream replayer must
                // see the same steps throughout.
                let mut output = EvalOutput::multi(vec![failed.clone(), steps_output, failed]);
                output.set_display_override(0, wrapper);
                output.set_display_override(SCENE_OUTPUT_PIN, scene_wrapper);
                output
            }
        }
    }

    /// Drops the cached inputs. The refresh system calls this whenever upstream
    /// may have changed, which is what makes reading the cache safe.
    fn clear_input_cache(&self) {
        self.invalidate_input_cache();
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    /// **`scene`, not `result`.** A reservoir atom exists only in the scene,
    /// and a recharge is authored by clicking one; with `result` alone
    /// displayed there would be nothing to click. Base ids are the same in
    /// both, so a click on the workpiece means the same thing either way.
    fn default_displayed_output_pins(&self) -> Option<HashSet<i32>> {
        Some(HashSet::from([SCENE_OUTPUT_PIN as i32]))
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
            // Written even when empty, for the same reason `authored` is: the
            // text editor takes its list of known literal properties off the
            // *node's own* `get_text_properties`, so an omitted key turns a
            // `muted: [...]` a user typed into an "unknown property" warning
            // and a silent drop.
            (
                "muted".to_string(),
                TextValue::Array(
                    self.muted
                        .iter()
                        .map(|name| TextValue::String(name.clone()))
                        .collect(),
                ),
            ),
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
        if let Some(value) = props.get("muted") {
            let TextValue::Array(elements) = value else {
                return Err("muted must be an array of operation names".to_string());
            };
            self.muted = elements
                .iter()
                .map(|element| {
                    element
                        .as_string()
                        .map(str::to_string)
                        .ok_or_else(|| "muted must be an array of operation names".to_string())
                })
                .collect::<Result<BTreeSet<String>, String>>()?;
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
        all_pins(NetworkResult::Error(format!(
            "mechanosynth_edit: {message}"
        )))
    }
}

/// The same result on all three output pins: the outputs are one evaluation, so
/// whatever stops the workpiece stops the step array and the scene too.
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
            "op" | "t" | "r" | "note" | "phase" | "layer" | "site" | "residual" | "approximate"
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
            The `feedstocks` and `tools` pins are the replayer's, and the third output pin, \
            `scene`, is the same merged view — the workpiece, the reservoirs and the tool \
            molecules at the cursor. A freshly placed node shows it, because a reservoir atom \
            exists only there and a recharge is authored exactly like a placement: click the \
            dump atom and choose the donation. An operation whose tool is not bound, is in the \
            wrong state, or whose tool side does not match at the tool's pose is offered dimmed \
            and cannot be committed, with the reason where the residual would be. A click on a \
            tool atom is refused: tools are rewritten by their operations, not placed on.\n\
            \n\
            When the block fails at the cursor step, both structure pins carry the error — a \
            downstream export must never receive a silently truncated build — while the \
            viewport keeps showing the state after the last successful step, which is the one \
            the failing step is being authored against.\n\
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
            // Appended, both optional and wire-only — the replayer's two
            // participant pins. Every wired entry must have the phase of
            // `base`, a rule the node states itself (`network_validator.rs`).
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
            OutputPinDefinition::fixed(
                "steps",
                DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    BUILD_STEP_RECORD.to_string(),
                )))),
            ),
            // The merged scene at the cursor, appended for the same reason it
            // is on the replayer, and the **display default**: a reservoir atom
            // exists only here, and a recharge is authored by clicking it.
            OutputPinDefinition::same_as_input("scene", "base"),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(MechanosynthEditData::new()),
        node_data_saver: mechanosynth_edit_data_saver,
        node_data_loader: mechanosynth_edit_data_loader,
    }
}
