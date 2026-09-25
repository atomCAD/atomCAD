//! `plan`: the enumeration half of a search. It combines the two inputs, finds
//! the sites and the adsorbate atoms that can bond, and enumerates every
//! bonding pattern the rules allow — without relaxing anything, so it is cheap
//! enough to run on every evaluation.
//!
//! The order is §5.2 of the design: first a set of transfers (none when no
//! transfer rule is enabled), because a transfer changes valences both ways —
//! its donor gains one (the OH leg that can now bond), its acceptor loses one;
//! then, against the valences that set leaves, the depth-first site
//! assignment of the bond-forming feet.

use super::config::{ChemisorptionError, ChemisorptionSearch, Side};
use super::score::{BondInventory, BondKind, is_scored_element, tabulated_enthalpy_kj};
use super::transfer::{SideAtoms, Transfer, candidate_transfers};
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DELETED, BOND_DOUBLE, BOND_QUADRUPLE, BOND_SINGLE, BOND_TRIPLE,
};
use crate::guided_placement::{covalent_max_neighbors, detect_hybridization};
use glam::DVec3;
use rustc_hash::FxHashMap;
use std::collections::{BTreeSet, HashSet};
use std::time::Instant;

/// One bonding pattern before relaxation: a non-empty set of bond changes.
/// Atom ids are those of [`SearchPlan::combined`].
#[derive(Debug, Clone, PartialEq)]
pub struct Hypothesis {
    /// Bonds formed between an adsorbate atom and a site,
    /// `(adsorbate atom, substrate atom)`, in adsorbate-atom order. The bond a
    /// transferred atom forms with its acceptor is not here but in
    /// `transfers`.
    pub formed: Vec<(u32, u32)>,
    /// Transfers, in enumeration order.
    pub transfers: Vec<Transfer>,
    pub inventory: BondInventory,
}

/// The normalized bond set a hypothesis is deduplicated and tie-broken by:
/// the formed pairs `(min, max)`, sorted, and the transfers as
/// `(donor, element, acceptor)`, sorted — the moving atom by its donor and
/// element rather than its id, so equivalent atoms on one donor count once.
pub type HypothesisKey = (Vec<(u32, u32)>, Vec<(u32, i16, u32)>);

/// The key of a bond-change set, shared by hypotheses and candidates.
pub fn change_key(formed: &[(u32, u32)], transfers: &[Transfer]) -> HypothesisKey {
    let pairs: BTreeSet<(u32, u32)> = formed.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
    let mut moves: Vec<(u32, i16, u32)> = transfers.iter().map(Transfer::key).collect();
    moves.sort_unstable();
    (pairs.into_iter().collect(), moves)
}

/// The atoms whose bonds a change set touches, sorted: both ends of every
/// formed bond, and every transfer's donor, moving atom and acceptor.
pub fn changed_atoms(formed: &[(u32, u32)], transfers: &[Transfer]) -> Vec<u32> {
    let set: BTreeSet<u32> = formed
        .iter()
        .flat_map(|&(a, b)| [a, b])
        .chain(
            transfers
                .iter()
                .flat_map(|t| [t.donor, t.moved, t.acceptor]),
        )
        .collect();
    set.into_iter().collect()
}

impl Hypothesis {
    pub fn key(&self) -> HypothesisKey {
        change_key(&self.formed, &self.transfers)
    }

    /// The atoms whose bonds this hypothesis changes, sorted.
    pub fn changed_atoms(&self) -> Vec<u32> {
        changed_atoms(&self.formed, &self.transfers)
    }
}

/// What `plan` found and counted. Every branch of the search ends exactly
/// once, so
/// `considered == pruned_valence + pruned_pair_tolerance + duplicates + to_relax`
/// — plus the one complete assignment that tripped the budget, when
/// `truncated`. A transfer set that breaks a valence, or repeats an earlier
/// one, is one considered (and pruned, or duplicate) branch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanStats {
    /// Adsorbate reactive atoms that could form a bond (a free valence, after
    /// some transfer set, and a site within reach).
    pub feet: usize,
    /// Distinct sites within reach of at least one of them.
    pub sites_in_reach: usize,
    /// Candidate transfers `(D, X, A)` the transfer rules allow; 0 without
    /// rules.
    pub transfer_candidates: usize,
    /// Assignments considered: pruned partial ones and complete ones.
    pub considered: usize,
    /// Rejected because an atom would exceed its valence (R6).
    pub pruned_valence: usize,
    /// Rejected by the pair tolerance (R3).
    pub pruned_pair_tolerance: usize,
    /// Merged with an earlier one of the same bond set.
    pub duplicates: usize,
    /// Valid, deduplicated hypotheses: the relaxations `evaluate` will run.
    pub to_relax: usize,
    /// The budget was hit; the enumeration is **not** exhaustive.
    pub truncated: bool,
    /// Bond kinds a hypothesis may form or break whose enthalpy is a Pauling
    /// estimate.
    pub estimated_pairs: Vec<BondKind>,
    /// Wall time of `plan` (s).
    pub seconds: f64,
}

/// Everything `plan` decided, nothing relaxed.
#[derive(Debug, Clone)]
pub struct SearchPlan {
    /// Adsorbate and substrate merged, at the given pose, no bond changes.
    pub combined: AtomicStructure,
    /// Input atom id → combined id, per side.
    pub adsorbate_ids: FxHashMap<u32, u32>,
    pub substrate_ids: FxHashMap<u32, u32>,
    /// In enumeration order, which is deterministic.
    pub hypotheses: Vec<Hypothesis>,
    pub stats: PlanStats,
}

/// The largest total bond order an element carries, for the twelve scored
/// elements; `None` elsewhere.
fn tabulated_valence(z: i16) -> Option<usize> {
    match z {
        1 | 9 | 17 | 35 | 53 => Some(1), // H, halogens
        8 | 16 => Some(2),               // O, S
        7 | 15 => Some(3),               // N, P
        6 | 14 | 32 => Some(4),          // C, Si, Ge
        _ => None,
    }
}

/// Bond order in half units, so an aromatic bond counts 1.5.
fn half_order(order: u8) -> usize {
    match order {
        BOND_DELETED => 0,
        BOND_SINGLE => 2,
        BOND_DOUBLE => 4,
        BOND_TRIPLE => 6,
        BOND_QUADRUPLE => 8,
        BOND_AROMATIC => 3,
        _ => 2, // dative, metallic: one bond
    }
}

/// How many more single bonds atom `id` can take: its valence minus the
/// total order of the bonds it has.
///
/// The valence is a fixed per-element table rather than one derived from the
/// atom's current hybridization, because a radical carbon with three single
/// bonds types as sp2 and would read as saturated. Outside the table (the
/// elements that cannot be scored anyway) it falls back to the geometric
/// neighbour limit the passivation path uses.
pub fn free_valence(structure: &AtomicStructure, id: u32) -> usize {
    let Some(atom) = structure.get_atom(id) else {
        return 0;
    };
    let z = structure.effective_atomic_number(atom);
    if z <= 0 {
        return 0;
    }
    let bonds: Vec<_> = atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .collect();
    match tabulated_valence(z) {
        Some(valence) => {
            let used: usize = bonds.iter().map(|b| half_order(b.bond_order())).sum();
            (2 * valence).saturating_sub(used) / 2
        }
        None => {
            let hyb = detect_hybridization(structure, id, None);
            covalent_max_neighbors(z, hyb).saturating_sub(bonds.len())
        }
    }
}

/// Reactive atoms of one side, as combined ids, sorted.
fn reactive_atoms(
    input: &AtomicStructure,
    ids: &FxHashMap<u32, u32>,
    tag: &Option<String>,
    side: Side,
) -> Result<Vec<u32>, ChemisorptionError> {
    let mut out: Vec<u32> = match tag.as_deref().map(str::trim) {
        None | Some("") => ids.values().copied().collect(),
        Some(t) => {
            let tagged = input.atoms_with_tag(t);
            if tagged.is_empty() {
                return Err(ChemisorptionError::UnknownTag {
                    side,
                    tag: t.to_string(),
                });
            }
            tagged.iter().map(|id| ids[id]).collect()
        }
    };
    out.sort_unstable();
    Ok(out)
}

/// A substrate reactive atom that is a site under at least one transfer set.
struct Site {
    id: u32,
    pos: DVec3,
    /// Free valence before any transfer.
    base: usize,
}

/// An adsorbate reactive atom that can bond under at least one transfer set.
struct Foot {
    id: u32,
    element: i16,
    pos: DVec3,
    /// Free valence before any transfer.
    base: usize,
    /// Indices into the site list, ascending: within reach, not both frozen.
    sites: Vec<usize>,
}

struct Enumerator<'a> {
    config: &'a ChemisorptionSearch,
    combined: &'a AtomicStructure,
    feet: Vec<Foot>,
    sites: Vec<Site>,
    /// Every transfer the rules allow.
    candidates: Vec<Transfer>,
    max_formed: usize,
    max_transfers: usize,

    // The current transfer set and what it leaves.
    transfer_choice: Vec<usize>,
    transfers: Vec<Transfer>,
    site_valence: Vec<usize>,
    active_feet: Vec<usize>,

    // The current site assignment: (index into `active_feet`, site index).
    chosen: Vec<(usize, usize)>,
    site_used: Vec<usize>,

    seen: HashSet<HypothesisKey>,
    seen_transfer_sets: HashSet<Vec<(u32, i16, u32)>>,
    feet_seen: BTreeSet<u32>,
    sites_seen: BTreeSet<u32>,
    hypotheses: Vec<Hypothesis>,
    stats: PlanStats,
}

impl Enumerator<'_> {
    fn done(&self) -> bool {
        self.stats.truncated
    }

    /// Transfer sets of up to `max_transfers` candidates, `start` onwards,
    /// extending the current one. Each set is searched as it is reached, the
    /// empty set first.
    fn transfer_sets(&mut self, start: usize) {
        if self.done() {
            return;
        }
        self.with_transfer_set();
        if self.transfer_choice.len() >= self.max_transfers {
            return;
        }
        for i in start..self.candidates.len() {
            if self.done() {
                return;
            }
            if !self.compatible(i) {
                continue;
            }
            self.transfer_choice.push(i);
            self.transfer_sets(i + 1);
            self.transfer_choice.pop();
        }
    }

    /// Whether candidate `i` can join the current set: an atom moves once,
    /// and a moving atom is never a donor or acceptor of another transfer
    /// (that would break or form one bond twice — H₂ is the case).
    fn compatible(&self, i: usize) -> bool {
        let t = self.candidates[i];
        self.transfer_choice.iter().all(|&j| {
            let o = self.candidates[j];
            t.moved != o.moved
                && t.moved != o.donor
                && t.moved != o.acceptor
                && o.moved != t.donor
                && o.moved != t.acceptor
        })
    }

    /// Searches the site assignments under the current transfer set.
    fn with_transfer_set(&mut self) {
        self.transfers = self
            .transfer_choice
            .iter()
            .map(|&i| self.candidates[i])
            .collect();

        // Valence change per atom: a donor gains one per atom it gives, an
        // acceptor loses one per atom it takes.
        let mut delta: FxHashMap<u32, isize> = FxHashMap::default();
        for t in &self.transfers {
            *delta.entry(t.donor).or_insert(0) += 1;
            *delta.entry(t.acceptor).or_insert(0) -= 1;
        }
        let valence = |id: u32, base: usize| -> isize {
            base as isize + delta.get(&id).copied().unwrap_or(0)
        };

        if !self.transfers.is_empty() {
            let mut key: Vec<(u32, i16, u32)> = self.transfers.iter().map(Transfer::key).collect();
            key.sort_unstable();
            if !self.seen_transfer_sets.insert(key) {
                self.stats.considered += 1;
                self.stats.duplicates += 1;
                return;
            }
            // R6 on the final edit: every atom the set touches stays within
            // its valence (a transfer may free the valence another needs).
            let broken = delta
                .keys()
                .any(|&id| valence(id, free_valence(self.combined, id)) < 0);
            if broken {
                self.stats.considered += 1;
                self.stats.pruned_valence += 1;
                return;
            }
        }

        for (s, site) in self.sites.iter().enumerate() {
            self.site_valence[s] = valence(site.id, site.base).max(0) as usize;
        }
        let mut active = Vec::new();
        for (f, foot) in self.feet.iter().enumerate() {
            if valence(foot.id, foot.base) > 0
                && foot.sites.iter().any(|&s| self.site_valence[s] > 0)
            {
                active.push(f);
            }
        }
        for &f in &active {
            self.feet_seen.insert(self.feet[f].id);
            for &s in &self.feet[f].sites {
                if self.site_valence[s] > 0 {
                    self.sites_seen.insert(self.sites[s].id);
                }
            }
        }
        self.active_feet = active;
        self.dfs(0);
    }

    fn dfs(&mut self, k: usize) {
        if self.done() {
            return;
        }
        if k == self.active_feet.len() {
            if !self.chosen.is_empty() || !self.transfers.is_empty() {
                self.leaf();
            }
            return;
        }
        let foot = self.active_feet[k];
        if self.chosen.len() < self.max_formed {
            for si in 0..self.feet[foot].sites.len() {
                if self.done() {
                    return;
                }
                let s = self.feet[foot].sites[si];
                if self.site_valence[s] == 0 {
                    // Not a site under this transfer set (a transfer took its
                    // valence, or it only has one when it donates).
                    continue;
                }
                if self.site_used[s] >= self.site_valence[s] {
                    self.stats.considered += 1;
                    self.stats.pruned_valence += 1;
                    continue;
                }
                if !self.pair_ok(foot, s) {
                    self.stats.considered += 1;
                    self.stats.pruned_pair_tolerance += 1;
                    continue;
                }
                self.chosen.push((foot, s));
                self.site_used[s] += 1;
                self.dfs(k + 1);
                self.site_used[s] -= 1;
                self.chosen.pop();
            }
        }
        // This foot forms no bond.
        self.dfs(k + 1);
    }

    /// Whether site `s` for foot `f` keeps every chosen pair within `δ`.
    fn pair_ok(&self, f: usize, s: usize) -> bool {
        let delta = self.config.pair_tolerance;
        if delta <= 0.0 {
            return true;
        }
        let (fp, sp) = (self.feet[f].pos, self.sites[s].pos);
        self.chosen.iter().all(|&(g, t)| {
            let foot_distance = fp.distance(self.feet[g].pos);
            let site_distance = sp.distance(self.sites[t].pos);
            (site_distance - foot_distance).abs() <= delta
        })
    }

    fn element(&self, id: u32) -> i16 {
        self.combined.get_atom(id).map_or(0, |a| a.atomic_number)
    }

    fn leaf(&mut self) {
        self.stats.considered += 1;
        let mut hypothesis = Hypothesis {
            formed: Vec::with_capacity(self.chosen.len()),
            transfers: self.transfers.clone(),
            inventory: BondInventory::default(),
        };
        for &(f, s) in &self.chosen {
            let (foot, site) = (&self.feet[f], &self.sites[s]);
            hypothesis.formed.push((foot.id, site.id));
            *hypothesis
                .inventory
                .formed
                .entry(BondKind::new(foot.element, self.element(site.id), 1))
                .or_insert(0) += 1;
        }
        for t in &self.transfers {
            *hypothesis
                .inventory
                .formed
                .entry(BondKind::new(t.element, self.element(t.acceptor), 1))
                .or_insert(0) += 1;
            *hypothesis
                .inventory
                .broken
                .entry(BondKind::new(t.element, self.element(t.donor), 1))
                .or_insert(0) += 1;
        }
        if !self.seen.insert(hypothesis.key()) {
            self.stats.duplicates += 1;
            return;
        }
        if self.hypotheses.len() >= self.config.budget {
            self.stats.truncated = true;
            return;
        }
        self.hypotheses.push(hypothesis);
    }
}

/// Records `(a, b)` as a bond kind a hypothesis may change: a blocking error
/// for an element that cannot be scored, a flagged estimate for a pair the
/// table lacks.
fn check_scorable(
    a: i16,
    b: i16,
    estimated: &mut BTreeSet<BondKind>,
) -> Result<(), ChemisorptionError> {
    for z in [a, b] {
        if !is_scored_element(z) {
            return Err(ChemisorptionError::UnscoredElement {
                element: crate::atomic_constants::element_symbol(z),
            });
        }
    }
    if tabulated_enthalpy_kj(a, b).is_none() {
        estimated.insert(BondKind::new(a, b, 1));
    }
    Ok(())
}

/// Enumerates the bonding patterns of `adsorbate` over `substrate` at the
/// given pose (§5.2 of the design): transfer sets first, then bond forming —
/// one new bond per adsorbate atom, a site taking as many as its valence
/// allows, pruned by the pair tolerance. Relaxes nothing.
pub fn plan(
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &ChemisorptionSearch,
) -> Result<SearchPlan, ChemisorptionError> {
    let start = Instant::now();
    config.validate()?;

    let mut combined = AtomicStructure::new();
    let adsorbate_ids = combined.add_atomic_structure(adsorbate)?;
    let substrate_ids = combined.add_atomic_structure(substrate)?;

    let ads_reactive = reactive_atoms(
        adsorbate,
        &adsorbate_ids,
        &config.adsorbate_tag,
        Side::Adsorbate,
    )?;
    let sub_reactive = reactive_atoms(
        substrate,
        &substrate_ids,
        &config.substrate_tag,
        Side::Substrate,
    )?;

    let candidates = if config.transfers.is_empty() {
        Vec::new()
    } else {
        candidate_transfers(
            &combined,
            &SideAtoms {
                ids: &adsorbate_ids,
                reactive: &ads_reactive,
            },
            &SideAtoms {
                ids: &substrate_ids,
                reactive: &sub_reactive,
            },
            &config.transfers,
            config.reach,
        )
    };
    // Atoms that gain a valence under some transfer set.
    let donors: BTreeSet<u32> = candidates.iter().map(|t| t.donor).collect();

    let atom = |id: u32| combined.get_atom(id).expect("combined atom");
    let sites: Vec<Site> = sub_reactive
        .iter()
        .filter_map(|&id| {
            let base = free_valence(&combined, id);
            (base > 0 || donors.contains(&id)).then(|| Site {
                id,
                pos: atom(id).position,
                base,
            })
        })
        .collect();

    let mut estimated: BTreeSet<BondKind> = BTreeSet::new();
    let mut feet: Vec<Foot> = Vec::new();
    for &id in &ads_reactive {
        let base = free_valence(&combined, id);
        if base == 0 && !donors.contains(&id) {
            continue;
        }
        let a = atom(id);
        let in_reach: Vec<usize> = sites
            .iter()
            .enumerate()
            .filter(|(_, site)| {
                site.pos.distance(a.position) <= config.reach
                    && !(a.is_frozen() && atom(site.id).is_frozen())
            })
            .map(|(i, _)| i)
            .collect();
        if in_reach.is_empty() {
            continue;
        }
        for &s in &in_reach {
            check_scorable(
                a.atomic_number,
                atom(sites[s].id).atomic_number,
                &mut estimated,
            )?;
        }
        feet.push(Foot {
            id,
            element: a.atomic_number,
            pos: a.position,
            base,
            sites: in_reach,
        });
    }
    for t in &candidates {
        check_scorable(t.element, atom(t.donor).atomic_number, &mut estimated)?;
        check_scorable(t.element, atom(t.acceptor).atomic_number, &mut estimated)?;
    }

    let site_count = sites.len();
    let mut enumerator = Enumerator {
        config,
        combined: &combined,
        feet,
        sites,
        candidates,
        max_formed: config.max_formed_bonds.unwrap_or(usize::MAX),
        max_transfers: if config.transfers.is_empty() {
            0
        } else {
            config.max_transfers
        },
        transfer_choice: Vec::new(),
        transfers: Vec::new(),
        site_valence: vec![0; site_count],
        active_feet: Vec::new(),
        chosen: Vec::new(),
        site_used: vec![0; site_count],
        seen: HashSet::new(),
        seen_transfer_sets: HashSet::new(),
        feet_seen: BTreeSet::new(),
        sites_seen: BTreeSet::new(),
        hypotheses: Vec::new(),
        stats: PlanStats::default(),
    };
    enumerator.transfer_sets(0);

    let mut stats = enumerator.stats;
    let hypotheses = enumerator.hypotheses;
    stats.feet = enumerator.feet_seen.len();
    stats.sites_in_reach = enumerator.sites_seen.len();
    stats.transfer_candidates = enumerator.candidates.len();
    stats.to_relax = hypotheses.len();
    stats.estimated_pairs = estimated.into_iter().collect();
    stats.seconds = start.elapsed().as_secs_f64();

    Ok(SearchPlan {
        combined,
        adsorbate_ids,
        substrate_ids,
        hypotheses,
        stats,
    })
}
