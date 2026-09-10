//! Kernel seam for the `mechanosynth` node's property panel.
//!
//! Three properties (two file names and a step number) plus one read-only
//! readout the panel cannot compute for itself, because the step count, the
//! current step's metadata and the script's chapter structure all live in the
//! parsed build script — payload that never crosses the bridge. The readout
//! follows the wired `build_file` and `step` pins when they are connected, so
//! it describes what the node evaluates rather than what it stores.
//!
//! Each entry point is a thin FRB wrapper over an `#[frb(ignore)]` function
//! taking an explicit `&StructureDesigner`, so the logic is testable without
//! the global `CAD_INSTANCE` (the `field_distribution_api` pattern).

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
    with_mut_cad_instance_or,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APIMechanosynthChapter, APIMechanosynthData, APIMechanosynthInfo,
};
use atomcad_crystolecule::mechanosynth::{BuildScript, NO_LAYER, NO_SITE, steps_applied};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::mechanosynth::{MechanosynthData, load_script_at};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_util::path_utils::get_parent_directory;

/// The design file's directory, which relative file names resolve against.
fn design_dir(designer: &StructureDesigner) -> Option<String> {
    designer
        .node_type_registry
        .design_file_name
        .as_ref()
        .and_then(|design_path| get_parent_directory(design_path))
}

/// The stored data of a `mechanosynth` node, against an explicit designer.
#[flutter_rust_bridge::frb(ignore)]
pub fn mechanosynth_data(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIMechanosynthData> {
    let data = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()?;
    Some(APIMechanosynthData {
        ops_file: data.ops_file.clone(),
        build_file: data.build_file.clone(),
        step: data.step,
    })
}

/// Writes the stored data of a `mechanosynth` node, against an explicit
/// designer.
///
/// A file name that actually **changes** drops its parsed cache and is re-read
/// here; one that does not keeps it. The property setters fire on every focus
/// loss of a path field, not only on a real edit, so re-reading unconditionally
/// would re-parse both files on every stray click, and dropping the cache
/// unconditionally would leave the node reporting a missing library for a file
/// it had loaded seconds earlier (`project_import_node_payload_wipe`).
///
/// Undo comes from the shared `SetNodeDataCommand` that
/// `set_node_network_data_scoped` pushes; its restore path runs the node's
/// loader with the design directory, which repopulates the caches.
#[flutter_rust_bridge::frb(ignore)]
pub fn set_mechanosynth_data(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    data: &APIMechanosynthData,
) {
    let design_dir = design_dir(designer);

    let Some(current) = designer
        .get_node_network_data_scoped(scope_path, node_id)
        .and_then(|node_data| node_data.as_any_ref().downcast_ref::<MechanosynthData>())
    else {
        return;
    };

    let mut updated = current
        .with_ops_file(data.ops_file.clone())
        .with_build_file(data.build_file.clone());
    updated.step = data.step;
    updated.reload_missing(design_dir.as_deref());

    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(updated));
}

/// The panel's read-only readout, against an explicit designer.
///
/// Reports what the node **evaluates**, not only what it stores: a build
/// script arriving on the wired `build_file` pin and a step arriving on the
/// wired `step` pin win over the stored properties, exactly as in the node's
/// `eval`. Without this a demo that switches the file name in by wire (one
/// `mechanosynth` node fed by a `switch` on two `string` nodes) replays fine
/// but shows "no build script loaded", a slider with no range and no step
/// readout. The two argument evaluations walk the upstream cone on every
/// panel rebuild, which for a file-name wire is a string or a switch and for
/// the step an int — see `StructureDesigner::evaluate_node_argument` for why
/// there is no memo to lean on. A wired file that fails to load reports the
/// all-zero readout; the failure itself reaches the user on the result pin.
#[flutter_rust_bridge::frb(ignore)]
pub fn mechanosynth_info(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIMechanosynthInfo> {
    let design_dir = design_dir(designer);
    // Cloned out rather than borrowed: the two argument evaluations below need
    // the designer mutably.
    let stored = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()?
        .clone();

    // Pin 2 is `build_file`. `None` means nothing is wired, so the stored
    // property applies: the parsed cache, else the stored name re-read (the
    // text-format edit path drops the cache, as `eval` knows).
    let script: Option<BuildScript> = match designer.evaluate_node_argument(scope_path, node_id, 2)
    {
        NetworkResult::None => stored.script.clone().or_else(|| {
            stored
                .build_file
                .as_deref()
                .and_then(|name| load_script_at(name, design_dir.as_deref()).ok())
        }),
        NetworkResult::String(name) => load_script_at(&name, design_dir.as_deref()).ok(),
        _ => None,
    };
    // Pin 3 is `step`.
    let step = match designer.evaluate_node_argument(scope_path, node_id, 3) {
        NetworkResult::Int(step) => step,
        _ => stored.step,
    };

    let Some(script) = script else {
        return Some(APIMechanosynthInfo {
            count: 0,
            applied: 0,
            current_op: String::new(),
            current_note: String::new(),
            current_method: String::new(),
            current_phase: String::new(),
            current_layer: NO_LAYER,
            current_site: NO_SITE,
            chapters: Vec::new(),
        });
    };
    let count = script.steps.len();
    let applied = steps_applied(step, count);

    // "The current step" is the last one applied, `steps[applied - 1]`. At
    // `applied = 0` nothing has run, so there is nothing to name.
    let current = applied
        .checked_sub(1)
        .and_then(|index| script.steps.get(index));

    Some(APIMechanosynthInfo {
        count: count as i32,
        applied: applied as i32,
        current_op: current.map(|step| step.op.clone()).unwrap_or_default(),
        current_note: current
            .and_then(|step| step.note.clone())
            .unwrap_or_default(),
        current_method: current.map(|step| step.method.clone()).unwrap_or_default(),
        current_phase: current.map(|step| step.phase.clone()).unwrap_or_default(),
        current_layer: current.map_or(NO_LAYER, |step| step.layer),
        current_site: current.map_or(NO_SITE, |step| step.site),
        chapters: chapters(&script),
    })
}

/// Splits a script into chapters: maximal runs of consecutive steps sharing a
/// `(phase, layer)`.
///
/// Computed from the cached script on every info call, which is one pass over a
/// few hundred steps — cheaper than the two argument evaluations the caller has
/// already done. Steps that name no phase and no layer are chapters too, so the
/// result always covers the whole script and the panel never has to reason
/// about gaps.
#[flutter_rust_bridge::frb(ignore)]
pub fn chapters(script: &BuildScript) -> Vec<APIMechanosynthChapter> {
    let mut chapters: Vec<APIMechanosynthChapter> = Vec::new();
    for (index, step) in script.steps.iter().enumerate() {
        let number = index as i32 + 1;
        match chapters.last_mut() {
            Some(open) if open.phase == step.phase && open.layer == step.layer => {
                open.last_step = number;
            }
            _ => chapters.push(APIMechanosynthChapter {
                phase: step.phase.clone(),
                layer: step.layer,
                first_step: number,
                last_step: number,
            }),
        }
    }
    chapters
}

// ============================================================================
// Flutter entry points
// ============================================================================

#[flutter_rust_bridge::frb(sync)]
pub fn get_mechanosynth_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMechanosynthData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                mechanosynth_data(&cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_mechanosynth_node_data(scope_path: Vec<u64>, node_id: u64, data: APIMechanosynthData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            set_mechanosynth_data(
                &mut cad_instance.structure_designer,
                &scope_path,
                node_id,
                &data,
            );
            // The refresh paths do not validate, so a stale error would linger
            // until an unrelated edit (`project_refresh_does_not_validate`).
            cad_instance.structure_designer.validate_active_network();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_mechanosynth_info(scope_path: Vec<u64>, node_id: u64) -> Option<APIMechanosynthInfo> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                mechanosynth_info(&mut cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}
