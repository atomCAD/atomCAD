//! The incremental layout pass, step by step.
//!
//! `doc/design_incremental_layout.md` §Algorithm — eight steps per scope, run
//! inside-out over the scopes of the post-edit network. Layout consumes one
//! [`EditDelta`](crate::layout::EditDelta) per scope and repairs only what that
//! delta broke; the pre-edit drawing is the baseline, however irregular.
//!
//! Phase 2 landed Step 2 and the primitives it stands on
//! ([`crate::layout::motion`]). Phase 3 landed Steps 1, 3, 4 and 5 — block
//! layout, placement and fitting — and the inside-out driver
//! ([`layout_incremental`]). Phase 4 completes the pass with Steps 6
//! (backward-wire repair), 7 (new comments) and 8 (drifted comments), and with
//! the two [`hand_moved`](crate::node_network::Node::hand_moved) tiebreakers
//! that make the fitting prefer to disturb a node nobody placed deliberately.
//!
//! # The one rule everything else depends on
//!
//! **An added node is invisible until its block is placed.** It is not an
//! obstacle, not a snap candidate, not part of the drawing's bounding box, and
//! neither motion primitive moves it. That is expressed by keeping it out of
//! the `sizes` map the primitives read the drawing through
//! ([`measure_scope`](crate::layout::motion::measure_scope) minus the added
//! set); it joins that map the moment its block lands. Get this wrong and
//! Step 2 pushes junk-positioned new nodes around while Step 5 slides around
//! phantoms.

use std::collections::{HashMap, HashSet, VecDeque};

use glam::DVec2;

use crate::layout::common::{
    COMMENT_CLEARANCE, LayoutAlgorithm, START_X, START_Y, VERTICAL_GAP, anchor_placement_box,
    is_comment, surrounding_candidates,
};
use crate::layout::delta::{DeltaTotals, EditDelta, WireKey, diff_scope, scopes_inside_out};
use crate::layout::motion::{
    CascadeDir, GAP, Rect, cascade, grow_rect, measure_scope, shift_half_plane, snap_shift_line,
    upstream_closure,
};
use crate::layout::size::{rendered_body_size, rendered_node_size};
use crate::node_layout::{FIRST_PIN_OFFSET, HOF_BODY_BOTTOM_PADDING, PER_PARAM_HEIGHT};
use crate::node_network::{NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;
use crate::text_format::{NamePath, PositionSnapshot};

/// The node box both hand-drawn corpora were measured at (160 x 83).
const TYPICAL_NODE_HEIGHT: f64 = 83.0;

/// How far from its anchor-derived target Step 5a will slide a block before it
/// gives up and pushes instead.
///
/// **The design's one tuning constant** — two node heights: far enough to clear
/// a neighbouring row, near enough that the block stays visibly beside the
/// nodes it is wired to. Sliding moves nothing; pushing moves the drawing, so
/// the window is the budget for finding a free spot at no cost.
pub const SLIDE_WINDOW: f64 = 2.0 * TYPICAL_NODE_HEIGHT;

/// How much further Step 5a will look when everything blocking the ordinary
/// window is [`hand_moved`](crate::node_network::Node::hand_moved).
///
/// The second of the flag's three uses (design doc, "Uses of `hand_moved`"):
/// sliding costs nothing and pushing a node a human placed deliberately is the
/// most expensive thing this pass can do, so it is worth looking twice as far
/// for free space first. A tiebreaker, never a constraint — if the extended
/// window is full too, the cascade runs and the hand-placed nodes move.
const HAND_MOVED_SLIDE_EXTENSION: f64 = 2.0;

// ---------------------------------------------------------------------------
// Step 2 — repair grown nodes
// ---------------------------------------------------------------------------

/// **Step 2 — Repair grown nodes.**
///
/// For each `(id, old, new)` in `delta.grown`, ascending id, `grow_rect`. The
/// grown node itself never moves; everything the growth reaches does, by the
/// half-plane shift on x and the cascade on y.
///
/// Positions are re-read between entries — `grow_rect` reads the live network
/// each time — so two grown nodes in one scope compose rather than fight: the
/// second one's shift line and cascade see where the first one left things.
///
/// Runs **before** blocks are placed (Step 3), so a block is fitted against
/// obstacles that have already settled.
///
/// `sizes` is the measured placed set
/// ([`measure_scope`](crate::layout::motion::measure_scope)); its keys are the
/// nodes that count as obstacles, which deliberately excludes the added nodes
/// until their block is placed.
///
/// Returns `(id, old position, new position)` for every node that moved,
/// ascending id, with one entry per node however many `grow_rect` calls touched
/// it.
pub fn repair_grown(
    network: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    delta: &EditDelta,
) -> Vec<(u64, DVec2, DVec2)> {
    // `id -> (first old position, latest new position)`, so a node moved twice
    // reports the net move rather than two overlapping ones.
    let mut net_moves: HashMap<u64, (DVec2, DVec2)> = HashMap::new();

    let mut grown = delta.grown.clone();
    grown.sort_by_key(|&(id, _, _)| id);
    for (id, old, new) in grown {
        for (moved, from, to) in grow_rect(network, sizes, id, old, new) {
            net_moves
                .entry(moved)
                .and_modify(|entry| entry.1 = to)
                .or_insert((from, to));
        }
    }

    let mut out: Vec<(u64, DVec2, DVec2)> = net_moves
        .into_iter()
        .filter(|(_, (from, to))| from != to)
        .map(|(id, (from, to))| (id, from, to))
        .collect();
    out.sort_by_key(|&(id, _, _)| id);
    out
}

// ---------------------------------------------------------------------------
// Step 3 — the block
// ---------------------------------------------------------------------------

/// One connected group of freshly added nodes, laid out by itself (D2).
///
/// The rects are in the block's **own** coordinates, with the bounding box's
/// top-left at the origin, so placing the block is a single translation and the
/// arrangement the layout algorithm computed survives it exactly.
#[derive(Debug, Clone)]
pub struct Block {
    /// The block's nodes, ascending id.
    pub ids: Vec<u64>,
    /// Each node's box in block-local coordinates.
    pub rects: HashMap<u64, Rect>,
    /// The block's bounding box, `(W, H)`.
    pub size: DVec2,
}

impl Block {
    /// A block of one node, its own bounding box.
    ///
    /// Step 3 never builds one — a lone added node is a one-member connected
    /// component and goes through the same path as any other — but Step 7 does:
    /// a new comment has no wires to lay it out by, and wrapping it as a block
    /// is what lets it reuse Step 5's fitting and Step 4's anchorless rule
    /// unchanged.
    fn single(id: u64, size: DVec2) -> Self {
        Self {
            ids: vec![id],
            rects: HashMap::from([(id, Rect::new(DVec2::ZERO, size))]),
            size,
        }
    }
}

/// What Step 4 needs to know about the body it is placing a block inside.
///
/// `None` at the top level. Inside a body the scope's own edges are anchors
/// too: `$element` and `output` are the dominant wires of a 2.9-node body, and
/// without them nearly every block would be anchorless and pile up in a corner.
#[derive(Debug, Clone)]
pub struct BodyFrame {
    /// The body region as it renders today — `max(stored, content + padding)`.
    /// Its width is the x the `output` anchor sits on.
    pub rendered: DVec2,
    /// The **stored** body size: the slack Step 5a searches before it lets the
    /// body grow and cascade that growth into the parent.
    pub stored: DVec2,
    /// `(body node id, zone-output pin index)` for every wire feeding the
    /// body's `output`, sorted.
    pub output_sources: Vec<(u64, usize)>,
}

/// One anchor: something already placed that a block is wired to.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    /// The anchor's box — a kept node's rect, or a zero-width box on the body's
    /// left or right edge for a cross-scope wire.
    rect: Rect,
    /// Vertical centre, in **block-local** coordinates, of the block node this
    /// wire attaches to.
    internal_center_y: f64,
    /// The kept node behind the anchor, if any. A synthesized edge anchor has
    /// none, and so contributes nothing to a shift's `fixed` set.
    node: Option<u64>,
}

/// The two anchor sets of one block: `U` feeds it, `D` is fed by it.
#[derive(Debug, Default, Clone)]
struct Anchors {
    upstream: Vec<Anchor>,
    downstream: Vec<Anchor>,
}

impl Anchors {
    fn is_empty(&self) -> bool {
        self.upstream.is_empty() && self.downstream.is_empty()
    }

    fn all(&self) -> impl Iterator<Item = &Anchor> {
        self.upstream.iter().chain(self.downstream.iter())
    }
}

// ---------------------------------------------------------------------------
// One scope, Steps 1-5
// ---------------------------------------------------------------------------

/// Run all eight steps of the incremental pass on one scope.
///
/// 1. **Take stock.** Nothing moves. The removed nodes are gone already and
///    leave holes (D4). The obstacle set is the kept nodes — every node in the
///    scope except the ones this edit added.
/// 2. **Repair grown nodes** ([`repair_grown`]).
/// 3. **Lay the added nodes out as blocks**, each connected component by
///    itself, through [`layout_subgraph`](crate::layout::layout_subgraph).
/// 4. **Choose each block's target position** from its anchors.
/// 5. **Fit it in**: slide into free space if there is any near the target,
///    else place it at the target and cascade the obstacles out of the way.
/// 6. **Repair the backward wires this edit created**
///    ([`repair_backward_wires`]).
/// 7. **Place the comments this edit created** ([`place_new_comments`]).
/// 8. **Pull a drifted comment back to its anchor**
///    ([`restore_drifted_comments`]).
///
/// `frame` is `Some` when `scope` is an HOF body, and carries the synthesized
/// anchors and the slack the body still has. Returns `(id, from, to)` for every
/// node whose position changed, ascending id — including the added nodes, which
/// move from the throwaway position the editor gave them at creation
/// (`auto_layout::calculate_new_node_position`) to wherever their block landed.
pub fn layout_scope(
    scope: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    delta: &EditDelta,
    frame: Option<&BodyFrame>,
    algorithm: LayoutAlgorithm,
) -> Vec<(u64, DVec2, DVec2)> {
    let before: HashMap<u64, DVec2> = scope
        .nodes
        .iter()
        .map(|(&id, node)| (id, node.position))
        .collect();

    // --- Step 1: take stock -------------------------------------------------
    let added: HashSet<u64> = delta
        .added
        .iter()
        .copied()
        .filter(|id| scope.nodes.contains_key(id))
        .collect();
    let measured = measure_scope(scope, registry);
    let mut sizes: HashMap<u64, DVec2> = measured
        .iter()
        .filter(|(id, _)| !added.contains(id))
        .map(|(&id, &size)| (id, size))
        .collect();

    // --- Step 2: repair grown nodes -----------------------------------------
    repair_grown(scope, &sizes, delta);

    // --- Step 3: lay the added nodes out as blocks --------------------------
    let blocks = build_blocks(scope, registry, &added, &measured, algorithm);

    // --- Steps 4 and 5: place them ------------------------------------------
    place_blocks(scope, &mut sizes, &blocks, frame);

    // --- Step 6: repair the backward wires this edit created -----------------
    repair_backward_wires(scope, &sizes, delta, &added);

    // --- Step 7: place the comments this edit created ------------------------
    place_new_comments(scope, registry, &mut sizes, &added, frame);

    // --- Step 8: pull a drifted comment back to its anchor -------------------
    restore_drifted_comments(scope, registry, &sizes, &added, &before, frame);

    let mut moves: Vec<(u64, DVec2, DVec2)> = before
        .iter()
        .filter_map(|(&id, &from)| {
            let to = scope.nodes.get(&id)?.position;
            (to != from).then_some((id, from, to))
        })
        .collect();
    moves.sort_by_key(|&(id, _, _)| id);
    moves
}

/// **Step 3.** Split the added nodes into connected components and lay each one
/// out on its own.
///
/// A comment is never in a block: it has no wires to lay it out by, and a *new*
/// comment is placed beside its anchor by Step 7 instead (D9, Phase 4).
fn build_blocks(
    scope: &NodeNetwork,
    registry: &NodeTypeRegistry,
    added: &HashSet<u64>,
    measured: &HashMap<u64, DVec2>,
    algorithm: LayoutAlgorithm,
) -> Vec<Block> {
    let members: HashSet<u64> = added
        .iter()
        .copied()
        .filter(|id| scope.nodes.get(id).is_some_and(|node| !is_comment(node)))
        .collect();

    let mut blocks = Vec::new();
    for component in components(scope, &members) {
        let ids: HashSet<u64> = component.iter().copied().collect();
        let local = crate::layout::layout_subgraph(scope, registry, &ids, algorithm);

        let mut rects: HashMap<u64, Rect> = HashMap::new();
        let mut min = DVec2::splat(f64::MAX);
        let mut max = DVec2::splat(f64::MIN);
        for &id in &component {
            let size = measured.get(&id).copied().unwrap_or_else(|| {
                scope
                    .nodes
                    .get(&id)
                    .map(|node| rendered_node_size(node, registry))
                    .unwrap_or(DVec2::ZERO)
            });
            let pos = local.get(&id).copied().unwrap_or(DVec2::ZERO);
            min = min.min(pos);
            max = max.max(pos + size);
            rects.insert(id, Rect::new(pos, size));
        }
        // Normalize: the block's bounding box starts at its own origin, so a
        // placement is one translation and nothing inside it is re-derived.
        for rect in rects.values_mut() {
            rect.pos -= min;
        }
        blocks.push(Block {
            ids: component,
            rects,
            size: max - min,
        });
    }
    blocks
}

/// The connected components of `members`, over the same-scope wires **between**
/// members. Each component ascending by id, the components ordered by their
/// smallest id (D7).
fn components(scope: &NodeNetwork, members: &HashSet<u64>) -> Vec<Vec<u64>> {
    let mut seeds: Vec<u64> = members.iter().copied().collect();
    seeds.sort_unstable();

    // Undirected adjacency, built once. A wire counts only when both of its
    // ends are added nodes: a wire to a kept node is an *anchor*, not an edge.
    let mut adjacency: HashMap<u64, Vec<u64>> = HashMap::new();
    for &id in &seeds {
        let Some(node) = scope.nodes.get(&id) else {
            continue;
        };
        for argument in &node.arguments {
            for wire in &argument.incoming_wires {
                if wire.source_scope_depth != 0
                    || !matches!(wire.source_pin, SourcePin::NodeOutput { .. })
                    || !members.contains(&wire.source_node_id)
                {
                    continue;
                }
                adjacency.entry(id).or_default().push(wire.source_node_id);
                adjacency.entry(wire.source_node_id).or_default().push(id);
            }
        }
    }

    let mut seen: HashSet<u64> = HashSet::new();
    let mut out: Vec<Vec<u64>> = Vec::new();
    for seed in seeds {
        if !seen.insert(seed) {
            continue;
        }
        let mut component = vec![seed];
        let mut queue: VecDeque<u64> = VecDeque::from([seed]);
        while let Some(id) = queue.pop_front() {
            let mut neighbours = adjacency.get(&id).cloned().unwrap_or_default();
            neighbours.sort_unstable();
            for other in neighbours {
                if seen.insert(other) {
                    component.push(other);
                    queue.push_back(other);
                }
            }
        }
        component.sort_unstable();
        out.push(component);
    }
    out
}

// ---------------------------------------------------------------------------
// Steps 4 and 5 — placement
// ---------------------------------------------------------------------------

/// Place every block, in order of `(min anchor x, min node id)`, each joining
/// the obstacle set as it lands.
///
/// The order matters because a block placed first is an obstacle for the next
/// one, and anchoring the queue to the leftmost anchor fills the drawing the
/// way the data flows. The sort key is taken once, against the pre-placement
/// drawing: a block's anchors are kept nodes, never other blocks (two added
/// nodes with a wire between them are in the *same* component), so nothing a
/// placement does can reorder the queue.
fn place_blocks(
    scope: &mut NodeNetwork,
    sizes: &mut HashMap<u64, DVec2>,
    blocks: &[Block],
    frame: Option<&BodyFrame>,
) {
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    let keys: Vec<(f64, u64)> = blocks
        .iter()
        .map(|block| {
            let anchors = anchors_of(scope, sizes, block, frame);
            let min_x = anchors
                .all()
                .map(|anchor| anchor.rect.left())
                .fold(f64::INFINITY, f64::min);
            (min_x, block.ids.first().copied().unwrap_or(u64::MAX))
        })
        .collect();
    order.sort_by(|&a, &b| {
        keys[a]
            .0
            .total_cmp(&keys[b].0)
            .then_with(|| keys[a].1.cmp(&keys[b].1))
    });

    for index in order {
        place_block(scope, sizes, &blocks[index], frame);
    }
}

/// Steps 4 and 5 for one block.
fn place_block(
    scope: &mut NodeNetwork,
    sizes: &mut HashMap<u64, DVec2>,
    block: &Block,
    frame: Option<&BodyFrame>,
) {
    if block.ids.is_empty() {
        return;
    }
    let anchors = anchors_of(scope, sizes, block, frame);
    // Step 4 may widen the drawing (the "no room" case), so it takes the
    // network mutably; the anchors it reasons about were read before that, and
    // the shift it performs moves only nodes at or right of the block's own x.
    let target = target_position(scope, sizes, block, &anchors, frame);
    let (origin, push) = fit(scope, sizes, block, target, frame);
    commit_block(scope, sizes, block, origin, push);
}

/// Put `block` down at `origin`, cascade the drawing out of the way if the fit
/// asked for it, and let the block join the obstacle set.
///
/// The tail of Step 5, split out because Step 7 places a new comment through
/// the same three moves after choosing its origin by a rule of its own.
fn commit_block(
    scope: &mut NodeNetwork,
    sizes: &mut HashMap<u64, DVec2>,
    block: &Block,
    origin: DVec2,
    push: bool,
) {
    for &id in &block.ids {
        let Some(rect) = block.rects.get(&id) else {
            continue;
        };
        if let Some(node) = scope.nodes.get_mut(&id) {
            node.position = origin + rect.pos;
        }
    }

    if push {
        // Step 5b. The block's own nodes are still absent from `sizes`, so the
        // cascade cannot push them — only the drawing they landed on.
        let r = Rect::new(origin, block.size);
        let dir = choose_cascade_dir(scope, sizes, r);
        cascade(scope, sizes, r, dir, &HashSet::new(), &HashSet::new());
    }

    for &id in &block.ids {
        if let Some(rect) = block.rects.get(&id) {
            sizes.insert(id, rect.size);
        }
    }
}

/// Which way Step 5b's cascade should push — the first of `hand_moved`'s three
/// uses (design doc, "Uses of `hand_moved`").
///
/// [`CascadeDir::Split`] is Step 5b's specified direction and stays the default:
/// with nothing hand-placed in the scope there is nothing to tie-break, and
/// splitting the obstacles around `r`'s centre is what moves the drawing least.
/// When the scope *does* hold hand-placed nodes, all three directions are
/// simulated and scored by the design's chain — **fewer hand-moved nodes
/// displaced, then fewer nodes, then less total displacement** — so a uniform
/// push that spares a node a human placed wins over a split that does not.
/// `Up` is tried before `Down`, and an exact tie keeps `Split`.
///
/// Simulating means running the real cascade and undoing it: a cascade only
/// ever writes `position.y` of a placed node, so restoring those is exact, and
/// it is far cheaper than cloning the network three times.
fn choose_cascade_dir(scope: &mut NodeNetwork, sizes: &HashMap<u64, DVec2>, r: Rect) -> CascadeDir {
    let hand: HashSet<u64> = placed_ids(sizes)
        .into_iter()
        .filter(|id| scope.nodes.get(id).is_some_and(|node| node.hand_moved))
        .collect();
    if hand.is_empty() {
        return CascadeDir::Split;
    }

    let saved: Vec<(u64, f64)> = placed_ids(sizes)
        .into_iter()
        .filter_map(|id| scope.nodes.get(&id).map(|node| (id, node.position.y)))
        .collect();

    let mut best: Option<((usize, usize, f64), CascadeDir)> = None;
    for dir in [CascadeDir::Split, CascadeDir::Up, CascadeDir::Down] {
        let moved = cascade(scope, sizes, r, dir, &HashSet::new(), &HashSet::new());
        let hand_count = moved.iter().filter(|id| hand.contains(id)).count();
        let displacement: f64 = saved
            .iter()
            .filter_map(|&(id, y)| scope.nodes.get(&id).map(|node| (node.position.y - y).abs()))
            .sum();
        for &(id, y) in &saved {
            if let Some(node) = scope.nodes.get_mut(&id) {
                node.position.y = y;
            }
        }

        let score = (hand_count, moved.len(), displacement);
        let better = best.as_ref().is_none_or(|(best_score, _)| {
            (score.0, score.1)
                .cmp(&(best_score.0, best_score.1))
                .then_with(|| score.2.total_cmp(&best_score.2))
                .is_lt()
        });
        if better {
            best = Some((score, dir));
        }
    }
    best.map(|(_, dir)| dir).unwrap_or(CascadeDir::Split)
}

/// **Step 4 anchors.** Every placed node wired to the block, plus — inside a
/// body — the scope's own edges.
///
/// | Wire | Synthesized anchor |
/// |---|---|
/// | `$name` (any depth), `^name` | zero-width box at the body's left edge, `x = 0`, at the pin's rendered y (body top for a capture) |
/// | the body's `output` | zero-width box at the body's right edge, `x = body_width`, at the output pin's y |
fn anchors_of(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    block: &Block,
    frame: Option<&BodyFrame>,
) -> Anchors {
    let mut anchors = Anchors::default();
    let members: HashSet<u64> = block.ids.iter().copied().collect();

    // Wires **into** the block.
    for &id in &block.ids {
        let (Some(node), Some(rect)) = (scope.nodes.get(&id), block.rects.get(&id)) else {
            continue;
        };
        let internal_center_y = rect.center().y;
        for argument in &node.arguments {
            for wire in &argument.incoming_wires {
                if wire.source_scope_depth == 0
                    && matches!(wire.source_pin, SourcePin::NodeOutput { .. })
                {
                    if members.contains(&wire.source_node_id) {
                        continue; // internal to the block
                    }
                    if let Some(rect) = rect_of(scope, sizes, wire.source_node_id) {
                        anchors.upstream.push(Anchor {
                            rect,
                            internal_center_y,
                            node: Some(wire.source_node_id),
                        });
                    }
                    continue;
                }
                // Cross-scope: the wire enters this body at its left edge.
                if frame.is_some() {
                    let y = match wire.source_pin {
                        SourcePin::ZoneInput { pin_index } => {
                            FIRST_PIN_OFFSET + pin_index as f64 * PER_PARAM_HEIGHT
                        }
                        // A capture has no pin row of its own on the body edge.
                        SourcePin::NodeOutput { .. } => 0.0,
                    };
                    anchors.upstream.push(Anchor {
                        rect: Rect::new(DVec2::new(0.0, y), DVec2::ZERO),
                        internal_center_y,
                        node: None,
                    });
                }
            }
        }
    }

    // Wires **out of** the block, into a kept node.
    for id in placed_ids(sizes) {
        if members.contains(&id) {
            continue;
        }
        let (Some(node), Some(rect)) = (scope.nodes.get(&id), rect_of(scope, sizes, id)) else {
            continue;
        };
        for argument in &node.arguments {
            for wire in &argument.incoming_wires {
                if wire.source_scope_depth != 0
                    || !matches!(wire.source_pin, SourcePin::NodeOutput { .. })
                    || !members.contains(&wire.source_node_id)
                {
                    continue;
                }
                let Some(source) = block.rects.get(&wire.source_node_id) else {
                    continue;
                };
                anchors.downstream.push(Anchor {
                    rect,
                    internal_center_y: source.center().y,
                    node: Some(id),
                });
            }
        }
    }

    // …and out of the block into the body's `output`.
    if let Some(frame) = frame {
        for &(source_id, index) in &frame.output_sources {
            let Some(source) = block.rects.get(&source_id) else {
                continue;
            };
            let y = FIRST_PIN_OFFSET + index as f64 * PER_PARAM_HEIGHT;
            anchors.downstream.push(Anchor {
                rect: Rect::new(DVec2::new(frame.rendered.x, y), DVec2::ZERO),
                internal_center_y: source.center().y,
                node: None,
            });
        }
    }

    anchors
}

/// **Step 4.** Where the block wants to go, before anything is checked for
/// collisions.
///
/// Horizontally it sits right of everything that feeds it and left of
/// everything it feeds; when those two demands conflict there is no room, and
/// the drawing is widened by a half-plane shift placed by the window rule — the
/// consumers and everything else at or right of the line move over by exactly
/// the deficit, while the inputs and their whole upstream closure stay.
/// Vertically the block is centred on its wires.
///
/// Inside a body the result is clamped to non-negative: a body's extent is
/// measured from its own origin, so content at a negative coordinate is simply
/// not measured and renders outside the box.
fn target_position(
    scope: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    block: &Block,
    anchors: &Anchors,
    frame: Option<&BodyFrame>,
) -> DVec2 {
    let bbox = drawing_bbox(scope, sizes);
    let width = block.size.x;

    let x_min = anchors
        .upstream
        .iter()
        .map(|anchor| anchor.rect.right() + GAP)
        .fold(f64::NEG_INFINITY, f64::max);
    let x_max = anchors
        .downstream
        .iter()
        .map(|anchor| anchor.rect.left() - GAP - width)
        .fold(f64::INFINITY, f64::min);

    let x = match (anchors.upstream.is_empty(), anchors.downstream.is_empty()) {
        // Anchorless: right of the whole drawing, where nothing can be in the way.
        (true, true) => match bbox {
            Some(bbox) => bbox.right() + GAP,
            None if frame.is_some() => HOF_BODY_BOTTOM_PADDING,
            None => START_X,
        },
        (true, false) => x_max,
        (false, true) => x_min,
        (false, false) => {
            if x_min > x_max {
                // No room. Widen the drawing by exactly the deficit: the line
                // starts at `x_min`, is clamped to the leftmost consumer (which
                // is what makes a consumer that overlaps its input horizontally
                // move at all), and snaps left out of any loose column.
                let seeds: Vec<u64> = anchors
                    .upstream
                    .iter()
                    .filter_map(|anchor| anchor.node)
                    .collect();
                let fixed = upstream_closure(scope, seeds);
                let right_end = anchors
                    .downstream
                    .iter()
                    .map(|anchor| anchor.rect.left())
                    .fold(f64::INFINITY, f64::min);
                let threshold = snap_shift_line(scope, sizes, x_min, right_end, &fixed);
                shift_half_plane(scope, sizes, threshold, x_min - x_max, &fixed);
            }
            x_min
        }
    };

    let y = if anchors.is_empty() {
        match bbox {
            Some(bbox) => bbox.bottom() + VERTICAL_GAP,
            // An empty body: level with the first zone-input pin.
            None if frame.is_some() => FIRST_PIN_OFFSET - block.size.y / 2.0,
            None => START_Y,
        }
    } else {
        // Align by connections: on average, every wire arrives level.
        let mut sum = 0.0;
        let mut count = 0.0;
        for anchor in anchors.all() {
            sum += anchor.rect.center().y - anchor.internal_center_y;
            count += 1.0;
        }
        sum / count
    };

    let target = DVec2::new(x, y);
    if frame.is_some() {
        target.max(DVec2::ZERO)
    } else {
        target
    }
}

/// **Step 5.** Find the block a home near its target.
///
/// **(5a) Slide.** Search for a free `y` near the target, alternating down and
/// up in [`VERTICAL_GAP`] steps within [`SLIDE_WINDOW`]. Nothing moves.
///
/// Inside a body the slide is **slack-first**: growth past the stored body size
/// cascades into the parent and moves the HOF's neighbours, so every candidate
/// inside the stored size is searched first — over the whole body height, not
/// just the window — and the ordinary window is only used when none of them is
/// free.
///
/// **(5b) Push.** Otherwise the block goes at its target `y` and the caller
/// cascades whatever it landed on out of the way.
///
/// One tiebreaker sits between the two: when *every* node blocking the ordinary
/// window is [`hand_moved`](crate::node_network::Node::hand_moved), the window
/// is widened by [`HAND_MOVED_SLIDE_EXTENSION`] and searched again before the
/// pass gives up and pushes. Sliding costs nothing; pushing a hand-placed node
/// is the most expensive thing this pass can do.
///
/// Returns the origin to place the block at and whether the caller owes it a
/// cascade.
fn fit(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    block: &Block,
    target: DVec2,
    frame: Option<&BodyFrame>,
) -> (DVec2, bool) {
    // 5a, slack-first: everything the stored body size can still absorb.
    if let Some(frame) = frame {
        let limit = frame.stored.y - HOF_BODY_BOTTOM_PADDING - block.size.y;
        if limit >= 0.0 {
            let mut candidates = vec![target.y.clamp(0.0, limit)];
            let mut y = 0.0;
            while y <= limit {
                candidates.push(y);
                y += VERTICAL_GAP;
            }
            candidates.sort_by(|a, b| (a - target.y).abs().total_cmp(&(b - target.y).abs()));
            let free = candidates.into_iter().find(|&y| {
                obstacles_at(scope, sizes, DVec2::new(target.x, y), block.size).is_empty()
            });
            if let Some(y) = free {
                return (DVec2::new(target.x, y), false);
            }
        }
    }

    // 5a, the ordinary window.
    let mut blockers: HashSet<u64> = HashSet::new();
    if let Some(y) = slide(
        scope,
        sizes,
        block.size,
        target,
        frame,
        SLIDE_WINDOW,
        &mut blockers,
    ) {
        return (DVec2::new(target.x, y), false);
    }

    // 5a, extended: nothing in the band was put there by the algorithm.
    let all_hand_placed = !blockers.is_empty()
        && blockers
            .iter()
            .all(|id| scope.nodes.get(id).is_some_and(|node| node.hand_moved));
    if all_hand_placed
        && let Some(y) = slide(
            scope,
            sizes,
            block.size,
            target,
            frame,
            SLIDE_WINDOW * HAND_MOVED_SLIDE_EXTENSION,
            &mut HashSet::new(),
        )
    {
        return (DVec2::new(target.x, y), false);
    }

    // 5b.
    (target, true)
}

/// Search for a free `y` at `target.x`, alternating down and up in
/// [`VERTICAL_GAP`] steps out to `window`. Nothing moves.
///
/// Every node that blocked a candidate is added to `blockers`, which is what
/// lets the caller ask whether the whole band was hand-placed.
fn slide(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    size: DVec2,
    target: DVec2,
    frame: Option<&BodyFrame>,
    window: f64,
    blockers: &mut HashSet<u64>,
) -> Option<f64> {
    let mut step = 0.0;
    while step <= window {
        for y in [target.y + step, target.y - step] {
            if frame.is_some() && y < 0.0 {
                continue;
            }
            let hits = obstacles_at(scope, sizes, DVec2::new(target.x, y), size);
            if hits.is_empty() {
                return Some(y);
            }
            blockers.extend(hits);
        }
        step += VERTICAL_GAP;
    }
    None
}

/// The placed nodes a box at `position` would come within `clearance` of.
///
/// Empty means the spot is free. `except` is for the callers asking on behalf
/// of a node already in the placed set (Steps 7 and 8, placing and re-homing a
/// comment), which must not read its own current box as an obstacle.
///
/// `clearance` is [`VERTICAL_GAP`] for a block being fitted, and
/// [`COMMENT_CLEARANCE`] for a comment being placed against its anchor: the
/// four candidate positions sit at the *anchor* gap, which is smaller than the
/// layout's ordinary vertical clearance, so testing them at 30 px would reject
/// all four of them every time and send every new anchored comment to the
/// fallback.
fn obstacles_at_except(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    position: DVec2,
    size: DVec2,
    clearance: f64,
    except: Option<u64>,
) -> Vec<u64> {
    let candidate = Rect::new(position, size);
    placed_ids(sizes)
        .into_iter()
        .filter(|id| Some(*id) != except)
        .filter(|&id| {
            rect_of(scope, sizes, id).is_some_and(|rect| rect.overlaps(&candidate, clearance))
        })
        .collect()
}

/// [`obstacles_at_except`] at the ordinary clearance, with nothing excluded.
fn obstacles_at(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    position: DVec2,
    size: DVec2,
) -> Vec<u64> {
    obstacles_at_except(scope, sizes, position, size, VERTICAL_GAP, None)
}

// ---------------------------------------------------------------------------
// Step 6 — repair new backward wires
// ---------------------------------------------------------------------------

/// **Step 6 — Repair the backward wires this edit created.**
///
/// For each wire in `delta.added_wires` whose two ends are both **kept** nodes
/// of this scope: if the source's right edge plus the gap reaches past the
/// destination's left edge, widen the drawing by exactly that deficit with a
/// half-plane shift placed by the window rule, holding the source and its whole
/// upstream closure fixed.
///
/// That covers the genuinely backward wire — a destination sitting *left* of
/// its source — and not merely a consumer crowding its producer. A plain
/// half-plane shift could never fix the former, since it preserves x-order and
/// any line at or left of the destination would carry the source along; the
/// `fixed` closure is what lets the destination and everything at or right of
/// the line move over while the producer chain stays. No other wire can flip,
/// which is the closure's standing guarantee.
///
/// Three exclusions, each deliberate:
///
/// - **A wire that was already backward is left alone.** It is not in
///   `added_wires`, and the pre-edit drawing is the baseline however irregular.
/// - **Cross-scope wires are skipped**: the two ends are in different
///   coordinate frames, so "left of" is not a statement about them.
/// - **A wire touching an added node is skipped**: Step 4 already placed that
///   block against this very wire as an anchor, and re-repairing it here would
///   shift the drawing a second time for one insertion.
///
/// Positions are re-read between entries, so several new wires in one scope
/// compose rather than fight.
fn repair_backward_wires(
    scope: &mut NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    delta: &EditDelta,
    added: &HashSet<u64>,
) {
    if delta.added_wires.is_empty() {
        return;
    }
    // Names are unique within a scope, so the last segment of a path is enough
    // to resolve it here — and it is the only handle that survives a
    // `--replace`, which mints fresh ids for everything.
    let by_name: HashMap<String, u64> = scope
        .nodes
        .iter()
        .filter_map(|(&id, node)| node.custom_name.clone().map(|name| (name, id)))
        .collect();

    for key in &delta.added_wires {
        if key.is_cross_scope() {
            continue;
        }
        let (Some(source_name), Some(destination_name)) =
            (key.source.path().last(), key.destination.last())
        else {
            continue;
        };
        let (Some(&source_id), Some(&destination_id)) =
            (by_name.get(source_name), by_name.get(destination_name))
        else {
            continue;
        };
        if source_id == destination_id
            || added.contains(&source_id)
            || added.contains(&destination_id)
        {
            continue;
        }

        let (Some(source), Some(destination)) = (
            rect_of(scope, sizes, source_id),
            rect_of(scope, sizes, destination_id),
        ) else {
            continue;
        };
        let deficit = source.right() + GAP - destination.left();
        if deficit <= 0.0 {
            continue;
        }

        let fixed = upstream_closure(scope, [source_id]);
        if fixed.contains(&destination_id) {
            // The destination feeds the source: a cycle, and no shift can put
            // both ends of it in order. Leave the drawing alone.
            continue;
        }
        let right_end = destination.left();
        let threshold = snap_shift_line(scope, sizes, right_end, right_end, &fixed);
        shift_half_plane(scope, sizes, threshold, deficit, &fixed);
    }
}

// ---------------------------------------------------------------------------
// Steps 7 and 8 — comments
// ---------------------------------------------------------------------------

/// **Step 7 — Place the comments this edit created.**
///
/// Only a comment the edit *created* needs a rule; every other comment is an
/// ordinary node on this path (D9), an obstacle at its real size that the
/// cascade may push and the half-plane may shift, but that nothing ever
/// re-places by rule.
///
/// An **anchored** one (`on:`) takes the first free position among the four
/// sides of its anchor, through the same
/// [`anchor_placement_box`] / [`surrounding_candidates`] pair the full reflow
/// uses — one rule for "beside its anchor", not two that drift — and falls back
/// to Step 5 (slide, else push) from the first of those candidates when all
/// four are taken.
///
/// An **unanchored** one is an anchorless block: right of the drawing, where
/// nothing can be in the way.
///
/// Ascending id, each comment joining the obstacle set as it lands.
fn place_new_comments(
    scope: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    sizes: &mut HashMap<u64, DVec2>,
    added: &HashSet<u64>,
    frame: Option<&BodyFrame>,
) {
    let mut comments: Vec<u64> = added
        .iter()
        .copied()
        .filter(|id| scope.nodes.get(id).is_some_and(is_comment))
        .collect();
    comments.sort_unstable();

    for id in comments {
        let Some(node) = scope.nodes.get(&id) else {
            continue;
        };
        let size = rendered_node_size(node, registry);
        let block = Block::single(id, size);

        let positions = placed_positions(scope, sizes);
        let Some(target) = anchor_placement_box(scope, registry, &positions, id) else {
            // Unanchored, or anchored at something this pass has no position
            // for: an anchorless block.
            place_block(scope, sizes, &block, frame);
            continue;
        };

        let candidates: Vec<DVec2> = surrounding_candidates(target, size)
            .into_iter()
            .filter(|pos| frame.is_none() || (pos.x >= 0.0 && pos.y >= 0.0))
            .collect();
        let free = candidates.iter().copied().find(|&pos| {
            obstacles_at_except(scope, sizes, pos, size, COMMENT_CLEARANCE, Some(id)).is_empty()
        });

        let (origin, push) = match free {
            Some(pos) => (pos, false),
            None => {
                let first = candidates.first().copied().unwrap_or(target.pos);
                let start = if frame.is_some() {
                    first.max(DVec2::ZERO)
                } else {
                    first
                };
                fit(scope, sizes, &block, start, frame)
            }
        };
        commit_block(scope, sizes, &block, origin, push);
    }
}

/// **Step 8 — Pull a drifted comment back to its anchor.**
///
/// For each anchored comment that was already there before the edit, ascending
/// id: if its distance to its anchor *grew* during this pass, put it back at
/// its exact original offset — `anchor_after + (comment_before −
/// anchor_before)` — provided that spot is collision-free. Otherwise leave it
/// where it is.
///
/// All-or-nothing, and with no constant of its own. The motivating case is a
/// comment sitting just left of a shift line whose anchor was carried right:
/// the comment stayed, the anchor moved, and the leader line grew for no
/// reason. Three things this can never do: move a comment *away* from its
/// anchor (the distance test), create an overlap (the collision test), or act
/// on an anchor it cannot resolve on both sides (a target the edit removed, or
/// one the edit added, whose "before" position is the throwaway the creation
/// placer gave it).
fn restore_drifted_comments(
    scope: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    sizes: &HashMap<u64, DVec2>,
    added: &HashSet<u64>,
    before: &HashMap<u64, DVec2>,
    frame: Option<&BodyFrame>,
) {
    // The pre-edit drawing: the placed set at the positions it held when this
    // scope's pass began. Added nodes are excluded, exactly as they are from
    // every other reading of the drawing.
    let was: HashMap<u64, DVec2> = before
        .iter()
        .filter(|(id, _)| sizes.contains_key(id) && !added.contains(id))
        .map(|(&id, &position)| (id, position))
        .collect();

    let mut comments: Vec<u64> = was
        .keys()
        .copied()
        .filter(|id| scope.nodes.get(id).is_some_and(is_comment))
        .collect();
    comments.sort_unstable();

    for id in comments {
        let (Some(&comment_before), Some(&size)) = (was.get(&id), sizes.get(&id)) else {
            continue;
        };
        let Some(anchor_before) = anchor_placement_box(scope, registry, &was, id) else {
            continue;
        };
        let now = placed_positions(scope, sizes);
        let Some(anchor_after) = anchor_placement_box(scope, registry, &now, id) else {
            continue;
        };
        let Some(comment_after) = now.get(&id).copied() else {
            continue;
        };

        let drifted = (comment_after - anchor_after.pos).length()
            > (comment_before - anchor_before.pos).length();
        if !drifted {
            continue;
        }

        let target = anchor_after.pos + (comment_before - anchor_before.pos);
        if frame.is_some() && (target.x < 0.0 || target.y < 0.0) {
            continue;
        }
        if !obstacles_at_except(scope, sizes, target, size, VERTICAL_GAP, Some(id)).is_empty() {
            continue;
        }
        if let Some(node) = scope.nodes.get_mut(&id) {
            node.position = target;
        }
    }
}

// ---------------------------------------------------------------------------
// The inside-out driver
// ---------------------------------------------------------------------------

/// What one whole run of [`layout_incremental`] did.
///
/// The two halves answer different questions and neither substitutes for the
/// other: `moved` is what the pass *did to the drawing*, `totals` is what the
/// edit *asked it to do*. A long `moved` against a one-node `totals` is the
/// signal the AI edit log exists to surface.
#[derive(Debug, Default, Clone)]
pub struct IncrementalOutcome {
    /// `(name path, from, to)` for every node that moved, sorted by path.
    pub moved: Vec<(NamePath, DVec2, DVec2)>,
    /// The edit's delta, summed over every scope.
    pub totals: DeltaTotals,
}

/// Run the incremental pass over every scope of `network`, deepest first (D12).
///
/// A body is a full `NodeNetwork` with its own coordinates, so Steps 1-5 run on
/// it unchanged; the driver only orders the scopes and feeds a body's outcome to
/// its parent. **The order is what does the feeding.** An HOF's footprint is
/// `max(stored, body content + padding)`, so once a body has settled its owner
/// measures bigger — and the parent's delta is computed at the parent's own
/// turn, against the live network, so the owner is already classified `grown` at
/// its *settled* size with no accumulator to thread through. (The design writes
/// that as an explicit `grown` set handed up the loop; computing the delta late
/// is the same set, and it cannot double-count the way a union of the two would:
/// `grow_rect` moves by `new - old`, so repairing one HOF twice with two
/// different `new`s would shift its neighbours twice.)
///
/// `snapshot` and `before_wires` are the pre-edit pair
/// (`text_format::snapshot_node_positions` and
/// [`collect_all_wires`](crate::layout::collect_all_wires)), both taken before
/// the edit ran. Returns the moves — `(name path, from, to)`, sorted by path,
/// the name-path keying every part of this design uses since a `--replace`
/// mints fresh ids — together with the whole edit's [`DeltaTotals`], which the
/// AI edit log records beside them so a move can be read against whether the
/// edit had any business touching that scope.
pub fn layout_incremental(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    snapshot: &PositionSnapshot,
    before_wires: &HashSet<WireKey>,
    algorithm: LayoutAlgorithm,
) -> IncrementalOutcome {
    let mut outcome = IncrementalOutcome::default();
    let moved = &mut outcome.moved;

    for (scope_ids, scope_names) in scopes_inside_out(network) {
        let Some(delta) = diff_scope(
            network,
            registry,
            snapshot,
            before_wires,
            &scope_ids,
            &scope_names,
        ) else {
            continue; // a body whose owner this edit deleted
        };
        // A delta the layout cannot see is a scope with no work in it. Note
        // that a wire *removal* alone still qualifies as "something happened"
        // and the steps run — they are all no-ops on it (D-"wire removal
        // triggers no repair"), and spelling that out here would only give the
        // rule two homes.
        outcome.totals.add(&delta);
        if delta.is_empty() {
            continue;
        }

        let frame = body_frame(network, registry, &scope_ids, &delta.added);
        let Some(scope) = scope_mut(network, &scope_ids) else {
            continue;
        };
        for (id, from, to) in layout_scope(scope, registry, &delta, frame.as_ref(), algorithm) {
            let Some(name) = scope
                .nodes
                .get(&id)
                .and_then(|node| node.custom_name.clone())
            else {
                continue;
            };
            let mut path = scope_names.clone();
            path.push(name);
            moved.push((path, from, to));
        }
    }

    moved.sort_by(|a, b| a.0.cmp(&b.0));
    outcome
}

/// The [`BodyFrame`] for a scope, or `None` if it is the top-level network.
///
/// `added` is the scope's added set, and it is excluded from the measurement:
/// the body's right edge — the x the `output` anchor sits on — is where the
/// body renders to *before* this edit's nodes are placed. Measuring them in
/// would read the throwaway positions the editor gave them at creation, which
/// is exactly the phantom geometry the invisibility rule exists to keep out.
fn body_frame(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    scope_ids: &[u64],
    added: &[u64],
) -> Option<BodyFrame> {
    let (&owner_id, parent) = scope_ids.split_last()?;
    let owner = scope_ref(network, parent)?.nodes.get(&owner_id)?;

    let (body_width, body_height) = match owner.zone.as_deref() {
        Some(body) => {
            let mut content = DVec2::ZERO;
            for (&id, node) in &body.nodes {
                if added.contains(&id) {
                    continue;
                }
                content = content.max(node.position + rendered_node_size(node, registry));
            }
            let padded = content + DVec2::splat(HOF_BODY_BOTTOM_PADDING);
            (
                padded.x.max(owner.body_width),
                padded.y.max(owner.body_height),
            )
        }
        // Not a body after all; `rendered_body_size` degrades to the stored
        // size, which is the only answer available.
        None => rendered_body_size(owner, registry),
    };
    let mut output_sources: Vec<(u64, usize)> = Vec::new();
    for (index, argument) in owner.zone_output_arguments.iter().enumerate() {
        for wire in &argument.incoming_wires {
            // A body-return wire counts from *inside* the body, so depth 0 is
            // the body itself.
            if wire.source_scope_depth == 0
                && matches!(wire.source_pin, SourcePin::NodeOutput { .. })
            {
                output_sources.push((wire.source_node_id, index));
            }
        }
    }
    output_sources.sort_unstable();

    Some(BodyFrame {
        rendered: DVec2::new(body_width, body_height),
        stored: DVec2::new(owner.body_width, owner.body_height),
        output_sources,
    })
}

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

/// The placed ids, ascending — the deterministic iteration order (D7).
fn placed_ids(sizes: &HashMap<u64, DVec2>) -> Vec<u64> {
    let mut ids: Vec<u64> = sizes.keys().copied().collect();
    ids.sort_unstable();
    ids
}

/// The placed set as a plain `id -> position` map — the shape
/// [`anchor_placement_box`] reads a drawing through.
fn placed_positions(network: &NodeNetwork, sizes: &HashMap<u64, DVec2>) -> HashMap<u64, DVec2> {
    sizes
        .keys()
        .filter_map(|&id| network.nodes.get(&id).map(|node| (id, node.position)))
        .collect()
}

/// The box `node_id` occupies, or `None` if it is not placed.
fn rect_of(network: &NodeNetwork, sizes: &HashMap<u64, DVec2>, node_id: u64) -> Option<Rect> {
    let size = *sizes.get(&node_id)?;
    let position = network.nodes.get(&node_id)?.position;
    Some(Rect::new(position, size))
}

/// The bounding box of the placed nodes — the drawing an anchorless block is
/// placed beside. Added nodes are absent from `sizes` and so, correctly, from
/// the box.
fn drawing_bbox(network: &NodeNetwork, sizes: &HashMap<u64, DVec2>) -> Option<Rect> {
    let mut min = DVec2::splat(f64::MAX);
    let mut max = DVec2::splat(f64::MIN);
    let mut any = false;
    for id in placed_ids(sizes) {
        let Some(rect) = rect_of(network, sizes, id) else {
            continue;
        };
        any = true;
        min = min.min(rect.pos);
        max = max.max(rect.pos + rect.size);
    }
    any.then(|| Rect::new(min, max - min))
}

/// Walk down to the network a scope path names, read-only.
fn scope_ref<'n>(root: &'n NodeNetwork, scope_path: &[u64]) -> Option<&'n NodeNetwork> {
    let mut network = root;
    for owner_id in scope_path {
        network = network.nodes.get(owner_id)?.zone.as_deref()?;
    }
    Some(network)
}

/// Walk down to the network a scope path names, for mutation. Each step goes
/// through `zone_mut()` so the `Arc` copy-on-write stays intact.
fn scope_mut<'n>(root: &'n mut NodeNetwork, scope_path: &[u64]) -> Option<&'n mut NodeNetwork> {
    let mut network = root;
    for owner_id in scope_path {
        network = network.nodes.get_mut(owner_id)?.zone_mut()?;
    }
    Some(network)
}
