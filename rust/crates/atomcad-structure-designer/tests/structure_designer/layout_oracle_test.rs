//! Self-tests for [`layout_oracle`](super::layout_oracle).
//!
//! The oracle is the safety net for every later phase, so it needs one of its
//! own: an invariant checker that silently passes everything is worse than no
//! checker, because it makes the scenario tests look better covered than they
//! are. Each test below breaks exactly one invariant and asserts the oracle
//! notices.
//!
//! The "tolerates a pre-existing violation" halves matter just as much. Both
//! hand-drawn corpora contain overlapping nodes and backward wires their
//! authors are content with (152 overlaps and 177 backward wires in the
//! maintainer's working file), and an oracle that flagged those would make the
//! corpus run useless on day one. *Repair only what this edit broke.*

use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::edit_network;
use glam::DVec2;

use super::layout_oracle::{Drawing, Touched, check, check_no_new_overlap, check_no_wire_flipped};
use super::layout_test_support::{empty_network, id_by_path};

/// `a -> b`, laid out left to right with room to spare.
fn chain() -> (NodeNetwork, NodeTypeRegistry) {
    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"
a = int { value: 1 }
b = expr { x: a, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
"#,
        false,
    );
    assert!(result.success, "{:?}", result.errors);
    place(&mut network, "a", DVec2::new(100.0, 100.0));
    place(&mut network, "b", DVec2::new(400.0, 100.0));
    (network, registry)
}

fn place(network: &mut NodeNetwork, name: &str, position: DVec2) {
    let id = id_by_path(network, &[name]);
    network.nodes.get_mut(&id).unwrap().position = position;
}

#[test]
fn a_drawing_that_did_not_change_passes_everything() {
    let (network, registry) = chain();
    let drawing = Drawing::of(&network, &registry);
    check(&drawing, &drawing, &Touched::nothing());
}

#[test]
#[should_panic(expected = "new overlap")]
fn a_new_overlap_is_caught() {
    let (mut network, registry) = chain();
    let before = Drawing::of(&network, &registry);
    place(&mut network, "b", DVec2::new(110.0, 105.0));
    let after = Drawing::of(&network, &registry);
    check_no_new_overlap(&before, &after);
}

#[test]
fn a_pre_existing_overlap_is_tolerated() {
    let (mut network, registry) = chain();
    place(&mut network, "b", DVec2::new(110.0, 105.0));
    let before = Drawing::of(&network, &registry);
    // The same overlap, one pixel over: still overlapping, still not this
    // edit's fault.
    place(&mut network, "b", DVec2::new(111.0, 105.0));
    let after = Drawing::of(&network, &registry);
    check_no_new_overlap(&before, &after);
}

#[test]
#[should_panic(expected = "flipped backward")]
fn a_flipped_wire_is_caught() {
    let (mut network, registry) = chain();
    let before = Drawing::of(&network, &registry);
    place(&mut network, "b", DVec2::new(0.0, 400.0));
    let after = Drawing::of(&network, &registry);
    check_no_wire_flipped(&before, &after);
}

#[test]
fn an_already_backward_wire_is_left_alone() {
    let (mut network, registry) = chain();
    place(&mut network, "b", DVec2::new(0.0, 400.0));
    let before = Drawing::of(&network, &registry);
    place(&mut network, "b", DVec2::new(10.0, 400.0));
    let after = Drawing::of(&network, &registry);
    check_no_wire_flipped(&before, &after);
}

#[test]
#[should_panic(expected = "in neither the delta nor the move list")]
fn an_undeclared_move_is_caught() {
    let (mut network, registry) = chain();
    let before = Drawing::of(&network, &registry);
    place(&mut network, "b", DVec2::new(400.0, 140.0));
    let after = Drawing::of(&network, &registry);
    check(&before, &after, &Touched::nothing());
}

#[test]
fn a_declared_move_is_accepted() {
    let (mut network, registry) = chain();
    let before = Drawing::of(&network, &registry);
    place(&mut network, "b", DVec2::new(400.0, 140.0));
    let after = Drawing::of(&network, &registry);
    check(&before, &after, &Touched::new([], [vec!["b".to_string()]]));
}

#[test]
#[should_panic(expected = "negative coordinate")]
fn a_negative_body_coordinate_is_caught() {
    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = int { value: 1 }
    output d
  }
}
"#,
        false,
    );
    assert!(result.success, "{:?}", result.errors);

    let m = id_by_path(&network, &["m"]);
    let d = id_by_path(&network, &["m", "d"]);
    let body = network.nodes.get_mut(&m).unwrap().zone_mut().unwrap();
    // A body's extent is measured from its own origin, so a node at a negative
    // coordinate simply renders outside the box.
    body.nodes.get_mut(&d).unwrap().position = DVec2::new(-40.0, 10.0);

    let drawing = Drawing::of(&network, &registry);
    check(&drawing, &drawing, &Touched::nothing());
}

#[test]
fn determinism_holds_for_an_unchanging_network() {
    let (network, registry) = chain();
    super::layout_oracle::assert_deterministic(|| Drawing::of(&network, &registry));
}
