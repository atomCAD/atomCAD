//! Library linking, Phase 2 (`doc/design_library_linking.md` §11): read-only
//! enforcement (§6), the evaluation base directory (D8), the transitive-link
//! warning (D4), and the recorded layout + positional text spelling of frozen
//! nodes (§13 item 6).
//!
//! Judged with the shared oracles of `library_links_support.rs` (§11.1).

use super::library_links_support::*;
use atomcad_structure_designer::library_links::{is_frozen, mount_fingerprint};
use atomcad_structure_designer::node_network::{CollapseMode, FunctionPinRole};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;
use glam::f64::DVec2;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// A library with one of everything the mutation alphabet needs
// ---------------------------------------------------------------------------

/// The node set every alphabet target network has, by name: `x`, `y` (int
/// sources), `s` (an `expr` fed by both), `c` (a custom-network instance),
/// `mp` (a `map` with a body node `g`), `cl` (a `closure`).
fn target_code(instance_type: &str) -> String {
    format!(
        "x = int {{ value: 1 }}
y = int {{ value: 2 }}
s = expr {{ a: x, b: y, expression: \"a + b\", parameters: [{{ name: \"a\", data_type: Int }}, {{ name: \"b\", data_type: Int }}] }}
c = {} {{ p: x }}
r = range {{ count: 2 }}
mp = map {{ xs: r, input_type: Int, output_type: Int, body {{ g = int {{ value: 1 }} output g }} }}
cl = closure {{ kind: \"custom\", params: [\"q\"], type_args: [Int, Int], body {{ k = int {{ value: 3 }} output k }} }}
output s
",
        instance_type
    )
}

const OTHER: &str = "p = parameter { param_name: \"p\", data_type: Int, sort_order: 0 }
output p
";

/// `lib.cnnd` (networks `other`, `n` in folder `grp`, record `R`) and
/// `host.cnnd` linking it as `lib`, with a local network `loc` of the same
/// shape and a local record `LR`.
fn alphabet_workspace() -> (tempfile::TempDir, PathBuf) {
    let ws = tempfile::tempdir().unwrap();
    {
        let mut lib = new_design(&ws.path().join("lib.cnnd"));
        lib.add_record_type_def(int_record("R", &["u", "v"]))
            .unwrap();
        edit(&mut lib, "other", OTHER);
        edit(&mut lib, "grp.n", &target_code("other"));
        lib.delete_node_network("Main").ok();
        save(&mut lib);
    }
    let host = ws.path().join("host.cnnd");
    {
        let mut d = new_design(&host);
        d.link_library("lib.cnnd", "lib").unwrap();
        d.add_record_type_def(int_record("LR", &["u", "v"]))
            .unwrap();
        edit(&mut d, "loc", &target_code("`lib.other`"));
        save(&mut d);
    }
    (ws, host)
}

/// Where an alphabet entry lands: the linked network or its local twin.
struct Target {
    net: String,
    /// `"lib."` for the linked target, `""` for the local one.
    ns_dot: &'static str,
    /// The namespace new content would go into.
    ns: &'static str,
    record: String,
}

fn linked_target() -> Target {
    Target {
        net: "lib.grp.n".to_string(),
        ns_dot: "lib.",
        ns: "lib",
        record: "lib.R".to_string(),
    }
}

fn local_target() -> Target {
    Target {
        net: "loc".to_string(),
        ns_dot: "",
        ns: "",
        record: "LR".to_string(),
    }
}

/// Id of node `name` in the target network (top level).
fn id(d: &StructureDesigner, t: &Target, name: &str) -> u64 {
    node_id(d, &t.net, name)
}

/// Id of the body node `name` of HOF `hof` in the target network.
fn body_id(d: &StructureDesigner, t: &Target, hof: &str, name: &str) -> u64 {
    let hof = id(d, t, hof);
    d.node_type_registry.node_networks[&t.net].nodes[&hof]
        .zone
        .as_ref()
        .unwrap()
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap()
        .id
}

/// One entry point of the mutation alphabet. `Some(true)` = it reported a
/// refusal, `Some(false)` = it reported success, `None` = its signature
/// reports nothing.
type OpFn = fn(&mut StructureDesigner, &Target) -> Option<bool>;

fn set_expr_text(d: &mut StructureDesigner, t: &Target) -> Option<bool> {
    use atomcad_structure_designer::text_format::TextValue;
    let s = id(d, t, "s");
    let mut data = d.node_type_registry.node_networks[&t.net].nodes[&s]
        .data
        .clone_box();
    let mut props = std::collections::HashMap::new();
    props.insert(
        "expression".to_string(),
        TextValue::String("a * b".to_string()),
    );
    data.set_text_properties(&props).unwrap();
    d.set_node_network_data(s, data);
    None
}

fn alphabet() -> Vec<(&'static str, OpFn)> {
    vec![
        ("add node", |d, _| {
            Some(d.add_node("int", DVec2::new(900.0, 900.0)) == 0)
        }),
        ("add node in a body", |d, t| {
            let mp = id(d, t, "mp");
            Some(d.add_node_scoped(&[mp], "int", DVec2::new(10.0, 10.0), None) == 0)
        }),
        ("delete selected", |d, t| {
            let s = id(d, t, "s");
            d.select_node(s);
            d.delete_selected();
            None
        }),
        ("delete selected in a body", |d, t| {
            let mp = id(d, t, "mp");
            let g = body_id(d, t, "mp", "g");
            d.select_node_scoped(&[mp], g);
            d.delete_selected_scoped(&[mp]);
            None
        }),
        ("connect wire", |d, t| {
            let (y, s) = (id(d, t, "y"), id(d, t, "s"));
            d.connect_nodes(y, 0, s, 0);
            None
        }),
        ("delete wire", |d, t| {
            let (x, s) = (id(d, t, "x"), id(d, t, "s"));
            d.select_wire(x, 0, s, 0);
            d.delete_selected();
            None
        }),
        ("move node", |d, t| {
            let x = id(d, t, "x");
            d.move_node(x, DVec2::new(777.0, 777.0));
            None
        }),
        ("drag nodes", |d, t| {
            let x = id(d, t, "x");
            d.select_node(x);
            d.begin_move_nodes();
            d.move_selected_nodes(DVec2::new(50.0, 50.0));
            d.end_move_nodes();
            None
        }),
        ("set node data", set_expr_text),
        ("paste", |d, t| {
            let x = id(d, t, "x");
            d.select_node(x);
            d.copy_selection();
            Some(d.paste_at_position(DVec2::new(900.0, 0.0)).is_empty())
        }),
        ("cut", |d, t| {
            let x = id(d, t, "x");
            d.select_node(x);
            Some(!d.cut_selection())
        }),
        ("duplicate node", |d, t| {
            let x = id(d, t, "x");
            Some(d.duplicate_node(x) == 0)
        }),
        ("factor selection", |d, t| {
            let (x, y, s) = (id(d, t, "x"), id(d, t, "y"), id(d, t, "s"));
            d.select_nodes(vec![x, y, s]);
            Some(
                d.factor_selection_into_subnetwork("factored", vec![])
                    .is_err(),
            )
        }),
        ("inline instance", |d, t| {
            let c = id(d, t, "c");
            Some(d.inline_custom_node(vec![], c).is_err())
        }),
        ("instance to closure", |d, t| {
            let c = id(d, t, "c");
            Some(d.convert_instance_to_closure(vec![], c).is_err())
        }),
        ("closure to network", |d, t| {
            let cl = id(d, t, "cl");
            Some(
                d.extract_closure_to_network(vec![], cl, "extracted")
                    .is_err(),
            )
        }),
        ("promote to parameter", |d, t| {
            let x = id(d, t, "x");
            Some(d.promote_node_to_parameter(x).is_err())
        }),
        ("set description", |d, _| {
            Some(d.set_active_network_description("changed".into()).is_err())
        }),
        ("set summary", |d, _| {
            Some(
                d.set_active_network_summary(Some("changed".into()))
                    .is_err(),
            )
        }),
        ("rename node", |d, t| {
            let x = id(d, t, "x");
            Some(d.rename_node(&[], x, "renamed").is_err())
        }),
        ("set function-pin role", |d, t| {
            let c = id(d, t, "c");
            d.set_function_pin_role(&[], c, 0, FunctionPinRole::Delayed);
            None
        }),
        ("set collapse mode", |d, t| {
            let mp = id(d, t, "mp");
            d.set_collapse_mode(&[], mp, CollapseMode::Collapsed);
            None
        }),
        ("set zone size", |d, t| {
            let mp = id(d, t, "mp");
            d.set_zone_size(&[], mp, 555.0, 333.0);
            None
        }),
        ("set return node", |d, t| {
            let x = id(d, t, "x");
            Some(!d.set_return_node_id(Some(x)))
        }),
        ("ai_text_edit (edit)", |d, _| {
            Some(
                !d.ai_text_edit("zz = int { value: 5 }", false)
                    .result
                    .errors
                    .is_empty(),
            )
        }),
        ("ai_text_edit (replace)", |d, t| {
            let text = {
                let reg = &d.node_type_registry;
                serialize_network(&reg.node_networks[&t.net], reg, None)
            };
            let text = format!("{}\nzz = int {{ value: 5 }}\n", text);
            Some(!d.ai_text_edit(&text, true).result.errors.is_empty())
        }),
        ("rename network", |d, t| {
            let new = format!("{}_renamed", t.net.rsplit('.').next().unwrap());
            Some(!d.rename_node_network(&t.net, &new))
        }),
        ("delete network", |d, t| {
            let net = t.net.clone();
            d.set_active_node_network_name(Some("Main".into()));
            Some(d.delete_node_network(&net).is_err())
        }),
        ("add network in the namespace", |d, t| {
            Some(d.add_new_node_network_in_namespace(t.ns).is_empty())
        }),
        ("add named network", |d, t| {
            Some(
                d.add_node_network_with_undo(&format!("{}new_net", t.ns_dot))
                    .is_err(),
            )
        }),
        ("add folder", |d, t| {
            Some(d.add_folder(&format!("{}new_folder", t.ns_dot)).is_err())
        }),
        ("add record def", |d, t| {
            Some(
                d.add_record_type_def(int_record(&format!("{}NewRec", t.ns_dot), &["w"]))
                    .is_err(),
            )
        }),
        ("add record def in the namespace", |d, t| {
            Some(d.add_new_record_type_def_in_namespace(t.ns).is_err())
        }),
        ("update record def", |d, t| {
            use atomcad_structure_designer::data_type::DataType;
            Some(
                d.update_record_type_def(&t.record, vec![("u".to_string(), DataType::Float)])
                    .is_err(),
            )
        }),
        ("rename record def", |d, t| {
            Some(d.rename_record_type_def(&t.record, "Renamed").is_err())
        }),
        ("delete record def", |d, t| {
            Some(d.delete_record_type_def(&t.record).is_err())
        }),
    ]
}

/// The alphabet entries that address the library's namespace rather than one
/// network; judged only on the linked side (the local side has no folder).
fn namespace_ops() -> Vec<(&'static str, OpFn)> {
    vec![
        ("rename namespace", |d, _| {
            Some(!d.rename_namespace("lib.grp", "moved"))
        }),
        ("rename namespace (the mount)", |d, _| {
            Some(!d.rename_namespace("lib", "moved"))
        }),
        ("delete namespace", |d, _| {
            Some(d.delete_namespace("lib.grp").is_err())
        }),
        ("move a local network into the mount", |d, _| {
            Some(!d.rename_node_network("loc", "lib.grp.loc"))
        }),
    ]
}

fn open_at(host: &Path, t: &Target) -> StructureDesigner {
    let mut d = open(host);
    d.set_active_node_network_name(Some(t.net.clone()));
    d
}

#[test]
fn every_mutation_of_linked_content_is_refused_and_changes_nothing() {
    let (_ws, host) = alphabet_workspace();
    let t = linked_target();
    let ops: Vec<_> = alphabet().into_iter().chain(namespace_ops()).collect();
    assert!(ops.len() > 35);
    for (name, op) in ops {
        let mut d = open_at(&host, &t);
        assert!(d.active_network_is_linked());
        // The mount really has content to protect.
        assert!(
            mount_fingerprint(&d.node_type_registry, "lib").contains("\"lib.grp.n\""),
            "{}",
            name
        );
        let mounts = mount_fingerprints(&d);
        let local = local_fingerprint(&mut d);
        let pushes = d.undo_stack.push_count();
        let refused = op(&mut d, &t);
        assert_ne!(refused, Some(false), "'{}' reported success", name);
        assert_eq!(
            mounts,
            mount_fingerprints(&d),
            "'{}' changed the mount",
            name
        );
        assert_eq!(
            local,
            local_fingerprint(&mut d),
            "'{}' changed local content",
            name
        );
        assert_eq!(
            pushes,
            d.undo_stack.push_count(),
            "'{}' pushed an undo step",
            name
        );
        assert!(!d.is_dirty(), "'{}' set the design dirty", name);
    }
}

/// The guard is not too broad: the same alphabet on a local network works.
#[test]
fn the_same_mutations_of_local_content_succeed() {
    let (_ws, host) = alphabet_workspace();
    let t = local_target();
    for (name, op) in alphabet() {
        let mut d = open_at(&host, &t);
        let before = local_fingerprint(&mut d);
        let pushes = d.undo_stack.push_count();
        let refused = op(&mut d, &t);
        assert_ne!(
            refused,
            Some(true),
            "'{}' was refused on local content",
            name
        );
        assert!(
            local_fingerprint(&mut d) != before || d.undo_stack.push_count() != pushes,
            "'{}' did nothing on local content",
            name
        );
    }
}

/// Namespace layer (§6 layer 1): nothing new may be named into a mount, nor
/// be named like the folder holding one.
#[test]
fn nothing_can_be_named_into_a_mount() {
    let ws = fixture_workspace();
    let d = open(&ws.path().join("host.cnnd"));
    let reg = &d.node_type_registry;
    assert!(reg.name_is_taken("a.fresh"));
    assert!(reg.name_is_taken("a.common.fresh"));
    assert!(reg.name_is_taken("a"));
    assert!(!reg.name_is_taken("ab"));
    assert!(!reg.name_is_taken("a_extra"));
}

// ---------------------------------------------------------------------------
// Browsing linked content (§5.4)
// ---------------------------------------------------------------------------

#[test]
fn browsing_a_linked_network_is_allowed_and_leaves_the_host_clean() {
    let ws = fixture_workspace();
    let host = ws.path().join("host.cnnd");
    let mut d = open(&host);
    let saved = saved_text(&mut d, Some(ws.path()));
    let pushes = d.undo_stack.push_count();

    d.set_active_node_network_name(Some("a.foo".to_string()));
    let s = node_id(&d, "a.foo", "s");
    assert!(d.select_node(s));
    d.set_node_display(s, false);
    d.set_node_display(s, true);
    d.toggle_output_pin_display(s, 0);
    let displayed = d.node_type_registry.node_networks["a.foo"]
        .displayed_nodes
        .contains_key(&s);
    d.navigate_back();

    assert!(!displayed, "the display toggles took effect");
    assert!(!d.is_dirty());
    assert_eq!(
        pushes,
        d.undo_stack.push_count(),
        "view state is no undo step"
    );
    assert_eq!(saved, saved_text(&mut d, Some(ws.path())), "nothing saved");
}

#[test]
fn duplicate_into_my_file_makes_an_editable_local_copy() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    let mut copy = String::new();
    ok(check_undo_inverse(&mut d, |d| {
        copy = d.duplicate_node_network("a.bar").unwrap();
    }));
    assert_eq!(copy, "bar");
    assert!(d.ensure_editable("bar").is_ok());
    let saved = saved_text(&mut d, Some(ws.path()));
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let networks = json["node_networks"].as_array().unwrap();
    let bar = networks
        .iter()
        .find(|n| n[0] == "bar")
        .expect("the copy is saved");
    // Its reference to another network of the library stays a link.
    assert!(bar.to_string().contains("\"a.foo\""));
    // The copy is editable.
    d.set_active_node_network_name(Some("bar".to_string()));
    assert_ne!(d.add_node("int", DVec2::ZERO), 0);
    ok(check_invariants(&d));
}

// ---------------------------------------------------------------------------
// Local networks that contain frozen nodes (§8)
// ---------------------------------------------------------------------------

/// Local edits on `host_missing.cnnd`'s `Main`. Each returns the node it removes
/// from `Main`, if any (whose own wires then go with it).
fn frozen_host_edits() -> Vec<(
    &'static str,
    fn(&mut StructureDesigner) -> Option<&'static str>,
)> {
    fn nid(d: &StructureDesigner, name: &str) -> u64 {
        node_id(d, "Main", name)
    }
    vec![
        ("add a node", |d| {
            assert_ne!(d.add_node("int", DVec2::new(900.0, 900.0)), 0);
            None
        }),
        ("connect an unrelated wire", |d| {
            let (i2, r1) = (nid(d, "i2"), nid(d, "r1"));
            d.connect_nodes(i2, 0, r1, 0);
            None
        }),
        ("delete a neighbour", |d| {
            let e3 = nid(d, "e3");
            d.select_node(e3);
            d.delete_selected();
            Some("e3")
        }),
        ("move a frozen node", |d| {
            let f = nid(d, "f");
            d.select_node(f);
            d.begin_move_nodes();
            d.move_selected_nodes(DVec2::new(40.0, 40.0));
            d.end_move_nodes();
            None
        }),
        ("set data of a frozen node's source", |d| {
            use atomcad_structure_designer::text_format::TextValue;
            let i1 = nid(d, "i1");
            let mut data = d.node_type_registry.node_networks["Main"].nodes[&i1]
                .data
                .clone_box();
            let mut props = std::collections::HashMap::new();
            props.insert("value".to_string(), TextValue::Int(11));
            data.set_text_properties(&props).unwrap();
            d.set_node_network_data(i1, data);
            None
        }),
        ("copy and paste a frozen node", |d| {
            let rc = nid(d, "rc");
            d.select_node(rc);
            d.copy_selection();
            let pasted = d.paste_at_position(DVec2::new(1200.0, 0.0));
            assert_eq!(pasted.len(), 1);
            let node = &d.node_type_registry.node_networks["Main"].nodes[&pasted[0]];
            assert!(
                is_frozen(node, &d.node_type_registry),
                "the copy is frozen too"
            );
            assert_eq!(node.arguments.len(), 3);
            None
        }),
        ("duplicate a frozen node", |d| {
            let f = nid(d, "f");
            let dup = d.duplicate_node(f);
            assert_ne!(dup, 0);
            let node = &d.node_type_registry.node_networks["Main"].nodes[&dup];
            assert!(
                is_frozen(node, &d.node_type_registry),
                "the copy is frozen too"
            );
            assert_eq!(node.arguments.len(), 2);
            None
        }),
        ("delete a frozen node", |d| {
            let f = nid(d, "f");
            d.select_node(f);
            d.delete_selected();
            Some("f")
        }),
        ("factor a selection holding a frozen node", |d| {
            let (f, e1) = (nid(d, "f"), nid(d, "e1"));
            d.select_nodes(vec![f, e1]);
            d.factor_selection_into_subnetwork("factored", vec!["p".into(), "q".into()])
                .unwrap();
            let factored = &d.node_type_registry.node_networks["factored"];
            let frozen = factored
                .nodes
                .values()
                .find(|n| n.node_type_name == "demolib.foo")
                .expect("the frozen node moved");
            assert!(
                frozen.arguments.iter().all(|a| !a.is_empty()),
                "its wires moved with it"
            );
            Some("f")
        }),
    ]
}

#[test]
fn local_edits_beside_frozen_nodes_keep_them_and_their_wires() {
    for (name, op) in frozen_host_edits() {
        let ws = fixture_workspace();
        let mut d = open(&ws.path().join("host_missing.cnnd"));
        d.set_active_node_network_name(Some("Main".to_string()));
        let before = frozen_wires(&d);
        assert!(before.len() > 10, "the fixture has frozen wires");
        let ids: std::collections::HashMap<String, u64> =
            d.node_type_registry.node_networks["Main"]
                .nodes
                .values()
                .filter_map(|n| n.custom_name.clone().map(|c| (c, n.id)))
                .collect();
        let removed = op(&mut d).map(|name| ids[name]);
        let after = frozen_wires(&d);
        for w in &before {
            let gone_with_node =
                removed.is_some_and(|id| w.network == "Main" && (w.dest == id || w.source == id));
            if !gone_with_node {
                assert!(after.contains(w), "'{}' lost frozen wire {:?}", name, w);
            }
        }
        ok(check_invariants(&d));
        // The edit is one undo step that restores every frozen wire.
        assert!(d.undo(), "'{}' is undoable", name);
        assert_eq!(
            before,
            frozen_wires(&d),
            "'{}': undo restores the frozen wires",
            name
        );
        assert!(d.redo());
        // And whatever it left survives a save and a reopen.
        ok(check_save_reopen_identity(&mut d));
    }
}

// ---------------------------------------------------------------------------
// Recorded layouts of frozen nodes (§13 item 6)
// ---------------------------------------------------------------------------

fn pin_names(d: &StructureDesigner, network: &str, name: &str) -> (Vec<String>, Vec<String>) {
    let reg = &d.node_type_registry;
    let node = &reg.node_networks[network].nodes[&node_id(d, network, name)];
    match reg.get_node_type_for_node(node) {
        Some(t) => (
            t.parameters.iter().map(|p| p.name.clone()).collect(),
            t.output_pins.iter().map(|p| p.name.clone()).collect(),
        ),
        None => (Vec::new(), Vec::new()),
    }
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn frozen_nodes_get_the_pins_of_their_recorded_interface() {
    let ws = fixture_workspace();
    let d = open(&ws.path().join("host_missing.cnnd"));
    let reg = &d.node_type_registry;
    for name in ["f", "f2", "rc", "rd", "pr", "sp"] {
        let node = &reg.node_networks["Main"].nodes[&node_id(&d, "Main", name)];
        assert!(is_frozen(node, reg), "{} stays frozen", name);
    }
    let (params, outputs) = pin_names(&d, "Main", "f");
    assert_eq!(params, strings(&["x", "y"]));
    assert_eq!(outputs.len(), 1);
    assert_eq!(pin_names(&d, "Main", "rc").0, strings(&["h", "k", "l"]));
    assert_eq!(pin_names(&d, "Main", "pr").0, strings(&["h", "k", "l"]));
    assert!(
        pin_names(&d, "Main", "rd")
            .1
            .ends_with(&strings(&["h", "k", "l"])),
        "{:?}",
        pin_names(&d, "Main", "rd").1
    );
    assert_eq!(pin_names(&d, "Main", "sp").0, strings(&["m"]));
    // An `apply` fed by a frozen node's function pin derives its argument
    // pins from the recorded signature.
    let (apply_params, _) = pin_names(&d, "Main", "ap");
    assert_eq!(apply_params.len(), 3, "{:?}", apply_params);
    assert_eq!(apply_params[0], "f");
    ok(check_invariants(&d));
}

/// `host_missing.cnnd` with its `uses` table deleted: no recorded interface.
fn host_missing_without_uses(ws: &Path) -> PathBuf {
    let host = ws.join("host_missing.cnnd");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&host).unwrap()).unwrap();
    for import in json["imports"].as_array_mut().unwrap() {
        import.as_object_mut().unwrap().remove("uses");
    }
    std::fs::write(&host, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    host
}

#[test]
fn a_frozen_node_without_a_recorded_interface_keeps_its_wires() {
    let ws = fixture_workspace();
    let d = open(&host_missing_without_uses(ws.path()));
    let reg = &d.node_type_registry;
    let f = &reg.node_networks["Main"].nodes[&node_id(&d, "Main", "f")];
    assert!(is_frozen(f, reg));
    assert!(
        f.custom_node_type.is_none(),
        "no layout without a `uses` entry"
    );
    assert_eq!(f.arguments.len(), 2);
    assert!(f.arguments.iter().all(|a| !a.is_empty()));
    ok(check_invariants(&d));
}

// ---------------------------------------------------------------------------
// The text format on hosts with frozen nodes (§13 item 6)
// ---------------------------------------------------------------------------

/// The errors of an AI edit other than the frozen nodes' own validation errors,
/// which `ai_text_edit` folds in and which every edit of such a network carries.
fn edit_errors(
    outcome: &atomcad_structure_designer::ai_text_edit::AiTextEditOutcome,
) -> Vec<String> {
    outcome
        .result
        .errors
        .iter()
        .filter(|e| !e.contains("(linked library not available)"))
        .cloned()
        .collect()
}

fn query(d: &StructureDesigner, network: &str) -> String {
    let reg = &d.node_type_registry;
    serialize_network(&reg.node_networks[network], reg, None)
}

/// `query` then `--replace` with that text: identical text back, the same
/// number of wires, frozen nodes still frozen, invariants intact. `--replace`
/// mints fresh node ids, so this is the text round-trip corpus's criterion.
fn assert_replace_is_a_no_op(d: &mut StructureDesigner, network: &str) -> String {
    d.set_active_node_network_name(Some(network.to_string()));
    let wires = wire_ledger(d).len();
    let frozen = frozen_wires(d).len();
    let text = query(d, network);
    let outcome = d.ai_text_edit(&text, true);
    assert!(
        edit_errors(&outcome).is_empty(),
        "{}: {:?}",
        network,
        edit_errors(&outcome)
    );
    assert!(
        outcome.result.warnings.is_empty(),
        "{}: {:?}",
        network,
        outcome.result.warnings
    );
    assert_eq!(text, query(d, network), "{}", network);
    assert_eq!(wires, wire_ledger(d).len(), "{}: wire count", network);
    assert_eq!(
        frozen,
        frozen_wires(d).len(),
        "{}: frozen wire count",
        network
    );
    ok(check_invariants(d));
    text
}

#[test]
fn replace_with_the_query_text_keeps_every_frozen_wire() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host_missing.cnnd"));
    assert!(frozen_wires(&d).len() > 10);
    for network in ["Main", "typed"] {
        let text = assert_replace_is_a_no_op(&mut d, network);
        // Every wire is spelled by name: the recorded layouts supply them.
        assert!(!text.contains(".@") && !text.contains(" @0:"), "{}", text);
    }
    let main = query(&d, "Main");
    assert!(main.contains("rd.k"), "{}", main);
    assert!(main.contains("x: i1"), "{}", main);
}

#[test]
fn without_a_recorded_interface_frozen_wires_are_spelled_by_position() {
    let ws = fixture_workspace();
    let mut d = open(&host_missing_without_uses(ws.path()));
    let text = assert_replace_is_a_no_op(&mut d, "Main");
    // Wires into the frozen instance and out of the frozen destructure.
    assert!(text.contains("@0: i1"), "{}", text);
    assert!(text.contains("@1: i2"), "{}", text);
    assert!(text.contains("rd.@"), "{}", text);
    // The `apply` fed by `@f2` keeps its argument wires positionally.
    let ap = &d.node_type_registry.node_networks["Main"].nodes[&node_id(&d, "Main", "ap")];
    assert_eq!(ap.arguments.iter().filter(|a| !a.is_empty()).count(), 3);
}

#[test]
fn an_unrelated_ai_edit_leaves_every_frozen_wire_in_place() {
    for without_uses in [false, true] {
        let ws = fixture_workspace();
        let host = if without_uses {
            host_missing_without_uses(ws.path())
        } else {
            ws.path().join("host_missing.cnnd")
        };
        let mut d = open(&host);
        d.set_active_node_network_name(Some("Main".to_string()));
        let before = wire_ledger(&d);
        let outcome = d.ai_text_edit(
            "zz = int { value: 9 }\ne9 = expr { a: zz, expression: \"a\", parameters: [{ name: \"a\", data_type: Int }] }",
            false,
        );
        assert!(
            edit_errors(&outcome).is_empty(),
            "{:?}",
            edit_errors(&outcome)
        );
        let e9 = node_id(&d, "Main", "e9");
        let after: BTreeSet<WireEntry> = wire_ledger(&d)
            .into_iter()
            .filter(|w| w.dest != e9)
            .collect();
        ok(check_wire_ledger(&before, &after, &BTreeSet::new()));
        ok(check_invariants(&d));
    }
}

#[test]
fn a_positional_pin_is_refused_on_a_node_whose_type_resolves() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host_missing.cnnd"));
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit("e1 = expr { @0: i2 }", false);
    let all = format!("{:?} {:?}", outcome.result.errors, outcome.result.warnings);
    assert!(all.contains("positional pin"), "{}", all);
    // And accepted on a frozen one: re-point `f`'s first pin by position.
    let outcome = d.ai_text_edit("f = `demolib.foo` { @0: i2 }", false);
    assert!(
        edit_errors(&outcome).is_empty(),
        "{:?}",
        edit_errors(&outcome)
    );
    assert!(
        outcome.result.warnings.is_empty(),
        "{:?}",
        outcome.result.warnings
    );
    let f = &d.node_type_registry.node_networks["Main"].nodes[&node_id(&d, "Main", "f")];
    let i2 = node_id(&d, "Main", "i2");
    assert_eq!(f.arguments[0].incoming_wires[0].source_node_id, i2);
}

// ---------------------------------------------------------------------------
// The evaluation base directory (D8)
// ---------------------------------------------------------------------------

const READ_TIP: &str = "s = string { value: \"tip.xyz\" }
t = import_xyz { file_name: s }
output t
";

const READ_TIP_IN_BODY: &str = "s = string { value: \"tip.xyz\" }
r = range { count: 1 }
mp = map { xs: r, input_type: Int, output_type: Molecule, body { t = import_xyz { file_name: ^s } output t } }
c = collect { iter: mp, element_type: Molecule }
output c
";

/// `ws/libs/xyzlib.cnnd` reads `tip.xyz` through a *wired* file name (so the
/// read happens at evaluation time), at top level and inside a map body;
/// `ws/host.cnnd` links it and has a local network doing the same read.
/// `ws/libs/tip.xyz` has two atoms, `ws/tip.xyz` three.
fn eval_dir_workspace() -> (tempfile::TempDir, PathBuf) {
    let ws = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(ws.path().join("libs")).unwrap();
    std::fs::write(
        ws.path().join("libs/tip.xyz"),
        "2\nlib tip\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\n",
    )
    .unwrap();
    std::fs::write(
        ws.path().join("tip.xyz"),
        "3\nhost tip\nO 0.0 0.0 0.0\nH 0.0 0.76 0.59\nH 0.0 -0.76 0.59\n",
    )
    .unwrap();
    {
        let mut lib = new_design(&ws.path().join("libs/xyzlib.cnnd"));
        edit(&mut lib, "tip", READ_TIP);
        edit(&mut lib, "tips", READ_TIP_IN_BODY);
        lib.delete_node_network("Main").ok();
        save(&mut lib);
    }
    let host = ws.path().join("host.cnnd");
    {
        let mut d = new_design(&host);
        d.link_library("libs/xyzlib.cnnd", "x").unwrap();
        edit(&mut d, "loc", READ_TIP);
        edit(
            &mut d,
            "Main",
            "h = `x.tip` {}\nhs = `x.tips` {}\noutput h\n",
        );
        save(&mut d);
    }
    (ws, host)
}

fn atom_count(
    result: atomcad_structure_designer::evaluator::network_result::NetworkResult,
) -> usize {
    use atomcad_structure_designer::evaluator::network_result::NetworkResult;
    match result {
        NetworkResult::Molecule(m) => m.atoms.get_num_of_atoms(),
        NetworkResult::Crystal(c) => c.atoms.get_num_of_atoms(),
        NetworkResult::Array(items) => {
            assert_eq!(items.len(), 1);
            atom_count(items.into_iter().next().unwrap())
        }
        other => panic!("expected atoms, got {}", other.to_display_string()),
    }
}

#[test]
fn a_relative_path_in_a_linked_network_resolves_against_the_library() {
    let (_ws, host) = eval_dir_workspace();
    let mut d = open(&host);
    d.set_active_node_network_name(Some("Main".to_string()));
    let h = node_id(&d, "Main", "h");
    let hs = node_id(&d, "Main", "hs");
    assert_eq!(
        atom_count(d.evaluate_node_output(&[], h, 0)),
        2,
        "libs/tip.xyz"
    );
    assert_eq!(
        atom_count(d.evaluate_node_output(&[], hs, 0)),
        2,
        "libs/tip.xyz, read from a map body inside the library"
    );
    d.set_active_node_network_name(Some("loc".to_string()));
    let t = node_id(&d, "loc", "t");
    assert_eq!(
        atom_count(d.evaluate_node_output(&[], t, 0)),
        3,
        "the host's tip.xyz"
    );
    // Browsing the linked network itself resolves against the library too.
    d.set_active_node_network_name(Some("x.tip".to_string()));
    let t = node_id(&d, "x.tip", "t");
    assert_eq!(atom_count(d.evaluate_node_output(&[], t, 0)), 2);
}

// ---------------------------------------------------------------------------
// The transitive-link warning (D4)
// ---------------------------------------------------------------------------

#[test]
fn using_a_transitive_library_warns_and_a_direct_one_does_not() {
    let ws = fixture_workspace();
    let mut d = open(&ws.path().join("host.cnnd"));
    d.set_active_node_network_name(Some("Main".to_string()));
    d.validate_active_network();
    let main = &d.node_type_registry.node_networks["Main"];
    let for_node = |name: &str| {
        let id = node_id(&d, "Main", name);
        main.validation_errors
            .iter()
            .filter(|e| e.node_id == Some(id))
            .map(|e| (e.error_text.clone(), e.blocking))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        for_node("s"),
        vec![(
            "uses transitive library `a.common`; link it directly".to_string(),
            false
        )]
    );
    assert!(for_node("f").is_empty());
    assert!(for_node("rc").is_empty());
    // The library's own use of its direct link is not the host's business.
    let lib_errors = &d.node_type_registry.node_networks["a.usecommon"].validation_errors;
    assert!(
        lib_errors
            .iter()
            .all(|e| !e.error_text.contains("transitive"))
    );
}
