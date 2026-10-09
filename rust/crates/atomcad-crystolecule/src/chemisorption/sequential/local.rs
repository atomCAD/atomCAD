//! The local phase (§4.6 of the design): legs 4 and later, for adsorbates
//! with four or more feet. Breadth first, one level per leg. Each level grows
//! from the relaxed states of the level above: for every unbonded foot, every
//! site within `reach` of the foot's **relaxed** position is one more leg, its
//! start geometry the relaxed parent plus the new bond (and the new leg's
//! transferred atom, placed by the rule of §5). Every path is relaxed; within
//! a level, the paths that reach one change set are deduplicated after
//! relaxing, keeping the lowest strain, whose parent becomes the canonical
//! one.
//!
//! A level holds only the positions of the movable atoms of its relaxed
//! states ([`Setup::movable`]), and only while the next level is being built:
//! nothing of it outlives `evaluate`. The debug view rebuilds a state by
//! [`replay`] instead.

use super::config::SequentialSearch;
use super::evaluate::{Candidate, RelaxedRow, SearchReport, check_cancelled};
use super::plan::{
    GEOMETRIC_LEGS, Hypothesis, SequentialPlan, admits, inventory_allows, step_kinds,
};
use super::setup::{Leg, NEAR_MISS_BAND, Setup, Step};
use super::tree::{NearMiss, RowKind, SearchTree};
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::atoms::{HypothesisKey, change_key};
use crate::chemisorption::config::{CHANGED_TAG, ChemisorptionError};
use crate::chemisorption::inventory::BondInventory;
use crate::chemisorption::relax::{RelaxSettings, Relaxed, StrainTerms, relax};
use atomcad_util::job_control::JobControl;
use glam::DVec3;
use rayon::prelude::*;
use std::collections::BTreeMap;

/// What one local level did. Every (foot, site) choice within `reach` is one
/// **path** and ends exactly once, so
///
/// `paths == pruned_valence + pruned_no_acceptor + pruned_filter + duplicates
///  + hypotheses`
///
/// and `relaxed == to_relax` unless the level is truncated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelStats {
    /// The leg count of this level's hypotheses (4, 5, …).
    pub legs: usize,
    /// Relaxed states of the level above it grew from.
    pub parents: usize,
    pub paths: usize,
    /// Distinct change sets.
    pub hypotheses: usize,
    /// Paths that reached a change set another path of the level had.
    pub duplicates: usize,
    /// The site had no valence left.
    pub pruned_valence: usize,
    /// The leg's transferred atom found no free site within `reach`.
    pub pruned_no_acceptor: usize,
    /// The leg's bond kinds exceed the `bond_inventory` filter.
    pub pruned_filter: usize,
    /// Hypotheses the filters admit.
    pub candidates: usize,
    /// Paths that need a relaxation: a candidate's, or a parent's of the next
    /// level. Duplicate paths included: their strains decide which is kept.
    pub to_relax: usize,
    /// Paths relaxed; fewer than `to_relax` when the budget ran out.
    pub relaxed: usize,
    pub unconverged: usize,
    /// The budget cut this level short: the search is not exhaustive.
    pub truncated: bool,
    /// Near misses recorded on this level's parents.
    pub near_misses: usize,
}

/// A relaxed state the next level grows from: all the frontier keeps.
#[derive(Debug, Clone)]
pub(crate) struct State {
    /// Index in the report's numbering ([`SearchReport::hypothesis`]).
    pub hypothesis: usize,
    pub row: u32,
    pub strain: f64,
    /// The movable atoms' relaxed positions ([`Setup::movable_positions`]).
    pub positions: Vec<DVec3>,
}

/// One path of a level, before it is relaxed.
struct Path {
    /// Index into the level's frontier.
    parent: usize,
    step: Step,
    row: u32,
    key: HypothesisKey,
    inventory: BondInventory,
    candidate: bool,
}

/// One relaxed path.
struct Done {
    moved: Option<u32>,
    relaxed: Relaxed,
    positions: Vec<DVec3>,
}

/// What a level hands back to `evaluate`.
pub(crate) struct Level {
    pub stats: LevelStats,
    /// The kept states the next level grows from, in row order; empty at the
    /// deepest level.
    pub next: Vec<State>,
    /// The kept candidates, relaxed, not yet ranked.
    pub candidates: Vec<Candidate>,
    /// Every relaxation, in the order the budget took them.
    pub relaxed: Vec<RelaxedRow>,
}

/// Valence left per site after `steps`' bonds and transfers.
fn left_after(setup: &Setup, steps: &[Step]) -> Vec<usize> {
    let mut left: Vec<usize> = setup.sites.iter().map(|s| s.valence).collect();
    for s in steps {
        left[s.leg.site] -= 1;
        if let Some(a) = s.acceptor {
            left[a] -= 1;
        }
    }
    left
}

/// The atom each donating step of `h` moves, in step order.
fn moved_of(h: &Hypothesis) -> Vec<u32> {
    h.transfers.iter().map(|t| t.moved).collect()
}

/// A hypothesis by its index in the report's numbering: the plan's first,
/// then the local phase's.
pub(crate) fn hypothesis<'a>(
    plan: &'a SequentialPlan,
    local: &'a [Hypothesis],
    index: usize,
) -> &'a Hypothesis {
    let n = plan.hypotheses.len();
    if index < n {
        &plan.hypotheses[index]
    } else {
        &local[index - n]
    }
}

/// The search context every level shares.
pub(crate) struct Local<'a> {
    pub plan: &'a SequentialPlan,
    pub config: &'a SequentialSearch,
    pub settings: &'a RelaxSettings,
    pub reference_energy: f64,
    pub reference_terms: &'a StrainTerms,
    pub control: Option<&'a JobControl>,
}

impl Local<'_> {
    /// Grows level `legs` from `frontier` (the relaxed states of the level
    /// above, in row order), spending at most `budget` relaxations. Appends
    /// its rows to `tree` and its hypotheses to `local`.
    pub fn level(
        &self,
        legs: usize,
        frontier: &[State],
        budget: usize,
        tree: &mut SearchTree,
        local: &mut Vec<Hypothesis>,
    ) -> Result<Level, ChemisorptionError> {
        let setup = &self.plan.setup;
        let deeper = legs < self.plan.max_legs;
        let mut stats = LevelStats {
            legs,
            parents: frontier.len(),
            ..Default::default()
        };
        let paths = self.enumerate(legs, frontier, tree, local, &mut stats);

        // Relax what is needed, the children of the best parents first, as
        // far as the budget goes (the order is fixed before truncating).
        let mut order: Vec<usize> = (0..paths.len())
            .filter(|&i| paths[i].candidate || deeper)
            .collect();
        order.sort_by(|&a, &b| {
            let (pa, pb) = (&paths[a], &paths[b]);
            frontier[pa.parent]
                .strain
                .total_cmp(&frontier[pb.parent].strain)
                .then_with(|| pa.key.cmp(&pb.key))
                .then(pa.row.cmp(&pb.row))
        });
        stats.to_relax = order.len();
        if order.len() > budget {
            order.truncate(budget);
            stats.truncated = true;
        }
        if let Some(c) = self.control {
            c.set_phase(&format!("Local phase: leg {legs}"));
            let total = c.snapshot().total.unwrap_or(0);
            c.set_total(total + order.len() as u64);
        }
        let relaxed: Vec<Done> = order
            .par_iter()
            .map(|&i| -> Result<Done, ChemisorptionError> {
                check_cancelled(self.control)?;
                let path = &paths[i];
                let state = &frontier[path.parent];
                let h = hypothesis(self.plan, local, state.hypothesis);
                let (mut s, moved) =
                    setup.grown_structure(&h.steps, &moved_of(h), &state.positions, path.step);
                let relaxed = relax(&mut s, self.settings)?;
                if let Some(c) = self.control {
                    c.advance(1);
                }
                Ok(Done {
                    moved,
                    relaxed,
                    positions: setup.movable_positions(&s),
                })
            })
            .collect::<Result<_, _>>()?;
        stats.relaxed = relaxed.len();
        stats.unconverged = relaxed.iter().filter(|d| !d.relaxed.converged).count();
        let strain = |d: &Done| d.relaxed.energy - self.reference_energy;

        // Deduplicate by change set: the lowest strain wins, ties to the
        // earlier row; when none of a set's paths was relaxed, its first path.
        let mut done: Vec<Option<Done>> = (0..paths.len()).map(|_| None).collect();
        for (&i, d) in order.iter().zip(relaxed) {
            done[i] = Some(d);
        }
        let mut groups: BTreeMap<&HypothesisKey, Vec<usize>> = BTreeMap::new();
        for (i, p) in paths.iter().enumerate() {
            groups.entry(&p.key).or_default().push(i);
        }
        let mut winner = vec![0; paths.len()];
        for members in groups.values() {
            let best = members
                .iter()
                .filter_map(|&i| done[i].as_ref().map(|d| (i, strain(d))))
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                .map_or(members[0], |(i, _)| i);
            for &i in members {
                winner[i] = best;
            }
        }
        // Each kept path's hypothesis index, in row order.
        let first = self.plan.hypotheses.len() + local.len();
        let mut index = vec![usize::MAX; paths.len()];
        let mut n = 0;
        for i in 0..paths.len() {
            if winner[i] == i {
                index[i] = first + n;
                n += 1;
            }
        }
        let relaxed_rows: Vec<RelaxedRow> = order
            .iter()
            .map(|&i| {
                let d = done[i].as_ref().expect("relaxed");
                RelaxedRow {
                    hypothesis: index[winner[i]],
                    row: paths[i].row,
                    strain: strain(d),
                    converged: d.relaxed.converged,
                }
            })
            .collect();

        let mut next = Vec::new();
        let mut candidates = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            if winner[i] != i {
                tree.row_mut(path.row).duplicate_of = Some(paths[winner[i]].row);
                tree.row_mut(frontier[path.parent].row).duplicates += 1;
                stats.duplicates += 1;
                continue;
            }
            stats.hypotheses += 1;
            if path.candidate {
                stats.candidates += 1;
            }
            tree.row_mut(path.row).hypothesis = Some(index[i] as u32);
            let parent = hypothesis(self.plan, local, frontier[path.parent].hypothesis);
            let mut steps = parent.steps.clone();
            steps.push(path.step);
            let d = done[i].take();
            // The moving atom: the relaxation's choice, or, unrelaxed, the
            // donor's lowest-id candidate (as in the geometric phase).
            let mut moved = moved_of(parent);
            if path.step.acceptor.is_some() {
                let foot = &setup.feet[path.step.leg.foot];
                moved.push(
                    d.as_ref()
                        .and_then(|d| d.moved)
                        .unwrap_or(foot.donatable[0]),
                );
            }
            let (formed, transfers) = setup.changes(&steps, Some(&moved));
            let h = Hypothesis {
                steps,
                formed,
                transfers,
                inventory: path.inventory.clone(),
                row: path.row,
                mirror: None,
                candidate: path.candidate,
                parent: deeper,
                seating: None,
            };
            if let Some(d) = d {
                let strain = strain(&d);
                if path.candidate {
                    let mut s = setup.state_structure(&h.steps, &moved, &d.positions);
                    for id in h.changed_atoms() {
                        s.add_atom_tag(id, CHANGED_TAG)?;
                    }
                    candidates.push(Candidate {
                        structure: s,
                        hypothesis: index[i],
                        row: path.row,
                        formed: h.formed.clone(),
                        transfers: h.transfers.clone(),
                        bond_inventory: h.inventory.clone(),
                        strain,
                        terms: d.relaxed.terms.minus(self.reference_terms),
                        converged: d.relaxed.converged,
                        worst_bond_ratio: d.relaxed.worst_bond_ratio,
                        energy: d.relaxed.energy,
                        seating_clash: false,
                    });
                }
                if deeper {
                    next.push(State {
                        hypothesis: index[i],
                        row: path.row,
                        strain,
                        positions: d.positions,
                    });
                }
            }
            local.push(h);
        }
        Ok(Level {
            stats,
            next,
            candidates,
            relaxed: relaxed_rows,
        })
    }

    /// Every path of level `legs`: per parent (in row order), per unbonded
    /// foot, per site within `reach` of the foot's relaxed position, the
    /// valence, transfer-rule and inventory checks. Pushes a row per path and
    /// records each parent's near misses and rejections.
    fn enumerate(
        &self,
        legs: usize,
        frontier: &[State],
        tree: &mut SearchTree,
        local: &[Hypothesis],
        stats: &mut LevelStats,
    ) -> Vec<Path> {
        let setup = &self.plan.setup;
        let mut paths = Vec::new();
        for (pi, state) in frontier.iter().enumerate() {
            let h = hypothesis(self.plan, local, state.hypothesis);
            let mut left = left_after(setup, &h.steps);
            let mut misses = Vec::new();
            for f in 0..setup.feet.len() {
                if h.steps.iter().any(|s| s.leg.foot == f) {
                    continue;
                }
                let foot_at = setup.position_in(&state.positions, setup.feet[f].id);
                for s in 0..setup.sites.len() {
                    let leg = Leg { foot: f, site: s };
                    if !setup.may_bond(leg) {
                        continue;
                    }
                    let site_at = setup.position_in(&state.positions, setup.sites[s].id);
                    let over = foot_at.distance(site_at) - self.config.reach;
                    if over > 0.0 {
                        if over <= NEAR_MISS_BAND && left[s] > 0 {
                            misses.push(NearMiss {
                                foot: f as u32,
                                site: s as u32,
                                miss: over as f32,
                            });
                        }
                        continue;
                    }
                    stats.paths += 1;
                    if left[s] == 0 {
                        stats.pruned_valence += 1;
                        tree.row_mut(state.row).rejected_valence += 1;
                        continue;
                    }
                    let acceptor = match setup.feet[f].donates {
                        None => None,
                        Some(_) => {
                            left[s] -= 1;
                            let a = setup.acceptor(s, &left);
                            left[s] += 1;
                            if a.is_none() {
                                stats.pruned_no_acceptor += 1;
                                tree.row_mut(state.row).rejected_no_acceptor += 1;
                                continue;
                            }
                            a
                        }
                    };
                    let step = Step { leg, acceptor };
                    let (formed, broken) = step_kinds(setup, step);
                    if !inventory_allows(
                        self.config,
                        &h.inventory.formed,
                        &h.inventory.broken,
                        &formed,
                        &broken,
                    ) {
                        stats.pruned_filter += 1;
                        tree.row_mut(state.row).rejected_filter += 1;
                        continue;
                    }
                    let mut inventory = h.inventory.clone();
                    for k in formed {
                        *inventory.formed.entry(k).or_insert(0) += 1;
                    }
                    for k in broken {
                        *inventory.broken.entry(k).or_insert(0) += 1;
                    }
                    let mut steps = h.steps.clone();
                    steps.push(step);
                    let (formed_ids, transfers) = setup.changes(&steps, None);
                    let row = tree.push(
                        state.row,
                        RowKind::Leg {
                            foot: f as u32,
                            site: s as u32,
                            acceptor: acceptor.map(|a| a as u32),
                        },
                        legs as u8,
                    );
                    paths.push(Path {
                        parent: pi,
                        step,
                        row,
                        key: change_key(&formed_ids, &transfers),
                        candidate: admits(self.config, false, legs, &inventory),
                        inventory,
                    });
                }
            }
            stats.near_misses += misses.len();
            tree.set_near_misses(state.row, misses);
        }
        paths
    }
}

/// A row's relaxed state, rebuilt (§6.5): what the debug view shows for a
/// relaxed row that is not among the kept candidates.
#[derive(Debug, Clone)]
pub struct Replayed {
    pub structure: AtomicStructure,
    pub relaxed: Relaxed,
    /// `relaxed.energy − reference_energy`: the strain the search recorded.
    pub strain: f64,
    /// The legs along the row's path, in binding order.
    pub steps: Vec<Step>,
}

/// Rebuilds the relaxed state of tree row `row`: a geometric row by seating
/// its change set and relaxing it, a local-phase row by replaying its
/// ancestors first — its own path's parent, which is always a kept
/// (canonical) state, since only those grow children. Relaxation is
/// deterministic, so this is the state the search produced, and its strain
/// is the one recorded for the row. A geometric duplicate row replays its
/// canonical row (the same change set seats the same).
pub fn replay(
    plan: &SequentialPlan,
    report: &SearchReport,
    config: &SequentialSearch,
    row: u32,
) -> Result<Replayed, ChemisorptionError> {
    let setup = &plan.setup;
    let settings = config.relax_settings();
    let r = report.tree.row(row);
    let RowKind::Leg {
        foot,
        site,
        acceptor,
    } = r.kind
    else {
        return Err(ChemisorptionError::InvalidConfig(
            "only a row with a bonded leg has a relaxed state".into(),
        ));
    };
    let (mut s, steps) = if r.legs as usize <= GEOMETRIC_LEGS {
        let canonical = r.duplicate_of.unwrap_or(row);
        let index = report
            .tree
            .row(canonical)
            .hypothesis
            .expect("a canonical row has a hypothesis") as usize;
        let h = report.hypothesis(plan, index);
        let seating = match &h.seating {
            Some(s) => s.clone(),
            None => setup.seat(&h.steps),
        };
        (setup.start_structure(&h.steps, &seating), h.steps.clone())
    } else {
        let parent = replay(plan, report, config, r.parent)?;
        let index = report
            .tree
            .row(r.parent)
            .hypothesis
            .expect("a parent row is canonical") as usize;
        let h = report.hypothesis(plan, index);
        let step = Step {
            leg: Leg {
                foot: foot as usize,
                site: site as usize,
            },
            acceptor: acceptor.map(|a| a as usize),
        };
        let positions = setup.movable_positions(&parent.structure);
        let (s, _) = setup.grown_structure(&h.steps, &moved_of(h), &positions, step);
        let mut steps = parent.steps;
        steps.push(step);
        (s, steps)
    };
    let relaxed = relax(&mut s, &settings)?;
    Ok(Replayed {
        strain: relaxed.energy - report.reference_energy,
        structure: s,
        relaxed,
        steps,
    })
}
