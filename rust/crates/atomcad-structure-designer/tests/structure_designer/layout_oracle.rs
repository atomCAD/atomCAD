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
//! The document lists eight. Six are implemented here; two need machinery that
//! does not exist yet and are called out at their check sites rather than
//! silently omitted:
//!
//! | # | Invariant | Status |
//! |---|---|---|
//! | 1 | No new overlap | live |
//! | 2 | No wire flipped | live |
//! | 3 | Untouched means untouched | live |
//! | 4 | Rigid shifts | Phase 2 (needs `shift_half_plane`'s moved set) |
//! | 5 | Order among the pushed | Phase 2 (needs the cascade's moved set) |
//! | 6 | Bodies | live |
//! | 7 | Determinism | live, via [`assert_deterministic`] |
//! | 8 | Cascade bound | Phase 4 (needs a per-cascade move count) |

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

/// Assert every live invariant of a layout pass.
///
/// Panics with the offending node's path on the first violation. Pre-existing
/// violations are tolerated throughout: the rule the whole design follows is
/// *repair only what this edit broke*, and the corpora contain overlapping
/// nodes and backward wires their authors are content with.
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
