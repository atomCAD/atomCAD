//! Exhaustive chemisorption search: given an adsorbate posed over a substrate
//! proxy, enumerate every bonding pattern the rules allow, relax each with UFF
//! and rank them by UFF energy against the same pose relaxed unbonded.
//!
//! The capability behind the `chemisorb` node, free of every node-network
//! concept. Split like `proxy_cut`: [`plan`] enumerates and counts without
//! relaxing anything (cheap, run on every evaluation), [`evaluate`] relaxes
//! and ranks (seconds to minutes, run only on request), [`search`] is both.
//!
//! The only notion of "surface" is the **site**: a substrate reactive atom
//! with a free valence. There is no plane, facet or passivation concept, so
//! any geometry works, and a hydrogen placed on the substrate blocks its host
//! by saturating it.
//!
//! Two reaction kinds. **Bond forming** is always on: each adsorbate reactive
//! atom with a free valence forms at most one new bond, to a site within
//! `reach`; a site takes as many as its free valence allows. **Transfers** are
//! opt-in, one
//! [`TransferRule`] per enabled (element, direction): a monovalent atom moves
//! from its donor to an acceptor on the other side (an OH leg handing its H to
//! a site, a radical foot abstracting surface H). Strains are relative to the
//! reference state (the same pose, no bond changes, relaxed the same way).
//!
//! **There is no bond-energy term.** The search is used under kinetic
//! control, where the absolute energy of a product says little about whether
//! it forms, and tabulated bond enthalpies are too crude to supply it anyway.
//! The ranking is UFF energy alone, which favours fewer bonds; candidates with
//! different bond inventories ([`BondInventory`]) do not compare cleanly, which
//! is why a search can be restricted to one formed-bond count or one
//! inventory. The enthalpy tables live in [`crate::bond_enthalpy`], unused here.
//!
//! **Every setting is a search setting, and each is applied as early as it can
//! be:** tags, reach, valence, the caps and the two filters during
//! enumeration (so only the hypotheses asked for are relaxed); `top_n` and the
//! energy window while relaxing (so only the kept structures are held). Memory
//! is bounded by `top_n`, not by the number of hypotheses.

pub mod config;
pub mod enumerate;
pub mod fingerprint;
pub mod inventory;
pub mod relax;
pub mod report;
pub mod sequential;
pub mod transfer;

pub use config::{CHANGED_TAG, ChemisorptionError, ChemisorptionSearch, Side};
pub use enumerate::{
    Hypothesis, HypothesisKey, PlanStats, SearchPlan, change_key, changed_atoms, free_valence, plan,
};
pub use fingerprint::input_fingerprint;
pub use inventory::{BondInventory, BondKind, InventoryParseError};
pub use relax::StrainTerms;
pub use report::{
    Candidate, SearchReport, SearchStats, evaluate, inventory_options, rank_candidates, search,
};
pub use transfer::{Transfer, TransferDirection, TransferRule, is_transferable_element};
