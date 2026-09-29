//! Library linking, Phase 6 (`doc/design_library_linking.md` §10, D3, §11):
//! *Make local copy* (vendoring), *Rename alias…*, and the `# linked from`
//! header of `query`.
//!
//! Vendoring and alias rename are the two operations that turn linked content
//! into saved local content or rewrite the host's references, so every test
//! here is judged by the shared oracles of `library_links_support.rs`: O2 (no
//! wire lost or moved), O3 (no library file written), O4 (save → reopen
//! identity), O5 (undo is the inverse, redo the post-state) and O6.

use super::library_links_refresh_test::{bump_mtime, edit_lib, workspace};
use super::library_links_support::*;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::library_links::{MountStatus, mount_fingerprint};
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use std::collections::BTreeSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The display string of `name`'s pin-0 value in `network`.
fn value(d: &mut StructureDesigner, network: &str, name: &str) -> String {
    d.set_active_node_network_name(Some(network.to_string()));
    let id = node_id(d, network, name);
    d.evaluate_node_output(&[], id, 0).to_display_string()
}

/// The O2 ledger of the networks in `locals` only — an operation that turns
/// linked networks into local ones adds *their* wires to the ledger, which
/// are not host wires that could have been lost.
fn ledger_of(d: &StructureDesigner, locals: &BTreeSet<String>) -> BTreeSet<WireEntry> {
    wire_ledger(d)
        .into_iter()
        .filter(|w| locals.contains(&w.network))
        .collect()
}

fn local_networks(d: &StructureDesigner) -> BTreeSet<String> {
    let registry = &d.node_type_registry;
    registry
        .node_networks
        .keys()
        .filter(|n| registry.library_links.mount_containing(n).is_none())
        .cloned()
        .collect()
}

/// `mount_fingerprint` without its list of mount records: the content of
/// every name under `prefix`, whether linked or local.
fn content_fingerprint(d: &StructureDesigner, prefix: &str) -> String {
    let text = mount_fingerprint(&d.node_type_registry, prefix);
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value.as_object_mut().unwrap().remove("mounts");
    serde_json::to_string_pretty(&value).unwrap()
}

/// Every refused operation leaves the design and its history as they were.
#[track_caller]
fn assert_refused(
    d: &mut StructureDesigner,
    op: impl FnOnce(&mut StructureDesigner) -> Result<(), String>,
    expect: &str,
) {
    let before = (local_fingerprint(d), mount_fingerprints(d));
    let pushes = d.undo_stack.push_count();
    let err = op(d).expect_err("the operation must be refused");
    assert!(
        err.contains(expect),
        "error '{}' does not mention '{}'",
        err,
        expect
    );
    assert_eq!(d.undo_stack.push_count(), pushes, "a refusal pushed a step");
    assert_eq!(
        (local_fingerprint(d), mount_fingerprints(d)),
        before,
        "a refusal changed the design"
    );
}

fn imports_of(text: &str) -> Vec<serde_json::Value> {
    let json: serde_json::Value = serde_json::from_str(text).unwrap();
    json.get("imports")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Make local copy (§10)
// ---------------------------------------------------------------------------

#[test]
fn making_a_library_local_keeps_every_name_and_wire_and_needs_no_file_afterwards() {
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let mut d = open(&host);
    let f_before = value(&mut d, "Main", "f");
    let s_before = value(&mut d, "Main", "s");

    let content_before = content_fingerprint(&d, "a");
    assert!(
        content_before.contains("a.common.slab"),
        "nested mount content"
    );
    let locals = local_networks(&d);
    let wires_before = wire_ledger(&d);
    assert!(!wires_before.is_empty());
    let tripwire = DiskTripwire::arm(ws.path());

    ok(check_undo_inverse(&mut d, |d| {
        d.make_library_local("a").expect("make local")
    }));

    // Nothing lost in the conversion: the same names hold the same content,
    // nested mount included; the mount records are gone.
    assert_eq!(content_fingerprint(&d, "a"), content_before);
    assert!(
        d.node_type_registry
            .library_links
            .iter()
            .all(|m| m.mount_path == "b"),
        "only `b` is still linked"
    );
    assert!(d.ensure_editable("a.foo").is_ok());
    assert!(d.ensure_editable("a.common.slab").is_ok());
    ok(check_wire_ledger(
        &wires_before,
        &ledger_of(&d, &locals),
        &BTreeSet::new(),
    ));
    ok(check_invariants(&d));
    assert!(d.is_dirty);

    // Saved as local content, with no `imports` entry for it.
    let dir = host.parent().map(Path::to_path_buf);
    let text = saved_text(&mut d, dir.as_deref());
    let aliases: Vec<String> = imports_of(&text)
        .iter()
        .map(|i| i["alias"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(aliases, vec!["b".to_string()]);
    assert!(
        text.contains("\"a.foo\""),
        "the vendored networks are saved"
    );
    ok(check_save_reopen_identity(&mut d));
    ok(tripwire.check_only_changed(&[]));

    // Save, delete the library files, reopen: still evaluates identically.
    save(&mut d);
    std::fs::remove_file(ws.path().join("lib_a.cnnd")).unwrap();
    std::fs::remove_file(ws.path().join("libs/common.cnnd")).unwrap();
    let mut reopened = open(&host);
    assert!(
        reopened
            .node_type_registry
            .library_links
            .iter()
            .all(|m| m.mount_path == "b")
    );
    assert_eq!(value(&mut reopened, "Main", "f"), f_before);
    assert_eq!(value(&mut reopened, "Main", "s"), s_before);
    ok(check_invariants(&reopened));
}

#[test]
fn making_a_library_local_rebases_its_relative_data_paths_onto_the_design_folder() {
    let ws = fixture_workspace();
    let host = ws.path().join("xyz_host.cnnd");
    let mut d = new_design(&host);
    d.link_library("libs/with_xyz.cnnd", "x").unwrap();
    edit(&mut d, "Main", "t = `x.tip` {}\noutput t\n");
    save(&mut d);
    let before = value(&mut d, "Main", "t");
    assert!(before.contains('2') || !before.is_empty());
    let file_name = |d: &StructureDesigner| {
        let net = &d.node_type_registry.node_networks["x.tip"];
        let node = net
            .nodes
            .values()
            .find(|n| n.node_type_name == "import_xyz")
            .unwrap();
        node.data.file_paths()
    };
    assert_eq!(file_name(&d), vec!["tip.xyz".to_string()]);

    ok(check_undo_inverse(&mut d, |d| {
        d.make_library_local("x").expect("make local")
    }));
    // The path now names the same file from the design's folder…
    assert_eq!(file_name(&d), vec!["libs/tip.xyz".to_string()]);
    assert_eq!(value(&mut d, "Main", "t"), before);
    // …and the watch follows the file, not the old spelling.
    let watched: Vec<Option<String>> = d
        .node_type_registry
        .library_links
        .data_files
        .keys()
        .map(|k| k.owner.clone())
        .collect();
    assert_eq!(watched, vec![None]);

    // Undo gives the library its own spelling back.
    assert!(d.undo());
    assert_eq!(file_name(&d), vec!["tip.xyz".to_string()]);
    assert!(d.redo());

    save(&mut d);
    std::fs::remove_file(ws.path().join("libs/with_xyz.cnnd")).unwrap();
    let mut reopened = open(&host);
    assert_eq!(value(&mut reopened, "Main", "t"), before);
}

#[test]
fn making_a_library_local_is_refused_without_content_or_with_unresolved_references() {
    // A missing library: nothing to copy.
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host_missing.cnnd"));
    assert_refused(&mut d, |d| d.make_library_local("demolib"), "not loaded");

    // A nested mount: only a direct link can be made local.
    let mut d = open(&ws.path().join("host.cnnd"));
    assert_refused(
        &mut d,
        |d| d.make_library_local("a.common"),
        "only a direct link",
    );
    assert_refused(
        &mut d,
        |d| d.make_library_local("nope"),
        "no linked library",
    );

    // A loaded library that no longer defines a name the host uses: the
    // frozen nodes are safe only while the name lies under a mount.
    let ws = workspace();
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        l.delete_node_network("bar").unwrap();
        l.delete_node_network("foo").unwrap();
    });
    d.refresh_library("lib").unwrap();
    assert_refused(&mut d, |d| d.make_library_local("lib"), "does not define");
}

#[test]
fn a_vendored_library_is_editable_and_no_longer_linked() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.make_library_local("lib").unwrap();
    assert!(d.unlink_library("lib").is_err());
    assert!(d.refresh_library("lib").is_err());
    // An edit of formerly linked content now lands.
    incr_edit(&mut d, "lib.foo", "z = int { value: 9 }\n");
    assert!(
        d.node_type_registry.node_networks["lib.foo"]
            .nodes
            .values()
            .any(|n| n.custom_name.as_deref() == Some("z"))
    );
    // The library file changing no longer reaches the design.
    let tripwire = DiskTripwire::arm(ws.dir.path());
    let fp = local_fingerprint(&mut d);
    edit_lib(&ws.lib, |l| {
        l.delete_node_network("bar").unwrap();
    });
    assert!(d.check_dependencies().is_none());
    assert_eq!(local_fingerprint(&mut d), fp);
    ok(tripwire.check_only_changed(&[ws.lib.clone()]));
}

fn incr_edit(d: &mut StructureDesigner, network: &str, code: &str) {
    d.set_active_node_network_name(Some(network.to_string()));
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
}

// ---------------------------------------------------------------------------
// Rename alias (D3)
// ---------------------------------------------------------------------------

#[test]
fn renaming_an_alias_rewrites_every_reference_kind() {
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let mut d = open(&host);
    let f_before = value(&mut d, "Main", "f");
    let wires_before = wire_ledger(&d);
    assert!(!wires_before.is_empty());
    let content_before = content_fingerprint(&d, "a");
    d.set_active_node_network_name(Some("a.foo".to_string()));
    let tripwire = DiskTripwire::arm(ws.path());

    ok(check_undo_inverse(&mut d, |d| {
        d.rename_library_alias("a", "libs.alpha").expect("rename")
    }));

    let registry = &d.node_type_registry;
    let leftovers: Vec<&String> = registry
        .node_networks
        .keys()
        .chain(registry.record_type_defs.keys())
        .filter(|n| n.as_str() == "a" || n.starts_with("a."))
        .collect();
    assert!(leftovers.is_empty(), "left under `a`: {:?}", leftovers);
    let mounts: Vec<(String, Option<String>)> = registry
        .library_links
        .iter()
        .map(|m| (m.mount_path.clone(), m.parent.clone()))
        .collect();
    assert_eq!(
        mounts,
        vec![
            ("b".to_string(), None),
            ("libs.alpha".to_string(), None),
            (
                "libs.alpha.common".to_string(),
                Some("libs.alpha".to_string())
            ),
        ]
    );
    // The same content, spelled under the new alias.
    assert_eq!(
        content_fingerprint(&d, "libs.alpha"),
        content_before
            .replace("\"a.", "\"libs.alpha.")
            .replace("`a.", "`libs.alpha.")
    );
    // Instances, record schemas, a transitive reference, a body instance, and
    // a type-only reference (a parameter typed with the linked record).
    let main = &registry.node_networks["Main"];
    let names: BTreeSet<&str> = main
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(names.contains("libs.alpha.foo"));
    assert!(names.contains("libs.alpha.common.slab"));
    let typed = &registry.node_networks["typed"];
    assert_eq!(
        typed.node_type.parameters[0].data_type.to_string(),
        "Record(`libs.alpha.Miller`)"
    );
    assert_eq!(
        d.active_node_network_name.as_deref(),
        Some("libs.alpha.foo")
    );

    ok(check_wire_ledger(
        &wires_before,
        &wire_ledger(&d),
        &BTreeSet::new(),
    ));
    ok(check_invariants(&d));
    assert_eq!(value(&mut d, "Main", "f"), f_before);

    // The saved file writes the new alias, with the same `uses` keys.
    let dir = host.parent().map(Path::to_path_buf);
    let text = saved_text(&mut d, dir.as_deref());
    let imports = imports_of(&text);
    let alpha = imports
        .iter()
        .find(|i| i["alias"] == "libs.alpha")
        .expect("renamed import");
    assert_eq!(alpha["path"], "lib_a.cnnd");
    assert!(alpha["uses"]["networks"].get("foo").is_some());
    assert!(alpha["uses"]["networks"].get("common.slab").is_some());
    assert!(alpha["uses"]["records"].get("Miller").is_some());
    assert!(
        !text.contains("\"a."),
        "no reference under the old alias left"
    );
    ok(check_save_reopen_identity(&mut d));
    // No library file is written.
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn renaming_the_alias_of_a_missing_library_keeps_frozen_nodes_and_their_layouts() {
    let ws = fixture_workspace();
    let host = ws.path().join("host_missing.cnnd");
    let mut d = open(&host);
    let positions_before = positional_ledger(&d);
    assert!(!positions_before.is_empty());
    let pin_names = |d: &StructureDesigner, alias: &str| -> Vec<String> {
        let main = &d.node_type_registry.node_networks["Main"];
        let f = main
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some("f"))
            .unwrap();
        assert_eq!(f.node_type_name, format!("{}.foo", alias));
        f.custom_node_type
            .as_ref()
            .map(|t| t.parameters.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default()
    };
    assert_eq!(pin_names(&d, "demolib"), vec!["x", "y"]);
    let tripwire = DiskTripwire::arm(ws.path());

    ok(check_undo_inverse(&mut d, |d| {
        d.rename_library_alias("demolib", "dl").expect("rename")
    }));

    // Frozen nodes keep their arguments exactly and their recorded layouts.
    assert_eq!(positional_ledger(&d), positions_before);
    assert_eq!(pin_names(&d, "dl"), vec!["x", "y"]);
    let mount = d.node_type_registry.library_links.get("dl").unwrap();
    assert_eq!(mount.status, MountStatus::Missing);
    // Recorded types move with the alias.
    let split = &mount.stored_uses.networks["split"];
    assert_eq!(split.params[0].data_type, "Record(`dl.Miller`)");
    ok(check_invariants(&d));

    let dir = host.parent().map(Path::to_path_buf);
    let text = saved_text(&mut d, dir.as_deref());
    assert!(
        !text.replace("missing/demolib.cnnd", "").contains("demolib"),
        "a reference to the old alias survived"
    );
    ok(check_save_reopen_identity(&mut d));
    ok(tripwire.check_only_changed(&[]));
}

#[test]
fn renaming_an_alias_is_refused_when_the_new_one_is_taken_or_overlaps() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    assert_refused(&mut d, |d| d.rename_library_alias("a", "Main"), "taken");
    assert_refused(&mut d, |d| d.rename_library_alias("a", "b"), "overlaps");
    assert_refused(&mut d, |d| d.rename_library_alias("a", "b.x"), "overlaps");
    assert_refused(&mut d, |d| d.rename_library_alias("a", "a.v2"), "two steps");
    assert_refused(
        &mut d,
        |d| d.rename_library_alias("a", "1x"),
        "invalid segment",
    );
    assert_refused(
        &mut d,
        |d| d.rename_library_alias("a.common", "c"),
        "only a direct link",
    );
    assert_refused(
        &mut d,
        |d| d.rename_library_alias("zz", "c"),
        "no linked library",
    );
    // A dangling reference under the new alias would start resolving into
    // the library.
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Holder",
        vec![(
            "r".to_string(),
            DataType::from_string("Record(`zz.R`)").unwrap(),
        )],
    ))
    .unwrap();
    assert_refused(
        &mut d,
        |d| d.rename_library_alias("a", "zz"),
        "already referred to",
    );
    // Unchanged is a no-op, not a step.
    let pushes = d.undo_stack.push_count();
    d.rename_library_alias("a", "a").unwrap();
    assert_eq!(d.undo_stack.push_count(), pushes);
}

#[test]
fn a_refresh_after_an_alias_rename_and_undo_across_both_stay_consistent() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let before = (local_fingerprint(&mut d), mount_fingerprints(&d));
    d.rename_library_alias("lib", "newlib").unwrap();
    // The library changes; the refresh finds it under its new alias.
    edit_lib(&ws.lib, |l| {
        incr_edit(
            l,
            "foo",
            "z = parameter { param_name: \"z\", data_type: Int, sort_order: 5 }\n",
        );
        l.validate_active_network();
    });
    bump_mtime(&ws.lib);
    let report = d.check_dependencies().expect("a change");
    assert_eq!(
        report.refreshed_mounts,
        vec!["newlib (lib.cnnd)".to_string()]
    );
    let net = &d.node_type_registry.node_networks["newlib.foo"];
    assert_eq!(
        atomcad_structure_designer::network_validator::live_parameters(net).len(),
        3
    );
    // Undo both, in order: back to the start.
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!((local_fingerprint(&mut d), mount_fingerprints(&d)), before);
    ok(check_invariants(&d));
}

// ---------------------------------------------------------------------------
// `query` shows where a linked network comes from
// ---------------------------------------------------------------------------

#[test]
fn query_marks_a_linked_network_with_the_file_it_comes_from() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    let second_line = |d: &mut StructureDesigner, network: &str| -> String {
        d.set_active_node_network_name(Some(network.to_string()));
        let text = d.query_active_network_text();
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some(format!("# Network: {}", network).as_str())
        );
        lines.next().unwrap_or_default().to_string()
    };
    let direct = second_line(&mut d, "a.foo");
    assert!(
        direct.starts_with("# linked from lib_a.cnnd (library `a`)"),
        "{}",
        direct
    );
    let nested = second_line(&mut d, "a.common.slab");
    assert!(
        nested.starts_with("# linked from libs/common.cnnd (library `a.common`, through `a`)"),
        "{}",
        nested
    );
    let local = second_line(&mut d, "Main");
    assert!(!local.contains("linked from"), "{}", local);
    // After *Make local copy* the network is the design's own: no header.
    d.make_library_local("a").unwrap();
    let vendored = second_line(&mut d, "a.foo");
    assert!(!vendored.contains("linked from"), "{}", vendored);
}
