//! Node jobs — the FFI surface (`doc/design_background_node_jobs.md`).
//!
//! An explicit, expensive action on one node (`chemisorb`'s Run) runs off the
//! UI thread. Every rule — the guards, the runner, routing a result to its
//! document, deferring an install under an open interaction — lives in
//! `atomcad_structure_designer::node_jobs` and `DocumentSet`, where it is
//! tested. Each wrapper here is one `DocumentSet` (or designer) call plus
//! type conversion; the runner lives in `cad_instance.documents`.

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance, with_mut_cad_instance_or,
};
use crate::api::structure_designer::structure_designer_api_types::APINodeJobPoll;
use atomcad_structure_designer::structure_designer::StructureDesigner;

/// Resolves a node by numeric id or by name in the active network, the way
/// the CLI's `evaluate` does.
#[flutter_rust_bridge::frb(ignore)]
pub fn resolve_node_identifier(designer: &StructureDesigner, identifier: &str) -> Option<u64> {
    identifier
        .parse::<u64>()
        .ok()
        .or_else(|| designer.find_node_id_by_name(identifier))
}

/// The CLI's `run`: the node job of the node named (or numbered)
/// `node_identifier` at the top level of the active network, run to
/// completion on the calling thread (D9). Returns the install summary. Does
/// not refresh.
#[flutter_rust_bridge::frb(ignore)]
pub fn run_node_job_named(
    designer: &mut StructureDesigner,
    node_identifier: &str,
) -> Result<String, String> {
    let node_id = resolve_node_identifier(designer, node_identifier)
        .ok_or_else(|| format!("Node not found: {node_identifier}"))?;
    designer.run_node_job_blocking(&[], node_id)
}

// ============================================================================
// FRB wrappers
// ============================================================================

/// Starts the job of node `node_id` in `scope_path` of the active network.
/// Returns the job id, or a message for the user (the node has no run action,
/// a job already runs on it, the network is read-only, …).
#[flutter_rust_bridge::frb(sync)]
pub fn start_node_job(scope_path: Vec<u64>, node_id: u64) -> Result<u64, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance.documents.start_job(
                    &mut cad_instance.structure_designer,
                    &scope_path,
                    node_id,
                )
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// Asks job `job_id` to stop. It is reported as cancelled by the poll after
/// its worker returns; an unknown id is ignored.
#[flutter_rust_bridge::frb(sync)]
pub fn cancel_node_job(job_id: u64) {
    unsafe {
        with_cad_instance(|cad_instance| cad_instance.documents.cancel_job(job_id));
    }
}

/// Running jobs' progress, and every job that ended since the last poll —
/// successful ones installed into their node now. `defer_installs`:
/// Flutter's half of D11 — true while an interaction only Flutter knows about
/// is in progress; finished jobs then stay in their slots. Refreshes once
/// when an install touched the active document.
#[flutter_rust_bridge::frb(sync)]
pub fn poll_node_jobs(defer_installs: bool) -> APINodeJobPoll {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let poll = cad_instance
                    .documents
                    .poll_jobs(&mut cad_instance.structure_designer, defer_installs);
                if poll.active_changed {
                    refresh_structure_designer_auto(cad_instance);
                }
                APINodeJobPoll::from(poll)
            },
            APINodeJobPoll {
                running: Vec::new(),
                finished: Vec::new(),
                pending_installs: 0,
                active_changed: false,
            },
        )
    }
}

/// **Run** by node name or id in the active network, for the CLI's `run`
/// command (through the AI HTTP server). Blocking; the summary as text.
#[flutter_rust_bridge::frb(sync)]
pub fn run_node_job_by_name(node_identifier: String) -> Result<String, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result =
                    run_node_job_named(&mut cad_instance.structure_designer, &node_identifier);
                refresh_structure_designer_auto(cad_instance);
                result
            },
            Err("CAD instance not available".to_string()),
        )
    }
}
