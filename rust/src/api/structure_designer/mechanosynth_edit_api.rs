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
//! Two rules the wrappers encode:
//!
//! - **The cursor setter does not validate and does not mark the project
//!   dirty for undo.** It is navigation, like the replayer's slider; every
//!   other mutator here records exactly one undo entry.
//! - **A near miss cannot be chosen.** `mechanosynth_edit_choose` refuses an
//!   operation the last offer list reported outside the gate, with the
//!   residual in the message, so an over-gate fit cannot reach the authored
//!   block through the API any more than through the popup.

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance_or,
};
use crate::api::common_api_types::APIVec3;
use crate::api::structure_designer::mechanosynth_api::{feedstock_rows, tool_rows};
use crate::api::structure_designer::structure_designer_api_types::{
    APIAuthoredStep, APIGhostAtom, APIMechanosynthAnchor, APIMechanosynthCandidate,
    APIMechanosynthChapter, APIMechanosynthEditData, APIMechanosynthOffer, APIMechanosynthOffers,
    APIMechanosynthToolStatus,
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

fn offers_view(sweep: &OfferSweep) -> APIMechanosynthOffers {
    APIMechanosynthOffers {
        anchor_atom_id: sweep.anchor_atom_id,
        anchor_position: vec3(sweep.anchor_position),
        anchor_atomic_number: sweep.anchor_atomic_number as i32,
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
        })
        .collect()
}

/// `methods` maps an operation name to its kind, from the wired library. The
/// method is the operation's now, so a row that names an operation the library
/// does not have simply shows none.
fn authored_view(step: &AuthoredStep, methods: &HashMap<String, String>) -> APIAuthoredStep {
    APIAuthoredStep {
        op: step.step.op.clone(),
        t: vec3(step.step.t),
        note: step.step.note.clone().unwrap_or_default(),
        method: methods.get(&step.step.op).cloned().unwrap_or_default(),
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
    let (op_names, methods) = match designer.evaluate_node_argument(scope_path, node_id, OPS_PIN) {
        NetworkResult::OpLibrary(library) => (
            library.ops.iter().map(|op| op.name.clone()).collect(),
            library
                .ops
                .iter()
                .map(|op| (op.name.clone(), op.method.as_str().to_string()))
                .collect(),
        ),
        _ => (Vec::new(), HashMap::new()),
    };

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
        op_names,
        inexact_count: inexact_count as i32,
        approximate_count: approximate_count as i32,
        last_error: data.last_error(),
        tool_state: data.placement.state().as_str().to_string(),
        anchor_atom_id: data.placement.anchor,
        chapters,
        tools: tool_rows(scene.as_ref()),
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
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_offers(
    scope_path: Vec<u64>,
    node_id: u64,
    atom_id: u32,
) -> Result<APIMechanosynthOffers, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .mechanosynth_edit_offers(&scope_path, node_id, atom_id)
                    .map(|sweep| offers_view(&sweep))
            },
            Err("no CAD instance".to_string()),
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
