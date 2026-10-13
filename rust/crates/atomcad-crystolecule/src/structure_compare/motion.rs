//! The motion search (§3.4): propose a motion of B onto A, then verify it.
//!
//! A proposal can only cost a merge, never cause a wrong one: whatever
//! proposed the motion and whatever the pairing went through on the way, a
//! pair is equal only when the final check finds a same-element matching
//! within the tolerance **and** the least-squares fit of that very matching
//! keeps every pair within the tolerance (D2).

use super::cluster::{Cluster, Label};
use super::exact::{Mismatch, compare_structures_matched};
use super::{CompareSettings, Motion};
use crate::rigid_fit::{Fit, RANK_EPSILON, rigid_fit};
use glam::{DMat3, DVec3};
use std::collections::BTreeMap;

/// How far a proposal may be off and still have its pairing accepted, in
/// tolerances: a fit on three anchors is only as good as three atoms, and a
/// frame from principal axes only as good as the eigenvectors.
pub const PAIRING_RADIUS_FACTOR: f64 = 3.0;

/// Iterative-closest-point rounds after a proposal: refit on the pairing,
/// re-apply, pair again, until the pairing stops changing.
pub const REFINEMENT_ROUNDS: usize = 3;

/// A motion of B onto A, `p ↦ r·p + t`, and whether it is improper (a
/// reflection is part of it). Refinement keeps the handedness it was given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Proposal {
    pub r: DMat3,
    pub t: DVec3,
    pub improper: bool,
}

impl Proposal {
    pub const IDENTITY: Proposal = Proposal {
        r: DMat3::IDENTITY,
        t: DVec3::ZERO,
        improper: false,
    };

    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.r * p + self.t
    }

    fn from_fit(fit: &Fit, improper: bool) -> Proposal {
        Proposal {
            r: fit.r,
            t: fit.t,
            improper,
        }
    }
}

/// What the final check found for one motion: equal, with the least-squares
/// fit of the matching it found.
#[derive(Debug, Clone)]
pub struct Found {
    /// The least-squares motion of the allowed kind for `matching`.
    pub motion: Proposal,
    /// `(index in A, index in B)`, one per atom, in A's order.
    pub matching: Vec<(usize, usize)>,
    pub max_deviation: f64,
    pub rmsd: f64,
}

/// A motion that did not verify, kept to report the closest miss.
#[derive(Debug, Clone)]
pub struct Attempt {
    /// The motion the attempt ended on (after refinement, if it ran).
    pub motion: Proposal,
    /// A atom → B atom within `3·tolerance` under `motion`, same label.
    pub pairing: Vec<Option<usize>>,
    pub paired: usize,
    /// Over the paired atoms; NaN when none paired.
    pub max_deviation: f64,
}

impl Attempt {
    /// More atoms paired is better; then a smaller deviation.
    fn better_than(&self, other: &Attempt) -> bool {
        self.paired > other.paired
            || (self.paired == other.paired
                && self.max_deviation.total_cmp(&other.max_deviation).is_lt())
    }
}

/// The verdict on one proposed motion.
#[derive(Debug, Clone)]
pub enum Verification {
    Equal(Found),
    NotEqual(Attempt),
}

/// The per-pair state shared by every proposal: which atoms of B may partner
/// which atoms of A.
pub struct PairContext<'a> {
    a: &'a Cluster,
    b: &'a Cluster,
    settings: &'a CompareSettings,
    /// A atom → its partner group; `b_groups[g]` are the B atoms of group `g`,
    /// ascending.
    group_of_a: Vec<usize>,
    b_groups: Vec<Vec<usize>>,
}

impl<'a> PairContext<'a> {
    pub fn new(a: &'a Cluster, b: &'a Cluster, settings: &'a CompareSettings) -> PairContext<'a> {
        let bonds = settings.compare_bonds;
        let mut group_ids: BTreeMap<Label, usize> = BTreeMap::new();
        let group_of_a: Vec<usize> = (0..a.len())
            .map(|i| {
                let next = group_ids.len();
                *group_ids.entry(pairing_key(a, i, bonds)).or_insert(next)
            })
            .collect();
        let mut b_groups = vec![Vec::new(); group_ids.len()];
        for j in 0..b.len() {
            if let Some(&g) = group_ids.get(&pairing_key(b, j, bonds)) {
                b_groups[g].push(j);
            }
        }
        PairContext {
            a,
            b,
            settings,
            group_of_a,
            b_groups,
        }
    }

    /// The B atoms that may partner A atom `i`, ascending.
    fn partners(&self, i: usize) -> &[usize] {
        &self.b_groups[self.group_of_a[i]]
    }

    /// Pairs each atom of A, in index order, with the nearest unclaimed atom of
    /// B under `motion` that has its label (its element when bonds are not
    /// compared), within `3·tolerance`. Ties go to the lower index.
    pub fn pair(&self, motion: &Proposal) -> Vec<Option<usize>> {
        let moved: Vec<DVec3> = self
            .b
            .positions()
            .iter()
            .map(|p| motion.apply(*p))
            .collect();
        let radius = PAIRING_RADIUS_FACTOR * self.settings.tolerance;
        let mut claimed = vec![false; self.b.len()];
        let mut pairing = vec![None; self.a.len()];
        for (i, slot) in pairing.iter_mut().enumerate() {
            let target = self.a.positions()[i];
            let mut best: Option<(usize, f64)> = None;
            for &j in self.partners(i) {
                if claimed[j] {
                    continue;
                }
                let d = moved[j].distance(target);
                if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((j, d));
                }
            }
            if let Some((j, _)) = best {
                claimed[j] = true;
                *slot = Some(j);
            }
        }
        pairing
    }

    /// The least-squares motion of the allowed kind taking B's atoms of
    /// `pairs` onto their A partners, with the handedness of `improper`.
    /// `None` when the fit is undetermined (no pairs).
    pub fn fit(&self, pairs: &[(usize, usize)], improper: bool) -> Option<Proposal> {
        if pairs.is_empty() {
            return None;
        }
        let a_points: Vec<DVec3> = pairs.iter().map(|&(i, _)| self.a.positions()[i]).collect();
        let b_points: Vec<DVec3> = pairs.iter().map(|&(_, j)| self.b.positions()[j]).collect();
        match self.settings.motion {
            Motion::None => Some(Proposal::IDENTITY),
            Motion::Translation => {
                let n = pairs.len() as f64;
                let t = a_points.iter().copied().sum::<DVec3>() / n
                    - b_points.iter().copied().sum::<DVec3>() / n;
                Some(Proposal {
                    r: DMat3::IDENTITY,
                    t,
                    improper: false,
                })
            }
            Motion::Rigid | Motion::RigidMirror => rigid_fit(&b_points, &a_points, improper)
                .map(|fit| Proposal::from_fit(&fit, improper)),
        }
    }

    /// Max and RMS distance of `pairs` under `motion`.
    pub fn deviations(&self, pairs: &[(usize, usize)], motion: &Proposal) -> (f64, f64) {
        if pairs.is_empty() {
            return (f64::NAN, f64::NAN);
        }
        let mut max = 0.0f64;
        let mut sum_sq = 0.0;
        for &(i, j) in pairs {
            let d = motion
                .apply(self.b.positions()[j])
                .distance(self.a.positions()[i]);
            max = max.max(d);
            sum_sq += d * d;
        }
        (max, (sum_sq / pairs.len() as f64).sqrt())
    }

    /// The final check's mismatches under `motion`: the same-element
    /// comparison of A with B moved, bond differences dropped when bonds are
    /// not compared. Also returns the matching it found, as index pairs.
    pub fn mismatches(&self, motion: &Proposal) -> (Vec<Mismatch>, Vec<(usize, usize)>) {
        let moved = self.b.moved_structure(|p| motion.apply(p));
        let matched =
            compare_structures_matched(self.a.structure(), &moved, self.settings.tolerance, true);
        let mut mismatches = matched.mismatches;
        if !self.settings.compare_bonds {
            mismatches.retain(|m| {
                matches!(
                    m,
                    Mismatch::UnmatchedInA { .. }
                        | Mismatch::UnmatchedInB { .. }
                        | Mismatch::ElementDiffers { .. }
                )
            });
        }
        // Cluster structures give atom `i` the id `i + 1`.
        let pairs = matched
            .pairs
            .into_iter()
            .map(|(ia, ib)| (ia as usize - 1, ib as usize - 1))
            .collect();
        (mismatches, pairs)
    }

    /// Verifies one proposed motion: pair, refine, decide (§3.4).
    pub fn verify(&self, proposal: &Proposal) -> Verification {
        let mut motion = *proposal;
        let mut pairing = self.pair(&motion);
        // `none` has no motion to refine and goes straight to the decision;
        // its pairing only describes the attempt.
        if self.settings.motion != Motion::None {
            for _ in 0..REFINEMENT_ROUNDS {
                let pairs = complete_pairs(&pairing);
                if pairs.len() < self.refinement_quorum() {
                    break;
                }
                let Some(refit) = self.fit(&pairs, motion.improper) else {
                    break;
                };
                motion = refit;
                let next = self.pair(&motion);
                if next == pairing {
                    break;
                }
                pairing = next;
            }
            if pairing.iter().any(Option::is_none) {
                return Verification::NotEqual(self.attempt(motion, pairing));
            }
        }

        if let Some(found) = self.decide(&motion) {
            return Verification::Equal(found);
        }
        Verification::NotEqual(self.attempt(motion, pairing))
    }

    /// The final check: the same-element comparison under `motion`, then the
    /// least-squares fit of the matching it found, which must keep every pair
    /// within the tolerance. That second part is what makes the verdict D2
    /// rather than "within tolerance under some motion".
    pub fn decide(&self, motion: &Proposal) -> Option<Found> {
        let (mismatches, matching) = self.mismatches(motion);
        if !mismatches.is_empty() || matching.len() != self.a.len() {
            return None;
        }
        let refit = self.fit(&matching, motion.improper)?;
        let (max_deviation, rmsd) = self.deviations(&matching, &refit);
        (max_deviation <= self.settings.tolerance).then_some(Found {
            motion: refit,
            matching,
            max_deviation,
            rmsd,
        })
    }

    /// How many atoms a pairing must hold before it is refitted on: a fit on
    /// a few right pairs pulls the stragglers in, a fit on a handful does not.
    /// The decision still needs every atom paired.
    fn refinement_quorum(&self) -> usize {
        let n = self.a.len();
        if n <= 3 { n } else { 3.max(n.div_ceil(2)) }
    }

    /// Every pair of `pairs` that `motion` leaves beyond the tolerance, as an
    /// unmatched atom of A — what is left to report when the final check
    /// passes under a motion whose matching's own fit then fails.
    pub fn far_pairs(&self, pairs: &[(usize, usize)], motion: &Proposal) -> Vec<Mismatch> {
        pairs
            .iter()
            .filter(|&&(i, j)| {
                motion
                    .apply(self.b.positions()[j])
                    .distance(self.a.positions()[i])
                    > self.settings.tolerance
            })
            .map(|&(i, _)| Mismatch::UnmatchedInB {
                position: self.a.positions()[i],
                element: self.a.elements()[i],
            })
            .collect()
    }

    fn attempt(&self, motion: Proposal, pairing: Vec<Option<usize>>) -> Attempt {
        let pairs = complete_pairs(&pairing);
        let (max_deviation, _) = self.deviations(&pairs, &motion);
        Attempt {
            motion,
            paired: pairs.len(),
            pairing,
            max_deviation,
        }
    }
}

/// What the whole search found for one pair.
#[derive(Debug, Clone, Default)]
pub struct SearchOutcome {
    pub found: Option<Found>,
    /// The closest miss, when no motion verified.
    pub best: Option<Attempt>,
    /// The work cap stopped the search with proposals left.
    pub undecided: bool,
    /// Proposals verified.
    pub proposals: usize,
}

/// Runs the proposals in order — the one proposal of `none` / `translation`;
/// the principal axes, then the anchor search, for the rigid motions — and
/// stops at the first that verifies or at the work cap.
pub fn search(context: &PairContext) -> SearchOutcome {
    let mut searcher = Searcher {
        context,
        outcome: SearchOutcome::default(),
    };
    let settings = context.settings;
    match settings.motion {
        Motion::None => {
            searcher.offer(Proposal::IDENTITY);
        }
        Motion::Translation => {
            searcher.offer(Proposal {
                r: DMat3::IDENTITY,
                t: context.a.centroid - context.b.centroid,
                improper: false,
            });
        }
        Motion::Rigid | Motion::RigidMirror => {
            let mirror = settings.motion == Motion::RigidMirror;
            let stopped = settings.proposals.principal_axes
                && principal_axes(context.a, context.b, mirror)
                    .into_iter()
                    .any(|p| searcher.offer(p));
            if !stopped && settings.proposals.anchors {
                anchor_search(&mut searcher, mirror);
            }
        }
    }
    searcher.outcome
}

struct Searcher<'c, 'a> {
    context: &'c PairContext<'a>,
    outcome: SearchOutcome,
}

impl Searcher<'_, '_> {
    /// Verifies `proposal` unless the search is over; `true` stops the search
    /// (equal found, or the cap reached with this proposal still to try).
    fn offer(&mut self, proposal: Proposal) -> bool {
        if self.outcome.found.is_some() {
            return true;
        }
        if self.outcome.proposals >= self.context.settings.max_proposals {
            self.outcome.undecided = true;
            return true;
        }
        self.outcome.proposals += 1;
        match self.context.verify(&proposal) {
            Verification::Equal(found) => {
                self.outcome.found = Some(found);
                true
            }
            Verification::NotEqual(attempt) => {
                if self
                    .outcome
                    .best
                    .as_ref()
                    .is_none_or(|best| attempt.better_than(best))
                {
                    self.outcome.best = Some(attempt);
                }
                false
            }
        }
    }
}

/// The principal-axes proposals: B's axes rotated onto A's about the
/// centroids, for every sign of each axis — 4 proper, and with `mirror` 4
/// improper. Fixed order.
pub fn principal_axes(a: &Cluster, b: &Cluster, mirror: bool) -> Vec<Proposal> {
    let mut proper = Vec::new();
    let mut improper = Vec::new();
    for bits in 0..8u32 {
        let sign = |k: u32| if bits & (1 << k) == 0 { 1.0 } else { -1.0 };
        let s = DMat3::from_diagonal(DVec3::new(sign(0), sign(1), sign(2)));
        let r = a.moment_axes * s * b.moment_axes.transpose();
        let proposal = |improper| Proposal {
            r,
            t: a.centroid - r * b.centroid,
            improper,
        };
        if r.determinant() > 0.0 {
            proper.push(proposal(false));
        } else {
            improper.push(proposal(true));
        }
    }
    if mirror {
        proper.extend(improper);
    }
    proper
}

/// The anchor search: up to three anchors in A, the first of the rarest label,
/// the others making a well-conditioned triangle; in B every label-consistent
/// triple whose distances agree within `2·tolerance`, each fitted proper and,
/// with `mirror`, improper.
fn anchor_search(searcher: &mut Searcher, mirror: bool) {
    let context = searcher.context;
    let (a, b) = (context.a, context.b);
    let tolerance = context.settings.tolerance;
    let anchors = choose_anchors(context);
    let a_points: Vec<DVec3> = anchors.iter().map(|&i| a.positions()[i]).collect();
    // The least-squares fit of the whole matching moves each anchor by at most
    // the tolerance, so the anchors' own fit leaves an RMS of at most that and
    // a max of at most √k times it: a triple fitting worse cannot belong to an
    // equal pair, and is dropped without counting as a proposal.
    let residual_bound = (anchors.len() as f64).sqrt() * tolerance + 1e-9;
    let distance_bound = 2.0 * tolerance + 1e-9;
    // Reflecting a point or a line only changes a rotation that was
    // undetermined anyway.
    let improper_too = mirror && anchors.len() == 3;

    let agrees = |chosen: &[usize], candidate: usize| {
        chosen.iter().enumerate().all(|(k, &j)| {
            j != candidate
                && ((b.positions()[candidate].distance(b.positions()[j]))
                    - a_points[k].distance(a_points[chosen.len()]))
                .abs()
                    <= distance_bound
        })
    };

    // Depth-first over the triples, in index order.
    let mut stack: Vec<(Vec<usize>, usize)> = vec![(Vec::new(), 0)];
    while let Some((chosen, next)) = stack.pop() {
        let depth = chosen.len();
        if depth == anchors.len() {
            let b_points: Vec<DVec3> = chosen.iter().map(|&j| b.positions()[j]).collect();
            for improper in [false, true] {
                if improper && !improper_too {
                    continue;
                }
                let Some(fit) = rigid_fit(&b_points, &a_points, improper) else {
                    continue;
                };
                if fit.residual > residual_bound {
                    continue;
                }
                if searcher.offer(Proposal::from_fit(&fit, improper)) {
                    return;
                }
            }
            continue;
        }
        let candidates = context.partners(anchors[depth]);
        // Resume this level after `next`, then descend into the candidate.
        if let Some(position) = candidates[next..]
            .iter()
            .position(|&j| agrees(&chosen, j))
            .map(|p| p + next)
        {
            stack.push((chosen.clone(), position + 1));
            let mut deeper = chosen;
            deeper.push(candidates[position]);
            stack.push((deeper, 0));
        }
    }
}

/// A's anchors: the first atom of the rarest pairing group (ties by the
/// group's label order), then the atom farthest from it, then the atom
/// farthest from the line through the two. Fewer when the cluster is a point
/// or a line.
fn choose_anchors(context: &PairContext) -> Vec<usize> {
    let a = context.a;
    let mut groups: BTreeMap<Label, Vec<usize>> = BTreeMap::new();
    for i in 0..a.len() {
        groups
            .entry(pairing_key(a, i, context.settings.compare_bonds))
            .or_default()
            .push(i);
    }
    let first = groups
        .values()
        .min_by_key(|members| members.len())
        .expect("a cluster has an atom")[0];
    let p = a.positions();
    let farthest = |score: &dyn Fn(usize) -> f64| {
        (0..a.len())
            .map(|i| (i, score(i)))
            .fold(None::<(usize, f64)>, |best, (i, s)| match best {
                Some((_, bs)) if s <= bs => best,
                _ => Some((i, s)),
            })
            .filter(|&(_, s)| s > RANK_EPSILON)
            .map(|(i, _)| i)
    };
    let mut anchors = vec![first];
    let Some(second) = farthest(&|i| p[i].distance(p[first])) else {
        return anchors;
    };
    anchors.push(second);
    let axis = (p[second] - p[first]).normalize();
    if let Some(third) = farthest(&|i| (p[i] - p[first]).cross(axis).length()) {
        anchors.push(third);
    }
    anchors
}

/// What an atom must share with its partner: its label when bonds are
/// compared, its element otherwise.
fn pairing_key(cluster: &Cluster, i: usize, bonds: bool) -> Label {
    if bonds {
        cluster.labels()[i].clone()
    } else {
        Label {
            element: cluster.elements()[i],
            neighbours: Vec::new(),
        }
    }
}

fn complete_pairs(pairing: &[Option<usize>]) -> Vec<(usize, usize)> {
    pairing
        .iter()
        .enumerate()
        .filter_map(|(i, j)| j.map(|j| (i, j)))
        .collect()
}
