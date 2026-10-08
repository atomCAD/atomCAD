//! `plan`: the geometric phase of a sequential search (§4.2–4.5 of the
//! design). Leg by leg, depth first: leg 1 within `anchor_reach` of a posed
//! foot, leg 2 on the shell around leg 1's site, leg 3 on the ring around legs
//! 1 and 2. Each leg's transferred atom is placed by the rule when the leg is
//! added; each change set is kept once (the first path to it is canonical);
//! three-leg assignments go through the mirror check; candidates are seated
//! and clash-checked. Relaxes nothing, and records the search tree as it goes.

use super::config::SequentialSearch;
use super::setup::{Leg, Mirror, NEAR_MISS_BAND, Seating, Setup, Step};
use super::tree::{NO_ROW, NearMiss, RowKind, SearchTree};
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::config::ChemisorptionError;
use crate::chemisorption::enumerate::{HypothesisKey, change_key};
use crate::chemisorption::inventory::{BondInventory, BondKind};
use crate::chemisorption::transfer::Transfer;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::time::Instant;

/// The deepest level the geometric phase reaches; legs 4 and later are the
/// local phase.
pub const GEOMETRIC_LEGS: usize = 3;

/// One distinct change set the geometric phase reached.
#[derive(Debug, Clone, PartialEq)]
pub struct Hypothesis {
    /// The legs in the canonical binding order (the first path to reach this
    /// change set).
    pub steps: Vec<Step>,
    /// Bonds formed, `(foot, site)` combined ids, in binding order.
    pub formed: Vec<(u32, u32)>,
    /// One per donating leg, in binding order. The moving atom is the
    /// seating's choice when seated, else the donor's lowest-id candidate.
    pub transfers: Vec<Transfer>,
    pub inventory: BondInventory,
    /// Its canonical row in the tree.
    pub row: u32,
    /// The mirror check's verdict; three-leg hypotheses only.
    pub mirror: Option<Mirror>,
    /// The filters admit it: it is relaxed (unless the clash filter or the
    /// budget stops it) and may be listed.
    pub candidate: bool,
    /// Its start placement; computed for candidates only.
    pub seating: Option<Seating>,
}

impl Hypothesis {
    /// The normalized change set it is deduplicated and tie-broken by.
    pub fn key(&self) -> HypothesisKey {
        change_key(&self.formed, &self.transfers)
    }

    pub fn legs(&self) -> usize {
        self.steps.len()
    }

    /// The atoms whose bonds it changes, sorted.
    pub fn changed_atoms(&self) -> Vec<u32> {
        crate::chemisorption::enumerate::changed_atoms(&self.formed, &self.transfers)
    }
}

/// What `plan` found and counted. Every (foot, site) choice that passes its
/// level's geometric test is one **path**, and ends exactly once, so when the
/// search is not truncated
///
/// `paths == pruned_valence + pruned_no_acceptor + pruned_filter + duplicates
///  + anchors + sphere_pairs + torus_triples`
///
/// and `candidates == to_relax + pruned_clash`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanStats {
    /// Adsorbate atoms that can bond: a free valence, or an atom to donate.
    pub feet: usize,
    /// Substrate atoms with a free valence.
    pub sites: usize,
    pub paths: usize,
    /// Distinct hypotheses per level: one, two and three legs.
    pub anchors: usize,
    pub sphere_pairs: usize,
    pub torus_triples: usize,
    /// Paths that reached a change set an earlier path had.
    pub duplicates: usize,
    /// The site had no valence left.
    pub pruned_valence: usize,
    /// The leg's transferred atom found no free site within `reach`.
    pub pruned_no_acceptor: usize,
    /// The leg's bond kinds exceed the `bond_inventory` filter.
    pub pruned_filter: usize,
    /// Three-leg hypotheses the mirror check rejected, and left undecided.
    pub pruned_mirror: usize,
    pub mirror_undecided: usize,
    /// Hypotheses the filters admit (mirror-pruned ones are not).
    pub candidates: usize,
    /// Candidates whose seating clashes, and those pruned for it (only with
    /// `clash_filter` on).
    pub seating_clashes: usize,
    pub pruned_clash: usize,
    /// The relaxations `evaluate` will run.
    pub to_relax: usize,
    /// Near misses recorded over all rows.
    pub near_misses: usize,
    /// The budget was hit; the plan is **not** exhaustive.
    pub truncated: bool,
    /// The adsorbate has four or more feet and the settings allow more than
    /// three legs: a local phase follows the geometric one (its count is known
    /// only after relaxing).
    pub local_phase: bool,
    /// Wall time of `plan` (s).
    pub seconds: f64,
}

/// Everything `plan` decided, nothing relaxed.
#[derive(Debug, Clone)]
pub struct SequentialPlan {
    pub setup: Setup,
    /// The inputs, kept for the separated reference state.
    pub adsorbate: AtomicStructure,
    pub substrate: AtomicStructure,
    /// In enumeration order, which is deterministic.
    pub hypotheses: Vec<Hypothesis>,
    /// Indices into `hypotheses`, in plan order: what `evaluate` relaxes.
    pub to_relax: Vec<usize>,
    pub tree: SearchTree,
    pub stats: PlanStats,
}

struct Planner<'a> {
    setup: &'a Setup,
    config: &'a SequentialSearch,
    depth_cap: usize,
    single_foot: bool,

    /// Valence left per site, after the current path's bonds and transfers.
    left: Vec<usize>,
    foot_used: Vec<bool>,
    steps: Vec<Step>,
    formed: BTreeMap<BondKind, usize>,
    broken: BTreeMap<BondKind, usize>,

    seen: FxHashMap<HypothesisKey, u32>,
    tree: SearchTree,
    hypotheses: Vec<Hypothesis>,
    to_relax: Vec<usize>,
    stats: PlanStats,
}

impl Planner<'_> {
    fn stopped(&self) -> bool {
        self.stats.truncated
    }

    fn run(&mut self) {
        let root = self.tree.push(NO_ROW, RowKind::Root, 0);
        if self.depth_cap == 0 {
            return;
        }
        for f in 0..self.setup.feet.len() {
            if self.stopped() {
                break;
            }
            let row = self.tree.push(root, RowKind::Foot(f as u32), 0);
            let mut misses = Vec::new();
            for s in 0..self.setup.sites.len() {
                if self.stopped() {
                    break;
                }
                let leg = Leg { foot: f, site: s };
                if !self.setup.may_bond(leg) {
                    continue;
                }
                let over = self.setup.anchor_distance(leg) - self.config.anchor_reach;
                if over > 0.0 {
                    if over <= NEAR_MISS_BAND && self.left[s] > 0 {
                        misses.push(near_miss(leg, over));
                    }
                    continue;
                }
                self.try_leg(row, leg);
            }
            self.stats.near_misses += misses.len();
            self.tree.set_near_misses(row, misses);
        }
    }

    /// The sphere (one leg bonded) or the ring (two): every unused foot
    /// against every site, by the shell test against each bonded leg.
    fn extend(&mut self, row: u32) {
        let bonded: Vec<Leg> = self.steps.iter().map(|s| s.leg).collect();
        let mut misses = Vec::new();
        'feet: for f in 0..self.setup.feet.len() {
            if self.foot_used[f] {
                continue;
            }
            for s in 0..self.setup.sites.len() {
                if self.stopped() {
                    break 'feet;
                }
                let leg = Leg { foot: f, site: s };
                if !self.setup.may_bond(leg) {
                    continue;
                }
                let over = self.setup.need_against(&bonded, leg) - self.config.tolerance;
                if over > 0.0 {
                    if over <= NEAR_MISS_BAND && self.left[s] > 0 {
                        misses.push(near_miss(leg, over));
                    }
                    continue;
                }
                self.try_leg(row, leg);
            }
        }
        self.stats.near_misses += misses.len();
        self.tree.set_near_misses(row, misses);
    }

    /// One path: `leg` passed its level's test under `parent`.
    fn try_leg(&mut self, parent: u32, leg: Leg) {
        self.stats.paths += 1;
        if self.left[leg.site] == 0 {
            self.stats.pruned_valence += 1;
            self.tree.row_mut(parent).rejected_valence += 1;
            return;
        }
        self.left[leg.site] -= 1;
        let foot = &self.setup.feet[leg.foot];
        let acceptor = match foot.donates {
            None => None,
            Some(_) => match self.setup.acceptor(leg.site, &self.left) {
                Some(a) => {
                    self.left[a] -= 1;
                    Some(a)
                }
                None => {
                    self.left[leg.site] += 1;
                    self.stats.pruned_no_acceptor += 1;
                    self.tree.row_mut(parent).rejected_no_acceptor += 1;
                    return;
                }
            },
        };
        let step = Step { leg, acceptor };
        let (formed, broken) = step_kinds(self.setup, step);
        if !self.inventory_allows(&formed, &broken) {
            if let Some(a) = acceptor {
                self.left[a] += 1;
            }
            self.left[leg.site] += 1;
            self.stats.pruned_filter += 1;
            self.tree.row_mut(parent).rejected_filter += 1;
            return;
        }
        for k in &formed {
            *self.formed.entry(*k).or_insert(0) += 1;
        }
        for k in &broken {
            *self.broken.entry(*k).or_insert(0) += 1;
        }
        self.steps.push(step);
        self.foot_used[leg.foot] = true;

        let depth = self.steps.len();
        let (formed_ids, transfers) = self.setup.changes(&self.steps, None);
        let key = change_key(&formed_ids, &transfers);
        let row = self.tree.push(
            parent,
            RowKind::Leg {
                foot: leg.foot as u32,
                site: leg.site as u32,
                acceptor: acceptor.map(|a| a as u32),
            },
            depth as u8,
        );
        if let Some(&canonical) = self.seen.get(&key) {
            self.tree.row_mut(row).duplicate_of = Some(canonical);
            self.tree.row_mut(parent).duplicates += 1;
            self.stats.duplicates += 1;
        } else {
            self.seen.insert(key, row);
            let mirrored = self.found(row, formed_ids, transfers);
            if !mirrored && !self.stopped() && depth < self.depth_cap {
                self.extend(row);
            }
        }

        // Undo.
        self.foot_used[leg.foot] = false;
        self.steps.pop();
        for k in &formed {
            *self.formed.get_mut(k).expect("counted") -= 1;
        }
        for k in &broken {
            *self.broken.get_mut(k).expect("counted") -= 1;
        }
        if let Some(a) = acceptor {
            self.left[a] += 1;
        }
        self.left[leg.site] += 1;
    }

    /// Whether the `bond_inventory` filter still allows the current path plus
    /// one step's bond kinds.
    fn inventory_allows(&self, formed: &[BondKind], broken: &[BondKind]) -> bool {
        let Some(target) = &self.config.bond_inventory else {
            return true;
        };
        let fits = |have: &BTreeMap<BondKind, usize>,
                    want: &BTreeMap<BondKind, usize>,
                    adding: &[BondKind]| {
            adding.iter().all(|k| {
                let extra = adding.iter().filter(|o| *o == k).count();
                have.get(k).copied().unwrap_or(0) + extra <= want.get(k).copied().unwrap_or(0)
            })
        };
        fits(&self.formed, &target.formed, formed) && fits(&self.broken, &target.broken, broken)
    }

    /// Records a new distinct hypothesis on `row`. Returns whether the mirror
    /// check rejected it.
    fn found(&mut self, row: u32, formed: Vec<(u32, u32)>, transfers: Vec<Transfer>) -> bool {
        let depth = self.steps.len();
        match depth {
            1 => self.stats.anchors += 1,
            2 => self.stats.sphere_pairs += 1,
            _ => self.stats.torus_triples += 1,
        }
        let inventory = BondInventory {
            formed: self
                .formed
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(k, n)| (*k, *n))
                .collect(),
            broken: self
                .broken
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(k, n)| (*k, *n))
                .collect(),
        };
        let mut mirror = None;
        if depth == GEOMETRIC_LEGS {
            let legs = [self.steps[0].leg, self.steps[1].leg, self.steps[2].leg];
            let up = self.setup.local_up(&legs.map(|l| l.site));
            let verdict = self.setup.mirror(legs, up);
            match verdict {
                Mirror::Mirrored => self.stats.pruned_mirror += 1,
                Mirror::Undecided => self.stats.mirror_undecided += 1,
                Mirror::Proper => {}
            }
            mirror = Some(verdict);
        }
        let mirrored = mirror == Some(Mirror::Mirrored);
        let candidate = !mirrored && self.admits(depth, &inventory);
        let index = self.hypotheses.len();
        let mut hypothesis = Hypothesis {
            steps: self.steps.clone(),
            formed,
            transfers,
            inventory,
            row,
            mirror,
            candidate,
            seating: None,
        };
        if candidate {
            self.stats.candidates += 1;
            let seating = self.setup.seat(&self.steps);
            for (t, &moved) in hypothesis.transfers.iter_mut().zip(&seating.moved) {
                t.moved = moved;
            }
            if seating.clashes() {
                self.stats.seating_clashes += 1;
            }
            if seating.clashes() && self.config.clash_filter {
                self.stats.pruned_clash += 1;
            } else if self.to_relax.len() >= self.config.budget {
                self.stats.truncated = true;
            } else {
                self.to_relax.push(index);
            }
            hypothesis.seating = Some(seating);
        }
        self.tree.row_mut(row).hypothesis = Some(index as u32);
        self.hypotheses.push(hypothesis);
        mirrored
    }

    /// Whether the filters admit a hypothesis of `legs` legs (§4.8): two or
    /// more legs (one only for a one-foot adsorbate, D8), the depth cap, and
    /// the exact leg count and inventory when set.
    fn admits(&self, legs: usize, inventory: &BondInventory) -> bool {
        (legs >= 2 || self.single_foot)
            && self.config.max_formed_bonds.is_none_or(|m| legs <= m)
            && self.config.formed_bonds.is_none_or(|n| legs == n)
            && self
                .config
                .bond_inventory
                .as_ref()
                .is_none_or(|t| t == inventory)
    }
}

fn near_miss(leg: Leg, over: f64) -> NearMiss {
    NearMiss {
        foot: leg.foot as u32,
        site: leg.site as u32,
        miss: over as f32,
    }
}

/// The bond kinds one step forms and breaks: its foot–site bond, and for a
/// donating foot the moved atom's bond to its acceptor (formed) and to its
/// donor (broken).
fn step_kinds(setup: &Setup, step: Step) -> (Vec<BondKind>, Vec<BondKind>) {
    let foot = &setup.feet[step.leg.foot];
    let mut formed = vec![BondKind::new(
        foot.element,
        setup.sites[step.leg.site].element,
        1,
    )];
    let mut broken = Vec::new();
    if let (Some(a), Some(x)) = (step.acceptor, foot.donates) {
        formed.push(BondKind::new(x, setup.sites[a].element, 1));
        broken.push(BondKind::new(x, foot.element, 1));
    }
    (formed, broken)
}

/// How many legs the settings allow at most, ignoring the geometric phase's
/// own limit: `None` = no cap.
fn leg_cap(config: &SequentialSearch) -> Option<usize> {
    let inventory = config
        .bond_inventory
        .as_ref()
        .map(|t| t.formed_count().saturating_sub(t.broken_count()));
    [config.max_formed_bonds, config.formed_bonds, inventory]
        .into_iter()
        .flatten()
        .min()
}

/// Enumerates the geometric phase of a sequential search of `adsorbate` over
/// `substrate` at the given pose. Relaxes nothing.
pub fn plan(
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &SequentialSearch,
) -> Result<SequentialPlan, ChemisorptionError> {
    let start = Instant::now();
    config.validate()?;
    let setup = Setup::new(adsorbate, substrate, config)?;
    let feet = setup.feet.len();
    let cap = leg_cap(config);
    let depth_cap = [Some(GEOMETRIC_LEGS), Some(feet), cap]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(0);

    let mut planner = Planner {
        setup: &setup,
        config,
        depth_cap,
        single_foot: feet == 1,
        left: setup.sites.iter().map(|s| s.valence).collect(),
        foot_used: vec![false; feet],
        steps: Vec::new(),
        formed: BTreeMap::new(),
        broken: BTreeMap::new(),
        seen: FxHashMap::default(),
        tree: SearchTree::default(),
        hypotheses: Vec::new(),
        to_relax: Vec::new(),
        stats: PlanStats::default(),
    };
    planner.run();
    let Planner {
        mut tree,
        hypotheses,
        to_relax,
        mut stats,
        ..
    } = planner;
    tree.finish();
    stats.feet = feet;
    stats.sites = setup.sites.len();
    stats.to_relax = to_relax.len();
    stats.local_phase = feet > GEOMETRIC_LEGS && cap.is_none_or(|c| c > GEOMETRIC_LEGS);
    stats.seconds = start.elapsed().as_secs_f64();
    Ok(SequentialPlan {
        setup,
        adsorbate: adsorbate.clone(),
        substrate: substrate.clone(),
        hypotheses,
        to_relax,
        tree,
        stats,
    })
}
