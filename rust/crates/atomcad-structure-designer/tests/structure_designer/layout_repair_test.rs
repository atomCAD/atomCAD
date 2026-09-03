//! Steps 6, 7 and 8 of the incremental layout pass, and the two `hand_moved`
//! tiebreakers (`doc/design_incremental_layout.md` Phase 4).
//!
//! Step 6 repairs the backward wires *this edit* created, Step 7 places the
//! comments it created, Step 8 pulls a comment that drifted from its anchor
//! back to its exact original offset. The `hand_moved` flag never blocks any of
//! that — it only breaks ties, so every test here that sets it has a control
//! case with the flag off, and the two differ only in which node the pass chose
//! to disturb.
//!
//! As everywhere in this family, each test states only what is specific to it
//! and gets the eight whole-pass invariants from the shared
//! [oracle](super::layout_oracle) — including "the pass never creates an
//! overlap", which is invariant 1 and is therefore asserted by every test
//! below rather than by one of its own.

use glam::DVec2;

use atomcad_structure_designer::node_layout::DEFAULT_HORIZONTAL_GAP as GAP;
use atomcad_structure_designer::text_format::{edit_network, serialize_network};

use super::layout_test_support::{Fixture, VERTICAL_GAP, center, expr};

/// The gap `layout::common::ANCHOR_GAP` keeps between a comment and the box it
/// is anchored to.
const ANCHOR_GAP: f64 = 24.0;

/// A comment statement, sized and optionally anchored.
fn comment(name: &str, size: DVec2, on: Option<&str>) -> String {
    let anchor = match on {
        Some(target) => format!(", on: {target}"),
        None => String::new(),
    };
    format!(
        "{name} = Comment {{ label: \"\", text: \"note\", width: {}, height: {}{anchor} }}\n",
        size.x, size.y
    )
}

// ============================================================================
// Step 6 — the backward wires this edit created
// ============================================================================

/// The case a plain half-plane shift can never fix: the new wire's destination
/// sits *left* of its source, so any line left of the destination would carry
/// the source along too. Holding the source's upstream closure fixed is what
/// lets everything at or right of the line move over it instead.
#[test]
fn a_rewire_whose_destination_sits_left_of_its_source_moves_the_destination_past_it() {
    let mut fixture = Fixture::new(&format!(
        "s = int {{ value: 1 }}\nt = int {{ value: 2 }}\nd = int {{ value: 3 }}\n{}{}",
        expr("a", "s"),
        expr("c", "t")
    ));
    fixture.place(&["t"], -500.0, 100.0);
    fixture.place(&["s"], -200.0, 300.0);
    fixture.place(&["a"], 600.0, 100.0);
    fixture.place(&["c"], 100.0, 100.0);
    fixture.place(&["d"], 1000.0, 400.0);

    // `c` now reads `a`, which sits 500 px to its right.
    fixture.edit(&expr("c", "a"));
    let outcome = fixture.run();

    let (a, c) = (outcome.rect(&["a"]), outcome.rect(&["c"]));
    assert_eq!(
        c.position.x,
        a.position.x + a.size.x + GAP,
        "the destination ended up exactly one gap right of its new source"
    );
    assert_eq!(
        outcome.rect(&["a"]).position,
        DVec2::new(600.0, 100.0),
        "the source stays: it is `fixed`, and so is everything upstream of it"
    );
    assert_eq!(outcome.rect(&["s"]).position, DVec2::new(-200.0, 300.0));
    assert_eq!(
        outcome.shift(&["d"]),
        outcome.shift(&["c"]),
        "a shift is rigid: what lay right of the line moved by the same vector"
    );
    assert_eq!(
        outcome.rect(&["t"]).position,
        DVec2::new(-500.0, 100.0),
        "and what lay left of it did not move at all"
    );
}

/// Source and destination a few pixels apart in x — one of the loose columns
/// half of both hand-drawn corpora sit in. The repair opens the column up to
/// exactly one gap; it does not re-tidy anything.
#[test]
fn a_rewire_within_one_loose_column_widens_the_gap() {
    let mut fixture = Fixture::new(&format!(
        "s = int {{ value: 1 }}\nt = int {{ value: 2 }}\n{}{}",
        expr("a", "s"),
        expr("c", "t")
    ));
    fixture.place(&["s"], -300.0, 100.0);
    fixture.place(&["t"], -500.0, 300.0);
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["c"], 104.0, 300.0);

    fixture.edit(&expr("c", "a"));
    let outcome = fixture.run();

    let (a, c) = (outcome.rect(&["a"]), outcome.rect(&["c"]));
    assert_eq!(
        c.position.x - (a.position.x + a.size.x),
        GAP,
        "the two column-mates are now exactly one gap apart"
    );
    assert_eq!(
        c.position.y, 300.0,
        "and nothing about the repair is vertical"
    );
    assert_eq!(a.position, DVec2::new(100.0, 100.0));
}

/// A wire that was already backward before the edit is not in `added_wires`,
/// and the pre-edit drawing is the baseline however irregular.
#[test]
fn an_already_backward_wire_is_left_alone() {
    let mut fixture = Fixture::new(&format!(
        "s = int {{ value: 1 }}\n{}{}",
        expr("a", "s"),
        expr("c", "a")
    ));
    fixture.place(&["s"], -500.0, 100.0);
    fixture.place(&["a"], 600.0, 100.0);
    fixture.place(&["c"], 100.0, 100.0);

    // Something unrelated, so the pass has work to do in this scope.
    fixture.edit("z = int { value: 9 }\n");
    let outcome = fixture.run();

    assert_eq!(outcome.rect(&["a"]).position, DVec2::new(600.0, 100.0));
    assert_eq!(outcome.rect(&["c"]).position, DVec2::new(100.0, 100.0));
    outcome.assert_only_moved(&[&["z"]]);
}

/// A `$element` reference has its two ends in different coordinate frames, so
/// "the source's right edge is left of the destination" is not a statement
/// about it. Step 6 skips every such wire.
#[test]
fn a_cross_scope_wire_is_never_repaired() {
    let two_param_expr = |a: &str| {
        format!(
            "m/d = expr {{ a: {a}, b: 2, expression: \"a + b\", parameters: \
             [{{ name: \"a\", data_type: Int }}, {{ name: \"b\", data_type: Int }}] }}\n"
        )
    };
    let mut fixture = Fixture::new(&format!(
        r#"
r = range {{ start: 0, step: 1, count: 3 }}
m = map {{
  xs: r,
  input_type: Int,
  output_type: Int,
  body {{
{}    output d
  }}
}}
"#,
        two_param_expr("1").replace("m/d", "    d")
    ));
    fixture.place(&["r"], -400.0, 100.0);
    fixture.place(&["m"], 100.0, 100.0);
    // Hard against the body's left edge, where the synthesized `$element`
    // anchor sits: a same-scope wire in this position would be repaired.
    fixture.place(&["m", "d"], 0.0, 10.0);

    fixture.edit(&two_param_expr("$element"));
    let outcome = fixture.run();

    assert_eq!(
        outcome.rect(&["m", "d"]).position,
        DVec2::new(0.0, 10.0),
        "a cross-scope wire is an anchor, never something to repair"
    );
    assert!(outcome.moved.is_empty());
}

/// Removing a wire cannot create an overlap or a backward wire, and a node left
/// with no wires stays put — so the delta carries the removal faithfully and
/// the layout does nothing with it.
#[test]
fn removing_a_wire_moves_nothing() {
    let mut fixture = Fixture::new(&format!("s = int {{ value: 1 }}\n{}", expr("c", "s")));
    fixture.place(&["s"], 100.0, 100.0);
    fixture.place(&["c"], 400.0, 100.0);

    // A literal on a wired pin: the wire goes, the pin layout does not change.
    fixture.edit(
        "c = expr { x: 7, expression: \"x + 1\", \
         parameters: [{ name: \"x\", data_type: Int }] }\n",
    );
    let outcome = fixture.run();

    assert!(outcome.moved.is_empty());
    assert_eq!(outcome.rect(&["c"]).position, DVec2::new(400.0, 100.0));
}

/// A `--replace` rebuilds the network from scratch with fresh node ids. Because
/// the diff is keyed by name path, none of the rebuilt wires reads as *added* —
/// an id-keyed diff would report every wire in the network and hand the whole
/// drawing to Step 6.
#[test]
fn a_replace_of_an_unchanged_script_moves_nothing() {
    let mut fixture = Fixture::new(&format!(
        "s = int {{ value: 1 }}\n{}{}",
        expr("a", "s"),
        expr("c", "a")
    ));
    fixture.place(&["s"], 100.0, 100.0);
    fixture.place(&["a"], 400.0, 260.0);
    fixture.place(&["c"], 700.0, 40.0);

    let text = serialize_network(&fixture.network, &fixture.registry, None);
    let result = edit_network(&mut fixture.network, &fixture.registry, &text, true);
    assert!(result.success, "replace failed: {:?}", result.errors);

    let outcome = fixture.run();

    assert!(outcome.moved.is_empty(), "a round-trip is not an edit");
    assert_eq!(outcome.rect(&["s"]).position, DVec2::new(100.0, 100.0));
    assert_eq!(outcome.rect(&["a"]).position, DVec2::new(400.0, 260.0));
    assert_eq!(outcome.rect(&["c"]).position, DVec2::new(700.0, 40.0));
}

// ============================================================================
// Step 7 — the comments this edit created
// ============================================================================

#[test]
fn a_new_anchored_comment_lands_beside_its_anchor() {
    let mut fixture = Fixture::new("a = int { value: 1 }\n");
    fixture.place(&["a"], 100.0, 100.0);

    fixture.edit(&comment("note", DVec2::new(200.0, 100.0), Some("a")));
    let outcome = fixture.run();

    let (a, note) = (outcome.rect(&["a"]), outcome.rect(&["note"]));
    assert_eq!(
        note.position.y + note.size.y + ANCHOR_GAP,
        a.position.y,
        "the first of the four sides is `above`, at the anchor gap"
    );
    assert_eq!(
        center(note).x,
        center(a).x,
        "and horizontally centred on the anchor"
    );
    outcome.assert_only_moved(&[&["note"]]);
}

#[test]
fn a_new_unanchored_comment_goes_below_the_drawing() {
    let mut fixture = Fixture::new(&format!("a = int {{ value: 1 }}\n{}", expr("b", "a")));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["b"], 400.0, 260.0);

    fixture.edit(&comment("note", DVec2::new(200.0, 100.0), None));
    let outcome = fixture.run();

    let (a, b, note) = (
        outcome.rect(&["a"]),
        outcome.rect(&["b"]),
        outcome.rect(&["note"]),
    );
    let left = a.position.x.min(b.position.x);
    let bottom = (a.position.y + a.size.y).max(b.position.y + b.size.y);
    assert_eq!(
        note.position,
        DVec2::new(left, bottom + VERTICAL_GAP),
        "an unanchored comment is an anchorless block: below the drawing, \
         flush with its left edge"
    );
    outcome.assert_only_moved(&[&["note"]]);
}

// ============================================================================
// Step 8 — a comment that drifted from its anchor
// ============================================================================

/// The motivating case: a comment sitting just left of a shift line whose
/// anchor was carried right by that shift. Nothing overlaps at the restored
/// spot, so the leader line goes back to exactly the length the human drew it.
#[test]
fn a_comment_left_behind_by_a_shift_is_pulled_back_to_its_exact_offset() {
    let mut fixture = Fixture::new(&format!(
        "s = int {{ value: 1 }}\n{}{}",
        expr("k", "s"),
        comment("note", DVec2::new(200.0, 100.0), Some("k"))
    ));
    fixture.place(&["s"], 0.0, 400.0);
    fixture.place(&["k"], 300.0, 400.0);
    fixture.place(&["note"], 50.0, 800.0);
    let offset = fixture.was(&["note"]).position - fixture.was(&["k"]).position;

    // Insert `b` between `s` and `k`: there is no room, so `k` is shifted right
    // — and `note`, left of the line, is not.
    fixture.edit(&format!("{}{}", expr("b", "s"), expr("k", "b")));
    let outcome = fixture.run();

    let (k, note) = (outcome.rect(&["k"]), outcome.rect(&["note"]));
    assert_eq!(
        outcome.shift(&["k"]),
        Some(DVec2::new(120.0, 0.0)),
        "the consumer moved right by exactly the room the block needed"
    );
    assert_eq!(
        note.position - k.position,
        offset,
        "and the comment followed it, to its exact original offset"
    );
}

/// The same drift, with the restored spot no longer free: a comment pushed
/// aside by a block cannot go back, because the block is now standing where it
/// used to be. All-or-nothing — it stays where the cascade left it rather than
/// moving part of the way.
#[test]
fn a_comment_whose_original_spot_is_taken_stays_where_the_cascade_left_it() {
    let mut fixture = Fixture::new(&format!(
        "a = int {{ value: 1 }}\n{}",
        comment("note", DVec2::new(200.0, 300.0), Some("a"))
    ));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["note"], 310.0, 60.0);

    fixture.edit(&expr("b", "a"));
    let outcome = fixture.run();

    let (note, b) = (outcome.rect(&["note"]), outcome.rect(&["b"]));
    assert_eq!(
        note.position,
        DVec2::new(310.0, b.position.y + b.size.y + VERTICAL_GAP),
        "pushed by the minimum that clears the block, and left there: the \
         restored position is occupied by that very block"
    );
}

/// A comment the pass moved *towards* its anchor is not touched. Step 8 only
/// ever undoes drift, never manufactures it, so the distance test is one-sided.
#[test]
fn a_comment_that_drifted_closer_is_left_alone() {
    let mut fixture = Fixture::new(&format!(
        "a = int {{ value: 1 }}\nu = int {{ value: 2 }}\n{}",
        comment("note", DVec2::new(400.0, 300.0), Some("u"))
    ));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["u"], 310.0, -600.0);
    fixture.place(&["note"], 310.0, -50.0);

    fixture.edit(&expr("b", "a"));
    let outcome = fixture.run();

    let (note, b) = (outcome.rect(&["note"]), outcome.rect(&["b"]));
    assert_eq!(
        note.position,
        DVec2::new(310.0, b.position.y - VERTICAL_GAP - note.size.y),
        "the cascade pushed it up, towards its anchor, and Step 8 left it"
    );
    assert_eq!(
        outcome.rect(&["u"]).position,
        DVec2::new(310.0, -600.0),
        "the anchor itself was never in the way"
    );
}

/// An anchor the edit removed resolves to nothing, and a comment with nothing
/// to measure a drift against is skipped rather than guessed at.
#[test]
fn a_comment_with_an_unresolvable_anchor_is_skipped() {
    let mut fixture = Fixture::new(&format!(
        "a = int {{ value: 1 }}\nk = int {{ value: 2 }}\n{}",
        comment("note", DVec2::new(200.0, 100.0), Some("k"))
    ));
    fixture.place(&["a"], 100.0, 100.0);
    fixture.place(&["k"], 400.0, 100.0);
    fixture.place(&["note"], 400.0, 400.0);

    fixture.edit("delete k\n");
    let outcome = fixture.run();

    assert!(outcome.moved.is_empty());
    assert_eq!(outcome.rect(&["note"]).position, DVec2::new(400.0, 400.0));
}

// ============================================================================
// `hand_moved` — a tiebreaker, in the two places the pass has a choice
// ============================================================================

/// Step 5b's cascade: `Split` sends the obstacle up, into a node a human
/// placed; `Down` sends it the other way and reaches nobody. Both clear the
/// block, so the flag decides.
#[test]
fn the_cascade_prefers_the_direction_that_spares_a_hand_placed_node() {
    let outcome = cascade_direction_fixture(true);

    let (big, wall_before) = (outcome.rect(&["big"]), DVec2::new(310.0, 100.0));
    assert!(
        big.position.y > wall_before.y,
        "the obstacle went down, away from the hand-placed node: {big:?}"
    );
    assert_eq!(
        outcome.shift(&["h"]),
        None,
        "and the hand-placed node was not displaced at all"
    );
}

/// The control: with nothing hand-placed, `Split` stands — it is Step 5b's
/// specified direction and the one that moves the drawing least.
#[test]
fn without_the_flag_the_cascade_splits_around_the_block_as_specified() {
    let outcome = cascade_direction_fixture(false);

    assert!(
        outcome.rect(&["big"]).position.y < 100.0,
        "the obstacle's centre is above the block's, so `Split` sends it up"
    );
    assert!(
        outcome.shift(&["h"]).is_some(),
        "…and the node above it is pushed along, flag or no flag"
    );
}

/// `big` (a 160x400 note) covers the whole slide window at the block's target,
/// so Step 5b runs; `h` sits above `big` and is the node the upward cascade
/// would reach.
fn cascade_direction_fixture(flag: bool) -> super::layout_test_support::Outcome {
    let mut fixture = Fixture::new(&format!(
        "a = int {{ value: 1 }}\nh = int {{ value: 2 }}\n{}",
        comment("big", DVec2::new(160.0, 400.0), None)
    ));
    fixture.place(&["a"], 100.0, 300.0);
    fixture.place(&["big"], 310.0, 100.0);
    fixture.place(&["h"], 310.0, -50.0);
    if flag {
        fixture.hand_moved(&["h"]);
    }

    fixture.edit(&expr("b", "a"));
    fixture.run()
}

/// Step 5a's window: the only thing in the band is hand-placed, so the pass
/// looks twice as far for free space before it resorts to pushing.
#[test]
fn the_slide_window_is_extended_when_the_band_is_all_hand_placed() {
    let outcome = slide_window_fixture(true);

    let (wall, b) = (outcome.rect(&["wall"]), outcome.rect(&["b"]));
    assert_eq!(
        outcome.shift(&["wall"]),
        None,
        "sliding moves nothing, which is the whole point of looking further"
    );
    assert!(
        b.position.y >= wall.position.y + wall.size.y + VERTICAL_GAP,
        "the block found free space below the wall: {b:?} vs {wall:?}"
    );
}

/// The control: the same wall, not hand-placed, is pushed out of the way.
#[test]
fn without_the_flag_the_ordinary_window_gives_up_and_the_block_pushes() {
    let outcome = slide_window_fixture(false);

    assert!(
        outcome.shift(&["wall"]).is_some(),
        "the ordinary window found nothing, so Step 5b moved the drawing"
    );
}

/// `wall` blocks every `y` inside `SLIDE_WINDOW` of the block's target, and
/// leaves free space just outside it.
fn slide_window_fixture(flag: bool) -> super::layout_test_support::Outcome {
    let mut fixture = Fixture::new(&format!(
        "a = int {{ value: 1 }}\n{}",
        comment("wall", DVec2::new(160.0, 300.0), None)
    ));
    fixture.place(&["a"], 100.0, 300.0);
    fixture.place(&["wall"], 310.0, 130.0);
    if flag {
        fixture.hand_moved(&["wall"]);
    }

    fixture.edit(&expr("b", "a"));
    fixture.run()
}
