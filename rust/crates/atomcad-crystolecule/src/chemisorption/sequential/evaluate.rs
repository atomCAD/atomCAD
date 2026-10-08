//! `evaluate`: relaxes the planned candidates from their seatings and ranks
//! them against the separated reference state (§4.8 of the design).

use super::config::SequentialSearch;
use super::plan::{PlanStats, SequentialPlan, plan};
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::config::{CHANGED_TAG, ChemisorptionError};
use crate::chemisorption::enumerate::{HypothesisKey, change_key};
use crate::chemisorption::inventory::BondInventory;
use crate::chemisorption::relax::{StrainTerms, relax};
use crate::chemisorption::transfer::Transfer;
use atomcad_util::job_control::JobControl;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::Instant;

/// One relaxed candidate.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Adsorbate + substrate, relaxed; the atoms whose bonds changed carry
    /// [`CHANGED_TAG`]. Atom ids are those of the plan's combined structure.
    pub structure: AtomicStructure,
    /// Index into `SequentialPlan::hypotheses`, and its canonical tree row.
    pub hypothesis: usize,
    pub row: u32,
    /// Bonds formed, `(foot, site)`, in binding order; its length is the leg
    /// count.
    pub formed: Vec<(u32, u32)>,
    pub transfers: Vec<Transfer>,
    pub bond_inventory: BondInventory,
    /// `E(candidate) − E(separated reference)` (kcal/mol): the ranking key.
    pub strain: f64,
    /// Strain by UFF term; sums to `strain`.
    pub terms: StrainTerms,
    pub converged: bool,
    pub worst_bond_ratio: f64,
    /// Absolute UFF energy of the relaxed structure (kcal/mol).
    pub energy: f64,
    /// Its start geometry clashed (§4.5). Only possible with the clash filter
    /// off.
    pub seating_clash: bool,
}

impl Candidate {
    pub fn key(&self) -> HypothesisKey {
        change_key(&self.formed, &self.transfers)
    }
}

/// One relaxation's outcome, kept for every relaxed hypothesis (no
/// structure), so the debug tree can show every strain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelaxedRow {
    pub hypothesis: usize,
    pub strain: f64,
    pub converged: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchStats {
    pub plan: PlanStats,
    /// UFF relaxations of hypotheses (the reference's two not counted).
    pub relaxed: usize,
    pub unconverged: usize,
    /// Wall time of the whole search (s), `plan` included.
    pub seconds: f64,
}

#[derive(Debug, Clone)]
pub struct SearchReport {
    /// `E(adsorbate relaxed alone) + E(substrate relaxed alone)`.
    pub reference_energy: f64,
    pub reference_terms: StrainTerms,
    /// The best `top_n` by strain (ties by the change set), cut to the energy
    /// window above the first.
    pub candidates: Vec<Candidate>,
    /// Every relaxed hypothesis, in plan order.
    pub relaxed: Vec<RelaxedRow>,
    pub stats: SearchStats,
}

fn rank_order(a: &Candidate, b: &Candidate) -> Ordering {
    a.strain
        .total_cmp(&b.strain)
        .then_with(|| a.key().cmp(&b.key()))
}

/// Adds `c` to `kept` (sorted, at most `top_n` long).
fn keep_best(kept: &mut Vec<Candidate>, c: Candidate, top_n: usize) {
    if kept.len() >= top_n
        && kept
            .last()
            .is_some_and(|worst| rank_order(&c, worst) != Ordering::Less)
    {
        return;
    }
    let at = kept.partition_point(|k| rank_order(k, &c) == Ordering::Less);
    kept.insert(at, c);
    kept.truncate(top_n);
}

fn check_cancelled(control: Option<&JobControl>) -> Result<(), ChemisorptionError> {
    match control {
        Some(c) if c.is_cancelled() => Err(ChemisorptionError::Cancelled),
        _ => Ok(()),
    }
}

/// Relaxes the separated reference and every planned candidate (in
/// parallel) and ranks them. Deterministic whatever order the relaxations
/// finish in. With a `control`, each relaxation advances it by one and a
/// cancel is honoured before each starts.
pub fn evaluate(
    plan: &SequentialPlan,
    config: &SequentialSearch,
    control: Option<&JobControl>,
) -> Result<SearchReport, ChemisorptionError> {
    let start = Instant::now();
    let settings = config.relax_settings();
    let advance = || {
        if let Some(c) = control {
            c.advance(1);
        }
    };

    check_cancelled(control)?;
    let mut alone = plan.adsorbate.clone();
    let adsorbate = relax(&mut alone, &settings)?;
    let mut alone = plan.substrate.clone();
    let substrate = relax(&mut alone, &settings)?;
    advance();
    let reference_energy = adsorbate.energy + substrate.energy;
    let t = (adsorbate.terms, substrate.terms);
    let reference_terms = StrainTerms {
        stretch: t.0.stretch + t.1.stretch,
        bend: t.0.bend + t.1.bend,
        torsion: t.0.torsion + t.1.torsion,
        inversion: t.0.inversion + t.1.inversion,
        vdw: t.0.vdw + t.1.vdw,
    };

    let kept: Mutex<Vec<Candidate>> = Mutex::new(Vec::with_capacity(config.top_n + 1));
    let unconverged = AtomicUsize::new(0);
    let calls = AtomicUsize::new(0);
    let relaxed: Vec<RelaxedRow> = plan
        .to_relax
        .par_iter()
        .map(|&index| -> Result<RelaxedRow, ChemisorptionError> {
            check_cancelled(control)?;
            let h = &plan.hypotheses[index];
            let seating = h.seating.as_ref().expect("a planned candidate is seated");
            let mut s = plan.setup.start_structure(&h.steps, seating);
            let r = relax(&mut s, &settings)?;
            calls.fetch_add(1, AtomicOrdering::Relaxed);
            advance();
            if !r.converged {
                unconverged.fetch_add(1, AtomicOrdering::Relaxed);
            }
            for id in h.changed_atoms() {
                s.add_atom_tag(id, CHANGED_TAG)?;
            }
            let strain = r.energy - reference_energy;
            let c = Candidate {
                structure: s,
                hypothesis: index,
                row: h.row,
                formed: h.formed.clone(),
                transfers: h.transfers.clone(),
                bond_inventory: h.inventory.clone(),
                strain,
                terms: r.terms.minus(&reference_terms),
                converged: r.converged,
                worst_bond_ratio: r.worst_bond_ratio,
                energy: r.energy,
                seating_clash: seating.clashes(),
            };
            keep_best(
                &mut kept.lock().expect("no panics under the lock"),
                c,
                config.top_n,
            );
            Ok(RelaxedRow {
                hypothesis: index,
                strain,
                converged: r.converged,
            })
        })
        .collect::<Result<_, _>>()?;
    let mut candidates = kept.into_inner().expect("no panics under the lock");
    if let Some(best) = candidates.first().map(|c| c.strain) {
        candidates.retain(|c| c.strain - best <= config.energy_window);
    }
    Ok(SearchReport {
        reference_energy,
        reference_terms,
        candidates,
        relaxed,
        stats: SearchStats {
            plan: plan.stats.clone(),
            relaxed: calls.into_inner(),
            unconverged: unconverged.into_inner(),
            seconds: plan.stats.seconds + start.elapsed().as_secs_f64(),
        },
    })
}

/// `plan` then `evaluate`. With a `control`: phase "Planning", then the total
/// (one per relaxation plus one for the reference) and phase "Relaxing".
pub fn search(
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &SequentialSearch,
    control: Option<&JobControl>,
) -> Result<SearchReport, ChemisorptionError> {
    if let Some(c) = control {
        c.set_phase("Planning");
    }
    let planned = plan(adsorbate, substrate, config)?;
    if let Some(c) = control {
        c.set_total(planned.to_relax.len() as u64 + 1);
        c.set_phase("Relaxing");
    }
    evaluate(&planned, config, control)
}
