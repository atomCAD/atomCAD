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

use crate::node_data::NodeData;
use crate::node_jobs::{JobInputs, JobResult, JobWork};
use crate::nodes::chemisorb::{
    ADSORBATE_INPUT_PIN, ChemisorbData, SUBSTRATE_INPUT_PIN, StoredSearch, TRANSFERS_INPUT_PIN,
    atomic_inputs, transfer_rules,
};
use crate::structure_designer::StructureDesigner;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::chemisorption::{ChemisorptionSearch, input_fingerprint, search};
use atomcad_util::job_control::JobControl;
use atomcad_util::number_format::format_natural;
use std::sync::Arc;

/// What a `chemisorb` job is called in the UI.
pub const CHEMISORB_JOB_LABEL: &str = "Chemisorption search";

/// What one Run found, for a caller that reports it (the CLI prints it).
#[derive(Debug, Clone, PartialEq)]
pub struct ChemisorbRunSummary {
    /// Hypotheses relaxed.
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
    pub config: ChemisorptionSearch,
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
    /// Run: the search, and its summary.
    pub fn search(self, control: Option<&JobControl>) -> Result<ChemisorbOutcome, String> {
        let report = search(&self.adsorbate, &self.substrate, &self.config, control)
            .map_err(|e| format!("chemisorb: {e}"))?;
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
