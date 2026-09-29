//! Library linking — the FFI surface (`doc/design_library_linking.md` §5.2).
//!
//! Thin wrappers over the `StructureDesigner` operations of
//! `library_link_ops.rs` / `library_refresh_ops.rs`. Rust owns every decision;
//! Flutter decides only *when* to call (focus, poll, menu) and how to show the
//! result. The conversions to the Dart-facing shapes live in
//! `view_builders.rs`, which is not scanned by codegen.

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance_or,
};
use crate::api::common_api_types::APIResult;
use crate::api::structure_designer::structure_designer_api_types::{
    APILibraryMount, APIRefreshReport,
};
use crate::api::structure_designer::view_builders::{
    failed_refresh_report, library_mount_view, refresh_report_view,
};
use atomcad_structure_designer::library_refresh::RefreshReport;

fn api_result(result: Result<(), String>) -> APIResult {
    match result {
        Ok(()) => APIResult {
            success: true,
            error_message: String::new(),
        },
        Err(e) => APIResult {
            success: false,
            error_message: e,
        },
    }
}

fn no_instance() -> APIResult {
    APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
    }
}

/// Runs a refresh-style operation, refreshes the scene when it did anything,
/// and converts its outcome.
fn run_refresh(
    op: impl FnOnce(
        &mut atomcad_structure_designer::structure_designer::StructureDesigner,
    ) -> Result<RefreshReport, String>,
) -> APIRefreshReport {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match op(&mut cad_instance.structure_designer) {
                Ok(report) => {
                    refresh_structure_designer_auto(cad_instance);
                    refresh_report_view(
                        &cad_instance.structure_designer.node_type_registry,
                        &report,
                    )
                }
                Err(e) => failed_refresh_report(e),
            },
            failed_refresh_report("CAD instance not available".to_string()),
        )
    }
}

/// Every linked library of the open design, direct and nested, in mount-path
/// order. No disk access.
#[flutter_rust_bridge::frb(sync)]
pub fn get_linked_libraries() -> Vec<APILibraryMount> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .linked_libraries()
                    .iter()
                    .map(library_mount_view)
                    .collect()
            },
            Vec::new(),
        )
    }
}

/// Why `alias` cannot be used for a new link, or `None` when it can. The
/// link dialog validates as the user types; `link_library` checks again.
#[flutter_rust_bridge::frb(sync)]
pub fn check_library_alias(alias: String) -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .check_new_alias(&alias)
                    .err()
            },
            Some("CAD instance not available".to_string()),
        )
    }
}

/// *File > Link library…*: mounts the `.cnnd` at `path` (absolute, or
/// relative to the design's folder) under `alias`. One undo step.
#[flutter_rust_bridge::frb(sync)]
pub fn link_library(path: String, alias: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance.structure_designer.link_library(&path, &alias);
                if result.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// *Unlink*: refused, with the list of users, while anything in the design
/// uses the library. One undo step.
#[flutter_rust_bridge::frb(sync)]
pub fn unlink_library(alias: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance.structure_designer.unlink_library(&alias);
                if result.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// *Change file…*: points `alias` at another `.cnnd` and brings it in like a
/// refresh (one undo step). A refused retarget comes back as a report whose
/// only content is `errors`.
#[flutter_rust_bridge::frb(sync)]
pub fn retarget_library(alias: String, path: String) -> APIRefreshReport {
    run_refresh(|sd| sd.retarget_library(&alias, &path))
}

/// *Refresh* on a mount folder: re-reads the direct library containing
/// `mount_path`, re-hashing unconditionally.
#[flutter_rust_bridge::frb(sync)]
pub fn refresh_library(mount_path: String) -> APIRefreshReport {
    run_refresh(|sd| sd.refresh_library(&mount_path))
}

/// *File > Refresh all dependencies*.
#[flutter_rust_bridge::frb(sync)]
pub fn refresh_all_dependencies() -> APIRefreshReport {
    run_refresh(|sd| sd.refresh_all_dependencies())
}

/// The automatic check (D7): Flutter calls it on focus, by a ~2 s poll while
/// focused, and after Save As. Whatever changed is refreshed in one undoable
/// command — or, while redo history exists, held and reported in `held`.
/// `None` when there is nothing to tell.
#[flutter_rust_bridge::frb(sync)]
pub fn check_dependencies() -> Option<APIRefreshReport> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let report = cad_instance.structure_designer.check_dependencies()?;
                refresh_structure_designer_auto(cad_instance);
                Some(refresh_report_view(
                    &cad_instance.structure_designer.node_type_registry,
                    &report,
                ))
            },
            None,
        )
    }
}

/// The report of the last file open (D7, D13), drained: a second call
/// returns `None`.
#[flutter_rust_bridge::frb(sync)]
pub fn take_load_library_report() -> Option<APIRefreshReport> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let report = cad_instance.structure_designer.take_load_library_report()?;
                Some(refresh_report_view(
                    &cad_instance.structure_designer.node_type_registry,
                    &report,
                ))
            },
            None,
        )
    }
}
