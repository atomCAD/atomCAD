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
use crate::guided_placement::{
    BondLengthMode, BondMode, GuidedPlacementMode, compute_guided_placement,
};
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
    /// terminator bond length, in the acceptor's open valence direction — the
    /// slot guided placement offers, where `passivate` would put a
    /// terminator. Of several open slots, the one nearest the side `X` comes
    /// from (the line from `A` towards `X`'s old position); with no slot to
    /// read (a bare acceptor) that line itself.
    ///
    /// The slot reads the acceptor's bonds in `s`, so a caller seats `X`
    /// after the acceptor's other new bonds are made.
    pub fn seat(&self, s: &AtomicStructure) -> DVec3 {
        let pos = |id: u32| s.get_atom(id).map_or(DVec3::ZERO, |a| a.position);
        let (x, a, d) = (pos(self.moved), pos(self.acceptor), pos(self.donor));
        let acceptor_z = s.get_atom(self.acceptor).map_or(0, |a| a.atomic_number);
        let towards = [x - a, d - a, DVec3::Z]
            .into_iter()
            .find(|v| v.length_squared() > 1e-12)
            .unwrap_or(DVec3::Z)
            .normalize();
        let direction =
            open_slot_direction(s, self.acceptor, self.element, towards).unwrap_or(towards);
        a + direction * terminator_bond_length(acceptor_z, self.element)
    }
}

/// Whether atoms of `z` may move by a transfer: H and the halogens.
pub fn is_transferable_element(z: i16) -> bool {
    matches!(z, 1 | 9 | 17 | 35 | 53)
}

/// The acceptor's open valence direction nearest `towards` (unit), from
/// guided placement: one of its fixed slots, or, when the slots are free to
/// turn about its single bond, the point of that cone nearest `towards`.
/// `None` when there is nothing to read: no open slot, or no bond at all.
fn open_slot_direction(
    s: &AtomicStructure,
    acceptor: u32,
    element: i16,
    towards: DVec3,
) -> Option<DVec3> {
    let a = s.get_atom(acceptor)?.position;
    let placement = compute_guided_placement(
        s,
        acceptor,
        element,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );
    match placement.mode {
        GuidedPlacementMode::FixedDots { guide_dots } => guide_dots
            .iter()
            .map(|g| (g.position - a).normalize_or_zero())
            .filter(|v| v.length_squared() > 0.5)
            .max_by(|p, q| p.dot(towards).total_cmp(&q.dot(towards))),
        GuidedPlacementMode::FreeRing {
            ring_center,
            ring_normal,
            ring_radius,
            ..
        } => {
            let n = ring_normal.normalize_or_zero();
            let off = towards - n * towards.dot(n);
            let out = if off.length_squared() > 1e-12 {
                off.normalize()
            } else {
                n.any_orthonormal_vector()
            };
            Some((ring_center + out * ring_radius - a).normalize_or_zero())
                .filter(|v| v.length_squared() > 0.5)
        }
        GuidedPlacementMode::FreeSphere { .. } => None,
    }
}

/// Applies transfers to `s`: breaks each `D–X`, re-seats `X` on its acceptor
/// and bonds it there. One at a time, in a fixed order (by acceptor, then
/// donor), so two atoms moving to one acceptor take two different slots and
/// the result does not depend on the order the transfers are listed in.
/// Each `X` is seated from its own unedited position.
pub fn apply_transfers(s: &mut AtomicStructure, transfers: &[Transfer]) {
    let mut ordered: Vec<&Transfer> = transfers.iter().collect();
    ordered.sort_by_key(|t| (t.acceptor, t.donor, t.moved));
    for t in ordered {
        let seat = t.seat(s);
        s.delete_bond(&crate::atomic_structure::BondReference {
            atom_id1: t.donor,
            atom_id2: t.moved,
        });
        s.set_atom_position(t.moved, seat);
        s.add_bond_checked(t.moved, t.acceptor, BOND_SINGLE);
    }
}
