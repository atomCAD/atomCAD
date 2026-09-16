//! Replaying a build onto the whole cast: the workpiece, the reservoirs it
//! draws on and the tool molecules that do the work.
//!
//! The engine has one primitive — a local `before`/`after` rewrite placed by a
//! rigid transform and matched by nearest atom within tolerance — and this
//! module applies it to **one merged structure**, the *scene*, with every atom
//! mapped to the [`Participant`] it belongs to. That map is the whole trick:
//!
//! - a tool's state change *is* the same rewrite, expressed in the tool's local
//!   frame and placed by the pose the tool's tagged atoms solve for, so every
//!   property the workpiece side has comes with it;
//! - an atom a step *adds* is entered under the participant its match landed
//!   in, which is what makes `result` — the **workpiece alone** — separable
//!   from the reservoirs after the fact. Only the engine knows, at match time,
//!   which structure a step acted on, and nothing a tag could say recovers it:
//!   an H dumped onto a reservoir carries no tag of its own.
//!
//! With nothing wired to `tools` the tool side is skipped for every step and no
//! state is tracked. That is not a compatibility mode but a *use* — looking at
//! what a build does to the workpiece without modelling the instruments, which
//! is how a library is developed before its tools exist.
//!
//! Design doc: `doc/design_mechanosynth_tools.md`.

use super::apply::{
    HighlightTags, apply_matched, match_positions, paint, resolve_tolerance, verify_pattern,
};
use super::pose::{ToolPose, name_pose_error, tool_pose};
use super::schema::{BuildScript, MechanosynthError, Method, NO_LAYER, OpLibrary, Step, ToolType};
use crate::atomic_constants::element_symbol;
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::tags::MAX_TAGS;
use glam::DVec3;
use rustc_hash::FxHashMap;

/// Which structure of the scene an atom belongs to. The index is the position
/// on the respective pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Participant {
    /// The workpiece — what `result` keeps and everything else drops.
    Base,
    /// A reservoir wired to `feedstocks`.
    Feedstock(usize),
    /// A tool molecule wired to `tools`.
    Tool(usize),
}

/// A wired tool molecule after binding.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolBinding {
    /// Index on the `tools` pin.
    pub instance: usize,
    /// The tool type whose name this molecule carries as an atom tag, and whose
    /// frame its tagged atoms fitted.
    pub tool_type: String,
    /// Where the molecule sits, solved from its four tagged atoms.
    pub pose: ToolPose,
    /// The symbolic state, starting at the type's initial state and moved by
    /// each tool side's `to`. `None` only for a type without `states`.
    pub state: Option<String>,
}

impl ToolBinding {
    /// `tool 0 (habst_tool)` — how every message names this tool.
    pub fn label(&self) -> String {
        format!("tool {} ({})", self.instance, self.tool_type)
    }
}

/// Everything a replay acts on, as one structure.
#[derive(Debug, Clone)]
pub struct Scene {
    pub structure: AtomicStructure,
    /// Every live atom of `structure`, including every atom a step added.
    pub participants: FxHashMap<u32, Participant>,
    /// One per wired tool, in pin order.
    pub bindings: Vec<ToolBinding>,
}

impl Scene {
    /// The participant of an atom, or [`Participant::Base`] for an id the map
    /// has lost track of — which cannot happen, and is the harmless answer.
    pub fn participant(&self, atom_id: u32) -> Participant {
        self.participants
            .get(&atom_id)
            .copied()
            .unwrap_or(Participant::Base)
    }

    /// How a message names a participant: `base`, `feedstock 1`,
    /// `tool 0 (habst_tool)`.
    pub fn label(&self, participant: Participant) -> String {
        match participant {
            Participant::Base => "base".to_string(),
            Participant::Feedstock(index) => format!("feedstock {index}"),
            Participant::Tool(index) => self
                .bindings
                .iter()
                .find(|binding| binding.instance == index)
                .map_or_else(|| format!("tool {index}"), ToolBinding::label),
        }
    }

    /// The label of the participant an atom belongs to, with its element, as a
    /// match failure names it: `the C of feedstock 0`.
    fn atom_label(&self, atom_id: u32) -> String {
        let element = self
            .structure
            .get_atom(atom_id)
            .map_or_else(|| "?".to_string(), |a| element_symbol(a.atomic_number));
        format!("the {element} of {}", self.label(self.participant(atom_id)))
    }

    /// The **workpiece alone**: the scene with every non-base atom deleted.
    ///
    /// Base atom ids are untouched by the merge — the base is cloned first —
    /// so for a build that touches no reservoir this is atom-for-atom what a
    /// tool-free [`replay`](super::replay) produces, and for one that does it
    /// is the base's atoms of the scene, added atoms included, with nothing of
    /// the reservoirs.
    pub fn workpiece(&self) -> AtomicStructure {
        let mut workpiece = self.structure.clone();
        let foreign: Vec<u32> = self
            .participants
            .iter()
            .filter(|(_, participant)| **participant != Participant::Base)
            .map(|(atom_id, _)| *atom_id)
            .collect();
        for atom_id in foreign {
            workpiece.delete_atom(atom_id);
        }
        workpiece
    }

    /// The binding of this tool type, if a wired molecule plays it.
    pub fn binding_of(&self, tool_type: &str) -> Option<&ToolBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.tool_type == tool_type)
    }
}

// ===========================================================================
// Replay
// ===========================================================================

/// Replays the first `step` steps of `script` onto the scene built from `base`,
/// `feedstocks` and `tools`.
///
/// All-or-nothing: a step that fails aborts the whole replay, and the partial
/// state is reachable by asking for one step fewer, as it has always been.
/// [`replay_scene_partial`] is the form the editor wants, which keeps the scene
/// after the last successful step *and* the error.
pub fn replay_scene(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<Scene, MechanosynthError> {
    let (scene, error) =
        replay_scene_partial(base, feedstocks, tools, library, script, step, tags)?;
    match error {
        Some(error) => Err(error),
        None => Ok(scene),
    }
}

/// [`replay_scene`], keeping what it managed to replay.
///
/// The outer `Err` is a failure to *build* the scene — a tag overflow, a
/// binding problem — after which there is nothing to show, since no step ran.
/// The inner `Some(error)` is a step failure, and the scene beside it is the
/// state after the last successful step: the editor renders that while the user
/// works on the step that is failing, which is the one they are authoring.
pub fn replay_scene_partial(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<(Scene, Option<MechanosynthError>), MechanosynthError> {
    let mut scene = build_scene(base, feedstocks, tools, library)?;
    let failure = replay_steps(&mut scene, library, script, step, tags)?;
    Ok((scene, failure))
}

/// Replays `script` **into a scene that already exists**, which is the form
/// with two passes over one cast: the editor replays its wired prefix with no
/// highlights and then its authored block with `ms_current`, and the second
/// pass has to see the tool states the first one left behind. One concatenated
/// script would not do — it would paint the highlight on the last *prefix* step
/// whenever the cursor sits at 0, which is exactly the state that must show no
/// highlight at all — and re-binding a fresh scene would put every tool back in
/// its initial state.
///
/// The outer `Err` is a script that names an operation the library does not
/// have; the inner `Some` is a step failure, after which `scene` is the state
/// the last successful step left. Ids are never reused, so replaying a second
/// script into a scene is exactly replaying a longer one.
pub fn replay_steps(
    scene: &mut Scene,
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<Option<MechanosynthError>, MechanosynthError> {
    super::parse::validate_script_ops(script, library)?;

    let tolerance = resolve_tolerance(library);
    let n = super::steps_applied(step, script.steps.len());
    let tool_model = !scene.bindings.is_empty();

    // The layer under construction is the last applied step's, read up front so
    // the loop can filter as it goes. `NO_LAYER` disables the layer set.
    let active_layer = n
        .checked_sub(1)
        .and_then(|index| script.steps.get(index))
        .map(|last| last.layer)
        .filter(|layer| *layer != NO_LAYER);

    let mut last_touched: Vec<u32> = Vec::new();
    let mut created: Vec<u32> = Vec::new();
    let mut created_in_layer: Vec<u32> = Vec::new();
    let mut failure: Option<MechanosynthError> = None;

    for (i, script_step) in script.steps.iter().take(n).enumerate() {
        let op = library
            .get(&script_step.op)
            .expect("validate_script_ops checked every op name");
        match apply_step_in_scene(scene, op, script_step, i + 1, tolerance, tool_model) {
            Ok(effect) => {
                if tags.added.is_some() {
                    created.extend(base_atoms(scene, &effect.added));
                }
                if tags.layer.is_some() && active_layer == Some(script_step.layer) {
                    created_in_layer.extend(base_atoms(scene, &effect.added));
                }
                last_touched = effect.touched;
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }

    // Ids are never reused, so a created atom a later step deleted is simply
    // gone; the sets are filtered against the finished scene rather than
    // bookkept step by step.
    paint(&mut scene.structure, tags.current, last_touched);
    paint(&mut scene.structure, tags.added, created);
    paint(&mut scene.structure, tags.layer, created_in_layer);
    paint_participants(scene, tags);

    Ok(failure)
}

/// `ms_added` and `ms_layer` mean "what the build created **on the workpiece**,
/// by layer". An abstracted H sitting on the tip, or dumped on a reservoir, is
/// cargo rather than construction, and the tags that tell those apart are
/// `ms_tool` and `ms_feedstock`.
fn base_atoms(scene: &Scene, atoms: &[u32]) -> Vec<u32> {
    atoms
        .iter()
        .copied()
        .filter(|atom_id| scene.participant(*atom_id) == Participant::Base)
        .collect()
}

fn paint_participants(scene: &mut Scene, tags: HighlightTags<'_>) {
    let of_kind = |scene: &Scene, want_tool: bool| -> Vec<u32> {
        scene
            .participants
            .iter()
            .filter(|(atom_id, participant)| {
                scene.structure.get_atom(**atom_id).is_some()
                    && match participant {
                        Participant::Tool(_) => want_tool,
                        Participant::Feedstock(_) => !want_tool,
                        Participant::Base => false,
                    }
            })
            .map(|(atom_id, _)| *atom_id)
            .collect()
    };
    let tool_atoms = of_kind(scene, true);
    let feedstock_atoms = of_kind(scene, false);
    paint(&mut scene.structure, tags.tool, tool_atoms);
    paint(&mut scene.structure, tags.feedstock, feedstock_atoms);
}

// ===========================================================================
// Building the scene
// ===========================================================================

/// Clones the base, merges every reservoir and every tool into it, records the
/// participant of every atom, and binds each tool molecule to its type.
///
/// **Binding happens before any step**, so a mis-tagged tool is reported once,
/// at the top, rather than at the first step that happens to use it.
pub fn build_scene(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
) -> Result<Scene, MechanosynthError> {
    let tolerance = resolve_tolerance(library);
    // The base is cloned first, so its atom ids are unchanged and `result` is
    // atom-for-atom comparable with a tool-free replay.
    let mut structure = base.clone();
    let mut participants: FxHashMap<u32, Participant> = structure
        .iter_atoms()
        .map(|(atom_id, _)| (*atom_id, Participant::Base))
        .collect();

    let merge = |structure: &mut AtomicStructure,
                 participants: &mut FxHashMap<u32, Participant>,
                 other: &AtomicStructure,
                 participant: Participant,
                 molecule: String|
     -> Result<FxHashMap<u32, u32>, MechanosynthError> {
        let existing = structure.tag_names().len();
        let remap =
            structure
                .add_atomic_structure(other)
                .map_err(|_| MechanosynthError::SceneTags {
                    molecule,
                    existing,
                    limit: MAX_TAGS,
                })?;
        for new_id in remap.values() {
            participants.insert(*new_id, participant);
        }
        Ok(remap)
    };

    for (index, feedstock) in feedstocks.iter().enumerate() {
        merge(
            &mut structure,
            &mut participants,
            feedstock,
            Participant::Feedstock(index),
            format!("feedstock {index}"),
        )?;
    }

    let mut tool_atoms: Vec<Vec<u32>> = Vec::with_capacity(tools.len());
    for (index, tool) in tools.iter().enumerate() {
        let remap = merge(
            &mut structure,
            &mut participants,
            tool,
            Participant::Tool(index),
            format!("tool {index}"),
        )?;
        tool_atoms.push(remap.into_values().collect());
    }

    let bindings = bind_tools(&structure, &tool_atoms, library, tolerance)?;

    Ok(Scene {
        structure,
        participants,
        bindings,
    })
}

/// Binds every wired molecule to the tool type whose name it carries as a tag,
/// and solves its pose from the four atoms carrying the type's frame tags.
///
/// **A tag lookup, never a search.** The design is supposed to supply exactly
/// the cast the library wrote parts for, so every deviation is an error naming
/// the molecule and never a guess. A type with *no* molecule is fine until a
/// step needs it.
fn bind_tools(
    structure: &AtomicStructure,
    tool_atoms: &[Vec<u32>],
    library: &OpLibrary,
    tolerance: f64,
) -> Result<Vec<ToolBinding>, MechanosynthError> {
    let mut bindings: Vec<ToolBinding> = Vec::with_capacity(tool_atoms.len());

    for (instance, atoms) in tool_atoms.iter().enumerate() {
        let molecule = format!("tool {instance}");

        let mut claimed: Vec<&ToolType> = Vec::new();
        for tool_type in &library.tools {
            if atoms
                .iter()
                .any(|atom_id| structure.atom_has_tag(*atom_id, &tool_type.name))
            {
                claimed.push(tool_type);
            }
        }
        let tool_type = match claimed.as_slice() {
            [] => {
                return Err(MechanosynthError::ToolUntagged {
                    molecule,
                    types: library.tool_type_names(),
                });
            }
            [only] => *only,
            [first, second, ..] => {
                return Err(MechanosynthError::ToolMultiType {
                    molecule,
                    first: first.name.clone(),
                    second: second.name.clone(),
                });
            }
        };

        if let Some(previous) = bindings
            .iter()
            .find(|binding| binding.tool_type == tool_type.name)
        {
            return Err(MechanosynthError::ToolDuplicate {
                tool_type: tool_type.name.clone(),
                first: format!("tool {}", previous.instance),
                second: molecule,
            });
        }

        // The frame tags are looked up **within this molecule**: the tag
        // vocabulary is meant to be shared across types (`apex`, `a`, `b`, `c`
        // for every tool), so a whole-scene lookup would find every tool's apex.
        let mut correspondences: Vec<(DVec3, DVec3)> = Vec::with_capacity(tool_type.frame.len());
        for entry in &tool_type.frame {
            let carriers: Vec<u32> = atoms
                .iter()
                .copied()
                .filter(|atom_id| structure.atom_has_tag(*atom_id, &entry.tag))
                .collect();
            let [atom_id] = carriers[..] else {
                return Err(MechanosynthError::ToolFrameTag {
                    molecule,
                    tool_type: tool_type.name.clone(),
                    tag: entry.tag.clone(),
                    found: carriers.len(),
                });
            };
            let position = structure
                .get_atom(atom_id)
                .expect("the tag index only names live atoms")
                .position;
            correspondences.push((entry.pos, position));
        }

        let pose = tool_pose(&correspondences, tolerance)
            .map_err(|error| name_pose_error(error, &molecule, &tool_type.name))?;

        bindings.push(ToolBinding {
            instance,
            tool_type: tool_type.name.clone(),
            pose,
            state: tool_type.initial_state().map(str::to_string),
        });
    }

    Ok(bindings)
}

// ===========================================================================
// One step
// ===========================================================================

/// What one step did to the scene: the union of both sides' `touched`, and the
/// atoms it created.
///
/// Public for the same reason [`StepEffect`](super::StepEffect) is: a generator
/// that emits a step and applies it immediately — so that later steps read
/// coordinates from the scene as built so far — needs to know which atoms the
/// step created and which it disturbed. [`replay_steps`] is the whole-script
/// form and reports neither.
#[derive(Debug, Default, Clone)]
pub struct SceneEffect {
    pub touched: Vec<u32>,
    pub added: Vec<u32>,
}

/// Applies one step: the target side, then the tool side when the operation is
/// `tip` and tools are wired.
///
/// **A step is all or nothing.** Both matches and every check run before the
/// first mutation, so a step that fails leaves the scene exactly as it found
/// it — which is what makes the partial-result form's "the scene after the last
/// successful step" true rather than approximately true. Matching both sides
/// against the same state is also the honest reading of "the tool side matches
/// over the whole scene": the two sides touch different participants by
/// construction, and the one case where they would not is
/// [`MechanosynthError::ToolSideOffTool`], which is raised here before anything
/// moves.
pub fn apply_step_in_scene(
    scene: &mut Scene,
    op: &super::schema::Operation,
    script_step: &Step,
    step_number: usize,
    tolerance: f64,
    tools_wired: bool,
) -> Result<SceneEffect, MechanosynthError> {
    // --- match the target side ---------------------------------------------
    // The positional pass alone: which participant the match landed in is a
    // question to answer *before* the bond and degree checks, because a match
    // that strayed onto the wrong structure fails those checks too and "it
    // matched on a tool" is the sentence that explains why.
    let target_match = {
        let label = |atom_id: u32| scene.label(scene.participant(atom_id));
        match_positions(
            &scene.structure,
            &op.name,
            &op.before,
            script_step,
            step_number,
            tolerance,
            Some(&label),
        )?
    };

    // Which structure the step acts on. A step whose matched atoms span two
    // participants has no answer, and one that landed on a tool is asking for
    // something tools do not do.
    let mut target = Participant::Base;
    let mut first: Option<Participant> = None;
    for atom_id in target_match.values() {
        let participant = scene.participant(*atom_id);
        if let Participant::Tool(_) = participant {
            return Err(MechanosynthError::StepOnTool {
                step: step_number,
                op: op.name.clone(),
                tool: scene.label(participant),
            });
        }
        match first {
            None => {
                first = Some(participant);
                target = participant;
            }
            Some(seen) if seen != participant => {
                return Err(MechanosynthError::StepAcrossParticipants {
                    step: step_number,
                    op: op.name.clone(),
                    first: scene.label(seen),
                    second: scene.label(participant),
                });
            }
            Some(_) => {}
        }
    }

    {
        let label = |atom_id: u32| scene.label(scene.participant(atom_id));
        verify_pattern(
            &scene.structure,
            &op.name,
            &op.before,
            &target_match,
            script_step,
            step_number,
            Some(&label),
        )?;
    }

    // --- match the tool side ------------------------------------------------
    let tool_side = op.tool.as_ref().filter(|_| tools_wired);
    let tool_plan = match tool_side {
        None => None,
        Some(tool_side) => {
            let Some(index) = scene
                .bindings
                .iter()
                .position(|binding| binding.tool_type == tool_side.tool_type)
            else {
                return Err(MechanosynthError::ToolMissing {
                    step: step_number,
                    op: op.name.clone(),
                    tool_type: tool_side.tool_type.clone(),
                });
            };

            // The symbolic check comes first, because its message is the one a
            // process author can act on. When the two disagree — the label says
            // *charged* and the apex has no atom to give — the geometric
            // failure below is what is reported, because the geometry is what a
            // viewer sees and the label is the thing that is wrong.
            if let Some(required) = &tool_side.from {
                let found = scene.bindings[index].state.clone().unwrap_or_default();
                if found != *required {
                    return Err(MechanosynthError::ToolState {
                        step: step_number,
                        op: op.name.clone(),
                        tool_type: tool_side.tool_type.clone(),
                        found: if found.is_empty() {
                            "stateless".to_string()
                        } else {
                            found
                        },
                        required: required.clone(),
                    });
                }
            }

            // The tool side is placed by the pose, not by the step's own
            // transform: the patterns are written in the tool's local frame,
            // and the binding is what maps that frame into the design.
            let pose = scene.bindings[index].pose;
            let mut tool_step = Step::new(op.name.clone(), pose.t);
            tool_step.r = pose.r;

            let matched = {
                let label = |atom_id: u32| scene.label(scene.participant(atom_id));
                match_positions(
                    &scene.structure,
                    &op.name,
                    &tool_side.before,
                    &tool_step,
                    step_number,
                    tolerance,
                    Some(&label),
                )?
            };

            // The tool side's match ranges over the whole scene, exactly as the
            // target side's does, so nothing about it is confined to the tool
            // by construction: a tool parked in contact with the workpiece can
            // have base atoms inside tolerance of a pattern position. The
            // mirror of `StepOnTool`.
            let instance = scene.bindings[index].instance;
            for atom_id in matched.values() {
                if scene.participant(*atom_id) != Participant::Tool(instance) {
                    return Err(MechanosynthError::ToolSideOffTool {
                        step: step_number,
                        op: op.name.clone(),
                        tool: scene.bindings[index].label(),
                        found: scene.atom_label(*atom_id),
                    });
                }
            }

            {
                let label = |atom_id: u32| scene.label(scene.participant(atom_id));
                verify_pattern(
                    &scene.structure,
                    &op.name,
                    &tool_side.before,
                    &matched,
                    &tool_step,
                    step_number,
                    Some(&label),
                )?;
            }

            Some((index, instance, tool_step, matched))
        }
    };

    // --- apply, now that nothing can fail -----------------------------------
    let mut effect = SceneEffect::default();

    let deleted = deleted_atoms(&op.before, &op.after, &target_match);
    let target_effect = apply_matched(
        &mut scene.structure,
        &op.before,
        &op.after,
        script_step,
        target_match,
    );
    for atom_id in deleted {
        scene.participants.remove(&atom_id);
    }
    // An atom a step adds belongs to the participant the match landed in. A
    // step whose `before` is empty — a pure addition — belongs to the base.
    for atom_id in &target_effect.added {
        scene.participants.insert(*atom_id, target);
    }
    effect.touched.extend(target_effect.touched.iter().copied());
    effect.added.extend(target_effect.added.iter().copied());

    let Some((index, instance, tool_step, matched)) = tool_plan else {
        return Ok(effect);
    };
    let tool_side = tool_side.expect("a tool plan exists only for a tool side");

    let deleted = deleted_atoms(&tool_side.before, &tool_side.after, &matched);
    let tool_effect = apply_matched(
        &mut scene.structure,
        &tool_side.before,
        &tool_side.after,
        &tool_step,
        matched,
    );
    for atom_id in deleted {
        scene.participants.remove(&atom_id);
    }
    for atom_id in &tool_effect.added {
        scene
            .participants
            .insert(*atom_id, Participant::Tool(instance));
    }
    effect.touched.extend(tool_effect.touched.iter().copied());
    effect.added.extend(tool_effect.added.iter().copied());

    if let Some(to) = &tool_side.to {
        scene.bindings[index].state = Some(to.clone());
    }

    Ok(effect)
}

/// The scene atoms a rewrite will delete: the ids only `before` names.
///
/// Read from the match rather than from the structure afterwards, because a
/// deleted atom leaves nothing behind to ask, and the participant map has to
/// let go of it.
fn deleted_atoms(
    before: &super::schema::Pattern,
    after: &super::schema::Pattern,
    matched: &FxHashMap<i64, u32>,
) -> Vec<u32> {
    before
        .atoms
        .iter()
        .filter(|atom| !after.has(atom.id))
        .filter_map(|atom| matched.get(&atom.id).copied())
        .collect()
}

// ===========================================================================
// Events
// ===========================================================================

/// The **event** each step belongs to, as a 0-based index over the script.
///
/// An event is a *display* grouping, never a replay semantics: the replay is
/// sequential for every kind, two bulk steps on neighbouring sites are not
/// independent, and the order of the steps inside an event **is** their
/// meaning. The rules, from `doc/design_mechanosynth_tools.md` §Methods:
///
/// - a `tip` step is its own event — one visit of one tool to one site;
/// - a maximal run of consecutive `bulk` steps **with the same agent** is one
///   event, the unit an exposure animates as;
/// - a `spontaneous` step belongs to the event of the preceding
///   non-spontaneous step; one at the very start of a script is its own event.
///
/// A step naming an operation the library does not have starts its own event —
/// [`validate_script_ops`](super::validate_script_ops) is what reports that,
/// and this function is not the place to raise it a second time.
pub fn event_indices(steps: &[Step], library: &OpLibrary) -> Vec<usize> {
    let mut events = Vec::with_capacity(steps.len());
    let mut current = 0usize;
    let mut previous: Option<(Method, Option<String>)> = None;

    for step in steps {
        let op = library.get(&step.op);
        let method = op.map(|op| op.method);
        let agent = op.and_then(|op| op.agent.clone());

        let starts_new = match (method, &previous) {
            // The first step is always the first event, whatever its kind.
            (_, None) => false,
            (Some(Method::Spontaneous), _) => false,
            (Some(Method::Bulk), Some((Method::Bulk, previous_agent))) => agent != *previous_agent,
            _ => true,
        };
        if starts_new {
            current += 1;
        }
        events.push(current);
        // A spontaneous step joins the run it settles; it must not *end* one,
        // or a settle between two exposures of one agent would split them.
        if method != Some(Method::Spontaneous) {
            previous = Some((method.unwrap_or(Method::Tip), agent));
        } else if previous.is_none() {
            previous = Some((Method::Spontaneous, None));
        }
    }
    events
}
