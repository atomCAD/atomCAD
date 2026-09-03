//! Common utilities for layout algorithms.
//!
//! This module provides shared functions used by multiple layout algorithms:
//! - Depth computation for DAG traversal
//! - Graph traversal utilities
//! - Common types

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use crate::layout::size::{comment_data, rendered_node_size_by_id};
use crate::node_layout;
use crate::node_network::{Node, NodeNetwork, SourcePin, Wire};
use crate::node_type_registry::NodeTypeRegistry;
use crate::nodes::comment::ResolvedAnchor;

/// Available layout algorithms for full network reorganization.
///
/// These algorithms reorganize the entire network. They are used:
/// - When "Auto-Layout Network" is triggered from the menu
/// - After AI edit operations (when auto_layout_after_edit is enabled)
///
/// Note: Incremental positioning of new nodes during editing is handled
/// separately by the auto_layout module, not through this enum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutAlgorithm {
    /// Simple layered layout based on topological depth. Fast and reliable.
    /// Organizes nodes into columns by their depth in the dependency graph.
    TopologicalGrid,

    /// Sophisticated layered layout with crossing minimization.
    /// Uses the Sugiyama algorithm for better visual quality on complex graphs.
    #[default]
    Sugiyama,
}

/// Compute the topological depth of each node in the network.
///
/// Each node is assigned a "depth" based on its position in the dependency graph:
/// - depth(node) = 0 if node has no input connections
/// - depth(node) = max(depth(inputs)) + 1 otherwise
///
/// This ensures:
/// - Source nodes (primitives, literals) are at depth 0
/// - Each node appears to the right of all its dependencies
/// - The final output/return node has the highest depth
///
/// # Arguments
/// * `network` - The node network to analyze
///
/// # Returns
/// A HashMap from node ID to its computed depth
pub fn compute_node_depths(network: &NodeNetwork) -> HashMap<u64, usize> {
    let all: HashSet<u64> = network.nodes.keys().copied().collect();
    compute_node_depths_within(network, &all)
}

/// [`compute_node_depths`] restricted to a subgraph.
///
/// Only the nodes in `ids` get a depth, and only wires **between** them count:
/// a wire from outside the set is not an input at all, so a node fed only from
/// outside is a source at depth 0. That is what makes a block of freshly added
/// nodes lay out as its own little drawing rather than inheriting the column
/// index its producer happens to sit in
/// (`doc/design_incremental_layout.md`, Step 3).
///
/// [`compute_node_depths`] is this function over every id, so the two cannot
/// drift.
pub fn compute_node_depths_within(
    network: &NodeNetwork,
    ids: &HashSet<u64>,
) -> HashMap<u64, usize> {
    let mut depths: HashMap<u64, usize> = HashMap::new();
    let mut visiting: HashSet<u64> = HashSet::new();

    fn visit(
        node_id: u64,
        network: &NodeNetwork,
        ids: &HashSet<u64>,
        depths: &mut HashMap<u64, usize>,
        visiting: &mut HashSet<u64>,
    ) -> usize {
        // Return cached depth if already computed
        if let Some(&depth) = depths.get(&node_id) {
            return depth;
        }

        // Cycle detection - if we're already visiting this node, treat as source
        if visiting.contains(&node_id) {
            return 0;
        }
        visiting.insert(node_id);

        let node = match network.nodes.get(&node_id) {
            Some(n) => n,
            None => return 0,
        };

        // Find all input node IDs, inside the subgraph only. A cross-scope
        // wire is skipped whatever id it carries: `$element` names the owning
        // HOF in the *parent* scope, and ids are unique per network, so a body
        // node can share that number without being connected to anything.
        let input_ids: Vec<u64> = node
            .arguments
            .iter()
            .flat_map(|arg| arg.incoming_wires.iter())
            .filter(|wire| wire.source_scope_depth == 0)
            .map(|wire| wire.source_node_id)
            .filter(|source_id| ids.contains(source_id))
            .collect();

        // If no inputs, this is a source node at depth 0
        if input_ids.is_empty() {
            visiting.remove(&node_id);
            depths.insert(node_id, 0);
            return 0;
        }

        // Compute max depth of all inputs
        let max_input_depth = input_ids
            .iter()
            .map(|&source_id| visit(source_id, network, ids, depths, visiting))
            .max()
            .unwrap_or(0);

        let depth = max_input_depth + 1;

        visiting.remove(&node_id);
        depths.insert(node_id, depth);
        depth
    }

    // Ascending id, never `HashMap` order (D7): the depths themselves do not
    // depend on the visit order, but the recursion's cycle break does.
    let mut seeds: Vec<u64> = ids.iter().copied().collect();
    seeds.sort_unstable();
    for node_id in seeds {
        if network.nodes.contains_key(&node_id) {
            visit(node_id, network, ids, &mut depths, &mut visiting);
        }
    }

    depths
}

/// Get all node IDs that feed into the given node (direct inputs).
///
/// # Arguments
/// * `network` - The node network
/// * `node_id` - The node to find inputs for
///
/// # Returns
/// A HashSet of node IDs that are direct inputs to the specified node
pub fn get_input_node_ids(network: &NodeNetwork, node_id: u64) -> HashSet<u64> {
    network
        .nodes
        .get(&node_id)
        .map(|node| {
            node.arguments
                .iter()
                .flat_map(|arg| arg.incoming_wires.iter().map(|w| w.source_node_id))
                .collect()
        })
        .unwrap_or_default()
}

/// Get all node IDs that consume the output of the given node (direct outputs).
///
/// # Arguments
/// * `network` - The node network
/// * `node_id` - The node to find outputs for
///
/// # Returns
/// A HashSet of node IDs that receive output from the specified node
pub fn get_output_node_ids(network: &NodeNetwork, node_id: u64) -> HashSet<u64> {
    let mut outputs = HashSet::new();

    for (&other_id, other_node) in &network.nodes {
        if other_id == node_id {
            continue;
        }

        for argument in &other_node.arguments {
            if argument.has_source(node_id) {
                outputs.insert(other_id);
                break;
            }
        }
    }

    outputs
}

/// Find all source nodes (nodes with no input connections).
///
/// Source nodes typically include:
/// - Primitive value nodes (int, float, vec3, etc.)
/// - Literal nodes
/// - Parameter nodes
///
/// # Arguments
/// * `network` - The node network to analyze
///
/// # Returns
/// A HashSet of node IDs for all source nodes
pub fn find_source_nodes(network: &NodeNetwork) -> HashSet<u64> {
    network
        .nodes
        .iter()
        .filter_map(|(&node_id, node)| {
            let has_inputs = node.arguments.iter().any(|arg| !arg.is_empty());
            if has_inputs { None } else { Some(node_id) }
        })
        .collect()
}

/// Find all sink nodes (nodes with no output connections).
///
/// Sink nodes are typically the final output nodes of a network,
/// such as return nodes or display nodes.
///
/// # Arguments
/// * `network` - The node network to analyze
///
/// # Returns
/// A HashSet of node IDs for all sink nodes
pub fn find_sink_nodes(network: &NodeNetwork) -> HashSet<u64> {
    // Build set of all nodes that are referenced as inputs
    let mut referenced_nodes: HashSet<u64> = HashSet::new();

    for node in network.nodes.values() {
        for argument in &node.arguments {
            for wire in &argument.incoming_wires {
                referenced_nodes.insert(wire.source_node_id);
            }
        }
    }

    // Sink nodes are those not referenced by any other node
    network
        .nodes
        .keys()
        .filter(|&node_id| !referenced_nodes.contains(node_id))
        .copied()
        .collect()
}

// =============================================================================
// Comment placement (`doc/design_wire_annotations.md`, D8 / D9 / D10)
// =============================================================================
//
// Comment nodes never take part in layer assignment (D8): an anchored one is
// placed by rule *after* the graph is laid out, against the target its anchor
// resolves to, and an unanchored one has no rule at all — its position is the
// thing being preserved. Both cases go through `place_comments` below.
//
// This whole section is deliberately *disposable* scaffolding (D10). A separate
// design will rework auto-layout around preserving human intent for all nodes;
// the constants here are cosmetic, and no placement is ever written to the file.

/// Shared layout origin: every algorithm canonicalizes the graph to start here.
pub const START_X: f64 = 100.0;
/// See [`START_X`].
pub const START_Y: f64 = 100.0;
/// Horizontal pitch between columns: `NODE_WIDTH` (160) + a 50 px gap.
///
/// Still the pitch the topological-grid layout uses. Sugiyama sizes each
/// column by its widest member instead — an expanded HOF is 460 px wide, so a
/// fixed 210 px pitch drew it straight through the next column — and spends
/// [`COLUMN_GAP`] between them.
pub const COLUMN_WIDTH: f64 = 210.0;
/// Whitespace between two Sugiyama columns, i.e. [`COLUMN_WIDTH`] minus the
/// base node width. Keeps a network of uniform 160 px nodes laid out exactly
/// as it was before per-layer widths landed.
pub const COLUMN_GAP: f64 = COLUMN_WIDTH - node_layout::NODE_WIDTH;
/// Vertical gap between two nodes stacked in the same column.
pub const VERTICAL_GAP: f64 = 30.0;

/// Gap between a comment and the box it is anchored to (tier 1).
const ANCHOR_GAP: f64 = 24.0;
/// How far outside the graph's bounding box a gutter-placed comment sits.
const GUTTER_OFFSET: f64 = 40.0;
/// Increment used when sliding a comment along a gutter to find a clear spot.
const SLIDE_STEP: f64 = 40.0;
/// Cap on that slide, so a pathological network cannot loop forever.
const MAX_SLIDE_STEPS: usize = 400;
/// Clearance kept between a placed comment and every obstacle. Deliberately
/// below [`ANCHOR_GAP`] so a tier-1 candidate placed at exactly that gap is not
/// rejected by its own gap.
///
/// Visible to the crate for the same reason [`surrounding_candidates`] is: Step
/// 7 of the incremental pass tests those same candidates for collisions and
/// would reject every one of them if it used the layout's ordinary 30 px
/// vertical clearance — the candidate sits at `ANCHOR_GAP`, which is less.
pub(crate) const COMMENT_CLEARANCE: f64 = 16.0;

/// An axis-aligned box on the canvas: top-left corner plus size.
///
/// Visible to the rest of the crate because Step 7 of the incremental pass
/// places a *newly created* comment with the same two helpers this pass uses
/// ([`anchor_placement_box`] and [`surrounding_candidates`]) — one rule for
/// "beside its anchor", not two that drift.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutRect {
    pub(crate) pos: DVec2,
    pub(crate) size: DVec2,
}

impl LayoutRect {
    pub(crate) fn center(&self) -> DVec2 {
        self.pos + self.size * 0.5
    }

    pub(crate) fn max(&self) -> DVec2 {
        self.pos + self.size
    }
}

/// Whether `node` is a comment node, and therefore excluded from the graph
/// layout entirely (D8).
pub fn is_comment(node: &Node) -> bool {
    comment_data(node).is_some()
}

/// The ids of the nodes that take part in the graph layout — every node in
/// `network` except the comments (D8).
pub fn graph_node_ids(network: &NodeNetwork) -> HashSet<u64> {
    network
        .nodes
        .iter()
        .filter(|(_, node)| !is_comment(node))
        .map(|(&id, _)| id)
        .collect()
}

/// The rendered height of a node, as both layout algorithms size their columns.
///
/// A thin projection of [`crate::layout::size::rendered_node_size`], which is
/// the one authority on how big a node is (design doc, "Prerequisite: one size
/// function"). It therefore sees a comment's real dimensions *and* an expanded
/// HOF's body region, where the old local estimate saw neither.
pub fn node_height(node_id: u64, network: &NodeNetwork, registry: &NodeTypeRegistry) -> f64 {
    node_size(node_id, network, registry).y
}

/// The rendered width of a node. See [`node_height`].
pub fn node_width(node_id: u64, network: &NodeNetwork, registry: &NodeTypeRegistry) -> f64 {
    node_size(node_id, network, registry).x
}

/// The box a node occupies for collision purposes.
pub fn node_size(node_id: u64, network: &NodeNetwork, registry: &NodeTypeRegistry) -> DVec2 {
    rendered_node_size_by_id(network, registry, node_id)
}

/// The union of `rects`, or `None` if there are none.
fn bounding_box(rects: impl Iterator<Item = LayoutRect>) -> Option<LayoutRect> {
    let mut min = DVec2::splat(f64::MAX);
    let mut max = DVec2::splat(f64::MIN);
    let mut any = false;
    for rect in rects {
        any = true;
        min = min.min(rect.pos);
        max = max.max(rect.max());
    }
    any.then(|| LayoutRect {
        pos: min,
        size: max - min,
    })
}

/// Whether `candidate` collides with any obstacle, keeping [`COMMENT_CLEARANCE`].
fn collides(candidate: LayoutRect, obstacles: &[LayoutRect]) -> bool {
    obstacles.iter().any(|obstacle| {
        node_layout::nodes_overlap(
            candidate.pos,
            candidate.size,
            obstacle.pos,
            obstacle.size,
            COMMENT_CLEARANCE,
        )
    })
}

/// Place every comment in `network` (D8/D9), extending `positions` — which on
/// entry holds the laid-out **graph** nodes and nothing else.
///
/// Anchored comments go first (by node id), then unanchored ones (by node id).
/// An anchored position carries information — it has to end up near its
/// anchor — while an unanchored note's position is only a soft preference, so
/// it yields. Each comment joins the obstacle set once placed, so later ones
/// avoid it. The node-id tiebreak is what makes the result deterministic;
/// iteration order over `network.nodes` is not.
///
/// Comments never displace graph nodes: moving one would undo the layout just
/// computed and break column alignment.
pub fn place_comments(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    positions: &mut HashMap<u64, DVec2>,
) {
    let mut comment_ids: Vec<u64> = network
        .nodes
        .iter()
        .filter(|(_, node)| is_comment(node))
        .map(|(&id, _)| id)
        .collect();
    if comment_ids.is_empty() {
        return;
    }
    comment_ids.sort_unstable();

    // The obstacle set starts as every laid-out graph node, and the graph's
    // bounding box — the thing the gutters are the edges of — is computed over
    // those alone. A comment must never influence either.
    let mut obstacles: Vec<LayoutRect> = positions
        .iter()
        .map(|(&id, &pos)| LayoutRect {
            pos,
            size: node_size(id, network, registry),
        })
        .collect();

    let after = bounding_box(obstacles.iter().copied());
    let before = bounding_box(positions.keys().filter_map(|&id| {
        network.nodes.get(&id).map(|node| LayoutRect {
            pos: node.position,
            size: node_size(id, network, registry),
        })
    }));

    // D9.1: unanchored comments translate by the graph's bounding-box origin
    // delta, which keeps each note's offset relative to the drawing's top-left.
    // With no graph at all there is no delta to speak of, so the comments are
    // canonicalized to the layout origin the way a graph would have been.
    let origin = DVec2::new(START_X, START_Y);
    let (graph_bbox, delta) = match (after, before) {
        (Some(after), Some(before)) => (after, after.pos - before.pos),
        _ => {
            let comments_before = bounding_box(comment_ids.iter().map(|&id| LayoutRect {
                pos: network.nodes[&id].position,
                size: node_size(id, network, registry),
            }));
            let comments_origin = comments_before.map(|b| b.pos).unwrap_or(origin);
            (
                LayoutRect {
                    pos: origin,
                    size: DVec2::ZERO,
                },
                origin - comments_origin,
            )
        }
    };

    let (anchored, unanchored): (Vec<u64>, Vec<u64>) = comment_ids
        .into_iter()
        .partition(|&id| has_anchors(network, id));

    for comment_id in anchored.into_iter().chain(unanchored) {
        let size = node_size(comment_id, network, registry);
        let target = anchor_placement_box(network, registry, positions, comment_id);
        let translated = network.nodes[&comment_id].position + delta;

        // Tier 1 — the natural spot, accepted if it is collision-free.
        let candidates = match target {
            Some(target) => surrounding_candidates(target, size),
            None => vec![translated],
        };
        let natural = candidates
            .iter()
            .copied()
            .find(|&pos| !collides(LayoutRect { pos, size }, &obstacles));

        // Tier 2 — the gutter, a fallback that always exists.
        let pos = natural.unwrap_or_else(|| {
            let from = target
                .map(|target| target.center())
                .unwrap_or_else(|| translated + size * 0.5);
            gutter_position(from, size, graph_bbox, &obstacles)
        });

        positions.insert(comment_id, pos);
        obstacles.push(LayoutRect { pos, size });
    }
}

/// Whether the comment `comment_id` carries any anchor at all.
///
/// Ordering keys off the stored list rather than off resolvability so it stays
/// cheap and stable; an anchor that turns out to be unresolvable simply falls
/// through to the unanchored placement rule.
fn has_anchors(network: &NodeNetwork, comment_id: u64) -> bool {
    network
        .nodes
        .get(&comment_id)
        .and_then(comment_data)
        .is_some_and(|comment| !comment.anchors.is_empty())
}

/// The box a comment is placed against: its **first resolvable** anchor's
/// target, per `doc/design_wire_annotations.md`. A node anchor resolves to that
/// node's laid-out box; a wire anchor to the wire's midpoint as a zero-size box.
///
/// `None` when the comment has no anchors, or none of them designates something
/// this pass has a position for — in which case the comment falls through to
/// the unanchored rule.
///
/// `positions` is the placed set: only a node with an entry there can be an
/// anchor target. The full reflow passes the positions it has just computed;
/// the incremental pass passes the drawing as it stands (Step 7) or as it stood
/// before the edit (Step 8), which is what lets a drift be *measured* rather
/// than guessed at.
pub(crate) fn anchor_placement_box(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    positions: &HashMap<u64, DVec2>,
    comment_id: u64,
) -> Option<LayoutRect> {
    let comment = network.nodes.get(&comment_id).and_then(comment_data)?;
    for anchor in &comment.anchors {
        let Some(resolved) = anchor.resolve(network) else {
            continue;
        };
        match resolved {
            ResolvedAnchor::Node(target_id) => {
                if let Some(&pos) = positions.get(&target_id) {
                    return Some(LayoutRect {
                        pos,
                        size: node_size(target_id, network, registry),
                    });
                }
            }
            ResolvedAnchor::Wire(wire) => {
                if let Some(pos) = wire_midpoint(positions, &wire) {
                    return Some(LayoutRect {
                        pos,
                        size: DVec2::ZERO,
                    });
                }
            }
        }
    }
    None
}

/// The midpoint of a laid-out wire.
///
/// A wire is drawn as a cubic Bezier whose control points are offset from their
/// own endpoint along x only, by the same amount in opposite directions, so the
/// point at `t = 0.5` reduces exactly to the endpoints' mean — the same point
/// the painter uses for a leader line's target end. Keeping the two in step is
/// one decision, not two (design doc, open question 1).
///
/// A source in an ancestor scope (a capture) or on a zone-input pin has no
/// position in this network; the destination pin alone then stands in for the
/// wire, which is where it visibly enters this scope.
fn wire_midpoint(positions: &HashMap<u64, DVec2>, wire: &Wire) -> Option<DVec2> {
    let destination = *positions.get(&wire.destination_node_id)?;
    let destination_pin =
        node_layout::input_pin_position(destination, wire.destination_argument_index);

    let source_pin = match wire.source_pin {
        SourcePin::NodeOutput { pin_index } if wire.source_scope_depth == 0 => positions
            .get(&wire.source_node_id)
            .map(|&pos| node_layout::output_pin_position(pos, pin_index.max(0) as usize)),
        _ => None,
    };

    Some(match source_pin {
        Some(source_pin) => (source_pin + destination_pin) * 0.5,
        None => destination_pin,
    })
}

/// The four tier-1 candidate positions around a placement box, at
/// [`ANCHOR_GAP`], tried above, below, right, left in that order.
///
/// "Above" puts the comment's bottom edge `ANCHOR_GAP` above the box's top
/// edge, horizontally centred on it; the others are the obvious rotations. For
/// a wire anchor the box is a point, so the four candidates simply surround the
/// wire's midpoint.
pub(crate) fn surrounding_candidates(target: LayoutRect, size: DVec2) -> Vec<DVec2> {
    let center = target.center();
    let max = target.max();
    vec![
        DVec2::new(center.x - size.x / 2.0, target.pos.y - ANCHOR_GAP - size.y),
        DVec2::new(center.x - size.x / 2.0, max.y + ANCHOR_GAP),
        DVec2::new(max.x + ANCHOR_GAP, center.y - size.y / 2.0),
        DVec2::new(target.pos.x - ANCHOR_GAP - size.x, center.y - size.y / 2.0),
    ]
}

/// Which side of the graph's bounding box a gutter runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gutter {
    Top,
    Bottom,
    Left,
    Right,
}

/// Tier 2: place the comment just outside the graph's bounding box.
///
/// The gutter preserves the placement box's centre `x` (top / bottom) or centre
/// `y` (left / right), then slides **along** the gutter axis until clear. Top
/// and bottom are preferred and tried first: a column layout is wide and only
/// moderately tall, so those bands run the full graph width — ample capacity —
/// and their tethers are short vertical lines rather than long diagonals
/// crossing the graph. Left and right are short, and are the last resort.
///
/// There is deliberately no "least overlap" fallback: a bounded ring search
/// around the anchor exhausts itself in a dense graph and ends up placing the
/// comment *on top of* a node, the exact outcome this pass exists to prevent.
fn gutter_position(
    from: DVec2,
    size: DVec2,
    graph_bbox: LayoutRect,
    obstacles: &[LayoutRect],
) -> DVec2 {
    // Horizontal gutters first, each pair ordered by which edge `from` is nearer.
    let top_first = (from.y - graph_bbox.pos.y).abs() <= (graph_bbox.max().y - from.y).abs();
    let left_first = (from.x - graph_bbox.pos.x).abs() <= (graph_bbox.max().x - from.x).abs();
    let order = [
        if top_first {
            Gutter::Top
        } else {
            Gutter::Bottom
        },
        if top_first {
            Gutter::Bottom
        } else {
            Gutter::Top
        },
        if left_first {
            Gutter::Left
        } else {
            Gutter::Right
        },
        if left_first {
            Gutter::Right
        } else {
            Gutter::Left
        },
    ];

    let mut fallback = None;
    for gutter in order {
        let base = match gutter {
            Gutter::Top => DVec2::new(
                from.x - size.x / 2.0,
                graph_bbox.pos.y - GUTTER_OFFSET - size.y,
            ),
            Gutter::Bottom => DVec2::new(from.x - size.x / 2.0, graph_bbox.max().y + GUTTER_OFFSET),
            Gutter::Left => DVec2::new(
                graph_bbox.pos.x - GUTTER_OFFSET - size.x,
                from.y - size.y / 2.0,
            ),
            Gutter::Right => DVec2::new(graph_bbox.max().x + GUTTER_OFFSET, from.y - size.y / 2.0),
        };
        fallback.get_or_insert(base);

        let axis = match gutter {
            Gutter::Top | Gutter::Bottom => DVec2::X,
            Gutter::Left | Gutter::Right => DVec2::Y,
        };
        if let Some(pos) = slide_until_clear(base, axis, size, obstacles) {
            return pos;
        }
    }

    // Unreachable in practice — the slide is capped, a gutter's capacity is not.
    fallback.unwrap_or(from)
}

/// Slide `base` along `axis` in alternating directions, in [`SLIDE_STEP`]
/// increments, until the box clears every obstacle.
fn slide_until_clear(
    base: DVec2,
    axis: DVec2,
    size: DVec2,
    obstacles: &[LayoutRect],
) -> Option<DVec2> {
    for step in 0..MAX_SLIDE_STEPS {
        // 0, +1, -1, +2, -2, … so the comment stays as close as it can to the
        // point it was projected from.
        let magnitude = step.div_ceil(2) as f64 * SLIDE_STEP;
        let offset = if step % 2 == 0 { magnitude } else { -magnitude };
        let pos = base + axis * offset;
        if !collides(LayoutRect { pos, size }, obstacles) {
            return Some(pos);
        }
    }
    None
}
