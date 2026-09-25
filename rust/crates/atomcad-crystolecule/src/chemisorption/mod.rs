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
//! Scope so far is bond forming only: each adsorbate reactive atom with a free
//! valence forms at most one new bond, to a site within `reach`; a site takes
//! as many as its free valence allows; multi-bond hypotheses are pruned by the
//! pair tolerance. Scores are relative to the reference state (the same pose,
//! no bond changes, relaxed the same way).

pub mod config;
pub mod enumerate;
pub mod relax;
pub mod report;
pub mod score;

pub use config::{CHANGED_TAG, ChemisorptionError, ChemisorptionSearch, Side};
pub use enumerate::{Hypothesis, PlanStats, SearchPlan, free_valence, plan};
pub use relax::StrainTerms;
pub use report::{
    Candidate, SearchReport, SearchStats, evaluate, listed_count, rank_candidates, search,
};
pub use score::{BondInventory, BondKind};
