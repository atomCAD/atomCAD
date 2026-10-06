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
    APIBundleResult, APIDependencyPlan, APILibraryMount, APIRefreshReport, APISaveAsResult,
};
use crate::api::structure_designer::view_builders::{
    bundle_result_view, dependency_plan_view, failed_refresh_report, library_mount_view,
    refresh_report_view, save_as_result_view,
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

// ---------------------------------------------------------------------------
// Save As dependency copy, project bundle, linking by copying (D6, D11)
// ---------------------------------------------------------------------------

/// What saving the design at `target_path` means for its libraries and data
/// files (D11). Flutter shows the Save As dependency dialog when
/// `needs_confirmation`; an `error` means that path is refused. No disk
/// writes.
#[flutter_rust_bridge::frb(sync)]
pub fn collect_file_dependencies(target_path: String) -> APIDependencyPlan {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                dependency_plan_view(
                    cad_instance
                        .structure_designer
                        .collect_file_dependencies(&target_path),
                )
            },
            dependency_plan_view(Err("CAD instance not available".to_string())),
        )
    }
}

/// *Save As* with the dependency copy (D11). `copy = false` is *Save without
/// dependencies*. A conflict is overwritten only when its target is listed in
/// `overwrite_targets` (the conflicts the user saw and chose to overwrite);
/// Rust recomputes the plan here, so a file that appeared since is kept.
/// Dependencies are copied first and the design last.
#[flutter_rust_bridge::frb(sync)]
pub fn save_as_with_dependencies(
    path: String,
    copy: bool,
    overwrite_targets: Vec<String>,
) -> APISaveAsResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                // The network worked on last reaches the file even if the user
                // never left it (`doc/design_network_thumbnails.md` D2).
                crate::api::api_common::capture_active_network_thumbnail(cad_instance);
                // Refused onto a path open in another tab
                // (`doc/design_multiple_documents.md` D5).
                let result = cad_instance.documents.save_as_with_dependencies(
                    &mut cad_instance.structure_designer,
                    &path,
                    copy,
                    &overwrite_targets,
                );
                let view = save_as_result_view(result);
                if view.success {
                    atomcad_structure_designer::recent_files::add_recent_file(&path);
                }
                view
            },
            save_as_result_view(Err("CAD instance not available".to_string())),
        )
    }
}

/// *File > Export project bundle…*: a zip of the design (as in memory) and
/// every relative dependency, laid out relative to the deepest folder
/// containing them all. External and missing files are listed, not included.
#[flutter_rust_bridge::frb(sync)]
pub fn export_project_bundle(zip_path: String) -> APIBundleResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                bundle_result_view(
                    cad_instance
                        .structure_designer
                        .export_project_bundle(&zip_path),
                )
            },
            bundle_result_view(Err("CAD instance not available".to_string())),
        )
    }
}

/// True when the `.cnnd` at `path` has no path relative to the design's
/// folder (another drive), so the link dialog offers to copy it (D6).
#[flutter_rust_bridge::frb(sync)]
pub fn library_needs_copy(path: String) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.library_needs_copy(&path),
            false,
        )
    }
}

/// Copies the library at `path` (with its own libraries and data files) to
/// `target_rel_path` next to the design, then links the copy under `alias`
/// (one undo step). Refused, before anything is copied, when a different
/// file already sits at any target.
#[flutter_rust_bridge::frb(sync)]
pub fn link_library_copying(path: String, target_rel_path: String, alias: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance
                    .structure_designer
                    .link_library_copying(&path, &target_rel_path, &alias)
                    .map(|_| ());
                if result.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                api_result(result)
            },
            no_instance(),
        )
    }
}

// ---------------------------------------------------------------------------
// Make local copy, rename alias (§10, D3)
// ---------------------------------------------------------------------------

/// *Make local copy* on a mount folder: the direct library `alias` (with
/// its own links) becomes part of the design, as it is in memory. One undo
/// step; refused while the library is not loaded or something refers to a
/// name it does not define.
#[flutter_rust_bridge::frb(sync)]
pub fn make_library_local(alias: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance.structure_designer.make_library_local(&alias);
                if result.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                api_result(result)
            },
            no_instance(),
        )
    }
}

/// *Rename alias…* on a mount folder: moves the direct library `alias` and
/// every reference to it under `new_alias`. One undo step.
#[flutter_rust_bridge::frb(sync)]
pub fn rename_library_alias(alias: String, new_alias: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance
                    .structure_designer
                    .rename_library_alias(&alias, &new_alias);
                if result.is_ok() {
                    refresh_structure_designer_auto(cad_instance);
                }
                api_result(result)
            },
            no_instance(),
        )
    }
}
