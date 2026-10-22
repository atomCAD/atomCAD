//! Phase 1 of `doc/design_output_driven_editing.md`: the trace engine, the
//! trait's default and the fallback.
//!
//! Engine behaviour that needs a node to *return* something uses the test-only
//! `ScriptedTraceNode`; fallback tests use the test-only `UntracedKeepNode` /
//! `UntracedMoveNode` / `UntracedFillNode`, so no node gaining a `trace` can
//! change their outcome (they used `tag` until it gained one,
//! `doc/design_trace_coverage.md` Phase 3).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use atomcad_geo_tree::GeoNode;
use atomcad_structure_designer::data_type::{DataType, FunctionType, RecordType};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::nodes::collect::CollectData;
use atomcad_structure_designer::nodes::fold::FoldData;
use atomcad_structure_designer::nodes::range::RangeData;
use atomcad_structure_designer::nodes::record_construct::RecordConstructData;
use atomcad_structure_designer::nodes::record_destructure::RecordDestructureData;
use atomcad_structure_designer::nodes::sequence::SequenceData;
use atomcad_structure_designer::trace::engine::{ROW_CAP, may_carry_phase};
use atomcad_structure_designer::trace::{
    Contribution, TraceHit, TraceHitTarget, TraceLink, TraceProbe, TraceStep,
};
use glam::f64::DVec3;

use crate::trace_support::*;

const ORIGIN: DVec3 = DVec3::ZERO;

fn sphere(center: DVec3, radius: f64) -> GeoNode {
    GeoNode::sphere(center, radius)
}

// ============================================================================
// Origins
// ============================================================================

#[test]
fn a_blueprint_leaf_is_an_origin_labelled_shape() {
    let mut d = setup();
    let ball = add_value(&mut d, blueprint(sphere(ORIGIN, 2.0)), "ball");
    let hit = TraceHit {
        node: NodeRef::top(ball),
        pin: 0,
        target: TraceHitTarget::Surface {
            point: DVec3::new(0.0, 0.0, -2.0),
        },
    };
    assert_eq!(trace(&mut d, &hit), "ball shape");
}

#[test]
fn an_atomic_leaf_is_an_origin_labelled_source() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    assert_eq!(trace(&mut d, &atom_hit(mol, ORIGIN)), "mol source");

    let mut d = setup();
    let xtal = add_value(&mut d, crystal(&[(6, ORIGIN)], None), "xtal");
    assert_eq!(trace(&mut d, &atom_hit(xtal, ORIGIN)), "xtal source");
}

// ============================================================================
// Fallback
// ============================================================================

#[test]
fn fallback_kept_an_untraced_node_that_does_not_move_atoms_keeps_the_probe() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let keep = add(&mut d, "untraced_keep", "keep");
    d.connect_nodes(mol, 0, keep, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(keep, ORIGIN)),
        "keep possibly contributed\nmol source"
    );
}

#[test]
fn fallback_lost_an_untraced_node_that_moves_atoms_loses_the_probe() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let (s, s_calls) = add_scripted(&mut d, "s", pass_by_position());
    let mover = add(&mut d, "untraced_move", "move");
    d.connect_nodes(mol, 0, s, 0);
    d.connect_nodes(s, 0, mover, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(mover, UNTRACED_MOVE)),
        "move possibly contributed\ns wired upstream\nmol wired upstream"
    );
    assert_eq!(
        s_calls.load(Ordering::SeqCst),
        0,
        "a node upstream of a lost probe must not be asked to trace"
    );
}

#[test]
fn fallback_converted_an_atom_probe_becomes_a_point_on_a_blueprint_input() {
    let mut d = setup();
    let a = add_value(&mut d, blueprint(sphere(ORIGIN, 2.0)), "a");
    let b = add_value(
        &mut d,
        blueprint(sphere(DVec3::new(10.0, 0.0, 0.0), 2.0)),
        "b",
    );
    let union = add(&mut d, "union", "union");
    d.connect_nodes(a, 0, union, 0);
    d.connect_nodes(b, 0, union, 0);
    let fill = add(&mut d, "untraced_fill", "fill");
    d.connect_nodes(union, 0, fill, 0);
    FILL_POSITION.set(ORIGIN);

    let rendered = trace(&mut d, &atom_hit(fill, ORIGIN));
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines.len(), 3, "{rendered}");
    assert_eq!(lines[0], "fill possibly contributed");
    // `union` is still a fallback node in Phase 1 and gets its own `trace` in
    // Phase 4; either way it keeps only the shape that contains the point.
    assert!(lines[1].starts_with("union "), "{rendered}");
    assert_eq!(lines[2], "a shape");
}

/// A `ScriptedTraceNode` that hands a `Point` at the origin to its input 0.
fn point_at_origin_script() -> Script {
    Arc::new(|_, _, _, _| {
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![TraceLink {
                pin: 0,
                element: None,
                probe: TraceProbe::Point {
                    position: ORIGIN,
                    on_surface: false,
                    delta: 1.0,
                },
            }],
        })
    })
}

#[test]
fn fallback_converted_a_point_is_kept_by_a_crystal_geometry_shell() {
    let far = DVec3::new(20.0, 0.0, 0.0);
    let mut d = setup();
    let xtal = add_value(
        &mut d,
        crystal(&[(6, far)], Some(sphere(ORIGIN, 3.0))),
        "xtal",
    );
    let keep = add(&mut d, "untraced_keep", "keep");
    let (s, _) = add_scripted(&mut d, "s", point_at_origin_script());
    d.connect_nodes(xtal, 0, keep, 0);
    d.connect_nodes(keep, 0, s, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(s, far)),
        "s passed\nkeep possibly contributed\nxtal source"
    );
}

#[test]
fn fallback_converted_a_point_is_lost_without_a_geometry_shell() {
    let far = DVec3::new(20.0, 0.0, 0.0);
    let mut d = setup();
    let xtal = add_value(&mut d, crystal(&[(6, far)], None), "xtal");
    let keep = add(&mut d, "untraced_keep", "keep");
    let (s, _) = add_scripted(&mut d, "s", point_at_origin_script());
    d.connect_nodes(xtal, 0, keep, 0);
    d.connect_nodes(keep, 0, s, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(s, far)),
        "s passed\nkeep possibly contributed\nxtal wired upstream"
    );
}

// ============================================================================
// Array elements and match by position
// ============================================================================

#[test]
fn an_array_element_lands_on_the_wire_that_delivered_it() {
    let a_atom = DVec3::new(0.0, 0.0, 0.0);
    // (expected source, expected element at that source) per merged index.
    let expected: [(&str, Option<usize>); 4] = [
        ("a", None),
        ("pair", Some(0)),
        ("pair", Some(1)),
        ("c", None),
    ];

    for (i, (want_source, want_element)) in expected.iter().enumerate() {
        let mut d = setup();
        // Created in id order, which is the merge order.
        let (a, a_rec) = add_recording_source(&mut d, "a", vec![vec![(6, a_atom)]], false);
        let (pair, pair_rec) = add_recording_source(
            &mut d,
            "pair",
            vec![
                vec![(6, DVec3::new(5.0, 0.0, 0.0))],
                vec![(6, DVec3::new(10.0, 0.0, 0.0))],
            ],
            true,
        );
        let (c, c_rec) = add_recording_source(
            &mut d,
            "c",
            vec![vec![(6, DVec3::new(15.0, 0.0, 0.0))]],
            false,
        );
        let script: Script = Arc::new(move |_, _, probe, _| {
            Some(TraceStep {
                contribution: Contribution::Passed,
                upstream: vec![TraceLink {
                    pin: 2,
                    element: Some(i),
                    probe: probe.clone(),
                }],
            })
        });
        let (s, _) = add_scripted(&mut d, "s", script);
        for source in [a, pair, c] {
            d.connect_nodes(source, 0, s, 2);
        }

        let rendered = trace(&mut d, &atom_hit(s, a_atom));
        assert_eq!(rendered, format!("s passed\n{want_source} source"));
        let records = [("a", &a_rec), ("pair", &pair_rec), ("c", &c_rec)];
        for (label, record) in records {
            let got = record.lock().unwrap().clone();
            if label == *want_source {
                assert_eq!(got, vec![*want_element], "element {i} at {label}");
            } else {
                assert!(got.is_empty(), "element {i} must not reach {label}");
            }
        }
    }
}

#[test]
fn a_match_by_position_link_is_dropped_where_nothing_matches() {
    let p = DVec3::new(1.0, 2.0, 3.0);
    let mut d = setup();
    let has = add_value(&mut d, molecule(&[(6, p)]), "has");
    let other = add_value(&mut d, molecule(&[(6, DVec3::new(9.0, 9.0, 9.0))]), "other");
    let script: Script = Arc::new(|_, _, probe, _| {
        let by_position = TraceProbe::Atom {
            id: None,
            position: probe.position(),
        };
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![
                TraceLink {
                    pin: 0,
                    element: None,
                    probe: by_position.clone(),
                },
                TraceLink {
                    pin: 1,
                    element: None,
                    probe: by_position,
                },
            ],
        })
    });
    let (s, _) = add_scripted(&mut d, "s", script);
    d.connect_nodes(has, 0, s, 0);
    d.connect_nodes(other, 0, s, 1);

    assert_eq!(trace(&mut d, &atom_hit(s, p)), "s passed\nhas source");
}

// ============================================================================
// Custom nodes
// ============================================================================

/// Build custom network `name`: `parameter` (Molecule) → `ScriptedTraceNode`
/// passing by position → return. Leaves `main` active again.
fn build_inner(
    d: &mut atomcad_structure_designer::structure_designer::StructureDesigner,
    name: &str,
) {
    d.add_node_network(name);
    d.set_active_node_network_name(Some(name.to_string()));
    let p = add(d, "parameter", &format!("p_{name}"));
    configure_parameter(d, p, "mol", DataType::Molecule);
    let (s, _) = add_scripted(d, &format!("s_{name}"), pass_by_position());
    d.connect_nodes(p, 0, s, 0);
    d.set_return_node_id(Some(s));
    d.validate_active_network();
    assert!(
        d.node_type_registry.node_networks.get(name).unwrap().valid,
        "{name} must be valid"
    );
    d.set_active_node_network_name(Some(MAIN.to_string()));
}

/// Build custom network `outer`: `parameter` (Molecule) → `inner` instance →
/// return.
fn build_outer(d: &mut atomcad_structure_designer::structure_designer::StructureDesigner) {
    d.add_node_network("outer");
    d.set_active_node_network_name(Some("outer".to_string()));
    let p = add(d, "parameter", "p_outer");
    configure_parameter(d, p, "mol", DataType::Molecule);
    let inner = add(d, "inner", "inner");
    d.connect_nodes(p, 0, inner, 0);
    d.set_return_node_id(Some(inner));
    d.validate_active_network();
    assert!(
        d.node_type_registry
            .node_networks
            .get("outer")
            .unwrap()
            .valid
    );
    d.set_active_node_network_name(Some(MAIN.to_string()));
}

#[test]
fn a_custom_node_is_entered_and_left_through_its_parameter() {
    let mut d = setup();
    build_inner(&mut d, "inner");
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let instance = add(&mut d, "inner", "inner");
    d.connect_nodes(mol, 0, instance, 0);

    let report = d.trace_hit(&atom_hit(instance, ORIGIN));
    assert_eq!(
        render(&d, &report),
        "inner passed\n  s_inner passed\n  p_inner passed\nmol source"
    );

    // The instance row stays in the viewed network; the inner rows carry the
    // custom network and the instance frame.
    let instance_row = &report.rows[0];
    assert_eq!(instance_row.row.network, MAIN);
    assert!(instance_row.row.frame.is_empty());
    let frame = vec![(MAIN.to_string(), NodeRef::top(instance))];
    for child in &instance_row.children {
        assert_eq!(child.row.network, "inner");
        assert_eq!(child.row.frame, frame);
    }
    assert_eq!(report.rows[1].row.network, MAIN);
    assert!(report.rows[1].row.frame.is_empty());
}

#[test]
fn an_instance_row_takes_the_strongest_contribution_of_its_children() {
    let mut d = setup();
    d.add_node_network("maker");
    d.set_active_node_network_name(Some("maker".to_string()));
    let made = add_value(&mut d, molecule(&[(6, ORIGIN)]), "made");
    d.set_return_node_id(Some(made));
    d.validate_active_network();
    d.set_active_node_network_name(Some(MAIN.to_string()));
    let instance = add(&mut d, "maker", "maker");

    assert_eq!(
        trace(&mut d, &atom_hit(instance, ORIGIN)),
        "maker source\n  made source"
    );
}

#[test]
fn a_nested_custom_node_nests_one_level_further() {
    let mut d = setup();
    build_inner(&mut d, "inner");
    build_outer(&mut d);
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let instance = add(&mut d, "outer", "outer");
    d.connect_nodes(mol, 0, instance, 0);

    let report = d.trace_hit(&atom_hit(instance, ORIGIN));
    assert_eq!(
        render(&d, &report),
        "outer passed\n  inner passed\n    s_inner passed\n    p_inner passed\n  p_outer passed\nmol source"
    );
    let inner_instance = &report.rows[0].children[0];
    let inner_rows = &inner_instance.children;
    assert_eq!(inner_rows[0].row.frame.len(), 2);
    assert_eq!(inner_rows[0].row.frame[0].0, MAIN);
    assert_eq!(inner_rows[0].row.frame[1].0, "outer");
}

// ============================================================================
// Dedup
// ============================================================================

#[test]
fn a_diamond_lists_each_node_once() {
    let mut d = setup();
    let leaf = add_value(&mut d, molecule(&[(6, ORIGIN)]), "leaf");
    let (s1, _) = add_scripted(&mut d, "s1", pass_by_position());
    let (s2, _) = add_scripted(&mut d, "s2", pass_by_position());
    let both: Script = Arc::new(|_, _, probe, _| {
        let by_position = TraceProbe::Atom {
            id: None,
            position: probe.position(),
        };
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![
                TraceLink {
                    pin: 0,
                    element: None,
                    probe: by_position.clone(),
                },
                TraceLink {
                    pin: 1,
                    element: None,
                    probe: by_position,
                },
            ],
        })
    });
    let (s3, _) = add_scripted(&mut d, "s3", both);
    d.connect_nodes(leaf, 0, s1, 0);
    d.connect_nodes(leaf, 0, s2, 0);
    d.connect_nodes(s1, 0, s3, 0);
    d.connect_nodes(s2, 0, s3, 1);

    assert_eq!(
        trace(&mut d, &atom_hit(s3, ORIGIN)),
        "s3 passed\ns1 passed\nleaf source\ns2 passed"
    );
}

#[test]
fn two_instances_of_one_custom_network_each_get_their_own_subtree() {
    let mut d = setup();
    build_inner(&mut d, "inner");
    let leaf = add_value(&mut d, molecule(&[(6, ORIGIN)]), "leaf");
    let first = add(&mut d, "inner", "first");
    let second = add(&mut d, "inner", "second");
    d.connect_nodes(leaf, 0, first, 0);
    d.connect_nodes(first, 0, second, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(second, ORIGIN)),
        "second passed\n  s_inner passed\n  p_inner passed\n\
         first passed\n  s_inner passed\n  p_inner passed\nleaf source"
    );
}

#[test]
fn a_custom_network_node_is_not_deduped_against_a_parent_node_with_the_same_id() {
    let mut d = setup();
    build_inner(&mut d, "inner");
    let leaf = add_value(&mut d, molecule(&[(6, ORIGIN)]), "leaf");
    let instance = add(&mut d, "inner", "inner");
    d.connect_nodes(leaf, 0, instance, 0);

    // Node ids restart in every network: the inner parameter and the outer
    // leaf share an id, and both must be listed.
    let inner_param_id = d
        .node_type_registry
        .node_networks
        .get("inner")
        .unwrap()
        .nodes
        .values()
        .find(|n| n.node_type_name == "parameter")
        .unwrap()
        .id;
    assert_eq!(inner_param_id, leaf, "the test relies on the ids colliding");

    let report = d.trace_hit(&atom_hit(instance, ORIGIN));
    let rendered = render(&d, &report);
    assert!(rendered.contains("p_inner passed"), "{rendered}");
    assert!(rendered.contains("leaf source"), "{rendered}");
}

// ============================================================================
// Opaque constructs
// ============================================================================

#[test]
fn a_hof_on_the_path_is_treated_as_a_node_without_trace() {
    let mut d = setup();
    let range = add(&mut d, "range", "range");
    set_node_data(
        &mut d,
        MAIN,
        range,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 2,
        }),
    );
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let fold = add(&mut d, "fold", "fold");
    set_node_data(
        &mut d,
        MAIN,
        fold,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Molecule,
        }),
    );
    d.connect_nodes(range, 0, fold, 0);
    d.connect_nodes(mol, 0, fold, 1);
    // Body: acc → tag → new_acc.
    let body_tag = add_to_body(&mut d, fold, "tag");
    wire_zone_input(&mut d, fold, 0, body_tag, 0);
    wire_zone_output(&mut d, fold, body_tag);
    d.validate_active_network();

    // The fold is listed without entering its body, and the probe continues
    // through its phase-typed `init` input by position.
    assert_eq!(
        trace(&mut d, &atom_hit(fold, ORIGIN)),
        "fold possibly contributed\nmol source"
    );
}

// ============================================================================
// Row cap
// ============================================================================

#[test]
fn a_trace_longer_than_the_cap_ends_in_a_more_row() {
    // Wide rather than deep: a chain this long would overflow the
    // evaluator's own recursion in a debug test thread before the trace ran.
    let mut d = setup();
    let fan_in: Script = Arc::new(|_, _, probe, _| {
        Some(TraceStep {
            contribution: Contribution::Passed,
            // No element: every element of the array pin.
            upstream: vec![TraceLink {
                pin: 2,
                element: None,
                probe: probe.clone(),
            }],
        })
    });
    let (s, _) = add_scripted(&mut d, "s", fan_in);
    for i in 0..ROW_CAP + 5 {
        let leaf = add_value(&mut d, molecule(&[(6, ORIGIN)]), &format!("leaf{i}"));
        d.connect_nodes(leaf, 0, s, 2);
    }

    let report = d.trace_hit(&atom_hit(s, ORIGIN));
    assert!(report.truncated);
    assert_eq!(report.rows.len(), ROW_CAP);
    assert!(render(&d, &report).ends_with(
        "
… more"
    ));
}

// ============================================================================
// Start of trace
// ============================================================================

#[test]
fn an_atom_hit_is_re_resolved_by_position_against_the_real_result() {
    let p = DVec3::new(1.0, 0.0, 0.0);
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN), (6, p)]), "mol");
    let seen = Arc::new(std::sync::Mutex::new(None));
    let seen_in_script = seen.clone();
    let script: Script = Arc::new(move |_, _, probe, _| {
        *seen_in_script.lock().unwrap() = Some(probe.clone());
        None
    });
    let (s, _) = add_scripted(&mut d, "s", script);
    d.connect_nodes(mol, 0, s, 0);

    // A display-side position a little off the atom still resolves to it.
    trace(&mut d, &atom_hit(s, p + DVec3::new(0.0, 0.05, 0.0)));
    let probe = seen.lock().unwrap().clone().unwrap();
    let output = mol_output_ids(&mut d, s);
    assert_eq!(
        probe,
        TraceProbe::Atom {
            id: Some(output[1]),
            position: p
        }
    );
}

/// The ids of the atoms in `node`'s real output, in insertion order.
fn mol_output_ids(
    d: &mut atomcad_structure_designer::structure_designer::StructureDesigner,
    node: u64,
) -> Vec<u32> {
    let (ids, _) = d.with_trace_eval_context(|evaluator, registry, context| {
        use atomcad_structure_designer::evaluator::network_evaluator::NetworkStackElement;
        let network = registry.node_networks.get(MAIN).unwrap();
        let stack = vec![NetworkStackElement::root(network)];
        match evaluator.evaluate(&stack, node, 0, registry, false, context) {
            NetworkResult::Molecule(m) => {
                let mut ids: Vec<u32> = m.atoms.atoms_values().map(|a| a.id).collect();
                ids.sort();
                ids
            }
            _ => panic!("expected a molecule"),
        }
    });
    ids
}

#[test]
fn a_bond_hit_is_traced_from_the_endpoint_nearer_the_hit_point() {
    let a = ORIGIN;
    let b = DVec3::new(1.5, 0.0, 0.0);
    for (point, want) in [
        (DVec3::new(0.4, 0.0, 0.0), a),
        (DVec3::new(1.1, 0.0, 0.0), b),
    ] {
        let mut d = setup();
        let mol = add_value(&mut d, molecule(&[(6, a), (6, b)]), "mol");
        let seen = Arc::new(std::sync::Mutex::new(None));
        let seen_in_script = seen.clone();
        let script: Script = Arc::new(move |_, _, probe, _| {
            *seen_in_script.lock().unwrap() = Some(probe.position());
            None
        });
        let (s, _) = add_scripted(&mut d, "s", script);
        d.connect_nodes(mol, 0, s, 0);
        let hit = TraceHit {
            node: NodeRef::top(s),
            pin: 0,
            target: TraceHitTarget::Bond {
                position1: a,
                position2: b,
                point,
            },
        };
        trace(&mut d, &hit);
        assert_eq!(seen.lock().unwrap().unwrap(), want);
    }
}

// ============================================================================
// Context and cost
// ============================================================================

#[test]
fn a_node_can_hold_its_input_and_output_at_once() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let script: Script = Arc::new(|_, _, probe, ctx| {
        let input = ctx.input(0);
        let output = ctx.output(0);
        assert!(matches!(input.as_ref(), NetworkResult::Molecule(_)));
        assert!(matches!(output.as_ref(), NetworkResult::Molecule(_)));
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![TraceLink {
                pin: 0,
                element: None,
                probe: TraceProbe::Atom {
                    id: None,
                    position: probe.position(),
                },
            }],
        })
    });
    let (s, _) = add_scripted(&mut d, "s", script);
    d.connect_nodes(mol, 0, s, 0);
    assert_eq!(trace(&mut d, &atom_hit(s, ORIGIN)), "s passed\nmol source");
}

#[test]
fn every_value_is_fetched_once_and_every_node_evaluated_once_per_trace() {
    const CHAIN: usize = 4;
    let mut d = setup();
    let mut previous = add(&mut d, "counting_leaf", "leaf");
    let rereading: Script = Arc::new(|_, _, probe, ctx| {
        for _ in 0..3 {
            let _ = ctx.input(0);
            let _ = ctx.output(0);
        }
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![TraceLink {
                pin: 0,
                element: None,
                probe: TraceProbe::Atom {
                    id: None,
                    position: probe.position(),
                },
            }],
        })
    });
    for i in 0..CHAIN {
        let (s, _) = add_scripted(&mut d, &format!("s{i}"), rereading.clone());
        d.connect_nodes(previous, 0, s, 0);
        previous = s;
    }

    // Leave counts from an ordinary pass in place; a trace must not touch
    // them.
    d.with_eval_context(false, |_, _, _, _| ());
    let counts_before = d.last_memo_counts().cloned();
    assert!(counts_before.is_some());

    LEAF_EVALS.set(0);
    let report = d.trace_hit(&atom_hit(previous, ORIGIN));
    assert_eq!(
        render(&d, &report).lines().last(),
        Some("leaf source"),
        "the chain is traced to its leaf"
    );
    // One input and one output entry per scripted node, plus the leaf's output
    // (read by the engine to match the last link by position).
    assert_eq!(report.stats.store_fills, 2 * CHAIN + 1);
    assert_eq!(LEAF_EVALS.get(), 1, "the leaf is evaluated once per trace");
    assert!(report.stats.memo.enabled);
    assert_eq!(d.last_memo_counts().cloned(), counts_before);
}

#[test]
fn a_trace_starting_at_an_unknown_node_is_empty() {
    let mut d = setup();
    let report = d.trace_hit(&atom_hit(999, ORIGIN));
    assert!(report.rows.is_empty());
    assert!(!report.truncated);
}

#[test]
fn a_hit_matching_no_atom_is_lost_at_the_displayed_node() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let mover = add(&mut d, "untraced_move", "move");
    d.connect_nodes(mol, 0, mover, 0);

    // The displayed node is where the probe was lost, so it is on the path;
    // only what is wired above it is "wired upstream".
    assert_eq!(
        trace(&mut d, &atom_hit(mover, DVec3::new(99.0, 0.0, 0.0))),
        "move possibly contributed\nmol wired upstream"
    );
}

// ============================================================================
// Trace coverage Phase 1: wired-upstream rows (`doc/design_trace_coverage.md`)
// ============================================================================

#[test]
fn a_custom_node_reached_only_through_a_lost_probe_is_wired_upstream() {
    let mut d = setup();
    build_inner(&mut d, "inner");
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let instance = add(&mut d, "inner", "inner");
    let mover = add(&mut d, "untraced_move", "move");
    d.connect_nodes(mol, 0, instance, 0);
    d.connect_nodes(instance, 0, mover, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(mover, UNTRACED_MOVE)),
        "move possibly contributed\ninner wired upstream\n  s_inner wired upstream\n  \
         p_inner wired upstream\nmol wired upstream"
    );
}

/// A `ScriptedTraceNode` with the default `trace`: an untraced node with two
/// atomic inputs (`input`, `other`) that outputs its `input`.
fn add_untraced_two_input(
    d: &mut atomcad_structure_designer::structure_designer::StructureDesigner,
    label: &str,
) -> u64 {
    add_scripted(d, label, Arc::new(|_, _, _, _| None)).0
}

#[test]
fn a_node_reached_precisely_first_and_then_through_a_lost_probe_is_listed_once() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let mover = add(&mut d, "untraced_move", "move");
    let t = add_untraced_two_input(&mut d, "t");
    d.connect_nodes(mol, 0, mover, 0);
    d.connect_nodes(mol, 0, t, 0);
    d.connect_nodes(mover, 0, t, 1);

    // `input` keeps the probe and reaches `mol`; `other` loses it, and the
    // lost cone reaches `mol` again. `mol` keeps only its traced row.
    assert_eq!(
        trace(&mut d, &atom_hit(t, ORIGIN)),
        "t possibly contributed\nmol source\nmove wired upstream"
    );
}

#[test]
fn a_node_reached_through_a_lost_probe_first_and_then_precisely_is_listed_once() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let mol5 = add_value(&mut d, molecule(&[(6, UNTRACED_MOVE)]), "mol5");
    let mover = add(&mut d, "untraced_move", "move");
    // `s` shows `mol5`'s atoms and says its atom came from `other` (`mol`) at
    // the origin.
    let to_origin: Script = Arc::new(|_, _, _, _| {
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![TraceLink {
                pin: 1,
                element: None,
                probe: TraceProbe::Atom {
                    id: None,
                    position: ORIGIN,
                },
            }],
        })
    });
    let (s, _) = add_scripted(&mut d, "s", to_origin);
    let t = add_untraced_two_input(&mut d, "t");
    d.connect_nodes(mol, 0, mover, 0);
    d.connect_nodes(mol5, 0, s, 0);
    d.connect_nodes(mol, 0, s, 1);
    d.connect_nodes(mover, 0, t, 0);
    d.connect_nodes(s, 0, t, 1);

    // `input` → `move` loses the probe into `mol` first; `other` → `s`
    // reaches `mol` precisely afterwards.
    assert_eq!(
        trace(&mut d, &atom_hit(t, UNTRACED_MOVE)),
        "t possibly contributed\nmove possibly contributed\ns passed\nmol source"
    );
}

// ============================================================================
// Trace coverage Phase 1: the honest stop at records and iterators
// ============================================================================

#[test]
fn a_record_reader_stops_the_trace_instead_of_claiming_to_be_a_source() {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Holder",
        vec![("mol".to_string(), DataType::Molecule)],
    ))
    .unwrap();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let construct = add(&mut d, "record_construct", "construct");
    set_node_data(
        &mut d,
        MAIN,
        construct,
        Box::new(RecordConstructData {
            schema: "Holder".to_string(),
            ..Default::default()
        }),
    );
    let destructure = add(&mut d, "record_destructure", "destructure");
    set_node_data(
        &mut d,
        MAIN,
        destructure,
        Box::new(RecordDestructureData::named("Holder")),
    );
    d.connect_nodes(mol, 0, construct, 0);
    d.connect_nodes(construct, 0, destructure, 0);
    d.validate_active_network();

    assert_eq!(
        trace(&mut d, &atom_hit(destructure, ORIGIN)),
        "destructure trace stops here"
    );
}

#[test]
fn collect_of_a_molecule_sequence_stops_the_trace() {
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    let seq = add(&mut d, "sequence", "seq");
    set_node_data(
        &mut d,
        MAIN,
        seq,
        Box::new(SequenceData {
            element_type: DataType::Molecule,
            input_count: 1,
        }),
    );
    let collect = add(&mut d, "collect", "collect");
    set_node_data(
        &mut d,
        MAIN,
        collect,
        Box::new(CollectData {
            element_type: DataType::Molecule,
            ..Default::default()
        }),
    );
    d.connect_nodes(mol, 0, seq, 0);
    d.connect_nodes(seq, 0, collect, 0);
    d.validate_active_network();

    assert_eq!(
        trace(&mut d, &atom_hit(collect, ORIGIN)),
        "collect trace stops here"
    );
}

#[test]
fn a_value_holding_a_molecule_is_still_a_source() {
    // No inputs at all: an origin, not an opaque reader.
    let mut d = setup();
    let mol = add_value(&mut d, molecule(&[(6, ORIGIN)]), "mol");
    assert_eq!(trace(&mut d, &atom_hit(mol, ORIGIN)), "mol source");
}

#[test]
fn may_carry_phase_sees_through_records_arrays_iterators_and_functions() {
    let mut d = setup();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Inner",
        vec![("xtal".to_string(), DataType::Crystal)],
    ))
    .unwrap();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Outer",
        vec![(
            "inner".to_string(),
            DataType::Record(RecordType::Named("Inner".to_string())),
        )],
    ))
    .unwrap();
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Floats",
        vec![
            ("a".to_string(), DataType::Float),
            ("b".to_string(), DataType::Float),
        ],
    ))
    .unwrap();
    let registry = &d.node_type_registry;
    let named = |n: &str| DataType::Record(RecordType::Named(n.to_string()));

    assert!(may_carry_phase(&named("Outer"), registry));
    assert!(!may_carry_phase(&named("Floats"), registry));
    assert!(may_carry_phase(
        &DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Blueprint)))),
        registry
    ));
    assert!(may_carry_phase(
        &DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Molecule)),
        registry
    ));
    assert!(!may_carry_phase(
        &DataType::Function(FunctionType::new(vec![DataType::Molecule], DataType::Float)),
        registry
    ));
    assert!(may_carry_phase(&named("NoSuchRecord"), registry));
}
