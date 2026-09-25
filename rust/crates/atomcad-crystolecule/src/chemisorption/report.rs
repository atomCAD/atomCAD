//! `evaluate`: the relaxation half of a search, and what it reports.

use super::config::{CHANGED_TAG, ChemisorptionError, ChemisorptionSearch};
use super::enumerate::{Hypothesis, HypothesisKey, SearchPlan, change_key, plan};
use super::relax::{Relaxed, StrainTerms, relax};
use super::score::{BondInventory, BondKind};
use super::transfer::{Transfer, apply_transfers};
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::BOND_SINGLE;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::time::Instant;

/// One hypothesis after relaxation and scoring.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Adsorbate + substrate, relaxed; the atoms whose bonds changed carry
    /// [`CHANGED_TAG`]. Atom ids are those of [`SearchPlan::combined`].
    pub structure: AtomicStructure,
    /// Bonds formed between an adsorbate atom and a site,
    /// `(adsorbate atom, substrate atom)`; transfers are not here.
    pub formed: Vec<(u32, u32)>,
    /// Transfers: each broke `donor–moved` and formed `moved–acceptor`.
    pub transfers: Vec<Transfer>,
    pub bond_inventory: BondInventory,
    /// `ΔE_UFF` against the reference state (kcal/mol).
    pub strain: f64,
    /// `ΔE_bond = Σ D(broken) − Σ D(formed)` (kcal/mol).
    pub bond_energy: f64,
    /// `strain + bond_energy`, the ranking key; lower is better.
    pub score: f64,
    /// `bond_energy` uses a Pauling estimate.
    pub estimated: bool,
    /// Strain by UFF term, against the reference state; sums to `strain`.
    pub terms: StrainTerms,
    pub converged: bool,
    pub worst_bond_ratio: f64,
    /// Absolute UFF energy of the relaxed structure (kcal/mol).
    pub energy: f64,
}

impl Candidate {
    /// The normalized bond set, the tie-breaker of the ranking (R10).
    pub fn key(&self) -> HypothesisKey {
        change_key(&self.formed, &self.transfers)
    }

    /// Every bond this candidate broke: one `(donor, moved)` per transfer.
    pub fn broken(&self) -> Vec<(u32, u32)> {
        self.transfers.iter().map(|t| (t.donor, t.moved)).collect()
    }

    /// Every atom this candidate moved to the other side.
    pub fn moved(&self) -> Vec<u32> {
        self.transfers.iter().map(|t| t.moved).collect()
    }
}

/// The whole search, including candidates a caller does not list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchStats {
    pub feet: usize,
    pub sites_in_reach: usize,
    pub transfer_candidates: usize,
    pub considered: usize,
    pub pruned_valence: usize,
    pub pruned_pair_tolerance: usize,
    pub duplicates: usize,
    /// Valid hypotheses found by `plan`.
    pub to_relax: usize,
    /// Hypotheses relaxed; equals `to_relax`.
    pub relaxed: usize,
    pub unconverged: usize,
    /// The budget was hit; the search is **not** exhaustive.
    pub truncated: bool,
    pub estimated_pairs: Vec<BondKind>,
    /// Wall time of the `plan` half (s).
    pub plan_seconds: f64,
    /// Wall time of the whole search (s).
    pub seconds: f64,
}

#[derive(Debug, Clone)]
pub struct SearchReport {
    /// The pose with no bond changes, relaxed the same way. Its `strain`,
    /// `bond_energy` and `score` are zero by definition.
    pub reference: Candidate,
    /// Ranked by score, ties broken by the normalized bond set; deduplicated.
    pub candidates: Vec<Candidate>,
    pub stats: SearchStats,
}

/// How many leading candidates a caller lists: at most `top_n`, and only
/// those within `energy_window` (kcal/mol) of the best score.
pub fn listed_count(candidates: &[Candidate], top_n: usize, energy_window: f64) -> usize {
    let Some(best) = candidates.first() else {
        return 0;
    };
    candidates
        .iter()
        .take(top_n)
        .take_while(|c| c.score - best.score <= energy_window)
        .count()
}

/// Sorts by score, lowest first, ties broken by the normalized bond set, so
/// the order never depends on which relaxation finished first (R10).
pub fn rank_candidates(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| match a.score.total_cmp(&b.score) {
        Ordering::Equal => a.key().cmp(&b.key()),
        other => other,
    });
}

/// Applies a hypothesis's bond changes to a copy of the combined structure:
/// its transfers (each moved atom re-seated on its acceptor), then its formed
/// bonds.
fn apply(combined: &AtomicStructure, h: &Hypothesis) -> AtomicStructure {
    let mut s = combined.clone();
    apply_transfers(&mut s, &h.transfers);
    for &(a, b) in &h.formed {
        s.add_bond_checked(a, b, BOND_SINGLE);
    }
    s
}

fn tag_changed(s: &mut AtomicStructure, h: &Hypothesis) -> Result<(), ChemisorptionError> {
    for id in h.changed_atoms() {
        s.add_atom_tag(id, CHANGED_TAG)?;
    }
    Ok(())
}

fn candidate(
    structure: AtomicStructure,
    h: &Hypothesis,
    relaxed: &Relaxed,
    reference: &Relaxed,
) -> Result<Candidate, ChemisorptionError> {
    let (bond_energy, estimated) = h.inventory.bond_energy()?;
    let strain = relaxed.energy - reference.energy;
    Ok(Candidate {
        structure,
        formed: h.formed.clone(),
        transfers: h.transfers.clone(),
        bond_inventory: h.inventory.clone(),
        strain,
        bond_energy,
        score: strain + bond_energy,
        estimated,
        terms: relaxed.terms.minus(&reference.terms),
        converged: relaxed.converged,
        worst_bond_ratio: relaxed.worst_bond_ratio,
        energy: relaxed.energy,
    })
}

/// Relaxes the reference state and every planned hypothesis (in parallel),
/// scores them and ranks them. The order is independent of which relaxation
/// finishes first (R10).
pub fn evaluate(
    plan: &SearchPlan,
    config: &ChemisorptionSearch,
) -> Result<SearchReport, ChemisorptionError> {
    let start = Instant::now();
    let no_change = Hypothesis {
        formed: Vec::new(),
        transfers: Vec::new(),
        inventory: BondInventory::default(),
    };

    let mut reference_structure = plan.combined.clone();
    let reference_relaxed = relax(&mut reference_structure, config)?;
    let reference = candidate(
        reference_structure,
        &no_change,
        &reference_relaxed,
        &reference_relaxed,
    )?;

    let mut candidates = plan
        .hypotheses
        .par_iter()
        .map(|h| {
            let mut s = apply(&plan.combined, h);
            let relaxed = relax(&mut s, config)?;
            tag_changed(&mut s, h)?;
            candidate(s, h, &relaxed, &reference_relaxed)
        })
        .collect::<Result<Vec<_>, _>>()?;

    rank_candidates(&mut candidates);

    let p = &plan.stats;
    let stats = SearchStats {
        feet: p.feet,
        sites_in_reach: p.sites_in_reach,
        transfer_candidates: p.transfer_candidates,
        considered: p.considered,
        pruned_valence: p.pruned_valence,
        pruned_pair_tolerance: p.pruned_pair_tolerance,
        duplicates: p.duplicates,
        to_relax: p.to_relax,
        relaxed: candidates.len(),
        unconverged: candidates.iter().filter(|c| !c.converged).count(),
        truncated: p.truncated,
        estimated_pairs: p.estimated_pairs.clone(),
        plan_seconds: p.seconds,
        seconds: p.seconds + start.elapsed().as_secs_f64(),
    };
    Ok(SearchReport {
        reference,
        candidates,
        stats,
    })
}

/// `plan` then `evaluate`: one whole search of one pose.
pub fn search(
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &ChemisorptionSearch,
) -> Result<SearchReport, ChemisorptionError> {
    let planned = plan(adsorbate, substrate, config)?;
    evaluate(&planned, config)
}
