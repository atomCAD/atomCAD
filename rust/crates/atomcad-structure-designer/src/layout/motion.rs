//! The two motion primitives of the incremental layout pass.
//!
//! `doc/design_incremental_layout.md` D6 — exactly two, one per axis:
//!
//! - [`shift_half_plane`], **horizontal, rigid, global.** Everything at or
//!   right of a threshold moves right by the same amount, except a `fixed` set
//!   (the site's must-not-move nodes plus their upstream closure). Preserving
//!   x-order is what makes it wire-safe: a forward wire can never turn
//!   backward.
//! - [`cascade`], **vertical, minimal, local.** Only nodes that a move actually
//!   collided with move, by the least that clears the collision, and the push
//!   propagates only through collisions the cascade itself created.
//!
//! [`grow_rect`] is the one operation every "this node got bigger" path uses
//! (D11): the width delta is a half-plane shift, the height delta a downward
//! cascade. It replaces the landed quadrant shift
//! (`node_inlining::make_space_for_inline`), which moved non-colliding nodes
//! vertically and could itself create an overlap.
//!
//! # Sizes are measured once
//!
//! Every function here takes `sizes`, a `node id -> rendered footprint` map
//! produced by [`measure_scope`], rather than a `&NodeTypeRegistry`. Two
//! reasons, and the second is not negotiable:
//!
//! 1. Motion only changes positions, and a node's footprint depends on its
//!    pins and on its body's *contents* — never on where it sits — so one
//!    measurement is valid for a whole pass over one scope.
//! 2. A network *lives inside* the registry (`NodeTypeRegistry::node_networks`),
//!    so `&mut NodeNetwork` and `&NodeTypeRegistry` cannot be held at once.
//!    Measuring up front is what lets the GUI reflow path call these at all.
//!
//! The map's **keys are the placed set**: the nodes that are visible to the
//! primitives, count as obstacles, and may be moved. From Phase 3 a freshly
//! added node is deliberately absent until its block is placed.

use std::collections::{HashMap, HashSet, VecDeque};

use glam::DVec2;

use crate::layout::common::VERTICAL_GAP;
use crate::layout::size::rendered_node_size;
use crate::node_layout::{DEFAULT_HORIZONTAL_GAP, NODE_WIDTH, nodes_overlap};
use crate::node_network::{NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;

/// How close two left edges have to be to read as one hand-drawn column.
///
/// Half the nodes in both measured corpora sit within 8 px of another node's
/// x (design doc, "What the hand-drawn corpora look like"). A shift line
/// dropped inside such a cluster would move some of its members and not
/// others, destroying alignment the human put there, so the window rule snaps
/// away from one.
pub const COLUMN_TOLERANCE: f64 = 8.0;

/// How far left of its default the window rule will look for whitespace.
///
/// One node width: far enough to clear the column the default landed in,
/// near enough that the line stays at the site it was computed for.
pub const SNAP_SEARCH_DISTANCE: f64 = NODE_WIDTH;

/// Hard stop on one [`cascade`]'s queue, so a pathological input cannot hang
/// the editor.
///
/// The design proves termination (each node is pushed only by nodes originally
/// entirely on its far side, a strict order), and Phase 4 asserts a far tighter
/// bound in tests. This is only a backstop against degenerate geometry — a
/// zero-height node, a NaN position — reaching the loop.
const MAX_CASCADE_POPS: usize = 10_000;

/// An axis-aligned box on the canvas: top-left corner plus size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub pos: DVec2,
    pub size: DVec2,
}

impl Rect {
    pub fn new(pos: DVec2, size: DVec2) -> Self {
        Self { pos, size }
    }

    pub fn left(&self) -> f64 {
        self.pos.x
    }

    pub fn right(&self) -> f64 {
        self.pos.x + self.size.x
    }

    pub fn top(&self) -> f64 {
        self.pos.y
    }

    pub fn bottom(&self) -> f64 {
        self.pos.y + self.size.y
    }

    pub fn center(&self) -> DVec2 {
        self.pos + self.size * 0.5
    }

    /// Whether the two boxes come within `gap` of each other on both axes —
    /// the same test the rest of the layout code uses
    /// ([`node_layout::nodes_overlap`]), so "overlap" means one thing
    /// everywhere.
    pub fn overlaps(&self, other: &Rect, gap: f64) -> bool {
        nodes_overlap(self.pos, self.size, other.pos, other.size, gap)
    }

    /// Whether the two boxes' x-intervals come within `gap` of each other.
    pub fn overlaps_x(&self, other: &Rect, gap: f64) -> bool {
        self.left() - gap / 2.0 < other.right() + gap / 2.0
            && self.right() + gap / 2.0 > other.left() - gap / 2.0
    }

    /// Whether the two boxes' y-intervals come within `gap` of each other.
    pub fn overlaps_y(&self, other: &Rect, gap: f64) -> bool {
        self.top() - gap / 2.0 < other.bottom() + gap / 2.0
            && self.bottom() + gap / 2.0 > other.top() - gap / 2.0
    }
}

/// Which way a [`cascade`] pushes the nodes it displaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeDir {
    /// Everything moves down. Used by [`grow_rect`]: a rect that only grew
    /// downward can only have created collisions below its old bottom edge.
    Down,
    /// Everything moves up.
    Up,
    /// Each first-round node goes away from `R`'s centre, by whichever side of
    /// it the node lies on. Splits the obstacles into an up-set and a down-set
    /// that provably never meet.
    Split,
}

impl CascadeDir {
    /// `+1` for down, `-1` for up. `Split` is resolved per node before this is
    /// ever called.
    fn sign(self) -> f64 {
        match self {
            CascadeDir::Down | CascadeDir::Split => 1.0,
            CascadeDir::Up => -1.0,
        }
    }
}

/// What a [`shift_half_plane`] call moved.
///
/// The two lists are disjoint and each is ascending by id. Kept apart because
/// the layout oracle asserts different things about them: the `shifted` set is
/// a rigid translation (pairwise offsets unchanged), the `pushed` set is not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShiftOutcome {
    /// Translated right by `dx`, offsets among them unchanged.
    pub shifted: Vec<u64>,
    /// Pushed vertically off a `fixed` node the translation landed on.
    pub pushed: Vec<u64>,
}

/// The rendered footprint of every node in `network`, measured once.
///
/// The placed set the motion primitives work over; see the module docs for why
/// this is a map rather than a registry borrow.
pub fn measure_scope(network: &NodeNetwork, registry: &NodeTypeRegistry) -> HashMap<u64, DVec2> {
    network
        .nodes
        .iter()
        .map(|(&id, node)| (id, rendered_node_size(node, registry)))
        .collect()
}

/// The box `node_id` occupies, or `None` if it is not placed.
fn rect_of(network: &NodeNetwork, sizes: &HashMap<u64, DVec2>, node_id: u64) -> Option<Rect> {
    let size = *sizes.get(&node_id)?;
    let position = network.nodes.get(&node_id)?.position;
    Some(Rect::new(position, size))
}

/// The placed ids, ascending — the deterministic iteration order everything
/// here uses (D7).
fn placed_ids(sizes: &HashMap<u64, DVec2>) -> Vec<u64> {
    let mut ids: Vec<u64> = sizes.keys().copied().collect();
    ids.sort_unstable();
    ids
}

/// Every node reachable by following wires *backwards* from `seeds`, the seeds
/// included.
///
/// This is what makes [`shift_half_plane`] wire-safe: with the whole upstream
/// closure of the must-not-move nodes held fixed, a wire from a moved node into
/// an unmoved one cannot exist, so no forward wire can turn backward.
///
/// Same-scope wires only — depth `0`, ordinary output pins. A capture or a
/// zone input crosses into another coordinate frame, where "upstream" says
/// nothing about x.
pub fn upstream_closure(
    network: &NodeNetwork,
    seeds: impl IntoIterator<Item = u64>,
) -> HashSet<u64> {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut stack: Vec<u64> = Vec::new();
    for seed in seeds {
        if seen.insert(seed) {
            stack.push(seed);
        }
    }
    while let Some(id) = stack.pop() {
        let Some(node) = network.nodes.get(&id) else {
            continue;
        };
        for argument in &node.arguments {
            for wire in &argument.incoming_wires {
                if wire.source_scope_depth != 0
                    || !matches!(wire.source_pin, SourcePin::NodeOutput { .. })
                {
                    continue;
                }
                if seen.insert(wire.source_node_id) {
                    stack.push(wire.source_node_id);
                }
            }
        }
    }
    seen
}

// ---------------------------------------------------------------------------
// The window rule
// ---------------------------------------------------------------------------

/// Place a half-plane shift line by the window rule (D15).
///
/// A shift is safe at any threshold but not equally good at any: dropped at an
/// arbitrary x it cuts through a loose column. The line therefore starts at the
/// site's `default`, clamped to `right_end` (never right of the leftmost
/// must-move node), and snaps **left, never right** — left keeps every required
/// node moving, right would not — to the nearest x where
///
/// - no placed node's left edge is within [`COLUMN_TOLERANCE`], and
/// - no placed **non-`fixed`** node's box straddles the line.
///
/// A `fixed` node still counts for the tolerance test (a line at its left edge
/// would move its column-mates and not it) but its box may be cut: it stays
/// whichever side of the line it is on. Without that exclusion the snap could
/// never enter the grown node's own box — which spans the entire `grow_rect`
/// window — and the default would always be kept.
///
/// The search gives up after [`SNAP_SEARCH_DISTANCE`] and returns the clamped
/// default; there is no left bound and no empty case, because whatever `fixed`
/// holds stays put wherever the line falls.
pub fn snap_shift_line(
    network: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    default: f64,
    right_end: f64,
    fixed: &HashSet<u64>,
) -> f64 {
    let clamped = default.min(right_end);
    let limit = clamped - SNAP_SEARCH_DISTANCE;

    // Every forbidden position is an open interval, so its own endpoints are
    // legal answers: jumping to the left endpoint of whichever interval
    // currently contains the line lands on the nearest legal x at or left of
    // it. Each jump strictly decreases the candidate and there are finitely
    // many endpoints, so this terminates.
    let ids = placed_ids(sizes);
    let mut candidate = clamped;
    'search: loop {
        if candidate < limit {
            return clamped;
        }
        for id in &ids {
            let Some(rect) = rect_of(network, sizes, *id) else {
                continue;
            };
            // (a) too close to a left edge — a loose column.
            if (rect.left() - candidate).abs() < COLUMN_TOLERANCE {
                candidate = rect.left() - COLUMN_TOLERANCE;
                continue 'search;
            }
            // (b) cutting a movable node in half.
            if !fixed.contains(id) && rect.left() < candidate && candidate < rect.right() {
                candidate = rect.left();
                continue 'search;
            }
        }
        return candidate;
    }
}

// ---------------------------------------------------------------------------
// shift_half_plane
// ---------------------------------------------------------------------------

/// Translate every placed node with `x >= threshold` right by `dx`, except the
/// nodes in `fixed`.
///
/// `fixed` is the site's must-not-move nodes together with their
/// [`upstream_closure`]. That closure is the whole correctness argument: a wire
/// from an unmoved node into a moved one only gets longer, a wire from a moved
/// node into a fixed one cannot exist (its source would be upstream of a fixed
/// node and therefore fixed itself), and wires within either set are unchanged.
/// So **a forward wire never turns backward**, and alignment and gaps are
/// preserved exactly within the moved set and within the unmoved set.
///
/// The exclusion has one cost: a moved node can land on a fixed node sitting at
/// or right of `threshold`. Each such fixed node is then handed to [`cascade`]
/// as `R`, which pushes the intruders off vertically, ignoring whatever already
/// overlapped it before the shift. That case is rare (an input whose consumer
/// sat left of it), so it is usually a no-op.
///
/// A non-positive `dx` moves nothing.
pub fn shift_half_plane(
    network: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    threshold: f64,
    dx: f64,
    fixed: &HashSet<u64>,
) -> ShiftOutcome {
    let mut outcome = ShiftOutcome::default();
    if !dx.is_finite() || dx <= 0.0 {
        return outcome;
    }

    let ids = placed_ids(sizes);

    // The fixed nodes the translation can land on, and what already overlapped
    // each of them — captured before anything moves, since "already" is a
    // statement about the pre-shift drawing.
    let mut landing_sites: Vec<(u64, Rect, HashSet<u64>)> = Vec::new();
    for &id in &ids {
        if !fixed.contains(&id) {
            continue;
        }
        let Some(rect) = rect_of(network, sizes, id) else {
            continue;
        };
        if rect.left() < threshold {
            continue;
        }
        let ignore: HashSet<u64> = ids
            .iter()
            .copied()
            .filter(|&other| other != id)
            .filter(|&other| {
                rect_of(network, sizes, other).is_some_and(|r| r.overlaps(&rect, VERTICAL_GAP))
            })
            .collect();
        landing_sites.push((id, rect, ignore));
    }

    for &id in &ids {
        if fixed.contains(&id) {
            continue;
        }
        let Some(node) = network.nodes.get_mut(&id) else {
            continue;
        };
        if node.position.x < threshold {
            continue;
        }
        node.position.x += dx;
        outcome.shifted.push(id);
    }

    // Clear whatever the translation landed on a fixed node.
    for (id, rect, ignore) in landing_sites {
        let mut hold = fixed.clone();
        hold.insert(id);
        let pushed = cascade(network, sizes, rect, CascadeDir::Split, &hold, &ignore);
        outcome.pushed.extend(pushed);
    }
    outcome.pushed.sort_unstable();
    outcome.pushed.dedup();

    outcome
}

// ---------------------------------------------------------------------------
// The vertical cascade
// ---------------------------------------------------------------------------

/// Clear `r` of placed nodes by pushing them vertically, minimally, and
/// propagate only through the collisions this call created.
///
/// A vertical Force-Scan restricted to **new** overlaps. The first round takes
/// every placed node overlapping `r` that is neither `fixed` nor in `ignore`,
/// ascending id, and moves it the least that clears `r`. Each node it *newly*
/// overlaps by moving is then pushed the same way, and so on.
///
/// - A **pre-existing** overlap is never repaired, whether it is listed in
///   `ignore` or merely holds between two nodes that already overlapped: a
///   node that already sat on `r` is neither enqueued nor an obstacle to those
///   that are.
/// - The enqueue test is "newly overlapped", not "further along `dir`". A node
///   that `n`'s move newly overlaps lies entirely on the `dir` side of `n`'s
///   previous rect, so pushing it the same way keeps the pair in its original
///   order whatever their centres say. A centre test would miss a taller node
///   — a comment, an expanded HOF — whose box `n` has entered but whose centre
///   is still behind `n`'s, and leave that overlap standing.
/// - Every placed node is tested, not a band around `r`: a pushed node can land
///   on a node whose x-interval overlaps *its* but not `r`'s.
///
/// The clearance is [`VERTICAL_GAP`] and it is applied uniformly — to the
/// detection *and* to the move. The design writes this as "`R` inflated by
/// `GAP`" at the call sites plus a `+ GAP` on each move; folding it into one
/// parameter is the same geometry without the double-count that reading both
/// literally would produce, and it keeps callers passing the node's plain rect.
///
/// Returns the ids moved, ascending. A node whose minimum clearing move is zero
/// is not reported.
pub fn cascade(
    network: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    r: Rect,
    dir: CascadeDir,
    fixed: &HashSet<u64>,
    ignore: &HashSet<u64>,
) -> Vec<u64> {
    // Vertical, so the layout module's vertical gap — not the design's `GAP`,
    // which names the *horizontal* clearance `shift_half_plane` works in.
    let gap = VERTICAL_GAP;
    let ids = placed_ids(sizes);

    // (id, direction, blocker rect) — the blocker travels with the entry so a
    // node pushed by two different nodes clears each of them in turn.
    let mut queue: VecDeque<(u64, CascadeDir, Rect)> = VecDeque::new();
    let mut direction: HashMap<u64, CascadeDir> = HashMap::new();

    for &id in &ids {
        if fixed.contains(&id) || ignore.contains(&id) {
            continue;
        }
        let Some(rect) = rect_of(network, sizes, id) else {
            continue;
        };
        if !rect.overlaps(&r, gap) {
            continue;
        }
        let node_dir = match dir {
            CascadeDir::Split => {
                if rect.center().y >= r.center().y {
                    CascadeDir::Down
                } else {
                    CascadeDir::Up
                }
            }
            other => other,
        };
        direction.insert(id, node_dir);
        queue.push_back((id, node_dir, r));
    }

    let mut moved: HashSet<u64> = HashSet::new();
    let mut pops = 0usize;

    while let Some((id, node_dir, blocker)) = queue.pop_front() {
        pops += 1;
        if pops > MAX_CASCADE_POPS {
            debug_assert!(
                false,
                "cascade did not settle within {MAX_CASCADE_POPS} pops"
            );
            break;
        }

        let Some(before) = rect_of(network, sizes, id) else {
            continue;
        };

        // Minimum (possibly zero) move along `dir` that clears the blocker …
        let mut after = before;
        clear(&mut after, &blocker, node_dir, gap);
        // … then far enough to clear any `fixed` node it newly landed on.
        for &other in &ids {
            if other == id || !fixed.contains(&other) {
                continue;
            }
            let Some(rect) = rect_of(network, sizes, other) else {
                continue;
            };
            if rect.overlaps(&before, gap) {
                continue; // it was already there; not this call's problem
            }
            if after.overlaps(&rect, gap) {
                clear(&mut after, &rect, node_dir, gap);
            }
        }

        if after.pos.y == before.pos.y {
            continue;
        }
        if let Some(node) = network.nodes.get_mut(&id) {
            node.position.y = after.pos.y;
        }
        moved.insert(id);

        // Everything this move *newly* overlaps inherits the direction.
        for &other in &ids {
            if other == id || fixed.contains(&other) {
                continue;
            }
            let Some(rect) = rect_of(network, sizes, other) else {
                continue;
            };
            if !after.overlaps_x(&rect, gap) {
                continue;
            }
            if !after.overlaps_y(&rect, gap) || before.overlaps_y(&rect, gap) {
                continue;
            }
            direction.insert(other, node_dir);
            queue.push_back((other, node_dir, after));
        }
    }

    let mut out: Vec<u64> = moved.into_iter().collect();
    out.sort_unstable();
    out
}

/// Move `rect` along `dir` by the least amount that leaves `gap` between it and
/// `blocker`. Never moves backwards.
fn clear(rect: &mut Rect, blocker: &Rect, dir: CascadeDir, gap: f64) {
    let target = match dir.sign() {
        s if s > 0.0 => blocker.bottom() + gap,
        _ => blocker.top() - gap - rect.size.y,
    };
    if dir.sign() > 0.0 {
        rect.pos.y = rect.pos.y.max(target);
    } else {
        rect.pos.y = rect.pos.y.min(target);
    }
}

// ---------------------------------------------------------------------------
// grow_rect
// ---------------------------------------------------------------------------

/// `node_id`'s footprint grew from `old` to `new` with its top-left fixed: make
/// room for it (D11).
///
/// The one operation every growth path uses — a node gaining a pin, an HOF body
/// growing, a collapsed HOF re-expanding, a custom instance being inlined. The
/// width delta is a [`shift_half_plane`] and the height delta a downward
/// [`cascade`], **in that order**, so the cascade sees post-shift positions.
///
/// Forcing the cascade's direction to `Down` is correct because the
/// pre-existing overlaps with the *old* rect are excluded: after the shift
/// every remaining new collision lies below the old bottom edge. A node left of
/// the line that reaches into the widened strip already overlapped the old rect
/// horizontally, so if its overlap is new, it is new in y — and the rect only
/// grew downward.
///
/// Returns `(id, old position, new position)` for every node that actually
/// moved, ascending id — the shape `ScopedMoves` bundles into the undo step.
pub fn grow_rect(
    network: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    node_id: u64,
    old: DVec2,
    new: DVec2,
) -> Vec<(u64, DVec2, DVec2)> {
    let Some(node) = network.nodes.get(&node_id) else {
        return Vec::new();
    };
    let anchor = node.position;
    let delta = (new - old).max(DVec2::ZERO);
    if delta.x == 0.0 && delta.y == 0.0 {
        return Vec::new();
    }

    let before: HashMap<u64, DVec2> = network
        .nodes
        .iter()
        .map(|(&id, node)| (id, node.position))
        .collect();

    let old_rect = Rect::new(anchor, old);
    let new_rect = Rect::new(anchor, new);

    // Pre-existing overlaps with the old rect, captured before anything moves.
    let ignore: HashSet<u64> = placed_ids(sizes)
        .into_iter()
        .filter(|&id| id != node_id)
        .filter(|&id| {
            rect_of(network, sizes, id).is_some_and(|r| r.overlaps(&old_rect, VERTICAL_GAP))
        })
        .collect();

    if delta.x > 0.0 {
        // The window: everything right of the old right edge must move, the
        // node itself and its inputs must not.
        let fixed = upstream_closure(network, [node_id]);
        let right_end = old_rect.right();
        let threshold = snap_shift_line(network, sizes, right_end, right_end, &fixed);
        shift_half_plane(network, sizes, threshold, delta.x, &fixed);
    }

    if delta.y > 0.0 {
        let mut hold = HashSet::new();
        hold.insert(node_id);
        cascade(network, sizes, new_rect, CascadeDir::Down, &hold, &ignore);
    }

    let mut moves: Vec<(u64, DVec2, DVec2)> = before
        .into_iter()
        .filter_map(|(id, old_pos)| {
            let new_pos = network.nodes.get(&id)?.position;
            (new_pos != old_pos).then_some((id, old_pos, new_pos))
        })
        .collect();
    moves.sort_by_key(|&(id, _, _)| id);
    moves
}

/// The horizontal clearance every layout path keeps between two nodes — the
/// design's `GAP`.
///
/// Re-exported here so a caller of these primitives does not have to reach into
/// `node_layout` for it.
pub const GAP: f64 = DEFAULT_HORIZONTAL_GAP;
