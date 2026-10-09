//! What a sequential search is tunable by.

use crate::chemisorption::config::ChemisorptionError;
use crate::chemisorption::inventory::BondInventory;
use crate::chemisorption::relax::RelaxSettings;
use crate::chemisorption::transfer::{TransferDirection, TransferRule, is_transferable_element};
use crate::simulation::uff::VdwMode;

/// Every setting one sequential search depends on. Plain values, validated by
/// [`SequentialSearch::validate`] before anything runs.
#[derive(Debug, Clone)]
pub struct SequentialSearch {
    /// Adsorbate reactive atoms: those carrying this tag. `None` = all atoms.
    pub adsorbate_tag: Option<String>,
    /// Substrate reactive atoms: those carrying this tag. `None` = all atoms.
    pub substrate_tag: Option<String>,
    /// Leg 1: sites within this distance of a posed foot (Å).
    pub anchor_reach: f64,
    /// Legs 2 and 3: how far a site may lie outside the exact shell or ring
    /// (Å). The bond lengths already cover every bond direction, so this is
    /// only the room the molecule has to flex.
    pub tolerance: f64,
    /// Legs 4 and later: sites within this of a relaxed foot (Å); also the
    /// reach of the transfer rule, measured site to site.
    pub reach: f64,
    /// Prune seatings that clash instead of relaxing them. Clashes are
    /// detected and reported either way.
    pub clash_filter: bool,
    /// The enabled transfer kinds; only `to_substrate` (a leg donating its
    /// atom to a site). Empty = bond forming only.
    pub transfers: Vec<TransferRule>,
    /// At most this many legs per hypothesis. `None` = no cap.
    pub max_formed_bonds: Option<usize>,
    /// Only hypotheses with exactly this many legs are candidates.
    pub formed_bonds: Option<usize>,
    /// Only hypotheses with exactly this bond inventory are candidates.
    pub bond_inventory: Option<BondInventory>,
    /// At most this many relaxations; past it the search is truncated.
    pub budget: usize,
    /// At most this many candidates are kept, the lowest strains.
    pub top_n: usize,
    /// …and only those within this many kcal/mol of the best.
    pub energy_window: f64,
    /// UFF iteration limit per relaxation.
    pub max_iterations: u32,
    /// UFF convergence tolerance, RMS gradient (kcal/(mol·Å)).
    pub gradient_rms_tolerance: f64,
    /// How van der Waals terms are computed during relaxation.
    pub vdw_mode: VdwMode,
}

impl Default for SequentialSearch {
    /// The defaults set by the Phase 0 spike (§13 of the design).
    fn default() -> Self {
        Self {
            adsorbate_tag: None,
            substrate_tag: None,
            anchor_reach: 3.5,
            tolerance: 0.5,
            reach: 3.0,
            clash_filter: true,
            transfers: Vec::new(),
            max_formed_bonds: None,
            formed_bonds: None,
            bond_inventory: None,
            budget: 10_000,
            top_n: 10,
            energy_window: 30.0,
            max_iterations: 2000,
            gradient_rms_tolerance: 1e-3,
            vdw_mode: VdwMode::AllPairs,
        }
    }
}

impl SequentialSearch {
    /// Rejects settings no search can honour. Called by `plan`.
    pub fn validate(&self) -> Result<(), ChemisorptionError> {
        let invalid = |msg: &str| Err(ChemisorptionError::InvalidConfig(msg.to_string()));
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !positive(self.anchor_reach) {
            return invalid("anchor reach must be a positive distance");
        }
        if !(self.tolerance.is_finite() && self.tolerance >= 0.0) {
            return invalid("tolerance must be zero or a positive distance");
        }
        if !positive(self.reach) {
            return invalid("reach must be a positive distance");
        }
        for rule in &self.transfers {
            let symbol = crate::atomic_constants::element_symbol(rule.element);
            if !is_transferable_element(rule.element) {
                return Err(ChemisorptionError::InvalidConfig(format!(
                    "a transfer moves one monovalent atom, H or a halogen; {symbol} is not one"
                )));
            }
            if rule.direction == TransferDirection::ToAdsorbate {
                return Err(ChemisorptionError::InvalidConfig(format!(
                    "the transfer record ({symbol}, to_adsorbate) is not supported: a leg can \
                     only donate an atom to the substrate"
                )));
            }
        }
        if self.budget == 0 {
            return invalid("budget must be at least 1");
        }
        if self.top_n == 0 {
            return invalid("top N must be at least 1");
        }
        if self.energy_window.is_nan() || self.energy_window < 0.0 {
            return invalid("the energy window must be zero or positive");
        }
        if !positive(self.gradient_rms_tolerance) {
            return invalid("gradient tolerance must be positive");
        }
        Ok(())
    }

    /// The relaxation settings, in the shape `relax` reads.
    pub fn relax_settings(&self) -> RelaxSettings {
        RelaxSettings {
            max_iterations: self.max_iterations,
            gradient_rms_tolerance: self.gradient_rms_tolerance,
            vdw_mode: self.vdw_mode.clone(),
        }
    }
}
