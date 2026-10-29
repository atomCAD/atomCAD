//! The layout report: what an edit's layout pass did, and why.
//!
//! `doc/design_deliberate_node_placement.md` A3 ("Hear"). The incremental pass
//! already knew *what* moved — its before/after diff — and this module adds the
//! two things an AI needs to read that list: **why** each node moved, and what
//! the edit left broken that nothing repaired.
//!
//! # Causes are recorded at the call sites
//!
//! Neither motion primitive knows why it was called. The pass does, so it
//! records `(id, cause, why)` into a [`Causes`] from what each call *returns*
//! (`cascade`'s moved ids, `shift_half_plane`'s [`ShiftOutcome`], `grow_rect`'s
//! moves). `from` / `to` still come from the one before/after diff, so a
//! reported move and an actual move cannot disagree by construction; what can
//! go wrong is a diffed node that found no cause, which is why
//! [`LayoutMove::cause`] is an `Option` and the corpus run asserts it is never
//! `None`.
//!
//! [`ShiftOutcome`]: crate::layout::motion::ShiftOutcome

use std::collections::{HashMap, HashSet};

use glam::DVec2;
use serde_json::{Value, json};

use crate::layout::delta::WireKey;
use crate::layout::motion::{GAP, Rect, upstream_closure};
use crate::node_layout::nodes_overlap;
use crate::node_network::NodeNetwork;
use crate::text_format::{NamePath, PositionSnapshot, PushDir, unique_node_names};

/// Why an existing node moved.
///
/// The first three are asked for by the AI's layout statements (Step 2½); the
/// last two are the automatic pass's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveCause {
    /// A `place` statement positioned this existing node.
    Place,
    /// An `arrange` / `straighten` statement listed it.
    Arrange,
    /// A placed or arranged node landed on it, and it went this way.
    Push(PushDir),
    /// A node this edit enlarged pushed it (Step 2).
    Grow,
    /// The safety net: room for a node nobody placed, a backward-wire repair,
    /// or a comment kept beside its anchor.
    Auto,
}

impl MoveCause {
    /// The word the report and the history log use. A push's direction is
    /// reported separately ([`MoveCause::push_dir`]).
    pub fn as_str(self) -> &'static str {
        match self {
            MoveCause::Place => "place",
            MoveCause::Arrange => "arrange",
            MoveCause::Push(_) => "push",
            MoveCause::Grow => "grow",
            MoveCause::Auto => "auto",
        }
    }

    /// Which way a `push` went, `None` for every other cause.
    pub fn push_dir(self) -> Option<PushDir> {
        match self {
            MoveCause::Push(dir) => Some(dir),
            _ => None,
        }
    }
}

/// Every cause recorded for every node during one scope's pass, in order.
///
/// Recording a node that turns out not to have moved (a cascade that pushed it
/// and a later step that put it back) is harmless: only nodes in the
/// before/after diff are ever looked up.
#[derive(Debug, Default, Clone)]
pub struct Causes {
    by_node: HashMap<u64, Vec<(MoveCause, String)>>,
}

impl Causes {
    /// Record one cause for each of `ids`.
    pub fn record<I: IntoIterator<Item = u64>>(&mut self, ids: I, cause: MoveCause, why: &str) {
        for id in ids {
            self.by_node
                .entry(id)
                .or_default()
                .push((cause, why.to_string()));
        }
    }

    /// The causes of one node, in the order they happened.
    pub fn of(&self, id: u64) -> &[(MoveCause, String)] {
        self.by_node.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// One existing node whose position changed.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutMove {
    /// Scoped name path (`m1/d`).
    pub path: NamePath,
    pub from: DVec2,
    pub to: DVec2,
    /// The cause of its *first* move. `None` only if the pass moved a node
    /// without recording why — a bug the corpus run exists to catch.
    pub cause: Option<MoveCause>,
    /// A short reason naming the node or wire the first move served.
    pub why: String,
    /// The causes of later moves that differ from `cause`, in order.
    pub also: Vec<MoveCause>,
}

impl LayoutMove {
    /// Build the entry for one diffed node from everything recorded for it.
    pub fn new(path: NamePath, from: DVec2, to: DVec2, causes: &[(MoveCause, String)]) -> Self {
        let first = causes.first();
        let cause = first.map(|(cause, _)| *cause);
        let why = first.map(|(_, why)| why.clone()).unwrap_or_default();
        let mut also: Vec<MoveCause> = Vec::new();
        // `also` names *causes*, so two pushes in different directions are
        // one `push`.
        let same = |a: &MoveCause, b: &MoveCause| a.as_str() == b.as_str();
        for (later, _) in causes.iter().skip(1) {
            if !cause.is_some_and(|c| same(&c, later))
                && !also.last().is_some_and(|last| same(last, later))
            {
                also.push(*later);
            }
        }
        Self {
            path,
            from,
            to,
            cause,
            why,
            also,
        }
    }
}

/// A node this edit created, with the box it finally landed in.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBox {
    pub path: NamePath,
    pub rect: Rect,
}

/// What kind of problem a [`Finding`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// Two boxes overlap after the edit and did not before.
    Overlap,
    /// A wire this edit created runs backward and nothing repaired it.
    BackwardWire,
}

impl FindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingKind::Overlap => "overlap",
            FindingKind::BackwardWire => "backward_wire",
        }
    }
}

/// Something this edit **created** and nothing repaired. Pre-existing overlaps
/// and backward wires are the baseline and never reported.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub kind: FindingKind,
    /// The nodes involved: the overlapping pair, sorted, or a wire's source
    /// and destination.
    pub nodes: Vec<NamePath>,
    /// One line, saying what, where and how much.
    pub message: String,
}

/// A layout statement that parsed but could not be carried out (D8): dropped,
/// or — a negative body coordinate — applied after a clamp. `fallback` says
/// what happened instead.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutWarning {
    pub line: usize,
    pub statement: String,
    pub problem: String,
    pub fallback: String,
}

/// The `layout` section of an edit result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutReport {
    /// Every node a `place` statement positioned, new or existing, with its
    /// final box, sorted by path. An existing one is also in `moved`.
    pub placed: Vec<PlacedBox>,
    /// Every new node the incremental rules positioned, sorted by path.
    pub auto_placed: Vec<PlacedBox>,
    /// Every existing node whose position changed, sorted by path.
    pub moved: Vec<LayoutMove>,
    pub findings: Vec<Finding>,
    pub warnings: Vec<LayoutWarning>,
    /// How many layout statements the edit had. Not serialized: it only
    /// decides whether there is a report at all.
    pub statements: usize,
}

impl LayoutReport {
    /// Whether the edit result should carry a `layout` key at all: an existing
    /// node moved, a finding or warning arose, or the edit had layout
    /// statements. An edit that leaves the drawing alone says nothing about
    /// layout.
    pub fn is_worth_reporting(&self) -> bool {
        !self.moved.is_empty()
            || !self.findings.is_empty()
            || !self.warnings.is_empty()
            || self.statements > 0
    }

    /// Sort every list into its canonical order (by path), so two runs over the
    /// same edit produce identical JSON.
    pub fn sort(&mut self) {
        self.placed.sort_by(|a, b| a.path.cmp(&b.path));
        self.auto_placed.sort_by(|a, b| a.path.cmp(&b.path));
        self.moved.sort_by(|a, b| a.path.cmp(&b.path));
        self.findings.sort_by(|a, b| {
            a.kind
                .as_str()
                .cmp(b.kind.as_str())
                .then_with(|| a.nodes.cmp(&b.nodes))
        });
        self.warnings.sort_by_key(|warning| warning.line);
    }

    /// The JSON the edit result carries under `layout`. Coordinates are whole
    /// pixels, rounded; a key whose list is empty is omitted, except
    /// `findings` and `warnings`, which are always present.
    pub fn to_json(&self) -> Value {
        let mut out = serde_json::Map::new();
        let boxes = |list: &[PlacedBox]| {
            Value::Array(
                list.iter()
                    .map(|placed| {
                        json!({
                            "node": placed.path.join("/"),
                            "box": box_json(placed.rect),
                        })
                    })
                    .collect(),
            )
        };
        if !self.placed.is_empty() {
            out.insert("placed".into(), boxes(&self.placed));
        }
        if !self.auto_placed.is_empty() {
            out.insert("auto_placed".into(), boxes(&self.auto_placed));
        }
        if !self.moved.is_empty() {
            out.insert(
                "moved".into(),
                Value::Array(self.moved.iter().map(move_json).collect()),
            );
        }
        out.insert(
            "findings".into(),
            Value::Array(
                self.findings
                    .iter()
                    .map(|finding| {
                        json!({
                            "kind": finding.kind.as_str(),
                            "nodes": finding.nodes.iter().map(|p| p.join("/")).collect::<Vec<_>>(),
                            "message": finding.message,
                        })
                    })
                    .collect(),
            ),
        );
        out.insert(
            "warnings".into(),
            Value::Array(
                self.warnings
                    .iter()
                    .map(|warning| {
                        json!({
                            "line": warning.line,
                            "statement": warning.statement,
                            "problem": warning.problem,
                            "fallback": warning.fallback,
                        })
                    })
                    .collect(),
            ),
        );
        Value::Object(out)
    }
}

fn px(value: f64) -> i64 {
    value.round() as i64
}

fn point_json(point: DVec2) -> Value {
    json!([px(point.x), px(point.y)])
}

fn box_json(rect: Rect) -> Value {
    json!([
        px(rect.pos.x),
        px(rect.pos.y),
        px(rect.size.x),
        px(rect.size.y)
    ])
}

fn move_json(moved: &LayoutMove) -> Value {
    let mut entry = serde_json::Map::new();
    entry.insert("node".into(), json!(moved.path.join("/")));
    entry.insert("from".into(), point_json(moved.from));
    entry.insert("to".into(), point_json(moved.to));
    if let Some(cause) = moved.cause {
        entry.insert("cause".into(), json!(cause.as_str()));
        if let Some(dir) = cause.push_dir() {
            entry.insert("dir".into(), json!(dir.as_str()));
        }
    }
    entry.insert("why".into(), json!(moved.why));
    if !moved.also.is_empty() {
        entry.insert(
            "also".into(),
            Value::Array(moved.also.iter().map(|c| json!(c.as_str())).collect()),
        );
    }
    Value::Object(entry)
}

// ---------------------------------------------------------------------------
// Findings
// ---------------------------------------------------------------------------

/// The findings of one scope, measured once its pass has finished.
///
/// `sizes` is the scope measured *after* the pass; `scope_names` is the scope's
/// name path; `snapshot` is the pre-edit drawing; `added_wires` the scope's
/// new wires.
pub fn scope_findings(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    scope_names: &[String],
    snapshot: &PositionSnapshot,
    added_wires: &[WireKey],
) -> Vec<Finding> {
    let names = unique_node_names(scope);
    let path_of = |id: u64| -> Option<NamePath> {
        let mut path = scope_names.to_vec();
        path.push(names.get(&id)?.clone());
        Some(path)
    };
    let rect_of = |id: u64| -> Option<Rect> {
        Some(Rect::new(scope.nodes.get(&id)?.position, *sizes.get(&id)?))
    };

    let mut findings = Vec::new();
    findings.extend(new_overlaps(scope, sizes, snapshot, &path_of, &rect_of));
    findings.extend(backward_wires(
        scope,
        &names,
        added_wires,
        &path_of,
        &rect_of,
    ));
    findings
}

/// Overlapping pairs (zero clearance) that did not overlap before the edit.
///
/// A new overlap needs a node whose box changed — new, moved or resized — so
/// only pairs with such a node are tested.
fn new_overlaps(
    scope: &NodeNetwork,
    sizes: &HashMap<u64, DVec2>,
    snapshot: &PositionSnapshot,
    path_of: &dyn Fn(u64) -> Option<NamePath>,
    rect_of: &dyn Fn(u64) -> Option<Rect>,
) -> Vec<Finding> {
    let mut ids: Vec<u64> = scope.nodes.keys().copied().collect();
    ids.sort_unstable();

    struct Entry {
        id: u64,
        path: NamePath,
        now: Rect,
        was: Option<Rect>,
    }
    let entries: Vec<Entry> = ids
        .iter()
        .filter(|id| sizes.contains_key(id))
        .filter_map(|&id| {
            let path = path_of(id)?;
            let now = rect_of(id)?;
            let was = snapshot
                .get(&path)
                .map(|state| Rect::new(state.position, state.footprint));
            Some(Entry { id, path, now, was })
        })
        .collect();

    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut out = Vec::new();
    for a in entries.iter().filter(|e| e.was != Some(e.now)) {
        for b in &entries {
            if a.id == b.id {
                continue;
            }
            let key = (a.id.min(b.id), a.id.max(b.id));
            if seen.contains(&key) {
                continue;
            }
            if !overlaps(a.now, b.now) {
                continue;
            }
            seen.insert(key);
            let existed = match (a.was, b.was) {
                (Some(pa), Some(pb)) => overlaps(pa, pb),
                _ => false,
            };
            if existed {
                continue;
            }
            let w = a.now.right().min(b.now.right()) - a.now.left().max(b.now.left());
            let h = a.now.bottom().min(b.now.bottom()) - a.now.top().max(b.now.top());
            let mut pair = [a.path.clone(), b.path.clone()];
            pair.sort();
            out.push(Finding {
                kind: FindingKind::Overlap,
                message: format!(
                    "{} overlaps {} by {}×{} px",
                    pair[0].join("/"),
                    pair[1].join("/"),
                    px(w),
                    px(h)
                ),
                nodes: pair.to_vec(),
            });
        }
    }
    out
}

/// New same-scope wires that do not run forward after the pass.
///
/// "Forward" is the test Step 6 repairs against: the source's right edge plus
/// the gap at or left of the destination's left edge.
fn backward_wires(
    scope: &NodeNetwork,
    names: &HashMap<u64, String>,
    added_wires: &[WireKey],
    path_of: &dyn Fn(u64) -> Option<NamePath>,
    rect_of: &dyn Fn(u64) -> Option<Rect>,
) -> Vec<Finding> {
    let by_name: HashMap<&str, u64> = names
        .iter()
        .map(|(&id, name)| (name.as_str(), id))
        .collect();
    let mut out = Vec::new();
    for key in added_wires {
        if key.is_cross_scope() {
            continue;
        }
        let (Some(source_name), Some(destination_name)) =
            (key.source.path().last(), key.destination.last())
        else {
            continue;
        };
        let (Some(&source_id), Some(&destination_id)) = (
            by_name.get(source_name.as_str()),
            by_name.get(destination_name.as_str()),
        ) else {
            continue;
        };
        if source_id == destination_id {
            continue;
        }
        let (Some(source), Some(destination)) = (rect_of(source_id), rect_of(destination_id))
        else {
            continue;
        };
        let deficit = source.right() + GAP - destination.left();
        if deficit <= 0.0 {
            continue;
        }
        let (Some(source_path), Some(destination_path)) =
            (path_of(source_id), path_of(destination_id))
        else {
            continue;
        };
        let cycle = upstream_closure(scope, [source_id]).contains(&destination_id);
        out.push(Finding {
            kind: FindingKind::BackwardWire,
            message: format!(
                "wire {} -> {} runs backward: {} px short of forward{}",
                source_path.join("/"),
                destination_path.join("/"),
                px(deficit),
                if cycle {
                    " (a cycle: no shift can put both ends in order)"
                } else {
                    ""
                }
            ),
            nodes: vec![source_path, destination_path],
        });
    }
    out
}

fn overlaps(a: Rect, b: Rect) -> bool {
    nodes_overlap(a.pos, a.size, b.pos, b.size, 0.0)
}
