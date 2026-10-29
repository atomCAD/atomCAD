//! Layout statements: `place`, `arrange`, `straighten`
//! (`doc/design_deliberate_node_placement.md` A2, Phase 5).
//!
//! Every scenario runs through [`Fixture::run`], which applies the whole-pass
//! oracle *and* the statement oracle (`layout_oracle::check_statements`), so a
//! test asserts only what is specific to it: where the subject landed, what
//! was pushed which way, and what the report says.

use glam::DVec2;
use serde_json::Value;

use atomcad_structure_designer::layout::{MoveCause, rendered_node_size};
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{
    LayoutOp, LayoutTargets, Parser, PlaceSpec, PushDir, Statement, edit_network_with_layout,
    serialize_network,
};

use super::layout_oracle::Placed;
use super::layout_test_support::{Fixture, VERTICAL_GAP, empty_network, expr, p};

const GAP: f64 = 50.0;

/// An `expr` with two inputs, `x` and `y`.
fn expr2(name: &str, x: &str, y: &str) -> String {
    format!(
        "{name} = expr {{ x: {x}, y: {y}, expression: \"x + y\", \
         parameters: [{{ name: \"x\", data_type: Int }}, {{ name: \"y\", data_type: Int }}] }}\n"
    )
}

fn bottom(r: Placed) -> f64 {
    r.position.y + r.size.y
}

fn right(r: Placed) -> f64 {
    r.position.x + r.size.x
}

/// A column `w, h, d` at x = 100, `VERTICAL_GAP` apart.
fn column() -> Fixture {
    let mut f = Fixture::new("w = int { value: 1 }\nh = int { value: 2 }\nd = int { value: 3 }\n");
    f.place(&["w"], 100.0, 100.0);
    let y = bottom(f.was(&["w"])) + VERTICAL_GAP;
    f.place(&["h"], 100.0, y);
    let y = bottom(f.was(&["h"])) + VERTICAL_GAP;
    f.place(&["d"], 100.0, y);
    f
}

/// A row `slab -> cut -> fill`, `GAP` apart, tops at 100.
fn chain() -> Fixture {
    let mut f = Fixture::new(&format!(
        "slab = int {{ value: 1 }}\n{}{}",
        expr("cut", "slab"),
        expr("fill", "cut")
    ));
    f.place(&["slab"], 100.0, 100.0);
    let x = right(f.was(&["slab"])) + GAP;
    f.place(&["cut"], x, 100.0);
    let x = right(f.was(&["cut"])) + GAP;
    f.place(&["fill"], x, 100.0);
    f
}

fn cause_of(outcome: &super::layout_test_support::Outcome, path: &[&str]) -> Option<MoveCause> {
    outcome
        .report
        .moved
        .iter()
        .find(|m| m.path == p(path))
        .and_then(|m| m.cause)
}

// ============================================================================
// Each clause
// ============================================================================

#[test]
fn right_of_is_one_gap_right_with_tops_aligned() {
    let mut f = column();
    f.edit("n = int { value: 9 }\nplace n right_of h\n");
    let out = f.run();
    let (h, n) = (out.rect(&["h"]), out.rect(&["n"]));
    assert_eq!(n.position, DVec2::new(right(h) + GAP, h.position.y));
    assert!(out.report.placed.iter().any(|b| b.path == p(&["n"])));
    assert!(out.report.auto_placed.iter().all(|b| b.path != p(&["n"])));
    out.assert_only_moved(&[]);
}

#[test]
fn left_of_is_one_gap_left_with_tops_aligned() {
    let mut f = column();
    f.edit("n = int { value: 9 }\nplace n left_of h\n");
    let out = f.run();
    let (h, n) = (out.rect(&["h"]), out.rect(&["n"]));
    assert_eq!(right(n), h.position.x - GAP);
    assert_eq!(n.position.y, h.position.y);
}

#[test]
fn below_and_above_align_lefts_one_vertical_gap_away() {
    let mut f = column();
    f.edit("n = int { value: 9 }\nm = int { value: 8 }\nplace n below d\nplace m above w\n");
    let out = f.run();
    let (w, d) = (out.rect(&["w"]), out.rect(&["d"]));
    let (n, m) = (out.rect(&["n"]), out.rect(&["m"]));
    assert_eq!(
        n.position,
        DVec2::new(d.position.x, bottom(d) + VERTICAL_GAP)
    );
    assert_eq!(bottom(m), w.position.y - VERTICAL_GAP);
    assert_eq!(m.position.x, w.position.x);
    out.assert_only_moved(&[]);
}

#[test]
fn gap_words_numbers_and_offsets() {
    let mut f = column();
    f.edit(
        "a1 = int { value: 9 }\na2 = int { value: 9 }\na3 = int { value: 9 }\n\
         place a1 right_of w (gap: wide)\n\
         place a2 right_of a1 (gap: 12)\n\
         place a3 right_of a2 (gap: tight, dx: 5, dy: -7)\n",
    );
    let out = f.run();
    let (w, a1, a2, a3) = (
        out.rect(&["w"]),
        out.rect(&["a1"]),
        out.rect(&["a2"]),
        out.rect(&["a3"]),
    );
    assert_eq!(a1.position.x, right(w) + 100.0);
    assert_eq!(a2.position.x, right(a1) + 12.0);
    assert_eq!(
        a3.position,
        DVec2::new(right(a2) + 25.0 + 5.0, a2.position.y - 7.0)
    );
}

#[test]
fn even_with_clauses_set_one_edge() {
    let mut f = column();
    f.edit(
        "n = int { value: 9 }\nm = int { value: 9 }\n\
         place n right_of w, bottom_even_with d\n\
         place m left_even_with n, top_even_with h\n",
    );
    let out = f.run();
    let (d, h, n, m) = (
        out.rect(&["d"]),
        out.rect(&["h"]),
        out.rect(&["n"]),
        out.rect(&["m"]),
    );
    assert_eq!(bottom(n), bottom(d));
    assert_eq!(m.position, DVec2::new(n.position.x, h.position.y));
}

#[test]
fn several_targets_measure_against_their_bounding_box() {
    let mut f = column();
    f.edit("n = int { value: 9 }\nplace n right_of w and d\n");
    let out = f.run();
    let (w, d, n) = (out.rect(&["w"]), out.rect(&["d"]), out.rect(&["n"]));
    assert_eq!(
        n.position,
        DVec2::new(right(w).max(right(d)) + GAP, w.position.y)
    );
}

#[test]
fn straight_from_levels_the_wire_into_the_subject() {
    let mut f = column();
    // `s.y` comes from `h`: its second input pin, one row lower than the first.
    f.edit(&format!(
        "{}place s right_of h, straight_from h\n",
        expr2("s", "w", "h")
    ));
    let out = f.run();
    let (h, s) = (out.rect(&["h"]), out.rect(&["s"]));
    assert_eq!(
        s.position.y,
        h.position.y - 22.0,
        "pin 1 is level with h's output"
    );
}

#[test]
fn straight_to_levels_the_wire_out_of_the_subject() {
    let mut f = column();
    f.edit(&format!(
        "n = int {{ value: 9 }}\n{}place d2 at (700, 300)\nplace n left_of d2, straight_to d2\n",
        expr2("d2", "w", "n")
    ));
    let out = f.run();
    let (d2, n) = (out.rect(&["d2"]), out.rect(&["n"]));
    // n feeds d2's second pin; n's output pin (row 0) must be level with it.
    assert_eq!(n.position.y, d2.position.y + 22.0);
}

// ============================================================================
// push
// ============================================================================

#[test]
fn push_down_inserts_into_a_column() {
    let mut f = column();
    let d_before = f.was(&["d"]);
    f.edit("n = int { value: 9 }\nplace n below h push down\n");
    let out = f.run();
    let (h, n, d) = (out.rect(&["h"]), out.rect(&["n"]), out.rect(&["d"]));
    assert_eq!(
        n.position,
        DVec2::new(h.position.x, bottom(h) + VERTICAL_GAP)
    );
    assert_eq!(
        d.position.y,
        bottom(n) + VERTICAL_GAP,
        "d makes way, just enough"
    );
    assert!(d.position.y > d_before.position.y);
    assert_eq!(cause_of(&out, &["d"]), Some(MoveCause::Push(PushDir::Down)));
    out.assert_only_moved(&[&["d"]]);
}

#[test]
fn push_up_moves_what_it_lands_on_up() {
    let mut f = column();
    f.edit("n = int { value: 9 }\nplace n above h push up\n");
    let out = f.run();
    let (w, n) = (out.rect(&["w"]), out.rect(&["n"]));
    assert_eq!(bottom(w), n.position.y - VERTICAL_GAP);
    assert_eq!(cause_of(&out, &["w"]), Some(MoveCause::Push(PushDir::Up)));
    out.assert_only_moved(&[&["w"]]);
}

#[test]
fn without_push_the_code_splits_around_the_subject() {
    let mut f = column();
    let h = f.was(&["h"]);
    // Squarely on `h`, a touch lower: `h`'s centre is above the subject's.
    f.edit(&format!(
        "n = int {{ value: 9 }}\nplace n at ({}, {})\n",
        h.position.x,
        h.position.y + 5.0
    ));
    let out = f.run();
    assert_eq!(cause_of(&out, &["h"]), Some(MoveCause::Push(PushDir::Up)));
    let moved = &out
        .report
        .moved
        .iter()
        .find(|m| m.path == p(&["h"]))
        .unwrap();
    assert!(moved.why.starts_with("line 2: place n at"), "{}", moved.why);
}

#[test]
fn push_right_inserts_into_a_chain_and_keeps_the_row() {
    let mut f = chain();
    let cut_before = f.was(&["cut"]);
    let fill_before = f.was(&["fill"]);
    f.edit(&format!(
        "{}place rot right_of slab push right\n",
        expr("rot", "slab")
    ));
    let out = f.run();
    let (slab, rot, cut, fill) = (
        out.rect(&["slab"]),
        out.rect(&["rot"]),
        out.rect(&["cut"]),
        out.rect(&["fill"]),
    );
    assert_eq!(rot.position, DVec2::new(right(slab) + GAP, slab.position.y));
    assert_eq!(
        cut.position.x,
        right(rot) + GAP,
        "cut clears rot by exactly the gap"
    );
    assert_eq!(
        cut.position.y, cut_before.position.y,
        "and stays in the row"
    );
    assert_eq!(
        fill.position.x - fill_before.position.x,
        cut.position.x - cut_before.position.x,
        "the half-plane moves rigidly"
    );
    assert_eq!(
        cause_of(&out, &["cut"]),
        Some(MoveCause::Push(PushDir::Right))
    );
    out.assert_only_moved(&[&["cut"], &["fill"]]);
}

#[test]
fn push_right_clears_a_node_the_subject_only_partly_overlaps() {
    let mut f = chain();
    f.edit(&format!(
        "{}place rot right_of slab (dy: 40) push right\n",
        expr("rot", "slab")
    ));
    let out = f.run();
    let (rot, cut) = (out.rect(&["rot"]), out.rect(&["cut"]));
    assert_eq!(cut.position.x, right(rot) + GAP);
    assert_eq!(cut.position.y, 100.0);
}

#[test]
fn an_overlap_the_drawing_already_had_is_not_pushed() {
    let mut f = column();
    let w = f.was(&["w"]);
    // `h` already sits half on `w`: the user's overlap, not the statement's.
    f.place(&["h"], 100.0, w.position.y + 10.0);
    f.edit(&format!(
        "place w at ({}, {})\n",
        w.position.x, w.position.y
    ));
    let out = f.run();
    out.assert_only_moved(&[]);

    // A small `by` that does not newly reach `h` leaves it alone too.
    let mut f = column();
    f.place(&["h"], 100.0, w.position.y + 10.0);
    f.edit("place w by (-5, 0)\n");
    let out = f.run();
    out.assert_only_moved(&[&["w"]]);
}

// ============================================================================
// by
// ============================================================================

#[test]
fn by_moves_one_node_from_where_it_stands() {
    let mut f = column();
    let w = f.was(&["w"]);
    f.edit("place w by (-300, 0)\n");
    let out = f.run();
    assert_eq!(
        out.rect(&["w"]).position,
        w.position + DVec2::new(-300.0, 0.0)
    );
    assert_eq!(cause_of(&out, &["w"]), Some(MoveCause::Place));
    assert!(out.report.placed.iter().any(|b| b.path == p(&["w"])));
}

#[test]
fn by_moves_a_list_rigidly() {
    let mut f = column();
    let (w, d) = (f.was(&["w"]), f.was(&["d"]));
    f.edit("place [w, d] by (400, 10)\n");
    let out = f.run();
    let (w2, d2) = (out.rect(&["w"]), out.rect(&["d"]));
    assert_eq!(w2.position, w.position + DVec2::new(400.0, 10.0));
    assert_eq!(d2.position - w2.position, d.position - w.position);
    out.assert_only_moved(&[&["w"], &["d"]]);
}

#[test]
fn by_is_measured_after_a_growth_moved_the_node() {
    // `g` grows (a second input pin) and pushes `k` down; `by` adds to that.
    let mut f = Fixture::new(&format!(
        "a = int {{ value: 1 }}\n{}k = int {{ value: 2 }}\n",
        expr("g", "a")
    ));
    f.place(&["a"], -200.0, 100.0);
    f.place(&["g"], 100.0, 100.0);
    let y = bottom(f.was(&["g"])) + VERTICAL_GAP;
    f.place(&["k"], 100.0, y);
    f.edit(&format!("{}place k by (0, 40)\n", expr2("g", "a", "a")));
    let out = f.run();
    let (g, k) = (out.rect(&["g"]), out.rect(&["k"]));
    assert_eq!(k.position.y, bottom(g) + VERTICAL_GAP + 40.0);
    let entry = out
        .report
        .moved
        .iter()
        .find(|m| m.path == p(&["k"]))
        .unwrap();
    assert_eq!(entry.cause, Some(MoveCause::Grow));
    assert!(entry.also.contains(&MoveCause::Place));
}

// ============================================================================
// Groups
// ============================================================================

#[test]
fn a_column_group_stacks_in_written_order_and_pushes_the_column_down() {
    let mut f = column();
    f.edit(
        "n = int { value: 9 }\npitch = int { value: 9 }\n\
         place [n, pitch] column below h push down\n",
    );
    let out = f.run();
    let (h, n, pitch, d) = (
        out.rect(&["h"]),
        out.rect(&["n"]),
        out.rect(&["pitch"]),
        out.rect(&["d"]),
    );
    assert_eq!(
        n.position,
        DVec2::new(h.position.x, bottom(h) + VERTICAL_GAP)
    );
    assert_eq!(
        pitch.position,
        DVec2::new(h.position.x, bottom(n) + VERTICAL_GAP)
    );
    assert_eq!(d.position.y, bottom(pitch) + VERTICAL_GAP);
}

#[test]
fn a_row_group_takes_its_gap() {
    let mut f = column();
    f.edit("a = int { value: 9 }\nb = int { value: 9 }\nplace [a, b] row (gap: wide) right_of w\n");
    let out = f.run();
    let (w, a, b) = (out.rect(&["w"]), out.rect(&["a"]), out.rect(&["b"]));
    assert_eq!(a.position, DVec2::new(right(w) + GAP, w.position.y));
    assert_eq!(b.position, DVec2::new(right(a) + 100.0, w.position.y));
}

// ============================================================================
// all, deleted targets, late binding, written order
// ============================================================================

#[test]
fn below_all_is_a_new_lane_under_the_drawing_measured_after_earlier_pushes() {
    let mut f = column();
    f.edit(
        "n = int { value: 9 }\nlane = int { value: 9 }\n\
         place n below h push down\nplace lane below all\n",
    );
    let out = f.run();
    let (w, d, lane) = (out.rect(&["w"]), out.rect(&["d"]), out.rect(&["lane"]));
    assert_eq!(
        lane.position,
        DVec2::new(w.position.x, bottom(d) + VERTICAL_GAP)
    );
}

#[test]
fn all_excludes_an_existing_subject() {
    let mut f = column();
    f.place(&["d"], 400.0, 50.0);
    let w = f.was(&["w"]);
    f.edit("place d below all\n");
    let out = f.run();
    let (h, d) = (out.rect(&["h"]), out.rect(&["d"]));
    assert_eq!(
        d.position,
        DVec2::new(w.position.x, bottom(h) + VERTICAL_GAP),
        "d no longer measures itself"
    );
}

#[test]
fn a_quoted_all_names_a_node() {
    let mut f = Fixture::new("`all` = int { value: 1 }\nz = int { value: 2 }\n");
    f.place(&["all"], 100.0, 100.0);
    f.place(&["z"], 100.0, 600.0);
    f.edit("n = int { value: 9 }\nplace n below `all`\n");
    let out = f.run();
    let all = out.rect(&["all"]);
    assert_eq!(out.rect(&["n"]).position.y, bottom(all) + VERTICAL_GAP);
}

#[test]
fn a_deleted_target_is_measured_at_its_pre_edit_box() {
    let mut f = column();
    f.place(&["d"], 400.0, 500.0);
    let d = f.was(&["d"]);
    f.edit("delete d\nn = int { value: 9 }\nplace n left_even_with d, top_even_with d\n");
    let out = f.run();
    assert_eq!(out.rect(&["n"]).position, d.position);
}

#[test]
fn a_target_pushed_by_an_earlier_statement_is_measured_where_it_ended() {
    let mut f = column();
    f.edit(
        "n = int { value: 9 }\nq = int { value: 9 }\n\
         place n below h push down\nplace q right_of d\n",
    );
    let out = f.run();
    let (d, q) = (out.rect(&["d"]), out.rect(&["q"]));
    assert_eq!(q.position, DVec2::new(right(d) + GAP, d.position.y));
}

#[test]
fn straighten_then_place_measures_the_straightened_node_and_order_matters() {
    let code = |order_swapped: bool| {
        let mut f = chain();
        f.place(&["cut"], 400.0, 260.0);
        f.place(&["fill"], 700.0, 400.0);
        let lines = [
            "straighten [cut, fill]\n",
            "n = int { value: 9 }\nplace n below fill\n",
        ];
        if order_swapped {
            f.check_statements = false; // the straighten moves n's target
            f.edit(&format!("{}{}", lines[1], lines[0]));
        } else {
            f.edit(&format!("{}{}", lines[0], lines[1]));
        }
        let out = f.run();
        (out.rect(&["fill"]), out.rect(&["n"]))
    };
    let (fill, n) = code(false);
    assert_eq!(
        n.position.y,
        bottom(fill) + VERTICAL_GAP,
        "n under the straightened fill"
    );
    let (fill2, n2) = code(true);
    assert_eq!(
        fill2.position, fill.position,
        "fill straightens the same either way"
    );
    assert_ne!(n2.position.y, n.position.y, "but n measured the old fill");
}

// ============================================================================
// Pinning and Step 6
// ============================================================================

#[test]
fn an_automatic_placement_never_moves_a_placed_node() {
    let mut f = chain();
    // `n` is placed exactly where `rot`'s block wants to go; `rot` is left to
    // the automatic rules and must go around it.
    let slab = f.was(&["slab"]);
    f.edit(&format!(
        "n = int {{ value: 9 }}\n{}place n at ({}, {})\n",
        expr("rot", "slab"),
        right(slab) + GAP,
        slab.position.y
    ));
    let out = f.run();
    assert_eq!(
        out.rect(&["n"]).position,
        DVec2::new(right(slab) + GAP, slab.position.y)
    );
}

#[test]
fn a_subject_placed_right_of_its_consumer_shifts_the_consumer_side() {
    let mut f = chain();
    let cut = f.was(&["cut"]);
    f.edit(&format!(
        "n = int {{ value: 9 }}\n{}place n at ({}, 400)\n",
        expr2("cut", "slab", "n"),
        right(cut) + 200.0
    ));
    let out = f.run();
    let (n, cut2) = (out.rect(&["n"]), out.rect(&["cut"]));
    assert!(
        cut2.position.x >= right(n) + GAP,
        "the new wire n -> cut runs forward"
    );
    assert_eq!(cause_of(&out, &["cut"]), Some(MoveCause::Auto));
    assert_eq!(n.position.y, 400.0);
}

// ============================================================================
// Bodies
// ============================================================================

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

#[test]
fn body_statements_are_in_body_coordinates_and_clamp_at_zero() {
    let mut f = Fixture::new(MAP_WITH_BODY);
    f.place(&["r"], -400.0, 100.0);
    f.place(&["m"], 100.0, 100.0);
    f.place(&["m", "d"], 10.0, 10.0);
    f.edit("m/e = int { value: 4 }\nplace m/e at (-30, 200)\n");
    let out = f.run();
    assert_eq!(out.rect(&["m", "e"]).position, DVec2::new(0.0, 200.0));
    assert_eq!(out.report.warnings.len(), 1, "{:?}", out.report.warnings);
    assert!(out.report.warnings[0].fallback.contains("clamped"));
}

#[test]
fn a_body_statement_written_inside_its_block_and_growth_reaches_the_parent() {
    let mut f = Fixture::new(&format!("{MAP_WITH_BODY}\nn = int {{ value: 7 }}\n"));
    f.place(&["r"], -400.0, 100.0);
    f.place(&["m"], 100.0, 100.0);
    f.place(&["m", "d"], 10.0, 10.0);
    let m = f.was(&["m"]);
    f.place(&["n"], 100.0, bottom(m) + VERTICAL_GAP);
    f.edit(
        "m = map {\n  xs: r,\n  input_type: Int,\n  output_type: Int,\n  body {\n    d = int { value: 1 }\n    \
         e = int { value: 2 }\n    place e below d (gap: 400)\n    output d\n  }\n}\n",
    );
    let out = f.run();
    let (d, e) = (out.rect(&["m", "d"]), out.rect(&["m", "e"]));
    assert_eq!(e.position, DVec2::new(d.position.x, bottom(d) + 400.0));
    let n = out.rect(&["n"]);
    assert!(
        n.position.y >= bottom(out.rect(&["m"])) + VERTICAL_GAP,
        "the grown map pushed n"
    );
}

// ============================================================================
// Rejections (D8)
// ============================================================================

/// Run one edit and return its single warning's problem, asserting the
/// subject fell back as the design says.
fn rejected(code: &str) -> (super::layout_test_support::Outcome, String) {
    let mut f = column();
    f.edit(code);
    let out = f.run();
    assert_eq!(out.report.warnings.len(), 1, "{:?}", out.report.warnings);
    let problem = out.report.warnings[0].problem.clone();
    (out, problem)
}

#[test]
fn an_unknown_target_drops_the_statement_and_the_node_is_placed_automatically() {
    let (out, problem) = rejected("n = int { value: 9 }\nplace n below zz\n");
    assert!(problem.contains("no node named `zz`"), "{problem}");
    assert!(out.report.auto_placed.iter().any(|b| b.path == p(&["n"])));
    assert_eq!(out.report.warnings[0].fallback, "placed automatically");
    assert_eq!(out.report.warnings[0].line, 2);
}

#[test]
fn a_target_in_another_scope_is_rejected() {
    let mut f = Fixture::new(MAP_WITH_BODY);
    f.edit("place m/d below r\n");
    let out = f.run();
    assert_eq!(out.report.warnings.len(), 1);
    assert!(out.report.warnings[0].problem.contains("another scope"));
    assert_eq!(out.report.warnings[0].fallback, "left where it was");
}

#[test]
fn an_axis_stated_twice_is_rejected() {
    let (_, problem) = rejected(&format!(
        "{}place s below w, straight_from w\n",
        expr("s", "w")
    ));
    assert!(problem.contains("y is stated twice"), "{problem}");
}

#[test]
fn an_axis_left_open_is_rejected() {
    let (_, problem) = rejected("n = int { value: 9 }\nplace n left_even_with w\n");
    assert!(problem.contains("on y"), "{problem}");
}

#[test]
fn a_node_placed_by_two_statements_keeps_the_first() {
    let mut f = column();
    f.edit(
        "n = int { value: 9 }\nm = int { value: 9 }\n\
         place n right_of w\nplace [n, m] column right_of d\n",
    );
    let out = f.run();
    assert_eq!(out.report.warnings.len(), 1);
    assert!(out.report.warnings[0].problem.contains("earlier statement"));
    let (w, n) = (out.rect(&["w"]), out.rect(&["n"]));
    assert_eq!(n.position, DVec2::new(right(w) + GAP, w.position.y));
    assert!(out.report.auto_placed.iter().any(|b| b.path == p(&["m"])));
    assert_eq!(
        out.report.warnings[0].fallback,
        "new nodes placed automatically; nodes an earlier statement placed keep that placement"
    );
}

#[test]
fn a_new_target_no_earlier_statement_placed_is_rejected() {
    let (_, problem) =
        rejected("a2 = int { value: 9 }\nb2 = int { value: 9 }\nplace a2 right_of b2\n");
    assert!(problem.contains("no position yet"), "{problem}");
}

#[test]
fn by_on_a_new_node_is_rejected() {
    let (out, problem) = rejected("n = int { value: 9 }\nplace n by (0, 40)\n");
    assert!(problem.contains("`by`"), "{problem}");
    assert!(out.report.auto_placed.iter().any(|b| b.path == p(&["n"])));
}

#[test]
fn a_deleted_node_cannot_be_a_subject() {
    let (_, problem) = rejected("delete d\nplace d by (0, 40)\n");
    assert!(problem.contains("deleted by this edit"), "{problem}");
}

#[test]
fn straight_from_without_a_wire_is_rejected() {
    let (_, problem) = rejected("n = int { value: 9 }\nplace n right_of w, straight_from h\n");
    assert!(problem.contains("no wire from `h`"), "{problem}");
}

#[test]
fn a_statement_in_no_body_is_rejected() {
    let (_, problem) = rejected("place nope/x by (0, 10)\n");
    assert!(problem.contains("names no body"), "{problem}");
}

#[test]
fn a_grammar_error_rejects_the_whole_edit() {
    let registry = atomcad_structure_designer::node_type_registry::NodeTypeRegistry::new();
    let mut network = empty_network();
    for code in [
        "n = int { value: 1 }\nplace n below n push left\n",
        "n = int { value: 1 }\nplace n at (1, 2), below n\n",
        "n = int { value: 1 }\nplace n below n push down push up\n",
        "n = int { value: 1 }\nplace [n] below n\n",
        "n = int { value: 1 }\nplace n beside n\n",
    ] {
        let (result, statements) = edit_network_with_layout(&mut network, &registry, code, false);
        assert!(!result.success, "{code:?} should not parse");
        assert!(statements.is_empty());
        assert!(network.nodes.is_empty(), "nothing applied for {code:?}");
    }
}

// ============================================================================
// The parser
// ============================================================================

#[test]
fn a_node_named_like_a_statement_head_is_still_assignable() {
    let statements =
        Parser::parse("place = int { value: 1 }\narrange/x = int { value: 1 }\n").expect("parses");
    assert!(matches!(statements[0], Statement::Assignment { .. }));
    assert!(matches!(statements[1], Statement::Assignment { .. }));
}

#[test]
fn the_parser_keeps_line_text_and_quoting() {
    let statements = Parser::parse(
        "a = int { value: 1 }\n\nplace a below `all` push down  # into the column\nplace a below all\n",
    )
    .expect("parses");
    let Statement::Layout(first) = &statements[1] else {
        panic!("a layout statement");
    };
    assert_eq!(first.line, 3);
    assert_eq!(first.text, "place a below `all` push down");
    assert_eq!(first.push, Some(PushDir::Down));
    let LayoutOp::Place {
        spec: PlaceSpec::Clauses(clauses),
        ..
    } = &first.op
    else {
        panic!("a place");
    };
    let atomcad_structure_designer::text_format::LayoutClause::Direction { targets, .. } =
        &clauses[0]
    else {
        panic!("a direction clause");
    };
    assert!(
        matches!(targets, LayoutTargets::Nodes(_)),
        "`all` quoted is a node"
    );
    let Statement::Layout(second) = &statements[2] else {
        panic!("a layout statement");
    };
    let LayoutOp::Place {
        spec: PlaceSpec::Clauses(clauses),
        ..
    } = &second.op
    else {
        panic!("a place");
    };
    assert!(matches!(
        clauses[0],
        atomcad_structure_designer::text_format::LayoutClause::Direction {
            targets: LayoutTargets::All,
            ..
        }
    ));
}

// ============================================================================
// End to end: ai_text_edit, --replace, the report JSON, D1
// ============================================================================

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

fn position(sd: &StructureDesigner, name: &str) -> DVec2 {
    sd.node_type_registry.node_networks["main"]
        .nodes
        .values()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}`"))
        .position
}

fn set_position(sd: &mut StructureDesigner, name: &str, x: f64, y: f64) {
    let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    let node = network
        .nodes
        .values_mut()
        .find(|node| node.custom_name.as_deref() == Some(name))
        .unwrap();
    node.position = DVec2::new(x, y);
}

#[test]
fn the_report_json_carries_placed_and_push_directions() {
    let mut sd = designer();
    sd.ai_text_edit("w = int { value: 1 }\nh = int { value: 2 }\n", false);
    set_position(&mut sd, "w", 100.0, 100.0);
    set_position(&mut sd, "h", 100.0, 200.0);
    let outcome = sd.ai_text_edit("n = int { value: 3 }\nplace n below w push down\n", false);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    let json: Value = serde_json::from_str(&outcome.to_json()).unwrap();
    let layout = &json["layout"];
    assert_eq!(layout["placed"][0]["node"], "n");
    assert_eq!(layout["moved"][0]["node"], "h");
    assert_eq!(layout["moved"][0]["cause"], "push");
    assert_eq!(layout["moved"][0]["dir"], "down");
    assert_eq!(
        layout["moved"][0]["why"],
        "line 2: place n below w push down"
    );
    assert!(layout.get("auto_placed").is_none());
    assert_eq!(layout["warnings"], Value::Array(vec![]));
}

#[test]
fn statements_alone_still_report_layout() {
    let mut sd = designer();
    sd.ai_text_edit("w = int { value: 1 }\n", false);
    set_position(&mut sd, "w", 100.0, 100.0);
    let outcome = sd.ai_text_edit("place w by (0, 40)\n", false);
    assert_eq!(position(&sd, "w"), DVec2::new(100.0, 140.0));
    let json: Value = serde_json::from_str(&outcome.to_json()).unwrap();
    assert_eq!(json["layout"]["moved"][0]["cause"], "place");
}

#[test]
fn an_edit_of_statements_alone_is_one_undo_step_and_dirties() {
    let mut sd = designer();
    sd.ai_text_edit("w = int { value: 1 }\n", false);
    set_position(&mut sd, "w", 100.0, 100.0);
    sd.set_dirty(false);
    sd.ai_text_edit("place w by (0, 40)\n", false);
    assert!(sd.is_dirty(), "moving a node is a change");
    assert_eq!(position(&sd, "w"), DVec2::new(100.0, 140.0));
    assert!(sd.undo());
    assert_eq!(position(&sd, "w"), DVec2::new(100.0, 100.0));
}

#[test]
fn replace_mode_with_statements_and_nothing_is_stored() {
    let mut sd = designer();
    let script = "a = int { value: 1 }\nb = int { value: 2 }\n";
    sd.ai_text_edit(script, false);
    set_position(&mut sd, "a", 100.0, 100.0);
    set_position(&mut sd, "b", 100.0, 300.0);
    let text_before = serialize_network(
        &sd.node_type_registry.node_networks["main"],
        &sd.node_type_registry,
        None,
    );

    let outcome = sd.ai_text_edit(&format!("{script}place a by (0, 40)\n"), true);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    assert_eq!(position(&sd, "a"), DVec2::new(100.0, 140.0));
    assert_eq!(position(&sd, "b"), DVec2::new(100.0, 300.0));
    let text_after = serialize_network(
        &sd.node_type_registry.node_networks["main"],
        &sd.node_type_registry,
        None,
    );
    assert_eq!(
        text_before, text_after,
        "a statement leaves nothing in the text (D1)"
    );
    assert!(!text_after.contains("place"));
}

#[test]
fn a_dry_run_with_statements_equals_the_real_edit() {
    let mut sd = designer();
    sd.ai_text_edit("w = int { value: 1 }\nh = int { value: 2 }\n", false);
    set_position(&mut sd, "w", 100.0, 100.0);
    set_position(&mut sd, "h", 100.0, 200.0);
    let code = "n = int { value: 3 }\nplace n below w push down\n";
    let dry = sd.ai_text_edit_with_options(code, false, true);
    assert_eq!(
        position(&sd, "h"),
        DVec2::new(100.0, 200.0),
        "the preview changed nothing"
    );
    let real = sd.ai_text_edit(code, false);
    let mut dry_json: Value = serde_json::from_str(&dry.to_json()).unwrap();
    dry_json.as_object_mut().unwrap().remove("dry_run");
    let real_json: Value = serde_json::from_str(&real.to_json()).unwrap();
    assert_eq!(dry_json, real_json);
}

#[test]
fn the_read_out_and_at_share_one_coordinate_system() {
    // `place n at (x, y)` with the read-out's own numbers moves nothing.
    let mut sd = designer();
    sd.ai_text_edit(
        &format!("a = int {{ value: 1 }}\n{}", expr("b", "a")),
        false,
    );
    set_position(&mut sd, "a", 100.4, 99.6);
    set_position(&mut sd, "b", 330.0, 100.0);
    let readout = sd.query_active_layout("", "a", "");
    let line = readout
        .lines()
        .find(|l| l.starts_with("a "))
        .expect("a's line");
    let xy: Vec<f64> = line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split(',')
        .map(|v| v.parse().unwrap())
        .collect();
    let b_before = position(&sd, "b");
    sd.ai_text_edit(&format!("place a at ({}, {})\n", xy[0], xy[1]), false);
    let a = position(&sd, "a");
    assert!((a.x - 100.4).abs() <= 0.5 && (a.y - 99.6).abs() <= 0.5);
    assert_eq!(position(&sd, "b"), b_before);
}

#[test]
fn rendered_size_is_what_placed_reports() {
    let mut sd = designer();
    sd.ai_text_edit("w = int { value: 1 }\n", false);
    let outcome = sd.ai_text_edit("n = int { value: 3 }\nplace n right_of w\n", false);
    let report = outcome.layout.expect("a report");
    let node = sd.node_type_registry.node_networks["main"]
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("n"))
        .unwrap();
    assert_eq!(
        report.placed[0].rect.size,
        rendered_node_size(node, &sd.node_type_registry)
    );
}
