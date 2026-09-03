//! Layout end to end, through the surfaces a user actually reaches
//! (`doc/design_incremental_layout.md` Phase 5).
//!
//! Everything under `layout_*_test.rs` up to now calls a layout function
//! directly. These tests go through the two real entry points instead:
//!
//! - **[`StructureDesigner::ai_text_edit`]**, the AI edit choke point, which
//!   now runs the incremental pass rather than reflowing the whole network —
//!   the change the whole design exists to make. There is no preference gating
//!   it: the pass is *repair*, not layout.
//! - **[`StructureDesigner::layout_active_network`]**, the *Edit >
//!   Auto-Layout Network* menu item, which is now **body-aware**: it descends
//!   into every HOF body, deepest first, so an expanded HOF's contents are
//!   arranged before the scope holding it is. Before Phase 5 a body was never
//!   laid out at all, which made "the reflow is the cure" (D4) false for
//!   exactly the drawings that needed it.

use std::collections::HashSet;

use glam::DVec2;

use atomcad_structure_designer::layout::rendered_node_size;
use atomcad_structure_designer::node_layout::nodes_overlap;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::NamePath;

use super::layout_oracle::{self, Drawing};
use super::layout_test_support::{p, touched_between};

// ============================================================================
// Harness
// ============================================================================

/// A designer with one empty active network called `main`.
///
/// `StructureDesigner::new()` loads the **real** user preferences, so anything
/// these tests depend on is pinned here rather than inherited from whichever
/// machine runs the suite (`project_issue_270_auto_layout_undo`).
fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.preferences
        .layout_preferences
        .respect_hand_moved_in_reflow = false;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

fn edit(sd: &mut StructureDesigner, code: &str) {
    let outcome = sd.ai_text_edit(code, false);
    assert!(
        outcome.result.success,
        "edit failed: {:?}",
        outcome.result.errors
    );
}

fn network(sd: &StructureDesigner) -> &NodeNetwork {
    sd.node_type_registry
        .node_networks
        .get("main")
        .expect("the active network")
}

fn drawing(sd: &StructureDesigner) -> Drawing {
    Drawing::of(network(sd), &sd.node_type_registry)
}

/// The name paths the last logged edit reports having moved.
fn moved_paths(sd: &StructureDesigner) -> HashSet<NamePath> {
    sd.ai_edit_log
        .last()
        .expect("every edit is logged")
        .layout
        .moved
        .iter()
        .map(|m| m.path.clone())
        .collect()
}

/// Put a node where the "human" wants it, in the top-level scope.
fn place(sd: &mut StructureDesigner, name: &str, x: f64, y: f64) {
    let network = sd
        .node_type_registry
        .node_networks
        .get_mut("main")
        .expect("the active network");
    let id = network
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}`"))
        .id;
    network.nodes.get_mut(&id).expect("just found").position = DVec2::new(x, y);
}

/// Assert no two nodes overlap, in **any** scope, at their rendered sizes.
///
/// Stronger than the oracle's invariant 1, which only forbids *new* overlaps:
/// these networks are built by the test, so there is no inherited mess to
/// tolerate and the reflow has no excuse.
fn assert_no_overlap_anywhere(sd: &StructureDesigner) {
    fn walk(scope: &NodeNetwork, path: &str, sd: &StructureDesigner) {
        let mut ids: Vec<u64> = scope.nodes.keys().copied().collect();
        ids.sort_unstable();
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                let (na, nb) = (&scope.nodes[&a], &scope.nodes[&b]);
                let (sa, sb) = (
                    rendered_node_size(na, &sd.node_type_registry),
                    rendered_node_size(nb, &sd.node_type_registry),
                );
                assert!(
                    !nodes_overlap(na.position, sa, nb.position, sb, 0.0),
                    "in scope `{path}`, `{}` at {:?}/{:?} overlaps `{}` at {:?}/{:?}",
                    na.custom_name.clone().unwrap_or_default(),
                    na.position,
                    sa,
                    nb.custom_name.clone().unwrap_or_default(),
                    nb.position,
                    sb,
                );
            }
        }
        for &id in &ids {
            if let Some(body) = scope.nodes[&id].zone.as_deref() {
                let name = scope.nodes[&id].custom_name.clone().unwrap_or_default();
                walk(body, &format!("{path}/{name}"), sd);
            }
        }
    }
    walk(network(sd), "", sd);
}

/// A `map` whose body holds a three-node chain, plus a neighbour to its right.
const MAP_WITH_BODY: &str = r#"
r = range { start: 0, step: 1, count: 5 }
scale = int { value: 3 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    e = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    f = expr { a: e, expression: "a + 2", parameters: [{ name: "a", data_type: Int }] }
    output f
  }
}
output m1
"#;

// ============================================================================
// The AI edit path
// ============================================================================

/// The headline property, end to end: an AI edit fits its own node in and
/// leaves the drawing alone. The same edit before Phase 5 reflowed the network
/// and rewrote every position in it.
#[test]
fn an_ai_added_node_moves_nothing_that_was_already_there() {
    let mut sd = designer();
    edit(
        &mut sd,
        "a = int { value: 1 }\nb = expr { x: a, expression: \"x + 1\", parameters: [{ name: \"x\", data_type: Int }] }\n",
    );
    // Arrange the two nodes the way a human would, nowhere near a canonical
    // Sugiyama column.
    place(&mut sd, "a", 137.0, 402.0);
    place(&mut sd, "b", 511.0, 388.0);
    let before = drawing(&sd);

    edit(&mut sd, "c = int { value: 9 }\n");

    let after = drawing(&sd);
    layout_oracle::check(
        &before,
        &after,
        &touched_between(&before, &after, moved_paths(&sd)),
    );
    assert_eq!(
        before.get(&p(&["a"])).unwrap().position,
        after.get(&p(&["a"])).unwrap().position,
        "`a` must be bit-identical"
    );
    assert_eq!(
        before.get(&p(&["b"])).unwrap().position,
        after.get(&p(&["b"])).unwrap().position,
        "`b` must be bit-identical"
    );
    assert!(moved_paths(&sd).is_empty(), "nothing may have moved");
}

/// A `query` → `edit --replace` round trip: the text format carries no
/// coordinates, so every position in the result comes from the D14 identity
/// snapshot being re-applied on a name match. If any part of that chain is
/// broken the whole drawing silently resets — including, before Phase 1, every
/// HOF's body size and collapse mode.
#[test]
fn a_replace_round_trip_of_a_body_bearing_network_moves_nothing() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    // Scatter everything, so "unchanged" cannot be satisfied by the layout
    // happening to agree with the placer.
    place(&mut sd, "r", 90.0, 610.0);
    place(&mut sd, "scale", 120.0, 330.0);
    place(&mut sd, "m1", 470.0, 505.0);
    let before = drawing(&sd);

    // What `query` returns is exactly what the log records as `before_text`.
    let script = sd.ai_edit_log.last().expect("logged").after_text.clone();
    let outcome = sd.ai_text_edit(&script, true);
    assert!(
        outcome.result.success,
        "the round trip failed: {:?}",
        outcome.result.errors
    );

    let after = drawing(&sd);
    for path in before.paths() {
        let was = before.get(&path).expect("enumerated");
        let now = after
            .get(&path)
            .unwrap_or_else(|| panic!("`{}` vanished across the round trip", path.join("/")));
        assert_eq!(
            was.position,
            now.position,
            "`{}` moved across a --replace of an unchanged script",
            path.join("/")
        );
    }
    assert!(
        moved_paths(&sd).is_empty(),
        "a --replace of an unchanged script must move nothing: {:?}",
        moved_paths(&sd)
    );
}

/// The inside-out driver, end to end: a statement addressing a node inside a
/// body is laid out in the body's own scope, and a body with slack absorbs it
/// without disturbing the parent.
#[test]
fn an_ai_edit_inside_a_body_leaves_the_parent_scope_alone() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    place(&mut sd, "r", 90.0, 610.0);
    place(&mut sd, "scale", 120.0, 330.0);
    place(&mut sd, "m1", 470.0, 505.0);
    let before = drawing(&sd);

    edit(&mut sd, "m1/extra = int { value: 7 }\n");

    let after = drawing(&sd);
    layout_oracle::check(
        &before,
        &after,
        &touched_between(&before, &after, moved_paths(&sd)),
    );
    for name in ["r", "scale"] {
        assert_eq!(
            before.get(&p(&[name])).unwrap().position,
            after.get(&p(&[name])).unwrap().position,
            "`{name}` is in the parent scope and must not move"
        );
    }
    assert!(
        after.get(&p(&["m1", "extra"])).is_some(),
        "the body node was created in the body"
    );
}

// ============================================================================
// The full reflow (Edit > Auto-Layout Network)
// ============================================================================

/// The claim Phase 5 makes about the menu item: a reflow of a network holding
/// an expanded HOF leaves **no** overlap, in any scope. It could not before,
/// because the body was never laid out and the HOF was measured against
/// whatever its body happened to look like.
#[test]
fn a_full_reflow_with_an_expanded_hof_produces_no_overlap() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);

    // Pile everything onto one point, in both scopes, so the reflow has real
    // work to do and any scope it skips shows up as an overlap.
    for name in ["r", "scale", "m1"] {
        place(&mut sd, name, 0.0, 0.0);
    }
    {
        let network = sd
            .node_type_registry
            .node_networks
            .get_mut("main")
            .expect("active");
        let m1 = network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some("m1"))
            .expect("m1")
            .id;
        let body = network
            .nodes
            .get_mut(&m1)
            .and_then(|node| node.zone_mut())
            .expect("m1 owns a body");
        let ids: Vec<u64> = body.nodes.keys().copied().collect();
        for id in ids {
            body.nodes.get_mut(&id).expect("just listed").position = DVec2::ZERO;
        }
    }

    assert!(
        sd.layout_active_network(),
        "the reflow must have moved something"
    );
    assert_no_overlap_anywhere(&sd);
}

/// The body half of the same claim, stated positively: the reflow reaches into
/// the body and rearranges it.
#[test]
fn a_full_reflow_lays_out_body_nodes() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    let before = drawing(&sd);

    {
        let network = sd
            .node_type_registry
            .node_networks
            .get_mut("main")
            .expect("active");
        let m1 = network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some("m1"))
            .expect("m1")
            .id;
        let body = network
            .nodes
            .get_mut(&m1)
            .and_then(|node| node.zone_mut())
            .expect("m1 owns a body");
        let ids: Vec<u64> = body.nodes.keys().copied().collect();
        for id in ids {
            body.nodes.get_mut(&id).expect("just listed").position = DVec2::ZERO;
        }
    }

    sd.layout_active_network();

    let after = drawing(&sd);
    let moved_in_body = ["d", "e", "f"]
        .iter()
        .filter(|name| {
            let path = p(&["m1", name]);
            before.get(&path).map(|r| r.position) != after.get(&path).map(|r| r.position)
        })
        .count();
    assert!(
        moved_in_body >= 2,
        "the reflow must arrange the body, not just the root scope"
    );
}

/// A reflow that moves nodes in two scopes is still **one** undo step, and one
/// `Ctrl+Z` restores every scope.
#[test]
fn a_body_aware_reflow_is_a_single_undo_step() {
    let mut sd = designer();
    edit(&mut sd, MAP_WITH_BODY);
    for name in ["r", "scale", "m1"] {
        place(&mut sd, name, 0.0, 0.0);
    }
    let before = drawing(&sd);

    assert!(sd.layout_active_network());
    assert_ne!(
        before.get(&p(&["r"])).unwrap().position,
        drawing(&sd).get(&p(&["r"])).unwrap().position
    );

    assert!(sd.undo(), "one undo reverts the whole reflow");
    let restored = drawing(&sd);
    for path in before.paths() {
        assert_eq!(
            before.get(&path).unwrap().position,
            restored.get(&path).unwrap().position,
            "`{}` was not restored by the single undo",
            path.join("/")
        );
    }
    // Exactly one command, not one per scope: a second undo must reach past the
    // reflow entirely.
    assert_ne!(
        sd.undo_stack.undo_description(),
        Some("Auto-Layout Network"),
        "the reflow pushed more than one undo step"
    );
}

/// The third documented use of `hand_moved`, and the only place in this design
/// where the flag is a hard constraint: with the preference on, a reflow leaves
/// the nodes the user dragged exactly where they are.
#[test]
fn a_reflow_respecting_hand_moved_leaves_them_bit_identical() {
    let mut sd = designer();
    sd.preferences
        .layout_preferences
        .respect_hand_moved_in_reflow = true;
    edit(
        &mut sd,
        "a = int { value: 1 }\nb = expr { x: a, expression: \"x + 1\", parameters: [{ name: \"x\", data_type: Int }] }\nc = expr { x: b, expression: \"x + 2\", parameters: [{ name: \"x\", data_type: Int }] }\n",
    );
    place(&mut sd, "b", 733.0, 921.0);
    {
        let network = sd
            .node_type_registry
            .node_networks
            .get_mut("main")
            .expect("active");
        let id = network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some("b"))
            .expect("b")
            .id;
        network.nodes.get_mut(&id).expect("b").hand_moved = true;
    }
    let pinned = drawing(&sd).get(&p(&["b"])).unwrap().position;

    sd.layout_active_network();

    assert_eq!(
        drawing(&sd).get(&p(&["b"])).unwrap().position,
        pinned,
        "a hand-placed node must survive a respecting reflow untouched"
    );
    assert_no_overlap_anywhere(&sd);
}

/// The control for the test above: with the preference off — the default — the
/// same reflow moves the hand-placed node like any other.
#[test]
fn a_reflow_ignores_hand_moved_by_default() {
    let mut sd = designer();
    edit(
        &mut sd,
        "a = int { value: 1 }\nb = expr { x: a, expression: \"x + 1\", parameters: [{ name: \"x\", data_type: Int }] }\nc = expr { x: b, expression: \"x + 2\", parameters: [{ name: \"x\", data_type: Int }] }\n",
    );
    place(&mut sd, "b", 733.0, 921.0);
    {
        let network = sd
            .node_type_registry
            .node_networks
            .get_mut("main")
            .expect("active");
        let id = network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some("b"))
            .expect("b")
            .id;
        network.nodes.get_mut(&id).expect("b").hand_moved = true;
    }
    let pinned = drawing(&sd).get(&p(&["b"])).unwrap().position;

    sd.layout_active_network();

    assert_ne!(
        drawing(&sd).get(&p(&["b"])).unwrap().position,
        pinned,
        "off by default: a reflow the user asked for is total"
    );
}
