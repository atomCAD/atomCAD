//! Insert and dissolve against real drawings
//! (`doc/design_insert_node_on_wire.md` §Testing, the corpus run).
//!
//! Hand-written fixtures cover the shapes someone thought of. This covers
//! the rest: every wire of every network in the corpus, every type the
//! insertable-types query offers there, and real overlaps. It checks
//! plan/commit agreement across every node type — a planned type must commit,
//! and `ResultMismatch` (the scratch network and the live one disagreeing)
//! must never occur. Every node a dissolve plan accepts must dissolve, and
//! dissolving a node right after inserting it must give back the wiring the
//! insert started from.
//!
//! The corpus is `demolib/baselib_with_demos.cnnd`, loaded through the real
//! loader as `layout_corpus_test.rs` does, or the file `ATOMCAD_LAYOUT_CORPUS`
//! names. A failure names its seed and triple. Reproduce it as a named test
//! in `insert_node_on_wire_test.rs` or `dissolve_node_test.rs` before fixing
//! it: this file is a net, not a workbench.

use super::layout_corpus_test::{Rng, demolib_path, load, private_corpus_path};
use super::layout_oracle::{Drawing, check_bodies, check_no_new_overlap, check_no_wire_flipped};
use super::splice_support::{assert_no_mutation, assert_one_step_round_trip, json_diff};
use atomcad_structure_designer::node_network::{ArgumentKind, NodeNetwork, Wire, WireAnchor};
use atomcad_structure_designer::serialization::node_networks_serialization::node_network_to_serializable;
use atomcad_structure_designer::splice_ops::{DissolveRefusal, NodeSource};
use atomcad_structure_designer::splice_plan::Arrangement;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use std::collections::HashSet;

/// How many `(wire, type, arrangement)` triples are committed.
const SAMPLE: usize = 150;
const SEED: u64 = 0x5_1ce;

/// One wire of the corpus: its network, storage scope and address, and the
/// scope the "click" lands in.
#[derive(Clone, Debug)]
struct CorpusWire {
    network: String,
    wire_scope: Vec<u64>,
    wire: WireAnchor,
    clicked: Vec<u64>,
}

/// Every wire stored in `network` or any body inside it, each clicked
/// either in its storage scope or in the destination's effective scope
/// (they differ for a body-return wire).
fn wires_of(network_name: &str, network: &NodeNetwork, rng: &mut Rng) -> Vec<CorpusWire> {
    fn walk(
        network_name: &str,
        network: &NodeNetwork,
        scope: &mut Vec<u64>,
        rng: &mut Rng,
        out: &mut Vec<CorpusWire>,
    ) {
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in &ids {
            let node = &network.nodes[id];
            let lists = [
                (ArgumentKind::External, &node.arguments),
                (ArgumentKind::ZoneOutput, &node.zone_output_arguments),
            ];
            for (kind, arguments) in lists {
                for (index, argument) in arguments.iter().enumerate() {
                    for incoming in &argument.incoming_wires {
                        let wire = Wire {
                            source_node_id: incoming.source_node_id,
                            source_pin: incoming.source_pin,
                            source_scope_depth: incoming.source_scope_depth,
                            destination_node_id: *id,
                            destination_argument_index: index,
                            destination_argument_kind: kind,
                        };
                        let mut clicked = scope.clone();
                        if kind == ArgumentKind::ZoneOutput || rng.below(2) == 0 {
                            // Somewhere on the destination's chain.
                            if kind == ArgumentKind::ZoneOutput {
                                clicked.push(*id);
                            }
                        } else {
                            // Shallower: clamped by D6 when it must be.
                            clicked.truncate(rng.below(clicked.len() + 1));
                        }
                        out.push(CorpusWire {
                            network: network_name.to_string(),
                            wire_scope: scope.clone(),
                            wire: WireAnchor::from_wire(&wire, network),
                            clicked,
                        });
                    }
                }
            }
        }
        for id in ids {
            if let Some(body) = network.nodes[&id].zone.as_deref() {
                scope.push(id);
                walk(network_name, body, scope, rng, out);
                scope.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(network_name, network, &mut Vec::new(), rng, &mut out);
    out
}

fn activate(d: &mut StructureDesigner, network: &str) {
    if d.active_node_network_name.as_deref() != Some(network) {
        d.set_active_node_network_name(Some(network.to_string()));
    }
}

fn run_insert_half(d: &mut StructureDesigner, seed: u64) {
    let mut rng = Rng::new(seed);
    let mut names: Vec<String> = d
        .node_type_registry
        .node_networks
        .iter()
        .filter(|(_, network)| network.nodes.len() >= 2)
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();

    // Plan every wire; gather every offered triple.
    let mut triples: Vec<(CorpusWire, String, Arrangement)> = Vec::new();
    let mut wire_count = 0;
    for name in &names {
        activate(d, name);
        let wires = wires_of(name, &d.node_type_registry.node_networks[name], &mut rng);
        wire_count += wires.len();
        for wire in wires {
            let offered = d
                .insertable_node_type_names(&wire.wire_scope, &wire.wire, &wire.clicked)
                .unwrap_or_else(|refusal| panic!("{wire:?}: refused with {refusal}"));
            for type_name in offered {
                let plan = d
                    .plan_insert_on_wire(
                        &wire.wire_scope,
                        &wire.wire,
                        &wire.clicked,
                        &NodeSource::New(type_name.clone()),
                    )
                    .unwrap();
                assert!(!plan.is_empty(), "{wire:?} offers {type_name} with no plan");
                for arrangement in plan {
                    triples.push((wire.clone(), type_name.clone(), arrangement));
                }
            }
        }
    }
    assert!(wire_count > 100, "the corpus has only {wire_count} wires");
    println!(
        "insert corpus (seed {seed:#x}): {wire_count} wires, {} triples, committing {}",
        triples.len(),
        SAMPLE.min(triples.len())
    );

    // Commit a seeded sample, with the oracles, and undo each.
    for _ in 0..SAMPLE.min(triples.len()) {
        let (wire, type_name, arrangement) = triples.swap_remove(rng.below(triples.len()));
        let context = format!("seed {seed:#x}: {type_name} on {wire:?} as {arrangement:?}");
        // Printed first, so a panic inside an oracle still names its triple
        // (under `--nocapture`).
        println!("commit: {context}");
        activate(d, &wire.network);
        let node = NodeSource::New(type_name.clone());

        let replanned = assert_no_mutation(d, |d| {
            d.plan_insert_on_wire(&wire.wire_scope, &wire.wire, &wire.clicked, &node)
        })
        .unwrap();
        assert!(
            replanned.contains(&arrangement),
            "{context}: the plan changed"
        );

        let before = Drawing::of(
            &d.node_type_registry.node_networks[&wire.network],
            &d.node_type_registry,
        );
        let nodes_before = nodes_of(&d.node_type_registry.node_networks[&wire.network]);
        let wiring_before = wiring(d, &wire.network);
        assert_one_step_round_trip(d, |d| {
            d.insert_node_on_wire(
                &wire.wire_scope,
                &wire.wire,
                &wire.clicked,
                &node,
                &arrangement,
            )
            .unwrap_or_else(|refusal| panic!("{context}: {refusal:?}"))
        });
        let after = Drawing::of(
            &d.node_type_registry.node_networks[&wire.network],
            &d.node_type_registry,
        );
        check_no_new_overlap(&before, &after);
        check_no_wire_flipped(&before, &after);
        check_bodies(&after);

        // Round trip: dissolving the inserted node gives the wiring back.
        let added: Vec<(Vec<u64>, u64)> =
            nodes_of(&d.node_type_registry.node_networks[&wire.network])
                .difference(&nodes_before)
                .cloned()
                .collect();
        assert_eq!(added.len(), 1, "{context}: one node added");
        let (scope, inserted) = &added[0];
        d.dissolve_node(scope, *inserted)
            .unwrap_or_else(|refusal| panic!("{context}: dissolve: {refusal:?}"));
        let wiring_after = wiring(d, &wire.network);
        assert!(
            wiring_before == wiring_after,
            "{context}: dissolve after insert differs at {:?}",
            json_diff(&wiring_before, &wiring_after)
        );
        assert!(d.undo(), "{context}: undo the dissolve");
        assert!(d.undo(), "{context}: undo");
    }
}

/// Every node of `network` and of every body inside it, as `(scope, id)`.
fn nodes_of(network: &NodeNetwork) -> HashSet<(Vec<u64>, u64)> {
    fn walk(network: &NodeNetwork, scope: &mut Vec<u64>, out: &mut HashSet<(Vec<u64>, u64)>) {
        for (id, node) in &network.nodes {
            out.insert((scope.clone(), *id));
            if let Some(body) = node.zone.as_deref() {
                scope.push(*id);
                walk(body, scope, out);
                scope.pop();
            }
        }
    }
    let mut out = HashSet::new();
    walk(network, &mut Vec::new(), &mut out);
    out
}

/// The serialized network without what an insert may move and a dissolve
/// does not move back: node positions, body sizes and the id counters.
fn wiring(d: &mut StructureDesigner, network: &str) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for key in [
                    "position",
                    "next_node_id",
                    "next_param_id",
                    "body_width",
                    "body_height",
                ] {
                    map.remove(key);
                }
                map.values_mut().for_each(strip);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let registry = &mut d.node_type_registry;
    let serializable = node_network_to_serializable(
        registry.node_networks.get_mut(network).unwrap(),
        &registry.built_in_node_types,
        None,
        false,
    )
    .unwrap();
    let mut value = serde_json::to_value(serializable).unwrap();
    strip(&mut value);
    value
}

/// Every node of every network (body nodes included) through
/// `plan_dissolve_node`; each accepted one is dissolved, with the oracles,
/// and undone.
fn run_dissolve_half(d: &mut StructureDesigner) {
    let mut names: Vec<String> = d.node_type_registry.node_networks.keys().cloned().collect();
    names.sort();
    let (mut planned, mut accepted) = (0, 0);
    for name in &names {
        activate(d, name);
        let mut nodes: Vec<(Vec<u64>, u64)> = nodes_of(&d.node_type_registry.node_networks[name])
            .into_iter()
            .collect();
        nodes.sort();
        for (scope, node_id) in nodes {
            let context = format!("dissolve {node_id} in {name}{scope:?}");
            planned += 1;
            let plan = assert_no_mutation(d, |d| d.plan_dissolve_node(&scope, node_id));
            match plan {
                Err(DissolveRefusal::NodeNotFound) => panic!("{context}: NodeNotFound"),
                Err(_) => continue,
                Ok(_) => {}
            }
            accepted += 1;
            println!("{context}");
            assert_one_step_round_trip(d, |d| {
                d.dissolve_node(&scope, node_id)
                    .unwrap_or_else(|refusal| panic!("{context}: {refusal:?}"))
            });
            assert!(d.undo(), "{context}: undo");
        }
    }
    assert!(accepted > 20, "only {accepted} of {planned} nodes dissolve");
    println!("dissolve corpus: {planned} nodes planned, {accepted} dissolved");
}

#[test]
fn insert_over_the_demolib_corpus() {
    let mut d = load(&demolib_path());
    run_insert_half(&mut d, SEED);
}

#[test]
fn dissolve_over_the_demolib_corpus() {
    let mut d = load(&demolib_path());
    run_dissolve_half(&mut d);
}

#[test]
fn dissolve_over_the_private_corpus() {
    let Some(path) = private_corpus_path() else {
        return;
    };
    let mut d = load(&path);
    run_dissolve_half(&mut d);
}

#[test]
fn insert_over_the_private_corpus() {
    let Some(path) = private_corpus_path() else {
        return;
    };
    let mut d = load(&path);
    run_insert_half(&mut d, SEED);
}
