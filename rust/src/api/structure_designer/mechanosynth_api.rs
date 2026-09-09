//! Kernel seam for the `mechanosynth` node's property panel.
//!
//! Three properties (two file names and a step number) plus one read-only
//! readout the panel cannot compute for itself, because the step count lives in
//! the parsed build script — payload that never crosses the bridge.
//!
//! Each entry point is a thin FRB wrapper over an `#[frb(ignore)]` function
//! taking an explicit `&StructureDesigner`, so the logic is testable without
//! the global `CAD_INSTANCE` (the `field_distribution_api` pattern).

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APIMechanosynthData, APIMechanosynthInfo,
};
use atomcad_structure_designer::nodes::mechanosynth::MechanosynthData;
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
#[flutter_rust_bridge::frb(ignore)]
pub fn mechanosynth_info(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIMechanosynthInfo> {
    let data = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()?;

    let Some((applied, count)) = data.step_counts() else {
        return Some(APIMechanosynthInfo {
            count: 0,
            applied: 0,
            current_op: String::new(),
            current_note: String::new(),
        });
    };

    // "The current step" is the last one applied, `steps[applied - 1]`. At
    // `applied = 0` nothing has run, so there is nothing to name.
    let current = applied
        .checked_sub(1)
        .and_then(|index| data.script.as_ref()?.steps.get(index));

    Some(APIMechanosynthInfo {
        count: count as i32,
        applied: applied as i32,
        current_op: current.map(|step| step.op.clone()).unwrap_or_default(),
        current_note: current
            .and_then(|step| step.note.clone())
            .unwrap_or_default(),
    })
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
        with_cad_instance_or(
            |cad_instance| {
                mechanosynth_info(&cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}
