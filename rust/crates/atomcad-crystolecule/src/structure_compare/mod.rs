//! Comparing two atomic structures up to a tolerance and, optionally, a rigid
//! motion. Design doc: `doc/design_structure_comparison.md`.
//!
//! **Equal** means (D2): a motion of the allowed kind and a one-to-one matching
//! of atoms, same element, every matched pair within `tolerance` after the
//! least-squares fit of that matching — and, when bonds are compared, the
//! matching agreeing on every bond and its order. The max deviation decides,
//! never the RMS.
//!
//! The pipeline, per pair of [`Cluster`]s:
//!
//! 1. **Pre-checks** ([`invariants`]): element counts, bond counts, bonded
//!    neighbourhoods, second-moment eigenvalues, sorted distances. Each is an
//!    exact rejection.
//! 2. **Motion search** ([`motion`]): proposals from the principal axes, then
//!    from anchor triples; each one paired, refined and decided.
//! 3. **Decision**: [`compare_structures_matched`] under the motion found, then
//!    the least-squares fit of the matching it returned.
//!
//! "Equal" is therefore exact by construction; "not equal" is very likely but
//! not proven (§3.5), and a pair that exceeds the work cap is reported
//! **undecided** rather than hidden.
//!
//! [`group_in_order`] groups a list of clusters with it, and the exact,
//! motion-free comparison the mechanosynth generators verify with
//! ([`compare_structures`]) lives here too.

pub mod cluster;
pub mod exact;
pub mod group;
pub mod invariants;
pub mod motion;

pub use cluster::{Cluster, Label};
pub use exact::{
    MatchedComparison, Mismatch, compare_structures, compare_structures_matched,
    describe_mismatches,
};
pub use group::{Grouping, group_in_order};

use crate::rigid_fit::Fit;
use motion::{PairContext, Proposal};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Largest accepted tolerance, Å. The final check pairs atoms of one element
/// by nearest neighbour, which is unambiguous while `4·tolerance` is below the
/// shortest same-element distance; 0.3 Å keeps that true for every ordinary
/// bond (§3.1).
pub const MAX_TOLERANCE: f64 = 0.3;

/// Which motions may relate the two structures (D3). One choice rather than
/// two checkboxes: a rotation without a translation means nothing without a
/// fixed origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    /// Compared where they stand.
    None,
    Translation,
    /// Proper rigid motions.
    Rigid,
    /// Rigid motions with or without a reflection.
    #[default]
    RigidMirror,
}

impl Motion {
    /// The spelling of the text format and the node data.
    pub fn name(self) -> &'static str {
        match self {
            Motion::None => "none",
            Motion::Translation => "translation",
            Motion::Rigid => "rigid",
            Motion::RigidMirror => "rigid_mirror",
        }
    }
}

/// Which proposal paths the rigid motions use. Both on by default and in every
/// caller; turning one off exists for the tests that check the two paths
/// against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposals {
    pub principal_axes: bool,
    pub anchors: bool,
}

impl Default for Proposals {
    fn default() -> Self {
        Proposals {
            principal_axes: true,
            anchors: true,
        }
    }
}

/// How two clusters are compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompareSettings {
    pub motion: Motion,
    /// Max per-atom deviation after the fit, Å, in `(0, MAX_TOLERANCE]`.
    pub tolerance: f64,
    /// Bonds between cluster atoms must agree in presence and order.
    pub compare_bonds: bool,
    /// The work cap (D5): proposals verified per pair before it is undecided.
    pub max_proposals: usize,
    pub proposals: Proposals,
}

/// Default tolerance, Å: two relaxations reaching the same minimum agree only
/// to the minimizer's convergence, and a shifted copy sits in a slightly
/// different finite-slab environment.
pub const DEFAULT_TOLERANCE: f64 = 0.15;

/// Default work cap. A proposal costs ~40 µs on a 125-atom cluster driven to
/// the cap (release, measured by `pair_at_the_work_cap_is_fast`: 8.5 ms for
/// 200), so 200 bound a pair at ~10 ms — and a typical pair needs a handful.
pub const DEFAULT_MAX_PROPOSALS: usize = 200;

impl Default for CompareSettings {
    fn default() -> Self {
        CompareSettings {
            motion: Motion::RigidMirror,
            tolerance: DEFAULT_TOLERANCE,
            compare_bonds: true,
            max_proposals: DEFAULT_MAX_PROPOSALS,
            proposals: Proposals::default(),
        }
    }
}

impl CompareSettings {
    pub fn validate(&self) -> Result<(), CompareError> {
        if !(self.tolerance > 0.0 && self.tolerance <= MAX_TOLERANCE) {
            return Err(CompareError::Tolerance(self.tolerance));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CompareError {
    #[error("tolerance {0} Å is outside (0, {MAX_TOLERANCE}]")]
    Tolerance(f64),
    #[error("no atom carries the tag '{0}'")]
    UnknownTag(String),
    #[error("atom {0} is not in the structure")]
    UnknownAtom(u32),
    #[error("an empty cluster cannot be compared")]
    EmptyCluster,
}

/// Which pre-check rejected a pair (§3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreCheck {
    ElementCounts,
    BondCounts,
    Labels,
    Eigenvalues,
    Distances,
}

/// A pre-check's rejection: which one, and a one-line reason
/// ("element counts differ: C 6/5").
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub check: PreCheck,
    pub reason: String,
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

/// The answer for one pair. Exactly one of three holds: `equal` (no
/// mismatches); `rejected_by` is set (no mismatches — the reason is the
/// pre-check's); or neither, and `mismatches` is non-empty against the best
/// fit found (with `undecided` set when the work cap stopped the search).
#[derive(Debug, Clone)]
pub struct Comparison {
    pub equal: bool,
    pub undecided: bool,
    /// A pre-check decided; no motion was tried.
    pub rejected_by: Option<Rejection>,
    /// Of the fit reported in `transform`; NaN when there is none.
    pub max_deviation: f64,
    pub rmsd: f64,
    /// B onto A; `r` may be improper under `rigid_mirror`. `None` for `none`,
    /// and when no fit was found. When equal, the least-squares fit of
    /// `matching`; otherwise the best fit: the motion that paired the most
    /// atoms, ties by the smaller deviation.
    pub transform: Option<Fit>,
    /// When equal: `(index in A, index in B)` for every atom. Empty otherwise.
    pub matching: Vec<(usize, usize)>,
    /// Against the best fit, when not equal and not rejected.
    pub mismatches: Vec<Mismatch>,
    /// Proposed motions verified.
    pub proposals: usize,
}

/// Compares `b` against `a` (§3).
pub fn compare(
    a: &Cluster,
    b: &Cluster,
    settings: &CompareSettings,
) -> Result<Comparison, CompareError> {
    settings.validate()?;
    let mut comparison = Comparison {
        equal: false,
        undecided: false,
        rejected_by: None,
        max_deviation: f64::NAN,
        rmsd: f64::NAN,
        transform: None,
        matching: Vec::new(),
        mismatches: Vec::new(),
        proposals: 0,
    };
    if let Err(rejection) = invariants::pre_check(
        a,
        b,
        settings.motion,
        settings.tolerance,
        settings.compare_bonds,
    ) {
        comparison.rejected_by = Some(rejection);
        return Ok(comparison);
    }

    let context = PairContext::new(a, b, settings);
    let outcome = motion::search(&context);
    comparison.proposals = outcome.proposals;
    comparison.undecided = outcome.undecided;
    let as_fit = |p: &Proposal, residual: f64| {
        (settings.motion != Motion::None).then_some(Fit {
            r: p.r,
            t: p.t,
            residual,
        })
    };

    if let Some(found) = outcome.found {
        comparison.equal = true;
        comparison.undecided = false;
        comparison.max_deviation = found.max_deviation;
        comparison.rmsd = found.rmsd;
        comparison.transform = as_fit(&found.motion, found.max_deviation);
        comparison.matching = found.matching;
        return Ok(comparison);
    }

    // Not equal: report against the best attempt. Its motion is refitted on
    // the atoms it did pair when that leaves fewer differences — the alignment
    // a user would pick by eye, not the raw proposal it started from.
    let Some(best) = outcome.best else {
        comparison.mismatches = describable(&context, &Proposal::IDENTITY);
        return Ok(comparison);
    };
    let pairs: Vec<(usize, usize)> = best
        .pairing
        .iter()
        .enumerate()
        .filter_map(|(i, j)| j.map(|j| (i, j)))
        .collect();
    let mut motion = best.motion;
    let mut mismatches = describable(&context, &motion);
    if let Some(refit) = context.fit(&pairs, best.motion.improper) {
        let refit_mismatches = describable(&context, &refit);
        if refit_mismatches.len() <= mismatches.len() {
            motion = refit;
            mismatches = refit_mismatches;
        }
    }
    if mismatches.is_empty() {
        // `describable` found nothing beyond the tolerance under the refit of
        // a complete matching: that is D2, so the pair is equal after all.
        if let Some(found) = context.decide(&motion) {
            comparison.equal = true;
            comparison.undecided = false;
            comparison.max_deviation = found.max_deviation;
            comparison.rmsd = found.rmsd;
            comparison.transform = as_fit(&found.motion, found.max_deviation);
            comparison.matching = found.matching;
            return Ok(comparison);
        }
    }
    let (max_deviation, rmsd) = context.deviations(&pairs, &motion);
    comparison.max_deviation = max_deviation;
    comparison.rmsd = rmsd;
    comparison.transform = as_fit(&motion, max_deviation);
    comparison.mismatches = mismatches;
    Ok(comparison)
}

/// The mismatches under `motion`, never empty for a pair that is not equal.
///
/// The final check can pass under a motion whose matching's own least-squares
/// fit then leaves a pair beyond the tolerance (the fit minimizes the sum of
/// squares, not the max). There the comparison has nothing to say, so every
/// atom of `a` the refit leaves too far is reported unmatched.
fn describable(context: &PairContext, motion: &Proposal) -> Vec<Mismatch> {
    let (mismatches, matching) = context.mismatches(motion);
    if !mismatches.is_empty() {
        return mismatches;
    }
    let refit = context.fit(&matching, motion.improper).unwrap_or(*motion);
    context.far_pairs(&matching, &refit)
}
