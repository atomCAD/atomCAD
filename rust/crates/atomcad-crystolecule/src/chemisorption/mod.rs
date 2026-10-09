//! Chemisorption search: given an adsorbate posed over a substrate proxy,
//! find the ways its feet can bond to the substrate's sites, relax each with
//! UFF and rank them by UFF energy against the separated state (the adsorbate
//! and the substrate each relaxed alone).
//!
//! The capability behind the `chemisorb` node, free of every node-network
//! concept. The search is [`sequential`]: it builds each binding leg by leg,
//! so it covers the adsorbate's orientations without sampling them (design
//! `design_chemisorption_sequential.md`, in the external mechanosynth working
//! folder). Split like `proxy_cut`: `sequential::plan` enumerates and counts
//! the geometric legs without relaxing anything (cheap, run on every
//! evaluation), `sequential::evaluate` relaxes, runs the local phase and
//! ranks (seconds to minutes, run only on request), `sequential::search` is
//! both.
//!
//! The only notion of "surface" is the **site**: a substrate reactive atom
//! with a free valence. There is no plane, facet or passivation concept in
//! what the search admits, so any geometry works, and a hydrogen placed on the
//! substrate blocks its host by saturating it.
//!
//! **There is no bond-energy term.** The search is used under kinetic
//! control, where the absolute energy of a product says little about whether
//! it forms, and tabulated bond enthalpies are too crude to supply it anyway.
//! The ranking is UFF energy alone, which favours fewer bonds; candidates with
//! different bond inventories ([`BondInventory`]) do not compare cleanly, which
//! is why a search can be restricted to one formed-bond count or one
//! inventory. The enthalpy tables live in [`crate::bond_enthalpy`], unused here.
//!
//! The modules beside `sequential` are what it is built from: the atoms a
//! search reads and the change-set key ([`atoms`]), the bond inventory, one
//! relaxation with its per-term energies ([`relax`]), transfers, and the
//! input fingerprint a caller keys a stored result by.

pub mod atoms;
pub mod config;
pub mod fingerprint;
pub mod inventory;
pub mod relax;
pub mod sequential;
pub mod transfer;

pub use atoms::{HypothesisKey, change_key, changed_atoms, free_valence};
pub use config::{CHANGED_TAG, ChemisorptionError, Side};
pub use fingerprint::input_fingerprint;
pub use inventory::{BondInventory, BondKind, InventoryParseError, inventory_options};
pub use relax::{RelaxSettings, StrainTerms};
pub use transfer::{Transfer, TransferDirection, TransferRule, is_transferable_element};
