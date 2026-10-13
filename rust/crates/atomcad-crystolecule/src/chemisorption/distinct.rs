//! What a chemisorption search lists: its kept candidates, tagged with the
//! atoms each binding involves and, with `unique` on, merged where two are the
//! same binding up to a rigid motion of their local cluster
//! (`doc/design_structure_comparison.md` §5, D14).
//!
//! This runs on a finished [`SearchReport`], never inside the search: the
//! settings here are **output settings** (D8). They are not
//! [`SequentialSearch`](super::sequential::SequentialSearch) fields, nothing
//! fingerprints them, and changing one re-lists a stored result without a new
//! search. `top_n` therefore counts candidates *before* merging (D7).
//!
//! Two tags are written on a clone of every kept candidate, whether `unique` is
//! on or off, so a comparison can be pointed at exactly what `unique` compared:
//!
//! - [`ADSORBATE_TAG`]: every atom that came from the adsorbate, a transferred
//!   H on the substrate included.
//! - [`LOCAL_TAG`]: the local cluster ([`local_cluster`]) — the adsorbate plus
//!   `local_hops` bonds into the substrate, H atoms as terminators.
//!
//! Grouping is the core's [`group_in_order`], in rank order, so the answer does
//! not depend on thread counts or the order relaxations finished in (D9).

use super::config::ChemisorptionError;
use super::sequential::{SearchReport, Setup};
use crate::atomic_structure::AtomicStructure;
use crate::proxy_cut::bond_distances_from;
use crate::structure_compare::{
    Cluster, CompareSettings, DEFAULT_TOLERANCE, MAX_TOLERANCE, Motion, group_in_order,
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// The tag on every atom that came from the `adsorbate` input.
pub const ADSORBATE_TAG: &str = "cs_adsorbate";
/// The tag on the local cluster `unique` compares.
pub const LOCAL_TAG: &str = "cs_local";

const HYDROGEN: i16 = 1;

/// How the kept candidates are listed. Every field defaults (D10), so a node
/// saved before these settings existed loads with them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DistinctSettings {
    /// The motion two equal bindings may differ by; `None` = `unique` off.
    /// `Some(Motion::None)` is shown as "exact".
    pub unique: Option<Motion>,
    /// The comparison tolerance (Å), in `(0, MAX_TOLERANCE]`.
    pub tolerance: f64,
    /// Bonds into the substrate the local cluster reaches.
    pub local_hops: usize,
}

impl Default for DistinctSettings {
    fn default() -> Self {
        DistinctSettings {
            unique: Some(Motion::RigidMirror),
            tolerance: DEFAULT_TOLERANCE,
            local_hops: 2,
        }
    }
}

impl DistinctSettings {
    /// Rejects settings no comparison can honour. The tolerance is checked
    /// with `unique` off too, so a bad value is reported where it was typed,
    /// not when `unique` is next switched on.
    pub fn validate(&self) -> Result<(), ChemisorptionError> {
        if !(self.tolerance > 0.0 && self.tolerance <= MAX_TOLERANCE) {
            return Err(ChemisorptionError::InvalidConfig(format!(
                "the unique tolerance must be in (0, {MAX_TOLERANCE}] Å, got {}",
                self.tolerance
            )));
        }
        Ok(())
    }

    /// The comparison `unique` runs: bonds always compared (two clusters with
    /// the same positions and different bonds are different bindings). `None`
    /// with `unique` off.
    pub fn compare_settings(&self) -> Option<CompareSettings> {
        self.unique.map(|motion| CompareSettings {
            motion,
            tolerance: self.tolerance,
            compare_bonds: true,
            ..CompareSettings::default()
        })
    }
}

/// One kept candidate, as it is listed or merged.
#[derive(Debug, Clone)]
pub struct DistinctCandidate {
    /// The candidate's structure with [`ADSORBATE_TAG`] and [`LOCAL_TAG`].
    pub structure: AtomicStructure,
    /// The representative's index in the kept list; `None` when listed.
    pub copy_of: Option<usize>,
    /// Kept candidates merged into this one; 0 for a copy.
    pub copies: usize,
}

/// The kept candidates, listed or merged.
#[derive(Debug, Clone)]
pub struct Distinct {
    /// One per kept candidate, in rank order (`SearchReport::candidates`).
    pub candidates: Vec<DistinctCandidate>,
    /// Candidates merged into an earlier one.
    pub merged: usize,
    /// Pairs that hit the comparison's work cap: counted as not equal.
    pub undecided: usize,
}

impl Distinct {
    /// The listed candidates' indices in the kept list, ascending.
    pub fn listed(&self) -> impl Iterator<Item = usize> + '_ {
        self.candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.copy_of.is_none())
            .map(|(i, _)| i)
    }
}

/// The local cluster of a binding (§5.2): starting from `seeds` (the
/// adsorbate's atoms), `hops` bonds outward. An H reached is included but not
/// expanded from — a terminator — while a seed H is expanded from, so a
/// transferred H reaches its acceptor at hop 1. Finally every H bonded to an
/// included atom is added, so a neighbour's passivation (whether it is a
/// radical) is part of the cluster. Sorted.
pub fn local_cluster(structure: &AtomicStructure, seeds: &[u32], hops: usize) -> Vec<u32> {
    let terminators: FxHashMap<u32, u32> = structure
        .iter_atoms()
        .filter(|(_, a)| a.atomic_number == HYDROGEN)
        .map(|(&id, _)| (id, id))
        .collect();
    let distance = bond_distances_from(structure, seeds, &terminators, true);
    let mut included: Vec<u32> = distance
        .into_iter()
        .filter(|&(_, d)| d as usize <= hops)
        .map(|(id, _)| id)
        .collect();
    let capping: Vec<u32> = included
        .iter()
        .filter_map(|&id| structure.get_atom(id))
        .flat_map(|a| a.bonds.iter().filter(|b| !b.is_delete_marker()))
        .map(|b| b.other_atom_id())
        .filter(|n| terminators.contains_key(n))
        .collect();
    included.extend(capping);
    included.sort_unstable();
    included.dedup();
    included
}

/// Tags every kept candidate of `report` and, with `unique` on, merges the
/// equal ones (§5.3): each is compared, in rank order, with the candidates
/// listed before it, and merged into the first equal one. `setup` is the
/// search's own (`SequentialPlan::setup`): candidate atom ids are its
/// combined structure's.
///
/// `Err` for invalid settings, and when a candidate cannot take the two tags
/// (its input already uses every tag slot).
pub fn distinct(
    setup: &Setup,
    report: &SearchReport,
    settings: &DistinctSettings,
) -> Result<Distinct, ChemisorptionError> {
    settings.validate()?;
    let compare = settings.compare_settings();
    let mut structures = Vec::with_capacity(report.candidates.len());
    let mut clusters = Vec::new();
    for candidate in &report.candidates {
        let mut structure = candidate.structure.clone();
        for &id in &setup.adsorbate_atoms {
            structure.add_atom_tag(id, ADSORBATE_TAG)?;
        }
        let local = local_cluster(&structure, &setup.adsorbate_atoms, settings.local_hops);
        for &id in &local {
            structure.add_atom_tag(id, LOCAL_TAG)?;
        }
        if compare.is_some() {
            clusters.push(Cluster::from_atoms(&structure, &local)?);
        }
        structures.push(structure);
    }

    let (class_of, copies, undecided) = match compare {
        Some(compare) => {
            let grouping = group_in_order(&clusters, &compare)?;
            (grouping.class_of, grouping.copies, grouping.undecided)
        }
        None => (
            (0..structures.len()).collect(),
            vec![0; structures.len()],
            0,
        ),
    };
    let candidates: Vec<DistinctCandidate> = structures
        .into_iter()
        .enumerate()
        .map(|(i, structure)| DistinctCandidate {
            structure,
            copy_of: (class_of[i] != i).then_some(class_of[i]),
            copies: copies[i],
        })
        .collect();
    let merged = candidates.iter().filter(|c| c.copy_of.is_some()).count();
    Ok(Distinct {
        candidates,
        merged,
        undecided,
    })
}
