//! Run: the one place a `chemisorb` search is computed — `chemisorb`'s node
//! job (`doc/design_background_node_jobs.md`).
//!
//! `chemisorb`'s `eval` never searches (see `nodes/chemisorb.rs`); this is the
//! explicit action behind the panel's Run button and the CLI's `run`. It comes
//! in the three halves of a node job:
//!
//! - **prepare** — [`ChemisorbData::prepare_work`], on the UI thread:
//!   evaluates the node's three inputs and builds the search config and the
//!   input fingerprint into a [`ChemisorbWork`] that owns all of it;
//! - **run** — [`ChemisorbWork::search`], on any thread: the search itself,
//!   reporting into a `JobControl`, and the summary computed from its report;
//! - **install** — [`ChemisorbOutcome`]'s `JobResult::install`, on the UI
//!   thread: stores the report on the node with the fingerprint. The next
//!   refresh then outputs it, for exactly as long as the inputs keep that
//!   fingerprint — which is why installing is safe whatever was edited while
//!   the search ran.
//!
//! Run is **not an undo step** and does not dirty the file: the stored result
//! is never saved, and it is a pure function of the inputs. Installing marks
//! the node's data changed so the refresh re-evaluates it and everything
//! downstream.
//!
//! The **debug view**'s select action (design §6.5) is here too, in the same
//! shape: [`StructureDesigner::prepare_chemisorb_debug`] evaluates the inputs
//! and finds the row; a view that is geometry alone is built at once
//! ([`ChemisorbDebugStep::Ready`], installed by the caller), one that must
//! replay a relaxation is a second kind of node job
//! ([`ChemisorbDebugWork`]). Both install through the same
//! `install_job_result`, so a selection is no more an undo step than a Run.

use crate::node_data::NodeData;
use crate::node_jobs::{JobInputs, JobResult, JobTarget, JobWork};
use crate::nodes::chemisorb::{
    ADSORBATE_INPUT_PIN, ChemisorbData, SUBSTRATE_INPUT_PIN, StoredDebug, StoredSearch,
    TRANSFERS_INPUT_PIN, atomic_inputs, transfer_rules,
};
use crate::structure_designer::StructureDesigner;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::input_fingerprint;
use atomcad_crystolecule::chemisorption::sequential::{
    DebugForm, SearchReport, SequentialPlan, SequentialSearch, debug_view, evaluate, find_row,
    needs_relaxation, plan, row_forms, row_label, row_path, shown_row, tree_of,
};
use atomcad_util::job_control::JobControl;
use atomcad_util::number_format::format_natural;
use std::sync::Arc;

/// What a `chemisorb` job is called in the UI.
pub const CHEMISORB_JOB_LABEL: &str = "Chemisorption search";

/// What one Run found, for a caller that reports it (the CLI prints it).
#[derive(Debug, Clone, PartialEq)]
pub struct ChemisorbRunSummary {
    /// Relaxations run, both phases.
    pub relaxed: usize,
    /// Candidates kept and listed: the best `top_n` within the window.
    pub listed: usize,
    /// The strain of the best listed candidate (kcal/mol), `None` when
    /// nothing is listed.
    pub best_strain: Option<f64>,
    /// The bond inventory of the best listed candidate, empty when nothing is
    /// listed.
    pub best_bonds: String,
    /// The budget was hit; the search is not exhaustive.
    pub truncated: bool,
    pub unconverged: usize,
    pub seconds: f64,
}

/// One line per fact: what the CLI's `run` prints and the job's install
/// summary.
pub fn format_run_result(result: &ChemisorbRunSummary) -> String {
    let mut lines = vec![format!(
        "Relaxed {} hypotheses in {} s; {} listed.",
        result.relaxed,
        format_natural(result.seconds, 3),
        result.listed
    )];
    match result.best_strain {
        Some(strain) => lines.push(format!(
            "Best listed: strain {} kcal/mol, {}.",
            format_natural(strain, 4),
            result.best_bonds
        )),
        None => lines.push("No candidate listed.".to_string()),
    }
    if result.unconverged > 0 {
        lines.push(format!(
            "{} relaxation(s) did not converge.",
            result.unconverged
        ));
    }
    if result.truncated {
        lines.push("Budget hit: the search is NOT exhaustive.".to_string());
    }
    lines.join("\n")
}

/// A prepared search: the evaluated inputs, the config (every setting,
/// filters and `top_n` included) and the fingerprint of all of it. Owns
/// everything, so it can run on any thread.
pub struct ChemisorbWork {
    pub adsorbate: AtomicStructure,
    pub substrate: AtomicStructure,
    pub config: SequentialSearch,
    pub fingerprint: u64,
}

/// A finished search, ready to install. The summary is computed on the
/// worker, from the report itself, so it is reported even if the install
/// later misses.
pub struct ChemisorbOutcome {
    pub stored: StoredSearch,
    pub summary: ChemisorbRunSummary,
}

impl ChemisorbData {
    /// Prepare: evaluates the three pins and builds the search. `Err` for
    /// broken inputs or settings, in the node's own words.
    pub fn prepare_work(&self, inputs: &mut JobInputs) -> Result<ChemisorbWork, String> {
        let adsorbate = inputs.eval_input_required(ADSORBATE_INPUT_PIN);
        let substrate = inputs.eval_input_required(SUBSTRATE_INPUT_PIN);
        let transfers = inputs.eval_input(TRANSFERS_INPUT_PIN);
        let (adsorbate, substrate) = atomic_inputs(adsorbate, substrate)?;
        let config =
            self.search_config(inputs.context().use_vdw_cutoff, transfer_rules(transfers)?)?;
        let fingerprint = input_fingerprint(&adsorbate, &substrate, &config);
        Ok(ChemisorbWork {
            adsorbate,
            substrate,
            config,
            fingerprint,
        })
    }
}

impl ChemisorbWork {
    /// Run: the search, and its summary. `plan` then `evaluate`, as
    /// `sequential::search` does, but keeping the plan: the stored result
    /// needs it (the report numbers its hypotheses after the plan's).
    pub fn search(self, control: Option<&JobControl>) -> Result<ChemisorbOutcome, String> {
        let fail = |e| format!("chemisorb: {e}");
        if let Some(c) = control {
            c.set_phase("Planning");
        }
        let planned = plan(&self.adsorbate, &self.substrate, &self.config).map_err(fail)?;
        if let Some(c) = control {
            c.set_total(planned.to_relax.len() as u64 + 1);
            c.set_phase("Relaxing");
        }
        let report = evaluate(&planned, &self.config, control).map_err(fail)?;
        let best = report.candidates.first();
        let summary = ChemisorbRunSummary {
            relaxed: report.stats.relaxed,
            listed: report.candidates.len(),
            best_strain: best.map(|c| c.strain),
            best_bonds: best.map_or_else(String::new, |c| c.bond_inventory.to_string()),
            truncated: report.stats.truncated,
            unconverged: report.stats.unconverged,
            seconds: report.stats.seconds,
        };
        Ok(ChemisorbOutcome {
            stored: StoredSearch {
                fingerprint: self.fingerprint,
                plan: planned,
                report,
            },
            summary,
        })
    }
}

impl JobWork for ChemisorbWork {
    fn label(&self) -> String {
        CHEMISORB_JOB_LABEL.to_string()
    }

    fn run(self: Box<Self>, control: Option<&JobControl>) -> Result<Box<dyn JobResult>, String> {
        Ok(Box::new(self.search(control)?))
    }
}

impl JobResult for ChemisorbOutcome {
    fn install(self: Box<Self>, data: &mut dyn NodeData) -> Result<String, String> {
        let data = data
            .as_any_mut()
            .downcast_mut::<ChemisorbData>()
            .ok_or("the node is no longer a chemisorb node")?;
        data.stored = Some(Arc::new(self.stored));
        Ok(format_run_result(&self.summary))
    }
}

impl StructureDesigner {
    /// Replaces a `chemisorb` node's settings: one undo step, through
    /// `set_node_network_data_scoped`. The stored result is carried over by
    /// `ChemisorbData::inherit_runtime_state`. A write to a node of another
    /// type is a no-op.
    pub fn set_chemisorb_data(&mut self, scope_path: &[u64], node_id: u64, data: ChemisorbData) {
        // Read-only guard (`doc/design_library_linking.md` §6).
        if self.ensure_active_editable().is_err() {
            return;
        }
        let is_chemisorb = self
            .get_node_network_data_scoped(scope_path, node_id)
            .is_some_and(|d| d.as_any_ref().is::<ChemisorbData>());
        if is_chemisorb {
            self.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
        }
    }

    /// Run on the calling thread with a typed summary: the same guards,
    /// prepare, search and install as `run_node_job_blocking`, which is what
    /// production code calls. Kept for the tests that assert on the summary's
    /// fields. `Err` for a node that is not a `chemisorb`, a node inside a
    /// higher-order-function body, broken inputs or settings, and a failed
    /// search; nothing is stored then.
    pub fn run_chemisorb(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<ChemisorbRunSummary, String> {
        let (target, work) = self.with_job_inputs(scope_path, node_id, |data, inputs| {
            data.as_any_ref()
                .downcast_ref::<ChemisorbData>()
                .ok_or_else(|| format!("Node {node_id} is not a chemisorb node"))?
                .prepare_work(inputs)
        })?;
        let outcome = work?.search(None)?;
        let summary = outcome.summary.clone();
        self.install_job_result(&target, Box::new(outcome))?;
        Ok(summary)
    }
}

// ============================================================================
// The debug view (design §6.5)
// ============================================================================

/// What a `chemisorb` debug-view job is called in the UI.
pub const CHEMISORB_DEBUG_JOB_LABEL: &str = "Chemisorption debug view";

/// At most this many children are listed under a selected row's description
/// (the CLI's `debug-select`).
const LISTED_CHILDREN: usize = 40;

/// Which tree row to show: by number (the panel), or by path (the CLI, as
/// `sequential::find_row` reads it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugRowRef {
    Row(u32),
    Path(String),
}

/// A row's view, built: what an install writes.
pub struct ChemisorbDebugOutcome {
    pub stored: StoredDebug,
    /// One fact per line; the first names the row, its form and its strain.
    pub description: String,
}

/// A row whose view needs relaxations (a relaxed row that is not a kept
/// candidate, or a local-phase row's start geometry): the replay, owned, to
/// run off the UI thread like Run.
pub struct ChemisorbDebugWork {
    pub fingerprint: u64,
    pub search: Arc<StoredSearch>,
    pub config: SequentialSearch,
    pub row: u32,
    pub form: DebugForm,
}

/// A prepared selection: built already, or a job to start.
pub enum ChemisorbDebugStep {
    Ready(Box<ChemisorbDebugOutcome>),
    Job(Box<ChemisorbDebugWork>),
}

pub struct ChemisorbDebugPrepared {
    pub target: JobTarget,
    pub step: ChemisorbDebugStep,
    /// The selected row's listed children, one per line with its path: what
    /// the CLI prints so an agent can go one level deeper.
    pub children: String,
}

fn build_debug(
    plan: &SequentialPlan,
    report: Option<&SearchReport>,
    config: &SequentialSearch,
    fingerprint: u64,
    row: u32,
    form: DebugForm,
) -> Result<ChemisorbDebugOutcome, String> {
    let view =
        debug_view(plan, report, config, row, form).map_err(|e| format!("chemisorb: {e}"))?;
    let description = view.describe(&plan.setup, tree_of(plan, report));
    Ok(ChemisorbDebugOutcome {
        stored: StoredDebug { fingerprint, view },
        description,
    })
}

/// The children of a row as `debug-select` lists them: path, label, and
/// what became of the hypothesis each reaches.
fn children_text(plan: &SequentialPlan, report: Option<&SearchReport>, row: u32) -> String {
    let tree = tree_of(plan, report);
    let row = shown_row(tree, row);
    let children = tree.visible_children(row, false);
    if children.is_empty() {
        return "children: none".to_string();
    }
    let mut lines = vec![format!("children ({}):", children.len())];
    for &c in children.iter().take(LISTED_CHILDREN) {
        let h = tree.row(c).hypothesis.map(|i| match report {
            Some(r) => r.hypothesis(plan, i as usize),
            None => &plan.hypotheses[i as usize],
        });
        let mut notes = Vec::new();
        if let Some(h) = h {
            if let Some(m) = h.mirror {
                notes.push(format!("{m:?}").to_lowercase());
            }
            if h.seating.as_ref().is_some_and(|s| s.clashes()) {
                notes.push("clash".to_string());
            }
            if h.candidate {
                notes.push("candidate".to_string());
            }
        }
        if let Some(r) = report.and_then(|r| r.relaxed.iter().find(|x| x.row == c)) {
            notes.push(format!("strain {:.2}", r.strain));
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!("  [{}]", notes.join(", "))
        };
        lines.push(format!(
            "  {}  {}{notes}",
            row_path(&plan.setup, tree, c),
            row_label(&plan.setup, tree, c),
        ));
    }
    if children.len() > LISTED_CHILDREN {
        lines.push(format!(
            "  ... and {} more",
            children.len() - LISTED_CHILDREN
        ));
    }
    lines.join("\n")
}

impl JobWork for ChemisorbDebugWork {
    fn label(&self) -> String {
        CHEMISORB_DEBUG_JOB_LABEL.to_string()
    }

    fn run(self: Box<Self>, control: Option<&JobControl>) -> Result<Box<dyn JobResult>, String> {
        if let Some(c) = control {
            c.set_phase("Relaxing the selected row");
        }
        let outcome = build_debug(
            &self.search.plan,
            Some(&self.search.report),
            &self.config,
            self.fingerprint,
            self.row,
            self.form,
        )?;
        Ok(Box::new(outcome))
    }
}

impl JobResult for ChemisorbDebugOutcome {
    fn install(self: Box<Self>, data: &mut dyn NodeData) -> Result<String, String> {
        let data = data
            .as_any_mut()
            .downcast_mut::<ChemisorbData>()
            .ok_or("the node is no longer a chemisorb node")?;
        data.debug = Some(Arc::new(self.stored));
        Ok(self.description)
    }
}

impl StructureDesigner {
    /// Selects a row of a `chemisorb` node's search tree for its debug pins
    /// (design §6.5): evaluates the inputs, finds the tree (the stored search
    /// when it matches them, else a fresh plan) and the row, and either builds
    /// the view at once — posed and seated geometric rows are geometry alone —
    /// or prepares a job for one that needs relaxing. `form` = `None` opens
    /// the row on its default form. Errors are user-facing.
    ///
    /// Selecting is not an undo step and does not dirty the file: the view is
    /// runtime state keyed by the input fingerprint, like the search result.
    pub fn prepare_chemisorb_debug(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        row: DebugRowRef,
        form: Option<DebugForm>,
    ) -> Result<ChemisorbDebugPrepared, String> {
        let (target, prepared) = self.with_job_inputs(scope_path, node_id, |data, inputs| {
            let data = data
                .as_any_ref()
                .downcast_ref::<ChemisorbData>()
                .ok_or_else(|| format!("Node {node_id} is not a chemisorb node"))?;
            let work = data.prepare_work(inputs)?;
            let stored = data
                .stored
                .clone()
                .filter(|s| s.fingerprint == work.fingerprint);
            Ok::<_, String>((work, stored))
        })?;
        let (work, stored) = prepared?;
        let fresh;
        let (plan, report) = match &stored {
            Some(s) => (&s.plan, Some(&s.report)),
            None => {
                fresh = plan(&work.adsorbate, &work.substrate, &work.config)
                    .map_err(|e| format!("chemisorb: {e}"))?;
                (&fresh, None)
            }
        };
        let tree = tree_of(plan, report);
        let row = match row {
            DebugRowRef::Row(r) if (r as usize) < tree.len() => r,
            DebugRowRef::Row(r) => return Err(format!("The search tree has no row {r}")),
            DebugRowRef::Path(path) => find_row(&plan.setup, tree, &path)?,
        };
        let forms = row_forms(plan, report, row);
        let form = form.unwrap_or(forms.default);
        if form == DebugForm::Relaxed && !forms.relaxed {
            return Err(format!(
                "{} has no relaxation{}",
                row_label(&plan.setup, tree, row),
                if report.is_none() { ": Run first" } else { "" }
            ));
        }
        let children = children_text(plan, report, row);
        let step = match (&stored, needs_relaxation(plan, report, row, form)) {
            (Some(search), true) => ChemisorbDebugStep::Job(Box::new(ChemisorbDebugWork {
                fingerprint: work.fingerprint,
                search: search.clone(),
                config: work.config,
                row,
                form,
            })),
            _ => ChemisorbDebugStep::Ready(Box::new(build_debug(
                plan,
                report,
                &work.config,
                work.fingerprint,
                row,
                form,
            )?)),
        };
        Ok(ChemisorbDebugPrepared {
            target,
            step,
            children,
        })
    }

    /// The CLI's `debug-select`: prepare, run any relaxation on the calling
    /// thread, install. Returns the row's description and its children. Does
    /// not refresh.
    pub fn chemisorb_debug_select_blocking(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        row: DebugRowRef,
        form: Option<DebugForm>,
    ) -> Result<String, String> {
        let prepared = self.prepare_chemisorb_debug(scope_path, node_id, row, form)?;
        let result: Box<dyn JobResult> = match prepared.step {
            ChemisorbDebugStep::Ready(outcome) => outcome,
            ChemisorbDebugStep::Job(work) => work.run(None)?,
        };
        let summary = self.install_job_result(&prepared.target, result)?;
        Ok(format!("{summary}\n{}", prepared.children))
    }
}
