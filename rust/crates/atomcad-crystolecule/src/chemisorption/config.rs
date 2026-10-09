//! What can go wrong in a chemisorption search, and the tag its results
//! carry. The settings are [`SequentialSearch`](super::sequential::SequentialSearch).

use crate::atomic_structure::TagError;

/// The tag every output structure carries on the atoms whose bonds the
/// candidate changed, so `apply_style` can highlight them.
pub const CHANGED_TAG: &str = "cs_changed";

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
    /// The search's `JobControl` was cancelled; no partial report is kept.
    #[error("search cancelled")]
    Cancelled,
}
