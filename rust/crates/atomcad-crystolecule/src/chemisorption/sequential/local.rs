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
//!
//! With `passivate_sites` (`passivate`) a state is always bare, and a
//! candidate is relaxed passivated. Above the deepest level a level runs in
//! two batches: every path bare (deduplicated on the bare strain, which picks
//! the winners and the next frontier), then each kept candidate passivated
//! from its winning path's start. At the deepest level nothing grows further,
//! so its paths are relaxed passivated directly and deduplicated on that.

use super::config::SequentialSearch;
use super::evaluate::{Candidate, RelaxedRow, SearchReport, check_cancelled};
use super::passivate::{PassivationKey, References};
use super::plan::{
    GEOMETRIC_LEGS, Hypothesis, SequentialPlan, admits, inventory_allows, step_kinds,
};
use super::setup::{Leg, NEAR_MISS_BAND, Setup, Step};
use super::tree::{NearMiss, RowKind, SearchTree};
use crate::atomic_structure::AtomicStructure;
use crate::chemisorption::atoms::{HypothesisKey, change_key};
use crate::chemisorption::config::{CHANGED_TAG, ChemisorptionError, PASSIVANT_TAG};
use crate::chemisorption::inventory::BondInventory;
use crate::chemisorption::relax::{RelaxSettings, Relaxed, StrainTerms, relax};
use atomcad_util::job_control::JobControl;
use glam::DVec3;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

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
    /// Relaxations the level needs: one per path that needs one (a
    /// candidate's, or a parent's of the next level; duplicate paths included,
    /// their strains decide which is kept), plus, with `passivate_sites`
    /// above the deepest level, one per candidate change set for its
    /// passivated relaxation.
    pub to_relax: usize,
    /// Relaxations run, both batches; fewer than `to_relax` when the budget
    /// ran out.
    pub relaxed: usize,
    /// Of `relaxed`, the second batch's: kept candidates relaxed again,
    /// passivated (`passivate_sites`, above the deepest level). Their rows
    /// were relaxed bare first, so they add no row of their own.
    pub twice: usize,
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
    /// Its three-leg ancestor (or itself) was a kept mirrored assignment.
    pub mirrored: bool,
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
    /// Against the bare reference, or a passivated relaxation's own.
    strain: f64,
    terms: StrainTerms,
    positions: Vec<DVec3>,
    /// A passivated relaxation: its key and its H's positions, in placement
    /// order (all the candidate's structure needs beyond `positions`).
    passivants: Option<(PassivationKey, Vec<DVec3>)>,
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
    /// its rows to `tree` and its hypotheses to `local`, and relaxes the
    /// passivated references its candidates need into `references`.
    pub fn level(
        &self,
        legs: usize,
        frontier: &[State],
        budget: usize,
        tree: &mut SearchTree,
        local: &mut Vec<Hypothesis>,
        references: &mut References,
    ) -> Result<Level, ChemisorptionError> {
        let setup = &self.plan.setup;
        let deeper = legs < self.plan.max_legs;
        let passivate = self.config.passivate_sites;
        // Above the deepest level a passivated candidate costs a second
        // relaxation; at the deepest, its one relaxation is passivated.
        let twice = passivate && deeper;
        let first_passivated = passivate && !deeper;
        let mut stats = LevelStats {
            legs,
            parents: frontier.len(),
            ..Default::default()
        };
        let paths = self.enumerate(legs, frontier, tree, local, &mut stats);
        let start = |i: usize| {
            let path = &paths[i];
            let state = &frontier[path.parent];
            let h = hypothesis(self.plan, local, state.hypothesis);
            setup.grown_structure(&h.steps, &moved_of(h), &state.positions, path.step)
        };

        // Relax what is needed, the children of the best parents first, as
        // far as the budget goes (the order is fixed before truncating). With
        // `twice`, the first path of a candidate change set also pays for
        // that set's passivated relaxation, so no set is half relaxed.
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
        let mut charged: BTreeSet<&HypothesisKey> = BTreeSet::new();
        let cost: Vec<usize> = order
            .iter()
            .map(|&i| 1 + usize::from(twice && paths[i].candidate && charged.insert(&paths[i].key)))
            .collect();
        stats.to_relax = cost.iter().sum();
        let mut spent = 0;
        let mut taken = 0;
        while taken < order.len() && spent + cost[taken] <= budget {
            spent += cost[taken];
            taken += 1;
        }
        if taken < order.len() {
            order.truncate(taken);
            stats.truncated = true;
        }
        if let Some(c) = self.control {
            c.set_phase(&format!("Local phase: leg {legs}"));
        }
        if first_passivated {
            let keys = order
                .iter()
                .map(|&i| setup.passivation_key(&start(i).0))
                .collect();
            references.ensure(self.plan, self.settings, keys, self.control)?;
        }
        let refs = &*references;
        let relax_path = |i: usize, passivated: bool| -> Result<Done, ChemisorptionError> {
            check_cancelled(self.control)?;
            let (mut s, moved) = start(i);
            let (reference, added) = if passivated {
                let (key, added) = setup.passivate(&mut s);
                (refs.get(&key), Some((key, added)))
            } else {
                (
                    super::passivate::Reference {
                        energy: self.reference_energy,
                        terms: *self.reference_terms,
                    },
                    None,
                )
            };
            let relaxed = relax(&mut s, self.settings)?;
            if let Some(c) = self.control {
                c.advance(1);
            }
            let passivants = added.map(|(key, ids)| {
                let at = ids
                    .iter()
                    .map(|&id| s.get_atom(id).expect("a placed H").position)
                    .collect();
                (key, at)
            });
            Ok(Done {
                moved,
                strain: relaxed.energy - reference.energy,
                terms: relaxed.terms.minus(&reference.terms),
                relaxed,
                positions: setup.movable_positions(&s),
                passivants,
            })
        };
        if let Some(c) = self.control {
            let total = c.snapshot().total.unwrap_or(0);
            c.set_total(total + order.len() as u64);
        }
        let relaxed: Vec<Done> = order
            .par_iter()
            .map(|&i| relax_path(i, first_passivated))
            .collect::<Result<_, _>>()?;
        stats.relaxed = relaxed.len();
        stats.unconverged = relaxed.iter().filter(|d| !d.relaxed.converged).count();

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
                .filter_map(|&i| done[i].as_ref().map(|d| (i, d.strain)))
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                .map_or(members[0], |(i, _)| i);
            for &i in members {
                winner[i] = best;
            }
        }

        // The second batch: each kept candidate, passivated, from its winning
        // path's start.
        let mut second: Vec<Option<Done>> = (0..paths.len()).map(|_| None).collect();
        if twice {
            let kept: Vec<usize> = (0..paths.len())
                .filter(|&i| winner[i] == i && paths[i].candidate && done[i].is_some())
                .collect();
            let keys = kept
                .iter()
                .map(|&i| setup.passivation_key(&start(i).0))
                .collect();
            references.ensure(self.plan, self.settings, keys, self.control)?;
            let refs = &*references;
            if let Some(c) = self.control {
                let total = c.snapshot().total.unwrap_or(0);
                c.set_total(total + kept.len() as u64);
            }
            let relax_path = |i: usize| -> Result<Done, ChemisorptionError> {
                check_cancelled(self.control)?;
                let (mut s, moved) = start(i);
                let (key, ids) = setup.passivate(&mut s);
                let reference = refs.get(&key);
                let relaxed = relax(&mut s, self.settings)?;
                if let Some(c) = self.control {
                    c.advance(1);
                }
                let at = ids
                    .iter()
                    .map(|&id| s.get_atom(id).expect("a placed H").position)
                    .collect();
                Ok(Done {
                    moved,
                    strain: relaxed.energy - reference.energy,
                    terms: relaxed.terms.minus(&reference.terms),
                    relaxed,
                    positions: setup.movable_positions(&s),
                    passivants: Some((key, at)),
                })
            };
            let relaxed: Vec<Done> = kept
                .par_iter()
                .map(|&i| relax_path(i))
                .collect::<Result<_, _>>()?;
            stats.relaxed += relaxed.len();
            stats.twice = relaxed.len();
            stats.unconverged += relaxed.iter().filter(|d| !d.relaxed.converged).count();
            for (&i, d) in kept.iter().zip(relaxed) {
                second[i] = Some(d);
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
        // A row relaxed twice records its passivated relaxation.
        let relaxed_rows: Vec<RelaxedRow> = order
            .iter()
            .map(|&i| {
                let d = second[i].as_ref().or(done[i].as_ref()).expect("relaxed");
                RelaxedRow {
                    hypothesis: index[winner[i]],
                    row: paths[i].row,
                    strain: d.strain,
                    converged: d.relaxed.converged,
                    passivated: d.passivants.is_some(),
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
                if path.candidate {
                    // The candidate's relaxation: the second batch's when
                    // there was one.
                    let c = second[i].take().unwrap_or_else(|| Done {
                        moved: d.moved,
                        relaxed: d.relaxed.clone(),
                        strain: d.strain,
                        terms: d.terms,
                        positions: d.positions.clone(),
                        passivants: d.passivants.clone(),
                    });
                    let (mut s, passivants) = match &c.passivants {
                        Some((key, at)) => setup.passivated_state_structure(
                            &h.steps,
                            &moved,
                            &c.positions,
                            key,
                            at,
                        ),
                        None => (
                            setup.state_structure(&h.steps, &moved, &c.positions),
                            Vec::new(),
                        ),
                    };
                    for id in h.changed_atoms() {
                        s.add_atom_tag(id, CHANGED_TAG)?;
                    }
                    for &id in &passivants {
                        s.add_atom_tag(id, PASSIVANT_TAG)?;
                    }
                    candidates.push(Candidate {
                        structure: s,
                        hypothesis: index[i],
                        row: path.row,
                        formed: h.formed.clone(),
                        transfers: h.transfers.clone(),
                        bond_inventory: h.inventory.clone(),
                        strain: c.strain,
                        terms: c.terms,
                        converged: c.relaxed.converged,
                        worst_bond_ratio: c.relaxed.worst_bond_ratio,
                        energy: c.relaxed.energy,
                        seating_clash: false,
                        mirrored: frontier[path.parent].mirrored,
                        passivants: passivants.len(),
                    });
                }
                if deeper {
                    // States are bare: the first batch's.
                    next.push(State {
                        hypothesis: index[i],
                        row: path.row,
                        strain: d.strain,
                        positions: d.positions,
                        mirrored: frontier[path.parent].mirrored,
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
                if h.steps.iter().any(|s| s.leg.foot == f) || !setup.may_be_next(f, h.steps.len()) {
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
    /// The H placed before relaxing (tagged `cs_passivant`): 0 unless the
    /// row's recorded relaxation was passivated.
    pub passivants: usize,
}

/// Rebuilds the relaxed state of tree row `row` as the search recorded it: a
/// geometric row by seating its change set and relaxing it, a local-phase row
/// by replaying its ancestors first — its own path's parent, which is always
/// a kept (canonical) state, since only those grow children. Relaxation is
/// deterministic, so this is the state the search produced, and its strain
/// is the one recorded for the row. A geometric duplicate row replays its
/// canonical row (the same change set seats the same). A row whose recorded
/// relaxation was passivated (`RelaxedRow::passivated`) is passivated as the
/// search did, its strain taken against its key's reference.
pub fn replay(
    plan: &SequentialPlan,
    report: &SearchReport,
    config: &SequentialSearch,
    row: u32,
) -> Result<Replayed, ChemisorptionError> {
    replay_as(plan, report, config, row, recorded_passivated(report, row))
}

/// The bare relaxed state of tree row `row`: what the next level grew from.
/// The same as [`replay`] unless the row's recorded relaxation is a
/// passivated one (a candidate that is also a parent).
pub fn replay_state(
    plan: &SequentialPlan,
    report: &SearchReport,
    config: &SequentialSearch,
    row: u32,
) -> Result<Replayed, ChemisorptionError> {
    replay_as(plan, report, config, row, false)
}

/// Whether the relaxation recorded for `row` (a geometric duplicate's: its
/// canonical row's) was passivated.
pub fn recorded_passivated(report: &SearchReport, row: u32) -> bool {
    let r = report.tree.row(row);
    let row = if r.legs as usize <= GEOMETRIC_LEGS {
        r.duplicate_of.unwrap_or(row)
    } else {
        row
    };
    report.relaxed.iter().any(|x| x.row == row && x.passivated)
}

fn replay_as(
    plan: &SequentialPlan,
    report: &SearchReport,
    config: &SequentialSearch,
    row: u32,
    passivated: bool,
) -> Result<Replayed, ChemisorptionError> {
    let (mut s, steps) = replay_start(plan, report, config, row)?;
    let (reference, passivants) = if passivated {
        let (key, added) = plan.setup.passivate(&mut s);
        let reference = report.references.get(&key).ok_or_else(|| {
            ChemisorptionError::DebugRow("the search relaxed no reference for this row".into())
        })?;
        (reference.energy, added)
    } else {
        (report.reference_energy, Vec::new())
    };
    let relaxed = relax(&mut s, &config.relax_settings())?;
    for &id in &passivants {
        s.add_atom_tag(id, PASSIVANT_TAG)?;
    }
    Ok(Replayed {
        strain: relaxed.energy - reference,
        structure: s,
        relaxed,
        steps,
        passivants: passivants.len(),
    })
}

/// The start geometry of tree row `row`, as the search relaxed it from, and
/// the legs along its path: a geometric row's seating (a function of its
/// change set, so a duplicate row seats as its canonical row), a local-phase
/// row's relaxed parent (replayed) plus its own leg. Relaxes only the
/// ancestors of a local-phase row.
pub fn replay_start(
    plan: &SequentialPlan,
    report: &SearchReport,
    config: &SequentialSearch,
    row: u32,
) -> Result<(AtomicStructure, Vec<Step>), ChemisorptionError> {
    let setup = &plan.setup;
    let r = report.tree.row(row);
    let RowKind::Leg {
        foot,
        site,
        acceptor,
    } = r.kind
    else {
        return Err(ChemisorptionError::InvalidConfig(
            "only a row with a bonded leg has a start geometry".into(),
        ));
    };
    if r.legs as usize <= GEOMETRIC_LEGS {
        let canonical = r.duplicate_of.unwrap_or(row);
        let index = report
            .tree
            .row(canonical)
            .hypothesis
            .expect("a canonical row has a hypothesis") as usize;
        let h = report.hypothesis(plan, index);
        let seating = plan.start_seating(h);
        return Ok((setup.start_structure(&h.steps, &seating), h.steps.clone()));
    }
    // A state is bare: the parent the search grew this row from.
    let parent = replay_state(plan, report, config, r.parent)?;
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
    Ok((s, steps))
}
