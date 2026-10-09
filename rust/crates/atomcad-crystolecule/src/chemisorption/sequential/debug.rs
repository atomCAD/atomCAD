//! The debug view's data (§6.5 of the design): which forms a row of the
//! search tree can be shown in, the structure it is shown as with its
//! markings, and its search shapes as an analytic field.
//!
//! Nothing here is stored by the search. A view is rebuilt from a row's data
//! on demand: a posed or seated geometric row by geometry alone (the start
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
use super::tree::{RowKind, SearchTree};
use crate::atomic_constants::element_symbol;
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::config::ChemisorptionError;
use crate::field::{FieldBounds, GridGeometry, ScalarField};
use glam::{DVec3, Vec3};
use std::collections::BTreeMap;

/// Bonded feet and sites, and transferred atoms with their acceptors: the
/// orange of `cs_changed` highlights.
pub const BONDED_COLOR: Vec3 = Vec3::new(1.0, 0.55, 0.0);
/// Feet not bonded yet.
pub const FOOT_COLOR: Vec3 = Vec3::new(0.75, 0.3, 1.0);
/// Sites the next leg's test accepts.
pub const CANDIDATE_COLOR: Vec3 = Vec3::new(0.1, 0.8, 0.3);
/// Sites just outside the test.
pub const NEAR_MISS_COLOR: Vec3 = Vec3::new(1.0, 0.85, 0.1);
/// Three-leg assignments the mirror check prunes.
pub const MIRRORED_COLOR: Vec3 = Vec3::new(0.3, 0.45, 1.0);
/// Three-leg assignments the mirror check leaves undecided.
pub const UNDECIDED_COLOR: Vec3 = Vec3::new(0.2, 0.85, 0.95);
/// Seatings that clash, and the atoms of a seating's clashing pairs.
pub const CLASH_COLOR: Vec3 = Vec3::new(0.95, 0.1, 0.15);
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

/// How a row's structure is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugForm {
    /// The inputs at the pose: the root and the foot rows.
    Posed,
    /// The start geometry the search relaxes from: a geometric row's seating,
    /// a local-phase row's relaxed parent plus its new bond.
    Seated,
    /// The relaxation the search recorded.
    Relaxed,
}

/// The forms a row has, and the one it opens on (§6.5, "Seated or relaxed").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowForms {
    pub seated: bool,
    pub relaxed: bool,
    pub default: DebugForm,
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

/// The forms `row` can be shown in, and the one it opens on. A row opens on
/// the form the next step of the search uses: a one- or two-leg row with
/// children seated (the sphere and the ring are searched from the seating),
/// every other row relaxed when it was, seated when it was not. The root and
/// the foot rows are posed.
pub fn row_forms(plan: &SequentialPlan, report: Option<&SearchReport>, row: u32) -> RowForms {
    let tree = tree_of(plan, report);
    let target = shown_row(tree, row);
    let r = tree.row(target);
    if !matches!(r.kind, RowKind::Leg { .. }) {
        return RowForms {
            seated: false,
            relaxed: false,
            default: DebugForm::Posed,
        };
    }
    let relaxed = report.is_some_and(|rep| relaxation(rep, target).is_some());
    let searched_from_seating =
        (r.legs as usize) < GEOMETRIC_LEGS && !tree.children(target).is_empty();
    RowForms {
        seated: true,
        relaxed,
        default: if relaxed && !searched_from_seating {
            DebugForm::Relaxed
        } else {
            DebugForm::Seated
        },
    }
}

/// Whether showing `row` in `form` takes UFF relaxations: a relaxed row that
/// is not a kept candidate is replayed, and so are a local-phase row's
/// ancestors for its start geometry. Posed and geometric seated rows are
/// geometry alone.
pub fn needs_relaxation(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    row: u32,
    form: DebugForm,
) -> bool {
    let tree = tree_of(plan, report);
    let target = shown_row(tree, row);
    match form {
        DebugForm::Posed => false,
        DebugForm::Seated => tree.row(target).legs as usize > GEOMETRIC_LEGS,
        DebugForm::Relaxed => report.is_none_or(|r| kept_candidate(r, target).is_none()),
    }
}

/// What a view marks, combined atom ids, each list sorted. An atom can be in
/// several lists (a site one foot's test accepts and another's misses); it is
/// coloured by the first of: bonded, foot, clashing, candidate, undecided,
/// mirrored, near miss.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// The feet not bonded yet (on a foot row, that foot).
    pub feet: Vec<u32>,
    /// The row's bonded feet and sites, its transferred atoms and their
    /// acceptors.
    pub bonded: Vec<u32>,
    /// Sites the next leg's test accepts (the row's children), by the
    /// verdict on the hypothesis each reaches.
    pub candidates: Vec<u32>,
    pub mirrored: Vec<u32>,
    pub undecided: Vec<u32>,
    /// Children whose seating clashes, and, for a seated row, the atoms of its
    /// own clashing pairs.
    pub clashing: Vec<u32>,
    /// Sites outside the next leg's test by at most the near-miss band, with
    /// the smallest miss (Å).
    pub near_misses: Vec<(u32, f32)>,
}

/// One row, ready to show: its structure (markings in the decorator: colour
/// overrides, labels, the rest of the substrate ghosted), its search shapes,
/// and what was marked.
#[derive(Debug, Clone)]
pub struct DebugView {
    /// The row shown: the row asked for, or its canonical row when it is a
    /// duplicate.
    pub row: u32,
    pub form: DebugForm,
    pub structure: AtomicStructure,
    /// The search shapes of the row's next level; `None` when it has none
    /// (a leaf, or a seated row whose next level is searched relaxed).
    pub shapes: Option<DebugShapes>,
    /// The recorded strain, for a relaxed row (kcal/mol).
    pub strain: Option<f64>,
    pub marks: Marks,
}

/// The root view: the posed molecule, every foot and its anchor sites, the
/// `anchor_reach` spheres. Geometry alone; what evaluation shows with no
/// selection.
pub fn root_view(plan: &SequentialPlan, config: &SequentialSearch) -> DebugView {
    debug_view(plan, None, config, 0, DebugForm::Posed).expect("the root is always posed")
}

/// Builds the view of tree row `row` in `form` (§6.5). `report` is the run's
/// result for the same plan, `None` before a run. Relaxes only when
/// [`needs_relaxation`] says so.
pub fn debug_view(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    config: &SequentialSearch,
    row: u32,
    form: DebugForm,
) -> Result<DebugView, ChemisorptionError> {
    let tree = tree_of(plan, report);
    if row as usize >= tree.len() {
        return Err(ChemisorptionError::DebugRow(format!(
            "the search tree has no row {row}"
        )));
    }
    let target = shown_row(tree, row);
    let forms = row_forms(plan, report, target);
    let setup = &plan.setup;
    let mut own_clashes = Vec::new();
    let (mut structure, strain) = match form {
        DebugForm::Posed => {
            if forms.default != DebugForm::Posed {
                return Err(ChemisorptionError::DebugRow(format!(
                    "row {target} has bonded legs: it is shown seated or relaxed"
                )));
            }
            (setup.combined.clone(), None)
        }
        DebugForm::Seated => {
            if !forms.seated {
                return Err(ChemisorptionError::DebugRow(
                    "the root and the foot rows are shown posed".into(),
                ));
            }
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
            let Some(report) = report.filter(|_| forms.relaxed) else {
                return Err(ChemisorptionError::DebugRow(format!(
                    "row {target} was not relaxed"
                )));
            };
            match kept_candidate(report, target) {
                Some(c) => (c.structure.clone(), Some(c.strain)),
                None => {
                    let r = replay(plan, report, config, target)?;
                    (r.structure, Some(r.strain))
                }
            }
        }
    };
    let marks = marks(plan, report, target, &own_clashes);
    decorate(&mut structure, setup, &marks);
    let shapes = shapes(plan, config, tree, target, &structure, form);
    Ok(DebugView {
        row: target,
        form,
        structure,
        shapes,
        strain,
        marks,
    })
}

/// What became of the hypothesis a child row reaches, as the view colours it.
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

/// The verdict on the hypothesis a child row reaches (a duplicate's: its
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
    target: u32,
    own_clashes: &[(u32, u32)],
) -> Marks {
    let tree = tree_of(plan, report);
    let setup = &plan.setup;
    let r = tree.row(target);
    let mut m = Marks::default();
    let bonded_feet: Vec<usize> = tree.path(target).iter().map(|l| l.foot).collect();
    match r.kind {
        RowKind::Foot(f) => m.feet.push(setup.feet[f as usize].id),
        _ => m.feet.extend(
            setup
                .feet
                .iter()
                .enumerate()
                .filter(|(i, _)| !bonded_feet.contains(i))
                .map(|(_, f)| f.id),
        ),
    }
    if let Some(index) = r.hypothesis {
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
    // The root marks what each foot row would: every anchor.
    let parents: Vec<u32> = match r.kind {
        RowKind::Root => tree.children(target).to_vec(),
        _ => vec![target],
    };
    let mut near: BTreeMap<u32, f32> = BTreeMap::new();
    for p in parents {
        for &c in tree.children(p) {
            let RowKind::Leg { site, .. } = tree.row(c).kind else {
                continue;
            };
            let id = setup.sites[site as usize].id;
            match child_verdict(plan, report, c) {
                ChildVerdict::Candidate => m.candidates.push(id),
                ChildVerdict::Mirrored => m.mirrored.push(id),
                ChildVerdict::Undecided => m.undecided.push(id),
                ChildVerdict::Clash => m.clashing.push(id),
            }
        }
        for n in tree.near_misses(p) {
            let id = setup.sites[n.site as usize].id;
            let e = near.entry(id).or_insert(n.miss);
            *e = e.min(n.miss);
        }
    }
    m.near_misses = near.into_iter().collect();
    for list in [
        &mut m.feet,
        &mut m.bonded,
        &mut m.candidates,
        &mut m.mirrored,
        &mut m.undecided,
        &mut m.clashing,
    ] {
        list.sort_unstable();
        list.dedup();
    }
    m
}

/// An atom's label: its element and combined id, as the candidate rows name
/// atoms.
pub fn atom_name(structure: &AtomicStructure, id: u32) -> String {
    let z = structure.get_atom(id).map_or(0, |a| a.atomic_number);
    format!("{}{id}", element_symbol(z))
}

/// Colours, labels and ghosting. Later writes win, so the lists go from the
/// least to the most important.
fn decorate(structure: &mut AtomicStructure, setup: &Setup, m: &Marks) {
    let mut colour: BTreeMap<u32, Vec3> = BTreeMap::new();
    let mut label: BTreeMap<u32, String> = BTreeMap::new();
    for &(id, miss) in &m.near_misses {
        colour.insert(id, NEAR_MISS_COLOR);
        label.insert(id, format!("{} +{:.2}", atom_name(structure, id), miss));
    }
    for (list, c) in [
        (&m.mirrored, MIRRORED_COLOR),
        (&m.undecided, UNDECIDED_COLOR),
        (&m.candidates, CANDIDATE_COLOR),
        (&m.clashing, CLASH_COLOR),
        (&m.feet, FOOT_COLOR),
        (&m.bonded, BONDED_COLOR),
    ] {
        for &id in list {
            colour.insert(id, c);
        }
    }
    for &id in m.feet.iter().chain(&m.bonded) {
        label.insert(id, atom_name(structure, id));
    }
    for &id in setup.substrate_ids.values() {
        if !colour.contains_key(&id) {
            structure.set_atom_ghost(id, true);
        }
    }
    let decorator = structure.decorator_mut();
    decorator.atom_color.extend(colour);
    decorator.atom_label.extend(label);
}

/// The search shapes of the row's next level: the `anchor_reach` spheres of
/// the root and the foot rows; for a row of one or two legs, per unbonded
/// foot, the shell (one leg) or the two-shell ring (two legs) its next site
/// must lie in, widened by the tolerance; for a relaxed local-phase parent,
/// the `reach` sphere around each unbonded foot's relaxed position.
fn shapes(
    plan: &SequentialPlan,
    config: &SequentialSearch,
    tree: &SearchTree,
    target: u32,
    structure: &AtomicStructure,
    form: DebugForm,
) -> Option<DebugShapes> {
    let setup = &plan.setup;
    let mut out = Vec::new();
    match tree.row(target).kind {
        RowKind::Root => out.extend(setup.feet.iter().map(|f| Shape::Ball {
            centre: f.position,
            radius: config.anchor_reach,
        })),
        RowKind::Foot(f) => out.push(Shape::Ball {
            centre: setup.feet[f as usize].position,
            radius: config.anchor_reach,
        }),
        RowKind::Leg { .. } => {
            let path = tree.path(target);
            let unbonded = (0..setup.feet.len()).filter(|f| path.iter().all(|l| l.foot != *f));
            if path.len() < plan.max_legs.min(GEOMETRIC_LEGS) {
                for f in unbonded {
                    out.push(Shape::Shells(
                        path.iter()
                            .map(|&b| shell(setup, b, f, config.tolerance))
                            .collect(),
                    ));
                }
            } else if path.len() < plan.max_legs && form == DebugForm::Relaxed {
                for f in unbonded {
                    let p = structure
                        .get_atom(setup.feet[f].id)
                        .map_or(setup.feet[f].position, |a| a.position);
                    out.push(Shape::Ball {
                        centre: p,
                        radius: config.reach,
                    });
                }
            }
        }
    }
    DebugShapes::new(out)
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

/// A row as the panel and the CLI name it: `root`, `foot O12`, or the leg it
/// adds, `O12–Si45`, with its transfer, `O12–Si45 (H→Si46)`.
pub fn row_label(setup: &Setup, tree: &SearchTree, row: u32) -> String {
    let name = |element: i16, id: u32| format!("{}{id}", element_symbol(element));
    match tree.row(row).kind {
        RowKind::Root => "root".to_string(),
        RowKind::Foot(f) => {
            let f = &setup.feet[f as usize];
            format!("foot {}", name(f.element, f.id))
        }
        RowKind::Leg {
            foot,
            site,
            acceptor,
        } => {
            let f = &setup.feet[foot as usize];
            let s = &setup.sites[site as usize];
            let mut text = format!("{}–{}", name(f.element, f.id), name(s.element, s.id));
            if let (Some(a), Some(x)) = (acceptor, f.donates) {
                let a = &setup.sites[a as usize];
                text.push_str(&format!(
                    " ({}→{})",
                    element_symbol(x),
                    name(a.element, a.id)
                ));
            }
            text
        }
    }
}

/// A row's path as [`find_row`] reads it: `root`, a foot's atom id (`12`),
/// or the legs along the path as `foot-site` atom-id pairs (`12-45,13-61`).
pub fn row_path(setup: &Setup, tree: &SearchTree, row: u32) -> String {
    match tree.row(row).kind {
        RowKind::Root => "root".to_string(),
        RowKind::Foot(f) => setup.feet[f as usize].id.to_string(),
        RowKind::Leg { .. } => tree
            .path(row)
            .iter()
            .map(|l| format!("{}-{}", setup.feet[l.foot].id, setup.sites[l.site].id))
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// The row a path names (the CLI's `debug-select`): `root` or empty, `#N`
/// for row `N`, a foot's atom id for its foot row, or the legs along the path
/// as `foot-site` pairs separated by `,` — `12-45,13-61` is the two-leg row
/// that bonds foot 12 to site 45, then foot 13 to site 61. Ids are the
/// combined ids the candidates' `sites` field uses; an element prefix
/// (`O12-Si45`) is allowed.
pub fn find_row(setup: &Setup, tree: &SearchTree, path: &str) -> Result<u32, String> {
    let path = path.trim();
    if path.is_empty() || path == "root" {
        return Ok(0);
    }
    if let Some(n) = path.strip_prefix('#') {
        let row: u32 = n
            .trim()
            .parse()
            .map_err(|_| format!("'{path}' is not a row number"))?;
        if row as usize >= tree.len() {
            return Err(format!("the search tree has no row {row}"));
        }
        return Ok(row);
    }
    let id = |text: &str| -> Result<u32, String> {
        text.trim()
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .parse()
            .map_err(|_| format!("'{text}' is not an atom id"))
    };
    let foot_of = |text: &str| -> Result<usize, String> {
        let id = id(text)?;
        setup
            .feet
            .iter()
            .position(|f| f.id == id)
            .ok_or_else(|| format!("atom {id} is not a foot"))
    };
    let child = |row: u32, want: &dyn Fn(RowKind) -> bool, what: &str| -> Result<u32, String> {
        tree.children(row)
            .iter()
            .copied()
            .find(|&c| want(tree.row(c).kind))
            .ok_or_else(|| format!("{} has no child {what}", row_label(setup, tree, row)))
    };
    let mut row = 0;
    for (i, step) in path.split(',').map(str::trim).enumerate() {
        let parts: Vec<&str> = step.split(['-', '–']).collect();
        let f = foot_of(parts[0])?;
        if i == 0 {
            row = child(
                row,
                &|k| k == RowKind::Foot(f as u32),
                &format!("foot {}", parts[0]),
            )?;
        }
        match parts.as_slice() {
            [_] if i == 0 => {}
            [_, s] => {
                let site_id = id(s)?;
                row = child(
                    row,
                    &|k| {
                        matches!(k, RowKind::Leg { foot, site, .. }
                            if foot as usize == f && setup.sites[site as usize].id == site_id)
                    },
                    step,
                )?;
            }
            _ => {
                return Err(format!(
                    "'{step}': a step is a foot-site pair; only the first may name a foot alone"
                ));
            }
        }
    }
    Ok(row)
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
        let r = tree.row(self.row);
        let strain = self
            .strain
            .map_or(String::new(), |e| format!(", strain {e:.2} kcal/mol"));
        let mut lines = vec![format!(
            "row #{}: {} ({} leg{}), shown {form}{strain}",
            self.row,
            row_label(setup, tree, self.row),
            r.legs,
            if r.legs == 1 { "" } else { "s" }
        )];
        lines.push(format!("path: {}", row_path(setup, tree, self.row)));
        let m = &self.marks;
        for (what, ids) in [
            ("bonded", &m.bonded),
            ("feet", &m.feet),
            ("candidates", &m.candidates),
            ("mirrored", &m.mirrored),
            ("undecided", &m.undecided),
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
