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
use crate::api::structure_designer::structure_designer_api_types::{
    APIAuthoredStep, APIGhostAtom, APIMechanosynthCandidate, APIMechanosynthChapter,
    APIMechanosynthEditData, APIMechanosynthOffer, APIMechanosynthOffers,
    APIMechanosynthPickResult,
};
use atomcad_crystolecule::mechanosynth::{BuildScript, GhostAtom, GhostKind};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::mechanosynth_edit_ops::{
    CandidateRow, OfferSweep, PickOutcome, StepMetadataField,
};
use atomcad_structure_designer::nodes::build_step::steps_from_array;
use atomcad_structure_designer::nodes::mechanosynth_edit::{AuthoredStep, OPS_PIN, STEPS_PIN};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::DVec3;

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

fn authored_view(step: &AuthoredStep) -> APIAuthoredStep {
    APIAuthoredStep {
        op: step.step.op.clone(),
        t: vec3(step.step.t),
        note: step.step.note.clone().unwrap_or_default(),
        method: step.step.method.clone(),
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
    let op_names = match designer.evaluate_node_argument(scope_path, node_id, OPS_PIN) {
        NetworkResult::OpLibrary(library) => library.ops.iter().map(|op| op.name.clone()).collect(),
        _ => Vec::new(),
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

    Some(APIMechanosynthEditData {
        prefix_count: prefix.len() as i32,
        authored: data.authored.iter().map(authored_view).collect(),
        cursor: data.cursor,
        applied: data.applied() as i32,
        op_names,
        inexact_count: inexact_count as i32,
        approximate_count: approximate_count as i32,
        last_error: data.last_error(),
        tool_state: data.placement.state().as_str().to_string(),
        armed_op: data.placement.armed.clone(),
        anchor_atom_id: data.placement.anchor,
        chapters,
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

#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_arm(scope_path: Vec<u64>, node_id: u64, op: String) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .mechanosynth_edit_arm(&scope_path, node_id, &op)
                    .err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_edit_pick(
    scope_path: Vec<u64>,
    node_id: u64,
    atom_id: u32,
) -> Result<APIMechanosynthPickResult, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance.structure_designer.mechanosynth_edit_pick(
                    &scope_path,
                    node_id,
                    atom_id,
                )?;
                // A commit changed the block, so the scene owes a refresh; the
                // other two outcomes changed only transient tool state.
                if matches!(outcome, PickOutcome::Committed { .. }) {
                    refresh_structure_designer_auto(cad_instance);
                }
                Ok(match outcome {
                    PickOutcome::Committed { index } => APIMechanosynthPickResult {
                        committed: true,
                        inserted_index: index as i32,
                        message: None,
                        candidates: Vec::new(),
                        offers: None,
                    },
                    PickOutcome::Candidates(rows) => APIMechanosynthPickResult {
                        committed: false,
                        inserted_index: -1,
                        message: None,
                        candidates: candidates_view(&rows),
                        offers: None,
                    },
                    PickOutcome::NoFit { message, offers } => APIMechanosynthPickResult {
                        committed: false,
                        inserted_index: -1,
                        message: Some(message),
                        candidates: Vec::new(),
                        offers: Some(offers_view(&offers)),
                    },
                })
            },
            Err("no CAD instance".to_string()),
        )
    }
}

/// Commits candidate `index` of operation `op`, from whatever the last click
/// produced. Refuses an operation the last offer list reported as a near miss.
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
                let _ = cad_instance
                    .structure_designer
                    .mechanosynth_edit_cancel(&scope_path, node_id);
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
/// `method`, `phase`, `layer`, `site`; `text` carries the first three and
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
