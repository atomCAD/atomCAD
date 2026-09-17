//! Kernel seam for the `mechanosynth_edit` node: its panel, and the viewport
//! placement tool.
//!
//! Everything here is a thin wrapper over a `StructureDesigner` method in
//! `mechanosynth_edit_ops.rs` plus a refresh — the orchestration is domain
//! work and lives downstairs, where it is reachable without the global
//! `CAD_INSTANCE` (the `ai_text_edit` / `field_distribution_api` pattern).
//! What this file owns is the **shape** of the transport: the domain's
//! candidates, ghosts and offer rows become plain records Dart can hold, and
//! the engine's `AtomicStructure` never crosses the bridge.
//!
//! Three rules the wrappers encode:
//!
//! - **The cursor setter does not validate and does not mark the project
//!   dirty for undo.** It is navigation, like the replayer's slider; every
//!   other mutator here records exactly one undo entry.
//! - **A near miss cannot be chosen.** `mechanosynth_edit_choose` refuses an
//!   operation the last offer list reported outside the gate, with the
//!   residual in the message, so an over-gate fit cannot reach the authored
//!   block through the API any more than through the popup.
//! - **Muting is a view filter over the offer sweep and nothing else**
//!   (`doc/design_mechanosynth_op_muting.md`). It reaches exactly two things
//!   here: which operations `mechanosynth_edit_offers` asks about, and the
//!   `muted` flag on a palette row. It is deliberately *not* a third refusal
//!   in `choose` — a muted operation is unreachable because it is not in the
//!   sweep, and a row that *is* in the list is placeable whatever put it
//!   there.

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance_or,
};
use crate::api::common_api_types::APIVec3;
use crate::api::structure_designer::mechanosynth_api::{feedstock_rows, tool_rows};
use crate::api::structure_designer::structure_designer_api_types::{
    APIAuthoredStep, APIGhostAtom, APIMechanosynthAnchor, APIMechanosynthCandidate,
    APIMechanosynthChapter, APIMechanosynthEditData, APIMechanosynthOffer, APIMechanosynthOffers,
    APIMechanosynthOp, APIMechanosynthToolStatus,
};
use atomcad_crystolecule::mechanosynth::{BuildScript, GhostAtom, GhostKind};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::mechanosynth_edit_ops::{
    CandidateRow, OfferSweep, StepMetadataField,
};
use atomcad_structure_designer::nodes::build_step::steps_from_array;
use atomcad_structure_designer::nodes::mechanosynth_edit::{AuthoredStep, OPS_PIN, STEPS_PIN};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::DVec3;
use std::collections::HashMap;

fn vec3(value: DVec3) -> APIVec3 {
    APIVec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn ghost(atom: &GhostAtom) -> APIGhostAtom {
    APIGhostAtom {
        kind: match atom.kind {
            GhostKind::Added => "added",
            GhostKind::Deleted => "deleted",
            GhostKind::Moved => "moved",
            GhostKind::Changed => "changed",
        }
        .to_string(),
        position: vec3(atom.position),
        from: vec3(atom.from),
        atomic_number: atom.atomic_number as i32,
    }
}

/// The popup's whole view of a sweep, against a sweep the caller already has.
///
/// `pub` so the api-side tests can project a sweep built from a plain
/// `StructureDesigner`; `frb(ignore)` because it is a projection between two
/// Rust types and has no business being a Dart entry point.
#[flutter_rust_bridge::frb(ignore)]
pub fn offers_view(sweep: &OfferSweep) -> APIMechanosynthOffers {
    APIMechanosynthOffers {
        anchor_atom_id: sweep.anchor_atom_id,
        anchor_position: vec3(sweep.anchor_position),
        anchor_atomic_number: sweep.anchor_atomic_number as i32,
        muted_count: sweep.skipped_muted as i32,
        library_count: sweep.library_count as i32,
        rows: sweep
            .rows
            .iter()
            .map(|row| APIMechanosynthOffer {
                op: row.op.clone(),
                note: row.note.clone(),
                candidate_count: row.candidate_count as i32,
                best_residual: row.best_residual,
                fits: row.fits,
                exact: row.exact,
                mirrored: row.mirrored,
                approximate: row.approximate,
                ghost: row.ghost.iter().map(ghost).collect(),
                candidates: candidates_view(&row.candidates),
                // An empty `tool_type` is the "no tool annotation" state, which
                // is what every row carries with `tools` unwired.
                tool_type: row
                    .tool
                    .as_ref()
                    .map(|tool| tool.tool_type.clone())
                    .unwrap_or_default(),
                tool_state: row
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.state.clone())
                    .unwrap_or_default(),
                tool_ready: row.tool.as_ref().is_none_or(|tool| tool.ready),
                tool_reason: row
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.reason.clone())
                    .unwrap_or_default(),
                offerable: row.offerable,
                muted: row.muted,
                blocked: row.blocked.clone().unwrap_or_default(),
            })
            .collect(),
    }
}

fn candidates_view(rows: &[CandidateRow]) -> Vec<APIMechanosynthCandidate> {
    rows.iter()
        .map(|row| APIMechanosynthCandidate {
            index: row.index as i32,
            residual: row.residual,
            exact: row.exact,
            mirrored: row.mirrored,
            approximate: row.approximate,
            ghost: row.ghost.iter().map(ghost).collect(),
            blocked: row.blocked.clone().unwrap_or_default(),
        })
        .collect()
}

/// What the wired library says about one operation: its kind, and the
/// instrument or the agent that performs it. All three are the **operation's**,
/// never the step's, so a row that names an operation the library does not have
/// simply shows none of them.
///
/// Not a transport type — it never leaves this file, so it is `frb(ignore)`d;
/// codegen otherwise walks every type declared under `api/` and would generate
/// a Dart twin for it.
#[flutter_rust_bridge::frb(ignore)]
#[derive(Clone, Default)]
struct OpFacts {
    method: String,
    tool_type: String,
    agent: String,
}

fn authored_view(step: &AuthoredStep, facts: &HashMap<String, OpFacts>) -> APIAuthoredStep {
    let op = facts.get(&step.step.op).cloned().unwrap_or_default();
    APIAuthoredStep {
        op: step.step.op.clone(),
        t: vec3(step.step.t),
        note: step.step.note.clone().unwrap_or_default(),
        method: op.method,
        tool_type: op.tool_type,
        agent: op.agent,
        phase: step.step.phase.clone(),
        layer: step.step.layer,
        site: step.step.site,
        residual: step.residual,
        exact: step.is_exact(),
        approximate: step.approximate,
    }
}

/// The panel's whole view of the node, against an explicit designer.
///
/// Takes `&mut` because two of the four things it reports come from
/// *evaluating* a pin — the prefix length and the library's operation names
/// are on wires, not in stored data — exactly as `mechanosynth_info` does for
/// the replayer.
#[flutter_rust_bridge::frb(ignore)]
pub fn mechanosynth_edit_data(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIMechanosynthEditData> {
    // Cloned out first: the two argument evaluations below need the designer
    // mutably.
    let data = designer
        .mechanosynth_edit_data(scope_path, node_id)?
        .clone();

    let prefix = match designer.evaluate_node_argument(scope_path, node_id, STEPS_PIN) {
        NetworkResult::None | NetworkResult::Error(_) => Vec::new(),
        array => steps_from_array(&array).unwrap_or_default(),
    };
    // One pass over the wired library, feeding two things: the palette's rows,
    // and the map `authored_view` looks a step's operation up in.
    let (mut ops, methods): (Vec<APIMechanosynthOp>, HashMap<String, OpFacts>) =
        match designer.evaluate_node_argument(scope_path, node_id, OPS_PIN) {
            NetworkResult::OpLibrary(library) => (
                library
                    .ops
                    .iter()
                    .map(|op| APIMechanosynthOp {
                        name: op.name.clone(),
                        note: op.note.clone().unwrap_or_default(),
                        method: op.method.as_str().to_string(),
                        tool_type: op
                            .tool
                            .as_ref()
                            .map(|tool| tool.tool_type.clone())
                            .unwrap_or_default(),
                        agent: op.agent.clone().unwrap_or_default(),
                        muted: data.is_muted(&op.name),
                    })
                    .collect(),
                library
                    .ops
                    .iter()
                    .map(|op| {
                        (
                            op.name.clone(),
                            OpFacts {
                                method: op.method.as_str().to_string(),
                                tool_type: op
                                    .tool
                                    .as_ref()
                                    .map(|tool| tool.tool_type.clone())
                                    .unwrap_or_default(),
                                agent: op.agent.clone().unwrap_or_default(),
                            },
                        )
                    })
                    .collect(),
            ),
            _ => (Vec::new(), HashMap::new()),
        };
    // Muted names the wired library does not define, appended after its own
    // with an empty `method`. They are kept rather than dropped — the `ops` pin
    // may be rewired back — so the panel has to be able to show them, and a
    // mute the user cannot see is a mute they cannot undo.
    ops.extend(
        data.muted
            .iter()
            .filter(|name| !methods.contains_key(*name))
            .map(|name| APIMechanosynthOp {
                name: name.clone(),
                note: String::new(),
                method: String::new(),
                tool_type: String::new(),
                agent: String::new(),
                muted: true,
            }),
    );

    let (inexact_count, approximate_count) = data.inexact_counts();
    // The chapter list is the replayer's, run over the authored block: the
    // panel's chapter navigation is the same widget, so it must be fed the same
    // shape.
    let chapters: Vec<APIMechanosynthChapter> =
        crate::api::structure_designer::mechanosynth_api::chapters(&BuildScript {
            file: "authored".to_string(),
            tolerance: None,
            steps: data.authored_steps(),
        });

    // The *Tools* readout and the last-good banner both come off the scene the
    // node's last evaluation parked — never by forcing one, since the panel is
    // rebuilt far more often than the block changes.
    let scene = data.last_scene();
    let last_good_atom_count = match (data.last_error(), scene.as_ref()) {
        (Some(_), Some(scene)) => scene.structure.iter_atoms().count() as i32,
        _ => -1,
    };

    Some(APIMechanosynthEditData {
        prefix_count: prefix.len() as i32,
        authored: data
            .authored
            .iter()
            .map(|step| authored_view(step, &methods))
            .collect(),
        cursor: data.cursor,
        applied: data.applied() as i32,
        ops,
        inexact_count: inexact_count as i32,
        approximate_count: approximate_count as i32,
        last_error: data.last_error(),
        tool_state: data.placement.state().as_str().to_string(),
        anchor_atom_id: data.placement.anchor,
        chapters,
        // No motion: the editor keeps its cursor and gets no step time
        // (`doc/design_mechanosynth_trajectory.md` §`mechanosynth_edit` keeps
        // its cursor), so no tool of its block replay is ever away from park.
        tools: tool_rows(scene.as_ref(), None),
        feedstocks: feedstock_rows(scene.as_ref()),
        last_good_atom_count,
    })
}

// ============================================================================
// Flutter entry points
// ============================================================================

#[flutter_rust_bridge::frb(sync)]
pub fn get_mechanosynth_edit_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIMechanosynthEditData> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                mechanosynth_edit_data(&mut cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}

/// Moves the cursor. **Not** an undo entry — bracket a scrub with nothing; the
/// replayer's slider behaves the same way.
#[flutter_rust_bridge::frb(sync)]
pub fn set_mechanosynth_edit_cursor(scope_path: Vec<u64>, node_id: u64, cursor: i32) {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let _ = cad_instance
                    .structure_designer
                    .set_mechanosynth_edit_cursor(&scope_path, node_id, cursor);
                refresh_structure_designer_auto(cad_instance);
            },
            (),
        )
    }
}

/// The atom-first entry point: what the wired library can do at `atom_id`.
/// An empty row list is an answer, not an error.
/// `include_muted` sweeps the whole library for this anchor, ignoring the
/// node's mute set — the popup's *show all here*, which is what keeps an empty
/// list an honest statement about the library's coverage. Every other caller
/// passes `false`.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_offers(
    scope_path: Vec<u64>,
    node_id: u64,
    atom_id: u32,
    include_muted: bool,
) -> Result<APIMechanosynthOffers, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let designer = &mut cad_instance.structure_designer;
                let sweep = if include_muted {
                    designer.mechanosynth_edit_offers_including_muted(&scope_path, node_id, atom_id)
                } else {
                    designer.mechanosynth_edit_offers(&scope_path, node_id, atom_id)
                };
                sweep.map(|sweep| offers_view(&sweep))
            },
            Err("no CAD instance".to_string()),
        )
    }
}

/// Mutes or unmutes `ops` on this node, in **one** undo entry however many
/// names it carries — a group toggle in the panel writes a dozen at once, and
/// that is one user action.
///
/// A name the wired library does not define is stored anyway: the `ops` pin may
/// be rewired, and a mute that evaporated when its library was briefly swapped
/// would be worse than one that waits.
#[flutter_rust_bridge::frb(sync)]
pub fn set_mechanosynth_edit_muted(
    scope_path: Vec<u64>,
    node_id: u64,
    ops: Vec<String>,
    muted: bool,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance.structure_designer.set_mechanosynth_edit_muted(
                    &scope_path,
                    node_id,
                    &ops,
                    muted,
                );
                if outcome.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

/// Which atom of this node's workpiece a viewport ray hits, or `None`.
///
/// The placement entry points take an atom id; this is what turns a click into
/// one. Scoped to the editor's own node on purpose — an atom belonging to some
/// other displayed structure is not a host this tool may place on.
///
/// The position and element come back with it because the popup is anchored to
/// the atom and has to follow it as the camera moves, and the path that opens a
/// candidate list has no offer sweep to take them from.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_anchor_at_ray(
    scope_path: Vec<u64>,
    node_id: u64,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> Option<APIMechanosynthAnchor> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .mechanosynth_edit_anchor_at_ray(
                        &scope_path,
                        node_id,
                        DVec3::new(ray_origin.x, ray_origin.y, ray_origin.z),
                        DVec3::new(ray_direction.x, ray_direction.y, ray_direction.z),
                    )
                    .map(|anchor| APIMechanosynthAnchor {
                        atom_id: anchor.atom_id,
                        position: vec3(anchor.position),
                        atomic_number: anchor.atomic_number as i32,
                    })
            },
            None,
        )
    }
}

/// Selects one row of the open list for **preview**, so the next evaluation
/// ghosts it on the workpiece.
///
/// The preview goes through the decorator and the tessellator like guided
/// placement, so it costs an evaluation of *this* node. It must not cost one of
/// the chain above it — `base` reaches back through a `mechanosynth` replaying
/// a hundred steps over a few thousand atoms, and nothing upstream has changed.
/// So this is the `atom_edit` drag pattern: `mark_skip_downstream` keeps the
/// refresh from clearing the node's input cache (and from walking the
/// downstream cone), and the node's `eval` then reuses its cached inputs. Safe
/// here for the same reason it is safe there — the tool only owns picks while
/// this node's own pin is the displayed one, so there is no downstream node
/// whose display could go stale.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_select_preview(
    scope_path: Vec<u64>,
    node_id: u64,
    op: String,
    index: u32,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance
                    .structure_designer
                    .mechanosynth_edit_select_preview(&scope_path, node_id, &op, index as usize);
                if outcome.is_ok() {
                    cad_instance.structure_designer.mark_skip_downstream();
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

/// Drops the preview without closing the list.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_clear_preview(scope_path: Vec<u64>, node_id: u64) {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if cad_instance
                    .structure_designer
                    .mechanosynth_edit_clear_preview(&scope_path, node_id)
                    .is_ok()
                {
                    // Dropping a preview is the same shape of change as taking
                    // one: this node only, nothing upstream.
                    cad_instance.structure_designer.mark_skip_downstream();
                    refresh_structure_designer_auto(cad_instance);
                }
            },
            (),
        )
    }
}

/// Commits candidate `index` of operation `op` from the open offer list.
/// Refuses an operation the list reported as a near miss.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_choose(
    scope_path: Vec<u64>,
    node_id: u64,
    op: String,
    index: u32,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance.structure_designer.mechanosynth_edit_choose(
                    &scope_path,
                    node_id,
                    &op,
                    index as usize,
                );
                if outcome.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_cancel(scope_path: Vec<u64>, node_id: u64) {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if cad_instance
                    .structure_designer
                    .mechanosynth_edit_cancel(&scope_path, node_id)
                    .is_ok()
                {
                    // Closing the list drops the preview, and the preview is in
                    // the scene now — so this owes a repaint that the
                    // Flutter-overlay version did not.
                    refresh_structure_designer_auto(cad_instance);
                }
            },
            (),
        )
    }
}

/// Duplicates the step at `index`, inserting the copy straight after it —
/// residual and `approximate` flag included, because the copy describes the
/// same fit.
///
/// This is the only "insert" the panel can offer: a step that was never fitted
/// against a workpiece has no `(r, t)` to write, so every other insertion comes
/// from a placement.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_duplicate_step(
    scope_path: Vec<u64>,
    node_id: u64,
    index: u32,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let designer = &mut cad_instance.structure_designer;
                let index = index as usize;
                let step = match designer
                    .mechanosynth_edit_data(&scope_path, node_id)
                    .and_then(|data| data.authored.get(index).cloned())
                {
                    Some(step) => step,
                    None => return Some(format!("no step at index {index}")),
                };
                let outcome =
                    designer.mechanosynth_edit_insert_step(&scope_path, node_id, index + 1, step);
                if outcome.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_delete_step(
    scope_path: Vec<u64>,
    node_id: u64,
    index: u32,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance
                    .structure_designer
                    .mechanosynth_edit_delete_step(&scope_path, node_id, index as usize);
                if outcome.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_move_step(
    scope_path: Vec<u64>,
    node_id: u64,
    from: u32,
    to: u32,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance.structure_designer.mechanosynth_edit_move_step(
                    &scope_path,
                    node_id,
                    from as usize,
                    to as usize,
                );
                if outcome.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

/// **Adopt these into the block**: copies the steps on the `steps` pin into the
/// authored block and disconnects the pin, in one undo entry. Returns how many
/// steps were adopted, for the confirmation the panel shows.
///
/// The adopted steps are ordinary authored steps afterwards — editable,
/// reorderable, and with no link to the file the wire came from. That is the
/// difference this action exists to make, and the panel says so once, when it
/// happens.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_adopt_prefix(scope_path: Vec<u64>, node_id: u64) -> Result<u32, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let adopted = cad_instance
                    .structure_designer
                    .mechanosynth_edit_adopt_prefix(&scope_path, node_id)?;
                refresh_structure_designer_auto(cad_instance);
                Ok(adopted as u32)
            },
            Err("no CAD instance".to_string()),
        )
    }
}

/// *Insert steps from file…*: splices a build file's steps into the block at
/// `index`, in one undo entry. Returns how many steps were inserted.
///
/// The second entry point for the same one-shot import. Not gated on the
/// `steps` pin — whether an imported step replays depends on the workpiece
/// state at `index`, not on how the steps ahead of it arrived. Nothing about
/// `file` is stored: a path the node
/// remembered would be a promise to track it, which is exactly what this is
/// not.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_insert_steps_from_file(
    scope_path: Vec<u64>,
    node_id: u64,
    file: String,
    index: u32,
) -> Result<u32, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let design_dir = cad_instance
                    .structure_designer
                    .node_type_registry
                    .design_file_name
                    .as_ref()
                    .and_then(|design_path| {
                        atomcad_util::path_utils::get_parent_directory(design_path)
                    });
                let inserted = cad_instance
                    .structure_designer
                    .mechanosynth_edit_insert_steps_from_file(
                        &scope_path,
                        node_id,
                        &file,
                        index as usize,
                        design_dir.as_deref(),
                    )?;
                refresh_structure_designer_auto(cad_instance);
                Ok(inserted as u32)
            },
            Err("no CAD instance".to_string()),
        )
    }
}

/// Writes one metadata field of one authored step. `field` is one of `note`,
/// `phase`, `layer`, `site`; `text` carries the first two and
/// `number` the last two. Consecutive writes to the same field of the same step
/// coalesce into one undo entry.
#[flutter_rust_bridge::frb(sync)]
pub fn set_mechanosynth_edit_step_metadata(
    scope_path: Vec<u64>,
    node_id: u64,
    index: u32,
    field: String,
    text: String,
    number: i32,
) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let Some(field) = StepMetadataField::parse(&field) else {
                    return Some(format!("'{field}' is not a step metadata field"));
                };
                let outcome = cad_instance
                    .structure_designer
                    .set_mechanosynth_edit_step_metadata(
                        &scope_path,
                        node_id,
                        index as usize,
                        field,
                        &text,
                        number,
                    );
                if outcome.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

/// Where the placement tool stands: the two fields the viewport needs on every
/// frame, and nothing else.
///
/// Deliberately **not** a projection of [`get_mechanosynth_edit_data`]: that one
/// evaluates two input pins to report the prefix length and the library's
/// operation names, and the viewport is rebuilt on every pointer move. This
/// reads stored transient state and evaluates nothing.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_tool_status(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIMechanosynthToolStatus> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .mechanosynth_edit_data(&scope_path, node_id)
                    .map(|data| APIMechanosynthToolStatus {
                        tool_state: data.placement.state().as_str().to_string(),
                    })
            },
            None,
        )
    }
}

/// Whether the address names a `mechanosynth_edit` node at all — the panel's
/// cheap existence check, and the only read here that needs no evaluation.
#[flutter_rust_bridge::frb(sync)]
pub fn is_mechanosynth_edit_node(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .mechanosynth_edit_data(&scope_path, node_id)
                    .is_some()
            },
            false,
        )
    }
}
