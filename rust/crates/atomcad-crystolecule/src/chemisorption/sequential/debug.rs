//! The debug view's data (§6.5 of the design, as revised in §18): the items
//! of the panel's tree, the forms each can be shown in, the structure it is
//! shown as with its markings, and its search shapes as an analytic field.
//!
//! The panel's tree alternates two kinds of item over the search tree, which
//! itself is unchanged:
//!
//! - a **state** — the root or a leg row: the bonds made so far, at the
//!   positions they produced. Marked: the bonded pairs, and in a seated form
//!   its clashing atom pairs. No shapes, nothing faded.
//! - a **step** — a *next foot* under a state: one foot's test from that
//!   state. Marked: the foot, the sites the test accepted (and the state's
//!   bonded pairs); the rest of the substrate faded; the test's shape on
//!   `debug_shapes`. Its children are the leg rows the test accepted for
//!   that foot. The root's steps are the tree's foot rows.
//!
//! So a property of a (foot, site) pair — the verdict on the hypothesis it
//! reaches — is never painted onto a single atom: it is the child row's.
//! A step is shown in the form its test read positions from: posed under the
//! root, seated under a one- or two-leg row, relaxed under a deeper one (the
//! local phase searches from relaxed positions).
//!
//! Nothing here is stored by the search. A view is rebuilt from the tree on
//! demand: a posed or seated geometric row by geometry alone (the start
//! geometry is a function of the change set), a relaxed row from the kept
//! candidate when it is one, else by [`replay`] — which relaxes, so only
//! [`debug_view`] with a form [`needs_relaxation`] reports costs UFF calls.
//!
//! A duplicate row is shown as its canonical row: only the path the search
//! kept produced the geometry of its change set.

use super::config::SequentialSearch;
use super::evaluate::{Candidate, RelaxedRow, SearchReport};
use super::local::{hypothesis as hypothesis_at, replay, replay_start};
use super::plan::{GEOMETRIC_LEGS, Hypothesis, SequentialPlan};
use super::setup::{Leg, Mirror, Setup};
use super::tree::{NearMiss, RowKind, SearchTree};
use crate::atomic_constants::element_symbol;
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::config::ChemisorptionError;
use crate::field::{FieldBounds, GridGeometry, ScalarField};
use glam::{DVec3, Vec3};
use std::collections::BTreeMap;

/// Bonded feet and sites, and transferred atoms with their acceptors: the
/// orange of `cs_changed` highlights.
pub const BONDED_COLOR: Vec3 = Vec3::new(1.0, 0.55, 0.0);
/// A step's foot.
pub const FOOT_COLOR: Vec3 = Vec3::new(0.75, 0.3, 1.0);
/// The sites a step's test accepts.
pub const ACCEPTED_COLOR: Vec3 = Vec3::new(0.1, 0.8, 0.3);
/// The atoms of a seating's clashing pairs.
pub const CLASH_COLOR: Vec3 = Vec3::new(0.95, 0.1, 0.15);
/// The opacity of the substrate atoms a step does not mark. Transparency,
/// not a colour change: a desaturated silicon looks like a silicon.
pub const FADED_ALPHA: f32 = 0.25;
/// The search shapes.
pub const SHAPE_COLOR: Vec3 = Vec3::new(0.3, 0.6, 1.0);
pub const SHAPE_ALPHA: f32 = 0.25;

/// The isovalue of [`DebugShapes`]: the field is `SHAPE_LEVEL − d` inside
/// (`d` the signed distance to the shapes), so its level set at this value
/// is the shapes' boundary.
pub const SHAPE_LEVEL: f64 = 1.0;
/// The sample spacing of [`DebugShapes`] (Å). Coarse on purpose: the shells
/// are up to ~20 Å across, and the fallback spacing an analytic field gets
/// would make every redraw sample millions of points.
pub const SHAPE_GRID: f64 = 0.3;

/// How an item's structure is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugForm {
    /// The inputs at the pose: the root and its steps.
    Posed,
    /// The start geometry the search relaxes from: a geometric row's seating,
    /// a local-phase row's relaxed parent plus its new bond.
    Seated,
    /// The relaxation the search recorded.
    Relaxed,
}

/// The forms an item has, and the one it opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowForms {
    pub seated: bool,
    pub relaxed: bool,
    pub default: DebugForm,
}

/// One item of the panel's tree: a state row (`foot` = `None`: the root or a
/// leg row, never a foot row), or the step from it for one next foot (an
/// index into `Setup::feet`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DebugItem {
    pub row: u32,
    pub foot: Option<u32>,
}

impl DebugItem {
    pub const ROOT: DebugItem = DebugItem { row: 0, foot: None };

    pub fn state(row: u32) -> Self {
        Self { row, foot: None }
    }

    pub fn step(row: u32, foot: u32) -> Self {
        Self {
            row,
            foot: Some(foot),
        }
    }

    pub fn is_step(&self) -> bool {
        self.foot.is_some()
    }
}

/// The tree a view reads: the report's (the plan's plus the local rows) after
/// a run, the plan's before.
pub fn tree_of<'a>(plan: &'a SequentialPlan, report: Option<&'a SearchReport>) -> &'a SearchTree {
    report.map_or(&plan.tree, |r| &r.tree)
}

fn hypothesis<'a>(
    plan: &'a SequentialPlan,
    report: Option<&'a SearchReport>,
    index: u32,
) -> &'a Hypothesis {
    hypothesis_at(plan, report.map_or(&[][..], |r| &r.local), index as usize)
}

/// The row a row is shown as: a duplicate's canonical row, else itself.
pub fn shown_row(tree: &SearchTree, row: u32) -> u32 {
    tree.row(row).duplicate_of.unwrap_or(row)
}

/// The relaxation recorded for `row` itself. A geometric duplicate has none
/// of its own: its change set was relaxed once, on its canonical row.
pub fn relaxation(report: &SearchReport, row: u32) -> Option<&RelaxedRow> {
    report.relaxed.iter().find(|r| r.row == row)
}

/// The kept candidate whose canonical row is `row`.
fn kept_candidate(report: &SearchReport, row: u32) -> Option<&Candidate> {
    report.candidates.iter().find(|c| c.row == row)
}

/// The item a tree row stands for: a foot row is the root's step for that
/// foot, every other row a state.
pub fn item_of_row(tree: &SearchTree, row: u32) -> DebugItem {
    match tree.row(row).kind {
        RowKind::Foot(f) => DebugItem::step(0, f),
        _ => DebugItem::state(row),
    }
}

/// The tree row whose children are a step's legs: the foot row under the
/// root, the state row itself below it.
fn step_row(tree: &SearchTree, item: DebugItem) -> Option<u32> {
    let f = item.foot?;
    if item.row == 0 {
        tree.children(0)
            .iter()
            .copied()
            .find(|&c| tree.row(c).kind == RowKind::Foot(f))
    } else {
        Some(item.row)
    }
}

/// Whether the search grew canonical leg row `row` by one more leg, trying
/// every unbonded foot from it: a geometric row of fewer legs than the
/// level cap, a relaxed local-phase parent whose next level ran, or any row
/// the search recorded something under.
fn grown(plan: &SequentialPlan, report: Option<&SearchReport>, row: u32) -> bool {
    let tree = tree_of(plan, report);
    let r = tree.row(row);
    if r.duplicate_of.is_some() {
        return false;
    }
    if !tree.children(row).is_empty()
        || !tree.near_misses(row).is_empty()
        || r.rejected_valence + r.rejected_no_acceptor + r.rejected_filter > 0
    {
        return true;
    }
    let legs = r.legs as usize;
    if legs >= plan.max_legs {
        false
    } else if legs < GEOMETRIC_LEGS {
        !plan.stats.truncated
    } else {
        report.is_some_and(|rep| {
            relaxation(rep, row).is_some() && rep.stats.local.iter().any(|l| l.legs == legs + 1)
        })
    }
}

/// The steps under state `row`: the feet the search tried next from it
/// (indices into `Setup::feet`), in foot order. None for a duplicate row.
pub fn next_feet(plan: &SequentialPlan, report: Option<&SearchReport>, row: u32) -> Vec<u32> {
    let tree = tree_of(plan, report);
    match tree.row(row).kind {
        RowKind::Root => tree
            .children(0)
            .iter()
            .filter_map(|&c| match tree.row(c).kind {
                RowKind::Foot(f) => Some(f),
                _ => None,
            })
            .collect(),
        RowKind::Foot(_) => Vec::new(),
        RowKind::Leg { .. } => {
            if !grown(plan, report, row) {
                return Vec::new();
            }
            let bonded: Vec<usize> = tree.path(row).iter().map(|l| l.foot).collect();
            (0..plan.setup.feet.len())
                .filter(|f| !bonded.contains(f))
                .map(|f| f as u32)
                .collect()
        }
    }
}

/// The leg rows a step reaches: the rows under its state that bond its
/// foot, duplicates included, in creation order. Empty for a state.
pub fn step_legs(tree: &SearchTree, item: DebugItem) -> Vec<u32> {
    let (Some(f), Some(row)) = (item.foot, step_row(tree, item)) else {
        return Vec::new();
    };
    tree.children(row)
        .iter()
        .copied()
        .filter(|&c| matches!(tree.row(c).kind, RowKind::Leg { foot, .. } if foot == f))
        .collect()
}

/// A step's near misses: the sites its test missed by at most the band.
pub fn step_near_misses(tree: &SearchTree, item: DebugItem) -> Vec<NearMiss> {
    let (Some(f), Some(row)) = (item.foot, step_row(tree, item)) else {
        return Vec::new();
    };
    tree.near_misses(row)
        .iter()
        .copied()
        .filter(|n| n.foot == f)
        .collect()
}

/// An item's children as the panel lists them: a state's steps; a step's leg
/// rows, duplicates only when asked for (a display option: the tree is the
/// same either way).
pub fn item_children(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    item: DebugItem,
    show_duplicates: bool,
) -> Vec<DebugItem> {
    let tree = tree_of(plan, report);
    match item.foot {
        None => next_feet(plan, report, item.row)
            .into_iter()
            .map(|f| DebugItem::step(item.row, f))
            .collect(),
        Some(_) => step_legs(tree, item)
            .into_iter()
            .filter(|&c| show_duplicates || tree.row(c).duplicate_of.is_none())
            .map(DebugItem::state)
            .collect(),
    }
}

/// The item above `item`: a step's state; a leg row's step (its parent state
/// and its own foot). `None` for the root.
pub fn item_parent(tree: &SearchTree, item: DebugItem) -> Option<DebugItem> {
    if item.foot.is_some() {
        return Some(DebugItem::state(item.row));
    }
    let r = tree.row(item.row);
    let RowKind::Leg { foot, .. } = r.kind else {
        return None;
    };
    let state = match tree.row(r.parent).kind {
        RowKind::Foot(_) => 0,
        _ => r.parent,
    };
    Some(DebugItem::step(state, foot))
}

/// The items from the root down to `item`, both included: what the panel
/// expands to reveal it.
pub fn item_ancestors(tree: &SearchTree, item: DebugItem) -> Vec<DebugItem> {
    let mut out = vec![item];
    let mut at = item;
    while let Some(parent) = item_parent(tree, at) {
        out.push(parent);
        at = parent;
    }
    out.reverse();
    out
}

/// A state row's item, or the item a foot row stands for: what the tree's own
/// rows are shown as. Errors past the tree's end.
fn checked_item(tree: &SearchTree, item: DebugItem) -> Result<DebugItem, ChemisorptionError> {
    if item.row as usize >= tree.len() {
        return Err(ChemisorptionError::DebugRow(format!(
            "the search tree has no row {}",
            item.row
        )));
    }
    Ok(match (tree.row(item.row).kind, item.foot) {
        (RowKind::Foot(f), None) => DebugItem::step(0, f),
        (RowKind::Foot(_), Some(_)) => {
            return Err(ChemisorptionError::DebugRow(format!(
                "row {} is a foot row: it has no next foot",
                item.row
            )));
        }
        _ => item,
    })
}

/// The form a step under state `row` is shown in: the one its test read
/// positions from.
fn step_form(tree: &SearchTree, row: u32) -> DebugForm {
    match tree.row(row).legs as usize {
        0 => DebugForm::Posed,
        legs if legs < GEOMETRIC_LEGS => DebugForm::Seated,
        _ => DebugForm::Relaxed,
    }
}

/// The forms `item` can be shown in, and the one it opens on: a step in the
/// one form its test read positions from; the root posed; a leg row relaxed
/// when it was relaxed, else seated, and seated on request.
pub fn row_forms(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    item: DebugItem,
) -> RowForms {
    let tree = tree_of(plan, report);
    let item = checked_item(tree, item).unwrap_or(DebugItem::ROOT);
    let target = shown_row(tree, item.row);
    if item.foot.is_some() {
        let form = step_form(tree, target);
        return RowForms {
            seated: form == DebugForm::Seated,
            relaxed: form == DebugForm::Relaxed,
            default: form,
        };
    }
    if !matches!(tree.row(target).kind, RowKind::Leg { .. }) {
        return RowForms {
            seated: false,
            relaxed: false,
            default: DebugForm::Posed,
        };
    }
    let relaxed = report.is_some_and(|rep| relaxation(rep, target).is_some());
    RowForms {
        seated: true,
        relaxed,
        default: if relaxed {
            DebugForm::Relaxed
        } else {
            DebugForm::Seated
        },
    }
}

/// Whether showing `item` in `form` takes UFF relaxations: a relaxed row
/// that is not a kept candidate is replayed, and so are a local-phase row's
/// ancestors for its start geometry. Posed and geometric seated rows are
/// geometry alone.
pub fn needs_relaxation(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    item: DebugItem,
    form: DebugForm,
) -> bool {
    let tree = tree_of(plan, report);
    let target = shown_row(tree, item.row);
    match form {
        DebugForm::Posed => false,
        DebugForm::Seated => tree.row(target).legs as usize > GEOMETRIC_LEGS,
        DebugForm::Relaxed => report.is_none_or(|r| kept_candidate(r, target).is_none()),
    }
}

/// What a view marks, combined atom ids, each list sorted. Colours go from
/// the least to the most telling: bonded, clashing, accepted, the foot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// A step's foot.
    pub foot: Option<u32>,
    /// The state's bonded feet and sites, its transferred atoms and their
    /// acceptors.
    pub bonded: Vec<u32>,
    /// The sites a step's test accepted: those of its leg rows, whatever
    /// became of the hypothesis each reaches (that is the row's to say).
    pub accepted: Vec<u32>,
    /// A seated state's clashing atom pairs.
    pub clashing: Vec<u32>,
    /// Not drawn: the sites a step's test missed by at most the near-miss
    /// band, with the smallest miss (Å). The CLI lists them.
    pub near_misses: Vec<(u32, f32)>,
}

/// One item, ready to show: its structure (markings in the decorator: colour
/// overrides, and on a step the rest of the substrate faded), its search
/// shapes, and what was marked.
#[derive(Debug, Clone)]
pub struct DebugView {
    /// The item shown: the one asked for, with a duplicate row replaced by
    /// its canonical row and a foot row by the root's step.
    pub item: DebugItem,
    pub form: DebugForm,
    pub structure: AtomicStructure,
    /// A step's test shape; `None` for a state.
    pub shapes: Option<DebugShapes>,
    /// The recorded strain, for a relaxed form (kcal/mol).
    pub strain: Option<f64>,
    pub marks: Marks,
}

/// The root view: the posed inputs, nothing marked, no shapes. What
/// evaluation shows with no selection.
pub fn root_view(plan: &SequentialPlan, config: &SequentialSearch) -> DebugView {
    debug_view(plan, None, config, DebugItem::ROOT, DebugForm::Posed)
        .expect("the root is always posed")
}

/// Builds the view of `item` in `form` (§6.5). `report` is the run's result
/// for the same plan, `None` before a run. Relaxes only when
/// [`needs_relaxation`] says so.
pub fn debug_view(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    config: &SequentialSearch,
    item: DebugItem,
    form: DebugForm,
) -> Result<DebugView, ChemisorptionError> {
    let tree = tree_of(plan, report);
    let item = checked_item(tree, item)?;
    let item = DebugItem {
        row: shown_row(tree, item.row),
        ..item
    };
    let setup = &plan.setup;
    if let Some(f) = item.foot
        && !next_feet(plan, report, item.row).contains(&f)
    {
        return Err(ChemisorptionError::DebugRow(format!(
            "the search tried no next foot {} from {}",
            atom_name(
                &setup.combined,
                setup.feet.get(f as usize).map_or(0, |x| x.id)
            ),
            item_label(setup, tree, DebugItem::state(item.row))
        )));
    }
    let forms = row_forms(plan, report, item);
    let allowed = match form {
        DebugForm::Posed => forms.default == DebugForm::Posed,
        DebugForm::Seated => forms.seated,
        DebugForm::Relaxed => forms.relaxed,
    };
    if !allowed {
        let shown = match forms.default {
            DebugForm::Posed => "posed",
            DebugForm::Seated => "seated",
            DebugForm::Relaxed => "relaxed",
        };
        return Err(ChemisorptionError::DebugRow(format!(
            "{} is shown {shown}{}",
            item_label(setup, tree, item),
            if form == DebugForm::Relaxed && forms.seated {
                ": it was not relaxed"
            } else {
                ""
            }
        )));
    }
    let target = item.row;
    let mut own_clashes = Vec::new();
    let (mut structure, strain) = match form {
        DebugForm::Posed => (setup.combined.clone(), None),
        DebugForm::Seated => {
            if tree.row(target).legs as usize <= GEOMETRIC_LEGS {
                let index = tree.row(target).hypothesis.expect("a canonical row");
                let h = hypothesis(plan, report, index);
                let seating = match &h.seating {
                    Some(s) => s.clone(),
                    None => setup.seat(&h.steps),
                };
                own_clashes = seating.clashes.clone();
                (setup.start_structure(&h.steps, &seating), None)
            } else {
                let report = report.expect("a local-phase row exists only after a run");
                (replay_start(plan, report, config, target)?.0, None)
            }
        }
        DebugForm::Relaxed => {
            let report = report.expect("a relaxed form exists only after a run");
            match kept_candidate(report, target) {
                Some(c) => (c.structure.clone(), Some(c.strain)),
                None => {
                    let r = replay(plan, report, config, target)?;
                    (r.structure, Some(r.strain))
                }
            }
        }
    };
    if item.is_step() {
        // The step reads the state's geometry; its clashes are the state's.
        own_clashes.clear();
    }
    let marks = marks(plan, report, item, &own_clashes);
    decorate(&mut structure, setup, &marks, item.is_step());
    let shapes = item
        .foot
        .and_then(|f| step_shapes(plan, config, tree, target, f, &structure));
    Ok(DebugView {
        item,
        form,
        structure,
        shapes,
        strain,
        marks,
    })
}

/// What became of the hypothesis a leg row reaches, as the panel counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildVerdict {
    /// Seated cleanly, or not seated (neither a candidate nor a parent).
    Candidate,
    /// Pruned by the mirror check.
    Mirrored,
    /// Left undecided by the mirror check, and seated cleanly.
    Undecided,
    /// Its seating clashes (pruned only with `clash_filter`).
    Clash,
}

/// The verdict on the hypothesis a leg row reaches (a duplicate's: its
/// canonical row's).
pub fn child_verdict(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    child: u32,
) -> ChildVerdict {
    let tree = tree_of(plan, report);
    let Some(index) = tree.row(shown_row(tree, child)).hypothesis else {
        return ChildVerdict::Candidate;
    };
    let h = hypothesis(plan, report, index);
    if h.mirror == Some(Mirror::Mirrored) {
        ChildVerdict::Mirrored
    } else if h.seating.as_ref().is_some_and(|s| s.clashes()) {
        ChildVerdict::Clash
    } else if h.mirror == Some(Mirror::Undecided) {
        ChildVerdict::Undecided
    } else {
        ChildVerdict::Candidate
    }
}

fn marks(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    item: DebugItem,
    own_clashes: &[(u32, u32)],
) -> Marks {
    let tree = tree_of(plan, report);
    let setup = &plan.setup;
    let mut m = Marks::default();
    if let Some(index) = tree.row(item.row).hypothesis {
        let h = hypothesis(plan, report, index);
        for &(f, s) in &h.formed {
            m.bonded.extend([f, s]);
        }
        for t in &h.transfers {
            m.bonded.extend([t.moved, t.acceptor]);
        }
    }
    for &(a, s) in own_clashes {
        m.clashing.extend([a, s]);
    }
    if let Some(f) = item.foot {
        m.foot = Some(setup.feet[f as usize].id);
        for c in step_legs(tree, item) {
            if let RowKind::Leg { site, .. } = tree.row(c).kind {
                m.accepted.push(setup.sites[site as usize].id);
            }
        }
        let mut near: BTreeMap<u32, f32> = BTreeMap::new();
        for n in step_near_misses(tree, item) {
            let e = near
                .entry(setup.sites[n.site as usize].id)
                .or_insert(n.miss);
            *e = e.min(n.miss);
        }
        m.near_misses = near.into_iter().collect();
    }
    for list in [&mut m.bonded, &mut m.accepted, &mut m.clashing] {
        list.sort_unstable();
        list.dedup();
    }
    m
}

/// An atom's name: its element and combined id, as the candidate rows name
/// atoms.
pub fn atom_name(structure: &AtomicStructure, id: u32) -> String {
    let z = structure.get_atom(id).map_or(0, |a| a.atomic_number);
    format!("{}{id}", element_symbol(z))
}

/// Colour overrides, and on a step the unmarked substrate faded. Later writes
/// win, so the lists go from the least to the most telling. No labels: an
/// atom's name is in its hover tooltip.
fn decorate(structure: &mut AtomicStructure, setup: &Setup, m: &Marks, step: bool) {
    let mut colour: BTreeMap<u32, Vec3> = BTreeMap::new();
    for (list, c) in [
        (&m.bonded, BONDED_COLOR),
        (&m.clashing, CLASH_COLOR),
        (&m.accepted, ACCEPTED_COLOR),
    ] {
        for &id in list {
            colour.insert(id, c);
        }
    }
    if let Some(id) = m.foot {
        colour.insert(id, FOOT_COLOR);
    }
    if step {
        for &id in setup.substrate_ids.values() {
            if !colour.contains_key(&id) {
                structure.set_atom_alpha(id, FADED_ALPHA);
            }
        }
    }
    structure.decorator_mut().atom_color.extend(colour);
}

/// A step's test shape (§4.3, §4.4, §4.8): the `anchor_reach` sphere around
/// the posed foot under the root; under a one- or two-leg row, the shell
/// (one leg) or the two-shell ring (two legs) the foot's site must lie in,
/// widened by the tolerance; under a deeper row, the `reach` sphere around
/// the foot's relaxed position.
fn step_shapes(
    plan: &SequentialPlan,
    config: &SequentialSearch,
    tree: &SearchTree,
    row: u32,
    foot: u32,
    structure: &AtomicStructure,
) -> Option<DebugShapes> {
    let setup = &plan.setup;
    let f = foot as usize;
    let shape = match tree.row(row).kind {
        RowKind::Leg { .. } => {
            let path = tree.path(row);
            if path.len() < GEOMETRIC_LEGS {
                Shape::Shells(
                    path.iter()
                        .map(|&b| shell(setup, b, f, config.tolerance))
                        .collect(),
                )
            } else {
                let p = structure
                    .get_atom(setup.feet[f].id)
                    .map_or(setup.feet[f].position, |a| a.position);
                Shape::Ball {
                    centre: p,
                    radius: config.reach,
                }
            }
        }
        _ => Shape::Ball {
            centre: setup.feet[f].position,
            radius: config.anchor_reach,
        },
    };
    DebugShapes::new(vec![shape])
}

/// The shell around bonded leg `b`'s site that foot `f`'s site must lie in
/// (§4.3): foot spacing ± the two bond lengths ± the tolerance. `f`'s bond
/// length depends on the site it would bond to, so the longest is taken:
/// the drawn shell contains the test's for every site element.
fn shell(setup: &Setup, b: Leg, f: usize, tolerance: f64) -> Shell {
    let d = setup.feet[b.foot].position.distance(setup.feet[f].position);
    let longest = (0..setup.sites.len())
        .map(|site| setup.bond_length(Leg { foot: f, site }))
        .fold(0.0, f64::max);
    let slack = setup.bond_length(b) + longest + tolerance;
    Shell {
        centre: setup.sites[b.site].position,
        inner: d - slack,
        outer: d + slack,
    }
}

/// A spherical shell, `inner ≤ |p − centre| ≤ outer`; a solid ball when
/// `inner ≤ 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shell {
    pub centre: DVec3,
    pub inner: f64,
    pub outer: f64,
}

/// One search shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Ball {
        centre: DVec3,
        radius: f64,
    },
    /// The overlap of shells: one is the sphere of leg 2, two the ring of
    /// leg 3.
    Shells(Vec<Shell>),
}

impl Shape {
    /// Signed distance, negative inside (the overlap's: the larger of its
    /// shells' distances).
    pub fn distance(&self, p: DVec3) -> f64 {
        match self {
            Shape::Ball { centre, radius } => p.distance(*centre) - radius,
            Shape::Shells(shells) => shells
                .iter()
                .map(|s| {
                    let d = p.distance(s.centre);
                    (s.inner - d).max(d - s.outer)
                })
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }

    /// A box that contains the shape, `None` when it is empty.
    fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let (mut lo, mut hi) = (DVec3::splat(f64::NEG_INFINITY), DVec3::splat(f64::INFINITY));
        let mut clip = |c: DVec3, r: f64| {
            lo = lo.max(c - DVec3::splat(r));
            hi = hi.min(c + DVec3::splat(r));
        };
        match self {
            Shape::Ball { centre, radius } => clip(*centre, *radius),
            Shape::Shells(shells) => {
                for s in shells {
                    clip(s.centre, s.outer);
                }
            }
        }
        (lo.cmple(hi).all() && lo.is_finite() && hi.is_finite()).then_some((lo, hi))
    }
}

/// A row's search shapes as one scalar field: `SHAPE_LEVEL − d` where `d`
/// is the signed distance to their union, clamped at 0, so the isosurface at
/// [`SHAPE_LEVEL`] is their boundary and the field is never negative (no
/// second, negative lobe is extracted).
///
/// It reports a native grid although it is analytic: [`SHAPE_GRID`] is the
/// resolution it is drawn at, chosen here because nothing else knows that a
/// debug shape needs far less than a molecular orbital.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugShapes {
    pub shapes: Vec<Shape>,
    grid: GridGeometry,
}

impl DebugShapes {
    /// `None` when there is nothing to draw.
    pub fn new(shapes: Vec<Shape>) -> Option<Self> {
        let boxes: Vec<(DVec3, DVec3)> = shapes.iter().filter_map(Shape::bounds).collect();
        let pad = DVec3::splat(2.0 * SHAPE_GRID);
        let lo = boxes.iter().map(|b| b.0).reduce(DVec3::min)? - pad;
        let hi = boxes.iter().map(|b| b.1).reduce(DVec3::max)? + pad;
        let dims = ((hi - lo) / SHAPE_GRID)
            .ceil()
            .to_array()
            .map(|n| n as usize + 1);
        Some(Self {
            shapes,
            grid: GridGeometry {
                origin: lo,
                axes: [
                    DVec3::X * SHAPE_GRID,
                    DVec3::Y * SHAPE_GRID,
                    DVec3::Z * SHAPE_GRID,
                ],
                dims,
            },
        })
    }

    /// Signed distance to the union of the shapes, negative inside.
    pub fn distance(&self, p: DVec3) -> f64 {
        self.shapes
            .iter()
            .map(|s| s.distance(p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether `p` lies in one of the shapes.
    pub fn contains(&self, p: DVec3) -> bool {
        self.distance(p) <= 0.0
    }
}

impl ScalarField for DebugShapes {
    fn sample(&self, point: DVec3) -> f64 {
        (SHAPE_LEVEL - self.distance(point)).max(0.0)
    }

    fn data_bounds(&self) -> Option<FieldBounds> {
        None
    }

    fn suggested_bounds(&self) -> FieldBounds {
        self.grid.bounds()
    }

    fn native_grid(&self) -> Option<GridGeometry> {
        Some(self.grid)
    }

    fn value_range(&self) -> Option<(f64, f64)> {
        let b = self.grid.bounds();
        Some((0.0, SHAPE_LEVEL + b.max.distance(b.min)))
    }

    fn description(&self) -> Option<&str> {
        Some("chemisorb search shapes")
    }

    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .shapes
                .iter()
                .map(|s| match s {
                    Shape::Shells(v) => v.len() * std::mem::size_of::<Shell>(),
                    Shape::Ball { .. } => 0,
                })
                .sum::<usize>()
            + self.shapes.len() * std::mem::size_of::<Shape>()
    }
}

fn element_name(element: i16, id: u32) -> String {
    format!("{}{id}", element_symbol(element))
}

/// The leg a leg row adds, `O12–Si45`, with its transfer, `O12–Si45
/// (H→Si46)`.
fn leg_label(setup: &Setup, foot: u32, site: u32, acceptor: Option<u32>) -> String {
    let f = &setup.feet[foot as usize];
    let s = &setup.sites[site as usize];
    let mut text = format!(
        "{}–{}",
        element_name(f.element, f.id),
        element_name(s.element, s.id)
    );
    if let (Some(a), Some(x)) = (acceptor, f.donates) {
        let a = &setup.sites[a as usize];
        text.push_str(&format!(
            " ({}→{})",
            element_symbol(x),
            element_name(a.element, a.id)
        ));
    }
    text
}

/// An item as the panel and the CLI name it: `root`, the leg a leg row adds
/// (`O12–Si45`, with its transfer `O12–Si45 (H→Si46)`), or a step, `next
/// foot O13`.
pub fn item_label(setup: &Setup, tree: &SearchTree, item: DebugItem) -> String {
    if let Some(f) = item.foot {
        let f = &setup.feet[f as usize];
        return format!("next foot {}", element_name(f.element, f.id));
    }
    match tree.row(item.row).kind {
        RowKind::Root => "root".to_string(),
        RowKind::Foot(f) => {
            let f = &setup.feet[f as usize];
            format!("next foot {}", element_name(f.element, f.id))
        }
        RowKind::Leg {
            foot,
            site,
            acceptor,
        } => leg_label(setup, foot, site, acceptor),
    }
}

/// An item's path as [`find_item`] reads it: `root`; the legs along a leg
/// row's path as `foot-site` atom-id pairs (`12-45,13-61`); a step as its
/// state's path followed by the foot alone (`12` under the root, `12-45,13`
/// below it).
pub fn item_path(setup: &Setup, tree: &SearchTree, item: DebugItem) -> String {
    let item = match tree.row(item.row).kind {
        RowKind::Foot(f) => DebugItem::step(0, f),
        _ => item,
    };
    let mut steps: Vec<String> = tree
        .path(item.row)
        .iter()
        .map(|l| format!("{}-{}", setup.feet[l.foot].id, setup.sites[l.site].id))
        .collect();
    if let Some(f) = item.foot {
        steps.push(setup.feet[f as usize].id.to_string());
    }
    if steps.is_empty() {
        "root".to_string()
    } else {
        steps.join(",")
    }
}

/// The item a path names (the CLI's `debug-select`): `root` or empty, `#N`
/// for tree row `N` (a foot row is the root's step), or steps separated by
/// `,` — each a `foot-site` pair, the last one possibly a foot alone.
/// `12-45,13-61` is the two-leg row that bonds foot 12 to site 45, then foot
/// 13 to site 61; `12-45,13` is the step that tries foot 13 after the first
/// leg; `12` is the root's step for foot 12. Ids are the combined ids the
/// candidates' `sites` field uses; an element prefix (`O12-Si45`) is allowed.
/// Whether a step was tried is checked when it is shown.
pub fn find_item(setup: &Setup, tree: &SearchTree, path: &str) -> Result<DebugItem, String> {
    let path = path.trim();
    if path.is_empty() || path == "root" {
        return Ok(DebugItem::ROOT);
    }
    if let Some(n) = path.strip_prefix('#') {
        let row: u32 = n
            .trim()
            .parse()
            .map_err(|_| format!("'{path}' is not a row number"))?;
        if row as usize >= tree.len() {
            return Err(format!("the search tree has no row {row}"));
        }
        return Ok(item_of_row(tree, row));
    }
    let id = |text: &str| -> Result<u32, String> {
        text.trim()
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .parse()
            .map_err(|_| format!("'{text}' is not an atom id"))
    };
    let foot_of = |text: &str| -> Result<u32, String> {
        let id = id(text)?;
        setup
            .feet
            .iter()
            .position(|f| f.id == id)
            .map(|f| f as u32)
            .ok_or_else(|| format!("atom {id} is not a foot"))
    };
    let steps: Vec<&str> = path.split(',').map(str::trim).collect();
    let mut state = 0;
    for (i, step) in steps.iter().enumerate() {
        let parts: Vec<&str> = step.split(['-', '–']).collect();
        let f = foot_of(parts[0])?;
        match parts.as_slice() {
            [_] if i + 1 == steps.len() => return Ok(DebugItem::step(state, f)),
            [_, s] => {
                let site_id = id(s)?;
                let item = DebugItem::step(state, f);
                state = step_legs(tree, item)
                    .into_iter()
                    .find(|&c| {
                        matches!(tree.row(c).kind, RowKind::Leg { site, .. }
                            if setup.sites[site as usize].id == site_id)
                    })
                    .ok_or_else(|| {
                        format!(
                            "{} has no leg {step}",
                            item_label(setup, tree, DebugItem::state(state))
                        )
                    })?;
            }
            _ => {
                return Err(format!(
                    "'{step}': a step is a foot-site pair; only the last may name a foot alone"
                ));
            }
        }
    }
    Ok(DebugItem::state(state))
}

impl DebugView {
    /// What the view shows, one fact per line: the CLI's `debug-select`.
    pub fn describe(&self, setup: &Setup, tree: &SearchTree) -> String {
        let s = &self.structure;
        let names = |ids: &[u32]| {
            ids.iter()
                .map(|&id| atom_name(s, id))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let form = match self.form {
            DebugForm::Posed => "posed",
            DebugForm::Seated => "seated",
            DebugForm::Relaxed => "relaxed",
        };
        let state = DebugItem::state(self.item.row);
        let legs = tree.row(self.item.row).legs;
        let strain = self
            .strain
            .map_or(String::new(), |e| format!(", strain {e:.2} kcal/mol"));
        let what = if self.item.is_step() {
            format!(
                "{} from {}",
                item_label(setup, tree, self.item),
                item_label(setup, tree, state)
            )
        } else {
            format!("#{} {}", self.item.row, item_label(setup, tree, state))
        };
        let mut lines = vec![format!(
            "{what} ({legs} leg{}), shown {form}{strain}",
            if legs == 1 { "" } else { "s" }
        )];
        lines.push(format!("path: {}", item_path(setup, tree, self.item)));
        let m = &self.marks;
        if let Some(foot) = m.foot {
            lines.push(format!("foot: {}", atom_name(s, foot)));
        }
        for (what, ids) in [
            ("bonded", &m.bonded),
            ("accepted", &m.accepted),
            ("clashing", &m.clashing),
        ] {
            if !ids.is_empty() {
                lines.push(format!("{what} ({}): {}", ids.len(), names(ids)));
            }
        }
        if !m.near_misses.is_empty() {
            let misses = m
                .near_misses
                .iter()
                .map(|&(id, miss)| format!("{} +{miss:.2}", atom_name(s, id)))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("near misses ({}): {misses}", m.near_misses.len()));
        }
        let shapes = self.shapes.as_ref().map_or(0, |d| d.shapes.len());
        lines.push(format!("shapes: {shapes}"));
        lines.join("\n")
    }
}
