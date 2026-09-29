//! Library linking, Phase 3 (`doc/design_library_linking.md` §11): persistent
//! ids, change detection, refresh, retarget, and reconciling the host's
//! wiring against the interfaces it recorded — on refresh and on open alike.
//!
//! Every test works in a temp dir, edits libraries the way a user does (open
//! the library in its own designer, edit, save), and judges the outcome with
//! the shared oracles of `library_links_support.rs`: O2 (every missing wire
//! is in the report), O3 (no file changed that the test did not change), O5
//! (undo is the inverse, redo restores the post-state).

use super::library_links_support::*;
use atomcad_structure_designer::library_links::{
    FileMeta, LinkFs, MountStatus, RealFs, is_frozen, mount_fingerprint,
};
use atomcad_structure_designer::library_refresh::{RefreshReport, WireSlot};
use atomcad_structure_designer::node_type_registry::FieldId;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// The workspace: `lib.cnnd` and a host linking it as `lib`
// ---------------------------------------------------------------------------

pub const FOO: &str = "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
y = parameter { param_name: \"y\", data_type: Int, sort_order: 1 }
s = expr { a: x, b: y, expression: \"a * 10 + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
";

pub const SPLIT: &str =
    "m = parameter { param_name: \"m\", data_type: Record(Miller), sort_order: 0 }
d = record_destructure { schema: \"Miller\", record: m }
output d
";

pub const BAR: &str = "one = int { value: 1 }
two = int { value: 2 }
c = foo { x: one, y: two }
output c
";

/// The library: a record def, a two-parameter network whose result depends
/// on which argument is which (`x * 10 + y`), a multi-output network, and a
/// network calling another one of the library.
pub fn write_lib(path: &Path) {
    let mut d = new_design(path);
    d.add_record_type_def(int_record("Miller", &["h", "k", "l"]))
        .unwrap();
    edit(&mut d, "foo", FOO);
    edit(&mut d, "split", SPLIT);
    edit(&mut d, "bar", BAR);
    d.delete_node_network("Main").ok();
    save(&mut d);
}

/// The host: an instance, record nodes, a consumer of a destructured field,
/// a multi-output instance with a consumer of its pin 2, a linked instance
/// inside a HOF body, and an `apply` fed by an instance's function pin.
const HOST_MAIN: &str = "i1 = int { value: 1 }
i2 = int { value: 2 }
f = `lib.foo` { x: i1, y: i2 }
rc = record_construct { schema: \"lib.Miller\", h: i1, k: i2, l: i1 }
rd = record_destructure { schema: \"lib.Miller\", record: rc }
e2 = expr { a: rd.k, expression: \"a + 100\", parameters: [{ name: \"a\", data_type: Int }] }
sp = `lib.split` { m: rc }
e3 = expr { a: sp.l, expression: \"a + 1000\", parameters: [{ name: \"a\", data_type: Int }] }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = `lib.foo` { x: $element, y: ^i2 } output g } }
f2 = `lib.foo` {}
ap = apply { f: @f2, arg0: i1, arg1: i2 }
co = collect { iter: mp }
output f
";

pub struct Ws {
    pub dir: tempfile::TempDir,
    pub host: PathBuf,
    pub lib: PathBuf,
}

pub fn workspace() -> Ws {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.cnnd");
    let host = dir.path().join("host.cnnd");
    write_lib(&lib);
    let mut d = new_design(&host);
    d.link_library("lib.cnnd", "lib").unwrap();
    edit(&mut d, "Main", HOST_MAIN);
    d.set_active_node_network_name(Some("Main".to_string()));
    save(&mut d);
    Ws { dir, host, lib }
}

/// An incremental edit (`ai_text_edit` merge mode): nodes keep their ids,
/// parameters keep their `param_id`s.
pub fn incr(d: &mut StructureDesigner, network: &str, code: &str) {
    d.set_active_node_network_name(Some(network.to_string()));
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty(),
        "edit of '{}' failed: {:?}\n{}",
        network,
        outcome.result.errors,
        code
    );
    d.validate_active_network();
}

/// Edits a library the way a user does: opens it on its own, edits, saves.
/// The file's mtime is moved forward explicitly, so a stat-based check sees
/// the change whatever the filesystem's timestamp granularity (§11.1 rules).
pub fn edit_lib(path: &Path, f: impl FnOnce(&mut StructureDesigner)) {
    let mut d = open(path);
    f(&mut d);
    save(&mut d);
    bump_mtime(path);
}

static MTIME_TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Moves `path`'s mtime to a time no other bump in this run uses.
pub fn bump_mtime(path: &Path) {
    let tick = MTIME_TICK.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let t = std::time::SystemTime::now() + std::time::Duration::from_secs(3600 + tick * 7);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

/// The display string of `name`'s pin-0 value in `network`.
fn value(d: &mut StructureDesigner, network: &str, name: &str) -> String {
    d.set_active_node_network_name(Some(network.to_string()));
    let id = node_id(d, network, name);
    d.evaluate_node_output(&[], id, 0).to_display_string()
}

fn status(d: &StructureDesigner, mount_path: &str) -> MountStatus {
    d.node_type_registry
        .library_links
        .get(mount_path)
        .unwrap()
        .status
        .clone()
}

/// The wires of node `name` in `network`, per argument, as source names.
fn wires_of(d: &StructureDesigner, network: &str, name: &str) -> Vec<Vec<String>> {
    let net = &d.node_type_registry.node_networks[network];
    let id = node_id(d, network, name);
    let name_of = |src: u64| {
        net.nodes
            .get(&src)
            .and_then(|n| n.custom_name.clone())
            .unwrap_or_else(|| src.to_string())
    };
    net.nodes[&id]
        .arguments
        .iter()
        .map(|a| {
            a.incoming_wires
                .iter()
                .map(|w| name_of(w.source_node_id))
                .collect()
        })
        .collect()
}

/// O2 for a refresh or an open: every wire of `before` (taken before the
/// library changed) is still there or in the report. A wire the report *warns*
/// about (§7.3: kept on an output pin ≥ 1 of a network whose outputs changed,
/// which matches by position) is visible, not silent, and may now read
/// another output — it is judged by the warning, not by the ledger.
#[track_caller]
fn check_ledger(before: &BTreeSet<WireEntry>, d: &StructureDesigner, report: &RefreshReport) {
    assert!(!before.is_empty(), "the host must have wires to lose");
    let warned: BTreeSet<(String, Vec<u64>, u64, u64, u8)> = report
        .output_pin_warnings
        .iter()
        .map(|w| {
            (
                w.network.clone(),
                w.scope_path.clone(),
                w.node_id,
                w.source_node_id,
                w.source_scope_depth,
            )
        })
        .collect();
    let unwarned = |set: &BTreeSet<WireEntry>| -> BTreeSet<WireEntry> {
        set.iter()
            .filter(|e| {
                !warned.contains(&(
                    e.network.clone(),
                    e.scope.clone(),
                    e.dest,
                    e.source,
                    e.depth,
                ))
            })
            .cloned()
            .collect()
    };
    ok(check_wire_ledger(
        &unwarned(before),
        &unwarned(&wire_ledger(d)),
        &reported_drops(report),
    ));
}

/// A refresh of `lib` judged by O2, O3 and O5; returns its report.
#[track_caller]
fn refresh_checked(
    d: &mut StructureDesigner,
    ws: &Ws,
    before: &BTreeSet<WireEntry>,
) -> RefreshReport {
    let tripwire = DiskTripwire::arm(ws.dir.path());
    let mut report = None;
    ok(check_undo_inverse(d, |d| {
        report = Some(d.refresh_library("lib").expect("refresh"));
    }));
    let report = report.unwrap();
    check_ledger(before, d, &report);
    ok(tripwire.check_only_changed(&[]));
    ok(check_invariants(d));
    report
}

/// A refresh that freezes nodes, judged by O3, O5 and position: a frozen node
/// keeps its arguments exactly (§8) — and its wires are keyed by position
/// from then on — so across such a refresh nothing may move at all.
#[track_caller]
fn refresh_freezing_checked(d: &mut StructureDesigner, ws: &Ws) -> RefreshReport {
    let before = positional_ledger(d);
    assert!(!before.is_empty());
    let tripwire = DiskTripwire::arm(ws.dir.path());
    let mut report = None;
    ok(check_undo_inverse(d, |d| {
        report = Some(d.refresh_library("lib").expect("refresh"));
    }));
    assert_eq!(
        positional_ledger(d),
        before,
        "a freezing refresh moved a wire"
    );
    ok(tripwire.check_only_changed(&[]));
    ok(check_invariants(d));
    report.unwrap()
}

// ---------------------------------------------------------------------------
// Persistent ids (§13 item 7) — first, since reconciliation matches on them
// ---------------------------------------------------------------------------

fn field_ids(d: &StructureDesigner, def: &str) -> Vec<(String, u64)> {
    d.node_type_registry
        .lookup_record_type_def(def)
        .unwrap()
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.id.0))
        .collect()
}

#[test]
fn record_field_ids_survive_a_save_after_a_field_is_inserted_at_the_front() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.cnnd");
    write_lib(&path);
    edit_lib(&path, |d| {
        d.update_record_type_def(
            "Miller",
            ["z", "h", "k", "l"]
                .iter()
                .map(|n| {
                    (
                        n.to_string(),
                        atomcad_structure_designer::data_type::DataType::Int,
                    )
                })
                .collect(),
        )
        .unwrap();
    });
    let d = open(&path);
    assert_eq!(
        field_ids(&d, "Miller"),
        vec![
            ("z".to_string(), 3),
            ("h".to_string(), 0),
            ("k".to_string(), 1),
            ("l".to_string(), 2)
        ]
    );
    let def = d
        .node_type_registry
        .lookup_record_type_def("Miller")
        .unwrap();
    assert_eq!(def.next_field_id, 4);
}

#[test]
fn a_record_def_whose_ids_are_derivable_saves_in_the_older_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.cnnd");
    write_lib(&path);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("field_ids"), "{}", text);
    assert!(!text.contains("next_field_id"));
    // A design whose only network was never deleted writes no counter at all.
    let single = dir.path().join("single.cnnd");
    let mut d = new_design(&single);
    edit(&mut d, "Main", FOO);
    save(&mut d);
    assert!(
        !std::fs::read_to_string(&single)
            .unwrap()
            .contains("next_param_id")
    );
}

// ---------------------------------------------------------------------------
// Re-created networks and defs never reuse ids (the file-level floor)
// ---------------------------------------------------------------------------

/// Parameter ids of network `name`, by parameter name.
fn param_ids(d: &StructureDesigner, name: &str) -> Vec<(String, u64)> {
    atomcad_structure_designer::network_validator::live_parameters(
        &d.node_type_registry.node_networks[name],
    )
    .into_iter()
    .map(|p| (p.name, p.id.unwrap()))
    .collect()
}

/// `foo` re-created with parameters of other names than the original's.
const FOO_OTHER_NAMES: &str = "u = parameter { param_name: \"u\", data_type: Int, sort_order: 0 }
v = parameter { param_name: \"v\", data_type: Int, sort_order: 1 }
s = expr { a: u, b: v, expression: \"a * 10 + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
";

#[test]
fn a_network_re_created_under_its_old_name_never_reuses_an_id() {
    // Deleted and re-created in one session, and across a save.
    for save_between in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib.cnnd");
        write_lib(&path);
        let old: BTreeSet<u64> = param_ids(&open(&path), "foo")
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let mut d = open(&path);
        d.delete_node_network("bar").unwrap();
        d.delete_node_network("foo").unwrap();
        if save_between {
            save(&mut d);
            d = open(&path);
        }
        edit(&mut d, "foo", FOO_OTHER_NAMES);
        save(&mut d);
        let new = param_ids(&open(&path), "foo");
        assert!(
            new.iter().all(|(_, id)| !old.contains(id)),
            "save between: {}: {:?} reuses {:?}",
            save_between,
            new,
            old
        );
    }
}

#[test]
fn a_host_never_moves_a_wire_onto_a_re_created_network_s_other_parameters() {
    for on_open in [false, true] {
        let ws = workspace();
        let mut d = open(&ws.host);
        let before = wire_ledger(&d);
        edit_lib(&ws.lib, |l| {
            l.delete_node_network("bar").unwrap();
            l.delete_node_network("foo").unwrap();
        });
        edit_lib(&ws.lib, |l| edit(l, "foo", FOO_OTHER_NAMES));
        let report = if on_open {
            d = open(&ws.host);
            d.take_load_library_report().unwrap()
        } else {
            d.refresh_library("lib").unwrap()
        };
        // Nothing matches `x` / `y` by id or by name: both wires are dropped
        // and reported — never moved onto `u` / `v`.
        check_ledger(&before, &d, &report);
        assert_eq!(
            wires_of(&d, "Main", "f"),
            vec![Vec::<String>::new(), vec![]]
        );
        let f = node_id(&d, "Main", "f");
        let mut dropped: Vec<String> = report
            .dropped_wires
            .iter()
            .filter(|w| w.node_id == f && w.scope_path.is_empty())
            .map(|w| w.pin_name.clone())
            .collect();
        dropped.sort();
        assert_eq!(dropped, vec!["x", "y"], "open: {}", on_open);
    }
}

#[test]
fn a_record_def_re_created_under_its_old_name_never_reuses_a_field_id() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let before = wire_ledger(&d);
    let old: BTreeSet<u64> = field_ids(&open(&ws.lib), "Miller")
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    edit_lib(&ws.lib, |l| {
        l.delete_node_network("split").unwrap();
        l.delete_record_type_def("Miller").unwrap();
    });
    edit_lib(&ws.lib, |l| {
        l.add_record_type_def(int_record("Miller", &["a", "b", "c"]))
            .unwrap();
    });
    let new = field_ids(&open(&ws.lib), "Miller");
    assert!(new.iter().all(|(_, id)| !old.contains(id)), "{:?}", new);
    let sp_wires = wires_of(&d, "Main", "sp");
    let report = d.refresh_library("lib").unwrap();
    // `sp` froze (`split` is gone): it keeps its wires positionally (§8) and
    // is keyed by position from now on, so it is judged apart.
    let sp = node_id(&d, "Main", "sp");
    assert!(is_frozen(
        &d.node_type_registry.node_networks["Main"].nodes[&sp],
        &d.node_type_registry
    ));
    assert_eq!(wires_of(&d, "Main", "sp"), sp_wires);
    let not_sp = |set: BTreeSet<WireEntry>| -> BTreeSet<WireEntry> {
        set.into_iter().filter(|e| e.dest != sp).collect()
    };
    ok(check_wire_ledger(
        &not_sp(before),
        &not_sp(wire_ledger(&d)),
        &reported_drops(&report),
    ));
    // `rc`'s three wires had fields h / k / l: dropped, not moved onto a / b / c.
    assert_eq!(
        wires_of(&d, "Main", "rc"),
        vec![Vec::<String>::new(), vec![], vec![]]
    );
}

#[test]
fn a_deleted_field_id_is_not_handed_out_again_after_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.cnnd");
    write_lib(&path);
    edit_lib(&path, |d| {
        d.update_record_type_def(
            "Miller",
            vec![
                (
                    "h".to_string(),
                    atomcad_structure_designer::data_type::DataType::Int,
                ),
                (
                    "k".to_string(),
                    atomcad_structure_designer::data_type::DataType::Int,
                ),
            ],
        )
        .unwrap();
    });
    edit_lib(&path, |d| {
        d.update_record_type_def(
            "Miller",
            ["h", "k", "w"]
                .iter()
                .map(|n| {
                    (
                        n.to_string(),
                        atomcad_structure_designer::data_type::DataType::Int,
                    )
                })
                .collect(),
        )
        .unwrap();
    });
    let d = open(&path);
    assert_eq!(
        field_ids(&d, "Miller"),
        vec![
            ("h".to_string(), 0),
            ("k".to_string(), 1),
            ("w".to_string(), 3)
        ]
    );
}

#[test]
fn a_deleted_param_id_is_not_handed_out_again_after_a_save() {
    use atomcad_structure_designer::network_validator::live_parameters;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.cnnd");
    write_lib(&path);
    let ids = |d: &StructureDesigner| -> Vec<(String, Option<u64>)> {
        live_parameters(&d.node_type_registry.node_networks["foo"])
            .into_iter()
            .map(|p| (p.name, p.id))
            .collect()
    };
    let before = ids(&open(&path));
    let y_id = before[1].1.unwrap();
    edit_lib(&path, |d| incr(d, "foo", "delete y\ns = expr { b: 0 }\n"));
    edit_lib(&path, |d| {
        incr(
            d,
            "foo",
            "w = parameter { param_name: \"w\", data_type: Int, sort_order: 1 }\n",
        )
    });
    let after = ids(&open(&path));
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1].0, "w");
    assert!(after[1].1.unwrap() > y_id, "{:?} recycles {}", after, y_id);
}

#[test]
fn stored_field_ids_are_healed_and_a_file_without_ids_gets_authored_order() {
    use atomcad_structure_designer::node_type_registry::RecordTypeDef;
    let with_dups: RecordTypeDef = serde_json::from_str(
        r#"{"name":"R","fields":[["a","Int"],["b","Int"],["c","Int"]],"field_ids":[4,4,1],"next_field_id":2}"#,
    )
    .unwrap();
    let ids: Vec<u64> = with_dups.fields.iter().map(|f| f.id.0).collect();
    assert_eq!(ids, vec![4, 5, 1]);
    assert_eq!(with_dups.next_field_id, 6);

    let legacy: RecordTypeDef =
        serde_json::from_str(r#"{"name":"R","fields":[["a","Int"],["b","Int"]]}"#).unwrap();
    assert_eq!(legacy.fields[1].id, FieldId(1));
    assert_eq!(legacy.next_field_id, 2);
    // Round trip: derivable ids write the older format, others persist.
    assert!(
        !serde_json::to_string(&legacy)
            .unwrap()
            .contains("field_ids")
    );
    let again: RecordTypeDef =
        serde_json::from_str(&serde_json::to_string(&with_dups).unwrap()).unwrap();
    assert_eq!(again, with_dups);
}

// ---------------------------------------------------------------------------
// Parameter edits: live refresh and open reconcile alike
// ---------------------------------------------------------------------------

/// One library edit of `foo`'s parameters.
struct ParamEdit {
    name: &'static str,
    code: &'static str,
    /// `f`'s wires afterwards, per argument.
    f_wires: &'static [&'static [&'static str]],
    /// Names of `f`'s parameters whose wire the report must list as dropped.
    dropped: &'static [&'static str],
    /// Names of `f`'s parameters the report must list as retyped.
    flagged: &'static [&'static str],
    /// `f`'s value afterwards, when it is meaningful.
    f_value: Option<&'static str>,
    /// `ap`'s value afterwards (an added parameter makes it a partial application).
    ap_value: Option<&'static str>,
}

const PARAM_EDITS: &[ParamEdit] = &[
    ParamEdit {
        name: "add at end",
        code: "z = parameter { param_name: \"z\", data_type: Int, sort_order: 2 }\n",
        f_wires: &[&["i1"], &["i2"], &[]],
        dropped: &[],
        flagged: &[],
        f_value: Some("12"),
        ap_value: None,
    },
    ParamEdit {
        name: "add in middle",
        code: "y = parameter { sort_order: 2 }\nz = parameter { param_name: \"z\", data_type: Int, sort_order: 1 }\n",
        f_wires: &[&["i1"], &[], &["i2"]],
        dropped: &[],
        flagged: &[],
        f_value: Some("12"),
        ap_value: None,
    },
    ParamEdit {
        name: "remove",
        code: "delete y\ns = expr { b: 5 }\n",
        f_wires: &[&["i1"]],
        dropped: &["y"],
        flagged: &[],
        f_value: None,
        ap_value: None,
    },
    ParamEdit {
        name: "reorder",
        code: "y = parameter { sort_order: -1 }\n",
        f_wires: &[&["i2"], &["i1"]],
        dropped: &[],
        flagged: &[],
        f_value: Some("12"),
        ap_value: Some("12"),
    },
    ParamEdit {
        name: "rename",
        code: "x = parameter { param_name: \"xx\" }\n",
        f_wires: &[&["i1"], &["i2"]],
        dropped: &[],
        flagged: &[],
        f_value: Some("12"),
        ap_value: Some("12"),
    },
    ParamEdit {
        name: "compatible retype",
        code: "y = parameter { data_type: Float }\ns = expr { parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Float }] }\n",
        f_wires: &[&["i1"], &["i2"]],
        dropped: &[],
        flagged: &["y"],
        f_value: None,
        ap_value: None,
    },
    ParamEdit {
        name: "incompatible retype",
        code: "y = parameter { data_type: String }\ns = expr { expression: \"a * 10\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: String }] }\n",
        f_wires: &[&["i1"], &["i2"]],
        dropped: &[],
        flagged: &["y"],
        f_value: None,
        ap_value: None,
    },
];

fn names_in(
    wires: &[atomcad_structure_designer::library_refresh::ReportedWire],
    d: &StructureDesigner,
    node: &str,
) -> Vec<String> {
    let id = node_id(d, "Main", node);
    let mut out: Vec<String> = wires
        .iter()
        .filter(|w| w.network == "Main" && w.scope_path.is_empty() && w.node_id == id)
        .map(|w| w.pin_name.clone())
        .collect();
    out.sort();
    out
}

fn expected_wires(e: &ParamEdit) -> Vec<Vec<String>> {
    e.f_wires
        .iter()
        .map(|a| a.iter().map(|s| s.to_string()).collect())
        .collect()
}

#[test]
fn parameter_edits_are_followed_by_param_id_on_refresh() {
    for e in PARAM_EDITS {
        let ws = workspace();
        let mut d = open(&ws.host);
        let before = wire_ledger(&d);
        edit_lib(&ws.lib, |l| incr(l, "foo", e.code));
        let report = refresh_checked(&mut d, &ws, &before);
        assert_eq!(wires_of(&d, "Main", "f"), expected_wires(e), "{}", e.name);
        assert_eq!(
            names_in(&report.dropped_wires, &d, "f"),
            e.dropped,
            "{}",
            e.name
        );
        assert_eq!(
            names_in(&report.flagged_wires, &d, "f"),
            e.flagged,
            "{}",
            e.name
        );
        if let Some(v) = e.f_value {
            assert_eq!(value(&mut d, "Main", "f"), v, "{}", e.name);
            // The call site inside the HOF body and the `apply` fed by the
            // function pin follow the same parameters.
            assert_eq!(value(&mut d, "Main", "co"), "[2, 12]", "{}", e.name);
            if let Some(v) = e.ap_value {
                assert_eq!(value(&mut d, "Main", "ap"), v, "{}", e.name);
            }
        }
        assert_eq!(status(&d, "lib"), MountStatus::Loaded);
        ok(check_save_reopen_identity(&mut d));
    }
}

#[test]
fn parameter_edits_made_while_the_host_was_closed_are_reconciled_on_open() {
    for e in PARAM_EDITS {
        // The live refresh, for the report to compare against.
        let ws = workspace();
        let mut live = open(&ws.host);
        edit_lib(&ws.lib, |l| incr(l, "foo", e.code));
        let live_report = live.refresh_library("lib").unwrap();

        let ws = workspace();
        let before = wire_ledger(&open(&ws.host));
        edit_lib(&ws.lib, |l| incr(l, "foo", e.code));
        let tripwire = DiskTripwire::arm(ws.dir.path());
        let mut d = open(&ws.host);
        let report = d.take_load_library_report().expect("an open report");
        check_ledger(&before, &d, &report);
        ok(tripwire.check_only_changed(&[]));
        ok(check_invariants(&d));
        assert_eq!(wires_of(&d, "Main", "f"), expected_wires(e), "{}", e.name);
        assert_eq!(
            report.dropped_wires, live_report.dropped_wires,
            "{}",
            e.name
        );
        assert_eq!(
            report.flagged_wires, live_report.flagged_wires,
            "{}",
            e.name
        );
        assert!(d.is_dirty, "{}: an open that reconciled is dirty", e.name);
        if let Some(v) = e.f_value {
            assert_eq!(value(&mut d, "Main", "f"), v, "{}", e.name);
            assert_eq!(value(&mut d, "Main", "co"), "[2, 12]", "{}", e.name);
            if let Some(v) = e.ap_value {
                assert_eq!(value(&mut d, "Main", "ap"), v, "{}", e.name);
            }
        }
        // Saving records the new interface; the next open reconciles nothing.
        save(&mut d);
        let mut again = open(&ws.host);
        let report = again.take_load_library_report();
        assert!(
            report
                .as_ref()
                .is_none_or(|r| r.reconciled_nodes.is_empty()),
            "{}: {:?}",
            e.name,
            report
        );
        assert!(!again.is_dirty);
    }
}

// ---------------------------------------------------------------------------
// Record defs
// ---------------------------------------------------------------------------

fn set_miller(d: &mut StructureDesigner, fields: &[&str]) {
    d.update_record_type_def(
        "Miller",
        fields
            .iter()
            .map(|n| {
                (
                    n.to_string(),
                    atomcad_structure_designer::data_type::DataType::Int,
                )
            })
            .collect(),
    )
    .unwrap();
}

#[test]
fn record_field_edits_follow_field_ids_on_refresh_and_on_open() {
    type Case = (
        &'static str,
        &'static [&'static str],
        Option<&'static str>,
        bool,
    );
    // (name, new field list, e2's value afterwards, e2's wire dropped)
    let cases: &[Case] = &[
        ("add", &["h", "k", "l", "m"], None, false),
        ("add at front", &["m", "h", "k", "l"], None, false),
        ("remove k", &["h", "l"], None, true),
        ("reorder", &["l", "k", "h"], Some("102"), false),
    ];
    for (name, fields, e2, dropped) in cases {
        for on_open in [false, true] {
            let ws = workspace();
            let mut d = open(&ws.host);
            let before = wire_ledger(&d);
            edit_lib(&ws.lib, |l| set_miller(l, fields));
            let report = if on_open {
                d = open(&ws.host);
                d.take_load_library_report().unwrap()
            } else {
                refresh_checked(&mut d, &ws, &before)
            };
            check_ledger(&before, &d, &report);
            let e2_id = node_id(&d, "Main", "e2");
            let e2_dropped = report
                .dropped_wires
                .iter()
                .any(|w| w.node_id == e2_id && w.scope_path.is_empty());
            assert_eq!(e2_dropped, *dropped, "{} (open: {})", name, on_open);
            if let Some(v) = e2 {
                assert_eq!(
                    value(&mut d, "Main", "e2"),
                    *v,
                    "{} (open: {})",
                    name,
                    on_open
                );
            }
            // record_construct's inputs follow the fields: h and l are i1.
            let rc = wires_of(&d, "Main", "rc");
            let def = field_ids(&d, "lib.Miller");
            for (i, (fname, _)) in def.iter().enumerate() {
                let expect: Vec<String> = match fname.as_str() {
                    "h" | "l" => vec!["i1".to_string()],
                    "k" => vec!["i2".to_string()],
                    _ => vec![],
                };
                assert_eq!(
                    rc[i], expect,
                    "{} (open: {}) field {}",
                    name, on_open, fname
                );
            }
            ok(check_invariants(&d));
        }
    }
}

#[test]
fn an_output_list_change_is_reported_for_wires_from_pin_one_up() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let before = wire_ledger(&d);
    // `split`'s outputs are Miller's fields; reordering them moves `l` away
    // from pin 2, which `e3` reads by position (§7.3).
    edit_lib(&ws.lib, |l| set_miller(l, &["l", "h", "k"]));
    let report = refresh_checked(&mut d, &ws, &before);
    let e3 = node_id(&d, "Main", "e3");
    assert!(
        report.output_pin_warnings.iter().any(|w| w.node_id == e3),
        "{:#?}",
        report
    );
    assert!(!report.is_clean());
}

// ---------------------------------------------------------------------------
// Names removed and re-added: freezing and unfreezing
// ---------------------------------------------------------------------------

#[test]
fn a_network_removed_from_the_library_freezes_its_instances_losslessly() {
    let ws = workspace();
    let mut d = open(&ws.host);
    // `bar` calls `foo`: remove it first.
    edit_lib(&ws.lib, |l| {
        l.delete_node_network("bar").unwrap();
        l.delete_node_network("foo").unwrap();
    });
    let report = refresh_freezing_checked(&mut d, &ws);
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
    for name in ["f", "f2"] {
        let id = node_id(&d, "Main", name);
        assert!(is_frozen(
            &d.node_type_registry.node_networks["Main"].nodes[&id],
            &d.node_type_registry
        ));
        assert!(
            report.frozen_nodes.iter().any(|n| n.node_id == id),
            "{}",
            name
        );
    }
    assert!(report.removed_names_in_use.contains(&"lib.foo".to_string()));
    assert!(
        report.dropped_wires.is_empty(),
        "{:#?}",
        report.dropped_wires
    );
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i1"], vec!["i2"]]);
    ok(check_save_reopen_identity(&mut d));
}

#[test]
fn a_record_def_removed_from_the_library_freezes_its_record_nodes_losslessly() {
    let ws = workspace();
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        l.delete_node_network("split").unwrap();
        l.delete_record_type_def("Miller").unwrap();
    });
    let report = refresh_freezing_checked(&mut d, &ws);
    for name in ["rc", "rd"] {
        let id = node_id(&d, "Main", name);
        assert!(
            report.frozen_nodes.iter().any(|n| n.node_id == id),
            "{}",
            name
        );
    }
    assert!(
        report
            .removed_names_in_use
            .contains(&"lib.Miller".to_string())
    );
    assert!(
        report.dropped_wires.is_empty(),
        "{:#?}",
        report.dropped_wires
    );
    assert_eq!(
        wires_of(&d, "Main", "rc"),
        vec![vec!["i1"], vec!["i2"], vec!["i1"]]
    );
    assert_eq!(wires_of(&d, "Main", "e2"), vec![vec!["rd"]]);
    ok(check_save_reopen_identity(&mut d));
}

#[test]
fn a_frozen_instance_unfreezes_with_its_wires_reconciled_to_the_new_interface() {
    // With and without an interface change while the name was gone.
    for (reinsert, f_wires) in [
        (FOO, vec![vec!["i1"], vec!["i2"]]),
        (
            "y = parameter { param_name: \"y\", data_type: Int, sort_order: 0 }
x = parameter { param_name: \"x\", data_type: Int, sort_order: 1 }
s = expr { a: x, b: y, expression: \"a * 10 + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
",
            vec![vec!["i2"], vec!["i1"]],
        ),
    ] {
        let ws = workspace();
        let lib_before = std::fs::read(&ws.lib).unwrap();
        let mut d = open(&ws.host);
        let before = wire_ledger(&d);
        // Remove `foo` (and its caller in the library).
        edit_lib(&ws.lib, |l| {
            l.delete_node_network("bar").unwrap();
            l.delete_node_network("foo").unwrap();
        });
        d.refresh_library("lib").unwrap();
        let f = node_id(&d, "Main", "f");
        assert!(is_frozen(&d.node_type_registry.node_networks["Main"].nodes[&f], &d.node_type_registry));

        // Re-add it — by name only, so its parameters have new ids and the
        // match is by name — or restore the very file.
        if reinsert == FOO {
            std::fs::write(&ws.lib, &lib_before).unwrap();
            bump_mtime(&ws.lib);
        } else {
            edit_lib(&ws.lib, |l| edit(l, "foo", reinsert));
        }
        let report = d.refresh_library("lib").unwrap();
        let node = &d.node_type_registry.node_networks["Main"].nodes[&f];
        assert!(!is_frozen(node, &d.node_type_registry));
        // The recorded layout must not shadow the network's own interface.
        assert!(node.custom_node_type.is_none());
        let own = &d.node_type_registry.node_networks["lib.foo"].node_type;
        assert_eq!(
            d.node_type_registry.get_node_type_for_node(node).unwrap().name,
            own.name
        );
        assert_eq!(wires_of(&d, "Main", "f"), f_wires);
        assert_eq!(value(&mut d, "Main", "f"), "12");
        assert!(report.dropped_wires.is_empty(), "{:#?}", report.dropped_wires);
        let _ = &before;
        ok(check_invariants(&d));
    }
}

/// Found by the randomized harness (seed 316777): a retype that makes a
/// HOF-body wire incompatible is disconnected by the zone-body repair — which
/// the report must say, not only "flagged".
#[test]
fn a_retype_that_breaks_a_body_wire_reports_it_dropped() {
    let retype = "x = parameter { data_type: String }\ns = expr { expression: \"b\", parameters: [{ name: \"a\", data_type: String }, { name: \"b\", data_type: Int }] }\n";
    for on_open in [false, true] {
        let ws = workspace();
        let mut d = open(&ws.host);
        let before = wire_ledger(&d);
        edit_lib(&ws.lib, |l| incr(l, "foo", retype));
        let report = if on_open {
            d = open(&ws.host);
            d.take_load_library_report().unwrap()
        } else {
            refresh_checked(&mut d, &ws, &before)
        };
        check_ledger(&before, &d, &report);
        let mp = node_id(&d, "Main", "mp");
        assert!(
            report
                .dropped_wires
                .iter()
                .any(|w| w.scope_path == vec![mp] && w.pin_name == "x"),
            "open: {}: {:#?}",
            on_open,
            report.dropped_wires
        );
    }
}

/// A retarget to a version without the name freezes; a retarget back
/// unfreezes with every wire where it was.
#[test]
fn a_retarget_can_freeze_and_a_retarget_back_unfreezes() {
    let ws = workspace();
    let v2 = ws.dir.path().join("lib_v2.cnnd");
    std::fs::copy(&ws.lib, &v2).unwrap();
    edit_lib(&v2, |l| {
        l.delete_node_network("bar").unwrap();
        l.delete_node_network("foo").unwrap();
    });
    let mut d = open(&ws.host);
    let before = positional_ledger(&d);
    d.retarget_library("lib", "lib_v2.cnnd").unwrap();
    let f = node_id(&d, "Main", "f");
    assert!(is_frozen(
        &d.node_type_registry.node_networks["Main"].nodes[&f],
        &d.node_type_registry
    ));
    assert_eq!(positional_ledger(&d), before);
    let report = d.retarget_library("lib", "lib.cnnd").unwrap();
    assert!(
        report.dropped_wires.is_empty(),
        "{:#?}",
        report.dropped_wires
    );
    assert!(!is_frozen(
        &d.node_type_registry.node_networks["Main"].nodes[&f],
        &d.node_type_registry
    ));
    assert_eq!(positional_ledger(&d), before);
    assert_eq!(value(&mut d, "Main", "f"), "12");
    ok(check_save_reopen_identity(&mut d));
}

// ---------------------------------------------------------------------------
// Retarget
// ---------------------------------------------------------------------------

#[test]
fn a_retarget_to_an_edited_copy_keeps_wires_by_param_id_and_undo_points_back() {
    let ws = workspace();
    let v3 = ws.dir.path().join("lib_v3.cnnd");
    std::fs::copy(&ws.lib, &v3).unwrap();
    edit_lib(&v3, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    let mut d = open(&ws.host);
    let before = wire_ledger(&d);
    let text_before = saved_text(&mut d, Some(ws.dir.path()));
    let tripwire = DiskTripwire::arm(ws.dir.path());
    let mut report = None;
    ok(check_undo_inverse(&mut d, |d| {
        report = Some(d.retarget_library("lib", "lib_v3.cnnd").unwrap());
    }));
    let report = report.unwrap();
    check_ledger(&before, &d, &report);
    ok(tripwire.check_only_changed(&[]));
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
    assert_eq!(value(&mut d, "Main", "f"), "12");
    let m = d.node_type_registry.library_links.get("lib").unwrap();
    assert_eq!(m.rel_path, "lib_v3.cnnd");
    // Only the import path changed in the host's own text.
    let text_after = saved_text(&mut d, Some(ws.dir.path()));
    assert!(text_after.contains("lib_v3.cnnd"));
    assert!(text_after.contains("`lib.foo`") || text_after.contains("\"lib.foo\""));
    assert_eq!(
        text_before.matches("lib.foo").count(),
        text_after.matches("lib.foo").count()
    );
    // Undo points the alias back at the old file.
    d.undo();
    assert_eq!(
        d.node_type_registry
            .library_links
            .get("lib")
            .unwrap()
            .rel_path,
        "lib.cnnd"
    );
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i1"], vec!["i2"]]);
}

#[test]
fn a_retarget_to_a_file_that_cannot_be_mounted_changes_nothing() {
    let ws = workspace();
    std::fs::write(ws.dir.path().join("broken.cnnd"), "not json").unwrap();
    let mut d = open(&ws.host);
    let fp = local_fingerprint(&mut d);
    let mounts = mount_fingerprints(&d);
    assert!(d.retarget_library("lib", "broken.cnnd").is_err());
    assert!(d.retarget_library("lib", "nowhere.cnnd").is_err());
    assert_eq!(local_fingerprint(&mut d), fp);
    assert_eq!(mount_fingerprints(&d), mounts);
    assert!(!d.undo_stack.can_undo());
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
}

// ---------------------------------------------------------------------------
// Detection (D7)
// ---------------------------------------------------------------------------

#[test]
fn detection_distinguishes_touches_changes_deletions_and_restorations() {
    let ws = workspace();
    let mut d = open(&ws.host);
    // Unchanged: nothing.
    assert!(d.check_dependencies().is_none());
    // Touched, same content: nothing (the hash decides).
    bump_mtime(&ws.lib);
    assert!(d.check_dependencies().is_none());
    assert!(!d.undo_stack.can_undo());

    // Deleted: `Missing`, nothing removed, the loaded version keeps working.
    let bytes = std::fs::read(&ws.lib).unwrap();
    std::fs::remove_file(&ws.lib).unwrap();
    let fp = local_fingerprint(&mut d);
    let report = d.check_dependencies().expect("a status change");
    assert_eq!(
        report.status_changes,
        vec![("lib".to_string(), MountStatus::Missing)]
    );
    assert_eq!(status(&d, "lib"), MountStatus::Missing);
    assert_eq!(local_fingerprint(&mut d), fp);
    assert_eq!(value(&mut d, "Main", "f"), "12");
    assert!(d.check_dependencies().is_none(), "reported once");
    // Saving while missing keeps the import intact.
    ok(check_save_reopen_identity(&mut d));

    // Restored as it was: back to `Loaded`, nothing to refresh.
    std::fs::write(&ws.lib, &bytes).unwrap();
    bump_mtime(&ws.lib);
    let report = d.check_dependencies().expect("a status change");
    assert_eq!(
        report.status_changes,
        vec![("lib".to_string(), MountStatus::Loaded)]
    );
    assert!(!d.undo_stack.can_undo());

    // Changed: refreshed, one undo step.
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "s = expr { expression: \"a * 100 + b\" }\n")
    });
    let report = d.check_dependencies().expect("a refresh");
    assert_eq!(report.refreshed_mounts.len(), 1);
    assert!(report.is_clean(), "{:#?}", report);
    assert_eq!(value(&mut d, "Main", "f"), "102");
    assert!(d.undo_stack.can_undo());
    assert!(d.check_dependencies().is_none());
}

#[test]
fn a_file_made_unparseable_keeps_the_loaded_version_and_is_refreshed_when_fixed() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let good = std::fs::read(&ws.lib).unwrap();
    std::fs::write(&ws.lib, "{ truncated").unwrap();
    bump_mtime(&ws.lib);
    let fp = local_fingerprint(&mut d);
    let report = d.check_dependencies().expect("a status change");
    assert!(
        matches!(status(&d, "lib"), MountStatus::Error(_)),
        "{:?}",
        report
    );
    assert!(!d.undo_stack.can_undo());
    assert_eq!(local_fingerprint(&mut d), fp);
    assert_eq!(value(&mut d, "Main", "f"), "12");
    assert!(
        d.check_dependencies().is_none(),
        "not retried until it changes"
    );

    // Fixed with a change: refreshed.
    std::fs::write(&ws.lib, &good).unwrap();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "s = expr { expression: \"a * 100 + b\" }\n")
    });
    d.check_dependencies().expect("a refresh");
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
    assert_eq!(value(&mut d, "Main", "f"), "102");
}

#[test]
fn several_changed_files_are_refreshed_in_one_command() {
    let ws = workspace();
    let lib2 = ws.dir.path().join("lib2.cnnd");
    write_lib(&lib2);
    let mut d = open(&ws.host);
    d.link_library("lib2.cnnd", "lib2").unwrap();
    incr(&mut d, "Main", "g = `lib2.foo` { x: i2, y: i1 }\n");
    save(&mut d);
    let mut d = open(&ws.host);
    let before = wire_ledger(&d);
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    edit_lib(&lib2, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    let mut report = None;
    ok(check_undo_inverse(&mut d, |d| {
        report = d.check_dependencies()
    }));
    let report = report.unwrap();
    assert_eq!(report.refreshed_mounts.len(), 2, "{:#?}", report);
    check_ledger(&before, &d, &report);
    assert_eq!(value(&mut d, "Main", "g"), "21");
    // One command: a single undo takes both back.
    d.undo();
    assert!(!d.undo_stack.can_undo());
}

// ---------------------------------------------------------------------------
// Undo (D9)
// ---------------------------------------------------------------------------

#[test]
fn undo_of_a_refresh_restores_the_old_content_and_evaluation() {
    let ws = workspace();
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        incr(
            l,
            "foo",
            "y = parameter { sort_order: -1 }\ns = expr { expression: \"a * 100 + b\" }\n",
        )
    });
    let mounts_before = mount_fingerprints(&d);
    d.refresh_library("lib").unwrap();
    assert_eq!(value(&mut d, "Main", "f"), "102");
    assert!(d.undo());
    assert_eq!(mount_fingerprints(&d), mounts_before);
    assert_eq!(value(&mut d, "Main", "f"), "12");
    assert_eq!(status(&d, "lib"), MountStatus::OlderThanDisk);
    // D7's `last_seen` rule: no automatic refresh follows the undo.
    assert!(d.check_dependencies().is_none());
    assert_eq!(value(&mut d, "Main", "f"), "12");
    // An explicit refresh brings it forward again (and drops the redo).
    d.refresh_library("lib").unwrap();
    assert_eq!(value(&mut d, "Main", "f"), "102");
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
}

#[test]
fn edits_before_and_after_a_refresh_interleave_with_it_in_the_history() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let s0 = local_fingerprint(&mut d);
    incr(&mut d, "Main", "i3 = int { value: 3 }\n");
    let s1 = local_fingerprint(&mut d);
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    d.refresh_library("lib").unwrap();
    let s2 = local_fingerprint(&mut d);
    incr(&mut d, "Main", "i4 = int { value: 4 }\n");
    // Undo the later edit, then the refresh, then the earlier edit.
    assert!(d.undo());
    assert_eq!(local_fingerprint(&mut d), s2);
    assert!(d.undo());
    assert_eq!(local_fingerprint(&mut d), s1);
    assert!(d.undo());
    assert_eq!(local_fingerprint(&mut d), s0);
    // And forward again.
    assert!(d.redo());
    assert!(d.redo());
    assert_eq!(local_fingerprint(&mut d), s2);
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
}

#[test]
fn an_automatic_refresh_never_destroys_redo_history() {
    let ws = workspace();
    let mut d = open(&ws.host);
    incr(&mut d, "Main", "i3 = int { value: 3 }\n");
    incr(&mut d, "Main", "i4 = int { value: 4 }\n");
    let edited = local_fingerprint(&mut d);
    d.undo();
    d.undo();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    let report = d.check_dependencies().expect("held");
    assert_eq!(report.held, vec!["lib".to_string()]);
    assert!(report.refreshed_mounts.is_empty());
    assert_eq!(status(&d, "lib"), MountStatus::ChangedOnDisk);
    assert!(
        d.check_dependencies().is_none(),
        "held changes are reported once"
    );
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i1"], vec!["i2"]]);
    // Both edits are still redoable.
    assert!(d.redo());
    assert!(d.redo());
    assert_eq!(local_fingerprint(&mut d), edited);
    assert_eq!(status(&d, "lib"), MountStatus::ChangedOnDisk);
    // A new edit, then the next check applies it as one command.
    incr(&mut d, "Main", "i5 = int { value: 5 }\n");
    let report = d.check_dependencies().expect("applied");
    assert_eq!(report.refreshed_mounts.len(), 1);
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
}

#[test]
fn an_explicit_refresh_while_held_applies_and_truncates_redo() {
    let ws = workspace();
    let mut d = open(&ws.host);
    incr(&mut d, "Main", "i3 = int { value: 3 }\n");
    d.undo();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    assert!(d.check_dependencies().unwrap().held.len() == 1);
    d.refresh_library("lib").unwrap();
    assert!(!d.undo_stack.can_redo());
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
}

#[test]
fn a_new_change_after_an_undone_refresh_is_held_until_the_next_edit() {
    let ws = workspace();
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    d.check_dependencies().expect("applied");
    d.undo();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "s = expr { expression: \"a * 100 + b\" }\n")
    });
    let report = d.check_dependencies().expect("held");
    assert_eq!(report.held, vec!["lib".to_string()]);
    assert_eq!(value(&mut d, "Main", "f"), "12");
    incr(&mut d, "Main", "i3 = int { value: 3 }\n");
    d.check_dependencies().expect("applied");
    assert_eq!(value(&mut d, "Main", "f"), "102");
}

#[test]
fn saving_after_an_undone_refresh_records_the_old_interface() {
    let ws = workspace();
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    d.refresh_library("lib").unwrap();
    d.undo();
    save(&mut d);
    // Reopened, the host is repaired forward from the recorded (old)
    // interface — not left shifted by position.
    let mut d = open(&ws.host);
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
    assert_eq!(value(&mut d, "Main", "f"), "12");
}

#[test]
fn a_held_refresh_survives_a_save() {
    let ws = workspace();
    let mut d = open(&ws.host);
    incr(&mut d, "Main", "i3 = int { value: 3 }\n");
    incr(&mut d, "Main", "i4 = int { value: 4 }\n");
    d.undo();
    d.undo();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    assert!(!d.check_dependencies().unwrap().held.is_empty());
    assert!(d.redo(), "the redo tail is usable up to the save");
    save(&mut d);
    let mut d = open(&ws.host);
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
    assert_eq!(value(&mut d, "Main", "f"), "12");
}

#[test]
fn a_refresh_evicted_from_the_history_leaves_a_state_that_saves() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.undo_stack.max_history = 5;
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    d.refresh_library("lib").unwrap();
    for i in 0..8 {
        incr(
            &mut d,
            "Main",
            &format!("n{} = int {{ value: {} }}\n", i, i),
        );
    }
    while d.undo() {}
    // The refresh was evicted: its effect stays.
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
    ok(check_save_reopen_identity(&mut d));
}

#[test]
fn a_refresh_keeps_the_host_dirty_and_a_host_edit_survives_it() {
    let ws = workspace();
    let mut d = open(&ws.host);
    incr(&mut d, "Main", "i3 = int { value: 3 }\n");
    assert!(d.is_dirty);
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    d.refresh_library("lib").unwrap();
    assert!(d.is_dirty);
    save(&mut d);
    let d = open(&ws.host);
    node_id(&d, "Main", "i3");
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
}

// ---------------------------------------------------------------------------
// Opening
// ---------------------------------------------------------------------------

#[test]
fn a_content_only_change_is_reported_on_open_and_repairs_nothing() {
    let ws = workspace();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "s = expr { expression: \"a * 100 + b\" }\n")
    });
    let mut d = open(&ws.host);
    let report = d.take_load_library_report().expect("a report");
    assert_eq!(report.changed_since_saved, vec!["lib".to_string()]);
    assert!(report.reconciled_nodes.is_empty());
    assert!(report.is_clean());
    assert!(!d.is_dirty);
    assert_eq!(value(&mut d, "Main", "f"), "102");
}

#[test]
fn an_unchanged_library_opens_without_a_report() {
    let ws = workspace();
    let mut d = open(&ws.host);
    assert!(d.take_load_library_report().is_none());
    assert!(!d.is_dirty);
}

#[test]
fn a_library_missing_at_save_and_back_with_a_new_interface_is_reconciled_on_open() {
    let ws = workspace();
    let bytes = std::fs::read(&ws.lib).unwrap();
    std::fs::remove_file(&ws.lib).unwrap();
    // Saved while missing: the `uses` table is carried over verbatim.
    let mut d = open(&ws.host);
    assert_eq!(status(&d, "lib"), MountStatus::Missing);
    let before = wire_ledger(&d);
    save(&mut d);
    // Back, with a parameter inserted in the middle.
    std::fs::write(&ws.lib, &bytes).unwrap();
    edit_lib(&ws.lib, |l| {
        incr(
            l,
            "foo",
            "y = parameter { sort_order: 2 }\nz = parameter { param_name: \"z\", data_type: Int, sort_order: 1 }\n",
        )
    });
    let mut d = open(&ws.host);
    let report = d.take_load_library_report().unwrap();
    assert!(
        report.dropped_wires.is_empty(),
        "{:#?}",
        report.dropped_wires
    );
    assert_eq!(
        wires_of(&d, "Main", "f"),
        vec![vec!["i1"], vec![], vec!["i2"]]
    );
    assert_eq!(value(&mut d, "Main", "f"), "12");
    let _ = before;
}

#[test]
fn nested_calls_are_reconciled_in_memory_and_the_library_file_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let common = dir.path().join("common.cnnd");
    write_lib(&common);
    let lib = dir.path().join("lib.cnnd");
    {
        let mut d = new_design(&lib);
        d.link_library("common.cnnd", "common").unwrap();
        edit(
            &mut d,
            "user",
            "one = int { value: 1 }\ntwo = int { value: 2 }\nc = `common.foo` { x: one, y: two }\noutput c\n",
        );
        d.delete_node_network("Main").ok();
        save(&mut d);
    }
    let host = dir.path().join("host.cnnd");
    {
        let mut d = new_design(&host);
        d.link_library("lib.cnnd", "lib").unwrap();
        edit(&mut d, "Main", "u = `lib.user` {}\noutput u\n");
        save(&mut d);
    }
    // common v2: a parameter inserted in the middle.
    edit_lib(&common, |l| {
        incr(
            l,
            "foo",
            "y = parameter { sort_order: 2 }\nz = parameter { param_name: \"z\", data_type: Int, sort_order: 1 }\n",
        )
    });
    let lib_bytes = std::fs::read(&lib).unwrap();
    let tripwire = DiskTripwire::arm(dir.path());
    let mut d = open(&host);
    assert_eq!(value(&mut d, "Main", "u"), "12");
    let c_wires = wires_of(&d, "lib.user", "c");
    assert_eq!(c_wires, vec![vec!["one"], vec![], vec!["two"]]);
    ok(tripwire.check_only_changed(&[]));
    assert_eq!(std::fs::read(&lib).unwrap(), lib_bytes);
}

#[test]
fn a_refresh_leaves_the_library_s_own_callers_as_the_library_wires_them() {
    let ws = workspace();
    let mut d = open(&ws.host);
    edit_lib(&ws.lib, |l| {
        incr(
            l,
            "foo",
            "y = parameter { sort_order: 2 }\nz = parameter { param_name: \"z\", data_type: Int, sort_order: 1 }\n",
        )
    });
    d.refresh_library("lib").unwrap();
    let standalone = open(&ws.lib);
    assert_eq!(
        wires_of(&d, "lib.bar", "c"),
        wires_of(&standalone, "bar", "c")
    );
    assert_eq!(
        wires_of(&d, "lib.bar", "c"),
        vec![vec!["one"], vec![], vec!["two"]]
    );
}

#[test]
fn duplicate_param_ids_in_a_library_never_move_a_wire_to_the_wrong_pin() {
    let ws = workspace();
    let d0 = open(&ws.host);
    let before = wire_ledger(&d0);
    // Hand-edit the library so that the node listed first (lower node id)
    // — `x` — keeps sharing its id with `y`, and rename `x` away: a correct
    // match can only be by name.
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&ws.lib).unwrap()).unwrap();
    let nets = json["node_networks"].as_array_mut().unwrap();
    let foo = nets.iter_mut().find(|n| n[0] == "foo").unwrap();
    let nodes = foo[1]["nodes"].as_array_mut().unwrap();
    let mut ids = Vec::new();
    for n in nodes.iter() {
        if n["node_type_name"] == "parameter" {
            ids.push(n["data"]["param_id"].as_u64().unwrap());
        }
    }
    let (x_id, y_id) = (ids[0], ids[1]);
    for n in nodes.iter_mut() {
        if n["node_type_name"] == "parameter" && n["data"]["param_id"].as_u64() == Some(x_id) {
            // `x` takes `y`'s id (and keeps its name).
            n["data"]["param_id"] = serde_json::json!(y_id);
        }
    }
    std::fs::write(&ws.lib, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    let mut d = open(&ws.host);
    let report = d.take_load_library_report().unwrap_or_default();
    // The heal gave the parameters new ids, so the id-keyed ledger cannot
    // follow them; judge by name and position instead: nothing was dropped
    // silently, and whatever was healed, `x` still gets `i1` and `y` `i2`.
    assert!(!before.is_empty());
    assert_eq!(
        positional_ledger(&d).len() + report.dropped_wires.len(),
        before.len()
    );
    let params = atomcad_structure_designer::network_validator::live_parameters(
        &d.node_type_registry.node_networks["lib.foo"],
    );
    let f = wires_of(&d, "Main", "f");
    for (i, p) in params.iter().enumerate() {
        let expect = if p.name == "x" { "i1" } else { "i2" };
        assert!(
            f[i].is_empty() || f[i] == vec![expect.to_string()],
            "{} got {:?}",
            p.name,
            f[i]
        );
    }
}

// ---------------------------------------------------------------------------
// The editor's position
// ---------------------------------------------------------------------------

#[test]
fn an_active_linked_network_removed_by_a_refresh_falls_back_and_comes_back_on_undo() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("lib.bar".to_string()));
    edit_lib(&ws.lib, |l| l.delete_node_network("bar").unwrap());
    d.refresh_library("lib").unwrap();
    let active = d.active_node_network_name.clone().unwrap();
    assert!(d.node_type_registry.node_networks.contains_key(&active));
    assert!(
        d.node_type_registry
            .library_links
            .mount_containing(&active)
            .is_none()
    );
    d.undo();
    assert!(d.node_type_registry.node_networks.contains_key("lib.bar"));
}

// ---------------------------------------------------------------------------
// Data files
// ---------------------------------------------------------------------------

#[test]
fn a_host_data_file_refresh_replaces_only_the_file_content() {
    let ws = workspace();
    let xyz = ws.dir.path().join("tip.xyz");
    std::fs::write(&xyz, "1\none\nC 0.0 0.0 0.0\n").unwrap();
    let mut d = open(&ws.host);
    incr(
        &mut d,
        "Main",
        "t = import_xyz { file_name: \"tip.xyz\" }\n",
    );
    save(&mut d);
    let mut d = open(&ws.host);
    let atoms = |d: &mut StructureDesigner| {
        d.set_active_node_network_name(Some("Main".to_string()));
        let id = node_id(d, "Main", "t");
        match d.evaluate_node_output(&[], id, 0) {
            atomcad_structure_designer::evaluator::network_result::NetworkResult::Molecule(m) => {
                m.atoms.get_num_of_atoms()
            }
            other => panic!("{}", other.to_display_string()),
        }
    };
    assert_eq!(atoms(&mut d), 1);
    let data_json = |d: &StructureDesigner| {
        let id = node_id(d, "Main", "t");
        let node = &d.node_type_registry.node_networks["Main"].nodes[&id];
        let mut copy = node.data.clone_box();
        let saver = d.node_type_registry.node_data_saver_for("import_xyz");
        saver(copy.as_mut(), None).unwrap()
    };
    let fields_before = data_json(&d);
    std::fs::write(&xyz, "2\ntwo\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\n").unwrap();
    bump_mtime(&xyz);
    let tripwire = DiskTripwire::arm(ws.dir.path());
    let mut report = None;
    ok(check_undo_inverse(&mut d, |d| {
        report = d.check_dependencies()
    }));
    let report = report.unwrap();
    ok(tripwire.check_only_changed(&[]));
    assert_eq!(report.refreshed_data_files.len(), 1, "{:#?}", report);
    assert_eq!(atoms(&mut d), 2);
    assert_eq!(data_json(&d), fields_before);
    d.undo();
    assert_eq!(atoms(&mut d), 1, "undo brings the old content back");
    assert!(d.check_dependencies().is_none(), "last_seen rule");
    d.redo();
    assert_eq!(atoms(&mut d), 2);
}

/// `import_cif` and `import_cube` read their file at load too: a refresh
/// re-reads it, keeps every user-set field (compared field by field through
/// the node's own saver), and its undo brings the old content back.
#[test]
fn cif_and_cube_data_files_are_refreshed_keeping_every_user_field() {
    use atomcad_test_support::fixture_path;
    for (node, file, first, second, props) in [
        (
            "import_cif",
            "data.cif",
            "cif/diamond.cif",
            "cif/nacl.cif",
            "use_cif_bonds: false, infer_bonds: true, bond_tolerance: 1.3",
        ),
        (
            "import_cube",
            "data.cube",
            "cube/ramp_3x4x5.cube",
            "cube/two_fragments.cube",
            "",
        ),
    ] {
        let ws = workspace();
        let path = ws.dir.path().join(file);
        std::fs::copy(fixture_path(first), &path).unwrap();
        let mut d = open(&ws.host);
        incr(&mut d, "Main", &format!("t = {} {{ {} }}\n", node, props));
        // The text format does not set these nodes' `file_name` (a literal
        // on that pin is ignored); set it through the node's own saver and
        // loader, as the property panel's file picker does.
        {
            let id = node_id(&d, "Main", "t");
            let saver = d.node_type_registry.node_data_saver_for(node);
            let loader = d.node_type_registry.node_data_loader_for(node);
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
            n.data = loader(&json, Some(&ws.dir.path().to_string_lossy())).unwrap();
        }
        save(&mut d);
        let mut d = open(&ws.host);
        let fields = |d: &StructureDesigner| {
            let id = node_id(d, "Main", "t");
            let n = &d.node_type_registry.node_networks["Main"].nodes[&id];
            let mut copy = n.data.clone_box();
            (d.node_type_registry.node_data_saver_for(node))(copy.as_mut(), None).unwrap()
        };
        let before_fields = fields(&d);
        let before_value = value(&mut d, "Main", "t");
        std::fs::copy(fixture_path(second), &path).unwrap();
        bump_mtime(&path);
        let report = d
            .check_dependencies()
            .unwrap_or_else(|| panic!("{}: nothing refreshed", node));
        assert_eq!(
            report.refreshed_data_files.len(),
            1,
            "{}: {:#?}",
            node,
            report
        );
        assert_eq!(fields(&d), before_fields, "{}", node);
        let after_value = value(&mut d, "Main", "t");
        assert_ne!(
            after_value, before_value,
            "{}: the new content is read",
            node
        );
        d.undo();
        assert_eq!(value(&mut d, "Main", "t"), before_value, "{}", node);
        assert_eq!(fields(&d), before_fields, "{}", node);
    }
}

#[test]
fn a_change_in_a_nested_library_refreshes_its_direct_mount() {
    let dir = tempfile::tempdir().unwrap();
    let common = dir.path().join("common.cnnd");
    write_lib(&common);
    let lib = dir.path().join("lib.cnnd");
    {
        let mut d = new_design(&lib);
        d.link_library("common.cnnd", "common").unwrap();
        edit(
            &mut d,
            "user",
            "one = int { value: 1 }\ntwo = int { value: 2 }\nc = `common.foo` { x: one, y: two }\noutput c\n",
        );
        d.delete_node_network("Main").ok();
        save(&mut d);
    }
    let host = dir.path().join("host.cnnd");
    {
        let mut d = new_design(&host);
        d.link_library("lib.cnnd", "lib").unwrap();
        edit(&mut d, "Main", "u = `lib.user` {}\noutput u\n");
        save(&mut d);
    }
    let mut d = open(&host);
    assert_eq!(value(&mut d, "Main", "u"), "12");
    edit_lib(&common, |l| {
        incr(l, "foo", "s = expr { expression: \"a * 100 + b\" }\n")
    });
    let report = d.check_dependencies().expect("refreshed");
    assert_eq!(report.refreshed_mounts.len(), 1, "{:#?}", report);
    assert!(report.refreshed_mounts[0].starts_with("lib "));
    assert_eq!(value(&mut d, "Main", "u"), "102");
    d.undo();
    assert_eq!(value(&mut d, "Main", "u"), "12");
    assert_eq!(status(&d, "lib.common"), MountStatus::OlderThanDisk);
    assert!(d.check_dependencies().is_none());
}

#[test]
fn a_library_data_file_change_refreshes_the_library() {
    let ws = workspace();
    let xyz = ws.dir.path().join("tip.xyz");
    std::fs::write(&xyz, "1\none\nC 0.0 0.0 0.0\n").unwrap();
    edit_lib(&ws.lib, |l| {
        edit(
            l,
            "tip",
            "t = import_xyz { file_name: \"tip.xyz\" }\noutput t\n",
        )
    });
    let mut d = open(&ws.host);
    incr(&mut d, "Main", "tp = `lib.tip` {}\n");
    let count = |d: &mut StructureDesigner| {
        d.set_active_node_network_name(Some("Main".to_string()));
        let id = node_id(d, "Main", "tp");
        match d.evaluate_node_output(&[], id, 0) {
            atomcad_structure_designer::evaluator::network_result::NetworkResult::Molecule(m) => {
                m.atoms.get_num_of_atoms()
            }
            other => panic!("{}", other.to_display_string()),
        }
    };
    assert_eq!(count(&mut d), 1);
    std::fs::write(&xyz, "2\ntwo\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\n").unwrap();
    bump_mtime(&xyz);
    let report = d.check_dependencies().expect("refreshed");
    assert_eq!(report.refreshed_mounts.len(), 1, "{:#?}", report);
    assert_eq!(count(&mut d), 2);
}

// ---------------------------------------------------------------------------
// Fault injection (§11.1)
// ---------------------------------------------------------------------------

/// A `LinkFs` over the real disk that can fail reads of one file, or change
/// that file right after it is stat'ed.
#[derive(Default)]
struct FaultFs {
    fail_read_of: Mutex<Option<PathBuf>>,
    /// `(path, bytes)`: written over `path` after its next stat.
    change_after_stat: Mutex<Option<(PathBuf, Vec<u8>)>>,
}

impl LinkFs for FaultFs {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        if self.fail_read_of.lock().unwrap().as_deref() == Some(p) {
            return Err(io::Error::other("injected read failure"));
        }
        RealFs.read(p)
    }
    fn stat(&self, p: &Path) -> io::Result<FileMeta> {
        let meta = RealFs.stat(p);
        let mut change = self.change_after_stat.lock().unwrap();
        if change.as_ref().is_some_and(|(path, _)| path == p) {
            let (path, bytes) = change.take().unwrap();
            std::fs::write(&path, bytes).unwrap();
            bump_mtime(&path);
        }
        meta
    }
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
        RealFs.canonicalize(p)
    }
    fn write(&self, p: &Path, b: &[u8]) -> io::Result<()> {
        RealFs.write(p, b)
    }
    fn rename(&self, a: &Path, b: &Path) -> io::Result<()> {
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
fn a_failing_library_read_marks_the_mount_and_keeps_the_loaded_content() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let fs = Arc::new(FaultFs::default());
    d.node_type_registry.library_links.set_fs(fs.clone());
    let canonical = std::fs::canonicalize(&ws.lib).unwrap();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    *fs.fail_read_of.lock().unwrap() = Some(canonical);
    let fp = local_fingerprint(&mut d);
    let mounts = mount_fingerprints(&d);
    let before = wire_ledger(&d);
    let report = d.check_dependencies().expect("a status change");
    assert!(
        matches!(status(&d, "lib"), MountStatus::Error(_)),
        "{:#?}",
        report
    );
    assert_eq!(local_fingerprint(&mut d), fp);
    assert_eq!(mount_fingerprints(&d), mounts, "nothing partially mounted");
    ok(check_wire_ledger(
        &before,
        &wire_ledger(&d),
        &BTreeSet::new(),
    ));
    assert_eq!(value(&mut d, "Main", "f"), "12");
    assert!(!d.undo_stack.can_undo());
    // Readable again: refreshed.
    *fs.fail_read_of.lock().unwrap() = None;
    bump_mtime(&ws.lib);
    d.check_dependencies().expect("refreshed");
    assert_eq!(status(&d, "lib"), MountStatus::Loaded);
    assert_eq!(wires_of(&d, "Main", "f"), vec![vec!["i2"], vec!["i1"]]);
}

#[test]
fn a_library_changing_during_a_refresh_is_detected_again_afterwards() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let fs = Arc::new(FaultFs::default());
    d.node_type_registry.library_links.set_fs(fs.clone());
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
    // The next version, written right after the refresh stats the file (so
    // between its stat and its read).
    let v1 = std::fs::read(&ws.lib).unwrap();
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "s = expr { expression: \"a * 100 + b\" }\n")
    });
    let v2 = std::fs::read(&ws.lib).unwrap();
    std::fs::write(&ws.lib, &v1).unwrap();
    bump_mtime(&ws.lib);
    let canonical = std::fs::canonicalize(&ws.lib).unwrap();
    // The check stats and reads v1; the refresh's mount stats (→ v2 lands)
    // and reads v2. Either way the stamp must describe the parsed bytes.
    d.check_dependencies().expect("refreshed");
    *fs.change_after_stat.lock().unwrap() = Some((canonical, v2));
    // Whatever was parsed, the next checks converge on the newest file.
    for _ in 0..3 {
        d.check_dependencies();
    }
    assert_eq!(value(&mut d, "Main", "f"), "102");
    let m = d.node_type_registry.library_links.get("lib").unwrap();
    let on_disk = blake3::hash(&std::fs::read(&ws.lib).unwrap())
        .to_hex()
        .to_string();
    assert_eq!(m.loaded.as_ref().unwrap().blake3, on_disk);
}

#[test]
fn the_mount_fingerprint_of_an_untouched_library_is_unchanged_by_a_host_data_refresh() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let lib = mount_fingerprint(&d.node_type_registry, "lib");
    std::fs::write(ws.dir.path().join("a.xyz"), "1\na\nC 0 0 0\n").unwrap();
    incr(&mut d, "Main", "t = import_xyz { file_name: \"a.xyz\" }\n");
    save(&mut d);
    let mut d = open(&ws.host);
    std::fs::write(ws.dir.path().join("a.xyz"), "1\na\nH 0 0 0\n").unwrap();
    bump_mtime(&ws.dir.path().join("a.xyz"));
    d.check_dependencies().expect("refreshed");
    assert_eq!(mount_fingerprint(&d.node_type_registry, "lib"), lib);
    let _ = WireSlot::Pos(0);
}
