//! Steps 3, 4 and 5 of the incremental layout pass, and the inside-out driver
//! (`doc/design_incremental_layout.md` Phase 3).
//!
//! Every test here arranges a "before" drawing **by hand** and then runs one
//! text edit through [`layout_incremental`], because the property the whole
//! design exists to buy is about a drawing a human made: the added nodes are
//! laid out as a block and fitted in, and nothing else moves unless the block
//! left it no room.
//!
//! Each test asserts only what is specific to it and calls the shared
//! [oracle](super::layout_oracle) for the seven whole-pass invariants — no new
//! overlap, no wire flipped, untouched means untouched, bodies contained and
//! non-negative.

use std::collections::HashSet;

use glam::DVec2;

use atomcad_structure_designer::layout::{LayoutAlgorithm, layout_subgraph, rendered_node_size};
use atomcad_structure_designer::node_layout::DEFAULT_HORIZONTAL_GAP as GAP;

use super::layout_oracle::{self, Drawing};
use super::layout_test_support::{
    Fixture, VERTICAL_GAP, center, expr, node_by_path, node_by_path_mut, p,
};

/// A `map` whose body is a `$element -> expr` and back out again.
const MAP_WITH_BODY: &str = r#"
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
"#;

// ============================================================================
// Step 4 — where a block lands, at the top level
// ============================================================================

#[test]
fn one_added_node_lands_beside_its_input_and_nothing_else_moves() {
    let mut fixture = Fixture::new("a = int { value: 1 }\n");
    fixture.place(&["a"], 100.0, 100.0);

    fixture.edit(&expr("b", "a"));
    let outcome = fixture.run();

    let (a, b) = (outcome.rect(&["a"]), outcome.rect(&["b"]));
    assert_eq!(
        b.position.x,
        a.position.x + a.size.x + GAP,
        "the block sits exactly one gap right of its input"
    );
    assert_eq!(
        center(b).y,
        center(a).y,
        "a single wire is aligned exactly: the mean of one term is that term"
    );
    outcome.assert_only_moved(&[&["b"]]);
}

#[test]
fn a_twenty_node_addition_is_placed_as_one_rigid_block() {
    let mut fixture = Fixture::new("a = int { value: 1 }\n");
    fixture.place(&["a"], 100.0, 100.0);

    let mut code = String::new();
    for i in 0..20 {
        let source = if i == 0 {
            "a".to_string()
        } else {
            format!("n{}", i - 1)
        };
        code.push_str(&expr(&format!("n{i}"), &source));
    }
    fixture.edit(&code);

    // What the block's own layout produced, before it was placed. Positions do
    // not feed layout, so re-running it after the pass gives the same answer.
    let ids: HashSet<u64> = (0..20).map(|i| fixture.id(&[&format!("n{i}")])).collect();
    let local = layout_subgraph(
        &fixture.network,
        &fixture.registry,
        &ids,
        LayoutAlgorithm::Sugiyama,
    );

    let outcome = fixture.run();

    // Every added node moved by the *same* vector from its block-local
    // position: the block was placed as one rigid unit, not re-derived per node.
    let mut offsets: Vec<DVec2> = Vec::new();
    for i in 0..20 {
        let name = format!("n{i}");
        let id = fixture.id(&[&name]);
        offsets.push(outcome.rect(&[&name]).position - local[&id]);
    }
    for offset in &offsets {
        assert_eq!(
            *offset, offsets[0],
            "the block's internal arrangement was not preserved"
        );
    }
    assert!(
        outcome.shift(&["a"]).is_none(),
        "a 20-node addition beside an input moves nothing that was there"
    );
}

#[test]
fn an_anchorless_addition_goes_below_the_drawing_at_its_left_edge() {
    let mut fixture = Fixture::new(&format!("a = int {{ value: 1 }}\n{}", expr("b", "a")));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["b"], 400.0, 260.0);

    fixture.edit("z = int { value: 5 }\n");
    let outcome = fixture.run();

    let (a, b, z) = (
        outcome.rect(&["a"]),
        outcome.rect(&["b"]),
        outcome.rect(&["z"]),
    );
    let left = a.position.x.min(b.position.x);
    let bottom = (a.position.y + a.size.y).max(b.position.y + b.size.y);
    assert_eq!(
        z.position.x, left,
        "flush with the drawing's left edge — a source placed leftmost can only          ever get forward wires"
    );
    assert_eq!(
        z.position.y,
        bottom + VERTICAL_GAP,
        "and below it — an anchorless block has nothing to align to"
    );
    outcome.assert_only_moved(&[&["z"]]);
}

// ============================================================================
// Step 4 — the "no room" half-plane shift
// ============================================================================

#[test]
fn a_block_needing_a_new_column_shifts_the_half_plane_by_the_deficit() {
    let mut fixture = Fixture::new(&format!("a = int {{ value: 1 }}\n{}", expr("c", "a")));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["c"], 310.0, 100.0);

    // Insert `b` between them: the assignment to `c.x` rewires it in one step.
    fixture.edit(&format!("{}{}", expr("b", "a"), expr("c", "b")));
    let outcome = fixture.run();

    let (a, b) = (outcome.rect(&["a"]), outcome.rect(&["b"]));
    assert_eq!(
        a.position,
        DVec2::new(100.0, 100.0),
        "the input and its upstream closure never move"
    );
    assert_eq!(b.position.x, a.position.x + a.size.x + GAP);
    assert_eq!(
        outcome.shift(&["c"]),
        Some(DVec2::new(b.size.x + GAP, 0.0)),
        "the consumer moves right by exactly the room the block needed"
    );
}

#[test]
fn a_consumer_overlapping_its_input_horizontally_is_still_moved() {
    // `x_min` (right of `a` plus a gap) lies *right* of the consumer's left
    // edge, so the window rule's clamp to `min d.x` is the only thing that
    // makes `c` move at all.
    let mut fixture = Fixture::new(&format!("a = int {{ value: 1 }}\n{}", expr("c", "a")));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["c"], 200.0, 100.0);

    fixture.edit(&format!("{}{}", expr("b", "a"), expr("c", "b")));
    let outcome = fixture.run();

    let (a, b, c) = (
        outcome.rect(&["a"]),
        outcome.rect(&["b"]),
        outcome.rect(&["c"]),
    );
    assert_eq!(a.position, DVec2::new(100.0, 100.0));
    assert_eq!(b.position.x, a.position.x + a.size.x + GAP);
    assert!(
        c.position.x >= b.position.x + b.size.x + GAP,
        "the consumer ended up clear of the block: {c:?} vs {b:?}"
    );
}

#[test]
fn a_consumer_left_of_its_input_is_moved_past_the_block_and_the_chain_stays() {
    // The genuinely backward arrangement: `c` sits left of `a`'s left edge. A
    // plain half-plane shift could never fix it, since any line left of `c`
    // would carry `a` along — the upstream closure in `fixed` is what holds
    // `s` and `a` still while everything else moves right.
    let mut fixture = Fixture::new(&format!(
        "s = int {{ value: 1 }}\n{}{}",
        expr("a", "s"),
        expr("c", "a")
    ));
    fixture.place(&["s"], -200.0, 100.0);
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["c"], 50.0, 100.0);

    fixture.edit(&format!("{}{}", expr("b", "a"), expr("c", "b")));
    let outcome = fixture.run();

    assert_eq!(outcome.rect(&["s"]).position, DVec2::new(-200.0, 100.0));
    assert_eq!(outcome.rect(&["a"]).position, DVec2::new(100.0, 100.0));
    let (b, c) = (outcome.rect(&["b"]), outcome.rect(&["c"]));
    assert!(
        c.position.x >= b.position.x + b.size.x + GAP,
        "the destination was moved past the block it now reads from"
    );
}

// ============================================================================
// Step 1 — an added node is invisible until its block is placed
// ============================================================================

#[test]
fn a_new_node_dropped_onto_a_kept_node_moves_nothing_before_its_block_is_placed() {
    let mut fixture = Fixture::new(&format!("a = int {{ value: 1 }}\n{}", expr("k", "a")));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["k"], 100.0, 400.0);

    fixture.edit(&expr("b", "a"));
    // The creation-time placer can drop a new node straight on top of an
    // existing one. It must not push it: an added node is not an obstacle and
    // not a mover until Step 5 places its block.
    node_by_path_mut(&mut fixture.network, &["b"]).position = DVec2::new(100.0, 400.0);

    let outcome = fixture.run();

    assert_eq!(
        outcome.rect(&["k"]).position,
        DVec2::new(100.0, 400.0),
        "the junk position of a new node must not disturb the drawing"
    );
    let (a, b) = (outcome.rect(&["a"]), outcome.rect(&["b"]));
    assert_eq!(b.position.x, a.position.x + a.size.x + GAP);
    outcome.assert_only_moved(&[&["b"]]);
}

// ============================================================================
// D9 — comments are ordinary obstacles and are never re-placed
// ============================================================================

#[test]
fn an_uncollided_comment_stays_put_on_either_side_of_its_anchor() {
    let mut fixture = Fixture::new(
        r#"
a = int { value: 1 }
before = Comment { label: "", text: "left of a", width: 200, height: 100, on: a }
after = Comment { label: "", text: "right of a", width: 200, height: 100, on: a }
"#,
    );
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["before"], -400.0, 100.0);
    fixture.place(&["after"], 100.0, 400.0);

    fixture.edit(&expr("b", "a"));
    let outcome = fixture.run();

    assert_eq!(
        outcome.rect(&["before"]).position,
        DVec2::new(-400.0, 100.0),
        "a comment's position is the thing being preserved (D9)"
    );
    assert_eq!(
        outcome.rect(&["after"]).position,
        DVec2::new(100.0, 400.0),
        "…on whichever side of its anchor it sits"
    );
    outcome.assert_only_moved(&[&["b"]]);
}

#[test]
fn a_block_slides_around_a_comment_at_its_real_size() {
    // A note parked exactly where the block's anchor wants it. Sliding moves
    // nothing, so the comment stays — but the block has to clear the comment's
    // *real* 100-px box, not the 83-px node estimate the old size helpers used.
    let mut fixture = Fixture::new(
        r#"
a = int { value: 1 }
note = Comment { label: "", text: "in the way", width: 200, height: 100, on: a }
"#,
    );
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["note"], 310.0, 60.0);

    fixture.edit(&expr("b", "a"));
    let outcome = fixture.run();

    let (note, b) = (outcome.rect(&["note"]), outcome.rect(&["b"]));
    assert_eq!(
        note.position,
        DVec2::new(310.0, 60.0),
        "a slide moves nothing at all"
    );
    assert!(
        b.position.y >= note.position.y + note.size.y,
        "the block cleared the comment's own box: {b:?} vs {note:?}"
    );
    outcome.assert_only_moved(&[&["b"]]);
}

#[test]
fn a_comment_too_tall_to_slide_past_is_pushed_by_the_minimum() {
    // A 200x300 note spanning more than the slide window: there is no free `y`
    // to slide to, so Step 5b places the block at its target and cascades. A
    // comment is an ordinary node on this path (D9) — it is pushed, at its real
    // size, by exactly the least that clears the block.
    let mut fixture = Fixture::new(
        r#"
a = int { value: 1 }
note = Comment { label: "", text: "in the way", width: 200, height: 300, on: a }
"#,
    );
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["note"], 310.0, 60.0);

    fixture.edit(&expr("b", "a"));
    let outcome = fixture.run();

    let (a, note, b) = (
        outcome.rect(&["a"]),
        outcome.rect(&["note"]),
        outcome.rect(&["b"]),
    );
    assert_eq!(
        b.position,
        DVec2::new(a.position.x + a.size.x + GAP, center(a).y - b.size.y / 2.0),
        "the block stayed at its anchor-derived target; the drawing moved"
    );
    assert_eq!(
        note.position,
        DVec2::new(310.0, b.position.y + b.size.y + VERTICAL_GAP),
        "and the comment went down by the minimum that clears the block"
    );
    assert_eq!(
        outcome.shift(&["a"]),
        None,
        "the cascade is vertical and local: the input is untouched"
    );
}

// ============================================================================
// Step 4 — bodies and their synthesized anchors
// ============================================================================

#[test]
fn a_body_block_lays_out_left_to_right_and_does_not_grow_a_default_body() {
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map { xs: r, input_type: Int, output_type: Int }
"#,
    );
    fixture.place(&["r"], -400.0, 100.0);
    fixture.place(&["m"], 100.0, 100.0);
    let map_before = fixture.before.get(&p(&["m"])).unwrap().size;

    fixture.edit(
        "m/d = expr { a: $element, expression: \"a + 1\", \
         parameters: [{ name: \"a\", data_type: Int }] }\noutput m/d\n",
    );
    let outcome = fixture.run();

    let d = outcome.rect(&["m", "d"]);
    assert_eq!(
        d.position,
        DVec2::new(GAP, 0.0),
        "one gap right of the body's left edge, and clamped to the origin: the \
         zone-input pin sits above the node's own centre, so the raw target is \
         negative and body coordinates are non-negative"
    );
    assert_eq!(
        outcome.rect(&["m"]).size,
        map_before,
        "the block fitted inside the stored body, so the HOF did not grow"
    );
    outcome.assert_only_moved(&[&["m", "d"]]);
}

#[test]
fn a_node_added_to_a_body_with_slack_leaves_the_parent_untouched() {
    let mut fixture = Fixture::new(MAP_WITH_BODY);
    fixture.place(&["r"], -400.0, 100.0);
    fixture.place(&["m"], 100.0, 100.0);
    fixture.place(&["m", "d"], 10.0, 10.0);
    // A body the user has dragged taller: room for a second row.
    fixture.body_size(&["m"], 320.0, 400.0);
    let map_before = fixture.before.get(&p(&["m"])).unwrap().size;

    fixture.edit(
        "m/e = expr { a: $element, expression: \"a + 1\", \
         parameters: [{ name: \"a\", data_type: Int }] }\n",
    );
    let outcome = fixture.run();

    let (d, e) = (outcome.rect(&["m", "d"]), outcome.rect(&["m", "e"]));
    assert_eq!(d.position, DVec2::new(10.0, 10.0), "the body's node stays");
    assert_eq!(e.position.x, GAP);
    assert!(
        e.position.y >= d.position.y + d.size.y + VERTICAL_GAP,
        "the slack-first slide found a free row below `d`: {e:?}"
    );
    assert_eq!(
        outcome.rect(&["m"]).size,
        map_before,
        "growth absorbed by the stored body size never reaches the parent (D4/D12)"
    );
    outcome.assert_only_moved(&[&["m", "e"]]);
}

#[test]
fn a_node_added_to_a_full_body_shifts_the_hofs_right_neighbour_by_the_width_delta() {
    let mut fixture = Fixture::new(&format!("{MAP_WITH_BODY}\nn = int {{ value: 7 }}\n"));
    fixture.place(&["r"], -400.0, 100.0);
    fixture.place(&["m"], 100.0, 100.0);
    fixture.place(&["n"], 700.0, 100.0);
    // Hard against the right edge of the stored body.
    fixture.place(&["m", "d"], 140.0, 10.0);
    let map_before = fixture.before.get(&p(&["m"])).unwrap().size;

    // Fed by `d`, so the block goes right of it — past the stored width.
    fixture.edit(&expr("m/e", "d"));
    let outcome = fixture.run();

    let map_after = outcome.rect(&["m"]).size;
    let width_delta = map_after.x - map_before.x;
    assert!(width_delta > 0.0, "the body grew past its stored width");
    assert_eq!(
        outcome.shift(&["n"]),
        Some(DVec2::new(width_delta, 0.0)),
        "the right neighbour takes the width delta and nothing else — the \
         height half of the growth is a cascade, and nothing collided"
    );
    assert_eq!(
        outcome.rect(&["m"]).position,
        DVec2::new(100.0, 100.0),
        "a grown node never moves itself"
    );
    assert!(outcome.shift(&["r"]).is_none(), "and its input stays put");
}

#[test]
fn growth_two_levels_down_cascades_to_the_grandparent() {
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
outer = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    inner = map {
      input_type: Int,
      output_type: Int,
      body {
        d = int { value: 1 }
        output d
      }
    }
    output inner
  }
}
n = int { value: 7 }
"#,
    );
    fixture.place(&["r"], -800.0, 100.0);
    fixture.place(&["outer"], 100.0, 100.0);
    fixture.place(&["n"], 1400.0, 100.0);
    fixture.place(&["outer", "inner"], 10.0, 10.0);
    fixture.place(&["outer", "inner", "d"], 10.0, 10.0);
    let outer_before = fixture.before.get(&p(&["outer"])).unwrap().size;

    // Fed by `d`, so the block goes right of it — past the stored width.
    fixture.edit(&expr("outer/inner/e", "d"));
    let outcome = fixture.run();

    let outer_after = outcome.rect(&["outer"]).size;
    assert!(
        outer_after.x > outer_before.x,
        "a node two levels down widened its own body, which widened its \
         owner's, which widened the HOF at the top"
    );
    assert_eq!(
        outcome.shift(&["n"]),
        Some(DVec2::new(outer_after.x - outer_before.x, 0.0)),
        "and the top-level neighbour took exactly that width delta"
    );
}

#[test]
fn a_new_hof_is_placed_at_its_settled_footprint() {
    let mut fixture = Fixture::new("a = int { value: 1 }\n");
    fixture.place(&["a"], 100.0, 100.0);

    fixture.edit(&format!(
        r#"
m2 = map {{
  input_type: Int,
  output_type: Int,
  body {{
    {}    {}    {}    output b3
  }}
}}
"#,
        "b1 = int { value: 1 }\n    ",
        expr("b2", "b1").replace('\n', "\n    "),
        expr("b3", "b2").replace('\n', "\n    "),
    ));
    let outcome = fixture.run();

    // The body settles first (D12), so the parent measures the HOF at the size
    // it will actually render at — the oracle's invariant 6 is the real
    // assertion here, and it is checked for every scope by `run`.
    let (a, m2) = (outcome.rect(&["a"]), outcome.rect(&["m2"]));
    assert!(
        m2.size.x > 460.0,
        "the three-node body widened the map well past its 320-px default: {m2:?}"
    );
    assert!(
        m2.position.x == a.position.x && m2.position.y >= a.position.y + a.size.y + VERTICAL_GAP,
        "an anchorless block goes below the drawing at its left edge: {m2:?} vs {a:?}"
    );
    for name in ["b1", "b2", "b3"] {
        let node = outcome.rect(&["m2", name]);
        assert!(
            node.position.x >= 0.0 && node.position.y >= 0.0,
            "body coordinates are non-negative: {name} at {:?}",
            node.position
        );
    }
}

// ============================================================================
// D7 — determinism
// ============================================================================

#[test]
fn the_same_edit_places_the_same_drawing_twice() {
    // Two full runs in one process, from identical input. `HashMap` iteration
    // order differs per process *and* per map instance, so a stray unordered
    // iteration shows up here as two different drawings.
    let build = || {
        let mut fixture = Fixture::new(&format!(
            "{MAP_WITH_BODY}\nk = int {{ value: 4 }}\nj = int {{ value: 5 }}\n"
        ));
        fixture.place(&["r"], -400.0, 100.0);
        fixture.place(&["m"], 100.0, 100.0);
        fixture.place(&["k"], 700.0, 100.0);
        fixture.place(&["j"], 700.0, 400.0);
        fixture.place(&["m", "d"], 140.0, 10.0);
        fixture.edit("m/e = int { value: 2 }\n");
        fixture.edit(&expr("c1", "k"));
        fixture.edit(&expr("c2", "k"));
        fixture.run();
        Drawing::of(&fixture.network, &fixture.registry)
    };

    layout_oracle::assert_deterministic(build);
}

// ============================================================================
// `layout_subgraph` itself
// ============================================================================

#[test]
fn a_subgraph_is_laid_out_as_if_nothing_else_existed() {
    // `c` is fed by `b`, which is fed by `a`. Restricted to `{b, c}` the wire
    // from `a` is not an edge at all, so `b` is a source at depth 0 — the
    // block starts its own drawing rather than inheriting `a`'s column index.
    let fixture = Fixture::new(&format!(
        "a = int {{ value: 1 }}\n{}{}",
        expr("b", "a"),
        expr("c", "b")
    ));
    let (a, b, c) = (fixture.id(&["a"]), fixture.id(&["b"]), fixture.id(&["c"]));

    let subgraph = layout_subgraph(
        &fixture.network,
        &fixture.registry,
        &HashSet::from([b, c]),
        LayoutAlgorithm::Sugiyama,
    );
    assert_eq!(subgraph.len(), 2, "only the requested nodes are placed");
    assert!(!subgraph.contains_key(&a));

    let whole = layout_subgraph(
        &fixture.network,
        &fixture.registry,
        &HashSet::from([a, b, c]),
        LayoutAlgorithm::Sugiyama,
    );
    assert_eq!(
        subgraph[&c].x - subgraph[&b].x,
        whole[&c].x - whole[&b].x,
        "the column pitch is the same; only the origin differs"
    );
    assert_eq!(
        subgraph[&b].x, whole[&a].x,
        "`b` is the subgraph's own leftmost column"
    );
}

#[test]
fn a_node_sized_map_still_measures_as_a_map_inside_a_block() {
    // Sanity check on the size function the block's bounding box is built from:
    // an expanded HOF is far wider than a plain node, and a block that measured
    // it as 160 px would place its neighbours straight through it.
    let fixture = Fixture::new(MAP_WITH_BODY);
    let size = rendered_node_size(node_by_path(&fixture.network, &["m"]), &fixture.registry);
    assert!(size.x > 400.0, "an expanded map is body-width plus gutters");
}

// ============================================================================
// Nothing to do
// ============================================================================

#[test]
fn an_edit_that_adds_nothing_and_grows_nothing_moves_nothing() {
    let mut fixture = Fixture::new(&format!("a = int {{ value: 1 }}\n{}", expr("b", "a")));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["b"], 400.0, 260.0);

    fixture.edit("a = int { value: 987654 }\n");
    let outcome = fixture.run();

    assert!(outcome.moved.is_empty());
    assert_eq!(outcome.rect(&["a"]).position, DVec2::new(100.0, 100.0));
    assert_eq!(outcome.rect(&["b"]).position, DVec2::new(400.0, 260.0));
}
