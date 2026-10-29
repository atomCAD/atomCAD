//! Step 2½ of the incremental pass: the AI's layout statements.
//!
//! `doc/design_deliberate_node_placement.md` A2 ("Tell"). `place`, `arrange`
//! and `straighten` run here, in written order, between Step 2 (growth repair)
//! and Step 3 (the automatic placement of every new node nobody placed). This
//! module chooses *where*; it adds no way of *getting there*:
//!
//! - a `place` subject is put down and whatever it lands on is moved off it
//!   with the same two primitives the rest of the pass uses — [`cascade`] for
//!   no `push`, `push up` and `push down`, [`shift_half_plane`] with the
//!   subject's upstream closure held fixed for `push right`;
//! - `arrange` / `straighten` are [`arrange`](super::arrange::arrange), the
//!   code behind the GUI's Arrange menu, committed here with a push (D11);
//! - a `[…] column` / `row` group is that same function's stack op.
//!
//! # Pinning (D6)
//!
//! A node a statement places or arranges is **pinned** the moment that
//! statement finishes: later statements and every later cascade of the pass
//! hold it, and push something else instead. After the last statement every
//! node a statement *measured against* is pinned too (`all` pins nothing). The
//! pinned set is handed to [`cascade`] as its existing `fixed` argument and
//! never added to a half-plane shift's `fixed`, which must stay an upstream
//! closure: a later rightward shift carries a pinned node with its side.
//!
//! # Rejection (D8)
//!
//! A statement that parsed but cannot be carried out is dropped and reported
//! as a [`LayoutWarning`]; its subject keeps what would have happened without
//! it (a new node is placed automatically, an existing one stays). Nothing is
//! moved by a statement that is then rejected: every check runs before the
//! first position is written.

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use super::arrange::{ArrangeOp, arrange, input_pin_offset, output_pin_offset};
use super::common::{Gap, START_X, START_Y, VERTICAL_GAP};
use super::incremental::{BodyFrame, choose_cascade_dir};
use super::motion::{
    CascadeDir, GAP, Rect, cascade, newly_collides, shift_half_plane_holding, snap_shift_line,
    upstream_closure,
};
use super::report::{Causes, LayoutWarning, MoveCause};
use crate::node_network::{NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;
use crate::text_format::{
    GroupKind, LayoutClause, LayoutDirection, LayoutEdge, LayoutNodeRef, LayoutOp, LayoutStatement,
    LayoutSubject, LayoutTargets, NamePath, PlaceSpec, PositionSnapshot, PushDir,
    ScopedLayoutStatement, unique_node_names,
};

/// One layout statement, assigned to the scope it acts in.
///
/// `base` is the scope it was *written* in (the `body { … }` block holding
/// it); `scope` is the one its subject lives in, which differs for a
/// path-addressed statement (`place m1/x below m1/d` written at the top level
/// acts in `m1`). Every name in it resolves as `base` + its own prefix, and
/// must land in `scope`.
#[derive(Debug, Clone)]
pub struct ScopeStatement {
    pub statement: LayoutStatement,
    pub base: NamePath,
    pub scope: NamePath,
}

impl ScopeStatement {
    /// Assign a written statement to the scope its subject (or the first node
    /// an `arrange` / `straighten` lists) lives in.
    pub fn from_scoped(scoped: &ScopedLayoutStatement) -> Self {
        let first = match &scoped.statement.op {
            LayoutOp::Place { subject, .. } => subject.members().first().copied(),
            LayoutOp::Arrange(nodes) | LayoutOp::Straighten(nodes) => nodes.first(),
        };
        let mut scope = scoped.scope.clone();
        if let Some(first) = first {
            scope.extend(first.scope_path.iter().cloned());
        }
        Self {
            statement: scoped.statement.clone(),
            base: scoped.scope.clone(),
            scope,
        }
    }

    /// The `why` of every move this statement causes.
    pub fn why(&self) -> String {
        format!("line {}: {}", self.statement.line, self.statement.text)
    }

    /// A warning that drops (or, for a clamp, qualifies) this statement.
    pub fn warning(
        &self,
        problem: impl Into<String>,
        fallback: impl Into<String>,
    ) -> LayoutWarning {
        LayoutWarning {
            line: self.statement.line,
            statement: self.statement.text.clone(),
            problem: problem.into(),
            fallback: fallback.into(),
        }
    }
}

/// What Step 2½ leaves behind for the rest of the pass.
#[derive(Debug, Default, Clone)]
pub struct StatementOutcome {
    /// Held by every later cascade (D6): what the statements placed, moved or
    /// arranged, and — after the last one — what they measured against.
    pub pinned: HashSet<u64>,
    /// Every node a statement placed, moved or arranged. A new node in here
    /// is not laid out again by Steps 3–5, and Step 6 checks its wires.
    pub handled: HashSet<u64>,
    /// The subjects of `place` statements, for the report's `placed` list.
    pub placed: Vec<u64>,
    pub warnings: Vec<LayoutWarning>,
}

/// The read-only context a scope's statements are resolved against.
pub struct StatementContext<'a> {
    pub registry: &'a NodeTypeRegistry,
    /// The pre-edit drawing: where a node this edit deletes is measured.
    pub snapshot: &'a PositionSnapshot,
    /// This scope's name path.
    pub scope_names: &'a [String],
    /// The nodes this edit added to this scope.
    pub added: &'a HashSet<u64>,
    /// Every node's rendered size, added nodes included.
    pub measured: &'a HashMap<u64, DVec2>,
    pub frame: Option<&'a BodyFrame>,
}

/// A target, measured.
#[derive(Debug, Clone, Copy)]
enum Target {
    /// A node of this scope, at its current box.
    Live(Rect),
    /// A node this edit deletes, at its pre-edit box. Pins nothing.
    Deleted(Rect),
}

impl Target {
    fn rect(&self) -> Rect {
        match self {
            Target::Live(rect) | Target::Deleted(rect) => *rect,
        }
    }
}

/// Run one scope's statements, in written order.
///
/// `sizes` is the placed set (Step 1's, after Step 2); a new node joins it
/// when a statement places it.
pub fn run_statements(
    scope: &mut NodeNetwork,
    sizes: &mut HashMap<u64, DVec2>,
    statements: &[ScopeStatement],
    ctx: &StatementContext,
    causes: &mut Causes,
) -> StatementOutcome {
    let mut out = StatementOutcome::default();
    let names: HashMap<String, u64> = unique_node_names(scope)
        .into_iter()
        .map(|(id, name)| (name, id))
        .collect();
    let mut measured_against: HashSet<u64> = HashSet::new();

    for statement in statements {
        let mut run = Run {
            scope: &mut *scope,
            sizes: &mut *sizes,
            ctx,
            names: &names,
            statement,
            outcome: &mut out,
            causes: &mut *causes,
            targets: HashSet::new(),
        };
        match run.execute() {
            Ok(()) => measured_against.extend(run.targets),
            Err(problem) => {
                let fallback = fallback_for(statement, &names, ctx.added, &out.handled);
                out.warnings.push(statement.warning(problem, fallback));
            }
        }
    }

    // After the last statement, what they measured against is pinned too, so
    // no automatic step reverses a relation (D6).
    out.pinned.extend(measured_against);
    out
}

/// What happens to a dropped statement's subject instead: a new node is
/// placed automatically, an existing one stays, and one an earlier statement
/// already placed keeps that placement.
fn fallback_for(
    statement: &ScopeStatement,
    names: &HashMap<String, u64>,
    added: &HashSet<u64>,
    handled: &HashSet<u64>,
) -> String {
    let refs: Vec<&LayoutNodeRef> = match &statement.statement.op {
        LayoutOp::Place { subject, .. } => subject.members(),
        LayoutOp::Arrange(_) | LayoutOp::Straighten(_) => {
            return "the listed nodes stay where they are".to_string();
        }
    };
    let (mut new, mut existing, mut earlier) = (0, 0, 0);
    for node in &refs {
        match names.get(&node.name) {
            Some(id) if handled.contains(id) => earlier += 1,
            Some(id) if added.contains(id) => new += 1,
            _ => existing += 1,
        }
    }
    let alone = refs.len() == 1;
    let mut parts = Vec::new();
    if new > 0 {
        parts.push(if alone {
            "placed automatically"
        } else {
            "new nodes placed automatically"
        });
    }
    if existing > 0 {
        parts.push(if alone {
            "left where it was"
        } else {
            "existing ones left where they were"
        });
    }
    if earlier > 0 {
        parts.push(if alone {
            "keeps the earlier statement's placement"
        } else {
            "nodes an earlier statement placed keep that placement"
        });
    }
    parts.join("; ")
}

/// One statement's execution: everything it needs, borrowed for its turn.
struct Run<'r, 'c> {
    scope: &'r mut NodeNetwork,
    sizes: &'r mut HashMap<u64, DVec2>,
    ctx: &'r StatementContext<'c>,
    names: &'r HashMap<String, u64>,
    statement: &'r ScopeStatement,
    outcome: &'r mut StatementOutcome,
    causes: &'r mut Causes,
    /// The live nodes this statement measured against.
    targets: HashSet<u64>,
}

impl Run<'_, '_> {
    fn execute(&mut self) -> Result<(), String> {
        let push = self.statement.statement.push;
        match &self.statement.statement.op {
            LayoutOp::Place { subject, spec } => self.place(subject, spec, push),
            LayoutOp::Arrange(nodes) => self.arrange(nodes, ArrangeOp::Layout, push),
            LayoutOp::Straighten(nodes) => self.arrange(nodes, ArrangeOp::Straighten, push),
        }
    }

    // -----------------------------------------------------------------------
    // Resolution
    // -----------------------------------------------------------------------

    /// The scope a reference names, by the written scope plus its prefix.
    fn in_this_scope(&self, node: &LayoutNodeRef) -> bool {
        let mut full = self.statement.base.clone();
        full.extend(node.scope_path.iter().cloned());
        full == self.statement.scope
    }

    fn other_scope(node: &LayoutNodeRef) -> String {
        format!(
            "`{}` is in another scope: a statement measures only against nodes of the scope it places in",
            node.display()
        )
    }

    /// Whether the pre-edit drawing had `name` in this scope, and its box.
    fn deleted_box(&self, name: &str) -> Option<Rect> {
        let mut path: NamePath = self.ctx.scope_names.to_vec();
        path.push(name.to_string());
        self.ctx
            .snapshot
            .get(&path)
            .map(|state| Rect::new(state.position, state.footprint))
    }

    fn live_rect(&self, id: u64) -> Option<Rect> {
        let size = *self.sizes.get(&id)?;
        Some(Rect::new(self.scope.nodes.get(&id)?.position, size))
    }

    /// Resolve the subject's (or a list's) members to live ids, in written
    /// order, refusing what cannot be one.
    fn resolve_members(&self, refs: &[&LayoutNodeRef], by: bool) -> Result<Vec<u64>, String> {
        let mut ids = Vec::new();
        for node in refs {
            if !self.in_this_scope(node) {
                return Err(Self::other_scope(node));
            }
            let Some(&id) = self.names.get(&node.name) else {
                return Err(if self.deleted_box(&node.name).is_some() {
                    format!(
                        "`{}` is deleted by this edit: it can be measured against, but not placed, moved or arranged",
                        node.display()
                    )
                } else {
                    format!("no node named `{}`", node.display())
                });
            };
            if ids.contains(&id) {
                return Err(format!("`{}` is listed twice", node.display()));
            }
            if self.outcome.handled.contains(&id) {
                return Err(format!(
                    "`{}` was already placed, moved or arranged by an earlier statement",
                    node.display()
                ));
            }
            if by && self.ctx.added.contains(&id) {
                return Err(format!(
                    "`by` offsets a node from where it stands, and `{}` is new: it has no position yet",
                    node.display()
                ));
            }
            ids.push(id);
        }
        Ok(ids)
    }

    /// Measure one target. `subject` is excluded: a statement cannot measure
    /// against what it moves.
    fn resolve_target(&mut self, node: &LayoutNodeRef, subject: &[u64]) -> Result<Target, String> {
        if !self.in_this_scope(node) {
            return Err(Self::other_scope(node));
        }
        let Some(&id) = self.names.get(&node.name) else {
            return match self.deleted_box(&node.name) {
                Some(rect) => Ok(Target::Deleted(rect)),
                None => Err(format!("no node named `{}`", node.display())),
            };
        };
        if subject.contains(&id) {
            return Err(format!(
                "`{}` is the statement's own subject and cannot also be its target",
                node.display()
            ));
        }
        let Some(rect) = self.live_rect(id) else {
            return Err(format!(
                "`{}` is new and no earlier statement placed it, so it has no position yet (place it first, or reorder the lines)",
                node.display()
            ));
        };
        self.targets.insert(id);
        Ok(Target::Live(rect))
    }

    /// The bounding box a clause measures against.
    fn targets_box(&mut self, targets: &LayoutTargets, subject: &[u64]) -> Result<Rect, String> {
        match targets {
            LayoutTargets::All => Ok(self.extent(subject)),
            LayoutTargets::Nodes(nodes) => {
                let mut rects = Vec::new();
                for node in nodes {
                    rects.push(self.resolve_target(node, subject)?.rect());
                }
                Ok(bounding_box(&rects).expect("a target list is never empty"))
            }
        }
    }

    /// `all`: the extent of every placed node of the scope except the
    /// subject, as it stands now. An empty scope is a point at its origin.
    fn extent(&self, subject: &[u64]) -> Rect {
        let mut ids: Vec<u64> = self.sizes.keys().copied().collect();
        ids.sort_unstable();
        let rects: Vec<Rect> = ids
            .into_iter()
            .filter(|id| !subject.contains(id))
            .filter_map(|id| self.live_rect(id))
            .collect();
        bounding_box(&rects).unwrap_or_else(|| {
            let origin = if self.ctx.frame.is_some() {
                DVec2::ZERO
            } else {
                DVec2::new(START_X, START_Y)
            };
            Rect::new(origin, DVec2::ZERO)
        })
    }

    // -----------------------------------------------------------------------
    // place
    // -----------------------------------------------------------------------

    fn place(
        &mut self,
        subject: &LayoutSubject,
        spec: &PlaceSpec,
        push: Option<PushDir>,
    ) -> Result<(), String> {
        let by = matches!(spec, PlaceSpec::By(_));
        let ids = self.resolve_members(&subject.members(), by)?;

        // The subject's shape: block-local rects with the bounding box at the
        // origin, the way Step 3 builds a block.
        let local: Vec<(u64, Rect)> = match subject {
            LayoutSubject::Node(_) => {
                let size = self.size_of(ids[0]);
                vec![(ids[0], Rect::new(DVec2::ZERO, size))]
            }
            LayoutSubject::Group { kind, gap, .. } => self.stacked(&ids, *kind, *gap),
            // `by`: the members as they stand, relative to their box.
            LayoutSubject::List(_) => self.as_they_stand(&ids),
        };
        let local_rects: Vec<Rect> = local.iter().map(|(_, r)| *r).collect();
        let size = bounding_box(&local_rects).map_or(DVec2::ZERO, |b| b.size);

        let mut origin = match spec {
            PlaceSpec::At(point) => *point,
            PlaceSpec::By(offset) => {
                let now = self.as_they_stand_box(&ids);
                now.pos + *offset
            }
            PlaceSpec::Clauses(clauses) => self.resolve_clauses(clauses, &ids, &local, size)?,
        };

        // A body subject at a negative coordinate is applied, clamped.
        if self.ctx.frame.is_some() && (origin.x < 0.0 || origin.y < 0.0) {
            let clamped = origin.max(DVec2::ZERO);
            self.outcome.warnings.push(self.statement.warning(
                format!(
                    "resolves to ({}, {}), outside the body",
                    origin.x.round(),
                    origin.y.round()
                ),
                "clamped to 0 on that axis",
            ));
            origin = clamped;
        }

        // --- Commit: nothing above this line has moved anything. ----------
        let why = self.statement.why();
        // Where the existing members stood: what they already overlapped there
        // is the drawing's, not this statement's, to answer for.
        let old: HashMap<u64, Rect> = ids
            .iter()
            .filter_map(|&id| Some((id, self.live_rect(id)?)))
            .collect();
        for (id, rect) in &local {
            if let Some(node) = self.scope.nodes.get_mut(id) {
                node.position = origin + rect.pos;
            }
            self.sizes.insert(*id, rect.size);
        }
        let existing: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| !self.ctx.added.contains(id))
            .collect();
        self.causes.record(existing, MoveCause::Place, &why);

        // What it lands on: one box for a node or a group (they are compact),
        // each member's own for a list moved `by`, which may be spread out and
        // whose bounding box would sweep everything between its members.
        let landing: Vec<(Rect, Option<Rect>)> = match subject {
            LayoutSubject::List(_) => local
                .iter()
                .map(|(id, r)| (Rect::new(origin + r.pos, r.size), old.get(id).copied()))
                .collect(),
            _ => {
                let was: Vec<Rect> = ids.iter().filter_map(|id| old.get(id).copied()).collect();
                let was = (was.len() == ids.len())
                    .then(|| bounding_box(&was))
                    .flatten();
                vec![(Rect::new(origin, size), was)]
            }
        };
        self.make_room(&landing, &ids, push, &why);

        self.outcome.handled.extend(ids.iter().copied());
        self.outcome.pinned.extend(ids.iter().copied());
        self.outcome.placed.extend(ids.iter().copied());
        Ok(())
    }

    fn size_of(&self, id: u64) -> DVec2 {
        self.ctx.measured.get(&id).copied().unwrap_or(DVec2::ZERO)
    }

    /// `[…] column` / `row`: [`arrange`]'s stack op, normalized to the origin.
    fn stacked(&self, ids: &[u64], kind: GroupKind, gap: Gap) -> Vec<(u64, Rect)> {
        let op = match kind {
            GroupKind::Column => ArrangeOp::StackColumn(gap),
            GroupKind::Row => ArrangeOp::StackRow(gap),
        };
        let positions = arrange(self.scope, self.ctx.registry, self.ctx.measured, ids, op);
        let rects: Vec<(u64, Rect)> = ids
            .iter()
            .filter_map(|id| Some((*id, Rect::new(*positions.get(id)?, self.size_of(*id)))))
            .collect();
        normalize(rects)
    }

    fn as_they_stand(&self, ids: &[u64]) -> Vec<(u64, Rect)> {
        normalize(
            ids.iter()
                .filter_map(|&id| Some((id, self.live_rect(id)?)))
                .collect(),
        )
    }

    fn as_they_stand_box(&self, ids: &[u64]) -> Rect {
        let rects: Vec<Rect> = ids.iter().filter_map(|&id| self.live_rect(id)).collect();
        bounding_box(&rects).unwrap_or(Rect::new(DVec2::ZERO, DVec2::ZERO))
    }

    /// The subject's top-left corner from its clauses: each axis stated
    /// exactly once, a lone direction supplying the other axis's default.
    fn resolve_clauses(
        &mut self,
        clauses: &[LayoutClause],
        ids: &[u64],
        local: &[(u64, Rect)],
        size: DVec2,
    ) -> Result<DVec2, String> {
        // (value, the clause that stated it) per axis.
        let mut explicit_x: Vec<(f64, String)> = Vec::new();
        let mut explicit_y: Vec<(f64, String)> = Vec::new();
        let mut default_x: Option<f64> = None;
        let mut default_y: Option<f64> = None;

        for clause in clauses {
            match clause {
                LayoutClause::Direction {
                    direction,
                    targets,
                    gap,
                    dx,
                    dy,
                } => {
                    let t = self.targets_box(targets, ids)?;
                    let gap = gap.unwrap_or(Gap::Normal);
                    let word = direction_word(*direction);
                    match direction {
                        LayoutDirection::RightOf => {
                            explicit_x.push((t.right() + gap.across() + dx, word.into()));
                            default_y = Some(t.top() + dy);
                        }
                        LayoutDirection::LeftOf => {
                            explicit_x.push((t.left() - gap.across() - size.x + dx, word.into()));
                            default_y = Some(t.top() + dy);
                        }
                        LayoutDirection::Below => {
                            explicit_y.push((t.bottom() + gap.down() + dy, word.into()));
                            default_x = Some(t.left() + dx);
                        }
                        LayoutDirection::Above => {
                            explicit_y.push((t.top() - gap.down() - size.y + dy, word.into()));
                            default_x = Some(t.left() + dx);
                        }
                    }
                }
                LayoutClause::EvenWith { edge, targets } => {
                    let t = self.targets_box(targets, ids)?;
                    match edge {
                        LayoutEdge::Top => explicit_y.push((t.top(), "top_even_with".into())),
                        LayoutEdge::Bottom => {
                            explicit_y.push((t.bottom() - size.y, "bottom_even_with".into()))
                        }
                        LayoutEdge::Left => explicit_x.push((t.left(), "left_even_with".into())),
                        LayoutEdge::Right => {
                            explicit_x.push((t.right() - size.x, "right_even_with".into()))
                        }
                    }
                }
                LayoutClause::StraightFrom(source) => {
                    let y = self.straight_from(source, ids, local)?;
                    explicit_y.push((y, "straight_from".into()));
                }
                LayoutClause::StraightTo(destination) => {
                    let y = self.straight_to(destination, ids, local)?;
                    explicit_y.push((y, "straight_to".into()));
                }
            }
        }

        let axis = |explicit: &[(f64, String)], default: Option<f64>, name: &str| {
            match explicit {
                [] => default.ok_or_else(|| {
                    format!(
                        "nothing says where the subject goes on {name}: add a clause for {name} (e.g. {})",
                        if name == "x" {
                            "`right_of`, `left_even_with`"
                        } else {
                            "`below`, `top_even_with`, `straight_from`"
                        }
                    )
                }),
                [(value, _)] => Ok(*value),
                several => Err(format!(
                    "{name} is stated twice ({}): state each axis once",
                    several
                        .iter()
                        .map(|(_, word)| format!("`{word}`"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                )),
            }
        };
        let x = axis(&explicit_x, default_x, "x")?;
        let y = axis(&explicit_y, default_y, "y")?;
        Ok(DVec2::new(x, y))
    }

    /// `straight_from S`: the subject's top so the first wire from `S` into
    /// it (members in written order, pins in order) runs horizontal.
    fn straight_from(
        &mut self,
        source: &LayoutNodeRef,
        ids: &[u64],
        local: &[(u64, Rect)],
    ) -> Result<f64, String> {
        let source_rect = match self.resolve_target(source, ids)? {
            Target::Live(rect) => rect,
            Target::Deleted(_) => {
                return Err(format!(
                    "no wire from `{}` into the subject",
                    source.display()
                ));
            }
        };
        let source_id = self.names[&source.name];
        for (id, rect) in local {
            let Some(node) = self.scope.nodes.get(id) else {
                continue;
            };
            for (arg_index, argument) in node.arguments.iter().enumerate() {
                for wire in &argument.incoming_wires {
                    let SourcePin::NodeOutput { pin_index } = wire.source_pin else {
                        continue;
                    };
                    if wire.source_scope_depth != 0 || wire.source_node_id != source_id {
                        continue;
                    }
                    let pin_y =
                        source_rect.top() + output_pin_offset(self.scope, source_id, pin_index);
                    let member_top = pin_y - input_pin_offset(arg_index);
                    return Ok(member_top - rect.pos.y);
                }
            }
        }
        Err(format!(
            "no wire from `{}` into the subject",
            source.display()
        ))
    }

    /// `straight_to D`: the subject's top so the first wire from it into `D`
    /// (`D`'s pins in order) runs horizontal.
    fn straight_to(
        &mut self,
        destination: &LayoutNodeRef,
        ids: &[u64],
        local: &[(u64, Rect)],
    ) -> Result<f64, String> {
        let destination_rect = match self.resolve_target(destination, ids)? {
            Target::Live(rect) => rect,
            Target::Deleted(_) => {
                return Err(format!(
                    "no wire from the subject into `{}`",
                    destination.display()
                ));
            }
        };
        let destination_id = self.names[&destination.name];
        let local: HashMap<u64, Rect> = local.iter().copied().collect();
        let Some(node) = self.scope.nodes.get(&destination_id) else {
            return Err(format!("no node named `{}`", destination.display()));
        };
        for (arg_index, argument) in node.arguments.iter().enumerate() {
            for wire in &argument.incoming_wires {
                let SourcePin::NodeOutput { pin_index } = wire.source_pin else {
                    continue;
                };
                if wire.source_scope_depth != 0 {
                    continue;
                }
                let Some(rect) = local.get(&wire.source_node_id) else {
                    continue;
                };
                let pin_y = destination_rect.top() + input_pin_offset(arg_index);
                let member_top =
                    pin_y - output_pin_offset(self.scope, wire.source_node_id, pin_index);
                return Ok(member_top - rect.pos.y);
            }
        }
        Err(format!(
            "no wire from the subject into `{}`",
            destination.display()
        ))
    }

    // -----------------------------------------------------------------------
    // arrange / straighten
    // -----------------------------------------------------------------------

    fn arrange(
        &mut self,
        nodes: &[LayoutNodeRef],
        op: ArrangeOp,
        push: Option<PushDir>,
    ) -> Result<(), String> {
        let refs: Vec<&LayoutNodeRef> = nodes.iter().collect();
        let ids = self.resolve_members(&refs, false)?;
        for (node, id) in nodes.iter().zip(&ids) {
            if !self.sizes.contains_key(id) {
                return Err(format!(
                    "`{}` is new and no earlier statement placed it, so it has no position to arrange from",
                    node.display()
                ));
            }
        }

        let positions = arrange(self.scope, self.ctx.registry, self.sizes, &ids, op);
        if op == ArrangeOp::Straighten {
            // A straightened wire is a relation with the unlisted node it
            // comes from, so that node is measured against: held during this
            // statement's push and pinned after the last one, like a target.
            for &id in &ids {
                if let Some(source) = self.first_wired_source(id)
                    && !ids.contains(&source)
                    && self.sizes.contains_key(&source)
                {
                    self.targets.insert(source);
                }
            }
        }
        let why = self.statement.why();
        let mut landing = Vec::new();
        for &id in &ids {
            let Some(&position) = positions.get(&id) else {
                continue;
            };
            let was = self.live_rect(id);
            if let Some(node) = self.scope.nodes.get_mut(&id) {
                node.position = position;
            }
            if let Some(rect) = self.live_rect(id) {
                landing.push((rect, was));
            }
        }
        let existing: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| !self.ctx.added.contains(id))
            .collect();
        self.causes.record(existing, MoveCause::Arrange, &why);

        // Each member's own rect: the members can be spread across the
        // drawing, and a bounding box would sweep everything between them.
        self.make_room(&landing, &ids, push, &why);

        self.outcome.handled.extend(ids.iter().copied());
        self.outcome.pinned.extend(ids.iter().copied());
        Ok(())
    }

    /// The node `straighten` aligns `id` with: the source of its first wired
    /// input (lowest pin, same scope) — `arrange`'s own rule.
    fn first_wired_source(&self, id: u64) -> Option<u64> {
        self.scope
            .nodes
            .get(&id)?
            .arguments
            .iter()
            .find_map(|argument| {
                argument.incoming_wires.iter().find_map(|wire| {
                    (wire.source_scope_depth == 0
                        && matches!(wire.source_pin, SourcePin::NodeOutput { .. })
                        && wire.source_node_id != id
                        && self.scope.nodes.contains_key(&wire.source_node_id))
                    .then_some(wire.source_node_id)
                })
            })
    }

    // -----------------------------------------------------------------------
    // push
    // -----------------------------------------------------------------------

    /// Move what `rects` landed on out of the way. The subject never yields:
    /// it, the nodes pinned so far and this statement's own targets are held.
    ///
    /// Each rect comes with the box it was moved from, when it was an existing
    /// node (or a group of them). A node the move did **not** newly reach —
    /// tested at both strictnesses, as `grow_rect` does — is left alone: an
    /// overlap the drawing already had is the user's, and a node placed where
    /// it already stands moves nothing (the read-out's coordinates round-trip).
    fn make_room(
        &mut self,
        rects: &[(Rect, Option<Rect>)],
        subject: &[u64],
        push: Option<PushDir>,
        why: &str,
    ) {
        let mut held: HashSet<u64> = self.outcome.pinned.clone();
        held.extend(subject.iter().copied());
        held.extend(self.targets.iter().copied());

        for &(rect, was) in rects {
            let ignore: HashSet<u64> = match was {
                None => HashSet::new(),
                Some(was) => self
                    .sizes
                    .keys()
                    .copied()
                    .filter(|id| !subject.contains(id))
                    .filter(|&id| {
                        self.live_rect(id)
                            .is_some_and(|r| !newly_collides(&was, &rect, &r, VERTICAL_GAP))
                    })
                    .collect(),
            };
            match push {
                Some(PushDir::Right) => self.push_right(rect, subject, &held, &ignore, why),
                Some(PushDir::Up) => {
                    self.cascade_off(rect, Some(CascadeDir::Up), &held, &ignore, why)
                }
                Some(PushDir::Down) => {
                    self.cascade_off(rect, Some(CascadeDir::Down), &held, &ignore, why)
                }
                None => self.cascade_off(rect, None, &held, &ignore, why),
            }
        }
    }

    /// One vertical cascade off `rect`. `None` is the code's choice
    /// (`choose_cascade_dir`: split, or scored when hand-placed nodes are
    /// around), exactly Step 5b's.
    fn cascade_off(
        &mut self,
        rect: Rect,
        dir: Option<CascadeDir>,
        held: &HashSet<u64>,
        ignore: &HashSet<u64>,
        why: &str,
    ) {
        let mut not_landed_on = held.clone();
        not_landed_on.extend(ignore.iter().copied());
        let dir =
            dir.unwrap_or_else(|| choose_cascade_dir(self.scope, self.sizes, rect, &not_landed_on));
        let before = self.ys();
        let moved = cascade(self.scope, self.sizes, rect, dir, held, ignore);
        self.record_vertical(&moved, &before, why);
    }

    /// `push right`: everything at or right of the subject's left edge (the
    /// line snapped by the window rule) shifts right by the least distance
    /// that clears what the subject overlaps; what feeds the subject stays. A
    /// node the shift could not clear (fixed, or left of a snapped line) is
    /// then cascaded off as with no `push`.
    fn push_right(
        &mut self,
        rect: Rect,
        subject: &[u64],
        held: &HashSet<u64>,
        ignore: &HashSet<u64>,
        why: &str,
    ) {
        let fixed = upstream_closure(self.scope, subject.iter().copied());
        let threshold = snap_shift_line(self.scope, self.sizes, rect.left(), rect.left(), &fixed);

        let mut ids: Vec<u64> = self.sizes.keys().copied().collect();
        ids.sort_unstable();
        let dx = ids
            .iter()
            .filter(|id| !fixed.contains(id) && !ignore.contains(id))
            .filter_map(|&id| self.live_rect(id))
            .filter(|r| r.left() >= threshold && r.overlaps(&rect, VERTICAL_GAP))
            .map(|r| rect.right() + GAP - r.left())
            .fold(0.0_f64, f64::max);

        if dx > 0.0 {
            let before = self.ys();
            let outcome =
                shift_half_plane_holding(self.scope, self.sizes, threshold, dx, &fixed, held);
            self.causes.record(
                outcome.shifted.iter().copied(),
                MoveCause::Push(PushDir::Right),
                why,
            );
            self.record_vertical(&outcome.pushed, &before, why);
        }
        // Whatever the shift could not clear.
        self.cascade_off(rect, None, held, ignore, why);
    }

    fn ys(&self) -> HashMap<u64, f64> {
        self.scope
            .nodes
            .iter()
            .map(|(&id, node)| (id, node.position.y))
            .collect()
    }

    /// Record a vertical push for each of `moved`, by which way it went.
    fn record_vertical(&mut self, moved: &[u64], before: &HashMap<u64, f64>, why: &str) {
        for &id in moved {
            let (Some(&was), Some(node)) = (before.get(&id), self.scope.nodes.get(&id)) else {
                continue;
            };
            let dir = if node.position.y < was {
                PushDir::Up
            } else {
                PushDir::Down
            };
            self.causes.record([id], MoveCause::Push(dir), why);
        }
    }
}

fn direction_word(direction: LayoutDirection) -> &'static str {
    match direction {
        LayoutDirection::RightOf => "right_of",
        LayoutDirection::LeftOf => "left_of",
        LayoutDirection::Above => "above",
        LayoutDirection::Below => "below",
    }
}

fn bounding_box(rects: &[Rect]) -> Option<Rect> {
    let first = rects.first()?;
    let mut min = first.pos;
    let mut max = first.pos + first.size;
    for rect in &rects[1..] {
        min = min.min(rect.pos);
        max = max.max(rect.pos + rect.size);
    }
    Some(Rect::new(min, max - min))
}

/// Translate `rects` so their bounding box starts at the origin.
fn normalize(rects: Vec<(u64, Rect)>) -> Vec<(u64, Rect)> {
    let all: Vec<Rect> = rects.iter().map(|(_, r)| *r).collect();
    let Some(bbox) = bounding_box(&all) else {
        return rects;
    };
    rects
        .into_iter()
        .map(|(id, r)| (id, Rect::new(r.pos - bbox.pos, r.size)))
        .collect()
}
