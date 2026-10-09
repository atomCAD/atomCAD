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
    with_mut_cad_instance_or,
};
use crate::api::structure_designer::node_jobs_api::resolve_node_identifier;
use crate::api::structure_designer::structure_designer_api_types::{
    APIChemisorbData, APIChemisorbDebugForm, APIChemisorbDebugRef, APIChemisorbDebugRow,
    APIChemisorbReport,
};
use atomcad_crystolecule::chemisorption::sequential::DebugForm;
use atomcad_structure_designer::chemisorb_ops::{ChemisorbDebugStep, DebugRowRef};
use atomcad_structure_designer::nodes::chemisorb::{
    ChemisorbData, ChemisorbEvalCache, DEBUG_OUTPUT_PIN, DEBUG_SHAPES_OUTPUT_PIN,
};
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

/// The selected `chemisorb` node's eval cache, as `chemisorb_node_report`
/// finds it.
fn selected_cache(designer: &StructureDesigner) -> Option<&ChemisorbEvalCache> {
    designer.get_selected_node_id_with_type("chemisorb")?;
    designer
        .get_selected_node_eval_cache()?
        .downcast_ref::<ChemisorbEvalCache>()
}

/// Item `item` of the selected `chemisorb` node's debug tree.
#[flutter_rust_bridge::frb(ignore)]
pub fn chemisorb_debug_row(
    designer: &StructureDesigner,
    item: APIChemisorbDebugRef,
) -> Option<APIChemisorbDebugRow> {
    selected_cache(designer)?
        .debug_row(item.into())
        .map(|v| APIChemisorbDebugRow::from(&v))
}

/// The children of `item` the panel lists (lazy loading: asked for when the
/// item is expanded): a state's next feet, a step's legs.
#[flutter_rust_bridge::frb(ignore)]
pub fn chemisorb_debug_children(
    designer: &StructureDesigner,
    item: APIChemisorbDebugRef,
    show_duplicates: bool,
    show_mirrored: bool,
) -> Vec<APIChemisorbDebugRow> {
    selected_cache(designer).map_or_else(Vec::new, |c| {
        c.debug_children(item.into(), show_duplicates, show_mirrored)
            .iter()
            .map(APIChemisorbDebugRow::from)
            .collect()
    })
}

/// The CLI's `debug-select`: the item named by `path` (`sequential::find_item`)
/// of the node named or numbered `node_identifier`, in `form` (`posed`,
/// `seated`, `relaxed`; empty = the row's default), built and installed on the
/// calling thread. With `show`, the node and its `debug` and `debug_shapes`
/// pins are displayed too (an ordinary, undoable display change). Returns the
/// item's description and its children. Does not refresh.
#[flutter_rust_bridge::frb(ignore)]
pub fn chemisorb_debug_select_named(
    designer: &mut StructureDesigner,
    node_identifier: &str,
    path: &str,
    form: &str,
    show_pins: bool,
) -> Result<String, String> {
    let node_id = resolve_node_identifier(designer, node_identifier)
        .ok_or_else(|| format!("Node not found: {node_identifier}"))?;
    let form = match form.trim() {
        "" => None,
        "posed" => Some(DebugForm::Posed),
        "seated" => Some(DebugForm::Seated),
        "relaxed" => Some(DebugForm::Relaxed),
        other => {
            return Err(format!(
                "'{other}' is not a form: use posed, seated or relaxed"
            ));
        }
    };
    let text = designer.chemisorb_debug_select_blocking(
        &[],
        node_id,
        DebugRowRef::Path(path.to_string()),
        form,
    )?;
    if show_pins {
        if !designer
            .get_scope_network(&[])
            .is_some_and(|n| n.is_node_displayed(node_id))
        {
            designer.set_node_display(node_id, true);
        }
        for pin in [DEBUG_OUTPUT_PIN, DEBUG_SHAPES_OUTPUT_PIN] {
            let shown = designer
                .get_scope_network(&[])
                .and_then(|n| n.get_displayed_pins(node_id))
                .is_some_and(|pins| pins.contains(&(pin as i32)));
            if !shown {
                designer.toggle_output_pin_display(node_id, pin as i32);
            }
        }
    }
    Ok(text)
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

/// Item `item` of the selected `chemisorb` node's debug tree; `None` when no
/// `chemisorb` is selected and evaluated, or past the tree's end.
#[flutter_rust_bridge::frb(sync)]
pub fn get_chemisorb_debug_row(item: APIChemisorbDebugRef) -> Option<APIChemisorbDebugRow> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| chemisorb_debug_row(&cad_instance.structure_designer, item),
            None,
        )
    }
}

/// The children of `item` in the selected `chemisorb` node's debug tree,
/// duplicates only when `show_duplicates`, mirror-pruned legs only when
/// `show_mirrored`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_chemisorb_debug_children(
    item: APIChemisorbDebugRef,
    show_duplicates: bool,
    show_mirrored: bool,
) -> Vec<APIChemisorbDebugRow> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                chemisorb_debug_children(
                    &cad_instance.structure_designer,
                    item,
                    show_duplicates,
                    show_mirrored,
                )
            },
            Vec::new(),
        )
    }
}

/// The items from the root down to `item`: what the panel expands to reveal
/// it (a duplicate row's "jump to the canonical one").
#[flutter_rust_bridge::frb(sync)]
pub fn get_chemisorb_debug_ancestors(item: APIChemisorbDebugRef) -> Vec<APIChemisorbDebugRef> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                selected_cache(&cad_instance.structure_designer).map_or_else(Vec::new, |c| {
                    c.debug_ancestors(item.into())
                        .into_iter()
                        .map(APIChemisorbDebugRef::from)
                        .collect()
                })
            },
            Vec::new(),
        )
    }
}

/// Selects item `item` of a `chemisorb` node's debug tree for its debug
/// pins, in `form` (`None` = the item's default). A view that is geometry
/// alone is built and shown at once (`Ok(None)`); one that needs relaxing
/// starts as a node job, whose id is returned, and shows when it is
/// installed. Not an undo step.
#[flutter_rust_bridge::frb(sync)]
pub fn chemisorb_debug_select(
    scope_path: Vec<u64>,
    node_id: u64,
    item: APIChemisorbDebugRef,
    form: Option<APIChemisorbDebugForm>,
) -> Result<Option<u64>, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let prepared = cad_instance.structure_designer.prepare_chemisorb_debug(
                    &scope_path,
                    node_id,
                    DebugRowRef::Item(item.into()),
                    form.map(DebugForm::from),
                )?;
                match prepared.step {
                    ChemisorbDebugStep::Ready(outcome) => {
                        cad_instance
                            .structure_designer
                            .install_job_result(&prepared.target, outcome)?;
                        refresh_structure_designer_auto(cad_instance);
                        Ok(None)
                    }
                    ChemisorbDebugStep::Job(work) => cad_instance
                        .documents
                        .jobs
                        .start(prepared.target, work)
                        .map(Some),
                }
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// The CLI's `debug-select`, through the AI HTTP server: blocking; the
/// item's description and its children as text.
#[flutter_rust_bridge::frb(sync)]
pub fn chemisorb_debug_select_by_name(
    node_identifier: String,
    path: String,
    form: String,
    show_pins: bool,
) -> Result<String, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = chemisorb_debug_select_named(
                    &mut cad_instance.structure_designer,
                    &node_identifier,
                    &path,
                    &form,
                    show_pins,
                );
                refresh_structure_designer_auto(cad_instance);
                result
            },
            Err("CAD instance not available".to_string()),
        )
    }
}
