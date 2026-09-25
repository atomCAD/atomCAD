//! Exhaustive chemisorption search: given an adsorbate posed over a substrate
//! proxy, enumerate every bonding pattern the rules allow, relax each with UFF
//! and rank them by UFF strain plus a bond-energy term.
//!
//! The capability behind the `chemisorb` node, free of every node-network
//! concept. Split like `proxy_cut`: [`plan`] enumerates and counts without
//! relaxing anything (cheap, run on every evaluation), [`evaluate`] relaxes
//! and scores (seconds to minutes, run only on request), [`search`] is both.
//!
//! The only notion of "surface" is the **site**: a substrate reactive atom
//! with a free valence. There is no plane, facet or passivation concept, so
//! any geometry works, and a hydrogen placed on the substrate blocks its host
//! by saturating it.
//!
//! Two reaction kinds. **Bond forming** is always on: each adsorbate reactive
//! atom with a free valence forms at most one new bond, to a site within
//! `reach`; a site takes as many as its free valence allows; multi-bond
//! hypotheses are pruned by the pair tolerance. **Transfers** are opt-in, one
//! [`TransferRule`] per enabled (element, direction): a monovalent atom moves
//! from its donor to an acceptor on the other side (an OH leg handing its H to
//! a site, a radical foot abstracting surface H). Scores are relative to the
//! reference state (the same pose, no bond changes, relaxed the same way).

pub mod config;
pub mod enumerate;
pub mod fingerprint;
pub mod relax;
pub mod report;
pub mod score;
pub mod transfer;

pub use config::{CHANGED_TAG, ChemisorptionError, ChemisorptionSearch, Side};
pub use enumerate::{
    Hypothesis, HypothesisKey, PlanStats, SearchPlan, change_key, changed_atoms, free_valence, plan,
};
pub use fingerprint::input_fingerprint;
pub use relax::StrainTerms;
pub use report::{
    Candidate, SearchReport, SearchStats, evaluate, listed_count, rank_candidates, search,
};
pub use score::{BondInventory, BondKind};
pub use transfer::{Transfer, TransferDirection, TransferRule, is_transferable_element};
