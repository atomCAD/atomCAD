//! `plan`: the enumeration half of a search. It combines the two inputs, finds
//! the sites and the adsorbate atoms that can bond, and enumerates every
//! bonding pattern the rules allow — without relaxing anything, so it is cheap
//! enough to run on every evaluation.

use super::config::{ChemisorptionError, ChemisorptionSearch, Side};
use super::score::{BondInventory, BondKind, is_scored_element, tabulated_enthalpy_kj};
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
    /// Bonds formed, `(adsorbate atom, substrate atom)`, in adsorbate-atom
    /// order.
    pub formed: Vec<(u32, u32)>,
    /// Bonds broken. Empty until transfers exist (phase 3 of the design).
    pub broken: Vec<(u32, u32)>,
    /// Atoms moved. Empty until transfers exist.
    pub moved: Vec<u32>,
    pub inventory: BondInventory,
}

/// The normalized bond set a hypothesis is deduplicated and tie-broken by:
/// every pair `(min, max)`, sorted.
pub type HypothesisKey = (Vec<(u32, u32)>, Vec<(u32, u32)>, Vec<u32>);

impl Hypothesis {
    pub fn key(&self) -> HypothesisKey {
        let norm = |pairs: &[(u32, u32)]| -> Vec<(u32, u32)> {
            let set: BTreeSet<(u32, u32)> =
                pairs.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
            set.into_iter().collect()
        };
        let mut moved = self.moved.clone();
        moved.sort_unstable();
        (norm(&self.formed), norm(&self.broken), moved)
    }

    /// The atoms whose bonds this hypothesis changes, sorted.
    pub fn changed_atoms(&self) -> Vec<u32> {
        let set: BTreeSet<u32> = self
            .formed
            .iter()
            .chain(self.broken.iter())
            .flat_map(|&(a, b)| [a, b])
            .chain(self.moved.iter().copied())
            .collect();
        set.into_iter().collect()
    }
}

/// What `plan` found and counted. Every branch of the site-assignment search
/// ends exactly once, so
/// `considered == pruned_valence + pruned_pair_tolerance + duplicates + to_relax`
/// — plus the one complete assignment that tripped the budget, when
/// `truncated`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanStats {
    /// Adsorbate reactive atoms with a free valence and at least one site
    /// within reach.
    pub feet: usize,
    /// Distinct sites within reach of at least one of them.
    pub sites_in_reach: usize,
    /// Assignments considered: pruned partial ones and complete ones.
    pub considered: usize,
    /// Rejected because a site had no valence left (R6).
    pub pruned_valence: usize,
    /// Rejected by the pair tolerance (R3).
    pub pruned_pair_tolerance: usize,
    /// Complete assignments merged with an earlier one of the same bond set.
    pub duplicates: usize,
    /// Valid, deduplicated hypotheses: the relaxations `evaluate` will run.
    pub to_relax: usize,
    /// The budget was hit; the enumeration is **not** exhaustive.
    pub truncated: bool,
    /// Bond kinds a hypothesis may form whose enthalpy is a Pauling estimate.
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

struct Site {
    id: u32,
    pos: DVec3,
    valence: usize,
}

struct Foot {
    id: u32,
    element: i16,
    pos: DVec3,
    /// Indices into the site list, ascending.
    sites: Vec<usize>,
}

struct Enumerator<'a> {
    config: &'a ChemisorptionSearch,
    combined: &'a AtomicStructure,
    feet: Vec<Foot>,
    sites: Vec<Site>,
    max_formed: usize,
    chosen: Vec<(usize, usize)>,
    site_used: Vec<usize>,
    seen: HashSet<HypothesisKey>,
    hypotheses: Vec<Hypothesis>,
    stats: PlanStats,
}

impl Enumerator<'_> {
    fn done(&self) -> bool {
        self.stats.truncated
    }

    fn dfs(&mut self, k: usize) {
        if self.done() {
            return;
        }
        if k == self.feet.len() {
            if !self.chosen.is_empty() {
                self.leaf();
            }
            return;
        }
        if self.chosen.len() < self.max_formed {
            for si in 0..self.feet[k].sites.len() {
                if self.done() {
                    return;
                }
                let s = self.feet[k].sites[si];
                if self.site_used[s] >= self.sites[s].valence {
                    self.stats.considered += 1;
                    self.stats.pruned_valence += 1;
                    continue;
                }
                if !self.pair_ok(k, s) {
                    self.stats.considered += 1;
                    self.stats.pruned_pair_tolerance += 1;
                    continue;
                }
                self.chosen.push((k, s));
                self.site_used[s] += 1;
                self.dfs(k + 1);
                self.site_used[s] -= 1;
                self.chosen.pop();
            }
        }
        // Foot `k` forms no bond.
        self.dfs(k + 1);
    }

    /// Whether site `s` for foot `k` keeps every chosen pair within `δ`.
    fn pair_ok(&self, k: usize, s: usize) -> bool {
        let delta = self.config.pair_tolerance;
        if delta <= 0.0 {
            return true;
        }
        let (fk, sk) = (self.feet[k].pos, self.sites[s].pos);
        self.chosen.iter().all(|&(j, t)| {
            let foot_distance = fk.distance(self.feet[j].pos);
            let site_distance = sk.distance(self.sites[t].pos);
            (site_distance - foot_distance).abs() <= delta
        })
    }

    fn leaf(&mut self) {
        self.stats.considered += 1;
        let mut hypothesis = Hypothesis {
            formed: Vec::with_capacity(self.chosen.len()),
            broken: Vec::new(),
            moved: Vec::new(),
            inventory: BondInventory::default(),
        };
        for &(k, s) in &self.chosen {
            let (foot, site) = (&self.feet[k], &self.sites[s]);
            hypothesis.formed.push((foot.id, site.id));
            let site_z = self
                .combined
                .get_atom(site.id)
                .map_or(0, |a| a.atomic_number);
            *hypothesis
                .inventory
                .formed
                .entry(BondKind::new(foot.element, site_z, 1))
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

/// Enumerates the bonding patterns of `adsorbate` over `substrate` at the
/// given pose (§5.2 of the design): bond forming only, one new bond per
/// adsorbate atom, a site taking as many as its free valence allows, pruned by
/// the pair tolerance. Relaxes nothing.
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

    let atom = |id: u32| combined.get_atom(id).expect("combined atom");
    let sites: Vec<Site> = sub_reactive
        .iter()
        .filter_map(|&id| {
            let valence = free_valence(&combined, id);
            (valence > 0).then(|| Site {
                id,
                pos: atom(id).position,
                valence,
            })
        })
        .collect();

    let mut estimated: BTreeSet<BondKind> = BTreeSet::new();
    let mut in_reach: BTreeSet<usize> = BTreeSet::new();
    let mut feet: Vec<Foot> = Vec::new();
    for &id in &ads_reactive {
        if free_valence(&combined, id) == 0 {
            continue;
        }
        let a = atom(id);
        let candidates: Vec<usize> = sites
            .iter()
            .enumerate()
            .filter(|(_, site)| {
                site.pos.distance(a.position) <= config.reach
                    && !(a.is_frozen() && atom(site.id).is_frozen())
            })
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            continue;
        }
        for &s in &candidates {
            let site_z = atom(sites[s].id).atomic_number;
            for z in [a.atomic_number, site_z] {
                if !is_scored_element(z) {
                    return Err(ChemisorptionError::UnscoredElement {
                        element: crate::atomic_constants::element_symbol(z),
                    });
                }
            }
            if tabulated_enthalpy_kj(a.atomic_number, site_z).is_none() {
                estimated.insert(BondKind::new(a.atomic_number, site_z, 1));
            }
            in_reach.insert(s);
        }
        feet.push(Foot {
            id,
            element: a.atomic_number,
            pos: a.position,
            sites: candidates,
        });
    }

    let max_formed = config.max_formed_bonds.unwrap_or(usize::MAX);
    let site_count = sites.len();
    let mut enumerator = Enumerator {
        config,
        combined: &combined,
        feet,
        sites,
        max_formed,
        chosen: Vec::new(),
        site_used: vec![0; site_count],
        seen: HashSet::new(),
        hypotheses: Vec::new(),
        stats: PlanStats::default(),
    };
    enumerator.dfs(0);

    let mut stats = enumerator.stats;
    let hypotheses = enumerator.hypotheses;
    stats.feet = enumerator.feet.len();
    stats.sites_in_reach = in_reach.len();
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
