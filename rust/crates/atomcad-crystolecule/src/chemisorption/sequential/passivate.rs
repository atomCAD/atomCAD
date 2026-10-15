//! `passivate_sites` (`doc/design_chemisorb_mirrors_and_passivation.md` §4):
//! a candidate is relaxed with every open valence its sites have left capped
//! by H, and its strain is taken against a reference that carries the same H.
//!
//! - **The hosts are the search's sites**, and what is capped is what the
//!   candidate's bonds left: an unused site in full, a used site its leftover
//!   valence, an acceptor whatever its transferred atom did not take. The
//!   slot count is `open_valence_slots` (the passivation code's), never the
//!   search's `free_valence`. No H is skipped (D8), and a site's
//!   hydrogen-passivation flag is ignored: a site made by deleting a surface
//!   H keeps it.
//! - **A reference is a function of its key**, the `(host, count)` list the
//!   candidate's passivation placed: the input substrate with the first
//!   `count` slots of each host capped, relaxed alone, plus the adsorbate
//!   relaxed alone. Never built from the candidate that first needed it,
//!   whose identity depends on the thread count.
//! - **References are relaxed before the candidates that need them**, so a
//!   strain is known the moment its relaxation ends (`keep_best` ranks while
//!   relaxing). The key is a function of the bond changes alone.

use super::config::SequentialSearch;
use super::evaluate::check_cancelled;
use super::plan::SequentialPlan;
use super::setup::{Setup, Step};
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::config::ChemisorptionError;
use crate::chemisorption::relax::{RelaxSettings, Relaxed, StrainTerms, relax};
use crate::hydrogen_passivation::{add_terminators_counted, open_valence_slots};
use atomcad_util::job_control::JobControl;
use glam::DVec3;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

/// The passivant: hydrogen only (D4a).
pub const PASSIVANT: i16 = 1;

/// What one candidate's passivation placed: `(host, count)`, combined host
/// ids, sorted by host, hosts with no H left out. A reference's key.
pub type PassivationKey = Vec<(u32, usize)>;

/// A separated reference state: `E(adsorbate alone) + E(substrate alone)`,
/// the substrate carrying a key's H, and the same split by UFF term.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reference {
    pub energy: f64,
    pub terms: StrainTerms,
}

impl Reference {
    /// The sum of the two relaxations of a separated state.
    pub fn of(adsorbate: &Relaxed, substrate: &Relaxed) -> Self {
        let (a, s) = (&adsorbate.terms, &substrate.terms);
        Self {
            energy: adsorbate.energy + substrate.energy,
            terms: StrainTerms {
                stretch: a.stretch + s.stretch,
                bend: a.bend + s.bend,
                torsion: a.torsion + s.torsion,
                inversion: a.inversion + s.inversion,
                vdw: a.vdw + s.vdw,
            },
        }
    }
}

impl Setup {
    /// The H `passivate` would place on `s`: every site's open slots, as the
    /// structure's bonds leave them.
    pub fn passivation_key(&self, s: &AtomicStructure) -> PassivationKey {
        self.sites
            .iter()
            .filter_map(|site| {
                let n = open_valence_slots(s, site.id, PASSIVANT);
                (n > 0).then_some((site.id, n))
            })
            .collect()
    }

    /// Caps every open valence of every site of `s` with H (§4.1). Returns
    /// the key and the added atoms' ids, in placement order.
    pub fn passivate(&self, s: &mut AtomicStructure) -> (PassivationKey, Vec<u32>) {
        let key = self.passivation_key(s);
        let added = add_terminators_counted(s, &key, PASSIVANT);
        (key, added)
    }

    /// A passivated local-phase candidate rebuilt from what a level keeps of
    /// it: the state (`Setup::state_structure`) with `key`'s H added, the H
    /// at `passivants` (in placement order). Returns the structure and the H
    /// ids.
    pub fn passivated_state_structure(
        &self,
        steps: &[Step],
        moved: &[u32],
        positions: &[DVec3],
        key: &PassivationKey,
        passivants: &[DVec3],
    ) -> (AtomicStructure, Vec<u32>) {
        let mut s = self.state_structure(steps, moved, positions);
        let added = add_terminators_counted(&mut s, key, PASSIVANT);
        debug_assert_eq!(added.len(), passivants.len());
        for (&id, &p) in added.iter().zip(passivants) {
            s.set_atom_position(id, p);
        }
        (s, added)
    }

    /// The reference substrate of `key` (§4.4): the input `substrate` with
    /// each host's first `count` open slots capped. Built from the key alone.
    pub fn passivated_substrate(
        &self,
        substrate: &AtomicStructure,
        key: &PassivationKey,
    ) -> AtomicStructure {
        let input: FxHashMap<u32, u32> = self
            .substrate_ids
            .iter()
            .map(|(&input, &combined)| (combined, input))
            .collect();
        let hosts: Vec<(u32, usize)> = key.iter().map(|&(host, n)| (input[&host], n)).collect();
        let mut s = substrate.clone();
        add_terminators_counted(&mut s, &hosts, PASSIVANT);
        s
    }
}

/// The reference of `key`, relaxed from scratch: the adsorbate alone and the
/// passivated substrate alone. What `evaluate` keeps per key, recomputed
/// without its map.
pub fn passivated_reference(
    plan: &SequentialPlan,
    config: &SequentialSearch,
    key: &PassivationKey,
) -> Result<Reference, ChemisorptionError> {
    let settings = config.relax_settings();
    let mut alone = plan.adsorbate.clone();
    let adsorbate = relax(&mut alone, &settings)?;
    let mut alone = plan.setup.passivated_substrate(&plan.substrate, key);
    let substrate = relax(&mut alone, &settings)?;
    Ok(Reference::of(&adsorbate, &substrate))
}

/// The references one search has relaxed, by key, and the adsorbate's own
/// relaxation they all share.
pub(crate) struct References {
    pub adsorbate: Relaxed,
    pub map: BTreeMap<PassivationKey, Reference>,
}

impl References {
    /// Relaxes the references of `keys` not yet in the map, in parallel, in
    /// key order. Raises the job's total by their number before relaxing,
    /// and advances it by one per reference.
    pub fn ensure(
        &mut self,
        plan: &SequentialPlan,
        settings: &RelaxSettings,
        keys: BTreeSet<PassivationKey>,
        control: Option<&JobControl>,
    ) -> Result<(), ChemisorptionError> {
        let missing: Vec<PassivationKey> = keys
            .into_iter()
            .filter(|k| !self.map.contains_key(k))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        if let Some(c) = control {
            let total = c.snapshot().total.unwrap_or(0);
            c.set_total(total + missing.len() as u64);
        }
        let relaxed: Vec<Reference> = missing
            .par_iter()
            .map(|key| -> Result<Reference, ChemisorptionError> {
                check_cancelled(control)?;
                let mut s = plan.setup.passivated_substrate(&plan.substrate, key);
                let substrate = relax(&mut s, settings)?;
                if let Some(c) = control {
                    c.advance(1);
                }
                Ok(Reference::of(&self.adsorbate, &substrate))
            })
            .collect::<Result<_, _>>()?;
        self.map.extend(missing.into_iter().zip(relaxed));
        Ok(())
    }

    pub fn get(&self, key: &PassivationKey) -> Reference {
        *self
            .map
            .get(key)
            .expect("a reference is relaxed before the candidates that need it")
    }
}
