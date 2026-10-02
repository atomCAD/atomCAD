//! Loading a `.cnnd` must not depend on how its networks are named.
//!
//! A file stores its networks sorted by name. The loader used to read, repair
//! and insert them one at a time in that order, so while network A was being
//! repaired every network whose name sorts after A was not in the registry
//! yet, and any pass that looked one up saw nothing. Two symptoms:
//!
//! - `repair_zone_body` compared a body instance of such a network against
//!   `DataType::None` and dropped its zone-input wires on every reopen (found
//!   by library linking Phase 6: *Make local copy* puts `a.*` networks after
//!   `Main`; reported again by a user whose `TIP.*` closure bodies called
//!   `geo.*` networks — uppercase sorts before lowercase).
//! - Everything else stage 1 derives from another network (an `apply`'s layout
//!   from the function pin wired into `f`, the type check of a body wire) came
//!   out differently depending on which side of the caller the callee's name
//!   sorted.
//!
//! The loader now inserts every network first and repairs them afterwards,
//! callees before callers (`NodeTypeRegistry::networks_in_dependency_order_among`).
//! The tests here check the order helper, each symptom, a cycle (which no
//! order can satisfy), and a library loaded by the same function. Names that
//! never resolve in a valid document — a missing library's — are frozen nodes,
//! covered by the library-linking tests.

use super::library_links_support::*;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use std::collections::BTreeSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The node named `name` directly inside the body of the top-level HOF named
/// `hof` in `network`.
fn body_node<'a>(
    d: &'a StructureDesigner,
    network: &str,
    hof: &str,
    name: &str,
) -> &'a atomcad_structure_designer::node_network::Node {
    let hof_id = node_id(d, network, hof);
    let body = d.node_type_registry.node_networks[network].nodes[&hof_id]
        .zone
        .as_ref()
        .expect("a body");
    body.nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no body node '{}' in '{}'/{}", name, network, hof))
}

/// How many wires feed argument `arg` of a node.
fn wires_into(node: &atomcad_structure_designer::node_network::Node, arg: usize) -> usize {
    node.arguments
        .get(arg)
        .map(|a| a.incoming_wires.len())
        .unwrap_or(0)
}

/// Whether argument `arg` of `node` is fed by the zone input `pin` of the HOF
/// one scope up.
fn fed_by_zone_input(
    node: &atomcad_structure_designer::node_network::Node,
    arg: usize,
    pin: usize,
) -> bool {
    node.arguments.get(arg).is_some_and(|a| {
        a.incoming_wires.iter().any(|w| {
            w.source_scope_depth == 1
                && matches!(w.source_pin, SourcePin::ZoneInput { pin_index } if pin_index == pin)
        })
    })
}

/// Rewrites the saved file at `path` through its JSON.
fn rewrite_json(path: &Path, f: impl FnOnce(&mut serde_json::Value)) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    f(&mut value);
    std::fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

/// The saved network `name` in a file's JSON.
fn json_network<'a>(file: &'a mut serde_json::Value, name: &str) -> &'a mut serde_json::Value {
    file["node_networks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|pair| pair[0] == name)
        .map(|pair| &mut pair[1])
        .unwrap_or_else(|| panic!("no saved network '{}'", name))
}

/// Renames every instance of network `from` to `to` among `network`'s saved
/// nodes, bodies included; returns how many it renamed.
fn retarget_in(network: &mut serde_json::Value, from: &str, to: &str) -> usize {
    let mut hits = 0;
    for node in network["nodes"].as_array_mut().unwrap() {
        if node["node_type_name"] == from {
            node["node_type_name"] = serde_json::Value::from(to);
            hits += 1;
        }
        if node["zone"].is_object() {
            hits += retarget_in(&mut node["zone"], from, to);
        }
    }
    hits
}

/// Renames every instance of network `from` to `to` inside saved network
/// `network` (not the network list itself: a hand-edited reference).
fn retarget_instances(file: &mut serde_json::Value, network: &str, from: &str, to: &str) {
    let hits = retarget_in(json_network(file, network), from, to);
    assert!(hits > 0, "no instance of '{}' in '{}'", from, network);
}

/// Loads `path` the way a library mount or the first half of an open does:
/// the serialization stage alone, without `StructureDesigner`'s validation.
/// Whatever is right here was derived *during* the load.
fn load_stage_one(path: &Path) -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &path.to_string_lossy()).expect("load");
    registry
}

fn top_level_node<'a>(
    network: &'a NodeNetwork,
    name: &str,
) -> &'a atomcad_structure_designer::node_network::Node {
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}'", name))
}

/// A callee with one Int parameter `x` and an Int result, under `name`.
fn add_int_callee(d: &mut StructureDesigner, name: &str) {
    edit(
        d,
        name,
        "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
output x
",
    );
}

/// `Main`: a `map` over two Ints whose body hands `$element` to `callee`.
fn add_map_caller(d: &mut StructureDesigner, callee: &str) {
    edit(
        d,
        "Main",
        &format!(
            "r = range {{ count: 2 }}
mp = map {{ xs: r, input_type: Int, output_type: Int, body {{ g = `{}` {{ x: $element }} output g }} }}
output mp
",
            callee
        ),
    );
}

// ---------------------------------------------------------------------------
// The order helper
// ---------------------------------------------------------------------------

/// Leaf `c`; `b` calls `c` inside a body; `a` calls `b`; `x` calls nothing.
fn order_design() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.new_project();
    edit(&mut d, "c", "k = int { value: 1 }\noutput k\n");
    edit(
        &mut d,
        "b",
        "r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = c {} output g } }
output mp
",
    );
    edit(&mut d, "a", "q = b {}\noutput q\n");
    edit(&mut d, "x", "k = int { value: 2 }\noutput k\n");
    d
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn dependency_order_puts_callees_first_including_calls_from_bodies() {
    let d = order_design();
    let order = d
        .node_type_registry
        .networks_in_dependency_order_among(&names(&["a", "x", "b", "c"]));
    assert_eq!(order, names(&["c", "b", "a", "x"]));
}

#[test]
fn dependency_order_keeps_the_given_order_among_independent_networks() {
    let d = order_design();
    let registry = &d.node_type_registry;
    assert_eq!(
        registry.networks_in_dependency_order_among(&names(&["x", "c"])),
        names(&["x", "c"])
    );
    assert_eq!(
        registry.networks_in_dependency_order_among(&names(&["c", "x"])),
        names(&["c", "x"])
    );
}

#[test]
fn dependency_order_follows_only_networks_in_the_given_set() {
    let d = order_design();
    // `a` calls `b`, which is not in the set: `b` (and `c` behind it) are not
    // pulled in, and `a` keeps its place.
    let order = d
        .node_type_registry
        .networks_in_dependency_order_among(&names(&["x", "a"]));
    assert_eq!(order, names(&["x", "a"]));
}

#[test]
fn dependency_order_skips_names_the_registry_does_not_have_and_lists_each_once() {
    let d = order_design();
    let order = d
        .node_type_registry
        .networks_in_dependency_order_among(&names(&["a", "missing", "a", "c"]));
    // `b` is not in the set, so `a` does not pull it (or `c`) in front of it.
    assert_eq!(order, names(&["a", "c"]));
}

#[test]
fn dependency_order_terminates_on_a_cycle_and_lists_every_member_once() {
    let mut d = order_design();
    // Make `c` call `a`: a -> b -> c -> a.
    edit(&mut d, "c", "q = a {}\nk = int { value: 1 }\noutput k\n");
    let order = d
        .node_type_registry
        .networks_in_dependency_order_among(&names(&["a", "b", "c", "x"]));
    let set: BTreeSet<&String> = order.iter().collect();
    assert_eq!(order.len(), 4, "{:?}", order);
    assert_eq!(set.len(), 4, "{:?}", order);
    assert_eq!(order.last().map(String::as_str), Some("x"));
}

// ---------------------------------------------------------------------------
// Zone-input wires into a body instance of a later-sorting network
// ---------------------------------------------------------------------------

#[test]
fn a_body_instance_of_a_network_that_sorts_later_keeps_its_zone_input_wires_on_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.cnnd");
    let mut d = new_design(&path);
    add_int_callee(&mut d, "zfoo");
    add_map_caller(&mut d, "zfoo");
    save(&mut d);
    let before = wire_ledger(&d);
    assert!(
        before.iter().any(|w| !w.scope.is_empty()),
        "the body wire must be there to lose"
    );
    let reopened = open(&path);
    ok(check_wire_ledger(
        &before,
        &wire_ledger(&reopened),
        &BTreeSet::new(),
    ));
    ok(check_invariants(&reopened));
}

/// The reported file's shape: an uppercase network (`TIP.…` sorts before every
/// lowercase name) whose `custom` closure hands its first parameter, a Float,
/// to a lowercase network's `radius`; the second parameter goes to a built-in.
#[test]
fn a_closure_parameter_wired_into_a_lowercase_network_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tip.cnnd");
    let mut d = new_design(&path);
    edit(
        &mut d,
        "geo.disk",
        "radius = parameter { param_name: \"radius\", data_type: Float, sort_order: 0 }
output radius
",
    );
    edit(
        &mut d,
        "TIP.tip",
        "cl = closure { kind: \"custom\", params: [\"r\", \"n\"], type_args: [Float, Int, Float], body { g = `geo.disk` { radius: $r } v = ivec3 { z: $n } output g } }
output cl
",
    );
    save(&mut d);

    let reopened = open(&path);
    let disk = body_node(&reopened, "TIP.tip", "cl", "g");
    assert!(
        fed_by_zone_input(disk, 0, 0),
        "closure parameter 0 -> geo.disk.radius was dropped"
    );
    let v = body_node(&reopened, "TIP.tip", "cl", "v");
    assert!(
        fed_by_zone_input(v, 2, 1),
        "the built-in's wire must stay too"
    );
    ok(check_invariants(&reopened));
}

// ---------------------------------------------------------------------------
// What the load itself derives no longer depends on names
// ---------------------------------------------------------------------------

/// An `apply` fed by the function pin of a network instance takes its argument
/// pins from that network's signature. With the callee loaded after the caller
/// the load left `apply` at the bare `[f]` and only the later validation pass
/// completed it; now the load itself sees the callee.
#[test]
fn the_load_derives_an_apply_layout_from_a_network_that_sorts_later() {
    for callee in ["Afoo", "zfoo"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apply.cnnd");
        let mut d = new_design(&path);
        add_int_callee(&mut d, callee);
        edit(
            &mut d,
            "Main",
            &format!(
                "i = int {{ value: 1 }}
g = `{}` {{}}
ap = apply {{ f: @g, arg0: i }}
output ap
",
                callee
            ),
        );
        save(&mut d);

        let registry = load_stage_one(&path);
        let main = &registry.node_networks["Main"];
        let ap = top_level_node(main, "ap");
        let pins = registry
            .get_node_type_for_node(ap)
            .map(|t| t.parameters.len())
            .unwrap_or(0);
        assert_eq!(
            pins, 2,
            "apply should be [f, arg0] after loading (callee '{}')",
            callee
        );
        assert_eq!(wires_into(ap, 1), 1, "arg0 wire (callee '{}')", callee);
    }
}

/// A body wire whose types no longer fit (the callee's parameter was retyped
/// in the file) is a repair decision. It used to be dropped when the callee
/// sorted before the caller and kept when it sorted after; the outcome must be
/// the same whatever the names — the repair drops it in both.
#[test]
fn an_incompatible_body_wire_meets_the_same_fate_whichever_way_the_callee_sorts() {
    let mut fates = Vec::new();
    for callee in ["Afoo", "zfoo"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("retyped.cnnd");
        let mut d = new_design(&path);
        edit(
            &mut d,
            callee,
            "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
k = int { value: 1 }
output k
",
        );
        add_map_caller(&mut d, callee);
        save(&mut d);
        assert!(fed_by_zone_input(body_node(&d, "Main", "mp", "g"), 0, 0));

        // The callee's `x` becomes a Structure: `$element` (Int) no longer fits.
        rewrite_json(&path, |file| {
            let net = json_network(file, callee);
            for node in net["nodes"].as_array_mut().unwrap() {
                if node["node_type_name"] == "parameter" {
                    node["data"]["data_type"] = serde_json::Value::from("Structure");
                }
            }
            for p in net["node_type"]["parameters"].as_array_mut().unwrap() {
                p["data_type"] = serde_json::Value::from("Structure");
            }
        });

        let reopened = open(&path);
        fates.push(wires_into(body_node(&reopened, "Main", "mp", "g"), 0));
    }
    assert_eq!(fates[0], fates[1], "Afoo vs zfoo: {:?}", fates);
    assert_eq!(
        fates,
        vec![0, 0],
        "the repair disconnects an incompatible body wire"
    );
}

// ---------------------------------------------------------------------------
// The whole corpus: the load leaves nothing for a repair to finish
// ---------------------------------------------------------------------------

/// Every node's wiring and pin layout, bodies included, keyed by scope.
fn network_fingerprint(network: &NodeNetwork, registry: &NodeTypeRegistry) -> Vec<String> {
    fn walk(net: &NodeNetwork, registry: &NodeTypeRegistry, scope: &str, out: &mut Vec<String>) {
        let mut ids: Vec<&u64> = net.nodes.keys().collect();
        ids.sort();
        for id in ids {
            let node = &net.nodes[id];
            let pins = registry
                .get_node_type_for_node(node)
                .map(|t| {
                    let params: Vec<String> = t
                        .parameters
                        .iter()
                        .map(|p| format!("{}:{}", p.name, p.data_type))
                        .collect();
                    let zone_in: Vec<String> = t
                        .zone_input_pins
                        .iter()
                        .map(|p| format!("{}:{}", p.name, p.data_type))
                        .collect();
                    format!("({}) zone({})", params.join(","), zone_in.join(","))
                })
                .unwrap_or_else(|| "<unresolved>".to_string());
            let wires: Vec<String> = node
                .arguments
                .iter()
                .map(|a| {
                    let mut ws: Vec<String> = a
                        .incoming_wires
                        .iter()
                        .map(|w| {
                            format!(
                                "{}@{}:{:?}",
                                w.source_node_id, w.source_scope_depth, w.source_pin
                            )
                        })
                        .collect();
                    ws.sort();
                    ws.join("+")
                })
                .collect();
            out.push(format!(
                "{}#{} {} {} [{}]",
                scope,
                id,
                node.node_type_name,
                pins,
                wires.join(" | ")
            ));
            if let Some(body) = node.zone.as_ref() {
                walk(body, registry, &format!("{}{}/", scope, id), out);
            }
        }
    }
    let mut out = Vec::new();
    walk(network, registry, "", &mut out);
    out
}

fn corpus_files() -> Vec<std::path::PathBuf> {
    fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "cnnd") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    collect(&atomcad_test_support::fixtures_root(), &mut out);
    collect(&atomcad_test_support::sample_path(""), &mut out);
    collect(
        &Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../..")).join("demolib"),
        &mut out,
    );
    out.sort();
    out
}

/// For every committed `.cnnd` that loads on its own: repairing each loaded
/// network again, against the fully loaded registry, changes nothing. A pass
/// that ran against a half-loaded registry would leave work behind for the
/// second run — a layout not derived, a wire not judged.
#[test]
fn every_corpus_file_loads_to_a_fixed_point_of_repair() {
    let mut loaded = 0;
    let mut failures = Vec::new();
    for path in corpus_files() {
        let mut registry = NodeTypeRegistry::new();
        if load_node_networks_from_file(&mut registry, &path.to_string_lossy()).is_err() {
            continue; // intentionally broken / version-gate fixtures
        }
        loaded += 1;
        let mut names: Vec<String> = registry
            .node_networks
            .keys()
            .filter(|n| registry.library_links.mount_containing(n).is_none())
            .cloned()
            .collect();
        names.sort();
        for name in names {
            let before = network_fingerprint(&registry.node_networks[&name], &registry);
            let mut network = registry.node_networks.remove(&name).unwrap();
            registry.repair_node_network(&mut network);
            registry.node_networks.insert(name.clone(), network);
            let after = network_fingerprint(&registry.node_networks[&name], &registry);
            if before != after {
                let diff: Vec<String> = before
                    .iter()
                    .zip(after.iter())
                    .filter(|(a, b)| a != b)
                    .map(|(a, b)| format!("    {}\n -> {}", a, b))
                    .take(3)
                    .collect();
                failures.push(format!(
                    "{} :: {} ({} vs {} lines)\n{}",
                    path.display(),
                    name,
                    before.len(),
                    after.len(),
                    diff.join("\n")
                ));
            }
        }
    }
    assert!(loaded >= 40, "only {} corpus files loaded", loaded);
    assert!(
        failures.is_empty(),
        "repair after load changed {} network(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// A cycle: no order puts every callee first
// ---------------------------------------------------------------------------

/// `Anet` -> `Bnet` -> `Anet` (a hand-edited reference closes the loop), and
/// `Main`'s body hands `$element` to `Anet`. The load terminates, every
/// network is there once, and the body wire is kept.
#[test]
fn a_file_with_a_network_cycle_loads_and_keeps_body_wires() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cycle.cnnd");
    let mut d = new_design(&path);
    edit(&mut d, "Cnet", "k = int { value: 2 }\noutput k\n");
    edit(
        &mut d,
        "Bnet",
        "c = Cnet {}\nk = int { value: 1 }\noutput k\n",
    );
    edit(
        &mut d,
        "Anet",
        "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
b = Bnet {}
output x
",
    );
    add_map_caller(&mut d, "Anet");
    save(&mut d);
    rewrite_json(&path, |file| {
        retarget_instances(file, "Bnet", "Cnet", "Anet")
    });

    let reopened = open(&path);
    for name in ["Anet", "Bnet", "Cnet", "Main"] {
        assert!(
            reopened.node_type_registry.node_networks.contains_key(name),
            "{} missing",
            name
        );
    }
    assert!(fed_by_zone_input(
        body_node(&reopened, "Main", "mp", "g"),
        0,
        0
    ));
    ok(check_invariants(&reopened));
}

// ---------------------------------------------------------------------------
// Libraries are loaded by the same function
// ---------------------------------------------------------------------------

/// A linked library whose body calls one of its own networks that sorts later
/// keeps the body wire when it is mounted, and on every reopen of the host.
#[test]
fn a_linked_library_keeps_body_wires_into_its_own_later_sorting_network() {
    let dir = tempfile::tempdir().unwrap();
    let lib_path = dir.path().join("lib.cnnd");
    let mut lib = new_design(&lib_path);
    add_int_callee(&mut lib, "zfoo");
    add_map_caller(&mut lib, "zfoo");
    save(&mut lib);

    let host_path = dir.path().join("host.cnnd");
    let mut host = new_design(&host_path);
    host.link_library(&lib_path.to_string_lossy(), "lib")
        .expect("link");
    assert!(fed_by_zone_input(
        body_node(&host, "lib.Main", "mp", "g"),
        0,
        0
    ));
    save(&mut host);

    let reopened = open(&host_path);
    assert!(
        fed_by_zone_input(body_node(&reopened, "lib.Main", "mp", "g"), 0, 0),
        "the library's body wire was dropped on reopen"
    );
}
