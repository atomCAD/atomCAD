//! Phase 2 of `doc/design_incremental_layout.md`: the two motion primitives
//! and the growth operation built on them.
//!
//! These tests build a bare `NodeNetwork` and hand the primitives an explicit
//! `sizes` map rather than measuring through a registry. That is not a
//! shortcut — the primitives take a measured map by design (a network lives
//! *inside* the registry, so `&mut NodeNetwork` and `&NodeTypeRegistry` cannot
//! coexist), and writing sizes out makes each scenario's geometry readable
//! instead of derived from a node type's pin count.
//!
//! `layout_size_test.rs` is where the measurement itself is pinned; nothing
//! here re-tests it.

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use atomcad_structure_designer::layout::motion::{
    COLUMN_TOLERANCE, CascadeDir, Rect, cascade, grow_rect, measure_scope, shift_half_plane,
    snap_shift_line, upstream_closure,
};
use atomcad_structure_designer::node_data::NoData;
use atomcad_structure_designer::node_network::NodeNetwork;

use super::layout_oracle::{Placed, check_cascade_bound, check_pushed_order, check_rigid_shift};
use super::layout_test_support::empty_network;

/// The vertical clearance the cascade keeps (`layout::common::VERTICAL_GAP`).
const VGAP: f64 = 30.0;

// ---------------------------------------------------------------------------
// Scaffolding
// ---------------------------------------------------------------------------

/// A network under test plus the `sizes` map the primitives read it through.
struct Scene {
    network: NodeNetwork,
    sizes: HashMap<u64, DVec2>,
}

impl Scene {
    fn new() -> Self {
        Self {
            network: NodeNetwork::new_empty(),
            sizes: HashMap::new(),
        }
    }

    /// Add a node at `pos` with an explicit rendered footprint.
    fn node(&mut self, pos: DVec2, size: DVec2) -> u64 {
        let id = self.network.add_node("union", pos, 2, Box::new(NoData {}));
        self.sizes.insert(id, size);
        id
    }

    /// A 160x83 node — the ordinary case — at `(x, y)`.
    fn plain(&mut self, x: f64, y: f64) -> u64 {
        self.node(DVec2::new(x, y), DVec2::new(160.0, 83.0))
    }

    fn wire(&mut self, source: u64, dest: u64, pin: usize) {
        self.network.connect_nodes(source, 0, dest, pin, false);
    }

    fn pos(&self, id: u64) -> DVec2 {
        self.network.nodes[&id].position
    }

    fn rect(&self, id: u64) -> Rect {
        Rect::new(self.pos(id), self.sizes[&id])
    }

    fn positions(&self) -> HashMap<u64, DVec2> {
        self.network
            .nodes
            .iter()
            .map(|(&id, n)| (id, n.position))
            .collect()
    }

    /// The scene as the oracle's primitive-level invariants read it.
    fn placed(&self) -> HashMap<u64, Placed> {
        self.sizes
            .keys()
            .map(|&id| {
                (
                    id,
                    Placed {
                        position: self.pos(id),
                        size: self.sizes[&id],
                        // Plain boxes: this harness builds no zone-owning node.
                        renders_body: false,
                    },
                )
            })
            .collect()
    }
}

fn ids(list: &[u64]) -> HashSet<u64> {
    list.iter().copied().collect()
}

/// Every pair of nodes whose boxes overlap, as an unordered set.
fn overlapping_pairs(scene: &Scene) -> HashSet<(u64, u64)> {
    let mut all: Vec<u64> = scene.sizes.keys().copied().collect();
    all.sort_unstable();
    let mut out = HashSet::new();
    for (i, &a) in all.iter().enumerate() {
        for &b in &all[i + 1..] {
            if scene.rect(a).overlaps(&scene.rect(b), 0.0) {
                out.insert((a, b));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// shift_half_plane
// ---------------------------------------------------------------------------

#[test]
fn shift_moves_the_half_plane_rigidly_and_leaves_the_rest() {
    let mut scene = Scene::new();
    let left = scene.plain(0.0, 0.0);
    let on_line = scene.plain(500.0, 40.0);
    let right = scene.plain(700.0, 300.0);

    let before = scene.positions();
    let placed_before = scene.placed();
    let outcome = shift_half_plane(
        &mut scene.network,
        &scene.sizes,
        500.0,
        90.0,
        &HashSet::new(),
    );

    // "x >= T" is inclusive: a node whose left edge sits exactly on the line
    // moves with the half-plane.
    assert_eq!(ids(&outcome.shifted), ids(&[on_line, right]));
    assert!(outcome.pushed.is_empty());
    assert_eq!(scene.pos(left), before[&left]);
    assert_eq!(scene.pos(on_line), before[&on_line] + DVec2::new(90.0, 0.0));
    assert_eq!(scene.pos(right), before[&right] + DVec2::new(90.0, 0.0));

    // Oracle invariant 4: the translation was rigid.
    check_rigid_shift(
        &placed_before,
        &scene.placed(),
        &ids(&outcome.shifted),
        &ids(&[left]),
    );
}

#[test]
fn a_forward_wire_crossing_the_line_stays_forward() {
    let mut scene = Scene::new();
    let source = scene.plain(0.0, 0.0);
    let dest = scene.plain(400.0, 0.0);
    scene.wire(source, dest, 0);

    // The line falls between them, so only the destination moves — the wire
    // gets longer, never shorter.
    shift_half_plane(
        &mut scene.network,
        &scene.sizes,
        200.0,
        250.0,
        &HashSet::new(),
    );

    assert_eq!(scene.pos(source), DVec2::new(0.0, 0.0));
    assert!(scene.rect(source).right() < scene.pos(dest).x);
}

#[test]
fn a_fixed_node_right_of_the_line_stays_and_its_intruder_is_pushed_off() {
    let mut scene = Scene::new();
    // `input` feeds `consumer`, but sits to its RIGHT — the backward-wire shape
    // the fixed set exists for. Shifting the half-plane at the consumer moves
    // the consumer onto the input, which the cascade then clears vertically.
    let input = scene.plain(300.0, 0.0);
    let consumer = scene.plain(100.0, 0.0);
    scene.wire(input, consumer, 0);

    let fixed = upstream_closure(&scene.network, [input]);
    let outcome = shift_half_plane(&mut scene.network, &scene.sizes, 100.0, 250.0, &fixed);

    // The input did not move; the consumer did, and was then pushed off it.
    assert_eq!(scene.pos(input), DVec2::new(300.0, 0.0));
    assert_eq!(ids(&outcome.shifted), ids(&[consumer]));
    assert_eq!(ids(&outcome.pushed), ids(&[consumer]));
    assert_eq!(scene.pos(consumer).x, 350.0);
    assert!(!scene.rect(consumer).overlaps(&scene.rect(input), 0.0));
    // Whether the *wire* also ends up forward is a matter of how big `dx` is,
    // which is Step 6's business (Phase 4); the primitive's contract is only
    // that the fixed node stayed and nothing landed on it.
}

#[test]
fn an_overlap_that_predates_the_shift_is_not_repaired() {
    let mut scene = Scene::new();
    let fixed_node = scene.plain(300.0, 0.0);
    // Already sitting on `fixed_node`, and left of the line so it does not move.
    let squatter = scene.plain(320.0, 10.0);

    let before = scene.positions();
    let fixed = ids(&[fixed_node]);
    shift_half_plane(&mut scene.network, &scene.sizes, 320.0, 100.0, &fixed);

    // `squatter` is at the threshold, so it is shifted — but the cascade at
    // `fixed_node` must not then push it, because the overlap is inherited.
    assert_eq!(scene.pos(fixed_node), before[&fixed_node]);
    assert_eq!(scene.pos(squatter).y, before[&squatter].y);
}

// ---------------------------------------------------------------------------
// The window rule
// ---------------------------------------------------------------------------

#[test]
fn a_loose_column_straddling_the_default_moves_as_a_whole() {
    let mut scene = Scene::new();
    // A hand-drawn column: three nodes within a few pixels of x = 500, which a
    // line dropped at exactly 500 would split.
    let a = scene.plain(497.0, 0.0);
    let b = scene.plain(500.0, 200.0);
    let c = scene.plain(503.0, 400.0);

    let threshold = snap_shift_line(&scene.network, &scene.sizes, 500.0, 500.0, &HashSet::new());
    assert!(
        threshold <= 497.0 - COLUMN_TOLERANCE,
        "the line should snap left of the whole column, got {threshold}"
    );

    let outcome = shift_half_plane(
        &mut scene.network,
        &scene.sizes,
        threshold,
        100.0,
        &HashSet::new(),
    );
    assert_eq!(ids(&outcome.shifted), ids(&[a, b, c]));
}

#[test]
fn the_snap_may_enter_a_fixed_nodes_own_box() {
    let mut scene = Scene::new();
    // The grown node spans the entire window: its box covers the default line,
    // so a snap that respected fixed boxes could never move at all.
    let grown = scene.node(DVec2::new(0.0, 0.0), DVec2::new(400.0, 300.0));
    // A loose column at the default (the grown node's old right edge, 400).
    let column = scene.plain(402.0, 0.0);

    let fixed = ids(&[grown]);
    let threshold = snap_shift_line(&scene.network, &scene.sizes, 400.0, 400.0, &fixed);

    assert!(
        threshold < 400.0,
        "the snap should have moved left into the grown node's box, got {threshold}"
    );
    // …and the grown node itself is unaffected: it is fixed, so it stays put
    // whichever side of the line it lands on.
    let outcome = shift_half_plane(&mut scene.network, &scene.sizes, threshold, 50.0, &fixed);
    assert_eq!(scene.pos(grown), DVec2::new(0.0, 0.0));
    assert!(outcome.shifted.contains(&column));
}

#[test]
fn the_default_is_kept_when_the_nearest_gap_is_over_a_node_width_away() {
    let mut scene = Scene::new();
    // A wall of nodes to the left of the default, each within `COLUMN_TOLERANCE`
    // of the next, so there is no legal line within one node width.
    for i in 0..40 {
        scene.plain(500.0 - i as f64 * 5.0, i as f64 * 120.0);
    }

    let threshold = snap_shift_line(&scene.network, &scene.sizes, 500.0, 500.0, &HashSet::new());
    assert_eq!(threshold, 500.0);
}

#[test]
fn the_default_is_clamped_to_the_windows_right_end() {
    let mut scene = Scene::new();
    scene.plain(0.0, 0.0);

    // `x_min` can lie right of the leftmost must-move node when a consumer's
    // left edge overlaps an input horizontally; clamping is what makes that
    // consumer move.
    let threshold = snap_shift_line(&scene.network, &scene.sizes, 900.0, 400.0, &HashSet::new());
    assert!(
        threshold <= 400.0,
        "expected a clamp to 400, got {threshold}"
    );
}

// ---------------------------------------------------------------------------
// The vertical cascade
// ---------------------------------------------------------------------------

#[test]
fn the_cascade_pushes_only_colliding_nodes_by_the_minimum() {
    let mut scene = Scene::new();
    let hit = scene.plain(0.0, 100.0);
    let clear_below = scene.plain(0.0, 600.0);
    let clear_beside = scene.plain(400.0, 100.0);

    let r = Rect::new(DVec2::ZERO, DVec2::new(200.0, 150.0));
    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Down,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(ids(&moved), ids(&[hit]));
    // Exactly clearing the rect plus the gap, not a pixel more.
    assert_eq!(scene.pos(hit), DVec2::new(0.0, 150.0 + VGAP));
    assert_eq!(scene.pos(clear_below), DVec2::new(0.0, 600.0));
    assert_eq!(scene.pos(clear_beside), DVec2::new(400.0, 100.0));
}

#[test]
fn the_cascade_propagates_through_a_dense_column_and_terminates() {
    let mut scene = Scene::new();
    let stack: Vec<u64> = (0..6)
        .map(|i| scene.plain(0.0, 100.0 + i as f64 * (83.0 + VGAP)))
        .collect();

    let placed_before = scene.placed();
    let r = Rect::new(DVec2::ZERO, DVec2::new(200.0, 150.0));
    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Down,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(
        ids(&moved),
        ids(&stack),
        "the whole column should shift down"
    );
    // Oracle invariant 5: the vertical order among the pushed is the pre-edit
    // order. Nothing overlaps afterwards either.
    check_pushed_order(&placed_before, &scene.placed(), &ids(&moved));
    assert!(overlapping_pairs(&scene).is_empty());
    check_cascade_bound(&moved);
}

/// Oracle invariant 8 on the worst case this design has a number for: a column
/// packed so tightly that every member is pushed.
///
/// The corpus simulation measured a real-world maximum of 11 pushed nodes
/// (`scripts/layout_cascade_sim.py`, over both hand-drawn files). This drops a
/// rect on a 20-deep column touching *every* one of them, which is the shape
/// that would break the guard first if the propagation rule ever stopped being
/// restricted to newly created overlaps.
#[test]
fn even_a_twenty_deep_column_stays_within_the_cascade_guard() {
    let mut scene = Scene::new();
    for i in 0..20 {
        scene.plain(0.0, 100.0 + i as f64 * (83.0 + VGAP));
    }

    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        Rect::new(DVec2::ZERO, DVec2::new(200.0, 150.0)),
        CascadeDir::Down,
        &HashSet::new(),
        &HashSet::new(),
    );

    check_cascade_bound(&moved);
    assert!(overlapping_pairs(&scene).is_empty());
}

#[test]
fn the_cascade_reaches_a_node_whose_x_overlaps_a_pushed_node_but_not_r() {
    let mut scene = Scene::new();
    // `first` is hit by R; `second` sits to the right of R entirely, but shares
    // an x-interval with `first`. A band-limited cascade would miss it.
    let first = scene.plain(0.0, 100.0);
    let second = scene.plain(150.0, 250.0);

    let r = Rect::new(DVec2::ZERO, DVec2::new(100.0, 150.0));
    assert!(
        scene.rect(second).left() > r.right(),
        "the second node must be outside R's x-band for this test to mean anything"
    );

    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Down,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(ids(&moved), ids(&[first, second]));
    assert!(overlapping_pairs(&scene).is_empty());
}

/// The enqueue test is "newly overlapped", not "further along `dir`". A centre
/// test would miss a tall node whose box the pusher has entered but whose
/// centre is still behind the pusher's, and leave that overlap standing.
#[test]
fn a_node_pushed_into_a_tall_comment_pushes_it_too_whatever_the_centres_say() {
    let mut scene = Scene::new();
    // Wide enough to overlap both R and the note. The note sits outside R's
    // x-band, so it cannot join the first round and must arrive by propagation.
    let pusher = scene.node(DVec2::new(150.0, 60.0), DVec2::new(200.0, 83.0));
    let note = scene.node(DVec2::new(300.0, 200.0), DVec2::new(400.0, 300.0));
    assert!(
        !scene.rect(pusher).overlaps(&scene.rect(note), 0.0),
        "the two must start clear of each other"
    );
    let note_centre_before = scene.rect(note).center().y;

    // A tall R, so the pusher is driven deep into the note — past its centre.
    let r = Rect::new(DVec2::ZERO, DVec2::new(200.0, 370.0));
    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Down,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert!(
        scene.rect(pusher).center().y > note_centre_before,
        "the pusher must have overshot the note's centre for the test to bite"
    );
    assert!(moved.contains(&pusher));
    assert!(moved.contains(&note), "the note must be pushed off too");
    assert!(!scene.rect(pusher).overlaps(&scene.rect(note), 0.0));
}

#[test]
fn a_pushed_node_that_reaches_a_fixed_node_clears_it() {
    let mut scene = Scene::new();
    let hit = scene.plain(0.0, 100.0);
    // Directly in the path the push takes, and immovable.
    let wall = scene.plain(0.0, 220.0);

    let r = Rect::new(DVec2::ZERO, DVec2::new(200.0, 150.0));
    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Down,
        &ids(&[wall]),
        &HashSet::new(),
    );

    assert_eq!(ids(&moved), ids(&[hit]));
    assert_eq!(scene.pos(wall), DVec2::new(0.0, 220.0));
    assert!(!scene.rect(hit).overlaps(&scene.rect(wall), 0.0));
    assert!(scene.pos(hit).y >= scene.rect(wall).bottom());
}

#[test]
fn the_up_set_and_the_down_set_never_touch() {
    let mut scene = Scene::new();
    let r = Rect::new(DVec2::new(0.0, 200.0), DVec2::new(200.0, 200.0));
    // Two nodes straddling R's centre (y = 300).
    let above = scene.plain(0.0, 180.0);
    let below = scene.plain(0.0, 350.0);

    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Split,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(ids(&moved), ids(&[above, below]));
    assert!(scene.rect(above).bottom() <= r.top());
    assert!(scene.pos(below).y >= r.bottom());
    assert!(!scene.rect(above).overlaps(&scene.rect(below), 0.0));
}

#[test]
fn a_pre_existing_overlap_is_neither_repaired_nor_an_obstacle() {
    let mut scene = Scene::new();
    let r = Rect::new(DVec2::ZERO, DVec2::new(200.0, 150.0));
    // `squatter` already sits on R and is declared inherited.
    let squatter = scene.plain(0.0, 60.0);
    // `hit` is pushed down and lands where the squatter is — and must not be
    // deflected by it, because that overlap is not this call's to repair.
    let hit = scene.plain(20.0, 120.0);

    let moved = cascade(
        &mut scene.network,
        &scene.sizes,
        r,
        CascadeDir::Down,
        &HashSet::new(),
        &ids(&[squatter]),
    );

    assert_eq!(ids(&moved), ids(&[hit]));
    assert_eq!(scene.pos(squatter), DVec2::new(0.0, 60.0));
    assert_eq!(scene.pos(hit), DVec2::new(20.0, 150.0 + VGAP));
}

// ---------------------------------------------------------------------------
// grow_rect
// ---------------------------------------------------------------------------

#[test]
fn width_only_growth_shifts_the_half_plane_and_moves_nothing_below() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(400.0, 300.0));
    let right = scene.plain(600.0, 40.0);
    let below = scene.plain(0.0, 400.0);
    let left = scene.plain(-400.0, 0.0);
    scene.sizes.insert(grown, DVec2::new(500.0, 300.0)); // the post-growth size

    let before = scene.positions();
    let moves = grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 300.0),
        DVec2::new(500.0, 300.0),
    );

    assert_eq!(
        moves.iter().map(|m| m.0).collect::<HashSet<_>>(),
        ids(&[right])
    );
    assert_eq!(scene.pos(grown), DVec2::ZERO);
    assert_eq!(scene.pos(right), before[&right] + DVec2::new(100.0, 0.0));
    assert_eq!(scene.pos(below), before[&below]);
    assert_eq!(scene.pos(left), before[&left]);
}

#[test]
fn height_only_growth_pushes_only_colliding_nodes() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(400.0, 300.0));
    // Directly below, at exactly the cascade's clearance: it must move by the
    // growth and no more.
    let under = scene.plain(0.0, 300.0 + VGAP);
    // Lower-RIGHT, the region the retired quadrant shift moved on both axes and
    // this one must not touch at all.
    let lower_right = scene.plain(900.0, 900.0);
    scene.sizes.insert(grown, DVec2::new(400.0, 420.0));

    let before = scene.positions();
    let moves = grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 300.0),
        DVec2::new(400.0, 420.0),
    );

    assert_eq!(
        moves.iter().map(|m| m.0).collect::<HashSet<_>>(),
        ids(&[under])
    );
    assert_eq!(scene.pos(under), before[&under] + DVec2::new(0.0, 120.0));
    assert_eq!(scene.pos(lower_right), before[&lower_right]);
}

/// The retired quadrant shift classified every node in the lower-right quadrant
/// by which side of the instance's own diagonal it fell on and moved it — even
/// when nothing was going to collide with it, and even when the two halves of
/// that split could be driven into each other. `grow_rect` moves a node only
/// for a reason: rightward because the width grew past it, downward because the
/// taller rect actually reaches it.
#[test]
fn growth_does_not_drag_non_colliding_neighbours_the_way_the_quadrant_shift_did() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(700.0, 500.0));
    // Below the old rect *and* clear of the new one: the quadrant rule shifted
    // it down for being under the diagonal. Nothing reaches it here.
    let below_left = scene.plain(100.0, 560.0);
    // Past both far edges: the quadrant rule moved it on both axes. Only the
    // width delta reaches it.
    let lower_right = scene.plain(450.0, 560.0);

    let inherited = overlapping_pairs(&scene);
    grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 400.0),
        DVec2::new(700.0, 500.0),
    );

    assert_eq!(scene.pos(below_left), DVec2::new(100.0, 560.0));
    assert_eq!(scene.pos(lower_right), DVec2::new(750.0, 560.0));
    assert!(
        overlapping_pairs(&scene).is_subset(&inherited),
        "grow_rect created an overlap: {:?} vs {:?}",
        overlapping_pairs(&scene),
        inherited
    );
}

#[test]
fn a_node_that_already_overlapped_the_old_rect_is_left_alone_vertically() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(400.0, 300.0));
    // Sitting on the old rect already — inherited, so the cascade skips it.
    let squatter = scene.plain(20.0, 250.0);
    scene.sizes.insert(grown, DVec2::new(400.0, 400.0));

    grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 300.0),
        DVec2::new(400.0, 400.0),
    );

    assert_eq!(scene.pos(squatter), DVec2::new(20.0, 250.0));
}

#[test]
fn an_input_right_of_the_line_stays_and_its_intruder_is_pushed_off() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(700.0, 300.0));
    // An input of the grown node sitting to its right — it is in the upstream
    // closure, so the shift must leave it exactly where it is.
    let input = scene.plain(700.0, 0.0);
    scene.wire(input, grown, 0);
    // …and an ordinary neighbour, clear of the input to start with, that the
    // shift will carry straight onto it.
    let neighbour = scene.plain(420.0, 0.0);

    grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 300.0),
        DVec2::new(700.0, 300.0),
    );

    assert_eq!(
        scene.pos(input),
        DVec2::new(700.0, 0.0),
        "an input must not move"
    );
    assert_eq!(scene.pos(neighbour).x, 720.0);
    assert!(!scene.rect(neighbour).overlaps(&scene.rect(input), 0.0));
}

#[test]
fn growth_on_both_axes_composes_the_two_primitives() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(400.0, 300.0));
    let right = scene.plain(600.0, 0.0);
    let under = scene.plain(0.0, 300.0 + VGAP);
    scene.sizes.insert(grown, DVec2::new(500.0, 400.0));

    let moves = grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 300.0),
        DVec2::new(500.0, 400.0),
    );

    assert_eq!(
        moves.iter().map(|m| m.0).collect::<HashSet<_>>(),
        ids(&[right, under])
    );
    assert_eq!(scene.pos(right), DVec2::new(700.0, 0.0));
    assert_eq!(scene.pos(under), DVec2::new(0.0, 400.0 + VGAP));
    assert!(overlapping_pairs(&scene).is_empty());
}

#[test]
fn a_shrink_moves_nothing() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(200.0, 200.0));
    scene.plain(300.0, 0.0);
    scene.plain(0.0, 300.0);

    let before = scene.positions();
    let moves = grow_rect(
        &mut scene.network,
        &scene.sizes,
        grown,
        DVec2::new(400.0, 400.0),
        DVec2::new(200.0, 200.0),
    );

    assert!(moves.is_empty());
    assert_eq!(scene.positions(), before);
}

// ---------------------------------------------------------------------------
// measure_scope
// ---------------------------------------------------------------------------

#[test]
fn measure_scope_covers_every_node_in_the_scope() {
    use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;

    let mut network = empty_network();
    let registry = NodeTypeRegistry::new();
    let a = network.add_node("union", DVec2::ZERO, 2, Box::new(NoData {}));
    let b = network.add_node("sphere", DVec2::new(300.0, 0.0), 1, Box::new(NoData {}));

    let sizes = measure_scope(&network, &registry);
    assert_eq!(sizes.len(), 2);
    assert!(sizes[&a].x > 0.0 && sizes[&a].y > 0.0);
    assert!(sizes[&b].x > 0.0 && sizes[&b].y > 0.0);
}
