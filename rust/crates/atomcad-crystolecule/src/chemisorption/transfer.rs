//! Transfers: a monovalent atom `X` moves from its only neighbour, the
//! **donor** `D`, to an **acceptor** `A` on the substrate. Bond-graph effect:
//! `D–X` broken, `X–A` formed, `X` moved.
//!
//! This is how an OH leg bonds: its H goes to a site, freeing the O. A
//! transfer is never enumerated; the sequential search fixes it by one rule
//! when the donating leg forms (`sequential::Setup::acceptor`): the site
//! nearest the foot's own site with valence left, never that site itself,
//! within `reach`, site to site. Only `to_substrate` is supported; a
//! `to_adsorbate` record (a radical foot abstracting surface H) is rejected
//! by the config.

use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::BOND_SINGLE;
use crate::hydrogen_passivation::terminator_bond_length;
use glam::DVec3;

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
