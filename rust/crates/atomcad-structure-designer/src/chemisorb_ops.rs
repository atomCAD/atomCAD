//! Run: the one place a `chemisorb` search is computed.
//!
//! `chemisorb`'s `eval` never searches (see `nodes/chemisorb.rs`); this is the
//! explicit action behind the panel's Run button, the API's `run_chemisorb`
//! and the CLI's `run`. It evaluates the node's three inputs in the active
//! network, searches, and stores the report on the node together with the
//! fingerprint of the inputs it was computed from. The next refresh then
//! outputs it, for exactly as long as the inputs keep that fingerprint.
//!
//! Run is **not an undo step** and does not dirty the file: the stored result
//! is never saved, and it is a pure function of the inputs. It marks the node's
//! data changed so the refresh re-evaluates it and everything downstream.
//!
//! Synchronous. Progress, cancel and a background runner would hook in here,
//! calling `plan` and `evaluate` separately.

use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::nodes::chemisorb::{
    ADSORBATE_INPUT_PIN, ChemisorbData, SUBSTRATE_INPUT_PIN, StoredSearch, TRANSFERS_INPUT_PIN,
    atomic_inputs, transfer_rules,
};
use crate::structure_designer::StructureDesigner;
use atomcad_crystolecule::chemisorption::{input_fingerprint, listed_count, search};
use std::sync::Arc;

/// What one Run found, for a caller that reports it (the CLI prints it).
#[derive(Debug, Clone, PartialEq)]
pub struct ChemisorbRunSummary {
    /// Hypotheses relaxed.
    pub relaxed: usize,
    /// Candidates the node lists under its current `top_n` / `energy_window`.
    pub listed: usize,
    /// The best score (kcal/mol), `None` when nothing was found.
    pub best_score: Option<f64>,
    /// The bond inventory of the best candidate, empty when nothing was found.
    pub best_bonds: String,
    /// The budget was hit; the search is not exhaustive.
    pub truncated: bool,
    pub unconverged: usize,
    pub seconds: f64,
}

impl StructureDesigner {
    /// Replaces a `chemisorb` node's settings: one undo step, through
    /// `set_node_network_data_scoped`. The stored result is carried over by
    /// `ChemisorbData::inherit_runtime_state`. A write to a node of another
    /// type is a no-op.
    pub fn set_chemisorb_data(&mut self, scope_path: &[u64], node_id: u64, data: ChemisorbData) {
        let is_chemisorb = self
            .get_node_network_data_scoped(scope_path, node_id)
            .is_some_and(|d| d.as_any_ref().is::<ChemisorbData>());
        if is_chemisorb {
            self.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
        }
    }

    /// Searches with the `chemisorb` node `node_id` of the active network and
    /// stores the result on it. `Err` for a node that is not a `chemisorb`, a
    /// node inside a higher-order-function body, broken inputs or settings,
    /// and a failed search; nothing is stored then.
    pub fn run_chemisorb(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<ChemisorbRunSummary, String> {
        if !scope_path.is_empty() {
            // A body node's inputs depend on per-iteration zone values that
            // do not exist outside an iteration, as for Execute.
            return Err(
                "Cannot run a chemisorb node inside a higher-order-function body".to_string(),
            );
        }
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active node network")?;
        let data = self
            .get_node_network_data_scoped(scope_path, node_id)
            .and_then(|d| d.as_any_ref().downcast_ref::<ChemisorbData>())
            .ok_or_else(|| format!("Node {node_id} is not a chemisorb node"))?
            .clone();

        let (adsorbate, substrate, transfers, use_vdw_cutoff) =
            self.with_eval_context(false, |evaluator, registry, _prefs, context| {
                let network = registry
                    .node_networks
                    .get(&network_name)
                    .expect("active network");
                let stack = vec![NetworkStackElement::root(network)];
                let adsorbate = evaluator.evaluate_arg_required(
                    &stack,
                    node_id,
                    registry,
                    context,
                    ADSORBATE_INPUT_PIN,
                );
                let substrate = evaluator.evaluate_arg_required(
                    &stack,
                    node_id,
                    registry,
                    context,
                    SUBSTRATE_INPUT_PIN,
                );
                let transfers =
                    evaluator.evaluate_arg(&stack, node_id, registry, context, TRANSFERS_INPUT_PIN);
                (adsorbate, substrate, transfers, context.use_vdw_cutoff)
            });
        let (adsorbate, substrate) = atomic_inputs(adsorbate, substrate)?;
        let config = data.search_config(use_vdw_cutoff, transfer_rules(transfers)?)?;
        let fingerprint = input_fingerprint(&adsorbate, &substrate, &config);
        let report =
            search(&adsorbate, &substrate, &config).map_err(|e| format!("chemisorb: {e}"))?;

        let best = report.candidates.first();
        let summary = ChemisorbRunSummary {
            relaxed: report.stats.relaxed,
            listed: listed_count(
                &report.candidates,
                data.top_n.max(1) as usize,
                data.energy_window,
            ),
            best_score: best.map(|c| c.score),
            best_bonds: best.map_or_else(String::new, |c| c.bond_inventory.to_string()),
            truncated: report.stats.truncated,
            unconverged: report.stats.unconverged,
            seconds: report.stats.seconds,
        };

        let stored = self
            .get_node_network_data_mut_scoped(scope_path, node_id)
            .and_then(|d| d.as_any_mut().downcast_mut::<ChemisorbData>())
            .ok_or_else(|| format!("Node {node_id} is not a chemisorb node"))?;
        stored.stored = Some(Arc::new(StoredSearch {
            fingerprint,
            report,
        }));
        Ok(summary)
    }
}
