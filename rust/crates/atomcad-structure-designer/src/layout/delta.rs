//! The edit delta: what one AI text edit did to one scope.
//!
//! `doc/design_incremental_layout.md` D1 — the unit of work is a diff. The
//! incremental layout pass never looks at a script; it looks at an
//! [`EditDelta`] per scope, computed by comparing the pre-edit identity
//! snapshot (`text_format::snapshot_node_positions`) against the post-edit
//! network.
//!
//! Two rules govern every line here.
//!
//! **Diffing is by name path, never by id.** `--replace` clears the network and
//! mints a fresh id for every node, so `[m1_id]` before and after are different
//! numbers naming the same body. Ids are resolved *after* the match. Name
//! matching is total: every node gets a `custom_name` at creation and the
//! `.cnnd` loader assigns one to every legacy node that lacks it, so a file
//! whose nodes carry no name on disk is fully named in memory. A renamed node
//! simply fails to match and is reported `added`, which is always safe.
//!
//! **A layout event is a footprint change, not a value change** (D13). A node
//! whose radius went from 5 to 50 is not a layout event; a node that gained a
//! pin, grew a body, or flipped out of the collapsed rendering is. That one
//! comparison subsumes every per-trigger detector, including the Auto-mode HOF
//! that re-expands when its `f:` wire is removed. A node that *shrank* is
//! excluded: deletion leaves holes and nothing ever compacts (D4).

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use crate::layout::size::rendered_node_size;
use crate::node_network::{ArgumentKind, Node, NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;
use crate::text_format::{NamePath, PositionSnapshot, unique_node_names};

/// One end of a wire, addressed the way the delta addresses everything: by the
/// name path of the node from the root of the network.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WireEnd {
    /// A node's output pin. `pin_index` is `-1` for the legacy function pin,
    /// `0` for the primary output, `>= 1` for the extra outputs.
    NodeOutput { path: NamePath, pin_index: i32 },
    /// A zone-input (inside-left) pin on the HOF owning some enclosing body —
    /// a `$element` reference. `path` names that HOF.
    ZoneInput { path: NamePath, pin_index: usize },
}

impl WireEnd {
    /// The node this end sits on, whichever kind of pin it is.
    pub fn path(&self) -> &NamePath {
        match self {
            WireEnd::NodeOutput { path, .. } | WireEnd::ZoneInput { path, .. } => path,
        }
    }
}

/// Which argument list on the destination node a wire terminates at, and where
/// in it. The same `(kind, index)` slot `WireAnchor` (#427) uses, so a comment
/// anchored to a wire and a delta entry for that wire agree on what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WireSlot {
    pub kind: ArgumentKind,
    pub index: usize,
}

/// A wire identified by the name paths of its endpoints.
///
/// Never by node id: `--replace` mints fresh ids, and an id-keyed diff would
/// report every wire of the network as `added` and hand the whole drawing to
/// the backward-wire repair pass.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WireKey {
    pub source: WireEnd,
    /// Name path of the node the wire terminates at.
    pub destination: NamePath,
    pub slot: WireSlot,
}

impl WireKey {
    /// Whether the wire's two ends live in different scopes — a capture
    /// (`^name`), a zone input (`$element`), or a body's `output`.
    ///
    /// Cross-scope wires appear in the delta because they are the dominant
    /// wires of a small body and serve as placement anchors, but they are never
    /// *repaired*: the two ends are in different coordinate frames, so
    /// "source's right edge is left of destination's x" is not a statement
    /// about anything.
    pub fn is_cross_scope(&self) -> bool {
        match &self.source {
            WireEnd::ZoneInput { .. } => true,
            // Same scope iff the two nodes share a parent path.
            WireEnd::NodeOutput { path, .. } => {
                path.len() != self.destination.len()
                    || path[..path.len().saturating_sub(1)]
                        != self.destination[..self.destination.len().saturating_sub(1)]
            }
        }
    }
}

/// What one edit did to one scope.
///
/// Produced by [`diff_scope`], consumed (from Phase 3 on) by the eight-step
/// incremental layout pass. Every vector is sorted, so two runs over the same
/// input produce byte-identical deltas (D7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditDelta {
    /// Nodes in this scope that did not exist before the edit, ascending id.
    ///
    /// They already sit at the throwaway positions
    /// `auto_layout::calculate_new_node_position` gave them; the layout pass
    /// treats them as invisible until it places their block.
    pub added: Vec<u64>,
    /// `(id, old footprint, new footprint)` for every node present in both
    /// whose rendered footprint grew in either axis (D13), ascending id.
    pub grown: Vec<(u64, DVec2, DVec2)>,
    /// Name paths of nodes that were in this scope before the edit and are
    /// gone now, sorted. Ids cannot be reported — the nodes no longer exist —
    /// and nothing needs them: deletion leaves a hole (D4). Their only layout
    /// use is skipping a comment whose anchor was removed.
    pub removed: Vec<NamePath>,
    /// Wires that terminate in this scope and did not exist before, sorted.
    pub added_wires: Vec<WireKey>,
    /// Wires that terminated in this scope and are gone now, sorted.
    ///
    /// **Layout-inert**: removing a wire cannot create an overlap or a backward
    /// wire, and a node left with no wires stays put. Carried for faithfulness
    /// (and for the edit log's counters). The one indirect effect — an
    /// Auto-mode HOF re-expanding when its `f` wire goes — is a footprint
    /// change and lands in [`grown`](Self::grown) instead.
    pub removed_wires: Vec<WireKey>,
}

impl EditDelta {
    /// True when the edit did nothing this scope's layout can see: no node
    /// appeared, grew or vanished, and no wire changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.grown.is_empty()
            && self.removed.is_empty()
            && self.added_wires.is_empty()
            && self.removed_wires.is_empty()
    }
}

/// The delta of a whole edit, summed over every scope it touched.
///
/// One AI text edit is one transaction over an arbitrary number of scopes (a
/// `m1/x = ...` statement and a root-scope one in the same script), so the edit
/// log's per-edit counters are a sum rather than any one scope's
/// [`EditDelta`]. Kept here rather than in `ai_edit_log` so the layout pass
/// never has to know what a log entry is; `ai_text_edit` converts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeltaTotals {
    pub nodes_added: usize,
    /// Nodes whose *footprint* grew (D13) - the delta's `grown`, named the way
    /// the log words it.
    pub nodes_modified: usize,
    pub nodes_removed: usize,
    pub wires_added: usize,
    pub wires_removed: usize,
}

impl DeltaTotals {
    /// Fold one scope's delta in.
    pub fn add(&mut self, delta: &EditDelta) {
        self.nodes_added += delta.added.len();
        self.nodes_modified += delta.grown.len();
        self.nodes_removed += delta.removed.len();
        self.wires_added += delta.added_wires.len();
        self.wires_removed += delta.removed_wires.len();
    }
}

/// Compare one scope of `network` against the pre-edit snapshot.
///
/// The "before" side is two halves taken together before the edit ran:
/// `snapshot`, the D14 identity map (`text_format::snapshot_node_positions`),
/// and `before_wires`, the whole-network wire set ([`collect_all_wires`]).
/// Wires are not in the identity map because the map is per node, and a wire
/// belongs to neither of its ends.
///
/// `scope_path` is the chain of zone-owning node ids from the root down to the
/// body being diffed (empty = the top-level network); `scope_names` is the same
/// chain spelled by name, which is what the snapshot is keyed by. Both are
/// needed: ids address the live network, names address the snapshot, and after
/// a `--replace` they are unrelated.
///
/// Returns `None` if `scope_path` does not resolve — a body whose owner was
/// deleted by this very edit, which has no layout left to repair.
pub fn diff_scope(
    root: &NodeNetwork,
    registry: &NodeTypeRegistry,
    snapshot: &PositionSnapshot,
    before_wires: &HashSet<WireKey>,
    scope_path: &[u64],
    scope_names: &[String],
) -> Option<EditDelta> {
    let scope = resolve_scope(root, scope_path)?;

    let mut delta = EditDelta::default();

    // --- nodes ------------------------------------------------------------
    let mut ids: Vec<u64> = scope.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &scope.nodes[&id];
        let Some(path) = node_path(scope_names, scope, node) else {
            // An unnamed node cannot be matched against the snapshot at all.
            // Name assignment is total in memory, so this is unreachable in
            // practice; treating it as new is the safe reading.
            delta.added.push(id);
            continue;
        };
        match snapshot.get(&path) {
            None => delta.added.push(id),
            Some(before) => {
                let after = rendered_node_size(node, registry);
                if after.x > before.footprint.x || after.y > before.footprint.y {
                    delta.grown.push((id, before.footprint, after));
                }
            }
        }
    }

    // --- removals ---------------------------------------------------------
    //
    // Everything the snapshot holds *directly* in this scope (one name deeper
    // than the scope's own path) that the post-edit scope no longer has.
    let live: HashSet<String> = unique_node_names(scope).into_values().collect();
    for path in snapshot.keys() {
        if path.len() != scope_names.len() + 1 || path[..scope_names.len()] != *scope_names {
            continue;
        }
        if !live.contains(path[scope_names.len()].as_str()) {
            delta.removed.push(path.clone());
        }
    }
    delta.removed.sort();

    // --- wires ------------------------------------------------------------
    //
    // A wire belongs to the scope its *destination* is in, which is the scope
    // whose drawing it constrains. Both sides are filtered the same way, so a
    // wire that moved between scopes reads as one removal and one addition.
    let after_wires = collect_wires(root, scope_path, scope_names);
    let terminates_here = |key: &WireKey| {
        key.destination.len() == scope_names.len() + 1
            && key.destination[..scope_names.len()] == *scope_names
    };

    for key in &after_wires {
        if !before_wires.contains(key) {
            delta.added_wires.push(key.clone());
        }
    }
    for key in before_wires.iter().filter(|key| terminates_here(key)) {
        if !after_wires.contains(key) {
            delta.removed_wires.push(key.clone());
        }
    }
    delta.added_wires.sort();
    delta.removed_wires.sort();

    Some(delta)
}

/// Every wire in `network`, at every depth, keyed by name path.
///
/// The "before" half of the wire diff, taken alongside the identity snapshot
/// and before the edit runs. Whole-network rather than per-scope because a
/// `WireKey` is already globally unique — its paths start at the root — so one
/// set serves every scope and [`diff_scope`] just filters it.
pub fn collect_all_wires(network: &NodeNetwork) -> HashSet<WireKey> {
    let mut out = HashSet::new();
    for (scope_path, scope_names) in scopes_inside_out(network) {
        out.extend(collect_wires(network, &scope_path, &scope_names));
    }
    out
}

/// The wires that terminate in one scope of `root`, keyed by name path.
///
/// Both argument lists count: `arguments` (ordinary input pins) and
/// `zone_output_arguments` (a body's `output`, which terminates on the HOF).
pub fn collect_wires(
    root: &NodeNetwork,
    scope_path: &[u64],
    scope_names: &[String],
) -> HashSet<WireKey> {
    let mut out = HashSet::new();
    let Some(scope) = resolve_scope(root, scope_path) else {
        return out;
    };

    for node in scope.nodes.values() {
        let Some(destination) = node_path(scope_names, scope, node) else {
            continue;
        };

        // The two argument lists differ in the frame their wires count from.
        // An ordinary input pin is reached from the node's own scope; a
        // zone-output pin is the *body's* return slot, so its wires count from
        // inside the body even though the argument list hangs off the HOF.
        let mut body_ids = scope_path.to_vec();
        body_ids.push(node.id);
        let mut body_names = scope_names.to_vec();
        body_names.push(destination.last().cloned().unwrap_or_default());

        let lists: [(ArgumentKind, &Vec<_>, &[u64], &[String]); 2] = [
            (
                ArgumentKind::External,
                &node.arguments,
                scope_path,
                scope_names,
            ),
            (
                ArgumentKind::ZoneOutput,
                &node.zone_output_arguments,
                &body_ids,
                &body_names,
            ),
        ];

        for (kind, arguments, frame_ids, frame_names) in lists {
            for (index, argument) in arguments.iter().enumerate() {
                for wire in &argument.incoming_wires {
                    // `source_scope_depth` counts frames *up* from `frame_ids`.
                    // For a `NodeOutput` it counts networks (`0` = this frame);
                    // for a `ZoneInput` it counts owning-HOF body frames (`1` =
                    // this frame's own owner), which is the same `+ 1`
                    // asymmetry `format_wire_source` and the evaluator encode.
                    let depth = wire.source_scope_depth as usize;
                    if depth > frame_ids.len() {
                        continue; // malformed; nothing sane to name it by
                    }
                    let outer = frame_ids.len() - depth;
                    let source = match wire.source_pin {
                        SourcePin::NodeOutput { pin_index } => {
                            let Some(source_scope) = resolve_scope(root, &frame_ids[..outer])
                            else {
                                continue;
                            };
                            let Some(source_node) = source_scope.nodes.get(&wire.source_node_id)
                            else {
                                continue;
                            };
                            let Some(path) =
                                node_path(&frame_names[..outer], source_scope, source_node)
                            else {
                                continue;
                            };
                            WireEnd::NodeOutput { path, pin_index }
                        }
                        SourcePin::ZoneInput { pin_index } => {
                            // `outer` indexes the HOF that owns the frame the
                            // wire reaches out to, and that HOF lives one scope
                            // further out again.
                            if outer >= frame_ids.len() {
                                continue;
                            }
                            let owner_id = frame_ids[outer];
                            let Some(owner_scope) = resolve_scope(root, &frame_ids[..outer]) else {
                                continue;
                            };
                            let Some(owner) = owner_scope.nodes.get(&owner_id) else {
                                continue;
                            };
                            let Some(path) = node_path(&frame_names[..outer], owner_scope, owner)
                            else {
                                continue;
                            };
                            WireEnd::ZoneInput { path, pin_index }
                        }
                    };
                    out.insert(WireKey {
                        source,
                        destination: destination.clone(),
                        slot: WireSlot { kind, index },
                    });
                }
            }
        }
    }
    out
}

/// Walk down to the network a scope path names.
fn resolve_scope<'n>(root: &'n NodeNetwork, scope_path: &[u64]) -> Option<&'n NodeNetwork> {
    let mut network = root;
    for owner_id in scope_path {
        network = network.nodes.get(owner_id)?.zone.as_deref()?;
    }
    Some(network)
}

/// `scope_names` extended by `node`'s own name — the name the text layer
/// writes it under (`unique_node_names`, so duplicates key apart), or `None`
/// if it has none. `scope` is the network the node lives in.
fn node_path(scope_names: &[String], scope: &NodeNetwork, node: &Node) -> Option<NamePath> {
    let name = unique_node_names(scope).remove(&node.id)?;
    let mut path = scope_names.to_vec();
    path.push(name);
    Some(path)
}

/// Every scope in `network`, **deepest first**, ties broken by name path (D12).
///
/// The incremental pass runs inside-out: a parent's node sizes are not known
/// until its children's bodies have settled, because an HOF's footprint is
/// `max(stored, body content + padding)`. Returned as `(scope path by id, scope
/// path by name)` pairs, the two addressings `diff_scope` needs.
pub fn scopes_inside_out(network: &NodeNetwork) -> Vec<(Vec<u64>, Vec<String>)> {
    fn walk(
        network: &NodeNetwork,
        ids: &mut Vec<u64>,
        names: &mut Vec<String>,
        out: &mut Vec<(Vec<u64>, Vec<String>)>,
    ) {
        out.push((ids.clone(), names.clone()));
        let mut child_ids: Vec<u64> = network.nodes.keys().copied().collect();
        child_ids.sort_unstable();
        let unique = unique_node_names(network);
        for id in child_ids {
            let node = &network.nodes[&id];
            let (Some(body), Some(name)) = (node.zone.as_deref(), unique.get(&id)) else {
                continue;
            };
            ids.push(id);
            names.push(name.clone());
            walk(body, ids, names, out);
            ids.pop();
            names.pop();
        }
    }

    let mut out = Vec::new();
    walk(network, &mut Vec::new(), &mut Vec::new(), &mut out);
    // Deepest first; equal depths in name-path order. `sort_by` is stable, and
    // the walk already emits a deterministic order, so this is total.
    out.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.1.cmp(&b.1)));
    out
}

/// `(name path) -> id` for every node in every scope of `network`.
///
/// The bridge between the two addressings: the snapshot and the delta speak
/// name paths, the network speaks ids.
pub fn node_ids_by_path(network: &NodeNetwork) -> HashMap<NamePath, u64> {
    fn walk(network: &NodeNetwork, prefix: &mut NamePath, out: &mut HashMap<NamePath, u64>) {
        let unique = unique_node_names(network);
        for node in network.nodes.values() {
            let Some(name) = unique.get(&node.id) else {
                continue;
            };
            prefix.push(name.clone());
            out.insert(prefix.clone(), node.id);
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
