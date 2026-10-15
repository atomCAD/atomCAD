//! What can go wrong in a chemisorption search, and the tag its results
//! carry. The settings are [`SequentialSearch`](super::sequential::SequentialSearch).

use crate::atomic_structure::TagError;
use crate::structure_compare::CompareError;

/// The tag every output structure carries on the atoms whose bonds the
/// candidate changed, so `apply_style` can highlight them.
pub const CHANGED_TAG: &str = "cs_changed";

/// The tag the H atoms `passivate_sites` places carry, in every candidate of a
/// search with the setting on, so `apply_style` can show or hide them.
pub const PASSIVANT_TAG: &str = "cs_passivant";

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
    /// Numbered foot tags (`foot1`, `foot2`, …) that do not give one leg
    /// order.
    #[error("foot order: {0}")]
    FootOrder(String),
    #[error("relaxation failed: {0}")]
    Relaxation(String),
    #[error("tagging the result failed: {0}")]
    Tag(#[from] TagError),
    /// Comparing the kept candidates for `unique` failed (`distinct`).
    #[error("comparing candidates: {0}")]
    Compare(#[from] CompareError),
    /// The search's `JobControl` was cancelled; no partial report is kept.
    #[error("search cancelled")]
    Cancelled,
    /// A debug view asked for a row the tree does not have, or a form the row
    /// has no geometry in.
    #[error("{0}")]
    DebugRow(String),
}
