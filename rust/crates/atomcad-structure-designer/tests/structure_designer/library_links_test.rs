//! Library linking, Phase 1 (`doc/design_library_linking.md` §11): format,
//! mounting, save, link/unlink, and the frozen-node rule of §8.
//!
//! Every test works on a temp copy of `fixtures/library_linking/` and judges
//! the outcome with the shared oracles of `library_links_support.rs` (§11.1),
//! each of which has a negative control below.

use super::library_links_support::*;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::library_links::{
    MountStatus, is_frozen, lexical_join, normalize_rel_path, unresolved_mount_ref,
};
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Building designs from text
// ---------------------------------------------------------------------------

/// Replaces network `network` (created if absent) with `code`, and validates.
fn edit(d: &mut StructureDesigner, network: &str, code: &str) {
    if !d.node_type_registry.node_networks.contains_key(network) {
        d.add_node_network(network);
    }
    d.set_active_node_network_name(Some(network.to_string()));
    let outcome = d.ai_text_edit(code, true);
    assert!(
        outcome.result.errors.is_empty(),
        "edit of '{}' failed: {:?}\n{}",
        network,
        outcome.result.errors,
        code
    );
    d.validate_active_network();
}

fn int_record(name: &str, fields: &[&str]) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name,
        fields
            .iter()
            .map(|f| (f.to_string(), DataType::Int))
            .collect(),
    )
}

/// A fresh designer whose only network is `Main`, saved at `path`.
fn new_design(path: &Path) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.new_project();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    d.save_node_networks_as(&path.to_string_lossy()).unwrap();
    d
}

fn save(d: &mut StructureDesigner) {
    d.save_node_networks().expect("has a path").expect("save");
}

const FOO: &str = "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
y = parameter { param_name: \"y\", data_type: Int, sort_order: 1 }
s = expr { a: x, b: y, expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
";

fn write_common(dir: &Path) {
    let mut d = new_design(&dir.join("libs/common.cnnd"));
    d.add_record_type_def(int_record("Pt", &["x"])).unwrap();
    edit(
        &mut d,
        "slab",
        "n = parameter { param_name: \"n\", data_type: Int, sort_order: 0 }
d = expr { a: n, expression: \"a * 2\", parameters: [{ name: \"a\", data_type: Int }] }
output d
",
    );
    d.delete_node_network("Main").ok();
    save(&mut d);
}

fn write_lib_a(dir: &Path) {
    let mut d = new_design(&dir.join("lib_a.cnnd"));
    d.link_library("libs/common.cnnd", "common").unwrap();
    d.add_record_type_def(int_record("Miller", &["h", "k", "l"]))
        .unwrap();
    edit(&mut d, "foo", FOO);
    edit(
        &mut d,
        "crystal",
        "c = cuboid { extent: (2, 2, 2) }
m = materialize { shape: c }
output m
",
    );
    edit(
        &mut d,
        "bar",
        "r = range { count: 3 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = foo { x: $element, y: $element } output g } }
output mp
",
    );
    edit(
        &mut d,
        "fv",
        "one = int { value: 1 }
two = int { value: 2 }
f1 = foo {}
ap = apply { f: @f1, arg0: one, arg1: two }
output ap
",
    );
    edit(
        &mut d,
        "usecommon",
        "n = parameter { param_name: \"n\", data_type: Int, sort_order: 0 }
c = `common.slab` { n: n }
output c
",
    );
    edit(
        &mut d,
        "mk",
        "one = int { value: 1 }
rc = record_construct { schema: \"Miller\", h: one, k: one, l: one }
output rc
",
    );
    d.delete_node_network("Main").ok();
    save(&mut d);
}

fn write_lib_b(dir: &Path) {
    let mut d = new_design(&dir.join("libs/lib_b.cnnd"));
    edit(&mut d, "baz", "v = int { value: 7 }\noutput v\n");
    d.delete_node_network("Main").ok();
    save(&mut d);
}

/// The host's main network: an instance, record nodes on a linked def, a
/// reference into the transitive mount, a second library, and a linked
/// instance inside a HOF body.
const HOST_MAIN: &str = "i1 = int { value: 3 }
i2 = int { value: 4 }
f = `a.foo` { x: i1, y: i2 }
rc = record_construct { schema: \"a.Miller\", h: i1, k: i2, l: i1 }
rd = record_destructure { schema: \"a.Miller\", record: rc }
e2 = expr { a: rd.k, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
s = `a.common.slab` { n: i1 }
bz = `b.baz` {}
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = `a.foo` { x: $element, y: ^i1 } output g } }
output f
";

fn write_host(dir: &Path) {
    let mut d = new_design(&dir.join("host.cnnd"));
    d.link_library("lib_a.cnnd", "a").unwrap();
    d.link_library("libs/lib_b.cnnd", "b").unwrap();
    edit(&mut d, "Main", HOST_MAIN);
    // A parameter typed with a linked record: a type-only reference, which
    // the `uses` table does not record.
    edit(
        &mut d,
        "typed",
        "m = parameter { param_name: \"m\", data_type: Record(`a.Miller`), sort_order: 0 }
output m
",
    );
    d.set_active_node_network_name(Some("Main".to_string()));
    save(&mut d);
}

/// `host_missing.cnnd`: built against `missing/demolib.cnnd` in a scratch
/// folder, then copied alone — so in the fixture folder the library is
/// missing. It refers to demolib by every reference kind of §8, with wired
/// pins, several of them inside HOF bodies.
fn write_host_missing(dir: &Path) {
    let scratch = tempfile::tempdir().unwrap();
    let lib = scratch.path().join("missing/demolib.cnnd");
    {
        let mut d = new_design(&lib);
        d.add_record_type_def(int_record("Miller", &["h", "k", "l"]))
            .unwrap();
        edit(&mut d, "foo", FOO);
        edit(
            &mut d,
            "split",
            "m = parameter { param_name: \"m\", data_type: Record(Miller), sort_order: 0 }
d = record_destructure { schema: \"Miller\", record: m }
output d
",
        );
        d.delete_node_network("Main").ok();
        save(&mut d);
    }
    let host = scratch.path().join("host_missing.cnnd");
    let mut d = new_design(&host);
    d.link_library("missing/demolib.cnnd", "demolib").unwrap();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![(
            "m".to_string(),
            DataType::from_string("Record(`demolib.Miller`)").unwrap(),
        )],
    ))
    .unwrap();
    edit(
        &mut d,
        "typed",
        "m = parameter { param_name: \"m\", data_type: Record(`demolib.Miller`), sort_order: 0 }
output m
",
    );
    edit(
        &mut d,
        "Main",
        "i1 = int { value: 3 }
i2 = int { value: 4 }
f = `demolib.foo` { x: i1, y: i2 }
e1 = expr { a: f, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
f2 = `demolib.foo` {}
ap = apply { f: @f2, arg0: i1, arg1: i2 }
rc = record_construct { schema: \"demolib.Miller\", h: i1, k: i2, l: i1 }
rd = record_destructure { schema: \"demolib.Miller\", record: rc }
e2 = expr { a: rd.k, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
e3 = expr { a: rd.l, expression: \"a + 2\", parameters: [{ name: \"a\", data_type: Int }] }
r1 = range { count: 2 }
r2 = range { count: 3 }
pr = product { target: \"demolib.Miller\", h: r1, k: r2, l: r1 }
sp = `demolib.split` { m: rc }
e4 = expr { a: sp.l, expression: \"a * 3\", parameters: [{ name: \"a\", data_type: Int }] }
arr = array { element_type: Record(`demolib.Miller`), elements: [] }
mp = map { xs: pr, input_type: Record(`demolib.Miller`), output_type: Int, body { g = record_destructure { schema: \"demolib.Miller\", record: $element } h = `demolib.foo` { x: g.h, y: g.k } output h } }
bx = record_construct { schema: \"Box\", m: rc }
t = typed { m: rc }
cl = closure { kind: \"custom\", params: [\"m\"], type_args: [Record(`demolib.Miller`), Int], body { p = int { value: 1 } output p } }
output e1
",
    );
    save(&mut d);
    std::fs::copy(&host, dir.join("host_missing.cnnd")).unwrap();
}

fn write_cycles(dir: &Path) {
    // `cycle_y` first, unlinked, so `cycle_x` can link it; then `cycle_y`
    // links `cycle_x` (whose own link back to `cycle_y` is then a `Cycle`).
    let mut y = new_design(&dir.join("cycle_y.cnnd"));
    edit(&mut y, "why", "v = int { value: 2 }\noutput v\n");
    save(&mut y);
    let mut x = new_design(&dir.join("cycle_x.cnnd"));
    edit(&mut x, "ex", "v = int { value: 1 }\noutput v\n");
    x.link_library("cycle_y.cnnd", "y").unwrap();
    save(&mut x);
    y.link_library("cycle_x.cnnd", "x").unwrap();
    save(&mut y);

    // A file that links itself cannot be made through `link_library` (it
    // refuses the cycle), so its import entry is written by hand.
    let path = dir.join("self_link.cnnd");
    let mut s = new_design(&path);
    edit(&mut s, "Main", "v = int { value: 5 }\noutput v\n");
    save(&mut s);
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["imports"] = serde_json::json!([{ "alias": "me", "path": "self_link.cnnd" }]);
    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
}

fn write_with_xyz(dir: &Path) {
    std::fs::write(
        dir.join("libs/tip.xyz"),
        "2\ntip\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\n",
    )
    .unwrap();
    let mut d = new_design(&dir.join("libs/with_xyz.cnnd"));
    edit(
        &mut d,
        "tip",
        "t = import_xyz { file_name: \"tip.xyz\" }\noutput t\n",
    );
    d.delete_node_network("Main").ok();
    save(&mut d);
}

/// Regenerates `rust/tests/fixtures/library_linking/`. Run once with
/// `cargo test --test structure_designer generate_library_linking_fixtures -- --ignored`
/// and commit the result (the `generate_healthy_fixture` precedent). A tool,
/// not a test — it is ignored because it writes to the source tree.
#[test]
#[ignore]
fn generate_library_linking_fixtures() {
    let dir = fixture_path("library_linking");
    // Wipe everything but `.gitattributes`, which keeps the fixtures'
    // line endings byte-exact on checkout.
    let _ = std::fs::remove_dir_all(dir.join("libs"));
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.file_name() != ".gitattributes" {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    std::fs::create_dir_all(dir.join("libs")).unwrap();
    write_common(&dir);
    write_lib_a(&dir);
    write_lib_b(&dir);
    write_host(&dir);
    write_host_missing(&dir);
    write_cycles(&dir);
    write_with_xyz(&dir);
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// The top-level node of `network` named `name`.
fn node_id(d: &StructureDesigner, network: &str, name: &str) -> u64 {
    d.node_type_registry.node_networks[network]
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}' in '{}'", name, network))
        .id
}

fn status(d: &StructureDesigner, mount_path: &str) -> Option<MountStatus> {
    d.node_type_registry
        .library_links
        .get(mount_path)
        .map(|m| m.status.clone())
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&read(path)).unwrap()
}

fn expect_atoms(
    result: atomcad_structure_designer::evaluator::network_result::NetworkResult,
) -> atomcad_crystolecule::atomic_structure::AtomicStructure {
    use atomcad_structure_designer::evaluator::network_result::NetworkResult;
    match result {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        other => panic!("expected atoms, got {}", other.to_display_string()),
    }
}

/// The three ways a mount can fail to resolve in Phase 1, each prepared in a
/// fresh copy of the fixture folder around `host_missing.cnnd`.
#[derive(Clone, Copy, Debug)]
enum Cause {
    Missing,
    Error,
    Cycle,
}

const CAUSES: [Cause; 3] = [Cause::Missing, Cause::Error, Cause::Cycle];

/// A workspace whose `host_missing.cnnd` cannot resolve `demolib` for
/// `cause`. Returns the workspace and the host path.
fn broken_host(cause: Cause) -> (tempfile::TempDir, PathBuf) {
    let ws = fixture_workspace();
    let host = ws.path().join("host_missing.cnnd");
    match cause {
        Cause::Missing => {}
        Cause::Error => {
            std::fs::create_dir_all(ws.path().join("missing")).unwrap();
            std::fs::write(ws.path().join("missing/demolib.cnnd"), "this is not json").unwrap();
        }
        Cause::Cycle => {
            // Point the import at the host itself: the file is on the stack.
            let text = read(&host).replace(
                "\"path\": \"missing/demolib.cnnd\"",
                "\"path\": \"host_missing.cnnd\"",
            );
            assert!(text.contains("\"path\": \"host_missing.cnnd\""));
            std::fs::write(&host, text).unwrap();
        }
    }
    (ws, host)
}

fn expected_status(cause: Cause) -> fn(&MountStatus) -> bool {
    match cause {
        Cause::Missing => |s| *s == MountStatus::Missing,
        Cause::Error => |s| matches!(s, MountStatus::Error(_)),
        Cause::Cycle => |s| *s == MountStatus::Cycle,
    }
}

/// Wires into or out of a frozen node, or inside a frozen HOF's body — the
/// wires §8 protects.
fn frozen_wires(d: &StructureDesigner) -> std::collections::BTreeSet<WireEntry> {
    let registry = &d.node_type_registry;
    let mut frozen_ids: std::collections::BTreeSet<(String, Vec<u64>, u64)> = Default::default();
    fn walk(
        registry: &atomcad_structure_designer::node_type_registry::NodeTypeRegistry,
        name: &str,
        net: &atomcad_structure_designer::node_network::NodeNetwork,
        scope: &[u64],
        out: &mut std::collections::BTreeSet<(String, Vec<u64>, u64)>,
    ) {
        for n in net.nodes.values() {
            if is_frozen(n, registry) {
                out.insert((name.to_string(), scope.to_vec(), n.id));
            }
            if let Some(body) = n.zone.as_deref() {
                let mut child = scope.to_vec();
                child.push(n.id);
                walk(registry, name, body, &child, out);
            }
        }
    }
    for (name, net) in &registry.node_networks {
        if registry.library_links.mount_containing(name).is_none() {
            walk(registry, name, net, &[], &mut frozen_ids);
        }
    }
    wire_ledger(d)
        .into_iter()
        .filter(|w| {
            frozen_ids.contains(&(w.network.clone(), w.scope.clone(), w.dest))
                || (w.depth == 0
                    && frozen_ids.contains(&(w.network.clone(), w.scope.clone(), w.source)))
                || frozen_ids.iter().any(|(n, s, id)| {
                    *n == w.network && w.scope.starts_with(&[s.as_slice(), &[*id]].concat())
                })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Negative controls: every oracle can fail (§11.1)
// ---------------------------------------------------------------------------

#[test]
fn oracle_o1_fingerprint_sees_a_moved_wire() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    let before = local_fingerprint(&mut d);
    // Move f's wire from `x` to `y` by hand.
    let f = node_id(&d, "Main", "f");
    let node = d
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&f)
        .unwrap();
    node.arguments.swap(0, 1);
    assert_ne!(before, local_fingerprint(&mut d));
}

#[test]
fn oracle_o2_ledger_catches_a_silent_drop_and_a_moved_wire() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    let before = wire_ledger(&d);
    assert!(!before.is_empty());
    let f = node_id(&d, "Main", "f");
    let args = &mut d
        .node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&f)
        .unwrap()
        .arguments;
    args[1].incoming_wires.clear();
    let after_drop = wire_ledger(&d);
    assert!(check_wire_ledger(&before, &after_drop, &Default::default()).is_err());
    // …but a drop that the operation reported is fine.
    let reported: std::collections::BTreeSet<WireEntry> =
        before.difference(&after_drop).cloned().collect();
    ok(check_wire_ledger(&before, &after_drop, &reported));

    let mut d = open(&ws.path().join("host.cnnd"));
    let f = node_id(&d, "Main", "f");
    d.node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&f)
        .unwrap()
        .arguments
        .swap(0, 1);
    let after_move = wire_ledger(&d);
    assert!(check_wire_ledger(&before, &after_move, &Default::default()).is_err());
}

#[test]
fn oracle_o3_tripwire_catches_a_write_and_a_delete() {
    let ws = fixture_workspace();
    let wire = DiskTripwire::arm(ws.path());
    ok(wire.check_only_changed(&[]));
    let lib = ws.path().join("lib_a.cnnd");
    std::fs::write(&lib, "changed").unwrap();
    assert!(wire.check_only_changed(&[]).is_err());
    ok(wire.check_only_changed(std::slice::from_ref(&lib)));
    std::fs::remove_file(ws.path().join("libs/lib_b.cnnd")).unwrap();
    assert!(wire.check_only_changed(&[lib]).is_err());
}

#[test]
fn oracle_o4_catches_state_that_does_not_survive_a_reopen() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    ok(check_save_reopen_identity(&mut d));
    // A slash in a node name is healed on load, so this state cannot survive.
    let f = node_id(&d, "Main", "f");
    d.node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&f)
        .unwrap()
        .custom_name = Some("f/x".to_string());
    assert!(check_save_reopen_identity(&mut d).is_err());
}

#[test]
fn oracle_o5_catches_an_undo_that_is_not_the_inverse() {
    use atomcad_structure_designer::undo::{UndoCommand, UndoContext, UndoRefreshMode};
    #[derive(Debug)]
    struct Nothing;
    impl UndoCommand for Nothing {
        fn description(&self) -> &str {
            "nothing"
        }
        fn undo(&self, _: &mut UndoContext) {}
        fn redo(&self, _: &mut UndoContext) {}
        fn refresh_mode(&self) -> UndoRefreshMode {
            UndoRefreshMode::Lightweight
        }
    }
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    let result = check_undo_inverse(&mut d, |d| {
        let f = node_id(d, "Main", "f");
        d.node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&f)
            .unwrap()
            .arguments
            .swap(0, 1);
        d.push_command(Nothing);
    });
    assert!(result.is_err());
}

#[test]
fn oracle_o6_catches_a_misaligned_live_node() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    ok(check_invariants(&d));
    let e2 = node_id(&d, "Main", "e2");
    d.node_type_registry
        .node_networks
        .get_mut("Main")
        .unwrap()
        .nodes
        .get_mut(&e2)
        .unwrap()
        .arguments
        .clear();
    assert!(check_invariants(&d).is_err());
}

// ---------------------------------------------------------------------------
// §8 — a missing library is lossless (red-first)
// ---------------------------------------------------------------------------

#[test]
fn missing_library_host_loads_and_saves_byte_identically() {
    let ws = fixture_workspace();
    let host = ws.path().join("host_missing.cnnd");
    let original = read(&host);
    let wire = DiskTripwire::arm(ws.path());
    let mut d = open(&host);
    assert_eq!(status(&d, "demolib"), Some(MountStatus::Missing));

    // Every reference kind is frozen and keeps its wires.
    for name in ["f", "f2", "rc", "rd", "pr", "sp", "mp", "cl", "arr"] {
        let id = node_id(&d, "Main", name);
        let node = &d.node_type_registry.node_networks["Main"].nodes[&id];
        assert!(
            is_frozen(node, &d.node_type_registry),
            "'{}' should be frozen",
            name
        );
    }
    let rc = node_id(&d, "Main", "rc");
    let rc_node = &d.node_type_registry.node_networks["Main"].nodes[&rc];
    assert_eq!(rc_node.arguments.len(), 3, "record_construct kept its pins");
    assert!(
        rc_node
            .arguments
            .iter()
            .all(|a| !a.incoming_wires.is_empty())
    );
    // An instance of a *local* network whose parameter is typed with a linked
    // record is not frozen (§8 looks at the node's own references), but the
    // wire into it survives: no repair pass drops a wire over a type check.
    let t = node_id(&d, "Main", "t");
    let t_node = &d.node_type_registry.node_networks["Main"].nodes[&t];
    assert!(!is_frozen(t_node, &d.node_type_registry));
    assert_eq!(t_node.arguments[0].incoming_wires.len(), 1);
    // The `apply` fed by a frozen node's function pin keeps its arg wires.
    let ap = node_id(&d, "Main", "ap");
    let ap_node = &d.node_type_registry.node_networks["Main"].nodes[&ap];
    assert_eq!(ap_node.arguments.len(), 3);
    assert!(
        ap_node
            .arguments
            .iter()
            .all(|a| a.incoming_wires.len() == 1)
    );
    // The record node inside the map body kept its `$element` wire.
    let mp = node_id(&d, "Main", "mp");
    let body = d.node_type_registry.node_networks["Main"].nodes[&mp]
        .zone
        .clone()
        .unwrap();
    let g = body
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("g"))
        .unwrap();
    assert_eq!(g.arguments[0].incoming_wires.len(), 1);
    ok(check_invariants(&d));

    // It shows the frozen error.
    let errors: Vec<String> = d.node_type_registry.node_networks["Main"]
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(rc))
        .map(|e| e.error_text.clone())
        .collect();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("Unknown record type `demolib.Miller`")),
        "{:?}",
        errors
    );

    save(&mut d);
    assert_eq!(read(&host), original, "load → save must be byte-identical");
    ok(wire.check_only_changed(std::slice::from_ref(&host)));
    ok(check_save_reopen_identity(&mut d));
}

#[test]
fn every_unresolvable_status_saves_byte_identically() {
    for cause in CAUSES {
        let (_ws, host) = broken_host(cause);
        let original = read(&host);
        let mut d = open(&host);
        let s = status(&d, "demolib").unwrap();
        assert!(expected_status(cause)(&s), "{:?}: got {:?}", cause, s);
        assert!(!frozen_wires(&d).is_empty(), "{:?}: nothing frozen", cause);
        save(&mut d);
        assert_eq!(read(&host), original, "{:?}: not byte-identical", cause);
    }
}

// ---------------------------------------------------------------------------
// The frozen-node predicate
// ---------------------------------------------------------------------------

#[test]
fn unresolved_mount_ref_names_each_reference_kind_for_every_status() {
    for cause in CAUSES {
        let (_ws, host) = broken_host(cause);
        let d = open(&host);
        let reg = &d.node_type_registry;
        let main = &reg.node_networks["Main"];
        let of = |name: &str| unresolved_mount_ref(&main.nodes[&node_id(&d, "Main", name)], reg);
        // node_type_name
        assert_eq!(of("f"), Some("demolib.foo".to_string()), "{:?}", cause);
        // record schema / target
        assert_eq!(of("rc"), Some("demolib.Miller".to_string()));
        assert_eq!(of("pr"), Some("demolib.Miller".to_string()));
        // a DataType in node data (HOF element type, closure signature,
        // array element type, parameter type)
        assert_eq!(of("mp"), Some("demolib.Miller".to_string()));
        assert_eq!(of("cl"), Some("demolib.Miller".to_string()));
        assert_eq!(of("arr"), Some("demolib.Miller".to_string()));
        let typed = &reg.node_networks["typed"];
        let m = typed.nodes.values().next().unwrap();
        assert_eq!(
            unresolved_mount_ref(m, reg),
            Some("demolib.Miller".to_string())
        );
        // A node referring to nothing linked is not frozen.
        assert_eq!(of("i1"), None);
        assert_eq!(of("e1"), None);
    }
}

#[test]
fn unknown_name_outside_every_mount_keeps_todays_behaviour() {
    // The same references, renamed to a prefix that is not a mount: none of
    // the §8 exemptions applies, so every repair pass runs its pre-linking
    // code path. (Checked at the serialization layer: today's validator does
    // not report an unknown record *schema*, so the Phase 0 debug assertion
    // fires on such a file once validated — a pre-existing gap.)
    use atomcad_structure_designer::library_links::{
        data_type_mentions_unresolved_mount, protected_node_ids,
    };
    use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
    use atomcad_structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
    let ws = fixture_workspace();
    let host = ws.path().join("host_missing.cnnd");
    let text = read(&host)
        .replace("`demolib.", "`nowhere.")
        .replace("\"demolib.", "\"nowhere.");
    std::fs::write(&host, text).unwrap();
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &host.to_string_lossy()).unwrap();
    assert_eq!(
        registry.library_links.get("demolib").unwrap().status,
        MountStatus::Missing
    );
    let main = &registry.node_networks["Main"];
    for node in main.nodes.values() {
        assert!(
            unresolved_mount_ref(node, &registry).is_none(),
            "{:?}",
            node.custom_name
        );
    }
    assert!(protected_node_ids(main, &registry).is_empty());
    let t = DataType::from_string("Record(`nowhere.Miller`)").unwrap();
    assert!(!data_type_mentions_unresolved_mount(&t, &registry));
    let t = DataType::from_string("Record(`demolib.Miller`)").unwrap();
    assert!(data_type_mentions_unresolved_mount(&t, &registry));
}

// ---------------------------------------------------------------------------
// The frozen-node matrix (§11.1): causes × passes, O2 + O6 in every cell
// ---------------------------------------------------------------------------

/// The Phase 1 passes that could touch a frozen node. Each returns the wires
/// it added itself (they are not a loss), and must not disturb any other.
/// A named edit applied to the designer.
type Pass = (&'static str, fn(&mut StructureDesigner));

fn matrix_passes() -> Vec<Pass> {
    vec![
        ("validate", |d| {
            for n in ["Main", "typed"] {
                d.set_active_node_network_name(Some(n.to_string()));
                d.validate_active_network();
            }
        }),
        ("add node", |d| {
            d.set_active_node_network_name(Some("Main".to_string()));
            d.add_node("int", glam::DVec2::new(900.0, 900.0));
        }),
        ("move node", |d| {
            d.set_active_node_network_name(Some("Main".to_string()));
            let rc = node_id(d, "Main", "rc");
            d.select_node(rc);
            d.begin_move_nodes();
            d.move_selected_nodes(glam::DVec2::new(15.0, 5.0));
            d.end_move_nodes();
        }),
        ("delete a neighbour", |d| {
            d.set_active_node_network_name(Some("Main".to_string()));
            let extra = d.add_node("int", glam::DVec2::new(950.0, 950.0));
            d.select_node(extra);
            d.delete_selected();
        }),
        ("connect an unrelated wire", |d| {
            d.set_active_node_network_name(Some("Main".to_string()));
            let src = d.add_node("int", glam::DVec2::new(900.0, 1000.0));
            let dst = d.add_node("int", glam::DVec2::new(1100.0, 1000.0));
            let _ = dst;
            // `e1`'s own pin stays wired; wire a fresh expr instead.
            let e = d.add_node("expr", glam::DVec2::new(1200.0, 1000.0));
            d.connect_nodes(src, 0, e, 0);
        }),
        ("add a local record def", |d| {
            d.add_record_type_def(int_record("Other", &["q"])).unwrap();
        }),
        ("update and rename a local record def", |d| {
            d.add_record_type_def(int_record("Other", &["q"])).unwrap();
            d.update_record_type_def(
                "Other",
                vec![
                    ("q".to_string(), DataType::Int),
                    ("w".to_string(), DataType::Int),
                ],
            )
            .unwrap();
            d.rename_record_type_def("Other", "Other2").unwrap();
        }),
    ]
}

#[test]
fn frozen_matrix_no_pass_touches_a_frozen_node() {
    for cause in CAUSES {
        for (pass, op) in matrix_passes() {
            let (_ws, host) = broken_host(cause);
            let mut d = open(&host);
            let before = frozen_wires(&d);
            assert!(before.len() >= 10, "{:?}: frozen wires present", cause);
            let ctx = format!("{:?} × {}", cause, pass);

            op(&mut d);
            let after = frozen_wires(&d);
            if let Err(e) = check_wire_ledger(&before, &after, &Default::default()) {
                panic!("{}: {}", ctx, e);
            }
            if let Err(e) = check_invariants(&d) {
                panic!("{}: {}", ctx, e);
            }

            // Undo and redo of the pass (when it pushed anything).
            while d.undo_stack.can_undo() {
                d.undo();
            }
            if let Err(e) = check_wire_ledger(&before, &frozen_wires(&d), &Default::default()) {
                panic!("{} (after undo): {}", ctx, e);
            }
            while d.undo_stack.can_redo() {
                d.redo();
            }
            if let Err(e) = check_wire_ledger(&before, &frozen_wires(&d), &Default::default()) {
                panic!("{} (after redo): {}", ctx, e);
            }
            if let Err(e) = check_invariants(&d) {
                panic!("{} (after redo): {}", ctx, e);
            }

            // Save → reopen.
            if let Err(e) = check_save_reopen_identity(&mut d) {
                panic!("{} (save → reopen): {}", ctx, e);
            }
        }
    }
}

#[test]
fn wires_out_of_a_frozen_record_destructure_survive() {
    for cause in CAUSES {
        let (_ws, host) = broken_host(cause);
        let d = open(&host);
        let rd = node_id(&d, "Main", "rd");
        let main = &d.node_type_registry.node_networks["Main"];
        let pins: Vec<i32> = main
            .nodes
            .values()
            .flat_map(|n| n.arguments.iter())
            .flat_map(|a| a.incoming_wires.iter())
            .filter(|w| w.source_node_id == rd)
            .filter_map(|w| match w.source_pin {
                atomcad_structure_designer::node_network::SourcePin::NodeOutput { pin_index } => {
                    Some(pin_index)
                }
                _ => None,
            })
            .collect();
        assert!(
            pins.contains(&1) && pins.contains(&2),
            "{:?}: {:?}",
            cause,
            pins
        );
        // And out of the multi-output linked network instance.
        let sp = node_id(&d, "Main", "sp");
        assert!(
            main.nodes
                .values()
                .flat_map(|n| n.arguments.iter())
                .flat_map(|a| a.incoming_wires.iter())
                .any(|w| w.source_node_id == sp)
        );
    }
}

#[test]
fn body_wires_of_a_map_over_a_linked_record_survive() {
    for cause in CAUSES {
        let (_ws, host) = broken_host(cause);
        let d = open(&host);
        let mp = node_id(&d, "Main", "mp");
        let hof = &d.node_type_registry.node_networks["Main"].nodes[&mp];
        let body = hof.zone.as_deref().unwrap();
        let element_wires = body
            .nodes
            .values()
            .flat_map(|n| n.arguments.iter())
            .flat_map(|a| a.incoming_wires.iter())
            .filter(|w| {
                matches!(
                    w.source_pin,
                    atomcad_structure_designer::node_network::SourcePin::ZoneInput { .. }
                )
            })
            .count();
        assert_eq!(element_wires, 1, "{:?}", cause);
        assert_eq!(hof.zone_output_arguments[0].incoming_wires.len(), 1);
        // The linked instance inside the body kept both its wires.
        let h = body
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some("h"))
            .unwrap();
        assert!(h.arguments.iter().all(|a| a.incoming_wires.len() == 1));
    }
}

// ---------------------------------------------------------------------------
// Mounting: prefixing, transitive links, cycles, eval equivalence
// ---------------------------------------------------------------------------

#[test]
fn mount_prefixes_networks_records_folders_and_every_reference() {
    let ws = fixture_workspace();
    // Give lib_a an empty folder marker to see it prefixed too.
    let lib = ws.path().join("lib_a.cnnd");
    let mut l = open(&lib);
    l.add_folder("empty_dir").unwrap();
    save(&mut l);

    let d = open(&ws.path().join("host.cnnd"));
    let reg = &d.node_type_registry;
    for n in [
        "a.foo",
        "a.crystal",
        "a.bar",
        "a.fv",
        "a.usecommon",
        "a.mk",
        "a.common.slab",
        "b.baz",
    ] {
        assert!(reg.node_networks.contains_key(n), "{} mounted", n);
        assert_eq!(reg.node_networks[n].node_type.name, n);
    }
    for n in ["foo", "bar", "slab", "baz", "common.slab"] {
        assert!(
            !reg.node_networks.contains_key(n),
            "{} must not leak unprefixed",
            n
        );
    }
    assert!(reg.record_type_defs.contains_key("a.Miller"));
    assert_eq!(reg.record_type_defs["a.Miller"].name, "a.Miller");
    assert!(reg.record_type_defs.contains_key("a.common.Pt"));
    assert!(reg.folders.contains("a.empty_dir"));

    // References inside a zone body (bar's map calls foo).
    let bar = &reg.node_networks["a.bar"];
    let mut body_refs = Vec::new();
    atomcad_structure_designer::node_network::walk_all_nodes(bar, &mut |n| {
        body_refs.push(n.node_type_name.clone())
    });
    assert!(body_refs.contains(&"a.foo".to_string()), "{:?}", body_refs);
    // A reference into the nested mount.
    let usecommon = &reg.node_networks["a.usecommon"];
    assert!(
        usecommon
            .nodes
            .values()
            .any(|n| n.node_type_name == "a.common.slab")
    );
    // Schema strings.
    let mk = &reg.node_networks["a.mk"];
    let rc = mk
        .nodes
        .values()
        .find(|n| n.node_type_name == "record_construct")
        .unwrap();
    let data = rc
        .data
        .as_any_ref()
        .downcast_ref::<atomcad_structure_designer::nodes::record_construct::RecordConstructData>()
        .unwrap();
    assert_eq!(data.schema, "a.Miller");
    // A `Named` record inside a network signature.
    assert_eq!(
        reg.node_networks["a.mk"].node_type.output_pins[0]
            .data_type
            .to_string(),
        "Record(`a.Miller`)"
    );
    // Linked content validates cleanly.
    for (name, net) in &reg.node_networks {
        if name.starts_with("a.") || name.starts_with("b.") {
            let errs: Vec<&String> = net
                .validation_errors
                .iter()
                .map(|e| &e.error_text)
                .collect();
            assert!(errs.is_empty(), "{}: {:?}", name, errs);
        }
    }
    ok(check_invariants(&d));
}

#[test]
fn transitive_mounts_nest_and_only_direct_links_are_saved() {
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let mut d = open(&host);
    let links = &d.node_type_registry.library_links;
    assert_eq!(links.get("a").unwrap().parent, None);
    assert_eq!(links.get("b").unwrap().parent, None);
    let common = links.get("a.common").unwrap();
    assert_eq!(common.parent.as_deref(), Some("a"));
    assert_eq!(common.rel_path, "libs/common.cnnd");
    assert_eq!(common.status, MountStatus::Loaded);
    // The nested mount keeps lib_a's own `uses` of common.
    assert!(common.stored_uses.networks.contains_key("slab"));

    let text = saved_text(&mut d, Some(ws.path()));
    let j: serde_json::Value = serde_json::from_str(&text).unwrap();
    let aliases: Vec<&str> = j["imports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["alias"].as_str().unwrap())
        .collect();
    assert_eq!(aliases, vec!["a", "b"]);
}

#[test]
fn saved_host_contains_no_linked_content_and_round_trips() {
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let mut d = open(&host);
    let text = saved_text(&mut d, Some(ws.path()));
    let j: serde_json::Value = serde_json::from_str(&text).unwrap();
    let nets: Vec<&str> = j["node_networks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n[0].as_str().unwrap())
        .collect();
    assert_eq!(nets, vec!["Main", "typed"]);
    assert!(j.get("record_type_defs").is_none());
    assert!(j.get("folders").is_none());
    for import in j["imports"].as_array().unwrap() {
        let path = import["path"].as_str().unwrap();
        assert!(
            !path.contains('\\') && !Path::new(path).is_absolute(),
            "{}",
            path
        );
    }
    assert_eq!(j["version"], 9);
    ok(check_save_reopen_identity(&mut d));
    ok(check_invariants(&d));
}

#[test]
fn a_cycle_and_a_self_link_load_with_the_offending_mount_marked() {
    let ws = fixture_workspace();
    let d = open(&ws.path().join("cycle_x.cnnd"));
    assert_eq!(status(&d, "y"), Some(MountStatus::Loaded));
    assert_eq!(status(&d, "y.x"), Some(MountStatus::Cycle));
    assert!(d.node_type_registry.node_networks.contains_key("y.why"));
    assert!(d.node_type_registry.node_networks.contains_key("ex"));

    let path = ws.path().join("self_link.cnnd");
    let original = read(&path);
    let mut d = open(&path);
    assert_eq!(status(&d, "me"), Some(MountStatus::Cycle));
    assert!(d.node_type_registry.node_networks.contains_key("Main"));
    assert_eq!(d.node_type_registry.node_networks.len(), 1);
    // Saving writes the import back and nothing else changes. (The fixture
    // was hand-edited, so compare after one canonical save.)
    save(&mut d);
    let canonical = read(&path);
    let mut again = open(&path);
    save(&mut again);
    assert_eq!(read(&path), canonical);
    assert_ne!(original.len(), 0);
}

#[test]
fn a_linked_instance_evaluates_like_the_library_opened_on_its_own() {
    let ws = fixture_workspace();
    let mut host = open(&ws.path().join("host.cnnd"));
    edit(
        &mut host,
        "evalcheck",
        "c = `a.crystal` {}
one = int { value: 1 }
two = int { value: 2 }
f = `a.foo` { x: one, y: two }
u = `a.usecommon` { n: two }
output c
",
    );
    let c = node_id(&host, "evalcheck", "c");
    let f = node_id(&host, "evalcheck", "f");
    let u = node_id(&host, "evalcheck", "u");
    let host_atoms = expect_atoms(host.evaluate_node_output(&[], c, 0));
    let host_sum = host.evaluate_node_output(&[], f, 0).to_display_string();
    let host_common = host.evaluate_node_output(&[], u, 0).to_display_string();

    let mut lib = open(&ws.path().join("lib_a.cnnd"));
    lib.set_active_node_network_name(Some("crystal".to_string()));
    let ret = lib.node_type_registry.node_networks["crystal"]
        .return_node_id
        .unwrap();
    let lib_atoms = expect_atoms(lib.evaluate_node_output(&[], ret, 0));
    assert!(host_atoms.get_num_of_atoms() > 0);
    atomcad_test_support::assert_structures_equivalent(&host_atoms, &lib_atoms, 1e-9);
    assert_eq!(host_sum, "3");
    assert_eq!(host_common, "4");
}

// ---------------------------------------------------------------------------
// Save filtering never drops local content (D5)
// ---------------------------------------------------------------------------

#[test]
fn save_keeps_every_local_name_that_only_resembles_a_mount() {
    let ws = fixture_workspace();
    let path = ws.path().join("filter.cnnd");
    let mut d = new_design(&path);
    d.link_library("lib_a.cnnd", "libs.demolib").unwrap();
    edit(&mut d, "libs.local_net", "v = int { value: 1 }\noutput v\n");
    edit(&mut d, "libs.demolibx", "v = int { value: 2 }\noutput v\n");
    edit(&mut d, "demolib_extra", "v = int { value: 3 }\noutput v\n");
    d.add_record_type_def(int_record("libs.LocalRec", &["q"]))
        .unwrap();
    d.add_record_type_def(int_record("libs.demolib_rec", &["q"]))
        .unwrap();
    d.set_cli_access("libs.local_net", false);
    d.set_cli_access("libs.demolib.foo", false);
    save(&mut d);

    let j = json(&path);
    let nets: Vec<&str> = j["node_networks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n[0].as_str().unwrap())
        .collect();
    for n in ["libs.local_net", "libs.demolibx", "demolib_extra", "Main"] {
        assert!(nets.contains(&n), "{} dropped: {:?}", n, nets);
    }
    assert!(!nets.iter().any(|n| n.starts_with("libs.demolib.")));
    let defs: Vec<&str> = j["record_type_defs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(defs, vec!["libs.LocalRec", "libs.demolib_rec"]);
    let rules = j["cli_access_rules"].as_object().unwrap();
    assert!(rules.contains_key("libs.local_net"));
    assert!(!rules.contains_key("libs.demolib.foo"));
    ok(check_save_reopen_identity(&mut d));
}

// ---------------------------------------------------------------------------
// The `uses` table (D13)
// ---------------------------------------------------------------------------

#[test]
fn uses_lists_exactly_the_referenced_names_with_their_interfaces() {
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let mut d = open(&host);
    // A reference to a record purely as a type is not recorded.
    edit(
        &mut d,
        "typeonly",
        "m = parameter { param_name: \"m\", data_type: Record(`a.common.Pt`), sort_order: 0 }
output m
",
    );
    let j: serde_json::Value = serde_json::from_str(&saved_text(&mut d, Some(ws.path()))).unwrap();
    let a = &j["imports"][0];
    assert_eq!(a["alias"], "a");
    let nets: Vec<&String> = a["uses"]["networks"].as_object().unwrap().keys().collect();
    // Sorted, relative to the alias, including a HOF-body reference (`foo` is
    // also called inside `mp`) and a reference into the nested mount.
    assert_eq!(nets, vec!["common.slab", "foo"]);
    let recs: Vec<&String> = a["uses"]["records"].as_object().unwrap().keys().collect();
    assert_eq!(
        recs,
        vec!["Miller"],
        "type-only references are not recorded"
    );
    let foo = &a["uses"]["networks"]["foo"];
    assert_eq!(foo["params"][0]["name"], "x");
    assert!(foo["params"][0]["id"].as_u64().is_some());
    assert_eq!(foo["params"][1]["name"], "y");
    assert_eq!(foo["outputs"][0]["type"], "Int");
    let miller = &a["uses"]["records"]["Miller"]["fields"];
    assert_eq!(miller.as_array().unwrap().len(), 3);
    assert_eq!(miller[2]["name"], "l");
    assert!(miller[2]["id"].as_u64().is_some());

    // `b` is referenced once; a link nothing refers to writes no `uses`.
    let mut unused = new_design(&ws.path().join("unused.cnnd"));
    unused.link_library("libs/lib_b.cnnd", "b").unwrap();
    let j: serde_json::Value =
        serde_json::from_str(&saved_text(&mut unused, Some(ws.path()))).unwrap();
    assert!(j["imports"][0].get("uses").is_none());
    assert!(j["imports"][0]["hash"].as_str().unwrap().starts_with("b3:"));

    // A library saved on its own records its own `uses`.
    let lib = json(&ws.path().join("lib_a.cnnd"));
    assert!(lib["imports"][0]["uses"]["networks"]["slab"].is_object());
}

// ---------------------------------------------------------------------------
// Paths (D6)
// ---------------------------------------------------------------------------

#[test]
fn library_paths_are_normalized_at_link_time_and_never_rewritten() {
    assert_eq!(normalize_rel_path("libs/../x.cnnd").unwrap(), "x.cnnd");
    assert_eq!(normalize_rel_path("./a/./b.cnnd").unwrap(), "a/b.cnnd");
    assert_eq!(
        normalize_rel_path("..\\libs\\x.cnnd").unwrap(),
        "../libs/x.cnnd"
    );
    assert!(normalize_rel_path("/abs/x.cnnd").is_err());
    assert!(normalize_rel_path("C:/abs/x.cnnd").is_err());
    let root = if cfg!(windows) { "C:\\" } else { "/" };
    assert!(
        lexical_join(Path::new(root), "../x.cnnd").is_err(),
        "climbs above the root"
    );

    // `../libs/x.cnnd` survives load → save → Save As unchanged.
    let ws = tempfile::tempdir().unwrap();
    let proj = ws.path().join("proj1");
    copy_dir(
        &fixture_path("library_linking/libs"),
        &ws.path().join("libs"),
    );
    let mut d = new_design(&proj.join("host.cnnd"));
    d.link_library("../libs/lib_b.cnnd", "b").unwrap();
    save(&mut d);
    let mut d = open(&proj.join("host.cnnd"));
    save(&mut d);
    assert_eq!(
        json(&proj.join("host.cnnd"))["imports"][0]["path"],
        "../libs/lib_b.cnnd"
    );
    d.save_node_networks_as(&proj.join("copy.cnnd").to_string_lossy())
        .unwrap();
    assert_eq!(
        json(&proj.join("copy.cnnd"))["imports"][0]["path"],
        "../libs/lib_b.cnnd"
    );

    // Linking by an absolute path on the same root stores it relative.
    let mut e = new_design(&proj.join("abs.cnnd"));
    let abs = std::fs::canonicalize(ws.path().join("libs/lib_b.cnnd")).unwrap();
    e.link_library(&abs.to_string_lossy(), "b").unwrap();
    assert_eq!(
        e.node_type_registry
            .library_links
            .get("b")
            .unwrap()
            .rel_path,
        "../libs/lib_b.cnnd"
    );
    // A normalized path is stored normalized.
    let mut f = new_design(&proj.join("norm.cnnd"));
    f.link_library("../libs/../libs/lib_b.cnnd", "b").unwrap();
    assert_eq!(
        f.node_type_registry
            .library_links
            .get("b")
            .unwrap()
            .rel_path,
        "../libs/lib_b.cnnd"
    );
}

#[test]
fn no_relative_path_exists_across_roots() {
    use atomcad_structure_designer::library_links::relative_path;
    if cfg!(windows) {
        assert_eq!(
            relative_path(Path::new("C:\\a\\b"), Path::new("D:\\x\\y.cnnd")),
            None
        );
        assert_eq!(
            relative_path(Path::new("C:\\a\\b"), Path::new("C:\\a\\x\\y.cnnd")).as_deref(),
            Some("../x/y.cnnd")
        );
    } else {
        assert_eq!(
            relative_path(Path::new("/a/b"), Path::new("/a/x/y.cnnd")).as_deref(),
            Some("../x/y.cnnd")
        );
    }
}

// ---------------------------------------------------------------------------
// Link / unlink (D2, D3, D9)
// ---------------------------------------------------------------------------

#[test]
fn a_failed_link_changes_nothing() {
    let ws = fixture_workspace();
    let path = ws.path().join("linker.cnnd");
    let mut d = new_design(&path);
    edit(&mut d, "taken", "v = int { value: 1 }\noutput v\n");
    d.add_record_type_def(int_record("TakenRec", &["q"]))
        .unwrap();
    std::fs::write(ws.path().join("notes.txt"), "hello").unwrap();
    std::fs::write(ws.path().join("broken.cnnd"), "{ not json").unwrap();
    let before = local_fingerprint(&mut d);
    let history = d.undo_stack.history_len();
    let wire = DiskTripwire::arm(ws.path());

    let attempts: Vec<(&str, &str)> = vec![
        ("lib_a.cnnd", "taken"),     // alias taken by a local network
        ("lib_a.cnnd", "TakenRec"),  // … by a local record def
        ("lib_a.cnnd", "cuboid"),    // … by a built-in node type
        ("lib_a.cnnd", "taken.sub"), // under a local entity
        ("lib_a.cnnd", "1lib"),      // invalid segment
        ("lib_a.cnnd", "libs..x"),   // empty segment
        ("notes.txt", "n"),          // not a .cnnd
        ("nowhere.cnnd", "n"),       // missing file
        ("broken.cnnd", "n"),        // unparseable
        ("linker.cnnd", "n"),        // links itself (cycle)
    ];
    for (p, alias) in attempts {
        let r = d.link_library(p, alias);
        assert!(r.is_err(), "{} as {} should fail", p, alias);
        assert_eq!(local_fingerprint(&mut d), before, "{} as {}", p, alias);
        assert_eq!(d.undo_stack.history_len(), history);
        assert!(
            d.node_type_registry.library_links.is_empty(),
            "{} as {}",
            p,
            alias
        );
    }
    ok(wire.check_only_changed(&[]));

    // An unsaved design cannot link (a relative path needs a folder).
    let mut unsaved = StructureDesigner::new();
    unsaved.new_project();
    assert!(unsaved.link_library("lib_a.cnnd", "a").is_err());
}

#[test]
fn link_and_unlink_are_undoable_inverses() {
    let ws = fixture_workspace();
    let path = ws.path().join("linker.cnnd");
    let mut d = new_design(&path);
    edit(
        &mut d,
        "Main",
        "v = int { value: 1 }\ne = expr { a: v, expression: \"a\", parameters: [{ name: \"a\", data_type: Int }] }\noutput e\n",
    );
    let ledger = wire_ledger(&d);
    let wire = DiskTripwire::arm(ws.path());

    ok(check_undo_inverse(&mut d, |d| {
        d.link_library("lib_a.cnnd", "a").unwrap()
    }));
    assert!(d.node_type_registry.node_networks.contains_key("a.foo"));
    assert!(
        d.node_type_registry
            .node_networks
            .contains_key("a.common.slab")
    );
    ok(check_wire_ledger(
        &ledger,
        &wire_ledger(&d),
        &Default::default(),
    ));

    ok(check_undo_inverse(&mut d, |d| {
        d.unlink_library("a").unwrap()
    }));
    assert!(!d.node_type_registry.node_networks.contains_key("a.foo"));
    assert!(d.node_type_registry.library_links.is_empty());
    ok(check_wire_ledger(
        &ledger,
        &wire_ledger(&d),
        &Default::default(),
    ));
    ok(check_invariants(&d));
    ok(wire.check_only_changed(&[]));
}

#[test]
fn unlink_is_refused_while_anything_uses_the_mount() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    let history = d.undo_stack.history_len();
    let err = d.unlink_library("a").unwrap_err();
    assert!(err.contains("Main"), "{}", err);
    assert!(d.node_type_registry.node_networks.contains_key("a.foo"));
    assert_eq!(d.undo_stack.history_len(), history);
    assert!(
        d.unlink_library("a.common").is_err(),
        "nested links are not the host's"
    );

    // A type-only use counts too: a local record def with a field typed by
    // a linked record keeps the link in place.
    let mut e = new_design(&ws.path().join("typeuse.cnnd"));
    e.link_library("lib_a.cnnd", "a").unwrap();
    e.add_record_type_def(RecordTypeDef::from_named_fields(
        "Wrap",
        vec![(
            "m".to_string(),
            DataType::from_string("Record(`a.Miller`)").unwrap(),
        )],
    ))
    .unwrap();
    let err = e.unlink_library("a").unwrap_err();
    assert!(err.contains("Wrap"), "{}", err);
}

#[test]
fn dotted_aliases_group_libraries_under_a_local_folder() {
    let ws = fixture_workspace();
    let mut d = new_design(&ws.path().join("dotted.cnnd"));
    edit(&mut d, "libs.local_net", "v = int { value: 1 }\noutput v\n");
    d.link_library("lib_a.cnnd", "libs.demolib").unwrap();
    assert!(
        d.node_type_registry
            .node_networks
            .contains_key("libs.demolib.foo")
    );
    assert!(
        d.node_type_registry
            .node_networks
            .contains_key("libs.demolib.common.slab")
    );
    d.link_library("libs/lib_b.cnnd", "libs.other").unwrap();
    // `libs` as a mount would contain two mounts; `libs.demolib.x` lies inside one.
    assert!(d.link_library("libs/lib_b.cnnd", "libs").is_err());
    assert!(d.link_library("libs/lib_b.cnnd", "libs.demolib.x").is_err());
    // Namespace operations on a folder holding a mount are refused.
    assert!(!d.rename_namespace("libs", "stuff"));
    assert!(!d.rename_namespace("libs.demolib", "libs.renamed"));
    assert!(d.delete_namespace("libs").is_err());
    assert!(
        d.node_type_registry
            .node_networks
            .contains_key("libs.local_net")
    );
    assert!(
        d.node_type_registry
            .node_networks
            .contains_key("libs.demolib.foo")
    );
    ok(check_save_reopen_identity(&mut d));
}

#[test]
fn a_linked_network_forgets_the_library_files_view_state() {
    use atomcad_structure_designer::camera_settings::CameraSettings;
    let ws = fixture_workspace();
    let lib = ws.path().join("lib_a.cnnd");
    let mut l = open(&lib);
    let camera = CameraSettings {
        eye: glam::DVec3::new(1.0, 2.0, 3.0),
        ..Default::default()
    };
    l.node_type_registry
        .node_networks
        .get_mut("foo")
        .unwrap()
        .camera_settings = Some(camera);
    save(&mut l);

    let d = open(&ws.path().join("host.cnnd"));
    assert!(
        d.node_type_registry.node_networks["a.foo"]
            .camera_settings
            .is_none()
    );
    let l = open(&lib);
    let eye = l.node_type_registry.node_networks["foo"]
        .camera_settings
        .as_ref()
        .unwrap()
        .eye;
    assert_eq!(eye, glam::DVec3::new(1.0, 2.0, 3.0));
}

// ---------------------------------------------------------------------------
// File format and disk writes
// ---------------------------------------------------------------------------

#[test]
fn version_gate_refuses_newer_files_and_reads_older_ones() {
    use atomcad_structure_designer::serialization::node_networks_serialization::{
        SERIALIZATION_VERSION, check_file_version,
    };
    assert_eq!(SERIALIZATION_VERSION, 9);
    assert!(
        check_file_version(9, 8).is_err(),
        "a reader capped at 8 refuses v9"
    );
    assert!(check_file_version(8, 9).is_ok());
    // A committed v8 fixture still loads.
    let v8 = fixture_path("concave_rebond/step_edge.cnnd");
    assert_eq!(json(&v8)["version"], 8);
    let _ = open(&v8);
}

#[test]
fn a_failed_write_leaves_the_previous_file_intact() {
    use atomcad_structure_designer::library_links::{FileMeta, LinkFs, RealFs, write_atomic};
    use std::io;
    struct FailRename;
    impl LinkFs for FailRename {
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
            RealFs.write(p, b)
        }
        fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
            Err(io::Error::other("injected"))
        }
        fn remove_file(&self, p: &Path) -> io::Result<()> {
            RealFs.remove_file(p)
        }
        fn create_dir_all(&self, p: &Path) -> io::Result<()> {
            RealFs.create_dir_all(p)
        }
    }
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let original = read(&host);
    let wire = DiskTripwire::arm(ws.path());
    assert!(write_atomic(&FailRename, &host, b"partial").is_err());
    assert_eq!(read(&host), original);
    ok(wire.check_only_changed(&[]));

    // Through the designer: the save fails and the file is untouched.
    let mut d = open(&host);
    d.node_type_registry
        .library_links
        .set_fs(std::sync::Arc::new(FailRename));
    assert!(d.save_node_networks().unwrap().is_err());
    assert_eq!(read(&host), original);
    ok(wire.check_only_changed(&[]));
}

// ---------------------------------------------------------------------------
// Text format: `query` → `--replace` is a no-op on a linking host (§11.1)
// ---------------------------------------------------------------------------

/// `--replace` mints fresh node ids, so the criterion is the one the text
/// round-trip corpus uses: the text comes back identical (which, wires being
/// spelled by pin name, means every wire came back to the same pin), and the
/// number of wires is unchanged.
///
/// Frozen nodes are *not* covered here: their wires cannot be spelled in the
/// text format yet (no pin names to spell them with), so a replace drops them.
/// That is the `ai_text_edit` item of Phase 2.
#[test]
fn replace_with_the_query_text_is_a_no_op_on_a_linking_host() {
    use atomcad_structure_designer::text_format::serialize_network;
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    for network in ["Main", "typed"] {
        d.set_active_node_network_name(Some(network.to_string()));
        let wires = wire_ledger(&d).len();
        let text = {
            let reg = &d.node_type_registry;
            serialize_network(&reg.node_networks[network], reg, None)
        };
        let outcome = d.ai_text_edit(&text, true);
        assert!(
            outcome.result.errors.is_empty(),
            "{:?}",
            outcome.result.errors
        );
        let after = {
            let reg = &d.node_type_registry;
            serialize_network(&reg.node_networks[network], reg, None)
        };
        assert_eq!(text, after, "{}", network);
        assert_eq!(wires, wire_ledger(&d).len(), "{}", network);
    }
    ok(check_invariants(&d));
}
