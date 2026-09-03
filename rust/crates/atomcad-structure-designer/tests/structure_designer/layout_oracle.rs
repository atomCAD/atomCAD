//! The layout oracle: the invariants every incremental-layout test asserts.
//!
//! `doc/design_incremental_layout.md` §Testing. The per-phase scenario lists in
//! that document are catalogues, not the safety net — *this* is the safety net.
//! A scenario test states only what is specific to it (which node landed where)
//! and calls [`check`] for everything else, so an algorithm change that breaks
//! a property nobody thought to assert still fails.
//!
//! Shared by every layout test and, from Phase 5, by the corpus run. The
//! `diff_test_support.rs` module beside this one is the precedent.
//!
//! # Which invariants are live
//!
//! All eight of the document's invariants are implemented here:
//!
//! | # | Invariant | Status |
//! |---|---|---|
//! | 1 | No new overlap | live |
//! | 2 | No wire flipped | live |
//! | 3 | Untouched means untouched | live |
//! | 4 | Rigid shifts | live, via [`check_rigid_shift`] |
//! | 5 | Order among the pushed | live, via [`check_pushed_order`] |
//! | 6 | Bodies | live |
//! | 7 | Determinism | live, via [`assert_deterministic`] |
//! | 8 | Cascade bound | live, via [`check_cascade_bound`] |
//!
//! Invariants 4, 5 and 8 are the ones that are *about a primitive* rather than
//! about a whole pass, so they take the primitive's own reported move set and a
//! plain `id -> rect` map instead of a [`Drawing`]. [`rects`] projects one scope
//! of a drawing into that shape for a caller that has a drawing instead.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use atomcad_structure_designer::layout::rendered_node_size;
use atomcad_structure_designer::node_layout::{
    DEFAULT_HORIZONTAL_GAP, HOF_BODY_BOTTOM_PADDING, nodes_overlap,
};
use atomcad_structure_designer::node_network::{Node, NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::NamePath;

/// One node's geometry, keyed the way everything in this design is keyed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub position: DVec2,
    pub size: DVec2,
}

impl Placed {
    fn right(&self) -> f64 {
        self.position.x + self.size.x
    }
}

/// A drawing reduced to what the oracle reasons about: every node's rect and
/// every wire, grouped by the scope they live in.
///
/// The scope key is the **name path of the owning HOF chain** (empty = the
/// top-level network), never an id chain: a `--replace` mints fresh ids, and
/// the whole point of the oracle is to compare a before and an after that may
/// share no ids at all.
#[derive(Debug, Default, Clone)]
pub struct Drawing {
    /// `scope name path -> (node name -> rect)`.
    pub scopes: HashMap<NamePath, HashMap<String, Placed>>,
    /// Same-scope wires as `(scope, source name, destination name)`. Wires whose
    /// ends are in different scopes are excluded: their coordinates are in
    /// different frames, so "forward" is not a statement about them.
    pub wires: HashSet<(NamePath, String, String)>,
}

impl Drawing {
    /// Measure `network` with `registry`.
    pub fn of(network: &NodeNetwork, registry: &NodeTypeRegistry) -> Self {
        let mut drawing = Drawing::default();
        collect(network, registry, &mut Vec::new(), &mut drawing);
        drawing
    }

    /// The rect of the node at `path`, if it exists.
    pub fn get(&self, path: &[String]) -> Option<Placed> {
        let (name, scope) = path.split_last()?;
        self.scopes.get(scope)?.get(name).copied()
    }

    /// Every node's full name path.
    pub fn paths(&self) -> HashSet<NamePath> {
        let mut out = HashSet::new();
        for (scope, nodes) in &self.scopes {
            for name in nodes.keys() {
                let mut path = scope.clone();
                path.push(name.clone());
                out.insert(path);
            }
        }
        out
    }
}

fn collect(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    scope: &mut NamePath,
    out: &mut Drawing,
) {
    let entry = out.scopes.entry(scope.clone()).or_default();
    let mut names: HashMap<u64, String> = HashMap::new();
    for node in network.nodes.values() {
        let Some(name) = node.custom_name.clone() else {
            continue;
        };
        names.insert(node.id, name.clone());
        entry.insert(
            name,
            Placed {
                position: node.position,
                size: rendered_node_size(node, registry),
            },
        );
    }

    for node in network.nodes.values() {
        let Some(destination) = node.custom_name.clone() else {
            continue;
        };
        for argument in node
            .arguments
            .iter()
            .chain(node.zone_output_arguments.iter())
        {
            for wire in &argument.incoming_wires {
                // Same scope only: depth 0 and an ordinary output pin.
                if wire.source_scope_depth != 0 {
                    continue;
                }
                let SourcePin::NodeOutput { .. } = wire.source_pin else {
                    continue;
                };
                let Some(source) = names.get(&wire.source_node_id) else {
                    continue;
                };
                out.wires
                    .insert((scope.clone(), source.clone(), destination.clone()));
            }
        }
    }

    let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &network.nodes[&id];
        let (Some(body), Some(name)) = (node.zone.as_deref(), node.custom_name.as_ref()) else {
            continue;
        };
        scope.push(name.clone());
        collect(body, registry, scope, out);
        scope.pop();
    }
}

/// What the caller claims the edit and the layout pass were allowed to touch.
pub struct Touched {
    /// Name paths the delta accounted for — added, grown or removed nodes.
    /// These may sit anywhere; nothing is asserted about where they landed.
    pub delta: HashSet<NamePath>,
    /// Name paths the layout pass reports having moved (its own `moved` list).
    /// Every other node must be bit-identical.
    pub moved: HashSet<NamePath>,
}

impl Touched {
    /// Nothing was allowed to move: the strictest form, used by the "an edit
    /// that changes a value moves nothing" tests.
    pub fn nothing() -> Self {
        Self {
            delta: HashSet::new(),
            moved: HashSet::new(),
        }
    }

    pub fn new<I, J>(delta: I, moved: J) -> Self
    where
        I: IntoIterator<Item = NamePath>,
        J: IntoIterator<Item = NamePath>,
    {
        Self {
            delta: delta.into_iter().collect(),
            moved: moved.into_iter().collect(),
        }
    }
}

/// Assert every whole-pass invariant of a layout pass.
///
/// Panics with the offending node's path on the first violation. Pre-existing
/// violations are tolerated throughout: the rule the whole design follows is
/// *repair only what this edit broke*, and the corpora contain overlapping
/// nodes and backward wires their authors are content with.
///
/// Invariants 4 and 5 are **not** included: they are statements about one
/// primitive call and need that call's own move set, so a test that made one
/// adds [`check_rigid_shift`] / [`check_pushed_order`] itself.
pub fn check(before: &Drawing, after: &Drawing, touched: &Touched) {
    check_no_new_overlap(before, after);
    check_no_wire_flipped(before, after);
    check_untouched_unmoved(before, after, touched);
    check_bodies(after);
}

/// **1 — No new overlap.** The set of overlapping node pairs after the edit is
/// a subset of the set before it, per scope.
pub fn check_no_new_overlap(before: &Drawing, after: &Drawing) {
    for (scope, nodes) in &after.scopes {
        let empty = HashMap::new();
        let was = before.scopes.get(scope).unwrap_or(&empty);
        let mut names: Vec<&String> = nodes.keys().collect();
        names.sort();
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                let (ra, rb) = (nodes[*a], nodes[*b]);
                if !overlaps(ra, rb) {
                    continue;
                }
                let existed = match (was.get(*a), was.get(*b)) {
                    (Some(pa), Some(pb)) => overlaps(*pa, *pb),
                    // A node that did not exist before cannot have a
                    // pre-existing overlap to inherit.
                    _ => false,
                };
                assert!(
                    existed,
                    "new overlap in scope {:?}: {} {:?} and {} {:?}",
                    scope, a, ra, b, rb
                );
            }
        }
    }
}

/// **2 — No wire flipped.** Every wire that pointed rightward before still
/// does. A wire that was already backward is left alone, and a wire this edit
/// created is not constrained here (Step 6 repairs those, Phase 4).
pub fn check_no_wire_flipped(before: &Drawing, after: &Drawing) {
    for key @ (scope, source, destination) in &before.wires {
        if !after.wires.contains(key) {
            continue; // the edit removed it
        }
        let (Some(sb), Some(db)) = (
            before.scopes.get(scope).and_then(|s| s.get(source)),
            before.scopes.get(scope).and_then(|s| s.get(destination)),
        ) else {
            continue;
        };
        if !is_forward(*sb, *db) {
            continue; // already backward before the edit
        }
        let (Some(sa), Some(da)) = (
            after.scopes.get(scope).and_then(|s| s.get(source)),
            after.scopes.get(scope).and_then(|s| s.get(destination)),
        ) else {
            continue;
        };
        assert!(
            is_forward(*sa, *da),
            "wire {} -> {} in scope {:?} flipped backward: {:?} -> {:?}",
            source,
            destination,
            scope,
            sa,
            da
        );
    }
}

/// **3 — Untouched means untouched.** Every node that is neither in the delta
/// nor in the reported move list is at its exact pre-edit position, bit for bit.
///
/// This is the invariant the whole design exists to buy, and the only one that
/// is an equality rather than an inequality: "close enough" would let a pass
/// nudge a drawing a pixel at a time across a session of edits.
pub fn check_untouched_unmoved(before: &Drawing, after: &Drawing, touched: &Touched) {
    for path in before.paths() {
        if touched.delta.contains(&path) || touched.moved.contains(&path) {
            continue;
        }
        let (Some(was), Some(now)) = (before.get(&path), after.get(&path)) else {
            continue; // gone: a removal the caller declared, or a rename
        };
        assert_eq!(
            was.position,
            now.position,
            "node {} moved but was in neither the delta nor the move list",
            path.join("/")
        );
    }
}

/// One scope of a drawing as the plain `id`-free `name -> rect` map the
/// primitive-level invariants take. Present so a scenario test that already
/// built a [`Drawing`] does not have to assemble one by hand; a test that drove
/// a primitive directly usually has ids and builds the map itself.
pub fn rects(drawing: &Drawing, scope: &[String]) -> HashMap<String, Placed> {
    drawing.scopes.get(scope).cloned().unwrap_or_default()
}

/// **4 — Rigid shifts.** Among the nodes a `shift_half_plane` reports having
/// moved, every pairwise offset is unchanged — on *both* axes, since a rigid
/// translation preserves the whole arrangement, not just the spacing along its
/// own axis. Nodes the call was told to hold `fixed` did not move at all.
///
/// This is what buys the design's D3: relative order, alignment, spacing and
/// whitespace inside the translated set survive exactly, so a shift can never
/// quietly re-tidy a region it was only supposed to push.
///
/// Keys may be anything hashable that names a node — an id when a test drove a
/// primitive directly, a name when it came from a [`Drawing`].
pub fn check_rigid_shift<K: std::hash::Hash + Eq + std::fmt::Debug>(
    before: &HashMap<K, Placed>,
    after: &HashMap<K, Placed>,
    shifted: &HashSet<K>,
    fixed: &HashSet<K>,
) {
    for key in fixed {
        let (Some(was), Some(now)) = (before.get(key), after.get(key)) else {
            continue;
        };
        assert_eq!(
            was.position, now.position,
            "fixed node {key:?} moved during a half-plane shift"
        );
    }

    let mut members: Vec<&K> = shifted.iter().collect();
    members.sort_by_key(|key| format!("{key:?}"));
    for (i, a) in members.iter().enumerate() {
        for b in &members[i + 1..] {
            let (Some(a0), Some(b0), Some(a1), Some(b1)) =
                (before.get(*a), before.get(*b), after.get(*a), after.get(*b))
            else {
                continue;
            };
            assert_eq!(
                a1.position - b1.position,
                a0.position - b0.position,
                "the shift was not rigid: {a:?} and {b:?} changed their offset"
            );
        }
    }
}

/// **5 — Order among the pushed.** For every pair of nodes a cascade moved
/// whose x-intervals overlap, the vertical order after is the vertical order
/// before.
///
/// The cascade is allowed to move a node a long way; it is not allowed to
/// reorder a column. Nodes with disjoint x-intervals are unconstrained — they
/// are in different columns and their relative height was never a statement.
pub fn check_pushed_order<K: std::hash::Hash + Eq + std::fmt::Debug>(
    before: &HashMap<K, Placed>,
    after: &HashMap<K, Placed>,
    pushed: &HashSet<K>,
) {
    let mut members: Vec<&K> = pushed.iter().collect();
    members.sort_by_key(|key| format!("{key:?}"));
    for (i, a) in members.iter().enumerate() {
        for b in &members[i + 1..] {
            let (Some(a0), Some(b0), Some(a1), Some(b1)) =
                (before.get(*a), before.get(*b), after.get(*a), after.get(*b))
            else {
                continue;
            };
            // Different columns: nothing to preserve.
            if a0.right() <= b0.position.x || b0.right() <= a0.position.x {
                continue;
            }
            let was = a0.position.y.partial_cmp(&b0.position.y);
            let now = a1.position.y.partial_cmp(&b1.position.y);
            assert_eq!(
                was, now,
                "the cascade reordered {a:?} and {b:?}: {a0:?}/{b0:?} became {a1:?}/{b1:?}"
            );
        }
    }
}

/// **6 — Bodies.** No body node has a negative coordinate, and every HOF's
/// rendered footprint contains its body's content plus the padding.
///
/// Body coordinates are non-negative because a body's extent is measured from
/// its own origin (`rendered_body_size`, Flutter's `_computeBodySize`): content
/// placed at a negative coordinate is simply not measured, and renders outside
/// the box.
pub fn check_bodies(after: &Drawing) {
    for (scope, nodes) in &after.scopes {
        if scope.is_empty() {
            continue;
        }
        for (name, rect) in nodes {
            let mut path = scope.clone();
            path.push(name.clone());
            assert!(
                rect.position.x >= 0.0 && rect.position.y >= 0.0,
                "body node {} has a negative coordinate: {:?}",
                path.join("/"),
                rect.position
            );
        }

        // The owning HOF's footprint must cover the content it now holds.
        let content = nodes
            .values()
            .fold(DVec2::ZERO, |acc, r| acc.max(r.position + r.size));
        let Some(owner) = after.get(scope) else {
            continue;
        };
        assert!(
            owner.size.x >= content.x + HOF_BODY_BOTTOM_PADDING
                && owner.size.y >= content.y + HOF_BODY_BOTTOM_PADDING,
            "HOF {} is {:?} but its body content reaches {:?}",
            scope.join("/"),
            owner.size,
            content
        );
    }
}

/// The bound invariant 8 holds a single cascade to.
///
/// The design imposes no *cap* — the cascade is proved to terminate, and
/// capping it would leave an overlap standing — so this is a test-side guard,
/// deliberately generous: the corpus simulation measured a worst case of 11
/// nodes over two hand-drawn files, and 50% of drops push nothing at all.
/// Blowing through 20 means the propagation rule regressed, not that some
/// drawing was unusually dense.
pub const CASCADE_MOVE_BOUND: usize = 20;

/// **8 — Bound.** No single cascade moved more than [`CASCADE_MOVE_BOUND`]
/// nodes.
///
/// Takes one `cascade` call's own reported move list, in the family of
/// invariants 4 and 5.
pub fn check_cascade_bound(moved: &[u64]) {
    assert!(
        moved.len() <= CASCADE_MOVE_BOUND,
        "one cascade moved {} nodes, over the guard of {CASCADE_MOVE_BOUND}: {moved:?}",
        moved.len()
    );
}

/// **7 — Determinism.** Run `produce` twice from the same input and assert the
/// two drawings are bit-identical.
///
/// The failure this catches is not a wrong layout but a *randomly* wrong one:
/// every iteration over nodes must be by ascending id and over scopes by depth
/// then name path (D7), and a single stray `HashMap` iteration reintroduces
/// per-process randomness that no scenario assertion would ever notice.
pub fn assert_deterministic(mut produce: impl FnMut() -> Drawing) {
    let first = produce();
    let second = produce();
    let mut paths: Vec<NamePath> = first.paths().into_iter().collect();
    paths.sort();
    let mut other: Vec<NamePath> = second.paths().into_iter().collect();
    other.sort();
    assert_eq!(paths, other, "two runs produced different node sets");
    for path in paths {
        assert_eq!(
            first.get(&path),
            second.get(&path),
            "two runs placed {} differently",
            path.join("/")
        );
    }
}

fn overlaps(a: Placed, b: Placed) -> bool {
    nodes_overlap(a.position, a.size, b.position, b.size, 0.0)
}

/// A wire is *forward* when the source's right edge, plus the gap, is at or
/// left of the destination's left edge — the same test Step 6 repairs against.
fn is_forward(source: Placed, destination: Placed) -> bool {
    source.right() + DEFAULT_HORIZONTAL_GAP <= destination.position.x
}

/// Every node in `network`, keyed by name path — a convenience for scenario
/// tests that want to name one node rather than walk a drawing.
pub fn node_paths(network: &NodeNetwork) -> HashMap<NamePath, DVec2> {
    fn walk(network: &NodeNetwork, prefix: &mut NamePath, out: &mut HashMap<NamePath, DVec2>) {
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node: &Node = &network.nodes[&id];
            let Some(name) = node.custom_name.as_ref() else {
                continue;
            };
            prefix.push(name.clone());
            out.insert(prefix.clone(), node.position);
            if let Some(body) = node.zone.as_deref() {
                walk(body, prefix, out);
            }
            prefix.pop();
        }
    }
    let mut out = HashMap::new();
    walk(network, &mut Vec::new(), &mut out);
    out
}
