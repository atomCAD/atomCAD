//! Transfers: a monovalent atom `X` moves from its only neighbour, the
//! **donor** `D`, to an **acceptor** `A` on the other side (adsorbate ↔
//! substrate). Bond-graph effect: `D–X` broken, `X–A` formed, `X` moved.
//!
//! This is how an OH leg bonds (its H goes to a neighbouring site, freeing the
//! O) and how a radical foot abstracts H from the surface. The rules
//! (§6.1 of the design):
//!
//! - `X` is an atom of a monovalent element (H or a halogen) with exactly one
//!   bond, a single bond, matching a rule's element. Groups never transfer.
//! - `D` is `X`'s only neighbour and must be a reactive atom of its side.
//!   Tags select the donor, never `X`, so nobody has to tag hydrogens.
//! - `A` is a reactive atom on the other side with an available valence —
//!   checked on the whole hypothesis, since another transfer can free it.
//! - Reach is measured from `X` to `A`: the distance the atom travels. Pair
//!   tolerance does not apply.
//! - `D`, `X` and `A` are all unfrozen.
//!
//! Moves within one side (H hopping along the surface) are not transfers.

use super::config::Side;
use super::enumerate::free_valence;
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::BOND_SINGLE;
use crate::hydrogen_passivation::terminator_bond_length;
use glam::DVec3;
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;

/// Which way a transfer goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransferDirection {
    /// Adsorbate donor, substrate acceptor: an OH leg giving its H to a site.
    ToSubstrate,
    /// Substrate donor, adsorbate acceptor: a radical foot abstracting H.
    ToAdsorbate,
}

impl TransferDirection {
    /// The text-format and record spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            TransferDirection::ToSubstrate => "to_substrate",
            TransferDirection::ToAdsorbate => "to_adsorbate",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "to_substrate" => Some(TransferDirection::ToSubstrate),
            "to_adsorbate" => Some(TransferDirection::ToAdsorbate),
            _ => None,
        }
    }

    /// The side the moving atom leaves.
    pub fn donor_side(&self) -> Side {
        match self {
            TransferDirection::ToSubstrate => Side::Adsorbate,
            TransferDirection::ToAdsorbate => Side::Substrate,
        }
    }
}

/// One enabled transfer kind: atoms of `element` may move in `direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferRule {
    pub element: i16,
    pub direction: TransferDirection,
}

/// One concrete transfer, in the combined structure's atom ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Transfer {
    pub donor: u32,
    /// The atom that moves (`X`).
    pub moved: u32,
    pub acceptor: u32,
    /// `X`'s element.
    pub element: i16,
}

impl Transfer {
    /// What deduplication compares: the moving atom by its donor and element,
    /// not by id, so moving either H of an SiH₂ (or of an H₂O) to the same
    /// acceptor is one transfer (R8).
    pub fn key(&self) -> (u32, i16, u32) {
        (self.donor, self.element, self.acceptor)
    }

    /// Where `X` is placed before relaxation: on its acceptor at the
    /// terminator bond length, on the line from `A` towards `X`'s old
    /// position, so it arrives from the side it came from.
    pub fn seat(&self, s: &AtomicStructure) -> DVec3 {
        let pos = |id: u32| s.get_atom(id).map_or(DVec3::ZERO, |a| a.position);
        let (x, a, d) = (pos(self.moved), pos(self.acceptor), pos(self.donor));
        let acceptor_z = s.get_atom(self.acceptor).map_or(0, |a| a.atomic_number);
        let direction = [x - a, d - a, DVec3::Z]
            .into_iter()
            .find(|v| v.length_squared() > 1e-12)
            .unwrap_or(DVec3::Z)
            .normalize();
        a + direction * terminator_bond_length(acceptor_z, self.element)
    }
}

/// Whether atoms of `z` may move by a transfer: H and the halogens.
pub fn is_transferable_element(z: i16) -> bool {
    matches!(z, 1 | 9 | 17 | 35 | 53)
}

/// The one side's input ids, as combined ids, with the side they are on.
pub struct SideAtoms<'a> {
    pub ids: &'a FxHashMap<u32, u32>,
    /// Reactive atoms of this side (combined ids, sorted).
    pub reactive: &'a [u32],
}

/// Every `(D, X, A)` triple the rules allow (§6.1), in a deterministic order:
/// by rule, then by `X`, then by `A`. Relaxes nothing.
pub fn candidate_transfers(
    combined: &AtomicStructure,
    adsorbate: &SideAtoms,
    substrate: &SideAtoms,
    rules: &[TransferRule],
    reach: f64,
) -> Vec<Transfer> {
    let rules: BTreeSet<TransferRule> = rules.iter().copied().collect();
    let sorted = |ids: &FxHashMap<u32, u32>| {
        let mut v: Vec<u32> = ids.values().copied().collect();
        v.sort_unstable();
        v
    };
    let (ads_atoms, sub_atoms) = (sorted(adsorbate.ids), sorted(substrate.ids));
    let unfrozen = |id: u32| combined.get_atom(id).is_some_and(|a| !a.is_frozen());

    // Pass 1: the (rule, D, X) pairs, and how many atoms each donor may give
    // away — an acceptor that is itself a donor can be freed by its own
    // transfer, so it counts as having an available valence.
    let mut donations: Vec<(TransferRule, u32, u32)> = Vec::new();
    let mut donor_capacity: FxHashMap<u32, usize> = FxHashMap::default();
    for rule in &rules {
        let (atoms, side) = match rule.direction.donor_side() {
            Side::Adsorbate => (&ads_atoms, adsorbate),
            Side::Substrate => (&sub_atoms, substrate),
        };
        for &x in atoms {
            let Some(atom) = combined.get_atom(x) else {
                continue;
            };
            if atom.atomic_number != rule.element || !is_transferable_element(atom.atomic_number) {
                continue;
            }
            let mut bonds = atom.bonds.iter().filter(|b| !b.is_delete_marker());
            let (Some(bond), None) = (bonds.next(), bonds.next()) else {
                continue;
            };
            if bond.bond_order() != BOND_SINGLE {
                continue;
            }
            let d = bond.other_atom_id();
            if side.reactive.binary_search(&d).is_err() || !unfrozen(x) || !unfrozen(d) {
                continue;
            }
            donations.push((*rule, d, x));
            *donor_capacity.entry(d).or_insert(0) += 1;
        }
    }

    // Pass 2: the acceptors of each.
    let mut out = Vec::new();
    for (rule, d, x) in donations {
        let acceptors = match rule.direction {
            TransferDirection::ToSubstrate => substrate.reactive,
            TransferDirection::ToAdsorbate => adsorbate.reactive,
        };
        let x_pos = combined.get_atom(x).expect("donation atom").position;
        for &a in acceptors {
            let Some(atom) = combined.get_atom(a) else {
                continue;
            };
            if atom.is_frozen() || atom.position.distance(x_pos) > reach {
                continue;
            }
            let available =
                free_valence(combined, a) + donor_capacity.get(&a).copied().unwrap_or(0);
            if available == 0 {
                continue;
            }
            out.push(Transfer {
                donor: d,
                moved: x,
                acceptor: a,
                element: rule.element,
            });
        }
    }
    out
}

/// Applies transfers to `s`: breaks each `D–X`, re-seats `X` on its acceptor
/// and bonds it there. Seats are computed from the unedited positions.
pub fn apply_transfers(s: &mut AtomicStructure, transfers: &[Transfer]) {
    let seats: Vec<DVec3> = transfers.iter().map(|t| t.seat(s)).collect();
    for (t, seat) in transfers.iter().zip(seats) {
        s.delete_bond(&crate::atomic_structure::BondReference {
            atom_id1: t.donor,
            atom_id2: t.moved,
        });
        s.set_atom_position(t.moved, seat);
        s.add_bond_checked(t.moved, t.acceptor, BOND_SINGLE);
    }
}
