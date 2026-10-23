//! Inserting a node into a wire (`doc/design_insert_node_on_wire.md`
//! Phase 1): `plan_insert_on_wire`, `insertable_node_type_names`, the
//! `insert_node_on_wire` commit, and the rewiring primitive both insert and
//! dissolve build on, `replace_wire_source`. The read-only refusal is in the
//! library-linking mutation alphabet (`library_links_readonly_test.rs`); the
//! sweep over real drawings is `insert_dissolve_corpus_test.rs`.

use crate::layout_oracle::{Drawing, check_bodies, check_no_new_overlap, check_no_wire_flipped};
use crate::splice_support::{assert_no_mutation, assert_one_step_round_trip};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::{ArgumentKind, NodeNetwork, SourcePin, WireAnchor};
use atomcad_structure_designer::nodes::comment::{CommentAnchor, CommentData};
use atomcad_structure_designer::splice_ops::{
    NodeSource, ReplaceWireSourceError, SpliceRefusal, slot_accepts_source,
};
use atomcad_structure_designer::splice_plan::Arrangement;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

const NET: &str = "Main";

/// - `b → mt`: a Blueprint into `materialize`; `mt` feeds `mv`
///   (`structure_move`) and `es` (`exit_structure`), which feeds `fm`;
/// - `u`: a `union` of three shapes, `s1`, `s2`, `s3`;
/// - `f → fe`: a Float into an `expr`;
/// - `mp`: a `map` over `r` whose body returns `e` (which captures `f`) and
///   holds `k`, fed by `element`; `cv` collects it;
/// - `cl → mq.f`: a closure into a bodyless `map`'s function pin.
const CODE: &str = "b = cuboid {}
mt = materialize { shape: b }
mv = structure_move { input: mt }
es = exit_structure { input: mt }
fm = free_move { input: es }
s1 = sphere {}
s2 = cuboid {}
s3 = sphere {}
u = union { shapes: [s1, s2, s3] }
f = float { value: 2.0 }
fe = expr { x: f, expression: \"x * 3.0\", parameters: [{ name: \"x\", data_type: Float }] }
r = range { count: 3 }
mp = map { xs: r, input_type: Int, output_type: Float, body { e = expr { a: ^f, expression: \"a * 2.0\", parameters: [{ name: \"a\", data_type: Float }] } k = expr { n: $element, expression: \"n + 1\", parameters: [{ name: \"n\", data_type: Int }] } output e } }
cv = collect { iter: mp }
cl = closure { kind: \"custom\", params: [\"q\"], type_args: [Int, Int], body { c = int { value: 3 } output c } }
mq = map { xs: r, f: cl, input_type: Int, output_type: Int }
";

fn setup() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network(NET);
    d.set_active_node_network_name(Some(NET.to_string()));
    let outcome = d.ai_text_edit(CODE, false);
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

/// The wire from `source` into input `index` of `destination`.
fn wire(destination: u64, index: usize, source: u64) -> WireAnchor {
    WireAnchor {
        destination_node_id: destination,
        destination_argument_kind: ArgumentKind::External,
        destination_argument_index: index,
        destination_param_id: None,
        source_node_id: source,
    }
}

fn new(type_name: &str) -> NodeSource {
    NodeSource::New(type_name.to_string())
}

/// `input name → output name` of each arrangement.
fn pairs(plan: &[Arrangement]) -> Vec<(String, String)> {
    plan.iter()
        .map(|a| {
            (
                a.input.as_ref().unwrap().name.clone(),
                a.output.as_ref().unwrap().name.clone(),
            )
        })
        .collect()
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

const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };

/// Plan `type_name` on `wire` and commit its only arrangement, as one undo
/// step that round-trips.
fn insert_only(
    d: &mut StructureDesigner,
    wire_scope: &[u64],
    wire: &WireAnchor,
    clicked: &[u64],
    type_name: &str,
) -> u64 {
    let plan = d
        .plan_insert_on_wire(wire_scope, wire, clicked, &new(type_name))
        .unwrap();
    assert_eq!(plan.len(), 1, "{:?}", plan);
    assert_one_step_round_trip(d, |d| {
        d.insert_node_on_wire(wire_scope, wire, clicked, &new(type_name), &plan[0])
            .unwrap()
    })
}

fn display(d: &mut StructureDesigner, scope: &[u64], node_id: u64) -> String {
    d.evaluate_node_output(scope, node_id, 0)
        .to_display_string()
}

// ---------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------

#[test]
fn structure_move_between_a_blueprint_and_materialize_has_one_arrangement() {
    let mut d = setup();
    let w = wire(id(&d, "mt"), 0, id(&d, "b"));
    let plan = assert_no_mutation(&mut d, |d| {
        d.plan_insert_on_wire(&[], &w, &[], &new("structure_move"))
    })
    .unwrap();
    assert_eq!(pairs(&plan), [("input".into(), "result".into())]);
    assert_eq!(
        plan[0].output.as_ref().unwrap().data_type,
        DataType::Blueprint
    );
}

#[test]
fn passivate_between_materialize_and_structure_move_types_through_same_as_input() {
    let mut d = setup();
    let w = wire(id(&d, "mv"), 0, id(&d, "mt"));
    let plan = assert_no_mutation(&mut d, |d| {
        d.plan_insert_on_wire(&[], &w, &[], &new("passivate"))
    })
    .unwrap();
    assert_eq!(pairs(&plan), [("molecule".into(), "result".into())]);
    assert_eq!(
        plan[0].output.as_ref().unwrap().data_type,
        DataType::Crystal
    );
}

#[test]
fn relax_after_exit_structure_resolves_its_output_to_molecule() {
    let d = setup();
    let w = wire(id(&d, "fm"), 0, id(&d, "es"));
    let plan = d.plan_insert_on_wire(&[], &w, &[], &new("relax")).unwrap();
    // `relax` has `result` and `diff`, and both are Molecules here.
    assert!(!plan.is_empty());
    for arrangement in &plan {
        assert_eq!(
            arrangement.output.as_ref().unwrap().data_type,
            DataType::Molecule
        );
    }
    assert_eq!(plan[0].output.as_ref().unwrap().name, "result");
}

#[test]
fn diff_on_a_blueprint_wire_yields_two_arrangements() {
    let d = setup();
    let w = wire(id(&d, "mt"), 0, id(&d, "b"));
    let plan = d.plan_insert_on_wire(&[], &w, &[], &new("diff")).unwrap();
    assert_eq!(
        pairs(&plan),
        [
            ("base".into(), "result".into()),
            ("sub".into(), "result".into())
        ]
    );
}

#[test]
fn the_function_pin_is_never_an_output() {
    let d = setup();
    for w in [
        wire(id(&d, "mt"), 0, id(&d, "b")),
        wire(id(&d, "fe"), 0, id(&d, "f")),
        wire(id(&d, "mq"), 1, id(&d, "cl")),
    ] {
        let names = d.insertable_node_type_names(&[], &w, &[]).unwrap();
        assert!(!names.is_empty());
        for name in names {
            for arrangement in d.plan_insert_on_wire(&[], &w, &[], &new(&name)).unwrap() {
                assert!(arrangement.output.unwrap().pin >= 0, "{name}");
            }
        }
    }
}

#[test]
fn a_type_whose_output_cannot_reach_the_destination_is_not_offered() {
    let mut d = setup();
    // `materialize` takes the Blueprint but makes a Crystal, and the slot
    // wants a Blueprint.
    let w = wire(id(&d, "mt"), 0, id(&d, "b"));
    assert_eq!(
        d.plan_insert_on_wire(&[], &w, &[], &new("materialize")),
        Ok(vec![])
    );
    let names = assert_no_mutation(&mut d, |d| d.insertable_node_type_names(&[], &w, &[])).unwrap();
    assert!(!names.contains(&"materialize".to_string()));
    for offered in ["structure_move", "diff", "union"] {
        assert!(names.contains(&offered.to_string()), "{offered}");
    }
}

#[test]
fn expr_is_listed_on_a_float_wire() {
    let d = setup();
    let w = wire(id(&d, "fe"), 0, id(&d, "f"));
    let names = d.insertable_node_type_names(&[], &w, &[]).unwrap();
    assert!(names.contains(&"expr".to_string()), "{names:?}");
}

#[test]
fn apply_on_a_function_typed_wire_takes_its_layout_from_the_source() {
    let d = setup();
    let mq = id(&d, "mq");
    let f_index = d.get_scope_network(&[]).unwrap().nodes[&mq]
        .custom_node_type
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .position(|p| p.name == "f")
        .unwrap();
    let w = wire(mq, f_index, id(&d, "cl"));
    let plan = d.plan_insert_on_wire(&[], &w, &[], &new("apply")).unwrap();
    assert_eq!(pairs(&plan), [("f".into(), "result".into())]);
    // Nothing applied yet: the output is the closure's own signature.
    assert!(
        matches!(
            plan[0].output.as_ref().unwrap().data_type,
            DataType::Function(_)
        ),
        "{:?}",
        plan[0].output
    );
}

/// The body-return wire `e → mp.result`, stored on `mp` in the top-level
/// network.
fn body_return(d: &StructureDesigner) -> WireAnchor {
    let mp = id(d, "mp");
    WireAnchor {
        destination_node_id: mp,
        destination_argument_kind: ArgumentKind::ZoneOutput,
        destination_argument_index: 0,
        destination_param_id: None,
        source_node_id: body_id(d, &[mp], "e"),
    }
}

#[test]
fn a_type_parameterized_node_has_a_plan_on_a_body_return_wire() {
    let d = setup();
    let mp = id(&d, "mp");
    let w = body_return(&d);
    // `array_at`-style adaptation: `expr` adapts to its Float source.
    let plan = d.plan_insert_on_wire(&[], &w, &[mp], &new("expr")).unwrap();
    assert_eq!(pairs(&plan), [("x".into(), "result".into())]);
    assert_eq!(plan[0].output.as_ref().unwrap().data_type, DataType::Float);
}

#[test]
fn a_parameter_is_never_listed_for_a_body_scope() {
    let d = setup();
    let mp = id(&d, "mp");
    let w = body_return(&d);
    let names = d.insertable_node_type_names(&[], &w, &[mp]).unwrap();
    assert!(!names.is_empty());
    assert!(!names.contains(&"parameter".to_string()));
    assert_eq!(
        d.plan_insert_on_wire(&[], &w, &[mp], &new("parameter")),
        Ok(vec![])
    );
}

// ---------------------------------------------------------------------------
// Commits
// ---------------------------------------------------------------------------

#[test]
fn a_structure_move_insert_rewires_both_halves() {
    let mut d = setup();
    let (b, mt) = (id(&d, "b"), id(&d, "mt"));
    let w = wire(mt, 0, b);
    let plan = d
        .plan_insert_on_wire(&[], &w, &[], &new("structure_move"))
        .unwrap();
    let mv = d
        .insert_node_on_wire(&[], &w, &[], &new("structure_move"), &plan[0])
        .unwrap();

    assert_eq!(wires_into(&d, &[], mv)[0], [(b, OUT, 0)]);
    assert_eq!(wires_into(&d, &[], mt)[0], [(mv, OUT, 0)]);
    let network = d.get_scope_network(&[]).unwrap();
    // The new node is the selected, active one (selection is view state, so
    // it is not part of the undo snapshot).
    assert_eq!(network.active_node_id, Some(mv));
    assert!(network.validation_errors.is_empty());
}

#[test]
fn inserting_into_the_second_wire_of_a_union_keeps_the_order() {
    let mut d = setup();
    let (s1, s2, s3, u) = (id(&d, "s1"), id(&d, "s2"), id(&d, "s3"), id(&d, "u"));
    let mv = insert_only(&mut d, &[], &wire(u, 0, s2), &[], "structure_move");
    assert_eq!(
        wires_into(&d, &[], u)[0],
        [(s1, OUT, 0), (mv, OUT, 0), (s3, OUT, 0)]
    );
}

#[test]
fn a_body_return_insert_lands_in_the_body() {
    let mut d = setup();
    let mp = id(&d, "mp");
    let cv = id(&d, "cv");
    let before = display(&mut d, &[], cv);
    let e = body_id(&d, &[mp], "e");
    let w = body_return(&d);
    let x = insert_only(&mut d, &[], &w, &[mp], "expr");

    let body = d.get_scope_network(&[mp]).unwrap();
    assert!(body.nodes.contains_key(&x));
    assert_eq!(wires_into(&d, &[mp], x)[0], [(e, OUT, 0)]);
    let returned = &d.get_scope_network(&[]).unwrap().nodes[&mp].zone_output_arguments[0];
    assert_eq!(returned.incoming_wires.len(), 1);
    assert_eq!(returned.incoming_wires[0].source_node_id, x);
    assert_eq!(returned.incoming_wires[0].source_scope_depth, 0);
    assert_eq!(display(&mut d, &[], cv), before);
}

#[test]
fn a_capture_insert_follows_the_click() {
    // The capture `f → e.a`, stored in `mp`'s body with depth 1.
    for clicked_inside in [false, true] {
        let mut d = setup();
        let (f, mp, cv) = (id(&d, "f"), id(&d, "mp"), id(&d, "cv"));
        let e = body_id(&d, &[mp], "e");
        let before = display(&mut d, &[], cv);
        let clicked: Vec<u64> = if clicked_inside { vec![mp] } else { vec![] };
        let x = insert_only(&mut d, &[mp], &wire(e, 0, f), &clicked, "expr");

        if clicked_inside {
            // In the body, once per element: the capture moves in front of
            // the new node.
            assert!(d.get_scope_network(&[mp]).unwrap().nodes.contains_key(&x));
            assert_eq!(wires_into(&d, &[mp], x)[0], [(f, OUT, 1)]);
            assert_eq!(wires_into(&d, &[mp], e)[0], [(x, OUT, 0)]);
        } else {
            // Outside, once: the new node's output is the capture.
            assert!(d.get_scope_network(&[]).unwrap().nodes.contains_key(&x));
            assert_eq!(wires_into(&d, &[], x)[0], [(f, OUT, 0)]);
            assert_eq!(wires_into(&d, &[mp], e)[0], [(x, OUT, 1)]);
        }
        // The identity expression makes the comparison exact.
        assert_eq!(display(&mut d, &[], cv), before, "inside: {clicked_inside}");
    }
}

#[test]
fn an_insert_on_an_element_wire_reads_element_at_depth_one() {
    let mut d = setup();
    let mp = id(&d, "mp");
    let k = body_id(&d, &[mp], "k");
    let element = SourcePin::ZoneInput { pin_index: 0 };
    let x = insert_only(&mut d, &[mp], &wire(k, 0, mp), &[mp], "expr");
    assert_eq!(wires_into(&d, &[mp], x)[0], [(mp, element, 1)]);
    assert_eq!(wires_into(&d, &[mp], k)[0], [(x, OUT, 0)]);
    // Clicked outside the body: clamped into it, the only place `element`
    // is visible.
    let mut d = setup();
    let plan = d
        .plan_insert_on_wire(&[mp], &wire(k, 0, mp), &[], &new("expr"))
        .unwrap();
    let x = d
        .insert_node_on_wire(&[mp], &wire(k, 0, mp), &[], &new("expr"), &plan[0])
        .unwrap();
    assert!(d.get_scope_network(&[mp]).unwrap().nodes.contains_key(&x));
}

#[test]
fn an_expr_insert_evaluates_as_before() {
    let mut d = setup();
    let fe = id(&d, "fe");
    let before = display(&mut d, &[], fe);
    let w = wire(fe, 0, id(&d, "f"));
    let x = insert_only(&mut d, &[], &w, &[], "expr");
    assert_eq!(display(&mut d, &[], fe), before);
    assert_eq!(display(&mut d, &[], x), "2.000000");
}

/// Add a comment in `scope` anchored to each of `anchors`.
fn add_comment(d: &mut StructureDesigner, anchors: Vec<WireAnchor>) -> u64 {
    let comment = d.add_node("Comment", DVec2::new(0.0, 600.0));
    let node = d
        .get_scope_network_mut(&[])
        .unwrap()
        .nodes
        .get_mut(&comment)
        .unwrap();
    node.data
        .as_any_mut()
        .downcast_mut::<CommentData>()
        .unwrap()
        .anchors = anchors.into_iter().map(CommentAnchor::Wire).collect();
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
fn a_comment_on_the_wire_follows_its_downstream_half() {
    let mut d = setup();
    let (b, mt) = (id(&d, "b"), id(&d, "mt"));
    let comment = add_comment(&mut d, vec![wire(mt, 0, b)]);
    let mv = insert_only(&mut d, &[], &wire(mt, 0, b), &[], "structure_move");
    assert_eq!(
        anchors_of(&d, comment),
        [CommentAnchor::Wire(wire(mt, 0, mv))]
    );
}

#[test]
fn bad_requests_are_refused_and_change_nothing() {
    let mut d = setup();
    let (b, mt, s1) = (id(&d, "b"), id(&d, "mt"), id(&d, "s1"));
    let w = wire(mt, 0, b);
    let plan = d
        .plan_insert_on_wire(&[], &w, &[], &new("structure_move"))
        .unwrap();

    // An arrangement the plan does not offer: `diff`'s, for `structure_move`.
    let diff_plan = d.plan_insert_on_wire(&[], &w, &[], &new("diff")).unwrap();
    let stale = assert_no_mutation(&mut d, |d| {
        d.insert_node_on_wire(&[], &w, &[], &new("structure_move"), &diff_plan[1])
    });
    assert_eq!(stale, Err(SpliceRefusal::StaleArrangement));

    // A wire that is not there.
    let missing = assert_no_mutation(&mut d, |d| {
        d.insert_node_on_wire(&[], &wire(mt, 0, s1), &[], &new("structure_move"), &plan[0])
    });
    assert_eq!(missing, Err(SpliceRefusal::WireNotFound));
    assert_eq!(
        d.plan_insert_on_wire(&[], &wire(mt, 0, s1), &[], &new("structure_move")),
        Err(SpliceRefusal::WireNotFound)
    );

    // A type that does not exist.
    let unknown = assert_no_mutation(&mut d, |d| {
        d.insert_node_on_wire(&[], &w, &[], &new("no_such_node"), &plan[0])
    });
    assert_eq!(
        unknown,
        Err(SpliceRefusal::UnknownNodeType("no_such_node".to_string()))
    );

    // A scope that does not exist.
    assert_eq!(
        assert_no_mutation(&mut d, |d| d.plan_insert_on_wire(
            &[999],
            &w,
            &[],
            &new("expr")
        )),
        Err(SpliceRefusal::ScopeNotFound)
    );
}

/// A design whose network `inner` is used by `outer`, which returns the
/// `structure_move` of an `inner` instance; `inner` is active.
fn nested(inner: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network("inner");
    d.set_active_node_network_name(Some("inner".to_string()));
    assert!(d.ai_text_edit(inner, false).result.errors.is_empty());
    d.add_node_network("outer");
    d.set_active_node_network_name(Some("outer".to_string()));
    let outer = "i = inner {}
mv = structure_move { input: i }
output mv
";
    assert!(d.ai_text_edit(outer, false).result.errors.is_empty());
    d.set_active_node_network_name(Some("inner".to_string()));
    d.validate_active_network();
    d
}

/// Found by the corpus run: a `parameter` spliced into a network gives its
/// callers an argument slot, which undo must take away again.
#[test]
fn inserting_a_parameter_round_trips_the_callers_argument_slots() {
    let mut d = nested(
        "c = cuboid {}
m = structure_move { input: c }
output m
",
    );
    let (c, m) = (id(&d, "c"), id(&d, "m"));
    let plan = d
        .plan_insert_on_wire(&[], &wire(m, 0, c), &[], &new("parameter"))
        .unwrap();
    assert_eq!(pairs(&plan), [("default".into(), "result".into())]);
    assert_one_step_round_trip(&mut d, |d| {
        d.insert_node_on_wire(&[], &wire(m, 0, c), &[], &new("parameter"), &plan[0])
            .unwrap()
    });
}

/// Found by the corpus run: an insert that changes a network's output type
/// changes its callers' derived types, which undo must restore too.
#[test]
fn an_insert_that_changes_the_output_type_round_trips_the_callers_types() {
    let mut d = nested(
        "c = cuboid {}
m = structure_move { input: c }
output m
",
    );
    let (c, m) = (id(&d, "c"), id(&d, "m"));
    let plan = d
        .plan_insert_on_wire(&[], &wire(m, 0, c), &[], &new("materialize"))
        .unwrap();
    assert_eq!(
        plan[0].output.as_ref().unwrap().data_type,
        DataType::Crystal
    );
    assert_one_step_round_trip(&mut d, |d| {
        d.insert_node_on_wire(&[], &wire(m, 0, c), &[], &new("materialize"), &plan[0])
            .unwrap()
    });
    let outer = &d.node_type_registry.node_networks["outer"];
    assert_eq!(
        outer.node_type.output_pins[0].data_type,
        atomcad_structure_designer::node_type::PinOutputType::Fixed(DataType::Crystal)
    );
}

// ---------------------------------------------------------------------------
// replace_wire_source
// ---------------------------------------------------------------------------

#[test]
fn replace_wire_source_keeps_the_slot_position_and_only_its_own_anchors() {
    let mut d = setup();
    let (s1, s2, s3, u, b) = (
        id(&d, "s1"),
        id(&d, "s2"),
        id(&d, "s3"),
        id(&d, "u"),
        id(&d, "b"),
    );
    let comment = add_comment(&mut d, vec![wire(u, 0, s2), wire(u, 0, s3)]);

    let anchor = d
        .replace_wire_source(&[], &wire(u, 0, s2), b, OUT, 0)
        .unwrap();
    assert_eq!(anchor, wire(u, 0, b));
    assert_eq!(
        wires_into(&d, &[], u)[0],
        [(s1, OUT, 0), (b, OUT, 0), (s3, OUT, 0)]
    );
    // The sibling wire's comment keeps its own anchor.
    assert_eq!(
        anchors_of(&d, comment),
        [
            CommentAnchor::Wire(wire(u, 0, b)),
            CommentAnchor::Wire(wire(u, 0, s3))
        ]
    );
}

#[test]
fn replace_wire_source_refuses_a_source_the_slot_already_holds() {
    let mut d = setup();
    let (s1, s2, u) = (id(&d, "s1"), id(&d, "s2"), id(&d, "u"));
    let argument = d.get_scope_network(&[]).unwrap().nodes[&u].arguments[0].clone();

    // `slot_accepts_source` agrees with the primitive on every case.
    let cases = [
        (s2, s1, false), // `s1` is already in the slot
        (s2, s2, true),  // the same source, re-pinned
        (s2, id(&d, "b"), true),
        (id(&d, "b"), s1, false), // not in the slot at all
    ];
    for (replaced, new_source, accepted) in cases {
        assert_eq!(
            slot_accepts_source(&argument, replaced, new_source),
            accepted,
            "{replaced} -> {new_source}"
        );
    }

    let refused = assert_no_mutation(&mut d, |d| {
        d.replace_wire_source(&[], &wire(u, 0, s2), s1, OUT, 0)
    });
    assert_eq!(refused, Err(ReplaceWireSourceError::SourceAlreadyInSlot));
    let missing = assert_no_mutation(&mut d, |d| {
        d.replace_wire_source(&[], &wire(u, 0, id(d, "b")), s1, OUT, 0)
    });
    assert_eq!(missing, Err(ReplaceWireSourceError::WireNotFound));
    assert!(
        d.replace_wire_source(&[], &wire(u, 0, s2), s2, OUT, 0)
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

fn position(d: &StructureDesigner, node_id: u64) -> DVec2 {
    d.get_scope_network(&[]).unwrap().nodes[&node_id].position
}

fn drawing(d: &StructureDesigner) -> Drawing {
    Drawing::of(d.get_scope_network(&[]).unwrap(), &d.node_type_registry)
}

fn width(d: &StructureDesigner, node_id: u64) -> f64 {
    atomcad_structure_designer::layout::rendered_node_size(
        &d.get_scope_network(&[]).unwrap().nodes[&node_id],
        &d.node_type_registry,
    )
    .x
}

#[test]
fn with_no_room_the_downstream_half_shifts_right_and_the_upstream_stays() {
    let mut d = setup();
    let (b, mt) = (id(&d, "b"), id(&d, "mt"));
    let (b_at, mt_at) = (position(&d, b), position(&d, mt));
    assert!(
        mt_at.x - (b_at.x + width(&d, b)) < 200.0,
        "the fixture leaves no room for a node"
    );
    let before = drawing(&d);

    let mv = insert_only(&mut d, &[], &wire(mt, 0, b), &[], "structure_move");

    let after = drawing(&d);
    check_no_new_overlap(&before, &after);
    check_no_wire_flipped(&before, &after);
    check_bodies(&after);
    assert_eq!(position(&d, b), b_at, "the upstream chain stays put");
    assert!(
        position(&d, mt).x > mt_at.x,
        "the downstream half moves right"
    );
    // Between its neighbours.
    assert!(position(&d, mv).x > b_at.x + width(&d, b));
    assert!(position(&d, mv).x + width(&d, mv) < position(&d, mt).x);
}

#[test]
fn with_room_the_node_lands_between_its_neighbours_and_nothing_moves() {
    let mut d = setup();
    let (b, mt) = (id(&d, "b"), id(&d, "mt"));
    // Open a wide gap: everything right of `b` moves 1000 to the right.
    let b_right = position(&d, b).x + width(&d, b);
    let network = d.get_scope_network_mut(&[]).unwrap();
    for node in network.nodes.values_mut() {
        if node.position.x >= b_right {
            node.position.x += 1000.0;
        }
    }
    let (b_at, mt_at) = (position(&d, b), position(&d, mt));
    let before = drawing(&d);

    let mv = insert_only(&mut d, &[], &wire(mt, 0, b), &[], "structure_move");

    let after = drawing(&d);
    check_no_new_overlap(&before, &after);
    check_no_wire_flipped(&before, &after);
    assert_eq!(position(&d, b), b_at);
    assert_eq!(position(&d, mt), mt_at);
    assert!(position(&d, mv).x > b_at.x + width(&d, b));
    assert!(position(&d, mv).x + width(&d, mv) < mt_at.x);
}
