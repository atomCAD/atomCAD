//! `chemisorb` — every way a posed adsorbate can bond to a substrate, relaxed
//! and ranked.
//!
//! Phases 2 and 3 of the chemisorption search design. A thin adapter over
//! `atomcad_crystolecule::chemisorption`: this file reads pins and properties,
//! builds a `ChemisorptionSearch`, and maps a report onto three outputs. It
//! holds no chemistry.
//!
//! **Evaluation never searches.** A search relaxes hundreds of structures, and
//! the evaluator re-evaluates a node on far more than its own edits
//! (selection, downstream edits, every full refresh), so `eval` only ever runs
//! the cheap `plan` and reads a stored result. The search itself runs on an
//! explicit action — the panel's Run button, the API's `run_chemisorb`, the
//! CLI's `run` — which is `StructureDesigner::run_chemisorb`
//! (`chemisorb_ops.rs`).
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
//! `top_n` and `energy_window` choose which candidates are *listed*, not what
//! is searched, so they stay out of the fingerprint: changing them re-lists
//! the stored result instead of making it stale.
//!
//! Bond forming is always on; transfers are enabled by wiring the `transfers`
//! pin, an array of `ChemisorbTransfer { element, direction }` records (one
//! per allowed element and direction). Disconnected, or an empty array, means
//! bond forming only — and then `max_transfers` is not read, and not
//! fingerprinted either.
//!
//! The panel's data goes into `context.selected_node_eval_cache`
//! ([`ChemisorbEvalCache`]) on root evaluations, as `proxy` and `relax` do.

use crate::data_type::{DataType, RecordType};
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::{MoleculeData, NetworkResult, first_array_element_error};
use crate::node_data::{EvalOutput, NodeData};
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
use atomcad_crystolecule::chemisorption::{
    Candidate, ChemisorptionSearch, SearchReport, TransferDirection, TransferRule,
    input_fingerprint, is_transferable_element, listed_count, plan,
};
use atomcad_crystolecule::simulation::uff::VdwMode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const CHEMISORB_TRANSFER_RECORD: &str = "ChemisorbTransfer";
pub const CHEMISORB_CANDIDATE_RECORD: &str = "ChemisorbCandidate";
pub const CHEMISORB_STRAIN_TERMS_RECORD: &str = "ChemisorbStrainTerms";
pub const CHEMISORB_STATS_RECORD: &str = "ChemisorbStats";

/// Input pin indices.
pub const ADSORBATE_INPUT_PIN: usize = 0;
pub const SUBSTRATE_INPUT_PIN: usize = 1;
pub const TRANSFERS_INPUT_PIN: usize = 2;

/// Output pin indices.
pub const BEST_OUTPUT_PIN: usize = 0;
pub const CANDIDATES_OUTPUT_PIN: usize = 1;
pub const STATS_OUTPUT_PIN: usize = 2;

/// The van der Waals cutoff `relax` uses when the preference asks for one.
const VDW_CUTOFF: f64 = 6.0;

fn default_reach() -> f64 {
    3.5
}
fn default_pair_tolerance() -> f64 {
    ChemisorptionSearch::default().pair_tolerance
}
fn default_max_transfers() -> i32 {
    1
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
#[derive(Debug)]
pub struct StoredSearch {
    pub fingerprint: u64,
    pub report: SearchReport,
}

/// Node data for `chemisorb`: the search settings (all persisted, all text
/// properties) and the last search result (neither).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChemisorbData {
    /// Adsorbate reactive atoms: those carrying this tag. Empty = all atoms.
    #[serde(default)]
    pub adsorbate_tag: String,
    /// Substrate reactive atoms: those carrying this tag. Empty = all atoms.
    #[serde(default)]
    pub substrate_tag: String,
    /// Maximum adsorbate atom to site distance for a bond to be considered (Å).
    #[serde(default = "default_reach")]
    pub reach: f64,
    /// Pair tolerance `δ` (Å); `0` switches the check off.
    #[serde(default = "default_pair_tolerance")]
    pub pair_tolerance: f64,
    /// At most this many bonds formed per hypothesis; `0` = no cap.
    #[serde(default)]
    pub max_formed_bonds: i32,
    /// At most this many transfers per hypothesis, over all `transfers`
    /// records. Read only while the pin carries at least one record.
    #[serde(default = "default_max_transfers")]
    pub max_transfers: i32,
    /// At most this many candidates are listed…
    #[serde(default = "default_top_n")]
    pub top_n: i32,
    /// …and only those within this many kcal/mol of the best score.
    #[serde(default = "default_energy_window")]
    pub energy_window: f64,
    /// At most this many hypotheses are relaxed; past it the search is
    /// truncated and not exhaustive.
    #[serde(default = "default_budget")]
    pub budget: i32,
    /// UFF iteration limit per relaxation.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: i32,
    /// The last search, written only by `StructureDesigner::run_chemisorb`.
    /// Never saved and never an undo step: it is a pure function of the
    /// inputs, and the fingerprint keeps a copied or outdated one harmless.
    #[serde(skip)]
    pub stored: Option<Arc<StoredSearch>>,
}

impl Default for ChemisorbData {
    fn default() -> Self {
        Self {
            adsorbate_tag: String::new(),
            substrate_tag: String::new(),
            reach: default_reach(),
            pair_tolerance: default_pair_tolerance(),
            max_formed_bonds: 0,
            max_transfers: default_max_transfers(),
            top_n: default_top_n(),
            energy_window: default_energy_window(),
            budget: default_budget(),
            max_iterations: default_max_iterations(),
            stored: None,
        }
    }
}

/// The whole search, as the `stats` pin and the panel show it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChemisorbStatsView {
    /// Adsorbate reactive atoms with a free valence and a site within reach.
    pub feet: usize,
    /// Distinct sites within reach of at least one of them.
    pub sites_in_reach: usize,
    /// Candidate transfers `(donor, atom, acceptor)` the records allow.
    pub transfer_candidates: usize,
    pub considered: usize,
    pub pruned_valence: usize,
    pub pruned_pair_tolerance: usize,
    pub duplicates: usize,
    /// The outputs are a search result for the current inputs.
    pub searched: bool,
    /// A stored result exists but was computed from other inputs.
    pub stale: bool,
    pub relaxed: usize,
    pub to_relax: usize,
    pub unconverged: usize,
    pub listed: usize,
    /// The budget was hit; the search is **not** exhaustive.
    pub truncated: bool,
    /// Bond pairs scored by a Pauling estimate, e.g. `"N–Si"`; empty if none.
    pub estimated_pairs: String,
    /// Wall time of the search, or of `plan` when `searched` is false (s).
    pub seconds: f64,
}

/// One listed candidate, as the `candidates` pin and the panel show it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChemisorbRowView {
    pub rank: usize,
    pub score: f64,
    pub strain: f64,
    pub bond_energy: f64,
    pub estimated: bool,
    /// The bond inventory, e.g. `"formed 3× O–Si"`.
    pub bonds: String,
    /// The formed bonds by atom id in the output structure, then the
    /// transfers, e.g. `"O12–Si45, O13–Si47; H14 O13→Si48"`.
    pub sites: String,
    pub formed_bonds: usize,
    pub transfers: usize,
    pub converged: bool,
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
}

impl ChemisorbData {
    /// The search settings, validated in the node's own words. `use_vdw_cutoff`
    /// is the simulation preference `relax` reads too; `transfers` is what the
    /// `transfers` pin carries (empty when it is disconnected).
    pub fn search_config(
        &self,
        use_vdw_cutoff: bool,
        transfers: Vec<TransferRule>,
    ) -> Result<ChemisorptionSearch, String> {
        if self.max_formed_bonds < 0 {
            return Err("chemisorb: max_formed_bonds must be >= 0 (0 = no cap)".to_string());
        }
        if self.max_transfers < 1 {
            return Err("chemisorb: max_transfers must be at least 1".to_string());
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
        if !(self.energy_window.is_finite() && self.energy_window >= 0.0) {
            return Err("chemisorb: energy_window must be >= 0".to_string());
        }
        let tag = |t: &str| {
            let t = t.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let config = ChemisorptionSearch {
            adsorbate_tag: tag(&self.adsorbate_tag),
            substrate_tag: tag(&self.substrate_tag),
            reach: self.reach,
            pair_tolerance: self.pair_tolerance,
            max_formed_bonds: (self.max_formed_bonds > 0).then_some(self.max_formed_bonds as usize),
            transfers,
            max_transfers: self.max_transfers as usize,
            budget: self.budget as usize,
            max_iterations: self.max_iterations as u32,
            vdw_mode: if use_vdw_cutoff {
                VdwMode::Cutoff(VDW_CUTOFF)
            } else {
                VdwMode::AllPairs
            },
            ..ChemisorptionSearch::default()
        };
        config.validate().map_err(|e| format!("chemisorb: {e}"))?;
        Ok(config)
    }

    /// The listed prefix of a ranked candidate list.
    fn listed<'a>(&self, candidates: &'a [Candidate]) -> &'a [Candidate] {
        let n = listed_count(candidates, self.top_n.max(1) as usize, self.energy_window);
        &candidates[..n]
    }

    /// The stored report, when it was computed from inputs with this
    /// fingerprint.
    pub fn matching_report(&self, fingerprint: u64) -> Option<&SearchReport> {
        self.stored
            .as_ref()
            .filter(|s| s.fingerprint == fingerprint)
            .map(|s| &s.report)
    }
}

/// The `transfers` pin's records. `None` (disconnected) and an empty array
/// both mean no transfers. An upstream error comes back verbatim, as the
/// other inputs' do.
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
        score: c.score,
        strain: c.strain,
        bond_energy: c.bond_energy,
        estimated: c.estimated,
        bonds: c.bond_inventory.to_string(),
        sites,
        formed_bonds: c.formed.len(),
        transfers: c.transfers.len(),
        converged: c.converged,
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
        ("score".to_string(), float(row.score)),
        ("strain".to_string(), float(row.strain)),
        ("bond_energy".to_string(), float(row.bond_energy)),
        ("estimated".to_string(), NetworkResult::Bool(row.estimated)),
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

fn stats_record(s: &ChemisorbStatsView) -> NetworkResult {
    let int = |n: usize| NetworkResult::Int(n as i32);
    NetworkResult::record(vec![
        ("feet".to_string(), int(s.feet)),
        ("sites_in_reach".to_string(), int(s.sites_in_reach)),
        (
            "transfer_candidates".to_string(),
            int(s.transfer_candidates),
        ),
        ("considered".to_string(), int(s.considered)),
        ("pruned_valence".to_string(), int(s.pruned_valence)),
        (
            "pruned_pair_tolerance".to_string(),
            int(s.pruned_pair_tolerance),
        ),
        ("duplicates".to_string(), int(s.duplicates)),
        ("searched".to_string(), NetworkResult::Bool(s.searched)),
        ("stale".to_string(), NetworkResult::Bool(s.stale)),
        ("relaxed".to_string(), int(s.relaxed)),
        ("to_relax".to_string(), int(s.to_relax)),
        ("unconverged".to_string(), int(s.unconverged)),
        ("listed".to_string(), int(s.listed)),
        ("truncated".to_string(), NetworkResult::Bool(s.truncated)),
        (
            "estimated_pairs".to_string(),
            NetworkResult::String(s.estimated_pairs.clone()),
        ),
        ("seconds".to_string(), NetworkResult::Float(s.seconds)),
    ])
}

fn estimated_label(kinds: &[atomcad_crystolecule::chemisorption::BondKind]) -> String {
    kinds
        .iter()
        .map(|k| k.pair_label())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The three outputs and the panel's data for one evaluation, from the
/// current inputs and whatever is stored. Relaxes nothing.
pub fn chemisorb_outputs(
    data: &ChemisorbData,
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &ChemisorptionSearch,
) -> Result<(Vec<NetworkResult>, ChemisorbEvalCache), String> {
    let fingerprint = input_fingerprint(adsorbate, substrate, config);

    if let Some(report) = data.matching_report(fingerprint) {
        let listed = data.listed(&report.candidates);
        let rows: Vec<ChemisorbRowView> = listed
            .iter()
            .enumerate()
            .map(|(i, c)| row_view(i + 1, c))
            .collect();
        let s = &report.stats;
        let stats = ChemisorbStatsView {
            feet: s.feet,
            sites_in_reach: s.sites_in_reach,
            transfer_candidates: s.transfer_candidates,
            considered: s.considered,
            pruned_valence: s.pruned_valence,
            pruned_pair_tolerance: s.pruned_pair_tolerance,
            duplicates: s.duplicates,
            searched: true,
            stale: false,
            relaxed: s.relaxed,
            to_relax: s.to_relax,
            unconverged: s.unconverged,
            listed: rows.len(),
            truncated: s.truncated,
            estimated_pairs: estimated_label(&s.estimated_pairs),
            seconds: s.seconds,
        };
        // With nothing found, `best` is the relaxed pose: still the most
        // honest picture of what the search looked at.
        let best = report
            .candidates
            .first()
            .map_or(&report.reference.structure, |c| &c.structure);
        let records = rows
            .iter()
            .zip(listed)
            .map(|(row, c)| candidate_record(row, &c.structure))
            .collect();
        let outputs = vec![
            molecule(best.clone()),
            NetworkResult::Array(records),
            stats_record(&stats),
        ];
        return Ok((outputs, ChemisorbEvalCache { stats, rows }));
    }

    let planned = plan(adsorbate, substrate, config).map_err(|e| format!("chemisorb: {e}"))?;
    let p = &planned.stats;
    let stats = ChemisorbStatsView {
        feet: p.feet,
        sites_in_reach: p.sites_in_reach,
        transfer_candidates: p.transfer_candidates,
        considered: p.considered,
        pruned_valence: p.pruned_valence,
        pruned_pair_tolerance: p.pruned_pair_tolerance,
        duplicates: p.duplicates,
        searched: false,
        stale: data.stored.is_some(),
        relaxed: 0,
        to_relax: p.to_relax,
        unconverged: 0,
        listed: 0,
        truncated: p.truncated,
        estimated_pairs: estimated_label(&p.estimated_pairs),
        seconds: p.seconds,
    };
    let outputs = vec![
        molecule(planned.combined),
        NetworkResult::Array(Vec::new()),
        stats_record(&stats),
    ];
    Ok((
        outputs,
        ChemisorbEvalCache {
            stats,
            rows: Vec::new(),
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
        let all_pins = |v: NetworkResult| EvalOutput::multi(vec![v.clone(), v.clone(), v]);

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
    /// search survives them, so "change reach, undo" brings the result back.
    /// Safe because it is only output while the fingerprint matches.
    fn inherit_runtime_state(&mut self, previous: &dyn NodeData) {
        if self.stored.is_none()
            && let Some(previous) = previous.as_any_ref().downcast_ref::<ChemisorbData>()
        {
            self.stored = previous.stored.clone();
        }
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        let mut subtitle = format!("reach {} Å · δ {} Å", self.reach, self.pair_tolerance);
        if connected_input_pins.contains("transfers") {
            subtitle.push_str(&format!(" · ≤{} transfers", self.max_transfers));
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
        vec![
            (
                "adsorbate_tag".to_string(),
                TextValue::String(self.adsorbate_tag.clone()),
            ),
            (
                "substrate_tag".to_string(),
                TextValue::String(self.substrate_tag.clone()),
            ),
            ("reach".to_string(), TextValue::Float(self.reach)),
            (
                "pair_tolerance".to_string(),
                TextValue::Float(self.pair_tolerance),
            ),
            (
                "max_formed_bonds".to_string(),
                TextValue::Int(self.max_formed_bonds),
            ),
            (
                "max_transfers".to_string(),
                TextValue::Int(self.max_transfers),
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
        ]
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
        if let Some(v) = string("adsorbate_tag")? {
            self.adsorbate_tag = v;
        }
        if let Some(v) = string("substrate_tag")? {
            self.substrate_tag = v;
        }
        if let Some(v) = float("reach")? {
            self.reach = v;
        }
        if let Some(v) = float("pair_tolerance")? {
            self.pair_tolerance = v;
        }
        if let Some(v) = int("max_formed_bonds")? {
            self.max_formed_bonds = v;
        }
        if let Some(v) = int("max_transfers")? {
            self.max_transfers = v;
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
        Ok(())
    }
}

pub fn get_node_type() -> NodeType {
    let named = |name: &str| DataType::Record(RecordType::Named(name.to_string()));
    NodeType {
        name: "chemisorb".to_string(),
        description: "Enumerates every way a posed adsorbate can bond to a substrate, relaxes \
                      each with UFF and ranks them. One search is one pose: the adsorbate as \
                      wired, over the substrate as wired.\n\
                      \n\
                      **The search runs only when you press Run** (panel, or `run` in the \
                      CLI). Until then, and whenever an input or a search setting changes \
                      after a run, the node shows the *plan*: `best` is the unrelaxed pose, \
                      `candidates` is empty and `stats` counts the hypotheses a run would \
                      relax (`searched` false, `stale` true after an earlier run). Results are \
                      not saved with the file.\n\
                      \n\
                      A **site** is a substrate reactive atom with a free valence; a hydrogen \
                      on the substrate blocks its host. Each adsorbate reactive atom with a \
                      free valence forms at most one bond, to a site within **reach** (Å). \
                      Every partial binding is enumerated too, up to **max_formed_bonds** \
                      (0 = no cap). **pair_tolerance** (Å, 0 = off) prunes multi-bond \
                      patterns whose site spacing differs from the adsorbate atoms' spacing by \
                      more than this. **adsorbate_tag** / **substrate_tag** restrict the \
                      reactive atoms (empty = all). **budget** caps the relaxations; a \
                      truncated search is not exhaustive.\n\
                      \n\
                      **Transfers** (optional `transfers` pin, an array of `ChemisorbTransfer` \
                      records): a monovalent atom (H or a halogen) moves from its only \
                      neighbour on one side to an atom with a free valence on the other, \
                      within **reach** of the moving atom. `to_substrate` lets an OH leg hand \
                      its H to a site so its O can bond; `to_adsorbate` lets a radical foot \
                      abstract surface H. The donor must be a reactive atom (the tag selects \
                      donors, never the H). **max_transfers** (default 1) caps them per \
                      pattern, over all records.\n\
                      \n\
                      **Score** = UFF strain + bond-energy term (mean bond enthalpies, \
                      kcal/mol, relative to the same pose with no bonds formed); lower is \
                      better. The bond term dominates: more bonds usually rank first, so read \
                      the strain and `worst_bond_ratio` before trusting rank 1. **top_n** and \
                      **energy_window** (kcal/mol above the best) choose the listed \
                      candidates. Changed atoms carry the `cs_changed` tag."
            .to_string(),
        summary: Some("Enumerate and rank chemisorption bonding patterns".to_string()),
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
        output_pins: vec![
            OutputPinDefinition::fixed("best", DataType::Molecule),
            OutputPinDefinition::fixed(
                "candidates",
                DataType::Array(Box::new(named(CHEMISORB_CANDIDATE_RECORD))),
            ),
            OutputPinDefinition::fixed("stats", named(CHEMISORB_STATS_RECORD)),
        ],
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(ChemisorbData::default()),
        node_data_saver: generic_node_data_saver::<ChemisorbData>,
        node_data_loader: generic_node_data_loader::<ChemisorbData>,
    }
}
