//! Moving the wires of one pin to another (`doc/design_move_wires.md` Phase
//! 1b): `pin_wires`, `plan_move_wires` and the `move_wires` commit. Every plan
//! and refusal runs under the *no mutation* oracle and every commit under the
//! *round trip* oracle (`splice_support.rs`), so each case states only what it
//! adds. The read-only refusal is in the library-linking mutation alphabet
//! (`library_links_readonly_test.rs`).

use crate::splice_support::{assert_no_mutation, assert_one_step_round_trip, assert_same_networks};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::{
    ArgumentKind, NodeNetwork, SourcePin, Wire, WireAnchor,
};
use atomcad_structure_designer::nodes::comment::{CommentAnchor, CommentData};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::wire_move_ops::{
    MOVE_WIRE_DESCRIPTION, MOVE_WIRES_DESCRIPTION, PinAddress, PinSide, WireMoveRefusal,
};
use glam::f64::DVec2;

const NET: &str = "Main";
const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };
const FN: SourcePin = SourcePin::NodeOutput { pin_index: -1 };
const ELEMENT: SourcePin = SourcePin::ZoneInput { pin_index: 0 };

fn design(code: &str) -> StructureDesigner {
    let (d, errors) = design_with_errors(code);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// [`design`] for a fixture that is meant to fail validation (an unwired
/// `f`, an empty body): the errors are returned, not asserted.
fn design_with_errors(code: &str) -> (StructureDesigner, Vec<String>) {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    let outcome = d.ai_text_edit(code, false);
    d.validate_active_network();
    d.undo_stack.clear();
    (d, outcome.result.errors)
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

/// The index of `node_id`'s input named `name`.
fn param(d: &StructureDesigner, scope: &[u64], node_id: u64, name: &str) -> usize {
    let node = &d.get_scope_network(scope).unwrap().nodes[&node_id];
    d.node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .parameters
        .iter()
        .position(|p| p.name == name)
        .unwrap_or_else(|| panic!("no input '{}'", name))
}

fn input(scope: &[u64], node_id: u64, index: usize) -> PinAddress {
    PinAddress {
        scope: scope.to_vec(),
        node_id,
        side: PinSide::Input {
            kind: ArgumentKind::External,
            index,
        },
    }
}

fn output(scope: &[u64], node_id: u64, pin: SourcePin) -> PinAddress {
    PinAddress {
        scope: scope.to_vec(),
        node_id,
        side: PinSide::Output(pin),
    }
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

/// The anchor of the top-level wire `source → destination[index]`, with the
/// destination parameter's persistent id when it has one (`expr` does).
fn anchor(d: &StructureDesigner, destination: u64, index: usize, source: u64) -> WireAnchor {
    let network = d.get_scope_network(&[]).unwrap();
    WireAnchor::from_wire(&Wire::node_output(source, 0, destination, index), network)
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

/// Plan, then move as one undo step that round-trips; the plan changes
/// nothing.
#[track_caller]
fn moved(d: &mut StructureDesigner, from: &PinAddress, to: &PinAddress) {
    assert_no_mutation(d, |d| d.plan_move_wires(from, to)).unwrap();
    assert_one_step_round_trip(d, |d| d.move_wires(from, to).unwrap());
}

/// Both the plan and the commit refuse with `expected`, and neither changes
/// anything.
#[track_caller]
fn refused(
    d: &mut StructureDesigner,
    from: &PinAddress,
    to: &PinAddress,
    expected: WireMoveRefusal,
) {
    let planned = assert_no_mutation(d, |d| d.plan_move_wires(from, to));
    assert_eq!(planned, Err(expected.clone_for_test()));
    let committed = assert_no_mutation(d, |d| d.move_wires(from, to));
    assert_eq!(committed, Err(expected));
}

/// `WireMoveRefusal` is not `Clone` (thiserror enums seldom are); the tests
/// need it twice.
trait CloneForTest {
    fn clone_for_test(&self) -> Self;
}

impl CloneForTest for WireMoveRefusal {
    fn clone_for_test(&self) -> Self {
        use WireMoveRefusal::*;
        match self {
            ReadOnly => ReadOnly,
            PinNotFound => PinNotFound,
            NoWires => NoWires,
            DirectionMismatch => DirectionMismatch,
            SamePin => SamePin,
            TooManyWires { count } => TooManyWires { count: *count },
            OutOfScope { wire } => OutOfScope { wire: wire.clone() },
            TypeMismatch {
                wire,
                from_type,
                to_type,
            } => TypeMismatch {
                wire: wire.clone(),
                from_type: from_type.clone(),
                to_type: to_type.clone(),
            },
            DuplicateSource { wire } => DuplicateSource { wire: wire.clone() },
        }
    }
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
    d.undo_stack.clear();
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

const INT_AB: &str =
    "parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }]";

/// Int sources `a`, `b`, `c`; `s` reads `a` and `b`; `t` has one unconnected
/// Int input. Geometry: `u` unions three shapes, `v` is an empty union, `x`
/// holds one, `y` holds `s1` and `s2`.
fn basic() -> StructureDesigner {
    design(&format!(
        "a = int {{ value: 1 }}
b = int {{ value: 2 }}
c = int {{ value: 3 }}
s = expr {{ a: a, b: b, expression: \"a + b\", {INT_AB} }}
t = expr {{ expression: \"x\", parameters: [{{ name: \"x\", data_type: Int }}] }}
s1 = sphere {{}}
s2 = sphere {{}}
s3 = cuboid {{}}
s4 = cuboid {{}}
u = union {{ shapes: [s1, s2, s3] }}
v = union {{}}
x = union {{ shapes: [s4] }}
y = union {{ shapes: [s1, s2] }}
"
    ))
}

/// A `map` `mk` over `r` whose body node `k` reads `element` and captures
/// `a`; `h` is a spare body source and `kk` a body node with a free input.
fn with_body() -> StructureDesigner {
    design(&format!(
        "a = int {{ value: 1 }}
c = int {{ value: 3 }}
s = expr {{ a: a, b: c, expression: \"a + b\", {INT_AB} }}
t = expr {{ expression: \"x\", parameters: [{{ name: \"x\", data_type: Int }}] }}
r = range {{ count: 2 }}
mk = map {{ xs: r, input_type: Int, output_type: Int, body {{ k = expr {{ a: $element, b: ^a, expression: \"a + b\", {INT_AB} }} h = int {{ value: 7 }} kk = expr {{ expression: \"x\", parameters: [{{ name: \"x\", data_type: Int }}] }} output k }} }}
"
    ))
}

// ---------------------------------------------------------------------------
// pin_wires
// ---------------------------------------------------------------------------

#[test]
fn an_unconnected_pin_holds_no_wires() {
    let d = basic();
    let (t, v, s) = (id(&d, "t"), id(&d, "v"), id(&d, "s"));
    assert!(d.pin_wires(&input(&[], t, 0)).is_empty());
    assert!(d.pin_wires(&input(&[], v, 0)).is_empty());
    assert!(d.pin_wires(&output(&[], s, OUT)).is_empty());
    // A pin that does not exist holds nothing either.
    assert!(d.pin_wires(&input(&[], 999, 0)).is_empty());
}

#[test]
fn an_input_holds_its_wires_in_slot_order_with_their_sources_fixed() {
    let d = basic();
    let (a, s, u) = (id(&d, "a"), id(&d, "s"), id(&d, "u"));
    let (s1, s2, s3) = (id(&d, "s1"), id(&d, "s2"), id(&d, "s3"));

    let single = d.pin_wires(&input(&[], s, 0));
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].storage_scope, Vec::<u64>::new());
    assert_eq!(single[0].anchor, anchor(&d, s, 0, a));
    assert_eq!(single[0].fixed_end, output(&[], a, OUT));

    let array = d.pin_wires(&input(&[], u, 0));
    let sources: Vec<u64> = array.iter().map(|w| w.anchor.source_node_id).collect();
    assert_eq!(sources, [s1, s2, s3]);
    let fixed: Vec<PinAddress> = array.iter().map(|w| w.fixed_end.clone()).collect();
    assert_eq!(
        fixed,
        [
            output(&[], s1, OUT),
            output(&[], s2, OUT),
            output(&[], s3, OUT)
        ]
    );
}

#[test]
fn a_body_input_holding_a_capture_and_element_fixes_their_far_ends() {
    let d = with_body();
    let (a, mk) = (id(&d, "a"), id(&d, "mk"));
    let k = body_id(&d, &[mk], "k");
    let element = d.pin_wires(&input(&[mk], k, 0));
    assert_eq!(element.len(), 1);
    assert_eq!(element[0].storage_scope, vec![mk]);
    assert_eq!(element[0].fixed_end, output(&[], mk, ELEMENT));
    let capture = d.pin_wires(&input(&[mk], k, 1));
    assert_eq!(capture[0].fixed_end, output(&[], a, OUT));
}

/// `rd` destructures `{p, q}`; `rd.p` is read at top level, captured one
/// body down by `k` and two bodies down by `z`; `rd.q` is read by `s.b`.
const READERS: &str = "a = int { value: 1 }
b = int { value: 2 }
rc = record_construct { fields: [{ name: \"p\", data_type: Int }, { name: \"q\", data_type: Int }], p: a, q: b }
rd = record_destructure { fields: [{ name: \"p\", data_type: Int }, { name: \"q\", data_type: Int }], record: rc }
s = expr { a: rd.p, b: rd.q, expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
r = range { count: 2 }
mk = map { xs: r, input_type: Int, output_type: Int, body { k = expr { a: $element, b: ^rd.p, expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] } inner = map { xs: ^r, input_type: Int, output_type: Int, body { z = expr { a: $element, b: ^^rd.p, expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] } output z } } ci = collect { iter: inner } j = expr { a: k, b: ci, expression: \"a + b[0]\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: [Int] }] } output j } }
";

fn rd_pin(d: &StructureDesigner, field: &str) -> SourcePin {
    let rd = id(d, "rd");
    let node = &d.get_scope_network(&[]).unwrap().nodes[&rd];
    let index = d
        .node_type_registry
        .get_node_type_for_node(node)
        .unwrap()
        .output_pins
        .iter()
        .position(|p| p.name == field)
        .unwrap();
    SourcePin::NodeOutput {
        pin_index: index as i32,
    }
}

#[test]
fn an_output_holds_every_reader_in_every_scope_and_no_sibling_outputs() {
    let d = design(READERS);
    let (s, mk, rd) = (id(&d, "s"), id(&d, "mk"), id(&d, "rd"));
    let (k, inner) = (body_id(&d, &[mk], "k"), body_id(&d, &[mk], "inner"));
    let z = body_id(&d, &[mk, inner], "z");

    let p = d.pin_wires(&output(&[], rd, rd_pin(&d, "p")));
    let fixed: Vec<PinAddress> = p.iter().map(|w| w.fixed_end.clone()).collect();
    assert_eq!(
        fixed,
        [
            input(&[], s, 0),
            input(&[mk], k, 1),
            input(&[mk, inner], z, 1)
        ],
        "scope depth first, the sibling `q` never"
    );
    let storage: Vec<Vec<u64>> = p.iter().map(|w| w.storage_scope.clone()).collect();
    assert_eq!(storage, [vec![], vec![mk], vec![mk, inner]]);

    let q = d.pin_wires(&output(&[], rd, rd_pin(&d, "q")));
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].fixed_end, input(&[], s, 1));
}

#[test]
fn a_zone_input_holds_its_readers_in_its_own_body() {
    let d = design(READERS);
    let mk = id(&d, "mk");
    let k = body_id(&d, &[mk], "k");
    let readers = d.pin_wires(&output(&[], mk, ELEMENT));
    let fixed: Vec<PinAddress> = readers.iter().map(|w| w.fixed_end.clone()).collect();
    // `z`'s `$element` is `inner`'s, not `mk`'s.
    assert_eq!(fixed, [input(&[mk], k, 0)]);
}

// ---------------------------------------------------------------------------
// Each source/target combination
// ---------------------------------------------------------------------------

#[test]
fn a_single_input_moves_its_wire_to_another_single_input() {
    let mut d = basic();
    let (a, b, s, t) = (id(&d, "a"), id(&d, "b"), id(&d, "s"), id(&d, "t"));
    moved(&mut d, &input(&[], s, 0), &input(&[], t, 0));
    assert_eq!(wires_into(&d, &[], s), [vec![], vec![(b, OUT, 0)]]);
    assert_eq!(wires_into(&d, &[], t), [vec![(a, OUT, 0)]]);
    assert_eq!(d.undo_stack.undo_description(), Some(MOVE_WIRE_DESCRIPTION));
}

#[test]
fn an_array_input_moves_its_wires_in_order_to_an_empty_array_input() {
    let mut d = basic();
    let (u, v) = (id(&d, "u"), id(&d, "v"));
    let (s1, s2, s3) = (id(&d, "s1"), id(&d, "s2"), id(&d, "s3"));
    moved(&mut d, &input(&[], u, 0), &input(&[], v, 0));
    assert_eq!(wires_into(&d, &[], u)[0], []);
    assert_eq!(
        wires_into(&d, &[], v)[0],
        [(s1, OUT, 0), (s2, OUT, 0), (s3, OUT, 0)]
    );
    assert_eq!(
        d.undo_stack.undo_description(),
        Some(MOVE_WIRES_DESCRIPTION)
    );
}

#[test]
fn an_array_input_appends_its_wires_after_the_targets_own() {
    let mut d = basic();
    let (u, x) = (id(&d, "u"), id(&d, "x"));
    let (s1, s2, s3, s4) = (id(&d, "s1"), id(&d, "s2"), id(&d, "s3"), id(&d, "s4"));
    moved(&mut d, &input(&[], u, 0), &input(&[], x, 0));
    assert_eq!(
        wires_into(&d, &[], x)[0],
        [(s4, OUT, 0), (s1, OUT, 0), (s2, OUT, 0), (s3, OUT, 0)]
    );
}

#[test]
fn an_array_input_with_three_wires_does_not_fit_a_single_input() {
    let mut d = basic();
    let (u, s) = (id(&d, "u"), id(&d, "s"));
    refused(
        &mut d,
        &input(&[], u, 0),
        &input(&[], s, 0),
        WireMoveRefusal::TooManyWires { count: 3 },
    );
}

#[test]
fn an_output_re_sources_its_readers_in_two_scopes() {
    let mut d = with_body();
    let (a, c, s, mk) = (id(&d, "a"), id(&d, "c"), id(&d, "s"), id(&d, "mk"));
    let k = body_id(&d, &[mk], "k");
    let t = id(&d, "t");
    d.connect_nodes(a, 0, t, 0);
    d.undo_stack.clear();

    // `a` is read by `s.a`, `t.x` and, as a capture, by `k.b`. `s.b` reads
    // `c` already, in another slot, which does not stop `s.a` reading it.
    assert_eq!(d.pin_wires(&output(&[], a, OUT)).len(), 3);
    moved(&mut d, &output(&[], a, OUT), &output(&[], c, OUT));
    assert_eq!(
        wires_into(&d, &[], s),
        [vec![(c, OUT, 0)], vec![(c, OUT, 0)]]
    );
    assert_eq!(wires_into(&d, &[], t), [vec![(c, OUT, 0)]]);
    assert_eq!(wires_into(&d, &[mk], k)[1], [(c, OUT, 1)]);
    assert!(d.pin_wires(&output(&[], a, OUT)).is_empty());
}

/// `inc` and `inc2` are one-Int-arg functions; `inc`'s function pin feeds
/// `mp.f` and `ap.f`.
const FUNCTIONS: &str =
    "inc = expr { expression: \"n + 1\", parameters: [{ name: \"n\", data_type: Int }] }
inc2 = expr { expression: \"n + 2\", parameters: [{ name: \"n\", data_type: Int }] }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, f: @inc }
ap = apply { f: @inc }
";

#[test]
fn a_function_pin_moves_its_readers_to_another_function_pin() {
    let mut d = design(FUNCTIONS);
    let (inc, inc2, mp, ap) = (id(&d, "inc"), id(&d, "inc2"), id(&d, "mp"), id(&d, "ap"));
    let f = param(&d, &[], mp, "f");
    moved(&mut d, &output(&[], inc, FN), &output(&[], inc2, FN));
    assert_eq!(wires_into(&d, &[], mp)[f], [(inc2, FN, 0)]);
    assert_eq!(wires_into(&d, &[], ap)[0], [(inc2, FN, 0)]);
    assert!(d.pin_wires(&output(&[], inc, FN)).is_empty());
}

#[test]
fn a_zone_input_moves_its_readers_to_a_body_node() {
    let mut d = with_body();
    let mk = id(&d, "mk");
    let (k, h) = (body_id(&d, &[mk], "k"), body_id(&d, &[mk], "h"));
    moved(&mut d, &output(&[], mk, ELEMENT), &output(&[mk], h, OUT));
    assert_eq!(wires_into(&d, &[mk], k)[0], [(h, OUT, 0)]);
}

#[test]
fn a_body_input_moves_its_capture_with_the_depth_kept_right() {
    // `k.b` (a capture of `a`) onto `kk.x` in the same body, then onto the
    // top-level `t.x`: the depth follows the new destination.
    let mut d = with_body();
    let (a, t, mk) = (id(&d, "a"), id(&d, "t"), id(&d, "mk"));
    let (k, kk) = (body_id(&d, &[mk], "k"), body_id(&d, &[mk], "kk"));
    moved(&mut d, &input(&[mk], k, 1), &input(&[mk], kk, 0));
    assert_eq!(wires_into(&d, &[mk], kk)[0], [(a, OUT, 1)]);
    moved(&mut d, &input(&[mk], kk, 0), &input(&[], t, 0));
    assert_eq!(wires_into(&d, &[], t)[0], [(a, OUT, 0)]);
}

// ---------------------------------------------------------------------------
// Input end onto an output (D16)
// ---------------------------------------------------------------------------

#[test]
fn a_single_input_dropped_on_an_output_re_sources_its_wire_in_place() {
    let mut d = basic();
    let (a, b, c, s) = (id(&d, "a"), id(&d, "b"), id(&d, "c"), id(&d, "s"));
    let comment = add_comment(&mut d, vec![CommentAnchor::Wire(wire(s, 0, a))]);
    moved(&mut d, &input(&[], s, 0), &output(&[], c, OUT));
    assert_eq!(
        wires_into(&d, &[], s),
        [vec![(c, OUT, 0)], vec![(b, OUT, 0)]]
    );
    assert_eq!(
        anchors_of(&d, comment),
        [CommentAnchor::Wire(wire(s, 0, c))]
    );
}

#[test]
fn an_array_input_holding_one_wire_dropped_on_an_output_re_sources_it() {
    let mut d = basic();
    let (x, s3) = (id(&d, "x"), id(&d, "s3"));
    let u_before = wires_into(&d, &[], id(&d, "u"));
    moved(&mut d, &input(&[], x, 0), &output(&[], s3, OUT));
    assert_eq!(wires_into(&d, &[], x)[0], [(s3, OUT, 0)]);
    // `u` also reads `s3`; it is not touched.
    assert_eq!(wires_into(&d, &[], id(&d, "u")), u_before);
}

#[test]
fn an_array_input_with_three_wires_cannot_be_dropped_on_an_output() {
    let mut d = basic();
    let (u, s4) = (id(&d, "u"), id(&d, "s4"));
    refused(
        &mut d,
        &input(&[], u, 0),
        &output(&[], s4, OUT),
        WireMoveRefusal::TooManyWires { count: 3 },
    );
}

#[test]
fn wires_held_by_an_output_cannot_be_dropped_on_an_input() {
    let mut d = basic();
    let (a, t) = (id(&d, "a"), id(&d, "t"));
    refused(
        &mut d,
        &output(&[], a, OUT),
        &input(&[], t, 0),
        WireMoveRefusal::DirectionMismatch,
    );
}

// ---------------------------------------------------------------------------
// The disconnect side (D17): a move equals a delete and a connect
// ---------------------------------------------------------------------------

/// Two collapsed `map`s, each with a neighbour to push: `mp.f` reads `inc`,
/// `mq.f` is free.
const TWO_MAPS: &str =
    "inc = expr { expression: \"n + 1\", parameters: [{ name: \"n\", data_type: Int }] }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, f: @inc }
mq = map { xs: r, input_type: Int, output_type: Int }
cp = collect { iter: mp }
cq = collect { iter: mq }
";

#[test]
fn a_wire_moved_off_a_map_f_expands_it_as_a_delete_would() {
    let fixture = || design_with_errors(TWO_MAPS).0;
    let mut moving = fixture();
    let (inc, mp, mq) = (id(&moving, "inc"), id(&moving, "mp"), id(&moving, "mq"));
    let f = param(&moving, &[], mp, "f");
    moved(&mut moving, &input(&[], mp, f), &input(&[], mq, f));

    let mut by_hand = fixture();
    assert!(by_hand.select_wire(inc, -1, mp, f));
    by_hand.delete_selected();
    by_hand.connect_nodes(inc, -1, mq, f);
    assert_same_networks(&mut moving, &mut by_hand);
}

/// `ap.f` reads the two-arg `add`, with `arg0` wired from `one`; `aq` is a
/// bare `apply`.
const TWO_APPLIES: &str = "add = expr { expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
one = int { value: 1 }
ap = apply { f: @add, arg0: one }
aq = apply {}
";

#[test]
fn a_wire_moved_off_an_apply_f_drops_its_arg_wires_as_a_delete_would() {
    let fixture = || design_with_errors(TWO_APPLIES).0;
    let mut moving = fixture();
    let (add, ap, aq) = (id(&moving, "add"), id(&moving, "ap"), id(&moving, "aq"));
    assert_eq!(wires_into(&moving, &[], ap).len(), 3, "f, arg0, arg1");
    moved(&mut moving, &input(&[], ap, 0), &input(&[], aq, 0));
    assert_eq!(
        wires_into(&moving, &[], ap),
        [vec![]],
        "the bare [f] layout"
    );

    let mut by_hand = fixture();
    assert!(by_hand.select_wire(add, -1, ap, 0));
    by_hand.delete_selected();
    by_hand.connect_nodes(add, -1, aq, 0);
    assert_same_networks(&mut moving, &mut by_hand);
}

/// `inc`'s function pin has one reader, `ap.f`.
const ONE_FUNCTION_READER: &str =
    "inc = expr { expression: \"n + 1\", parameters: [{ name: \"n\", data_type: Int }] }
inc2 = expr { expression: \"n + 2\", parameters: [{ name: \"n\", data_type: Int }] }
ap = apply { f: @inc }
";

#[test]
fn moving_the_last_reader_off_a_function_pin_leaves_function_mode() {
    let fixture = || design(ONE_FUNCTION_READER);
    let mut moving = fixture();
    let (inc, inc2, ap) = (id(&moving, "inc"), id(&moving, "inc2"), id(&moving, "ap"));
    moved(&mut moving, &output(&[], inc, FN), &output(&[], inc2, FN));
    let network = moving.get_scope_network(&[]).unwrap();
    assert!(!network.function_pin_consumed(inc));

    let mut by_hand = fixture();
    assert!(by_hand.select_wire(inc, -1, ap, 0));
    by_hand.delete_selected();
    by_hand.connect_nodes(inc2, -1, ap, 0);
    assert_same_networks(&mut moving, &mut by_hand);
}

// ---------------------------------------------------------------------------
// Displacing, refusing
// ---------------------------------------------------------------------------

#[test]
fn a_connected_single_input_target_displaces_its_wire_and_its_anchor() {
    let mut d = basic();
    let (a, b, s) = (id(&d, "a"), id(&d, "b"), id(&d, "s"));
    let comment = add_comment(&mut d, vec![CommentAnchor::Wire(wire(s, 1, b))]);
    let plan = d
        .plan_move_wires(&input(&[], s, 0), &input(&[], s, 1))
        .unwrap();
    assert_eq!(plan.wires, [(vec![], anchor(&d, s, 0, a))]);
    assert_eq!(plan.displaced, Some((vec![], anchor(&d, s, 1, b))));
    moved(&mut d, &input(&[], s, 0), &input(&[], s, 1));
    assert_eq!(wires_into(&d, &[], s), [vec![], vec![(a, OUT, 0)]]);
    assert_eq!(anchors_of(&d, comment), []);
}

#[test]
fn a_missing_pin_is_refused() {
    let mut d = basic();
    let (s, t) = (id(&d, "s"), id(&d, "t"));
    refused(
        &mut d,
        &input(&[], 999, 0),
        &input(&[], t, 0),
        WireMoveRefusal::PinNotFound,
    );
    refused(
        &mut d,
        &input(&[], s, 0),
        &input(&[], t, 7),
        WireMoveRefusal::PinNotFound,
    );
    refused(
        &mut d,
        &input(&[], s, 0),
        &output(&[], t, SourcePin::NodeOutput { pin_index: 5 }),
        WireMoveRefusal::PinNotFound,
    );
}

#[test]
fn an_unconnected_pin_has_nothing_to_move() {
    let mut d = basic();
    let (s, t) = (id(&d, "s"), id(&d, "t"));
    refused(
        &mut d,
        &input(&[], t, 0),
        &input(&[], s, 0),
        WireMoveRefusal::NoWires,
    );
}

#[test]
fn dropping_on_the_starting_pin_is_refused() {
    let mut d = basic();
    let (a, s) = (id(&d, "a"), id(&d, "s"));
    refused(
        &mut d,
        &input(&[], s, 0),
        &input(&[], s, 0),
        WireMoveRefusal::SamePin,
    );
    refused(
        &mut d,
        &output(&[], a, OUT),
        &output(&[], a, OUT),
        WireMoveRefusal::SamePin,
    );
}

#[test]
fn a_wire_of_the_wrong_type_is_refused() {
    let mut d = basic();
    let (s, v) = (id(&d, "s"), id(&d, "v"));
    let planned = assert_no_mutation(&mut d, |d| {
        d.plan_move_wires(&input(&[], s, 0), &input(&[], v, 0))
    });
    let Err(WireMoveRefusal::TypeMismatch {
        wire,
        from_type,
        to_type,
    }) = planned
    else {
        panic!("expected a type mismatch, got {planned:?}");
    };
    assert_eq!(wire, "a → s.a");
    assert_eq!(from_type, DataType::Int.to_string());
    let expected = WireMoveRefusal::TypeMismatch {
        wire,
        from_type,
        to_type,
    };
    refused(&mut d, &input(&[], s, 0), &input(&[], v, 0), expected);
}

#[test]
fn a_zone_input_moved_out_of_its_body_is_out_of_scope() {
    let mut d = with_body();
    let (t, mk) = (id(&d, "t"), id(&d, "mk"));
    let k = body_id(&d, &[mk], "k");
    refused(
        &mut d,
        &input(&[mk], k, 0),
        &input(&[], t, 0),
        WireMoveRefusal::OutOfScope {
            wire: "mk → k.a".to_string(),
        },
    );
}

#[test]
fn top_level_readers_moved_to_a_body_output_are_out_of_scope() {
    let mut d = with_body();
    let (c, mk) = (id(&d, "c"), id(&d, "mk"));
    let h = body_id(&d, &[mk], "h");
    refused(
        &mut d,
        &output(&[], c, OUT),
        &output(&[mk], h, OUT),
        WireMoveRefusal::OutOfScope {
            wire: "c → s.b".to_string(),
        },
    );
}

#[test]
fn a_target_slot_reading_a_moved_source_already_is_refused() {
    let mut d = basic();
    let (u, y) = (id(&d, "u"), id(&d, "y"));
    // Input side: `y` already reads `s1`.
    refused(
        &mut d,
        &input(&[], u, 0),
        &input(&[], y, 0),
        WireMoveRefusal::DuplicateSource {
            wire: "s1 → u.shapes".to_string(),
        },
    );
    // Output side: `u` (first by id) already reads `s1`.
    let (s1, s2) = (id(&d, "s1"), id(&d, "s2"));
    refused(
        &mut d,
        &output(&[], s2, OUT),
        &output(&[], s1, OUT),
        WireMoveRefusal::DuplicateSource {
            wire: "s2 → u.shapes".to_string(),
        },
    );
}

#[test]
fn moving_between_two_outputs_of_one_node_is_not_a_duplicate() {
    let mut d = design(READERS);
    let (rd, s) = (id(&d, "rd"), id(&d, "s"));
    let (p, q) = (rd_pin(&d, "p"), rd_pin(&d, "q"));
    // `s.a` reads `rd.p` and `s.b` reads `rd.q`, so each slot already reads
    // node `rd`.
    moved(&mut d, &output(&[], rd, q), &output(&[], rd, p));
    assert_eq!(wires_into(&d, &[], s), [vec![(rd, p, 0)], vec![(rd, p, 0)]]);
}

// ---------------------------------------------------------------------------
// Comment anchors, adoption
// ---------------------------------------------------------------------------

#[test]
fn a_note_on_a_moved_wire_follows_it_on_the_input_side() {
    let mut d = basic();
    let (a, s, t) = (id(&d, "a"), id(&d, "s"), id(&d, "t"));
    let comment = add_comment(&mut d, vec![CommentAnchor::Wire(wire(s, 0, a))]);
    moved(&mut d, &input(&[], s, 0), &input(&[], t, 0));
    assert_eq!(
        anchors_of(&d, comment),
        [CommentAnchor::Wire(anchor(&d, t, 0, a))]
    );
}

#[test]
fn a_note_on_a_moved_wire_follows_it_on_the_output_side() {
    let mut d = basic();
    let (a, c, s) = (id(&d, "a"), id(&d, "c"), id(&d, "s"));
    let comment = add_comment(&mut d, vec![CommentAnchor::Wire(wire(s, 0, a))]);
    moved(&mut d, &output(&[], a, OUT), &output(&[], c, OUT));
    assert_eq!(
        anchors_of(&d, comment),
        [CommentAnchor::Wire(wire(s, 0, c))]
    );
}

#[test]
fn a_wire_moved_into_another_scope_loses_its_note() {
    let mut d = with_body();
    let (a, s, mk) = (id(&d, "a"), id(&d, "s"), id(&d, "mk"));
    let kk = body_id(&d, &[mk], "kk");
    let comment = add_comment(&mut d, vec![CommentAnchor::Wire(wire(s, 0, a))]);
    moved(&mut d, &input(&[], s, 0), &input(&[mk], kk, 0));
    assert_eq!(wires_into(&d, &[mk], kk)[0], [(a, OUT, 1)]);
    assert_eq!(anchors_of(&d, comment), []);
}

#[test]
fn a_record_wire_moved_into_an_unset_reader_is_adopted() {
    let mut d = design(
        "e = expr { expression: \"{a: 1.0, b: 2.0}\", parameters: [] }
d1 = record_destructure {}
d2 = record_destructure {}
",
    );
    let (e, d1, d2) = (id(&d, "e"), id(&d, "d1"), id(&d, "d2"));
    d.connect_nodes(e, 0, d1, 0);
    d.undo_stack.clear();
    let output_names = |d: &StructureDesigner, node_id: u64| -> Vec<String> {
        let node = &d.get_scope_network(&[]).unwrap().nodes[&node_id];
        d.node_type_registry
            .get_node_type_for_node(node)
            .unwrap()
            .output_pins
            .iter()
            .map(|p| p.name.clone())
            .collect()
    };
    assert_eq!(output_names(&d, d2), ["result"]);
    moved(&mut d, &input(&[], d1, 0), &input(&[], d2, 0));
    assert_eq!(output_names(&d, d2), ["a", "b"]);
}

// ---------------------------------------------------------------------------
// Wording
// ---------------------------------------------------------------------------

#[test]
fn every_refusal_reads_as_its_variant() {
    use WireMoveRefusal::*;
    let wire = || "cube → union.shapes".to_string();
    let cases = [
        (ReadOnly, "this network is read-only (linked library)"),
        (PinNotFound, "the pin no longer exists"),
        (NoWires, "the pin has no wires to move"),
        (
            DirectionMismatch,
            "wires held by their output end can only be dropped on an output",
        ),
        (SamePin, "the wires are already on this pin"),
        (
            TooManyWires { count: 3 },
            "3 wires cannot go where only one fits",
        ),
        (
            OutOfScope { wire: wire() },
            "cube → union.shapes: the source is not visible from there",
        ),
        (
            TypeMismatch {
                wire: wire(),
                from_type: "Int".to_string(),
                to_type: "Blueprint".to_string(),
            },
            "cube → union.shapes: Int does not convert to Blueprint",
        ),
        (
            DuplicateSource { wire: wire() },
            "cube → union.shapes: the pin already reads that source",
        ),
    ];
    for (refusal, text) in cases {
        assert_eq!(refusal.to_string(), text);
    }
}

// ---------------------------------------------------------------------------
// The plan is authoritative (D9): a sweep over every pair of pins
// ---------------------------------------------------------------------------

/// One of each pin kind: top level and in a `map` body, single and array
/// inputs, a multi-output node, a `-1` pin with a reader, zone pins, and
/// unconnected pins.
const SWEEP: &str = "a = int { value: 1 }
b = int { value: 2 }
s = expr { a: a, b: b, expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
s1 = sphere {}
u = union { shapes: [s1] }
v = union {}
rc = record_construct { fields: [{ name: \"p\", data_type: Int }, { name: \"q\", data_type: Int }], p: a, q: b }
rd = record_destructure { fields: [{ name: \"p\", data_type: Int }, { name: \"q\", data_type: Int }], record: rc }
inc = expr { expression: \"n + 1\", parameters: [{ name: \"n\", data_type: Int }] }
r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, f: @inc }
mk = map { xs: r, input_type: Int, output_type: Int, body { k = expr { a: $element, b: ^rd.q, expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] } output k } }
";

/// Every pin of every node in every scope of `d`.
fn all_pins(d: &StructureDesigner) -> Vec<PinAddress> {
    fn walk(d: &StructureDesigner, scope: &mut Vec<u64>, out: &mut Vec<PinAddress>) {
        let network = d.get_scope_network(scope).unwrap();
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for node_id in ids {
            let node = &network.nodes[&node_id];
            let Some(node_type) = d.node_type_registry.get_node_type_for_node(node) else {
                continue;
            };
            for index in 0..node_type.parameters.len() {
                out.push(input(scope, node_id, index));
            }
            for index in 0..node_type.zone_output_pins.len() {
                out.push(PinAddress {
                    scope: scope.clone(),
                    node_id,
                    side: PinSide::Input {
                        kind: ArgumentKind::ZoneOutput,
                        index,
                    },
                });
            }
            for pin_index in -1..node_type.output_pin_count() as i32 {
                out.push(output(scope, node_id, SourcePin::NodeOutput { pin_index }));
            }
            for pin_index in 0..node_type.zone_input_pins.len() {
                out.push(output(scope, node_id, SourcePin::ZoneInput { pin_index }));
            }
            if node.zone.is_some() {
                scope.push(node_id);
                walk(d, scope, out);
                scope.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(d, &mut Vec::new(), &mut out);
    out
}

#[test]
fn plan_and_commit_agree_for_every_pair_of_pins() {
    let mut shared = design(SWEEP);
    let pins = all_pins(&shared);
    assert!(pins.len() > 40, "the fixture has {} pins", pins.len());
    let (mut accepted, mut refusals) = (0, 0);
    for from in &pins {
        for to in &pins {
            // A refusal changes nothing, so the shared copy stays fresh.
            let planned = assert_no_mutation(&mut shared, |d| d.plan_move_wires(from, to));
            match planned {
                Ok(_) => {
                    accepted += 1;
                    let mut d = design(SWEEP);
                    assert_one_step_round_trip(&mut d, |d| {
                        d.move_wires(from, to)
                            .unwrap_or_else(|r| panic!("{from:?} → {to:?}: {r}"))
                    });
                }
                Err(refusal) => {
                    refusals += 1;
                    let committed = assert_no_mutation(&mut shared, |d| d.move_wires(from, to));
                    assert_eq!(committed, Err(refusal), "{from:?} → {to:?}");
                }
            }
        }
    }
    assert!(
        accepted > 10 && refusals > 100,
        "{accepted} accepted, {refusals} refused"
    );
}
