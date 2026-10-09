//! `chemisorb` — the ways a posed adsorbate can bond to a substrate, found
//! leg by leg, relaxed and ranked.
//!
//! A thin adapter over `atomcad_crystolecule::chemisorption::sequential`
//! (design `design_chemisorption_sequential.md`): this file reads pins and
//! properties, builds a `SequentialSearch`, and maps a report onto two
//! outputs. It holds no chemistry.
//!
//! **Evaluation never searches.** A search relaxes hundreds of structures, and
//! the evaluator re-evaluates a node on far more than its own edits
//! (selection, downstream edits, every full refresh), so `eval` only ever runs
//! the cheap `plan` (the geometric legs, seating and the clash and mirror
//! checks, nothing relaxed) and reads a stored result. The search itself runs
//! on an explicit action — the panel's Run button, the CLI's `run` — which is
//! this node's **node job** (`prepare_job`; the halves are in
//! `chemisorb_ops.rs`, the layer in `node_jobs/`): the search runs off the UI
//! thread and its result is installed on the node when it lands.
//!
//! **The stored result is a cache keyed by an input fingerprint.**
//! [`ChemisorbData::stored`] holds the last report with the
//! `input_fingerprint` of the inputs it was computed from. `#[serde(skip)]`,
//! not undoable, not saved. On every evaluation the node fingerprints its
//! current inputs: a match outputs the stored result; anything else outputs
//! the plan (`stats.searched = false`, and `stale = true` when a stored result
//! exists for other inputs). A stale result is never output.
//!
//! Reading `stored` from `eval` goes against the rule that node data is shared
//! by every call site of a subnetwork and must not be read in `eval`. It is
//! safe here for the same reason the fingerprint exists: a result that belongs
//! to another call site, or to an older pose, can only fail to match — it is
//! never output for inputs it was not computed from.
//!
//! **Every property is a search setting.** `formed_bonds` and
//! `bond_inventory` restrict which hypotheses are candidates (so only that
//! group is relaxed), and `top_n` / `energy_window` what is kept while relaxing
//! (so only the listed structures are ever held); all are fingerprinted, and a
//! change makes a stored result stale like any other. The filters exist because
//! the ranking is UFF energy alone: fewer bonds rank first, and strains compare
//! cleanly only within one bond inventory.
//!
//! The `bond_inventory` dropdown's choices come from a second `plan` without
//! the inventory filter — with it, the plan would only ever contain the
//! current choice. The plan covers legs 1–3 only; deeper inventories are
//! listed once a matching unfiltered result exists.
//!
//! Bond forming is always on; H transfer is enabled by wiring the `transfers`
//! pin, an array of `ChemisorbTransfer { element, direction }` records. Only
//! `to_substrate` is supported: an OH foot hands its H to the site nearest the
//! one it bonds to (the search's one deterministic rule).
//!
//! The panel's data goes into `context.selected_node_eval_cache`
//! ([`ChemisorbEvalCache`]) on root evaluations, as `proxy` and `relax` do.
//!
//! **The debug view** (pins 2 and 3, design §6.5) shows one row of the search
//! tree: `debug` the row's structure with its markings, `debug_shapes` its
//! search shapes. Selecting a row is an action (`chemisorb_ops.rs`), never
//! evaluation: it builds the view — relaxing, as a node job, when the row is a
//! relaxed one that is not a kept candidate — and stores it with the input
//! fingerprint ([`ChemisorbData::debug`], like the search result: not saved,
//! not undoable, and not fingerprinted itself). Evaluation outputs the stored
//! view while the fingerprint matches, and otherwise the **root view** (the
//! posed inputs, the feet, their anchor sites, the `anchor_reach` spheres),
//! which it builds itself: geometry alone, as cheap as `plan`, so it follows
//! `anchor_reach` live with no click and no Run.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::{MoleculeData, NetworkResult, first_array_element_error};
use crate::node_data::{EvalOutput, NodeData};
use crate::node_jobs::{JobInputs, JobWork};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::atomic_constants::element_symbol;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::sequential::{
    Candidate, ChildVerdict, DebugForm, DebugShapes, DebugView, LevelStats, Mirror, NO_ROW,
    PlanStats, RowKind, SHAPE_ALPHA, SHAPE_COLOR, SHAPE_LEVEL, SearchReport, SearchTree,
    SequentialPlan, SequentialSearch, child_verdict, plan, relaxation, root_view, row_forms,
    row_label, row_path, shown_row,
};
use atomcad_crystolecule::chemisorption::{
    BondInventory, TransferDirection, TransferRule, input_fingerprint, inventory_options,
    is_transferable_element,
};
use atomcad_crystolecule::field::isosurface::{IsosurfaceColoring, IsosurfaceData, LevelBasis};
use atomcad_crystolecule::simulation::uff::VdwMode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const CHEMISORB_TRANSFER_RECORD: &str = "ChemisorbTransfer";
pub const CHEMISORB_CANDIDATE_RECORD: &str = "ChemisorbCandidate";
pub const CHEMISORB_STRAIN_TERMS_RECORD: &str = "ChemisorbStrainTerms";
pub const CHEMISORB_STATS_RECORD: &str = "ChemisorbStats";
pub const CHEMISORB_LEVEL_RECORD: &str = "ChemisorbLevel";

/// Input pin indices.
pub const ADSORBATE_INPUT_PIN: usize = 0;
pub const SUBSTRATE_INPUT_PIN: usize = 1;
pub const TRANSFERS_INPUT_PIN: usize = 2;

/// Output pin indices.
pub const CANDIDATES_OUTPUT_PIN: usize = 0;
pub const STATS_OUTPUT_PIN: usize = 1;
pub const DEBUG_OUTPUT_PIN: usize = 2;
pub const DEBUG_SHAPES_OUTPUT_PIN: usize = 3;

/// The van der Waals cutoff `relax` uses when the preference asks for one.
const VDW_CUTOFF: f64 = 6.0;

fn default_anchor_reach() -> f64 {
    SequentialSearch::default().anchor_reach
}
fn default_tolerance() -> f64 {
    SequentialSearch::default().tolerance
}
fn default_reach() -> f64 {
    SequentialSearch::default().reach
}
fn default_clash_filter() -> bool {
    SequentialSearch::default().clash_filter
}
/// "No cap".
fn no_cap() -> i32 {
    -1
}
fn default_top_n() -> i32 {
    10
}
fn default_energy_window() -> f64 {
    30.0
}
fn default_budget() -> i32 {
    10_000
}
fn default_max_iterations() -> i32 {
    2000
}

/// A finished search and the fingerprint of the inputs it was computed from.
/// The plan is kept beside the report: the report numbers its hypotheses
/// after the plan's, and its tree extends the plan's.
#[derive(Debug)]
pub struct StoredSearch {
    pub fingerprint: u64,
    pub plan: SequentialPlan,
    pub report: SearchReport,
}

/// The debug view of one tree row, built by the select action
/// (`chemisorb_ops.rs`) for inputs with this fingerprint.
#[derive(Debug)]
pub struct StoredDebug {
    pub fingerprint: u64,
    pub view: DebugView,
}

/// Node data for `chemisorb`: the search settings (all persisted, all text
/// properties), the last search result and the debug view (neither).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChemisorbData {
    /// Adsorbate reactive atoms: those carrying this tag. Empty = all atoms.
    #[serde(default)]
    pub adsorbate_tag: String,
    /// Substrate reactive atoms: those carrying this tag. Empty = all atoms.
    #[serde(default)]
    pub substrate_tag: String,
    /// Leg 1: sites within this distance of a posed foot (Å).
    #[serde(default = "default_anchor_reach")]
    pub anchor_reach: f64,
    /// Legs 2 and 3: how far a site may lie outside the exact shell or ring
    /// the foot spacing and bond lengths allow (Å) — the molecule's own flex.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Legs 4 and later: sites within this of a relaxed foot (Å); also how far
    /// a transferred H may go from its foot's site, site to site.
    #[serde(default = "default_reach")]
    pub reach: f64,
    /// Prune seatings that clash instead of relaxing them; clashes are
    /// reported either way.
    #[serde(default = "default_clash_filter")]
    pub clash_filter: bool,
    /// At most this many legs (bonds formed) per hypothesis: `-1` = no cap.
    /// The panel shows it as a checkbox plus a value; `-1` is the unticked
    /// state.
    #[serde(default = "no_cap")]
    pub max_formed_bonds: i32,
    /// Only bindings with exactly this many legs are candidates. `None` = any
    /// count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formed_bonds: Option<i32>,
    /// Only bindings with exactly this bond inventory are candidates, written
    /// as a candidate's `bonds` field reads, e.g. `"formed 2× O–Si"`. `None` =
    /// any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_inventory: Option<String>,
    /// At most this many candidates are kept and listed…
    #[serde(default = "default_top_n")]
    pub top_n: i32,
    /// …and only those within this many kcal/mol of the best.
    #[serde(default = "default_energy_window")]
    pub energy_window: f64,
    /// At most this many relaxations; past it the search is truncated and not
    /// exhaustive.
    #[serde(default = "default_budget")]
    pub budget: i32,
    /// UFF iteration limit per relaxation.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: i32,
    /// The last search, written only by the node job's install
    /// (`ChemisorbOutcome`, `chemisorb_ops.rs`).
    /// Never saved and never an undo step: it is a pure function of the
    /// inputs, and the fingerprint keeps a copied or outdated one harmless.
    #[serde(skip)]
    pub stored: Option<Arc<StoredSearch>>,
    /// The selected row's debug view, written only by the select action. Like
    /// `stored`: never saved, never an undo step, output only while the
    /// fingerprint matches. A selection is UI state, not a search setting, so
    /// nothing fingerprints it.
    #[serde(skip)]
    pub debug: Option<Arc<StoredDebug>>,
}

impl Default for ChemisorbData {
    fn default() -> Self {
        Self {
            adsorbate_tag: String::new(),
            substrate_tag: String::new(),
            anchor_reach: default_anchor_reach(),
            tolerance: default_tolerance(),
            reach: default_reach(),
            clash_filter: default_clash_filter(),
            max_formed_bonds: no_cap(),
            formed_bonds: None,
            bond_inventory: None,
            top_n: default_top_n(),
            energy_window: default_energy_window(),
            budget: default_budget(),
            max_iterations: default_max_iterations(),
            stored: None,
            debug: None,
        }
    }
}

/// One local-phase level (legs 4, 5, …), as the `stats` pin and the panel
/// show it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChemisorbLevelView {
    pub legs: usize,
    pub parents: usize,
    pub paths: usize,
    pub hypotheses: usize,
    pub duplicates: usize,
    pub pruned_valence: usize,
    pub pruned_no_acceptor: usize,
    pub pruned_filter: usize,
    pub candidates: usize,
    pub to_relax: usize,
    pub relaxed: usize,
    pub unconverged: usize,
    pub truncated: bool,
    pub near_misses: usize,
}

impl From<&LevelStats> for ChemisorbLevelView {
    fn from(l: &LevelStats) -> Self {
        Self {
            legs: l.legs,
            parents: l.parents,
            paths: l.paths,
            hypotheses: l.hypotheses,
            duplicates: l.duplicates,
            pruned_valence: l.pruned_valence,
            pruned_no_acceptor: l.pruned_no_acceptor,
            pruned_filter: l.pruned_filter,
            candidates: l.candidates,
            to_relax: l.to_relax,
            relaxed: l.relaxed,
            unconverged: l.unconverged,
            truncated: l.truncated,
            near_misses: l.near_misses,
        }
    }
}

/// The whole search, as the `stats` pin and the panel show it. The plan's
/// counts cover the geometric legs (1–3); `local` the local phase, known only
/// after Run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChemisorbStatsView {
    /// Adsorbate atoms that can bond: a free valence, or an H to donate.
    pub feet: usize,
    /// Substrate atoms with a free valence.
    pub sites: usize,
    /// Every (foot, site) choice that passed its level's test.
    pub paths: usize,
    /// Distinct one-, two- and three-leg hypotheses.
    pub anchors: usize,
    pub sphere_pairs: usize,
    pub torus_triples: usize,
    pub duplicates: usize,
    pub pruned_valence: usize,
    pub pruned_no_acceptor: usize,
    /// Rejected by `bond_inventory` while enumerating.
    pub pruned_filter: usize,
    pub pruned_mirror: usize,
    pub mirror_undecided: usize,
    /// Hypotheses the filters admit (legs 1–3).
    pub candidates: usize,
    /// Three-leg hypotheses relaxed only to grow the local phase from.
    pub parents: usize,
    /// Seatings that clash (always counted), and those pruned for it (only
    /// with `clash_filter` on).
    pub seating_clashes: usize,
    pub pruned_clash: usize,
    /// A local phase (legs 4+) follows the geometric one.
    pub local_phase: bool,
    /// The outputs are a search result for the current inputs.
    pub searched: bool,
    /// A stored result exists but was computed from other inputs.
    pub stale: bool,
    /// Relaxations run (both phases), and those of the local phase.
    pub relaxed: usize,
    pub local_relaxed: usize,
    /// The relaxations the geometric phase needs; the local phase's are known
    /// only after Run.
    pub to_relax: usize,
    pub unconverged: usize,
    /// Candidates kept: the best `top_n` within the window.
    pub listed: usize,
    /// The budget was hit; the search is **not** exhaustive.
    pub truncated: bool,
    /// Wall time of the search, or of `plan` when `searched` is false (s).
    pub seconds: f64,
    /// Per local level, in order; empty before Run or without a local phase.
    pub local: Vec<ChemisorbLevelView>,
}

impl ChemisorbStatsView {
    fn of_plan(p: &PlanStats) -> Self {
        Self {
            feet: p.feet,
            sites: p.sites,
            paths: p.paths,
            anchors: p.anchors,
            sphere_pairs: p.sphere_pairs,
            torus_triples: p.torus_triples,
            duplicates: p.duplicates,
            pruned_valence: p.pruned_valence,
            pruned_no_acceptor: p.pruned_no_acceptor,
            pruned_filter: p.pruned_filter,
            pruned_mirror: p.pruned_mirror,
            mirror_undecided: p.mirror_undecided,
            candidates: p.candidates,
            parents: p.parents,
            seating_clashes: p.seating_clashes,
            pruned_clash: p.pruned_clash,
            local_phase: p.local_phase,
            to_relax: p.to_relax,
            truncated: p.truncated,
            seconds: p.seconds,
            ..Self::default()
        }
    }
}

/// One listed candidate, as the `candidates` pin and the panel show it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChemisorbRowView {
    pub rank: usize,
    /// UFF energy against the separated state — the adsorbate and the
    /// substrate each relaxed alone (kcal/mol): the ranking key, lower is
    /// better.
    pub strain: f64,
    /// The bond inventory, e.g. `"formed 3× O–Si"`.
    pub bonds: String,
    /// The formed bonds in binding order, by atom id in the output structure,
    /// then the transfers, e.g. `"O12–Si45, O13–Si47; H14 O13→Si48"`.
    pub sites: String,
    /// The leg count (transfers not counted).
    pub formed_bonds: usize,
    pub transfers: usize,
    pub converged: bool,
    /// Its start geometry clashed (only possible with `clash_filter` off).
    pub seating_clash: bool,
    pub worst_bond_ratio: f64,
    pub stretch: f64,
    pub bend: f64,
    pub torsion: f64,
    pub inversion: f64,
    pub vdw: f64,
}

/// What the properties panel reads after a root evaluation of the selected
/// node.
#[derive(Debug, Clone)]
pub struct ChemisorbEvalCache {
    pub stats: ChemisorbStatsView,
    pub rows: Vec<ChemisorbRowView>,
    /// The `bond_inventory` choices: each distinct bond inventory with how
    /// many relaxations it takes, from a plan without the inventory filter
    /// (but with `formed_bonds`), by leg count then label. Legs 4 and later
    /// appear once a matching result without the inventory filter exists.
    pub inventory_options: Vec<(String, usize)>,
    /// `reach` is read: a local phase follows, or a transfer rule is wired.
    /// The panel greys it out otherwise.
    pub reach_used: bool,
    /// What the debug tree is read from.
    pub tree: ChemisorbTree,
    /// The row the debug pins show and its form; `None` = the root view.
    pub debug_selected: Option<(u32, DebugForm)>,
    /// Identifies the tree: the input fingerprint. Row numbers stay valid while
    /// it holds (a run only appends the local rows), so the panel drops what it
    /// fetched of the tree when it, or `stats.searched`, changes.
    pub tree_key: u64,
}

/// A search tree and what its rows refer to, shared with the node through an
/// `Arc`, never copied: the plan of this evaluation, or the stored search
/// matching the inputs (whose tree extends the plan's with the local phase).
#[derive(Debug, Clone)]
pub enum ChemisorbTree {
    Plan {
        plan: Arc<SequentialPlan>,
        clash_filter: bool,
    },
    Search {
        search: Arc<StoredSearch>,
        clash_filter: bool,
    },
}

impl ChemisorbTree {
    pub fn plan(&self) -> &SequentialPlan {
        match self {
            ChemisorbTree::Plan { plan, .. } => plan,
            ChemisorbTree::Search { search, .. } => &search.plan,
        }
    }

    pub fn report(&self) -> Option<&SearchReport> {
        match self {
            ChemisorbTree::Plan { .. } => None,
            ChemisorbTree::Search { search, .. } => Some(&search.report),
        }
    }

    pub fn tree(&self) -> &SearchTree {
        self.report().map_or(&self.plan().tree, |r| &r.tree)
    }

    fn clash_filter(&self) -> bool {
        match self {
            ChemisorbTree::Plan { clash_filter, .. }
            | ChemisorbTree::Search { clash_filter, .. } => *clash_filter,
        }
    }
}

/// What a debug row stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChemisorbDebugRowKind {
    Root,
    Foot,
    Leg,
}

/// One row of the debug tree, as the panel lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChemisorbDebugRowView {
    pub row: u32,
    pub parent: Option<u32>,
    pub kind: ChemisorbDebugRowKind,
    pub legs: u32,
    /// `root`, `foot O12`, or the leg the row adds, `O12–Si45`.
    pub label: String,
    /// The CLI's path to the row (`12-45,13-61`).
    pub path: String,
    /// A duplicate path: the canonical row of its change set.
    pub duplicate_of: Option<u32>,
    /// Children listed with duplicates hidden, and the duplicates hidden.
    pub children: u32,
    pub hidden_duplicates: u32,
    /// The children by the verdict on what they reach (duplicates included).
    pub candidates: u32,
    pub mirrored: u32,
    pub undecided: u32,
    pub clashes: u32,
    /// Children rejected for want of valence, of an H acceptor, by the
    /// inventory filter.
    pub rejected_valence: u32,
    pub rejected_no_acceptor: u32,
    pub rejected_filter: u32,
    /// The sites just outside the next leg's test, e.g. `Si52 +0.27`.
    pub near_misses: Vec<String>,
    /// The row's own hypothesis: a candidate, a local-phase parent, its
    /// mirror verdict (three legs), whether its seating clashes and whether
    /// that pruned it.
    pub candidate: bool,
    pub local_parent: bool,
    pub mirror: Option<Mirror>,
    pub seating_clash: bool,
    pub pruned_clash: bool,
    /// The relaxation recorded for this path (a local duplicate has its own;
    /// a geometric one shares its canonical row's), kcal/mol.
    pub strain: Option<f64>,
    pub converged: bool,
    /// The row was to be relaxed and the budget ran out first.
    pub budget_cut: bool,
    /// The forms it can be shown in, and the one it opens on.
    pub can_seat: bool,
    pub can_relax: bool,
    pub default_form: DebugForm,
}

impl ChemisorbEvalCache {
    /// Row `row` of the debug tree, `None` past its end.
    pub fn debug_row(&self, row: u32) -> Option<ChemisorbDebugRowView> {
        ((row as usize) < self.tree.tree().len()).then(|| debug_row_view(&self.tree, row))
    }

    /// The children of `row` the panel lists: duplicates only when asked. A
    /// display option, nothing more: the tree is the same either way.
    pub fn debug_children(&self, row: u32, show_duplicates: bool) -> Vec<ChemisorbDebugRowView> {
        let tree = self.tree.tree();
        if row as usize >= tree.len() {
            return Vec::new();
        }
        tree.visible_children(row, show_duplicates)
            .into_iter()
            .map(|c| debug_row_view(&self.tree, c))
            .collect()
    }

    /// The rows from the root down to `row`, both included: what the panel
    /// expands to reveal it.
    pub fn debug_ancestors(&self, row: u32) -> Vec<u32> {
        let tree = self.tree.tree();
        if row as usize >= tree.len() {
            return Vec::new();
        }
        let mut out = vec![row];
        let mut r = row;
        while tree.row(r).parent != NO_ROW {
            r = tree.row(r).parent;
            out.push(r);
        }
        out.reverse();
        out
    }
}

/// The panel's view of one row: the tree's counts, and what became of the
/// row's own hypothesis.
fn debug_row_view(source: &ChemisorbTree, row: u32) -> ChemisorbDebugRowView {
    let plan = source.plan();
    let report = source.report();
    let tree = source.tree();
    let r = tree.row(row);
    let forms = row_forms(plan, report, row);
    let mut v = ChemisorbDebugRowView {
        row,
        parent: (r.parent != NO_ROW).then_some(r.parent),
        kind: match r.kind {
            RowKind::Root => ChemisorbDebugRowKind::Root,
            RowKind::Foot(_) => ChemisorbDebugRowKind::Foot,
            RowKind::Leg { .. } => ChemisorbDebugRowKind::Leg,
        },
        legs: r.legs as u32,
        label: row_label(&plan.setup, tree, row),
        path: row_path(&plan.setup, tree, row),
        duplicate_of: r.duplicate_of,
        children: 0,
        hidden_duplicates: r.duplicates,
        candidates: 0,
        mirrored: 0,
        undecided: 0,
        clashes: 0,
        rejected_valence: r.rejected_valence,
        rejected_no_acceptor: r.rejected_no_acceptor,
        rejected_filter: r.rejected_filter,
        near_misses: tree
            .near_misses(row)
            .iter()
            .map(|n| {
                let site = &plan.setup.sites[n.site as usize];
                format!("{}{} +{:.2}", element_symbol(site.element), site.id, n.miss)
            })
            .collect(),
        candidate: false,
        local_parent: false,
        mirror: None,
        seating_clash: false,
        pruned_clash: false,
        strain: None,
        converged: false,
        budget_cut: false,
        can_seat: forms.seated,
        can_relax: forms.relaxed,
        default_form: forms.default,
    };
    for &c in tree.children(row) {
        if tree.row(c).duplicate_of.is_none() {
            v.children += 1;
        }
        match child_verdict(plan, report, c) {
            ChildVerdict::Candidate => v.candidates += 1,
            ChildVerdict::Mirrored => v.mirrored += 1,
            ChildVerdict::Undecided => v.undecided += 1,
            ChildVerdict::Clash => v.clashes += 1,
        }
    }
    let canonical = shown_row(tree, row);
    if let Some(index) = tree.row(canonical).hypothesis {
        let h = match report {
            Some(rep) => rep.hypothesis(plan, index as usize),
            None => &plan.hypotheses[index as usize],
        };
        v.candidate = h.candidate;
        v.local_parent = h.parent;
        v.mirror = h.mirror;
        v.seating_clash = h.seating.as_ref().is_some_and(|s| s.clashes());
        v.pruned_clash = v.seating_clash && source.clash_filter();
    }
    if let Some(rep) = report {
        if let Some(x) = relaxation(rep, row).or_else(|| relaxation(rep, canonical)) {
            v.strain = Some(x.strain);
            v.converged = x.converged;
        }
        v.budget_cut = (v.candidate || v.local_parent)
            && !v.pruned_clash
            && v.mirror != Some(Mirror::Mirrored)
            && relaxation(rep, canonical).is_none();
    }
    v
}

impl ChemisorbData {
    /// The search settings, validated in the node's own words. `use_vdw_cutoff`
    /// is the simulation preference `relax` reads too; `transfers` is what the
    /// `transfers` pin carries (empty when it is disconnected).
    pub fn search_config(
        &self,
        use_vdw_cutoff: bool,
        transfers: Vec<TransferRule>,
    ) -> Result<SequentialSearch, String> {
        if self.max_formed_bonds != -1 && self.max_formed_bonds < 1 {
            return Err(
                "chemisorb: max_formed_bonds must be -1 (no cap) or at least 1".to_string(),
            );
        }
        if self.budget < 1 {
            return Err("chemisorb: budget must be at least 1".to_string());
        }
        if self.max_iterations < 1 {
            return Err("chemisorb: max_iterations must be at least 1".to_string());
        }
        if self.top_n < 1 {
            return Err("chemisorb: top_n must be at least 1".to_string());
        }
        if self.formed_bonds.is_some_and(|n| n < 1) {
            return Err(
                "chemisorb: formed_bonds must be at least 1 (leave it unset for any)".to_string(),
            );
        }
        if !(self.energy_window.is_finite() && self.energy_window >= 0.0) {
            return Err("chemisorb: energy_window must be >= 0".to_string());
        }
        let bond_inventory = self
            .bond_inventory
            .as_deref()
            .map(str::parse::<BondInventory>)
            .transpose()
            .map_err(|e| format!("chemisorb: bond_inventory: {e}"))?;
        let tag = |t: &str| {
            let t = t.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let config = SequentialSearch {
            adsorbate_tag: tag(&self.adsorbate_tag),
            substrate_tag: tag(&self.substrate_tag),
            anchor_reach: self.anchor_reach,
            tolerance: self.tolerance,
            reach: self.reach,
            clash_filter: self.clash_filter,
            transfers,
            max_formed_bonds: (self.max_formed_bonds >= 1)
                .then_some(self.max_formed_bonds as usize),
            formed_bonds: self.formed_bonds.map(|n| n as usize),
            bond_inventory,
            budget: self.budget as usize,
            top_n: self.top_n as usize,
            energy_window: self.energy_window,
            max_iterations: self.max_iterations as u32,
            vdw_mode: if use_vdw_cutoff {
                VdwMode::Cutoff(VDW_CUTOFF)
            } else {
                VdwMode::AllPairs
            },
            ..SequentialSearch::default()
        };
        config.validate().map_err(|e| format!("chemisorb: {e}"))?;
        Ok(config)
    }

    /// The stored search, when it was computed from inputs with this
    /// fingerprint.
    pub fn matching_search(&self, fingerprint: u64) -> Option<&StoredSearch> {
        self.stored
            .as_deref()
            .filter(|s| s.fingerprint == fingerprint)
    }
}

/// The `transfers` pin's records. `None` (disconnected) and an empty array
/// both mean no transfers. An upstream error comes back verbatim, as the
/// other inputs' do. A `to_substrate`-only check is the config's
/// (`SequentialSearch::validate`), so its message names the record.
pub fn transfer_rules(value: NetworkResult) -> Result<Vec<TransferRule>, String> {
    let items = match value {
        NetworkResult::None => return Ok(Vec::new()),
        NetworkResult::Error(message) => return Err(message),
        NetworkResult::Array(items) => items,
        other => {
            return Err(format!(
                "chemisorb: transfers must be an array of ChemisorbTransfer records, got {:?}",
                other.infer_data_type()
            ));
        }
    };
    if let Some(NetworkResult::Error(message)) = first_array_element_error("transfers", &items) {
        return Err(message);
    }
    let mut rules = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let element = match item.extract_record_field("element") {
            Some(NetworkResult::Int(z)) => *z,
            other => {
                return Err(format!(
                    "chemisorb.transfers[{i}].element: expected Int, got {:?}",
                    other.map(NetworkResult::infer_data_type)
                ));
            }
        };
        let element = i16::try_from(element)
            .ok()
            .filter(|&z| is_transferable_element(z))
            .ok_or_else(|| {
                format!(
                    "chemisorb.transfers[{i}].element: {element} is not H or a halogen; \
                     only a monovalent atom can transfer"
                )
            })?;
        let direction = match item.extract_record_field("direction") {
            Some(NetworkResult::String(text)) => {
                TransferDirection::parse(text).ok_or_else(|| {
                    format!(
                        "chemisorb.transfers[{i}].direction: '{text}' is neither \
                     'to_substrate' nor 'to_adsorbate'"
                    )
                })?
            }
            other => {
                return Err(format!(
                    "chemisorb.transfers[{i}].direction: expected String, got {:?}",
                    other.map(NetworkResult::infer_data_type)
                ));
            }
        };
        rules.push(TransferRule { element, direction });
    }
    Ok(rules)
}

/// The two atomic inputs, or the message to output on every pin: an upstream
/// error's own text (forwarded verbatim, never re-wrapped), or a type
/// complaint.
pub fn atomic_inputs(
    adsorbate: NetworkResult,
    substrate: NetworkResult,
) -> Result<(AtomicStructure, AtomicStructure), String> {
    let mut out = Vec::with_capacity(2);
    for (name, value) in [("adsorbate", adsorbate), ("substrate", substrate)] {
        if let NetworkResult::Error(message) = value {
            return Err(message);
        }
        let kind = value.infer_data_type();
        match value.extract_atomic() {
            Some(atoms) => out.push(atoms),
            None => {
                return Err(format!(
                    "chemisorb: {name} must be an atomic structure, got {kind:?}"
                ));
            }
        }
    }
    let substrate = out.pop().expect("two inputs");
    let adsorbate = out.pop().expect("two inputs");
    Ok((adsorbate, substrate))
}

fn molecule(atoms: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

fn row_view(rank: usize, c: &Candidate) -> ChemisorbRowView {
    let label = |id: u32| {
        let z = c.structure.get_atom(id).map_or(0, |a| a.atomic_number);
        format!("{}{id}", element_symbol(z))
    };
    let formed = c
        .formed
        .iter()
        .map(|&(a, s)| format!("{}–{}", label(a), label(s)))
        .collect::<Vec<_>>()
        .join(", ");
    let moves = c
        .transfers
        .iter()
        .map(|t| {
            format!(
                "{} {}→{}",
                label(t.moved),
                label(t.donor),
                label(t.acceptor)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let sites = [formed, moves]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    ChemisorbRowView {
        rank,
        strain: c.strain,
        bonds: c.bond_inventory.to_string(),
        sites,
        formed_bonds: c.formed.len(),
        transfers: c.transfers.len(),
        converged: c.converged,
        seating_clash: c.seating_clash,
        worst_bond_ratio: c.worst_bond_ratio,
        stretch: c.terms.stretch,
        bend: c.terms.bend,
        torsion: c.terms.torsion,
        inversion: c.terms.inversion,
        vdw: c.terms.vdw,
    }
}

fn candidate_record(row: &ChemisorbRowView, structure: &AtomicStructure) -> NetworkResult {
    let float = NetworkResult::Float;
    let int = |n: usize| NetworkResult::Int(n as i32);
    NetworkResult::record(vec![
        ("structure".to_string(), molecule(structure.clone())),
        ("rank".to_string(), int(row.rank)),
        ("strain".to_string(), float(row.strain)),
        (
            "bonds".to_string(),
            NetworkResult::String(row.bonds.clone()),
        ),
        (
            "sites".to_string(),
            NetworkResult::String(row.sites.clone()),
        ),
        ("formed_bonds".to_string(), int(row.formed_bonds)),
        ("transfers".to_string(), int(row.transfers)),
        ("converged".to_string(), NetworkResult::Bool(row.converged)),
        (
            "seating_clash".to_string(),
            NetworkResult::Bool(row.seating_clash),
        ),
        ("worst_bond_ratio".to_string(), float(row.worst_bond_ratio)),
        (
            "terms".to_string(),
            NetworkResult::record(vec![
                ("stretch".to_string(), float(row.stretch)),
                ("bend".to_string(), float(row.bend)),
                ("torsion".to_string(), float(row.torsion)),
                ("inversion".to_string(), float(row.inversion)),
                ("vdw".to_string(), float(row.vdw)),
            ]),
        ),
    ])
}

fn level_record(l: &ChemisorbLevelView) -> NetworkResult {
    let int = |n: usize| NetworkResult::Int(n as i32);
    NetworkResult::record(vec![
        ("legs".to_string(), int(l.legs)),
        ("parents".to_string(), int(l.parents)),
        ("paths".to_string(), int(l.paths)),
        ("hypotheses".to_string(), int(l.hypotheses)),
        ("duplicates".to_string(), int(l.duplicates)),
        ("pruned_valence".to_string(), int(l.pruned_valence)),
        ("pruned_no_acceptor".to_string(), int(l.pruned_no_acceptor)),
        ("pruned_filter".to_string(), int(l.pruned_filter)),
        ("candidates".to_string(), int(l.candidates)),
        ("to_relax".to_string(), int(l.to_relax)),
        ("relaxed".to_string(), int(l.relaxed)),
        ("unconverged".to_string(), int(l.unconverged)),
        ("truncated".to_string(), NetworkResult::Bool(l.truncated)),
        ("near_misses".to_string(), int(l.near_misses)),
    ])
}

fn stats_record(s: &ChemisorbStatsView) -> NetworkResult {
    let int = |n: usize| NetworkResult::Int(n as i32);
    let boolean = NetworkResult::Bool;
    NetworkResult::record(vec![
        ("feet".to_string(), int(s.feet)),
        ("sites".to_string(), int(s.sites)),
        ("paths".to_string(), int(s.paths)),
        ("anchors".to_string(), int(s.anchors)),
        ("sphere_pairs".to_string(), int(s.sphere_pairs)),
        ("torus_triples".to_string(), int(s.torus_triples)),
        ("duplicates".to_string(), int(s.duplicates)),
        ("pruned_valence".to_string(), int(s.pruned_valence)),
        ("pruned_no_acceptor".to_string(), int(s.pruned_no_acceptor)),
        ("pruned_filter".to_string(), int(s.pruned_filter)),
        ("pruned_mirror".to_string(), int(s.pruned_mirror)),
        ("mirror_undecided".to_string(), int(s.mirror_undecided)),
        ("candidates".to_string(), int(s.candidates)),
        ("parents".to_string(), int(s.parents)),
        ("seating_clashes".to_string(), int(s.seating_clashes)),
        ("pruned_clash".to_string(), int(s.pruned_clash)),
        ("local_phase".to_string(), boolean(s.local_phase)),
        ("searched".to_string(), boolean(s.searched)),
        ("stale".to_string(), boolean(s.stale)),
        ("relaxed".to_string(), int(s.relaxed)),
        ("local_relaxed".to_string(), int(s.local_relaxed)),
        ("to_relax".to_string(), int(s.to_relax)),
        ("unconverged".to_string(), int(s.unconverged)),
        ("listed".to_string(), int(s.listed)),
        ("truncated".to_string(), boolean(s.truncated)),
        ("seconds".to_string(), NetworkResult::Float(s.seconds)),
        (
            "local".to_string(),
            NetworkResult::Array(s.local.iter().map(level_record).collect()),
        ),
    ])
}

/// The inventories `plan` would relax as candidates, with how many of each.
fn plan_inventories(p: &SequentialPlan) -> impl Iterator<Item = (&BondInventory, usize)> {
    p.to_relax
        .iter()
        .map(|&i| &p.hypotheses[i])
        .filter(|h| h.candidate)
        .map(|h| (&h.inventory, h.legs()))
}

/// The `debug_shapes` output: the shapes as a translucent isosurface, or
/// nothing.
pub fn shapes_result(shapes: &Option<DebugShapes>) -> NetworkResult {
    match shapes {
        Some(shapes) => NetworkResult::Isosurface(IsosurfaceData {
            field: Arc::new(shapes.clone()),
            level: SHAPE_LEVEL,
            level_basis: LevelBasis::Absolute,
            coloring: IsosurfaceColoring::Phase {
                positive: SHAPE_COLOR,
                negative: SHAPE_COLOR,
            },
            alpha: SHAPE_ALPHA,
        }),
        None => NetworkResult::None,
    }
}

/// The four outputs and the panel's data for one evaluation, from the current
/// inputs and whatever is stored. Relaxes nothing.
pub fn chemisorb_outputs(
    data: &ChemisorbData,
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &SequentialSearch,
) -> Result<(Vec<NetworkResult>, ChemisorbEvalCache), String> {
    let fingerprint = input_fingerprint(adsorbate, substrate, config);
    let stored_arc = data
        .stored
        .as_ref()
        .filter(|s| s.fingerprint == fingerprint);
    let stored = stored_arc.map(|s| &**s);

    // The plan (the stored one when it matches: the same inputs make the same
    // plan), and the dropdown's choices from a plan without the inventory
    // filter. Cheap: nothing is relaxed.
    let tree = match stored_arc {
        Some(s) => ChemisorbTree::Search {
            search: s.clone(),
            clash_filter: config.clash_filter,
        },
        None => ChemisorbTree::Plan {
            plan: Arc::new(
                plan(adsorbate, substrate, config).map_err(|e| format!("chemisorb: {e}"))?,
            ),
            clash_filter: config.clash_filter,
        },
    };
    let planned = tree.plan();

    // The debug pins: the selected row while it matches, else the root view.
    let selected = data.debug.as_ref().filter(|d| d.fingerprint == fingerprint);
    let debug_selected = selected.map(|d| (d.view.row, d.view.form));
    let root;
    let view = match selected {
        Some(d) => &d.view,
        None => {
            root = root_view(planned, config);
            &root
        }
    };
    let debug_outputs = [
        molecule(view.structure.clone()),
        shapes_result(&view.shapes),
    ];
    let inventory_options = if config.bond_inventory.is_some() {
        let unfiltered = SequentialSearch {
            bond_inventory: None,
            ..config.clone()
        };
        let all = plan(adsorbate, substrate, &unfiltered).map_err(|e| format!("chemisorb: {e}"))?;
        inventory_options(plan_inventories(&all), None)
    } else {
        // Without the filter, a result also knows the local phase's
        // inventories.
        let local = stored.into_iter().flat_map(|s| {
            s.report
                .local
                .iter()
                .filter(|h| h.candidate)
                .map(|h| (&h.inventory, h.legs()))
        });
        inventory_options(plan_inventories(planned).chain(local), None)
    };
    let reach_used = planned.stats.local_phase || !config.transfers.is_empty();

    if let Some(stored) = stored {
        let report = &stored.report;
        let rows: Vec<ChemisorbRowView> = report
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| row_view(i + 1, c))
            .collect();
        let s = &report.stats;
        let stats = ChemisorbStatsView {
            searched: true,
            stale: false,
            relaxed: s.relaxed,
            local_relaxed: s.local_relaxed,
            unconverged: s.unconverged,
            listed: rows.len(),
            truncated: s.truncated,
            seconds: s.seconds,
            local: s.local.iter().map(ChemisorbLevelView::from).collect(),
            ..ChemisorbStatsView::of_plan(&s.plan)
        };
        let records = rows
            .iter()
            .zip(&report.candidates)
            .map(|(row, c)| candidate_record(row, &c.structure))
            .collect();
        let [debug, shapes] = debug_outputs;
        let outputs = vec![
            NetworkResult::Array(records),
            stats_record(&stats),
            debug,
            shapes,
        ];
        return Ok((
            outputs,
            ChemisorbEvalCache {
                stats,
                rows,
                inventory_options,
                reach_used,
                tree,
                debug_selected,
                tree_key: fingerprint,
            },
        ));
    }

    let stats = ChemisorbStatsView {
        searched: false,
        stale: data.stored.is_some(),
        ..ChemisorbStatsView::of_plan(&planned.stats)
    };
    let [debug, shapes] = debug_outputs;
    let outputs = vec![
        NetworkResult::Array(Vec::new()),
        stats_record(&stats),
        debug,
        shapes,
    ];
    Ok((
        outputs,
        ChemisorbEvalCache {
            stats,
            rows: Vec::new(),
            inventory_options,
            reach_used,
            tree,
            debug_selected,
            tree_key: fingerprint,
        },
    ))
}

impl NodeData for ChemisorbData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let all_pins = |v: NetworkResult| EvalOutput::multi(vec![v; 4]);

        let adsorbate = network_evaluator.evaluate_arg_required(
            network_stack,
            node_id,
            registry,
            context,
            ADSORBATE_INPUT_PIN,
        );
        let substrate = network_evaluator.evaluate_arg_required(
            network_stack,
            node_id,
            registry,
            context,
            SUBSTRATE_INPUT_PIN,
        );
        let (adsorbate, substrate) = match atomic_inputs(adsorbate, substrate) {
            Ok(inputs) => inputs,
            Err(message) => return all_pins(NetworkResult::Error(message)),
        };
        let transfers = network_evaluator.evaluate_arg(
            network_stack,
            node_id,
            registry,
            context,
            TRANSFERS_INPUT_PIN,
        );
        let transfers = match transfer_rules(transfers) {
            Ok(rules) => rules,
            Err(message) => return all_pins(NetworkResult::Error(message)),
        };
        let config = match self.search_config(context.use_vdw_cutoff, transfers) {
            Ok(config) => config,
            Err(message) => return all_pins(NetworkResult::Error(message)),
        };
        match chemisorb_outputs(self, &adsorbate, &substrate, &config) {
            Ok((outputs, cache)) => {
                if network_stack.len() == 1 {
                    context.selected_node_eval_cache = Some(Box::new(cache));
                }
                EvalOutput::multi(outputs)
            }
            Err(message) => all_pins(NetworkResult::Error(message)),
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    /// A settings edit, and its undo, replace the whole data; the stored
    /// search survives them, so "change tolerance, undo" brings the result
    /// back. Safe because it is only output while the fingerprint matches.
    fn inherit_runtime_state(&mut self, previous: &dyn NodeData) {
        if self.stored.is_none()
            && let Some(previous) = previous.as_any_ref().downcast_ref::<ChemisorbData>()
        {
            self.stored = previous.stored.clone();
        }
        if self.debug.is_none()
            && let Some(previous) = previous.as_any_ref().downcast_ref::<ChemisorbData>()
        {
            self.debug = previous.debug.clone();
        }
    }

    /// Run is this node's job (`chemisorb_ops.rs`).
    fn prepare_job(&self, inputs: &mut JobInputs) -> Option<Result<Box<dyn JobWork>, String>> {
        Some(
            self.prepare_work(inputs)
                .map(|work| Box::new(work) as Box<dyn JobWork>),
        )
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        let mut subtitle = format!("anchor {} Å · tol {} Å", self.anchor_reach, self.tolerance);
        if let Some(n) = self.formed_bonds {
            subtitle.push_str(&format!(" · {n} leg{}", if n == 1 { "" } else { "s" }));
        }
        Some(subtitle)
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("adsorbate".to_string(), (true, None));
        m.insert("substrate".to_string(), (true, None));
        m.insert("transfers".to_string(), (false, None));
        m
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![
            (
                "adsorbate_tag".to_string(),
                TextValue::String(self.adsorbate_tag.clone()),
            ),
            (
                "substrate_tag".to_string(),
                TextValue::String(self.substrate_tag.clone()),
            ),
            (
                "anchor_reach".to_string(),
                TextValue::Float(self.anchor_reach),
            ),
            ("tolerance".to_string(), TextValue::Float(self.tolerance)),
            ("reach".to_string(), TextValue::Float(self.reach)),
            (
                "clash_filter".to_string(),
                TextValue::Bool(self.clash_filter),
            ),
            (
                "max_formed_bonds".to_string(),
                TextValue::Int(self.max_formed_bonds),
            ),
            ("top_n".to_string(), TextValue::Int(self.top_n)),
            (
                "energy_window".to_string(),
                TextValue::Float(self.energy_window),
            ),
            ("budget".to_string(), TextValue::Int(self.budget)),
            (
                "max_iterations".to_string(),
                TextValue::Int(self.max_iterations),
            ),
        ];
        if let Some(n) = self.formed_bonds {
            props.push(("formed_bonds".to_string(), TextValue::Int(n)));
        }
        if let Some(bonds) = &self.bond_inventory {
            props.push((
                "bond_inventory".to_string(),
                TextValue::String(bonds.clone()),
            ));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        let string = |key: &str| -> Result<Option<String>, String> {
            props
                .get(key)
                .map(|v| {
                    v.as_string()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{key} must be a string"))
                })
                .transpose()
        };
        let float = |key: &str| -> Result<Option<f64>, String> {
            props
                .get(key)
                .map(|v| {
                    v.as_float()
                        .ok_or_else(|| format!("{key} must be a number"))
                })
                .transpose()
        };
        let int = |key: &str| -> Result<Option<i32>, String> {
            props
                .get(key)
                .map(|v| {
                    v.as_int()
                        .ok_or_else(|| format!("{key} must be an integer"))
                })
                .transpose()
        };
        let boolean = |key: &str| -> Result<Option<bool>, String> {
            props
                .get(key)
                .map(|v| v.as_bool().ok_or_else(|| format!("{key} must be a bool")))
                .transpose()
        };
        if let Some(v) = string("adsorbate_tag")? {
            self.adsorbate_tag = v;
        }
        if let Some(v) = string("substrate_tag")? {
            self.substrate_tag = v;
        }
        if let Some(v) = float("anchor_reach")? {
            self.anchor_reach = v;
        }
        if let Some(v) = float("tolerance")? {
            self.tolerance = v;
        }
        if let Some(v) = float("reach")? {
            self.reach = v;
        }
        if let Some(v) = boolean("clash_filter")? {
            self.clash_filter = v;
        }
        if let Some(v) = int("max_formed_bonds")? {
            self.max_formed_bonds = v;
        }
        if let Some(v) = int("top_n")? {
            self.top_n = v;
        }
        if let Some(v) = float("energy_window")? {
            self.energy_window = v;
        }
        if let Some(v) = int("budget")? {
            self.budget = v;
        }
        if let Some(v) = int("max_iterations")? {
            self.max_iterations = v;
        }
        if let Some(v) = int("formed_bonds")? {
            self.formed_bonds = Some(v);
        }
        if let Some(v) = string("bond_inventory")? {
            self.bond_inventory = Some(v);
        }
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    let named = |name: &str| DataType::Record(RecordType::Named(name.to_string()));
    NodeType {
        name: "chemisorb".to_string(),
        description: "Finds the ways a posed adsorbate can bond to a substrate, leg by leg, \
                      relaxes each with UFF and ranks them. The pose fixes only where the \
                      first bond lands; the orientations follow from the site choices.\n\
                      \n\
                      **The search runs only when you press Run** (panel, or `run` in the \
                      CLI). Until then, and whenever an input or a search setting changes \
                      after a run, the node shows the *plan*: `candidates` is empty and \
                      `stats` counts what a run would relax (`searched` false, `stale` true \
                      after an earlier run). Results are not saved with the file. To view a \
                      candidate, take its `structure` field downstream.\n\
                      \n\
                      A **site** is a substrate reactive atom with a free valence; a hydrogen \
                      on the substrate blocks its host. A **foot** is an adsorbate reactive \
                      atom with a free valence (or, with a transfer record, an OH oxygen); \
                      each forms at most one bond. **Leg 1**: a site within **anchor_reach** \
                      (Å) of a posed foot. **Legs 2 and 3**: any site the foot spacing and \
                      the bond lengths allow, plus **tolerance** (Å) for the molecule's own \
                      flex. **Legs 4 and later**: a site within **reach** (Å) of a foot's \
                      relaxed position. The adsorbate is seated rigidly on its bonded sites \
                      before relaxing; seatings that put it through the substrate are \
                      pruned (**clash_filter**, on by default) and mirror-image three-leg \
                      assignments always are. One-leg bindings are listed only for a \
                      one-foot adsorbate. **adsorbate_tag** / **substrate_tag** restrict the \
                      reactive atoms (empty = all; tag the facet to keep the search small). \
                      **max_formed_bonds** caps the legs (-1 = no cap). **budget** caps the \
                      relaxations; a truncated search is not exhaustive.\n\
                      \n\
                      **Transfers** (optional `transfers` pin, an array of `ChemisorbTransfer` \
                      records, `to_substrate` only): an OH foot hands its H to the free site \
                      nearest the site it bonds to, within **reach** of it — one fixed rule, \
                      not a search. Tag the O feet: with no adsorbate tag, every C–H carbon \
                      becomes a donor foot too.\n\
                      \n\
                      **Ranking** is by `strain`: the UFF energy against the separated state \
                      (adsorbate and substrate each relaxed alone, kcal/mol), lower first. \
                      There is no bond-energy term, so fewer bonds usually rank first, and \
                      candidates with different bond inventories do not compare cleanly: \
                      set **formed_bonds** (exactly this many legs) or **bond_inventory** \
                      (exactly this inventory, as in a candidate's `bonds` field) before \
                      reading the ranking. **top_n** and **energy_window** (kcal/mol above \
                      the best) choose which relaxed candidates are kept. Every setting \
                      needs a new Run. Changed atoms carry the `cs_changed` tag.
                      
                      **Debug view.** The panel's search tree lists every path the search                       took (root, the leg-1 foot, then one leg per level). Selecting a row                       shows it on `debug`: the structure posed, seated or relaxed, bonded                       pairs orange, unbonded feet violet, the sites the next leg's test                       accepts green, near misses yellow (labelled with how far they missed),                       mirror-pruned blue, undecided cyan, clashing red, the rest of the                       substrate dimmed. `debug_shapes` draws the row's search shapes: the                       `anchor_reach` spheres, the leg-2 shell, the leg-3 ring, the `reach`                       spheres of the local phase. With no row selected (or after the inputs                       change) both show the root: every foot and its anchor sites."
            .to_string(),
        summary: Some("Find and rank chemisorption bindings leg by leg".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "adsorbate".to_string(),
                data_type: DataType::HasAtoms,
            },
            // Any future pin must be APPENDED — pin indices are persisted in
            // wires.
            Parameter {
                id: None,
                name: "substrate".to_string(),
                data_type: DataType::HasAtoms,
            },
            Parameter {
                id: None,
                name: "transfers".to_string(),
                data_type: DataType::Array(Box::new(named(CHEMISORB_TRANSFER_RECORD))),
            },
        ],
        // Output pins too: `best` (old pin 0) was removed, the one renumbering
        // (design_chemisorption_sequential.md §6.2). Append from here on.
        output_pins: vec![
            OutputPinDefinition::fixed(
                "candidates",
                DataType::Array(Box::new(named(CHEMISORB_CANDIDATE_RECORD))),
            ),
            OutputPinDefinition::fixed("stats", named(CHEMISORB_STATS_RECORD)),
            // The debug view (§6.5): the selected tree row, and its search
            // shapes. Each pin can be shown on its own.
            OutputPinDefinition::fixed("debug", DataType::Molecule),
            OutputPinDefinition::fixed("debug_shapes", DataType::Isosurface),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ChemisorbData::default()),
        node_data_saver: generic_node_data_saver::<ChemisorbData>,
        node_data_loader: generic_node_data_loader::<ChemisorbData>,
    }
}
