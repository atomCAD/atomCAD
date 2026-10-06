//! Kernel seam for the `chemisorb` node: its settings in and out, and the
//! report the properties panel renders.
//!
//! The report lives in the selected node's eval cache (`ChemisorbEvalCache`),
//! as `proxy`'s does; the search result itself lives on the node data, keyed
//! by an input fingerprint, and is written only by Run — the node's job
//! (`doc/design_background_node_jobs.md`). Evaluation never searches. Run is
//! the generic node-job FFI: `node_jobs_api::start_node_job` for the panel,
//! `run_node_job_by_name` for the CLI's `run`.
//!
//! Each entry point is a thin FRB wrapper over an `#[frb(ignore)]` function
//! taking an explicit designer, so the logic is testable without the global
//! `CAD_INSTANCE` (the `proxy_api` pattern).

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APIChemisorbData, APIChemisorbReport,
};
use atomcad_structure_designer::nodes::chemisorb::{ChemisorbData, ChemisorbEvalCache};
use atomcad_structure_designer::structure_designer::StructureDesigner;

/// The stored settings of a `chemisorb` node. `None` when `node_id` names no
/// node in `scope_path` or a node of another type.
#[flutter_rust_bridge::frb(ignore)]
pub fn chemisorb_node_data(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIChemisorbData> {
    let data = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<ChemisorbData>()?;
    Some(APIChemisorbData::from(data))
}

/// Writes the settings of a `chemisorb` node (one undo step); the stored
/// search is kept, and goes stale if the new settings no longer match it.
#[flutter_rust_bridge::frb(ignore)]
pub fn set_chemisorb_node_data(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    data: &APIChemisorbData,
) {
    designer.set_chemisorb_data(scope_path, node_id, ChemisorbData::from(data));
}

/// The report of the last root evaluation of the **selected** `chemisorb`
/// node: the plan or the result, whichever the node output. `None` when the
/// selected node is not a `chemisorb` or has not been evaluated as a root
/// node since it was selected (a node that is not displayed never is).
#[flutter_rust_bridge::frb(ignore)]
pub fn chemisorb_node_report(designer: &StructureDesigner) -> Option<APIChemisorbReport> {
    designer.get_selected_node_id_with_type("chemisorb")?;
    let cache = designer.get_selected_node_eval_cache()?;
    let cache = cache.downcast_ref::<ChemisorbEvalCache>()?;
    Some(APIChemisorbReport::from(cache))
}

// ============================================================================
// FRB wrappers
// ============================================================================

#[flutter_rust_bridge::frb(sync)]
pub fn get_chemisorb_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIChemisorbData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                chemisorb_node_data(&cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_chemisorb_data(scope_path: Vec<u64>, node_id: u64, data: APIChemisorbData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            set_chemisorb_node_data(
                &mut cad_instance.structure_designer,
                &scope_path,
                node_id,
                &data,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_chemisorb_report() -> Option<APIChemisorbReport> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| chemisorb_node_report(&cad_instance.structure_designer),
            None,
        )
    }
}
