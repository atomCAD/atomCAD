//! Undo of an edit that changes a network's parameters must restore the
//! network's callers too (`undo/AGENTS.md` → Known Pitfalls).
//!
//! When a network loses a `parameter`, the call-site repair drops every
//! instance's wire into it — in *other* networks, outside the edited one's
//! undo snapshot. The commands that can remove a parameter record the
//! callers' wiring first (`network_validator::capture_call_sites`) and hand it
//! back on undo; `StructureDesigner::undo` writes it after the refresh. Each
//! test runs the edit through `assert_one_step_round_trip`, which compares
//! every network after the undo and the redo.

use crate::splice_support::assert_one_step_round_trip;
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::structure_designer::StructureDesigner;

/// `inner` takes parameters `p` and `q` (both Int, summed); `outer` calls it
/// at top level (`c`, wired from `x` and `y`) and inside a `map` body (`b`,
/// wired from the element and a capture of `y`). `inner` is active.
fn design() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network("inner");
    d.set_active_node_network_name(Some("inner".to_string()));
    let inner = "p = parameter { param_name: \"p\", data_type: Int, sort_order: 0 }
q = parameter { param_name: \"q\", data_type: Int, sort_order: 1 }
k = int { value: 4 }
e = expr { a: p, b: q, c: k, expression: \"a + b + c\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }, { name: \"c\", data_type: Int }] }
output e
";
    let outcome = d.ai_text_edit(inner, false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    d.add_node_network("outer");
    d.set_active_node_network_name(Some("outer".to_string()));
    let outer = "x = int { value: 1 }
y = int { value: 2 }
c = inner { p: x, q: y }
r = range { count: 3 }
mp = map { xs: r, input_type: Int, output_type: Int, body { b = inner { p: $element, q: ^y } output b } }
";
    let outcome = d.ai_text_edit(outer, false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    d.validate_active_network();
    d.set_active_node_network_name(Some("inner".to_string()));
    d.validate_active_network();
    d
}

fn id_in(network: &NodeNetwork, name: &str) -> u64 {
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node '{}'", name))
        .id
}

fn id(d: &StructureDesigner, network: &str, name: &str) -> u64 {
    id_in(&d.node_type_registry.node_networks[network], name)
}

/// The sources wired into each input of the `outer` instances `c` and the
/// body's `b`, as `(source id, depth)`.
fn caller_wiring(d: &StructureDesigner) -> Vec<Vec<(u64, u8)>> {
    let outer = &d.node_type_registry.node_networks["outer"];
    let mp = id_in(outer, "mp");
    let body = outer.nodes[&mp].zone.as_deref().unwrap();
    [
        &outer.nodes[&id_in(outer, "c")],
        &body.nodes[&id_in(body, "b")],
    ]
    .iter()
    .flat_map(|node| {
        node.arguments.iter().map(|argument| {
            argument
                .incoming_wires
                .iter()
                .map(|w| (w.source_node_id, w.source_scope_depth))
                .collect()
        })
    })
    .collect()
}

#[test]
fn deleting_a_parameter_and_undoing_restores_the_callers_wires() {
    let mut d = design();
    let before = caller_wiring(&d);
    assert!(before.iter().all(|wires| wires.len() == 1), "{before:?}");

    // Delete `p`, the first of two parameters: the callers' `q` wires move
    // down a slot, and their `p` wires are dropped.
    let p = id(&d, "inner", "p");
    d.select_node(p);
    assert_one_step_round_trip(&mut d, |d| d.delete_selected());
    assert_eq!(caller_wiring(&d).len(), 2, "one slot left on each caller");

    assert!(d.undo());
    assert_eq!(caller_wiring(&d), before);
}

#[test]
fn the_undo_restores_the_callers_with_another_network_active() {
    let mut d = design();
    let before = caller_wiring(&d);
    let p = id(&d, "inner", "p");
    d.select_node(p);
    d.delete_selected();

    d.set_active_node_network_name(Some("outer".to_string()));
    assert!(d.undo());
    assert_eq!(caller_wiring(&d), before);
}

#[test]
fn an_ai_edit_that_deletes_a_parameter_undoes_its_callers_wires_too() {
    let mut d = design();
    let before = caller_wiring(&d);
    assert_one_step_round_trip(&mut d, |d| {
        let outcome = d.ai_text_edit("delete q\n", false);
        assert!(
            outcome.result.errors.is_empty(),
            "{:?}",
            outcome.result.errors
        );
    });
    assert!(d.undo());
    assert_eq!(caller_wiring(&d), before);
}

#[test]
fn dissolving_a_parameter_undoes_its_callers_wires_too() {
    let mut d = design();
    // Give `q` a default so it has an input wire to dissolve onto.
    let outcome = d.ai_text_edit(
        "dq = int { value: 7 }\nq = parameter { default: dq }\n",
        false,
    );
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    let before = caller_wiring(&d);
    let q = id(&d, "inner", "q");
    assert_one_step_round_trip(&mut d, |d| d.dissolve_node(&[], q).unwrap());
    assert!(d.undo());
    assert_eq!(caller_wiring(&d), before);
}

#[test]
fn undoing_a_promote_takes_the_new_slot_away_from_the_callers() {
    let mut d = design();
    let before = caller_wiring(&d);
    let k = id(&d, "inner", "k");
    assert_one_step_round_trip(&mut d, |d| d.promote_node_to_parameter(k).unwrap());
    assert!(d.undo());
    assert_eq!(
        caller_wiring(&d),
        before,
        "no empty third slot is left behind"
    );
}

/// The capture records exactly the instances the call-site repair remaps.
#[test]
fn the_capture_finds_every_instance_with_its_scope() {
    let d = design();
    let sites = atomcad_structure_designer::network_validator::capture_call_sites(
        &d.node_type_registry,
        "inner",
    );
    let outer = &d.node_type_registry.node_networks["outer"];
    let mp = id_in(outer, "mp");
    let mut found: Vec<(Vec<u64>, usize)> = sites
        .iter()
        .map(|site| {
            assert_eq!(site.host_network, "outer");
            (site.scope_path.clone(), site.arguments.len())
        })
        .collect();
    found.sort();
    assert_eq!(found, [(vec![], 2), (vec![mp], 2)]);
    let element = sites
        .iter()
        .find(|site| site.scope_path == [mp])
        .unwrap()
        .arguments[0]
        .incoming_wires[0]
        .source_pin;
    assert_eq!(element, SourcePin::ZoneInput { pin_index: 0 });
}
