//! Arrange operations over a chosen set of nodes
//! (`doc/design_deliberate_node_placement.md`, Part B2 and A2).
//!
//! One implementation of "align these", "stack these", "straighten these
//! wires" and "lay these out", shared by the GUI's Arrange menu and, from Phase
//! 5, by the AI's `arrange` / `straighten` statements and `[…] column` / `row`
//! groups. The words are the same in both interfaces, so the code is too.
//!
//! [`arrange`] is **pure**: it returns target positions the way
//! [`compute_layout`](super::compute_layout) and
//! [`layout_subgraph`](super::layout_subgraph) do and moves nothing. Only the
//! listed nodes appear in the result, so "only listed nodes move" holds by
//! construction. The caller commits: the GUI through one `MoveNodesCommand`
//! (nothing is pushed, D11), the AI path through placement (which does push).

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use super::common::{Gap, LayoutAlgorithm, compute_node_depths_within};
use super::layout_subgraph;
use super::motion::Rect;
use crate::node_network::{NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;

/// Vertical offset from a node's top edge to the centre of its first pin row.
/// Flutter's `NODE_VERT_WIRE_OFFSET` (`node_network.dart`).
const PIN_ROW_OFFSET: f64 = 33.0;
/// Height of one pin row. Flutter's `NODE_VERT_WIRE_OFFSET_PER_PARAM`.
const PIN_ROW_HEIGHT: f64 = 22.0;
/// Vertical offset of a pin in the title bar: the function pin, and a
/// `closure`'s single output. Flutter's `NODE_VERT_WIRE_OFFSET_FUNCTION_PIN`.
const TITLE_BAR_PIN_OFFSET: f64 = 16.0;

/// One arrange operation. Which nodes it acts on is the `ids` argument of
/// [`arrange`]; this says what is done to them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArrangeOp {
    /// Left edges on the leftmost left edge.
    AlignLeft,
    /// Right edges on the rightmost right edge.
    AlignRight,
    /// Top edges on the topmost top edge.
    AlignTop,
    /// Bottom edges on the bottommost bottom edge.
    AlignBottom,
    /// The nodes **in the order given**, one under the other, lefts on the
    /// leftmost left edge, the first one's top on the topmost top edge, `Gap`
    /// apart.
    StackColumn(Gap),
    /// The nodes **in the order given**, side by side, tops on the topmost top
    /// edge, the first one's left on the leftmost left edge, `Gap` apart.
    StackRow(Gap),
    /// The top and bottom nodes stay, and the gaps between consecutive nodes
    /// (ordered by centre y) are made equal. Only y changes.
    DistributeVertically,
    /// The leftmost and rightmost nodes stay, and the gaps between consecutive
    /// nodes (ordered by centre x) are made equal. Only x changes.
    DistributeHorizontally,
    /// Each node, in topological order among the listed ones, moves vertically
    /// until the wire from its first wired input runs horizontal. Only y
    /// changes; a node with no such wire stays.
    Straighten,
    /// Sugiyama over the listed nodes only ([`layout_subgraph`]), the result
    /// translated so the nodes' bounding box keeps its top-left corner.
    Layout,
}

/// Target positions for `ids` under `op`.
///
/// `sizes` is the measured scope ([`measure_scope`](super::measure_scope)); a
/// listed node absent from it or from `scope` is ignored. The result holds a
/// position for every listed node the operation places — including ones that
/// end up where they already are — and nothing else. The order of `ids`
/// matters only to the two stack operations; every other operation orders the
/// nodes itself.
pub fn arrange(
    scope: &NodeNetwork,
    registry: &NodeTypeRegistry,
    sizes: &HashMap<u64, DVec2>,
    ids: &[u64],
    op: ArrangeOp,
) -> HashMap<u64, DVec2> {
    // De-duplicated in the order given, placed nodes only.
    let mut seen = HashSet::new();
    let members: Vec<(u64, Rect)> = ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .filter_map(|id| {
            let size = *sizes.get(&id)?;
            let node = scope.nodes.get(&id)?;
            Some((id, Rect::new(node.position, size)))
        })
        .collect();
    if members.is_empty() {
        return HashMap::new();
    }

    let left = members
        .iter()
        .map(|(_, r)| r.left())
        .fold(f64::MAX, f64::min);
    let top = members
        .iter()
        .map(|(_, r)| r.top())
        .fold(f64::MAX, f64::min);
    let right = members
        .iter()
        .map(|(_, r)| r.right())
        .fold(f64::MIN, f64::max);
    let bottom = members
        .iter()
        .map(|(_, r)| r.bottom())
        .fold(f64::MIN, f64::max);

    match op {
        ArrangeOp::AlignLeft => each(&members, |r| DVec2::new(left, r.top())),
        ArrangeOp::AlignRight => each(&members, |r| DVec2::new(right - r.size.x, r.top())),
        ArrangeOp::AlignTop => each(&members, |r| DVec2::new(r.left(), top)),
        ArrangeOp::AlignBottom => each(&members, |r| DVec2::new(r.left(), bottom - r.size.y)),
        ArrangeOp::StackColumn(gap) => {
            let mut y = top;
            members
                .iter()
                .map(|&(id, r)| {
                    let pos = DVec2::new(left, y);
                    y += r.size.y + gap.down();
                    (id, pos)
                })
                .collect()
        }
        ArrangeOp::StackRow(gap) => {
            let mut x = left;
            members
                .iter()
                .map(|&(id, r)| {
                    let pos = DVec2::new(x, top);
                    x += r.size.x + gap.across();
                    (id, pos)
                })
                .collect()
        }
        ArrangeOp::DistributeVertically => distribute(&members, Axis::Y),
        ArrangeOp::DistributeHorizontally => distribute(&members, Axis::X),
        ArrangeOp::Straighten => straighten(scope, &members),
        ArrangeOp::Layout => layout(scope, registry, &members),
    }
}

fn each(members: &[(u64, Rect)], place: impl Fn(&Rect) -> DVec2) -> HashMap<u64, DVec2> {
    members.iter().map(|(id, r)| (*id, place(r))).collect()
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn of(self, v: DVec2) -> f64 {
        match self {
            Axis::X => v.x,
            Axis::Y => v.y,
        }
    }

    fn with(self, v: DVec2, value: f64) -> DVec2 {
        match self {
            Axis::X => DVec2::new(value, v.y),
            Axis::Y => DVec2::new(v.x, value),
        }
    }
}

/// Equal gaps along `axis`, the first and last node (by centre, then id) kept.
///
/// The gap is the free span between the first node's near edge and the last
/// node's far edge, minus every member's extent, shared equally. It can come
/// out negative when the members do not fit; they then overlap evenly rather
/// than the command refusing.
fn distribute(members: &[(u64, Rect)], axis: Axis) -> HashMap<u64, DVec2> {
    let mut ordered: Vec<(u64, Rect)> = members.to_vec();
    ordered.sort_by(|(a_id, a), (b_id, b)| {
        axis.of(a.center())
            .total_cmp(&axis.of(b.center()))
            .then(a_id.cmp(b_id))
    });
    if ordered.len() < 3 {
        return each(&ordered, |r| r.pos);
    }

    let (_, first) = ordered[0];
    let (_, last) = ordered[ordered.len() - 1];
    let start = axis.of(first.pos);
    let end = axis.of(last.pos + last.size);
    let extents: f64 = ordered.iter().map(|(_, r)| axis.of(r.size)).sum();
    let gap = (end - start - extents) / (ordered.len() - 1) as f64;

    let mut at = start;
    ordered
        .iter()
        .map(|&(id, r)| {
            let pos = axis.with(r.pos, at);
            at += axis.of(r.size) + gap;
            (id, pos)
        })
        .collect()
}

/// [`ArrangeOp::Straighten`].
///
/// Topological order among the listed nodes (depth within the set, then top,
/// then id) is what lets a chain straighten in one pass: a node's source, if
/// listed, has already moved when the node is aligned to it. The source may be
/// any node of the scope; an unlisted one stays where it is.
///
/// "First wired input" is the lowest-indexed input pin carrying a wire from a
/// node output in this same scope. Captures and zone-input references have no
/// box in this scope to align with, so they are skipped.
fn straighten(scope: &NodeNetwork, members: &[(u64, Rect)]) -> HashMap<u64, DVec2> {
    let ids: HashSet<u64> = members.iter().map(|(id, _)| *id).collect();
    let depths = compute_node_depths_within(scope, &ids);
    let mut ordered: Vec<(u64, Rect)> = members.to_vec();
    ordered.sort_by(|(a_id, a), (b_id, b)| {
        depths
            .get(a_id)
            .cmp(&depths.get(b_id))
            .then(a.top().total_cmp(&b.top()))
            .then(a_id.cmp(b_id))
    });

    let mut placed: HashMap<u64, DVec2> = HashMap::new();
    for (id, rect) in ordered {
        let Some(node) = scope.nodes.get(&id) else {
            continue;
        };
        let source = node
            .arguments
            .iter()
            .enumerate()
            .find_map(|(arg_index, argument)| {
                argument.incoming_wires.iter().find_map(|wire| {
                    let SourcePin::NodeOutput { pin_index } = wire.source_pin else {
                        return None;
                    };
                    (wire.source_scope_depth == 0
                        && wire.source_node_id != id
                        && scope.nodes.contains_key(&wire.source_node_id))
                    .then_some((wire.source_node_id, pin_index, arg_index))
                })
            });

        let position = match source {
            Some((source_id, pin_index, arg_index)) => {
                let source_top = placed
                    .get(&source_id)
                    .map(|p| p.y)
                    .unwrap_or(scope.nodes[&source_id].position.y);
                let source_pin_y = source_top + output_pin_offset(scope, source_id, pin_index);
                DVec2::new(rect.left(), source_pin_y - input_pin_offset(arg_index))
            }
            None => rect.pos,
        };
        placed.insert(id, position);
    }
    placed
}

/// A node's input pin `index`, measured from its top edge. Mirrors
/// `ScopeResolver._pinPositionNormal`'s `externalInput` arm.
pub(crate) fn input_pin_offset(index: usize) -> f64 {
    PIN_ROW_OFFSET + (index as f64 + 0.5) * PIN_ROW_HEIGHT
}

/// A node's output pin `pin_index` (`-1` = the function pin), measured from
/// its top edge. Mirrors the `functionPin` / `externalOutput` arms: the
/// function pin and a `closure`'s single output sit in the title bar.
///
/// `node_layout::output_pin_position` is not used here: it puts every pin one
/// half row higher than Flutter does, which cancels between two ordinary pins
/// but not against a title-bar pin.
pub(crate) fn output_pin_offset(scope: &NodeNetwork, node_id: u64, pin_index: i32) -> f64 {
    let in_title_bar = pin_index < 0
        || scope
            .nodes
            .get(&node_id)
            .is_some_and(|node| node.zone.is_some() && node.node_type_name == "closure");
    if in_title_bar {
        TITLE_BAR_PIN_OFFSET
    } else {
        PIN_ROW_OFFSET + (pin_index as f64 + 0.5) * PIN_ROW_HEIGHT
    }
}

/// [`ArrangeOp::Layout`]: Sugiyama on the subset, translated so the laid-out
/// nodes' bounding box keeps its top-left corner.
///
/// Comments take no part in the layering (layout/AGENTS.md, rule 1), so
/// [`layout_subgraph`] returns no position for a listed comment and it stays.
fn layout(
    scope: &NodeNetwork,
    registry: &NodeTypeRegistry,
    members: &[(u64, Rect)],
) -> HashMap<u64, DVec2> {
    let ids: HashSet<u64> = members.iter().map(|(id, _)| *id).collect();
    let positions = layout_subgraph(scope, registry, &ids, LayoutAlgorithm::Sugiyama);
    let Some(min) = positions.values().copied().reduce(|a, b| a.min(b)) else {
        return HashMap::new();
    };
    // Only the laid-out nodes' own box counts: a listed comment keeps its
    // place, so it must not pull the translation either.
    let Some(before_min) = members
        .iter()
        .filter(|(id, _)| positions.contains_key(id))
        .map(|(_, r)| r.pos)
        .reduce(|a, b| a.min(b))
    else {
        return HashMap::new();
    };
    let offset = before_min - min;
    positions
        .into_iter()
        .map(|(id, pos)| (id, pos + offset))
        .collect()
}
