//! Kernel seam for the `chemisorb` node: its settings in and out, the report
//! the properties panel renders, and **Run**.
//!
//! The report lives in the selected node's eval cache (`ChemisorbEvalCache`),
//! as `proxy`'s does; the search result itself lives on the node data, keyed
//! by an input fingerprint, and is written only by Run
//! (`StructureDesigner::run_chemisorb`). Evaluation never searches.
//!
//! Each entry point is a thin FRB wrapper over an `#[frb(ignore)]` function
//! taking an explicit designer, so the logic is testable without the global
//! `CAD_INSTANCE` (the `proxy_api` pattern).

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
    with_mut_cad_instance_or,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APIChemisorbData, APIChemisorbReport, APIChemisorbRunResult,
};
use atomcad_structure_designer::nodes::chemisorb::{ChemisorbData, ChemisorbEvalCache};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_util::number_format::format_natural;

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

/// Resolves a node by numeric id or by name in the active network, the way
/// the CLI's `evaluate` does.
#[flutter_rust_bridge::frb(ignore)]
pub fn resolve_node_identifier(designer: &StructureDesigner, identifier: &str) -> Option<u64> {
    identifier
        .parse::<u64>()
        .ok()
        .or_else(|| designer.find_node_id_by_name(identifier))
}

/// One line per fact, for the CLI's `run`.
#[flutter_rust_bridge::frb(ignore)]
pub fn format_run_result(result: &APIChemisorbRunResult) -> String {
    let mut lines = vec![format!(
        "Relaxed {} hypotheses in {} s; {} listed.",
        result.relaxed,
        format_natural(result.seconds, 3),
        result.listed
    )];
    match result.best_score {
        Some(score) => lines.push(format!(
            "Best: score {} kcal/mol, {}.",
            format_natural(score, 4),
            result.best_bonds
        )),
        None => lines.push("No bonding pattern found.".to_string()),
    }
    if result.unconverged > 0 {
        lines.push(format!(
            "{} relaxation(s) did not converge.",
            result.unconverged
        ));
    }
    if result.truncated {
        lines.push("Budget hit: the search is NOT exhaustive.".to_string());
    }
    lines.join("\n")
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

/// **Run**: searches with the node and stores the result, then refreshes so
/// the outputs show it. Synchronous — seconds to minutes; the panel shows a
/// modal placard meanwhile. `Err` carries a message for the user.
#[flutter_rust_bridge::frb(sync)]
pub fn run_chemisorb(scope_path: Vec<u64>, node_id: u64) -> Result<APIChemisorbRunResult, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance
                    .structure_designer
                    .run_chemisorb(&scope_path, node_id)
                    .map(APIChemisorbRunResult::from);
                refresh_structure_designer_auto(cad_instance);
                result
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// **Run** by node name or id in the active network, for the CLI's `run`
/// command. Returns the summary as text.
#[flutter_rust_bridge::frb(sync)]
pub fn run_chemisorb_node(node_identifier: String) -> Result<String, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let designer = &mut cad_instance.structure_designer;
                let node_id = resolve_node_identifier(designer, &node_identifier)
                    .ok_or_else(|| format!("Node not found: {node_identifier}"))?;
                let result = designer
                    .run_chemisorb(&[], node_id)
                    .map(APIChemisorbRunResult::from);
                refresh_structure_designer_auto(cad_instance);
                result.map(|r| format_run_result(&r))
            },
            Err("CAD instance not available".to_string()),
        )
    }
}
