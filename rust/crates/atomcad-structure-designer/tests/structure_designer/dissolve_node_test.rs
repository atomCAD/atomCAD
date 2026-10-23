//! Dissolving a node (`doc/design_insert_node_on_wire.md` Phase 3):
//! `plan_dissolve_node` and the `dissolve_node` commit. The read-only refusal
//! is in the library-linking mutation alphabet
//! (`library_links_readonly_test.rs`, with the check-order case beside it);
//! the sweep over real drawings is `insert_dissolve_corpus_test.rs`.

use crate::splice_support::{assert_no_mutation, assert_one_step_round_trip, json_diff};
use crate::undo_test::snapshot_all_networks;
use atomcad_structure_designer::node_network::{ArgumentKind, NodeNetwork, SourcePin, WireAnchor};
use atomcad_structure_designer::nodes::comment::{CommentAnchor, CommentData};
use atomcad_structure_designer::splice_ops::{CandidateList, DissolveRefusal, NodeSource};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

const NET: &str = "Main";
const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };

fn design(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    let outcome = d.ai_text_edit(code, false);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
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

fn id(d: &StructureDesigner, name: &str) -> u64 {
    id_in(d.get_scope_network(&[]).unwrap(), name)
}

fn body_id(d: &StructureDesigner, scope: &[u64], name: &str) -> u64 {
    id_in(d.get_scope_network(scope).unwrap(), name)
}

fn wire(destination: u64, index: usize, source: u64) -> WireAnchor {
    WireAnchor {
        destination_node_id: destination,
        destination_argument_kind: ArgumentKind::External,
        destination_argument_index: index,
        destination_param_id: None,
        source_node_id: source,
    }
}

/// `(source node, source pin, depth)` of every wire into each input of
/// `node_id` in `scope`.
fn wires_into(
    d: &StructureDesigner,
    scope: &[u64],
    node_id: u64,
) -> Vec<Vec<(u64, SourcePin, u8)>> {
    d.get_scope_network(scope).unwrap().nodes[&node_id]
        .arguments
        .iter()
        .map(|arg| {
            arg.incoming_wires
                .iter()
                .map(|w| (w.source_node_id, w.source_pin, w.source_scope_depth))
                .collect()
        })
        .collect()
}

fn display(d: &mut StructureDesigner, scope: &[u64], node_id: u64) -> String {
    d.evaluate_node_output(scope, node_id, 0)
        .to_display_string()
}

/// Plan, then dissolve as one undo step that round-trips; the plan changes
/// nothing.
fn dissolve(d: &mut StructureDesigner, scope: &[u64], node_id: u64) {
    assert_no_mutation(d, |d| d.plan_dissolve_node(scope, node_id)).unwrap();
    assert_one_step_round_trip(d, |d| d.dissolve_node(scope, node_id).unwrap());
    assert!(
        !d.get_scope_network(scope)
            .unwrap()
            .nodes
            .contains_key(&node_id)
    );
}

/// Asserts that both the plan and the commit refuse with `expected`, and
/// that neither changes anything.
#[track_caller]
fn assert_refused(
    d: &mut StructureDesigner,
    scope: &[u64],
    node_id: u64,
    expected: DissolveRefusal,
) {
    let planned = assert_no_mutation(d, |d| d.plan_dissolve_node(scope, node_id));
    assert_eq!(planned, Err(expected.clone_for_test()));
    let committed = assert_no_mutation(d, |d| d.dissolve_node(scope, node_id));
    assert_eq!(committed, Err(expected));
}

/// `DissolveRefusal` is not `Clone` (thiserror enums seldom are); the tests
/// need it twice.
trait CloneForTest {
    fn clone_for_test(&self) -> Self;
}

impl CloneForTest for DissolveRefusal {
    fn clone_for_test(&self) -> Self {
        use DissolveRefusal::*;
        match self {
            ReadOnly { node } => ReadOnly { node: node.clone() },
            NoConsumer { node } => NoConsumer { node: node.clone() },
            UsedAsFunction { node } => UsedAsFunction { node: node.clone() },
            ReturnNode { node } => ReturnNode { node: node.clone() },
            NoCandidate { node } => NoCandidate { node: node.clone() },
            SeveralCandidates { node, candidates } => SeveralCandidates {
                node: node.clone(),
                candidates: candidates.clone(),
            },
            DuplicateSource {
                node,
                consumer,
                source_name,
            } => DuplicateSource {
                node: node.clone(),
                consumer: consumer.clone(),
                source_name: source_name.clone(),
            },
            NodeNotFound => NodeNotFound,
        }
    }
}

fn node(name: &str) -> String {
    name.to_string()
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn every_refusal_reads_as_the_checks_table() {
    use DissolveRefusal::*;
    let cases = [
        (
            ReadOnly { node: node("n") },
            "Can't dissolve n: this network is read-only (linked library)",
        ),
        (
            NoConsumer { node: node("n") },
            "Can't dissolve n: nothing uses its output — use Delete",
        ),
        (
            UsedAsFunction { node: node("n") },
            "Can't dissolve n: it is used as a function",
        ),
        (
            ReturnNode { node: node("n") },
            "Can't dissolve n: it is the network's return node",
        ),
        (
            NoCandidate { node: node("n") },
            "Can't dissolve n: no input wire fits where its output goes",
        ),
        (
            SeveralCandidates {
                node: node("diff"),
                candidates: CandidateList(vec![(node("base"), 1), (node("sub"), 1)]),
            },
            "Can't dissolve diff: 2 input wires could replace it (base, sub)",
        ),
        (
            SeveralCandidates {
                node: node("union"),
                candidates: CandidateList(vec![(node("shapes"), 3)]),
            },
            "Can't dissolve union: 3 input wires could replace it (3 wires into shapes)",
        ),
        (
            DuplicateSource {
                node: node("n"),
                consumer: node("union"),
                source_name: node("a"),
            },
            "Can't dissolve n: union would receive a twice",
        ),
        (NodeNotFound, "Can't dissolve: the node no longer exists"),
    ];
    for (refusal, text) in cases {
        assert_eq!(refusal.to_string(), text);
    }
}

#[test]
fn a_diff_with_both_inputs_wired_from_blueprints_is_refused() {
    let mut d = design(
        "a = cuboid {}
s = sphere {}
df = diff { base: a, sub: s }
mt = materialize { shape: df }
",
    );
    let df = id(&d, "df");
    assert_refused(
        &mut d,
        &[],
        df,
        DissolveRefusal::SeveralCandidates {
            node: node("df"),
            candidates: CandidateList(vec![(node("base"), 1), (node("sub"), 1)]),
        },
    );
}

#[test]
fn a_three_input_union_is_refused() {
    let mut d = design(
        "s1 = sphere {}
s2 = cuboid {}
s3 = sphere {}
u = union { shapes: [s1, s2, s3] }
mt = materialize { shape: u }
",
    );
    let u = id(&d, "u");
    assert_refused(
        &mut d,
        &[],
        u,
        DissolveRefusal::SeveralCandidates {
            node: node("u"),
            candidates: CandidateList(vec![(node("shapes"), 3)]),
        },
    );
}

#[test]
fn a_node_with_no_consumer_is_refused() {
    let mut d = design(
        "b = cuboid {}
mv = structure_move { input: b }
",
    );
    let mv = id(&d, "mv");
    assert_refused(
        &mut d,
        &[],
        mv,
        DissolveRefusal::NoConsumer { node: node("mv") },
    );
}

#[test]
fn a_node_whose_function_pin_is_consumed_is_refused() {
    // `inc` feeds `out` by value *and* `mq.f` by its function pin.
    let mut d = design(
        "r = range { count: 3 }
i = int { value: 1 }
inc = expr { n: i, expression: \"n + 1\", parameters: [{ name: \"n\", data_type: Int }] }
out = expr { m: inc, expression: \"m\", parameters: [{ name: \"m\", data_type: Int }] }
",
    );
    // A bodyless `map` reports its empty body until `f` is wired below.
    d.ai_text_edit(
        "mq = map { xs: r, input_type: Int, output_type: Int }",
        false,
    );
    let (inc, mq) = (id(&d, "inc"), id(&d, "mq"));
    // `n` is wired, so it is a capture and `inc` is a nullary function;
    // delay it so the function takes the element.
    d.set_function_pin_role(
        &[],
        inc,
        0,
        atomcad_structure_designer::node_network::FunctionPinRole::Delayed,
    );
    let f_index = d.get_scope_network(&[]).unwrap().nodes[&mq]
        .custom_node_type
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .position(|p| p.name == "f")
        .unwrap();
    d.get_scope_network_mut(&[]).unwrap().connect_wire(
        inc,
        SourcePin::NodeOutput { pin_index: -1 },
        0,
        mq,
        f_index,
        false,
    );
    d.validate_active_network();
    assert_refused(
        &mut d,
        &[],
        inc,
        DissolveRefusal::UsedAsFunction { node: node("inc") },
    );
}

#[test]
fn the_return_node_is_refused() {
    let mut d = design(
        "b = cuboid {}
mv = structure_move { input: b }
mt = materialize { shape: mv }
output mv
",
    );
    let mv = id(&d, "mv");
    assert_refused(
        &mut d,
        &[],
        mv,
        DissolveRefusal::ReturnNode { node: node("mv") },
    );
}

// A `parameter` dissolves like any node; that its undo restores the callers'
// wires (found by the corpus run) is `call_site_undo_test.rs`'s.

#[test]
fn a_consumer_that_would_receive_a_source_twice_is_refused() {
    // `union(a, move(a))` would become `union(a, a)`.
    let mut d = design(
        "a = sphere {}
mv = structure_move { input: a }
u = union { shapes: [a, mv] }
",
    );
    let mv = id(&d, "mv");
    assert_refused(
        &mut d,
        &[],
        mv,
        DissolveRefusal::DuplicateSource {
            node: node("mv"),
            consumer: node("u"),
            source_name: node("a"),
        },
    );
}

#[test]
fn a_consumer_whose_pin_cannot_take_the_candidate_blocks_the_dissolve() {
    // `structure_move` takes the Blueprint, but `exit_structure` wants a
    // Crystal.
    let mut d = design(
        "b = cuboid {}
mt = materialize { shape: b }
mv = structure_move { input: mt }
es = exit_structure { input: mt }
",
    );
    let mt = id(&d, "mt");
    assert_refused(
        &mut d,
        &[],
        mt,
        DissolveRefusal::NoCandidate { node: node("mt") },
    );
}

#[test]
fn a_missing_node_is_refused() {
    let mut d = design("b = cuboid {}\n");
    assert_refused(&mut d, &[], 999, DissolveRefusal::NodeNotFound);
    assert_refused(&mut d, &[999], 1, DissolveRefusal::NodeNotFound);
}

// ---------------------------------------------------------------------------
// Successes
// ---------------------------------------------------------------------------

#[test]
fn a_structure_move_with_a_wired_translation_dissolves_to_its_input() {
    // D14: the `translation` wire is not a candidate, since an IVec3 cannot
    // feed `materialize`, so the answer is unique.
    let mut d = design(
        "b = cuboid {}
t = ivec3 { x: 1 }
mv = structure_move { input: b, translation: t }
mt = materialize { shape: mv }
",
    );
    let (b, t, mv, mt) = (id(&d, "b"), id(&d, "t"), id(&d, "mv"), id(&d, "mt"));
    let plan = d.plan_dissolve_node(&[], mv).unwrap();
    assert_eq!(plan.candidate.source_node_id, b);
    assert_eq!(plan.consumers, [(vec![], wire(mt, 0, mv))]);

    dissolve(&mut d, &[], mv);
    assert_eq!(wires_into(&d, &[], mt)[0], [(b, OUT, 0)]);
    assert!(d.get_scope_network(&[]).unwrap().nodes.contains_key(&t));
    assert!(
        d.get_scope_network(&[])
            .unwrap()
            .validation_errors
            .is_empty()
    );
}

#[test]
fn a_node_feeding_two_consumers_re_points_both_in_their_own_slots() {
    let mut d = design(
        "b = cuboid {}
s = sphere {}
mv = structure_move { input: b }
m1 = materialize { shape: mv }
u = union { shapes: [s, mv] }
",
    );
    let (b, s, mv, m1, u) = (
        id(&d, "b"),
        id(&d, "s"),
        id(&d, "mv"),
        id(&d, "m1"),
        id(&d, "u"),
    );
    dissolve(&mut d, &[], mv);
    assert_eq!(wires_into(&d, &[], m1)[0], [(b, OUT, 0)]);
    // The position in the array slot is kept (D5).
    assert_eq!(wires_into(&d, &[], u)[0], [(s, OUT, 0), (b, OUT, 0)]);
}

/// A `map` over `r` whose body returns `e`, which captures `g`; `g` is an
/// identity over `f`, so dissolving it changes no value. The body also holds
/// `k`, an identity over `element`, read by `k2` and, through a capture, by
/// `z` one body deeper.
const BODIES: &str = "f = float { value: 2.0 }
g = expr { x: f, expression: \"x\", parameters: [{ name: \"x\", data_type: Float }] }
r = range { count: 3 }
mp = map { xs: r, input_type: Int, output_type: Float, body { e = expr { a: ^g, expression: \"a * 2.0\", parameters: [{ name: \"a\", data_type: Float }] } output e } }
cv = collect { iter: mp }
mk = map { xs: r, input_type: Int, output_type: Int, body { k = expr { n: $element, expression: \"n\", parameters: [{ name: \"n\", data_type: Int }] } k2 = expr { m: k, expression: \"m + 1\", parameters: [{ name: \"m\", data_type: Int }] } inner = map { xs: r, input_type: Int, output_type: Int, body { z = expr { q: ^k, p: $element, expression: \"q * 10 + p\", parameters: [{ name: \"q\", data_type: Int }, { name: \"p\", data_type: Int }] } output z } } ci = collect { iter: inner } j = expr { u: k2, w: ci, expression: \"u + w[0]\", parameters: [{ name: \"u\", data_type: Int }, { name: \"w\", data_type: [Int] }] } output j } }
ck = collect { iter: mk }
";

#[test]
fn a_capture_consumer_is_re_pointed_with_its_depth_recomputed() {
    let mut d = design(BODIES);
    let (f, g, mp, cv) = (id(&d, "f"), id(&d, "g"), id(&d, "mp"), id(&d, "cv"));
    let e = body_id(&d, &[mp], "e");
    let before = display(&mut d, &[], cv);

    let plan = d.plan_dissolve_node(&[], g).unwrap();
    assert_eq!(plan.consumers.len(), 1);
    let (storage_scope, anchor) = &plan.consumers[0];
    assert_eq!(storage_scope, &vec![mp]);
    assert_eq!((anchor.destination_node_id, anchor.source_node_id), (e, g));
    dissolve(&mut d, &[], g);

    assert_eq!(wires_into(&d, &[mp], e)[0], [(f, OUT, 1)]);
    assert_eq!(display(&mut d, &[], cv), before);
}

#[test]
fn a_body_node_fed_by_element_re_points_its_consumers_to_element() {
    let mut d = design(BODIES);
    let (mk, ck) = (id(&d, "mk"), id(&d, "ck"));
    let (k, k2, inner) = (
        body_id(&d, &[mk], "k"),
        body_id(&d, &[mk], "k2"),
        body_id(&d, &[mk], "inner"),
    );
    let z = body_id(&d, &[mk, inner], "z");
    let before = display(&mut d, &[], ck);

    dissolve(&mut d, &[mk], k);

    let element = SourcePin::ZoneInput { pin_index: 0 };
    // In the body: `element` at depth 1.
    assert_eq!(wires_into(&d, &[mk], k2)[0], [(mk, element, 1)]);
    // One body deeper: depth 2, beside `z`'s own `element` at depth 1.
    assert_eq!(wires_into(&d, &[mk, inner], z)[0], [(mk, element, 2)]);
    assert_eq!(display(&mut d, &[], ck), before);
}

/// Add a top-level comment anchored to each of `anchors`.
fn add_comment(d: &mut StructureDesigner, anchors: Vec<CommentAnchor>) -> u64 {
    let comment = d.add_node("Comment", DVec2::new(0.0, 600.0));
    d.get_scope_network_mut(&[])
        .unwrap()
        .nodes
        .get_mut(&comment)
        .unwrap()
        .data
        .as_any_mut()
        .downcast_mut::<CommentData>()
        .unwrap()
        .anchors = anchors;
    comment
}

fn anchors_of(d: &StructureDesigner, comment: u64) -> Vec<CommentAnchor> {
    d.get_scope_network(&[]).unwrap().nodes[&comment]
        .data
        .as_any_ref()
        .downcast_ref::<CommentData>()
        .unwrap()
        .anchors
        .clone()
}

#[test]
fn a_comment_on_a_consumer_wire_follows_it_and_one_on_the_node_is_dropped() {
    let mut d = design(
        "b = cuboid {}
mv = structure_move { input: b }
mt = materialize { shape: mv }
",
    );
    let (b, mv, mt) = (id(&d, "b"), id(&d, "mv"), id(&d, "mt"));
    let on_consumer = add_comment(&mut d, vec![CommentAnchor::Wire(wire(mt, 0, mv))]);
    let on_candidate = add_comment(
        &mut d,
        vec![CommentAnchor::Wire(wire(mv, 0, b)), CommentAnchor::Node(mv)],
    );
    dissolve(&mut d, &[], mv);
    assert_eq!(
        anchors_of(&d, on_consumer),
        [CommentAnchor::Wire(wire(mt, 0, b))]
    );
    assert_eq!(anchors_of(&d, on_candidate), []);
}

#[test]
fn dissolving_a_map_deletes_its_body() {
    let mut d = design(
        "r = range { count: 3 }
mp = map { xs: r, input_type: Int, output_type: Int, body { k = expr { n: $element, expression: \"n\", parameters: [{ name: \"n\", data_type: Int }] } output k } }
cv = collect { iter: mp }
",
    );
    let (r, mp, cv) = (id(&d, "r"), id(&d, "mp"), id(&d, "cv"));
    dissolve(&mut d, &[], mp);
    assert_eq!(wires_into(&d, &[], cv)[0], [(r, OUT, 0)]);
    assert!(d.get_scope_network(&[mp]).is_none());
}

#[test]
fn no_other_node_moves() {
    let mut d = design(
        "b = cuboid {}
t = ivec3 { x: 1 }
mv = structure_move { input: b, translation: t }
mt = materialize { shape: mv }
es = exit_structure { input: mt }
",
    );
    let mv = id(&d, "mv");
    let positions = |d: &StructureDesigner| {
        let mut all: Vec<(u64, DVec2)> = d
            .get_scope_network(&[])
            .unwrap()
            .nodes
            .values()
            .filter(|n| n.id != mv)
            .map(|n| (n.id, n.position))
            .collect();
        all.sort_by_key(|(id, _)| *id);
        all
    };
    let before = positions(&d);
    dissolve(&mut d, &[], mv);
    assert_eq!(positions(&d), before);
}

/// The serialized design with every node position (and the id counter, which
/// an insert advances and a dissolve does not take back) removed.
fn wiring(d: &mut StructureDesigner) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("position");
                map.remove("next_node_id");
                for child in map.values_mut() {
                    strip(child);
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut value = snapshot_all_networks(&mut d.node_type_registry);
    strip(&mut value);
    value
}

#[test]
fn dissolving_right_after_an_insert_restores_the_wiring() {
    let mut d = design(
        "b = cuboid {}
mt = materialize { shape: b }
",
    );
    let (b, mt) = (id(&d, "b"), id(&d, "mt"));
    let comment = add_comment(&mut d, vec![CommentAnchor::Wire(wire(mt, 0, b))]);
    let before = wiring(&mut d);

    let w = wire(mt, 0, b);
    let node = NodeSource::New("structure_move".to_string());
    let plan = d.plan_insert_on_wire(&[], &w, &[], &node).unwrap();
    let mv = d
        .insert_node_on_wire(&[], &w, &[], &node, &plan[0])
        .unwrap();
    dissolve(&mut d, &[], mv);

    let after = wiring(&mut d);
    assert!(
        before == after,
        "differs at {:?}",
        json_diff(&before, &after)
    );
    assert_eq!(
        anchors_of(&d, comment),
        [CommentAnchor::Wire(wire(mt, 0, b))]
    );
}
