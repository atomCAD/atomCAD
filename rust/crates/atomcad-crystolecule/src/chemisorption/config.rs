//! What a chemisorption search is tunable by, and what can go wrong.

use crate::atomic_structure::TagError;
use crate::simulation::uff::VdwMode;

/// The tag every output structure carries on the atoms whose bonds the
/// candidate changed, so `apply_style` can highlight them.
pub const CHANGED_TAG: &str = "cs_changed";

/// Every setting one search depends on. Plain values, validated by
/// [`ChemisorptionSearch::validate`] before anything runs; the node's property
/// rules are its own business.
#[derive(Debug, Clone)]
pub struct ChemisorptionSearch {
    /// Adsorbate reactive atoms: those carrying this tag. `None` = all atoms.
    pub adsorbate_tag: Option<String>,
    /// Substrate reactive atoms: those carrying this tag. `None` = all atoms.
    pub substrate_tag: Option<String>,
    /// Maximum distance between an adsorbate reactive atom and a site for a
    /// bond between them to be considered (Å).
    pub reach: f64,
    /// Pair tolerance `δ` (Å): two chosen sites must be as far apart as the
    /// two adsorbate atoms bonding to them, within this. `0.0` switches the
    /// check off.
    ///
    /// The default is calibrated, not guessed (the ignored
    /// `chemisorption_pruning_calibration` test): UFF lets a tripod's feet flex
    /// far enough that its best binding on Si(100) can need 1.45 Å, and its
    /// full three-leg binding, 23 kcal/mol above that, 2.64 Å. The design's
    /// first guess, 1.0 Å, pruned the best candidate in most poses.
    pub pair_tolerance: f64,
    /// At most this many bonds formed per hypothesis. `None` = no cap.
    pub max_formed_bonds: Option<usize>,
    /// At most this many valid hypotheses are relaxed; past it the search is
    /// truncated and makes no exhaustiveness claim.
    pub budget: usize,
    /// UFF iteration limit per relaxation.
    pub max_iterations: u32,
    /// UFF convergence tolerance, RMS gradient (kcal/(mol·Å)).
    pub gradient_rms_tolerance: f64,
    /// How van der Waals terms are computed during relaxation.
    pub vdw_mode: VdwMode,
}

impl Default for ChemisorptionSearch {
    fn default() -> Self {
        Self {
            adsorbate_tag: None,
            substrate_tag: None,
            reach: 3.5,
            pair_tolerance: 3.0,
            max_formed_bonds: None,
            budget: 10_000,
            max_iterations: 2000,
            gradient_rms_tolerance: 1e-3,
            vdw_mode: VdwMode::AllPairs,
        }
    }
}

impl ChemisorptionSearch {
    /// Rejects settings no search can honour. Called by `plan`.
    pub fn validate(&self) -> Result<(), ChemisorptionError> {
        let invalid = |msg: &str| Err(ChemisorptionError::InvalidConfig(msg.to_string()));
        if !(self.reach.is_finite() && self.reach > 0.0) {
            return invalid("reach must be a positive distance");
        }
        if !(self.pair_tolerance.is_finite() && self.pair_tolerance >= 0.0) {
            return invalid("pair tolerance must be zero (off) or a positive distance");
        }
        if self.max_formed_bonds == Some(0) {
            return invalid("max formed bonds must be at least 1");
        }
        if self.budget == 0 {
            return invalid("budget must be at least 1");
        }
        if !(self.gradient_rms_tolerance.is_finite() && self.gradient_rms_tolerance > 0.0) {
            return invalid("gradient tolerance must be positive");
        }
        Ok(())
    }
}

/// Which input a message is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Adsorbate,
    Substrate,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Side::Adsorbate => "adsorbate",
            Side::Substrate => "substrate",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChemisorptionError {
    #[error("invalid chemisorption settings: {0}")]
    InvalidConfig(String),
    /// A reactive tag no atom of that input carries. Reported rather than
    /// searched, since a mistyped tag would otherwise read as "found nothing".
    #[error("no {side} atom carries the tag '{tag}'")]
    UnknownTag { side: Side, tag: String },
    /// A bond change involves an element with no bond enthalpy, tabulated or
    /// estimable, so the candidate cannot be scored.
    #[error(
        "no bond enthalpy for {element}: it is outside the 12 scored elements \
         (H, C, N, O, F, Si, P, S, Cl, Ge, Br, I); untag it to keep it out of the search"
    )]
    UnscoredElement { element: String },
    #[error("relaxation failed: {0}")]
    Relaxation(String),
    #[error("tagging the result failed: {0}")]
    Tag(#[from] TagError),
}
