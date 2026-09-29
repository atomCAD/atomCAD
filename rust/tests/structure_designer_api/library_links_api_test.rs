//! Library linking, Phases 4 and 5 (`doc/design_library_linking.md` §11):
//! the view fields the Flutter UI reads — `read_only` on the network list,
//! the add-node lists without transitive mounts, the mount / report shapes,
//! and the Save As dependency plan / outcome / bundle shapes.
//!
//! The FFI wrappers in `library_links_api.rs` need the global `CADInstance`,
//! whose renderer needs a GPU, so they are not driven here; the operations
//! under them are tested in
//! `crates/atomcad-structure-designer/tests/structure_designer/library_links_*`.

use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIMountStatus, APINodeCategoryView,
};
use rust_lib_flutter_cad::api::structure_designer::view_builders::{
    bundle_result_view, dependency_plan_view, display_path, failed_refresh_report,
    get_compatible_node_types, get_node_networks_with_validation, get_node_type_views,
    library_mount_view, refresh_report_view, save_as_result_view,
};
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// `host.cnnd` (links `a` = `lib_a.cnnd`, which links `common`, and `b`),
/// opened from a temp copy of the fixtures.
fn open_host() -> (tempfile::TempDir, StructureDesigner) {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixture_path("library_linking"), dir.path());
    let mut d = StructureDesigner::new();
    d.load_node_networks(&dir.path().join("host.cnnd").to_string_lossy())
        .unwrap();
    (dir, d)
}

fn names(views: &[APINodeCategoryView]) -> Vec<String> {
    views
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.clone()))
        .collect()
}

#[test]
fn read_only_is_set_exactly_on_networks_under_a_mount() {
    let (_dir, d) = open_host();
    let list = get_node_networks_with_validation(&d.node_type_registry);
    assert!(list.iter().any(|n| n.name == "Main" && !n.read_only));
    assert!(list.iter().any(|n| n.name == "a.foo" && n.read_only));
    assert!(
        list.iter()
            .any(|n| n.name == "a.common.slab" && n.read_only)
    );
    for n in &list {
        let linked = d.is_linked_name(&n.name);
        assert_eq!(n.read_only, linked, "{}", n.name);
    }
    assert!(list.iter().filter(|n| n.read_only).count() >= 2);
    assert!(list.iter().any(|n| !n.read_only));
}

#[test]
fn the_add_node_lists_offer_direct_mounts_but_not_transitive_ones() {
    let (_dir, d) = open_host();
    let registry = &d.node_type_registry;
    assert!(registry.node_networks.contains_key("a.common.slab"));

    let offered = names(&get_node_type_views(registry));
    assert!(offered.contains(&"a.foo".to_string()));
    assert!(offered.contains(&"Main".to_string()));
    assert!(
        !offered.iter().any(|n| n.starts_with("a.common.")),
        "transitive mount offered: {:?}",
        offered
    );

    // The drag-aware popup follows the same rule.
    for from_output in [true, false] {
        let compatible = names(&get_compatible_node_types(
            registry,
            &atomcad_structure_designer::data_type::DataType::Blueprint,
            from_output,
        ));
        assert!(
            !compatible.iter().any(|n| n.starts_with("a.common.")),
            "transitive mount offered: {:?}",
            compatible
        );
    }
}

#[test]
fn mount_views_carry_the_label_parts_and_the_status() {
    let (_dir, d) = open_host();
    let mounts: Vec<_> = d
        .linked_libraries()
        .iter()
        .map(library_mount_view)
        .collect();
    let a = mounts.iter().find(|m| m.mount_path == "a").unwrap();
    assert!(a.direct);
    assert_eq!(a.parent, None);
    assert_eq!(a.file_name, "lib_a.cnnd");
    assert_eq!(a.rel_path, "lib_a.cnnd");
    assert_eq!(a.status, APIMountStatus::Loaded);
    let common = mounts.iter().find(|m| m.mount_path == "a.common").unwrap();
    assert!(!common.direct);
    assert_eq!(common.parent.as_deref(), Some("a"));
    assert_eq!(common.file_name, "common.cnnd");
}

#[test]
fn a_missing_library_is_reported_with_its_status_and_frozen_nodes() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixture_path("library_linking"), dir.path());
    let mut d = StructureDesigner::new();
    d.load_node_networks(&dir.path().join("host_missing.cnnd").to_string_lossy())
        .unwrap();
    let mounts: Vec<_> = d
        .linked_libraries()
        .iter()
        .map(library_mount_view)
        .collect();
    assert!(
        mounts
            .iter()
            .any(|m| m.direct && m.status == APIMountStatus::Missing)
    );

    let report = d.take_load_library_report().expect("an open report");
    let view = refresh_report_view(&d.node_type_registry, &report);
    assert!(!view.is_clean);
    assert!(
        view.status_changes
            .iter()
            .any(|c| c.status == APIMountStatus::Missing)
    );
    assert!(!view.frozen_nodes.is_empty());
    for node in &view.frozen_nodes {
        assert!(!node.node_label.is_empty(), "every frozen node has a label");
    }
}

#[test]
fn a_failed_operation_crosses_as_a_report_with_only_its_error() {
    let view = failed_refresh_report("nope".to_string());
    assert_eq!(view.errors, vec!["nope".to_string()]);
    assert!(!view.is_clean);
    assert!(view.refreshed_mounts.is_empty());
}

// ---------------------------------------------------------------------------
// Phase 5: Save As dependency plan, outcome, bundle
// ---------------------------------------------------------------------------

#[test]
fn paths_are_shown_without_the_verbatim_prefix() {
    assert_eq!(
        display_path(Path::new(r"\\?\C:\work\lib.cnnd")),
        r"C:\work\lib.cnnd"
    );
    assert_eq!(display_path(Path::new("/work/lib.cnnd")), "/work/lib.cnnd");
}

#[test]
fn the_dependency_plan_crosses_with_its_groups_and_statuses() {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
        APIDependencyGroup, APIDependencyKind, APIDependencyStatus,
    };
    let (dir, d) = open_host();
    // Deeper than the host's folder: the host's own links stay inside, the
    // copies land in the new folder.
    let target = dir.path().join("moved/deeper/host.cnnd");
    let plan = dependency_plan_view(d.collect_file_dependencies(&target.to_string_lossy()));
    assert!(plan.error.is_none(), "{:?}", plan.error);
    assert!(plan.needs_confirmation);
    let lib_a = plan
        .entries
        .iter()
        .find(|e| e.source_abs.ends_with("lib_a.cnnd"))
        .expect("lib_a listed");
    assert_eq!(lib_a.kind, APIDependencyKind::Library);
    assert_eq!(lib_a.group, APIDependencyGroup::Inside);
    assert_eq!(lib_a.status, APIDependencyStatus::WillCopy);
    assert_eq!(lib_a.rel_path.as_deref(), Some("lib_a.cnnd"));
    let target_abs = lib_a.target_abs.as_ref().unwrap();
    assert!(!target_abs.starts_with(r"\\?\"), "{}", target_abs);
    assert!(
        target_abs
            .replace('\\', "/")
            .ends_with("moved/deeper/lib_a.cnnd")
    );
}

#[test]
fn a_refused_plan_crosses_as_an_error_with_no_entries() {
    let plan = dependency_plan_view(Err("refused".to_string()));
    assert_eq!(plan.error.as_deref(), Some("refused"));
    assert!(plan.entries.is_empty() && !plan.needs_confirmation);
}

#[test]
fn a_failed_copy_crosses_as_a_failure_listing_the_copies() {
    use atomcad_structure_designer::file_dependencies::CopyOutcome;
    let view = save_as_result_view(Ok(CopyOutcome {
        copied: vec![Path::new("a/one.cnnd").to_path_buf()],
        error: Some("cannot copy 'x' to 'y': denied".to_string()),
        ..Default::default()
    }));
    assert!(!view.success);
    assert!(
        view.error_message.contains("not saved"),
        "{}",
        view.error_message
    );
    assert!(
        view.error_message.contains("one.cnnd"),
        "{}",
        view.error_message
    );
    assert_eq!(view.copied.len(), 1);

    let ok = save_as_result_view(Ok(CopyOutcome::default()));
    assert!(ok.success && ok.error_message.is_empty());
    assert!(!bundle_result_view(Err("no".to_string())).success);
}
