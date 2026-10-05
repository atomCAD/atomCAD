//! `evaluate`: the relaxation half of a search, and what it reports.

use super::config::{CHANGED_TAG, ChemisorptionError, ChemisorptionSearch};
use super::enumerate::{Hypothesis, HypothesisKey, SearchPlan, change_key, plan};
use super::inventory::BondInventory;
use super::relax::{Relaxed, StrainTerms, relax};
use super::transfer::{Transfer, apply_transfers};
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::BOND_SINGLE;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::time::Instant;

/// One hypothesis after relaxation.
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
    /// `ΔE_UFF` against the reference state (kcal/mol): the ranking key,
    /// lower is better. Between candidates of different bond inventories it is
    /// not purely strain — a different bond graph shifts the UFF energy too.
    pub strain: f64,
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
    pub duplicates: usize,
    /// Valid hypotheses found by `plan`.
    pub to_relax: usize,
    /// Hypotheses relaxed; equals `to_relax`.
    pub relaxed: usize,
    pub unconverged: usize,
    /// The budget was hit; the search is **not** exhaustive.
    pub truncated: bool,
    /// Wall time of the `plan` half (s).
    pub plan_seconds: f64,
    /// Wall time of the whole search (s).
    pub seconds: f64,
}

#[derive(Debug, Clone)]
pub struct SearchReport {
    /// The pose with no bond changes, relaxed the same way. Its `strain` is
    /// zero by definition.
    pub reference: Candidate,
    /// Ranked by strain, ties broken by the normalized bond set; deduplicated.
    pub candidates: Vec<Candidate>,
    pub stats: SearchStats,
}

/// Which candidates a caller lists. Applied to a finished, ranked report:
/// nothing here is a search setting, so changing it never calls for a new
/// search.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    /// Only candidates with exactly this many formed bonds (transfers not
    /// counted). `None` = any count.
    pub formed_bonds: Option<usize>,
    /// Only candidates whose bond inventory reads exactly this
    /// (`BondInventory`'s display form, e.g. `"formed 2× O–Si"`). `None` = any.
    pub inventory: Option<String>,
    /// At most this many listed…
    pub top_n: usize,
    /// …and only those within this many kcal/mol of the best candidate that
    /// passes the two filters above.
    pub energy_window: f64,
}

impl Listing {
    /// Whether `c` passes the two filters (not the window or the count).
    pub fn admits(&self, c: &Candidate) -> bool {
        self.formed_bonds.is_none_or(|n| c.formed.len() == n)
            && self
                .inventory
                .as_ref()
                .is_none_or(|label| c.bond_inventory.to_string() == *label)
    }
}

/// What a [`Listing`] picked from a ranked candidate list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listed {
    /// Indices into the candidate list, in rank order.
    pub indices: Vec<usize>,
    /// Candidates that pass the filters, listed or not; the rest of the
    /// candidates were filtered out, and `matching - indices.len()` are past
    /// `top_n` or the energy window.
    pub matching: usize,
}

/// Applies a [`Listing`] to a ranked candidate list. The window is measured
/// from the best candidate that passes the filters, so it works within one
/// formed-bond count or one inventory.
pub fn list_candidates(candidates: &[Candidate], listing: &Listing) -> Listed {
    let matching: Vec<usize> = (0..candidates.len())
        .filter(|&i| listing.admits(&candidates[i]))
        .collect();
    let Some(&best) = matching.first() else {
        return Listed::default();
    };
    let best = candidates[best].strain;
    let indices = matching
        .iter()
        .copied()
        .take(listing.top_n)
        .take_while(|&i| candidates[i].strain - best <= listing.energy_window)
        .collect();
    Listed {
        indices,
        matching: matching.len(),
    }
}

/// The distinct bond inventories among `inventories`, as display labels with
/// how many there are of each, restricted to `formed_bonds` formed bonds when
/// it is set. Ordered by formed-bond count, then label — an inventory is
/// topology, not a ranking. The input is each candidate's (or hypothesis's)
/// inventory with its formed-bond count.
pub fn inventory_options<'a>(
    inventories: impl IntoIterator<Item = (&'a BondInventory, usize)>,
    formed_bonds: Option<usize>,
) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<(usize, String), usize> = Default::default();
    for (inventory, formed) in inventories {
        if formed_bonds.is_none_or(|n| n == formed) {
            *counts.entry((formed, inventory.to_string())).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .map(|((_, label), n)| (label, n))
        .collect()
}

/// Sorts by strain, lowest first, ties broken by the normalized bond set, so
/// the order never depends on which relaxation finished first (R10).
pub fn rank_candidates(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| match a.strain.total_cmp(&b.strain) {
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
) -> Candidate {
    Candidate {
        structure,
        formed: h.formed.clone(),
        transfers: h.transfers.clone(),
        bond_inventory: h.inventory.clone(),
        strain: relaxed.energy - reference.energy,
        terms: relaxed.terms.minus(&reference.terms),
        converged: relaxed.converged,
        worst_bond_ratio: relaxed.worst_bond_ratio,
        energy: relaxed.energy,
    }
}

/// Relaxes the reference state and every planned hypothesis (in parallel)
/// and ranks them. The order is independent of which relaxation
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
    );

    let mut candidates = plan
        .hypotheses
        .par_iter()
        .map(|h| {
            let mut s = apply(&plan.combined, h);
            let relaxed = relax(&mut s, config)?;
            tag_changed(&mut s, h)?;
            Ok(candidate(s, h, &relaxed, &reference_relaxed))
        })
        .collect::<Result<Vec<_>, ChemisorptionError>>()?;

    rank_candidates(&mut candidates);

    let p = &plan.stats;
    let stats = SearchStats {
        feet: p.feet,
        sites_in_reach: p.sites_in_reach,
        transfer_candidates: p.transfer_candidates,
        considered: p.considered,
        pruned_valence: p.pruned_valence,
        duplicates: p.duplicates,
        to_relax: p.to_relax,
        relaxed: candidates.len(),
        unconverged: candidates.iter().filter(|c| !c.converged).count(),
        truncated: p.truncated,
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
