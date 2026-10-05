//! What a chemisorption search is tunable by, and what can go wrong.

use super::transfer::{TransferRule, is_transferable_element};
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
    /// At most this many bonds formed per hypothesis. `None` = no cap;
    /// `Some(0)` leaves only the patterns that form no bond (pure transfers).
    /// Transfers are not counted.
    pub max_formed_bonds: Option<usize>,
    /// The enabled transfer kinds (§6.1 of the design). Empty = bond forming
    /// only.
    pub transfers: Vec<TransferRule>,
    /// At most this many transfers per hypothesis, summed over all rules.
    /// `None` = no cap; `Some(0)` = none, the same search as no rules. Read
    /// only when `transfers` is non-empty.
    pub max_transfers: Option<usize>,
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
            max_formed_bonds: None,
            transfers: Vec::new(),
            max_transfers: Some(1),
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
        if let Some(rule) = self
            .transfers
            .iter()
            .find(|r| !is_transferable_element(r.element))
        {
            return Err(ChemisorptionError::InvalidConfig(format!(
                "a transfer moves one monovalent atom, H or a halogen; {} is not one",
                crate::atomic_constants::element_symbol(rule.element)
            )));
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
    #[error("relaxation failed: {0}")]
    Relaxation(String),
    #[error("tagging the result failed: {0}")]
    Tag(#[from] TagError),
}
