//! Multiple open documents — the FFI surface
//! (`doc/design_multiple_documents.md` §5.2).
//!
//! Every rule lives in `atomcad_structure_designer::document_set::DocumentSet`,
//! where it is tested; these wrappers cannot be (they need the GPU-backed
//! `CADInstance`). Each one therefore does exactly one `DocumentSet` call, plus
//! the renderer work around a switch (the camera, the scene refresh), plus the
//! conversion through a view builder. A refusal, a lookup or a fallback written
//! here instead would be untested — keep it out.

use crate::api::api_common::{
    CADInstance, apply_camera_settings, refresh_structure_designer_auto, with_cad_instance_or,
    with_mut_cad_instance, with_mut_cad_instance_or,
};
use crate::api::common_api_types::APIResult;
use crate::api::structure_designer::structure_designer_api_types::{
    APIActivateResult, APIDocumentTab, APIOpenDocumentResult,
};
use crate::api::structure_designer::view_builders::{
    activate_result_view, api_result_view, document_tabs_view, failed_open_document_result,
    open_document_result_view, switch_result_view,
};
use atomcad_structure_designer::camera_settings::CameraSettings;
use atomcad_structure_designer::document_set::DocumentId;

fn no_instance() -> APIResult {
    APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
    }
}

/// §5.2 step 0: keeps the outgoing network's stored camera current before a
/// switch parks it. Unlike `sync_camera_to_active_network`, never marks the
/// design dirty: a switch must not dirty a document.
fn store_outgoing_camera(cad_instance: &mut CADInstance) {
    let camera = &cad_instance.renderer.camera;
    let current = CameraSettings {
        eye: camera.eye,
        target: camera.target,
        up: camera.up,
        orthographic: camera.orthographic,
        ortho_half_height: camera.ortho_half_height,
        pivot_point: camera.pivot_point,
        nav_up: camera.nav_up,
        nav_up_label: camera.nav_up_label.clone(),
    };
    if let Some(network) = cad_instance
        .structure_designer
        .get_active_node_network_mut()
        && network.camera_settings.as_ref() != Some(&current)
    {
        network.camera_settings = Some(current);
    }
}

/// §5.2 steps 5 and 7b: shows the (new) active document — its active
/// network's camera, then the refresh the domain call marked as full.
fn show_active(cad_instance: &mut CADInstance) {
    let settings = cad_instance
        .structure_designer
        .active_network_camera_settings();
    apply_camera_settings(&mut cad_instance.renderer, settings.as_ref());
    refresh_structure_designer_auto(cad_instance);
}

/// The open documents in tab order.
#[flutter_rust_bridge::frb(sync)]
pub fn list_documents() -> Vec<APIDocumentTab> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                document_tabs_view(&cad_instance.documents, &cad_instance.structure_designer)
            },
            Vec::new(),
        )
    }
}

/// *File > New*: a fresh Untitled tab after the active one, activated. Set up
/// as `new_project_direct_editing` when `direct_editing`, else as
/// `new_project`. Refused during an open interaction (D4).
#[flutter_rust_bridge::frb(sync)]
pub fn new_document(direct_editing: bool) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                store_outgoing_camera(cad_instance);
                let outcome = cad_instance
                    .documents
                    .new_document(&mut cad_instance.structure_designer, direct_editing);
                if outcome.is_ok() {
                    show_active(cad_instance);
                }
                switch_result_view(outcome)
            },
            no_instance(),
        )
    }
}

/// *File > Open*: activates the tab that has `file_path` open (D5), or loads
/// the file into a new tab — replacing a pristine active Untitled tab. A failed
/// load adds no tab and leaves the active document untouched.
#[flutter_rust_bridge::frb(sync)]
pub fn open_document(file_path: String) -> APIOpenDocumentResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                store_outgoing_camera(cad_instance);
                let outcome = cad_instance
                    .documents
                    .open(&mut cad_instance.structure_designer, &file_path);
                if outcome.is_ok() {
                    show_active(cad_instance);
                    atomcad_structure_designer::recent_files::add_recent_file(&file_path);
                }
                open_document_result_view(outcome, &cad_instance.structure_designer)
            },
            failed_open_document_result("CAD instance not available".to_string()),
        )
    }
}

/// Makes document `id` the active one (the swap of §5.2). Refused during an
/// open interaction (D4) or for an unknown id.
#[flutter_rust_bridge::frb(sync)]
pub fn activate_document(id: u64) -> APIActivateResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                store_outgoing_camera(cad_instance);
                let outcome = cad_instance
                    .documents
                    .activate(&mut cad_instance.structure_designer, DocumentId(id));
                if outcome.is_ok() {
                    show_active(cad_instance);
                }
                activate_result_view(outcome, &cad_instance.structure_designer)
            },
            APIActivateResult {
                result: no_instance(),
                library_report: None,
            },
        )
    }
}

/// Closes document `id` — no dirty check, Flutter asks first. Closing the
/// active tab activates its neighbour first (refused during an open
/// interaction, D4); closing the last tab leaves a fresh Untitled one.
#[flutter_rust_bridge::frb(sync)]
pub fn close_document(id: u64) -> APIActivateResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let closes_active = cad_instance.documents.active_id() == DocumentId(id);
                if closes_active {
                    store_outgoing_camera(cad_instance);
                }
                let outcome = cad_instance
                    .documents
                    .close(&mut cad_instance.structure_designer, DocumentId(id));
                if closes_active && outcome.is_ok() {
                    show_active(cad_instance);
                }
                activate_result_view(outcome, &cad_instance.structure_designer)
            },
            APIActivateResult {
                result: no_instance(),
                library_report: None,
            },
        )
    }
}

/// Moves tab `id` to position `new_index` of the tab order (tab drag).
#[flutter_rust_bridge::frb(sync)]
pub fn move_document(id: u64, new_index: u32) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .documents
                .move_to(DocumentId(id), new_index as usize);
        });
    }
}

/// The CLI's `--document <path-or-id>` guard (D8): success when `spec` names
/// the active document, otherwise an error naming the active one.
#[flutter_rust_bridge::frb(sync)]
pub fn check_document_guard(spec: String) -> APIResult {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                api_result_view(
                    cad_instance
                        .documents
                        .check_guard(&cad_instance.structure_designer, &spec),
                )
            },
            no_instance(),
        )
    }
}
