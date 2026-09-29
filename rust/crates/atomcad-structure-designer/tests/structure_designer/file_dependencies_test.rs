//! Library linking, Phase 5 (`doc/design_library_linking.md` D11, §11):
//! the Save As dependency copy, the project bundle and linking a library by
//! copying it next to the design.
//!
//! Every test arms the disk tripwire (O3) on the whole workspace, with the
//! allowed list equal to exactly the plan's *will copy* targets (plus the
//! *conflict* targets it chose to overwrite) and the new design file — Save
//! As is where a bug would overwrite a file of the user's that has nothing to
//! do with linking.
//!
//! The workspace is the one §11 names: `ws/proj1/host.cnnd` linking
//! `../libs/demolib.cnnd` and `libs_local/b.cnnd`; `ws/libs/demolib.cnnd`
//! linking `common.cnnd` and reading `tip.xyz`; the host reading
//! `data/local.xyz`, and one `import_cif` with an absolute path (external).

use super::library_links_support::*;
use atomcad_structure_designer::file_dependencies::{
    DependencyGroup, DependencyKind, DependencyPlan, DependencyStatus, canonical_or_lexical,
    path_key, read_zip, zip_bytes,
};
use atomcad_structure_designer::library_links::{FileMeta, LinkFs, MountStatus, RealFs};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TIP: &str = "2\ntip\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\n";
const LOCAL: &str = "3\nlocal\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\nH 1.0 0.0 0.0\n";

const HOST_MAIN: &str = "tip = `demolib.tip` {}
sev = `demolib.seven` {}
three = `b.three` {}
loc = import_xyz { file_name: \"data/local.xyz\" }
cif = import_cif {}
output tip
";

/// The nodes of the host's `Main` whose values the tests compare.
const PROBES: [&str; 5] = ["tip", "sev", "three", "loc", "cif"];

struct Ws {
    _dir: tempfile::TempDir,
    /// The temp dir, canonical: the tripwire's root and the form every
    /// expected path is written in (plan paths are canonical).
    root: PathBuf,
    /// `<temp>/ws`, as the operations see it (not canonical).
    ws: PathBuf,
    host: PathBuf,
}

impl Ws {
    /// `rel` under the canonical temp root.
    fn c(&self, rel: &str) -> PathBuf {
        canonical_or_lexical(&RealFs, &self.root.join(rel))
    }
    /// `rel` under the operational temp root (for passing to operations).
    fn p(&self, rel: &str) -> PathBuf {
        self.ws.parent().unwrap().join(rel)
    }
}

fn write_design(path: &Path, links: &[(&str, &str)], networks: &[(&str, &str)]) {
    let mut d = new_design(path);
    for (rel, alias) in links {
        d.link_library(rel, alias).unwrap();
    }
    for (name, code) in networks {
        edit(&mut d, name, code);
    }
    save(&mut d);
}

/// Sets a file-reading node's `file_name` through its own saver and loader,
/// as the property panel's file picker does (the text format ignores a
/// literal on that pin for `import_cif`).
fn set_file_name(d: &mut StructureDesigner, node: &str, file: &str) {
    let id = node_id(d, "Main", node);
    let type_name = d.node_type_registry.node_networks["Main"].nodes[&id]
        .node_type_name
        .clone();
    let saver = d.node_type_registry.node_data_saver_for(&type_name);
    let loader = d.node_type_registry.node_data_loader_for(&type_name);
    let n = d
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&id)
        .unwrap();
    let mut json = saver(n.data.as_mut(), None).unwrap();
    json["file_name"] = serde_json::json!(file);
    n.data = loader(&json, None).unwrap();
}

fn workspace() -> Ws {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    let libs = ws.join("libs");
    std::fs::create_dir_all(&libs).unwrap();
    std::fs::write(libs.join("tip.xyz"), TIP).unwrap();
    write_design(
        &libs.join("common.cnnd"),
        &[],
        &[("seven", "s = int { value: 7 }\noutput s\n")],
    );
    write_design(
        &libs.join("demolib.cnnd"),
        &[("common.cnnd", "common")],
        &[
            (
                "tip",
                "t = import_xyz { file_name: \"tip.xyz\" }\noutput t\n",
            ),
            ("seven", "s = `common.seven` {}\noutput s\n"),
        ],
    );
    let proj = ws.join("proj1");
    std::fs::create_dir_all(proj.join("data")).unwrap();
    std::fs::write(proj.join("data/local.xyz"), LOCAL).unwrap();
    write_design(
        &proj.join("libs_local/b.cnnd"),
        &[],
        &[("three", "t = int { value: 3 }\noutput t\n")],
    );
    let cif = fixture_path("cif/diamond.cif")
        .to_string_lossy()
        .replace('\\', "/");
    let host = proj.join("host.cnnd");
    let mut d = new_design(&host);
    d.link_library("../libs/demolib.cnnd", "demolib").unwrap();
    d.link_library("libs_local/b.cnnd", "b").unwrap();
    edit(&mut d, "Main", HOST_MAIN);
    set_file_name(&mut d, "cif", &cif);
    save(&mut d);
    let root = std::fs::canonicalize(dir.path()).unwrap();
    Ws {
        _dir: dir,
        root,
        ws,
        host,
    }
}

fn values(d: &mut StructureDesigner) -> Vec<String> {
    d.set_active_node_network_name(Some("Main".to_string()));
    PROBES
        .iter()
        .map(|name| {
            let id = node_id(d, "Main", name);
            d.evaluate_node_output(&[], id, 0).to_display_string()
        })
        .collect()
}

fn plan(d: &StructureDesigner, new_file: &Path) -> DependencyPlan {
    d.collect_file_dependencies(&new_file.to_string_lossy())
        .unwrap_or_else(|e| panic!("plan refused: {}", e))
}

/// The plan entry whose source file is named `file_name`.
fn entry<'a>(
    plan: &'a DependencyPlan,
    file_name: &str,
) -> &'a atomcad_structure_designer::file_dependencies::Dependency {
    plan.entries
        .iter()
        .find(|e| e.source.file_name().unwrap().to_string_lossy() == file_name)
        .unwrap_or_else(|| panic!("no entry for {}: {:#?}", file_name, plan.entries))
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

/// What O3 must allow after copying `plan` (overwriting `overwritten`
/// conflicts) and writing the design at `new_file`.
fn allowed(plan: &DependencyPlan, overwritten: &[PathBuf], new_file: PathBuf) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = plan
        .with_status(DependencyStatus::WillCopy)
        .filter_map(|e| e.target.clone())
        .collect();
    out.extend(overwritten.iter().cloned());
    out.push(new_file);
    out
}

fn assert_mounts_loaded(d: &StructureDesigner) {
    let mounts = d.linked_libraries();
    assert_eq!(mounts.len(), 3, "{:#?}", mounts);
    for m in mounts {
        assert_eq!(m.status, MountStatus::Loaded, "{}", m.mount_path);
    }
}

fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| panic!("{}: {}", p.display(), e))
}

// ---------------------------------------------------------------------------
// Collection and targets
// ---------------------------------------------------------------------------

#[test]
fn collection_is_transitive_and_complete() {
    let ws = workspace();
    let d = open(&ws.host);
    let plan = plan(&d, &ws.p("ws/proj2/host.cnnd"));
    let mut names: Vec<String> = plan
        .entries
        .iter()
        .map(|e| e.source.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "b.cnnd",
            "common.cnnd",
            "demolib.cnnd",
            "diamond.cif",
            "local.xyz",
            "tip.xyz"
        ],
        "each dependency once: {:#?}",
        plan.entries
    );
    for lib in ["b.cnnd", "common.cnnd", "demolib.cnnd"] {
        assert_eq!(entry(&plan, lib).kind, DependencyKind::Library);
    }
    for data in ["diamond.cif", "local.xyz", "tip.xyz"] {
        assert_eq!(entry(&plan, data).kind, DependencyKind::DataFile);
    }
    let cif = entry(&plan, "diamond.cif");
    assert_eq!(cif.group, DependencyGroup::External);
    assert_eq!(cif.status, DependencyStatus::External);
    assert!(cif.target.is_none());
    assert!(!plan.has_wired_paths);
}

#[test]
fn a_sibling_folder_reuses_the_shared_libraries_and_copies_the_rest() {
    let ws = workspace();
    let d = open(&ws.host);
    let tripwire = DiskTripwire::arm(&ws.root);
    let plan = plan(&d, &ws.p("ws/proj2/host.cnnd"));
    for lib in ["demolib.cnnd", "common.cnnd", "tip.xyz"] {
        let e = entry(&plan, lib);
        assert_eq!(e.status, DependencyStatus::AlreadyThere, "{}", lib);
        assert_eq!(e.group, DependencyGroup::Outside, "{}", lib);
        assert!(same_path(e.target.as_ref().unwrap(), &e.source), "{}", lib);
    }
    let b = entry(&plan, "b.cnnd");
    assert_eq!(b.status, DependencyStatus::WillCopy);
    assert_eq!(b.group, DependencyGroup::Inside);
    assert_eq!(b.rel_path.as_deref(), Some("libs_local/b.cnnd"));
    assert!(same_path(
        b.target.as_ref().unwrap(),
        &ws.c("ws/proj2/libs_local/b.cnnd")
    ));
    let local = entry(&plan, "local.xyz");
    assert_eq!(local.status, DependencyStatus::WillCopy);
    assert!(same_path(
        local.target.as_ref().unwrap(),
        &ws.c("ws/proj2/data/local.xyz")
    ));
    assert!(plan.needs_confirmation());
    ok(tripwire.check_only_changed(&[]));

    // The same folder: every dependency is already there, no dialog.
    let same = self::plan(&d, &ws.p("ws/proj1/renamed.cnnd"));
    assert!(!same.needs_confirmation(), "{:#?}", same.entries);
}

#[test]
fn a_deeper_folder_puts_the_parent_libraries_outside_it() {
    let ws = workspace();
    let d = open(&ws.host);
    let plan = plan(&d, &ws.p("other/deep/proj/host.cnnd"));
    for (name, target) in [
        ("demolib.cnnd", "other/deep/libs/demolib.cnnd"),
        ("common.cnnd", "other/deep/libs/common.cnnd"),
        ("tip.xyz", "other/deep/libs/tip.xyz"),
    ] {
        let e = entry(&plan, name);
        assert_eq!(e.group, DependencyGroup::Outside, "{}", name);
        assert_eq!(e.status, DependencyStatus::WillCopy, "{}", name);
        assert!(
            same_path(e.target.as_ref().unwrap(), &ws.c(target)),
            "{}",
            name
        );
    }
    assert_eq!(entry(&plan, "b.cnnd").group, DependencyGroup::Inside);
}

#[test]
fn a_target_above_the_filesystem_root_is_refused() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let tripwire = DiskTripwire::arm(&ws.root);
    let root = ws.ws.ancestors().last().unwrap().to_path_buf();
    let err = d
        .collect_file_dependencies(&root.join("p5_host.cnnd").to_string_lossy())
        .unwrap_err();
    assert!(err.contains("above the filesystem root"), "{}", err);
    let err = d
        .save_as_with_dependencies(&root.join("p5_host.cnnd").to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("above the filesystem root"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
    assert!(!root.join("p5_host.cnnd").exists());
}

// ---------------------------------------------------------------------------
// Never overwrite what is not in the plan
// ---------------------------------------------------------------------------

/// A small design `w/h.cnnd` reading the data files `reads` (paths as stored).
fn small_design(root: &Path, reads: &[&str]) -> StructureDesigner {
    let host = root.join("w/h.cnnd");
    let mut d = new_design(&host);
    let code: String = reads
        .iter()
        .enumerate()
        .map(|(i, r)| format!("x{} = import_xyz {{ file_name: \"{}\" }}\n", i, r))
        .collect();
    edit(&mut d, "Main", &code);
    save(&mut d);
    d
}

#[test]
fn saving_over_a_dependency_is_refused() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let tripwire = DiskTripwire::arm(&ws.root);
    let target = ws.p("ws/proj1/libs_local/b.cnnd");
    let err = d
        .save_as_with_dependencies(&target.to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("would overwrite"), "{}", err);
    assert!(err.contains("b.cnnd"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn a_copy_onto_the_new_design_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("w")).unwrap();
    std::fs::write(dir.path().join("w/a.xyz"), TIP).unwrap();
    let mut d = small_design(dir.path(), &["a.xyz"]);
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let tripwire = DiskTripwire::arm(&root);
    let new_file = dir.path().join("other/a.xyz");
    let err = d
        .save_as_with_dependencies(&new_file.to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("onto the design file itself"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn a_copy_onto_the_current_design_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("h.cnnd"), "not the design").unwrap();
    let mut d = small_design(dir.path(), &["../h.cnnd"]);
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let tripwire = DiskTripwire::arm(&root);
    let err = d
        .save_as_with_dependencies(
            &dir.path().join("w/sub/n.cnnd").to_string_lossy(),
            true,
            &[],
        )
        .unwrap_err();
    assert!(err.contains("current file"), "{}", err);
    assert!(err.contains("h.cnnd"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn a_copy_onto_another_dependency_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("w")).unwrap();
    std::fs::write(dir.path().join("w/a.xyz"), TIP).unwrap();
    std::fs::write(dir.path().join("a.xyz"), LOCAL).unwrap();
    let mut d = small_design(dir.path(), &["a.xyz", "../a.xyz"]);
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let tripwire = DiskTripwire::arm(&root);
    let err = d
        .save_as_with_dependencies(&dir.path().join("n.cnnd").to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("also depends on"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn a_folder_at_a_target_is_refused() {
    let ws = workspace();
    let mut d = open(&ws.host);
    std::fs::create_dir_all(ws.p("ws/proj2/data/local.xyz")).unwrap();
    let tripwire = DiskTripwire::arm(&ws.root);
    let err = d
        .save_as_with_dependencies(&ws.p("ws/proj2/host.cnnd").to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("is a folder"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
    assert!(!ws.p("ws/proj2/host.cnnd").exists());
}

/// A link at a target that points elsewhere is not written through. Creating
/// a symlink needs a privilege on Windows; without it the case is skipped.
#[test]
fn a_link_at_a_target_is_refused() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let elsewhere = ws.p("elsewhere.cnnd");
    std::fs::write(&elsewhere, "someone else's file").unwrap();
    let link = ws.p("ws/proj2/libs_local/b.cnnd");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&elsewhere, &link);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&elsewhere, &link);
    if made.is_err() {
        eprintln!("skipped: cannot create a symlink here");
        return;
    }
    let err = d
        .save_as_with_dependencies(&ws.p("ws/proj2/host.cnnd").to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("link"), "{}", err);
    assert_eq!(read(&elsewhere), b"someone else's file");
}

/// Two sources that differ only by case are one file on Windows; were they
/// ever listed apart, their copies would collide, and the plan refuses that.
#[cfg(windows)]
#[test]
fn two_copies_to_one_target_are_refused() {
    use atomcad_structure_designer::file_dependencies::{DependencySource, plan_move};
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("w/a")).unwrap();
    std::fs::write(root.join("w/a/x.xyz"), TIP).unwrap();
    let source = |p: &str| DependencySource {
        kind: DependencyKind::DataFile,
        path: root.join(p),
        external: false,
    };
    let err = plan_move(
        &RealFs,
        &[source("w/a/x.xyz"), source("w/A/X.xyz")],
        false,
        Some(&root.join("w/h.cnnd")),
        &root.join("n/h.cnnd"),
    )
    .unwrap_err();
    assert!(err.contains("both be copied"), "{}", err);
}

// ---------------------------------------------------------------------------
// Conflicts
// ---------------------------------------------------------------------------

/// A different `common.cnnd` (its `seven` is 8) and an identical `tip.xyz`
/// already at the deep target.
fn prepare_deep_target(ws: &Ws) {
    write_design(
        &ws.p("other/deep/libs/common.cnnd"),
        &[],
        &[("seven", "s = int { value: 8 }\noutput s\n")],
    );
    std::fs::write(ws.p("other/deep/libs/tip.xyz"), TIP).unwrap();
}

#[test]
fn conflicts_are_kept_unless_named_and_the_plan_is_recomputed_at_copy_time() {
    let ws = workspace();
    let mut d = open(&ws.host);
    prepare_deep_target(&ws);
    let new_file = ws.p("other/deep/proj/host.cnnd");
    let shown = plan(&d, &new_file);
    assert_eq!(
        entry(&shown, "common.cnnd").status,
        DependencyStatus::Conflict
    );
    assert_eq!(
        entry(&shown, "tip.xyz").status,
        DependencyStatus::AlreadyThere
    );
    assert_eq!(
        entry(&shown, "local.xyz").status,
        DependencyStatus::WillCopy
    );
    // A file appears at a *will copy* target after the dialog was shown.
    let late = ws.p("other/deep/proj/data/local.xyz");
    std::fs::create_dir_all(late.parent().unwrap()).unwrap();
    std::fs::write(&late, "late arrival").unwrap();
    let common_before = read(&ws.p("other/deep/libs/common.cnnd"));

    let tripwire = DiskTripwire::arm(&ws.root);
    let now = plan(&d, &new_file);
    let outcome = d
        .save_as_with_dependencies(&new_file.to_string_lossy(), true, &[])
        .unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    ok(tripwire.check_only_changed(&allowed(&now, &[], ws.c("other/deep/proj/host.cnnd"))));
    assert_eq!(read(&ws.p("other/deep/libs/common.cnnd")), common_before);
    assert_eq!(read(&late), b"late arrival", "a late conflict is kept");
    assert_eq!(outcome.kept.len(), 2, "{:#?}", outcome);
}

#[test]
fn a_named_conflict_is_overwritten() {
    let ws = workspace();
    let mut d = open(&ws.host);
    prepare_deep_target(&ws);
    let new_file = ws.p("other/deep/proj/host.cnnd");
    let plan = plan(&d, &new_file);
    let conflict = entry(&plan, "common.cnnd").target.clone().unwrap();
    let tripwire = DiskTripwire::arm(&ws.root);
    let outcome = d
        .save_as_with_dependencies(
            &new_file.to_string_lossy(),
            true,
            &[conflict.to_string_lossy().to_string()],
        )
        .unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    ok(tripwire.check_only_changed(&allowed(
        &plan,
        std::slice::from_ref(&conflict),
        ws.c("other/deep/proj/host.cnnd"),
    )));
    assert_eq!(read(&conflict), read(&ws.p("ws/libs/common.cnnd")));
}

// ---------------------------------------------------------------------------
// Round trips
// ---------------------------------------------------------------------------

#[test]
fn a_copied_design_opens_complete_and_evaluates_identically() {
    let ws = workspace();
    let mut d = open(&ws.host);
    assert_mounts_loaded(&d);
    let expected = values(&mut d);
    for v in &expected {
        assert!(!v.to_lowercase().contains("error"), "{:?}", expected);
    }
    let new_file = ws.p("other/deep/proj/host.cnnd");
    let plan = plan(&d, &new_file);
    let tripwire = DiskTripwire::arm(&ws.root);
    let outcome = d
        .save_as_with_dependencies(&new_file.to_string_lossy(), true, &[])
        .unwrap();
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.copied.len(), 5, "{:#?}", outcome);
    assert_eq!(outcome.external.len(), 1);
    ok(tripwire.check_only_changed(&allowed(&plan, &[], ws.c("other/deep/proj/host.cnnd"))));

    // Paths keep their meaning without being rewritten: every file is a copy.
    assert_eq!(read(&new_file), read(&ws.host));
    for (copy, source) in [
        ("other/deep/libs/demolib.cnnd", "ws/libs/demolib.cnnd"),
        ("other/deep/libs/common.cnnd", "ws/libs/common.cnnd"),
        ("other/deep/libs/tip.xyz", "ws/libs/tip.xyz"),
        (
            "other/deep/proj/libs_local/b.cnnd",
            "ws/proj1/libs_local/b.cnnd",
        ),
        ("other/deep/proj/data/local.xyz", "ws/proj1/data/local.xyz"),
    ] {
        assert_eq!(read(&ws.p(copy)), read(&ws.p(source)), "{}", copy);
    }

    // The open design now points at the copies, which are merely touched.
    for m in d.linked_libraries() {
        assert!(
            path_key(&m.abs_path).contains("other/deep"),
            "{} still at {}",
            m.mount_path,
            m.abs_path.display()
        );
    }
    assert!(d.check_dependencies().is_none());
    assert_eq!(values(&mut d), expected);

    let mut reopened = open(&new_file);
    assert_mounts_loaded(&reopened);
    assert_eq!(values(&mut reopened), expected);
}

#[test]
fn saving_without_dependencies_keeps_every_path_and_loses_nothing() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let new_file = ws.p("other/deep/proj/host.cnnd");
    let tripwire = DiskTripwire::arm(&ws.root);
    let outcome = d
        .save_as_with_dependencies(&new_file.to_string_lossy(), false, &[])
        .unwrap();
    ok(tripwire.check_only_changed(&[ws.c("other/deep/proj/host.cnnd")]));
    assert_eq!(read(&new_file), read(&ws.host), "paths verbatim");
    assert_eq!(outcome.missing.len(), 5, "{:#?}", outcome);
    assert_eq!(outcome.external.len(), 1);

    // The open design sees what is (not) there now.
    let report = d.check_dependencies().expect("status changes");
    let missing: Vec<&str> = report
        .status_changes
        .iter()
        .filter(|(_, s)| *s == MountStatus::Missing)
        .map(|(m, _)| m.as_str())
        .collect();
    assert!(
        missing.contains(&"demolib") && missing.contains(&"b"),
        "{:#?}",
        report
    );

    let mut reopened = open(&new_file);
    for m in reopened.linked_libraries() {
        assert_eq!(m.status, MountStatus::Missing, "{}", m.mount_path);
    }
    ok(check_invariants(&reopened));
    ok(check_save_reopen_identity(&mut reopened));
    assert_eq!(local_fingerprint(&mut reopened), local_fingerprint(&mut d));
}

#[test]
fn after_save_as_a_kept_different_file_is_refreshed() {
    let ws = workspace();
    let mut d = open(&ws.host);
    prepare_deep_target(&ws);
    // A different local.xyz is kept too.
    let kept_xyz = ws.p("other/deep/proj/data/local.xyz");
    std::fs::create_dir_all(kept_xyz.parent().unwrap()).unwrap();
    std::fs::write(&kept_xyz, TIP).unwrap();
    let before = values(&mut d);
    let outcome = d
        .save_as_with_dependencies(
            &ws.p("other/deep/proj/host.cnnd").to_string_lossy(),
            true,
            &[],
        )
        .unwrap();
    assert_eq!(outcome.kept.len(), 2, "{:#?}", outcome);
    let report = d.check_dependencies().expect("the kept files differ");
    assert_eq!(report.refreshed_mounts.len(), 1, "{:#?}", report);
    assert!(report.refreshed_mounts[0].starts_with("demolib"));
    assert_eq!(report.refreshed_data_files.len(), 1, "{:#?}", report);
    assert!(report.dropped_wires.is_empty(), "{:#?}", report);
    let after = values(&mut d);
    assert_ne!(after[1], before[1], "sev now reads the kept common.cnnd");
    assert_ne!(after[3], before[3], "loc now reads the kept local.xyz");
    assert!(after[1].contains('8'), "{:?}", after);
    // One undoable command.
    assert!(d.undo());
    assert_eq!(values(&mut d), before);
}

#[test]
fn undoing_an_earlier_refresh_keeps_the_mounts_at_the_new_folder() {
    let ws = workspace();
    let mut d = open(&ws.host);
    write_design(
        &ws.p("ws/libs/common.cnnd"),
        &[],
        &[("seven", "s = int { value: 9 }\noutput s\n")],
    );
    bump(&ws.p("ws/libs/common.cnnd"));
    let report = d.check_dependencies().expect("common changed");
    assert_eq!(report.refreshed_mounts.len(), 1, "{:#?}", report);
    d.save_as_with_dependencies(
        &ws.p("other/deep/proj/host.cnnd").to_string_lossy(),
        true,
        &[],
    )
    .unwrap();
    assert!(d.undo(), "the refresh is still undoable");
    for m in d.linked_libraries() {
        assert!(
            path_key(&m.abs_path).contains("other/deep"),
            "{} points back at {}",
            m.mount_path,
            m.abs_path.display()
        );
    }
    for key in d.node_type_registry.library_links.data_files.keys() {
        let k = path_key(&key.path);
        assert!(k.contains("other/deep") || k.ends_with(".cif"), "{}", k);
    }
}

// ---------------------------------------------------------------------------
// Failures
// ---------------------------------------------------------------------------

/// A `LinkFs` over the real disk whose writes or renames fail for any path
/// containing a given string.
struct FailingFs {
    fail_write: Option<String>,
    fail_rename: Option<String>,
}

fn contains(p: &Path, what: &Option<String>) -> bool {
    what.as_ref()
        .is_some_and(|w| p.to_string_lossy().contains(w.as_str()))
}

impl LinkFs for FailingFs {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        RealFs.read(p)
    }
    fn stat(&self, p: &Path) -> io::Result<FileMeta> {
        RealFs.stat(p)
    }
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
        RealFs.canonicalize(p)
    }
    fn write(&self, p: &Path, b: &[u8]) -> io::Result<()> {
        if contains(p, &self.fail_write) {
            return Err(io::Error::other("injected write failure"));
        }
        RealFs.write(p, b)
    }
    fn rename(&self, a: &Path, b: &Path) -> io::Result<()> {
        if contains(b, &self.fail_rename) {
            return Err(io::Error::other("injected rename failure"));
        }
        RealFs.rename(a, b)
    }
    fn remove_file(&self, p: &Path) -> io::Result<()> {
        RealFs.remove_file(p)
    }
    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        RealFs.create_dir_all(p)
    }
}

#[test]
fn a_failed_copy_leaves_the_design_unwritten_and_reports_the_copies() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.node_type_registry
        .library_links
        .set_fs(Arc::new(FailingFs {
            fail_write: Some("local.xyz".to_string()),
            fail_rename: None,
        }));
    let new_file = ws.p("other/deep/proj/host.cnnd");
    let tripwire = DiskTripwire::arm(&ws.root);
    let outcome = d
        .save_as_with_dependencies(&new_file.to_string_lossy(), true, &[])
        .unwrap();
    let message = outcome
        .failure_message("The design was not saved.")
        .expect("a failure");
    assert!(message.contains("local.xyz"), "{}", message);
    assert!(!new_file.exists(), "the design is written last");
    assert!(!outcome.copied.is_empty(), "something was copied first");
    for copy in &outcome.copied {
        assert!(copy.exists(), "{}", copy.display());
        assert!(message.contains(&copy.display().to_string()), "{}", message);
    }
    ok(tripwire.check_only_changed(&outcome.copied));
    assert_eq!(d.file_path.as_deref(), Some(&*ws.host.to_string_lossy()));
}

#[test]
fn a_failed_design_write_leaves_the_old_file_at_the_destination() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let target = ws.p("ws/proj1/target.cnnd");
    std::fs::write(&target, "the old file").unwrap();
    d.node_type_registry
        .library_links
        .set_fs(Arc::new(FailingFs {
            fail_write: None,
            fail_rename: Some("target.cnnd".to_string()),
        }));
    let tripwire = DiskTripwire::arm(&ws.root);
    let err = d
        .save_as_with_dependencies(&target.to_string_lossy(), true, &[])
        .unwrap_err();
    assert!(err.contains("target.cnnd"), "{}", err);
    assert_eq!(read(&target), b"the old file");
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn a_wired_file_path_is_flagged() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit(
        "fname = string { value: \"data/local.xyz\" }\nw = import_xyz { file_name: fname }\n",
        false,
    );
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    assert!(plan(&d, &ws.p("ws/proj2/host.cnnd")).has_wired_paths);
}

// ---------------------------------------------------------------------------
// Linking by copying
// ---------------------------------------------------------------------------

/// A library in a second root: `lib/extlib.cnnd` linking `sub/nested.cnnd`
/// and reading `t.xyz`.
fn external_library() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("t.xyz"), TIP).unwrap();
    write_design(
        &lib.join("sub/nested.cnnd"),
        &[],
        &[("four", "f = int { value: 4 }\noutput f\n")],
    );
    write_design(
        &lib.join("extlib.cnnd"),
        &[("sub/nested.cnnd", "nested")],
        &[
            ("tip", "t = import_xyz { file_name: \"t.xyz\" }\noutput t\n"),
            ("four", "f = `nested.four` {}\noutput f\n"),
        ],
    );
    (dir, lib.join("extlib.cnnd"))
}

#[test]
fn linking_by_copying_copies_the_library_and_its_dependencies() {
    let ws = workspace();
    let (ext, extlib) = external_library();
    let mut d = open(&ws.host);
    assert!(!d.library_needs_copy(&extlib.to_string_lossy()));
    let ext_root = std::fs::canonicalize(ext.path()).unwrap();
    let ext_tripwire = DiskTripwire::arm(&ext_root);
    let tripwire = DiskTripwire::arm(&ws.root);
    let copied = d
        .link_library_copying(&extlib.to_string_lossy(), "extlib.cnnd", "ext")
        .unwrap();
    let expected = [
        ("ws/proj1/extlib.cnnd", extlib.clone()),
        (
            "ws/proj1/sub/nested.cnnd",
            ext.path().join("lib/sub/nested.cnnd"),
        ),
        ("ws/proj1/t.xyz", ext.path().join("lib/t.xyz")),
    ];
    assert_eq!(copied.len(), 3, "{:#?}", copied);
    let allowed: Vec<PathBuf> = expected.iter().map(|(p, _)| ws.c(p)).collect();
    ok(tripwire.check_only_changed(&allowed));
    ok(ext_tripwire.check_only_changed(&[]));
    for (copy, source) in &expected {
        assert_eq!(read(&ws.p(copy)), read(source), "paths inside unchanged");
    }
    let ext_mount = d
        .linked_libraries()
        .into_iter()
        .find(|m| m.mount_path == "ext")
        .expect("linked");
    assert_eq!(ext_mount.rel_path, "extlib.cnnd");
    assert_eq!(ext_mount.status, MountStatus::Loaded);
    assert!(
        d.linked_libraries()
            .iter()
            .any(|m| m.mount_path == "ext.nested" && m.status == MountStatus::Loaded)
    );
    // One undo step.
    assert!(d.undo());
    assert!(d.linked_libraries().iter().all(|m| m.mount_path != "ext"));
}

#[test]
fn linking_by_copying_refuses_a_different_file_at_a_target() {
    let ws = workspace();
    let (_ext, extlib) = external_library();
    let mut d = open(&ws.host);
    std::fs::write(ws.p("ws/proj1/t.xyz"), LOCAL).unwrap();
    let tripwire = DiskTripwire::arm(&ws.root);
    let err = d
        .link_library_copying(&extlib.to_string_lossy(), "extlib.cnnd", "ext")
        .unwrap_err();
    assert!(err.contains("t.xyz"), "{}", err);
    assert!(err.contains("nothing was copied"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
    assert!(d.linked_libraries().iter().all(|m| m.mount_path != "ext"));
}

// ---------------------------------------------------------------------------
// The project bundle
// ---------------------------------------------------------------------------

#[test]
fn zip_round_trips() {
    let entries = vec![
        ("a.txt".to_string(), b"hello".to_vec()),
        (
            "dir/b.bin".to_string(),
            (0u8..=255).cycle().take(5000).collect(),
        ),
        ("empty".to_string(), Vec::new()),
    ];
    assert_eq!(read_zip(&zip_bytes(&entries).unwrap()).unwrap(), entries);
}

#[test]
fn the_bundle_reproduces_the_layout_anywhere() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let expected = values(&mut d);
    let zip = ws.p("out/bundle.zip");
    let tripwire = DiskTripwire::arm(&ws.root);
    let outcome = d.export_project_bundle(&zip.to_string_lossy()).unwrap();
    ok(tripwire.check_only_changed(&[ws.c("out/bundle.zip")]));
    assert!(same_path(&outcome.root, &ws.c("ws")));
    let mut files = outcome.files.clone();
    assert_eq!(files[0], "proj1/host.cnnd", "the design first");
    files.sort();
    assert_eq!(
        files,
        [
            "libs/common.cnnd",
            "libs/demolib.cnnd",
            "libs/tip.xyz",
            "proj1/data/local.xyz",
            "proj1/host.cnnd",
            "proj1/libs_local/b.cnnd",
        ]
    );
    assert_eq!(outcome.external.len(), 1);
    assert!(outcome.external[0].ends_with("diamond.cif"));

    let unzip = tempfile::tempdir().unwrap();
    for (name, bytes) in read_zip(&read(&zip)).unwrap() {
        assert!(!name.ends_with(".cif"), "external files stay out");
        let path = unzip.path().join("somewhere").join(&name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut reopened = open(&unzip.path().join("somewhere/proj1/host.cnnd"));
    assert_mounts_loaded(&reopened);
    assert_eq!(values(&mut reopened), expected);
}

#[test]
fn the_bundle_carries_unsaved_edits_and_refuses_to_overwrite_a_dependency() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit("extra = int { value: 42 }\n", false);
    assert!(outcome.result.errors.is_empty());
    let zip = ws.p("out/bundle.zip");
    d.export_project_bundle(&zip.to_string_lossy()).unwrap();
    let host = read_zip(&read(&zip))
        .unwrap()
        .into_iter()
        .find(|(n, _)| n == "proj1/host.cnnd")
        .unwrap()
        .1;
    assert!(String::from_utf8(host).unwrap().contains("\"extra\""));

    let tripwire = DiskTripwire::arm(&ws.root);
    let err = d
        .export_project_bundle(&ws.p("ws/libs/tip.xyz").to_string_lossy())
        .unwrap_err();
    assert!(err.contains("would overwrite"), "{}", err);
    ok(tripwire.check_only_changed(&[]));
}

static MTIME_TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Moves `path`'s mtime forward, so a stat-based check sees the change.
fn bump(path: &Path) {
    let tick = MTIME_TICK.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let t = std::time::SystemTime::now() + std::time::Duration::from_secs(3600 + tick * 7);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}
