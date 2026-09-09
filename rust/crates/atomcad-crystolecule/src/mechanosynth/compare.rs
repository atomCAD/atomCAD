//! Position-tolerant comparison of two [`AtomicStructure`]s.
//!
//! "Verify by replay" needs a structure comparison that reports *what* differs
//! rather than asserting equality: the generator compares its replayed script
//! against the target it holds in memory, and a bare `false` would leave it
//! nothing to print. Hence a list of [`Mismatch`]es — empty means equal up to
//! the tolerance.
//!
//! Tags, per-atom flags and atom ids are ignored. Ids in particular are
//! meaningless across a replay, since every added atom takes the next free
//! slot.

use crate::atomic_constants::ATOM_INFO;
use crate::atomic_structure::AtomicStructure;
use glam::DVec3;
use rustc_hash::FxHashMap;
use std::fmt;

/// One way in which two structures differ. Positions are reported from
/// whichever side owns the thing that is missing on the other.
#[derive(Debug, Clone, PartialEq)]
pub enum Mismatch {
    /// An atom of `a` with no partner in `b` within the tolerance.
    UnmatchedInB { position: DVec3, element: i16 },
    /// An atom of `b` that no atom of `a` claimed.
    UnmatchedInA { position: DVec3, element: i16 },
    /// A matched pair whose elements differ.
    ElementDiffers {
        position: DVec3,
        a_element: i16,
        b_element: i16,
    },
    /// A bond between two matched atoms that only `a` has.
    BondOnlyInA { position1: DVec3, position2: DVec3 },
    /// A bond between two matched atoms that only `b` has.
    BondOnlyInB { position1: DVec3, position2: DVec3 },
    /// A bond both have, with different orders.
    BondOrderDiffers {
        position1: DVec3,
        position2: DVec3,
        a_order: u8,
        b_order: u8,
    },
}

fn symbol(atomic_number: i16) -> String {
    ATOM_INFO
        .get(&(atomic_number as i32))
        .map(|info| info.symbol.clone())
        .unwrap_or_else(|| format!("Z={atomic_number}"))
}

fn at(p: DVec3) -> String {
    format!("({:.3}, {:.3}, {:.3})", p.x, p.y, p.z)
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mismatch::UnmatchedInB { position, element } => write!(
                f,
                "{} at {} is missing from b",
                symbol(*element),
                at(*position)
            ),
            Mismatch::UnmatchedInA { position, element } => write!(
                f,
                "{} at {} is missing from a",
                symbol(*element),
                at(*position)
            ),
            Mismatch::ElementDiffers {
                position,
                a_element,
                b_element,
            } => write!(
                f,
                "element differs at {}: a has {}, b has {}",
                at(*position),
                symbol(*a_element),
                symbol(*b_element)
            ),
            Mismatch::BondOnlyInA {
                position1,
                position2,
            } => write!(
                f,
                "bond {}–{} is missing from b",
                at(*position1),
                at(*position2)
            ),
            Mismatch::BondOnlyInB {
                position1,
                position2,
            } => write!(
                f,
                "bond {}–{} is missing from a",
                at(*position1),
                at(*position2)
            ),
            Mismatch::BondOrderDiffers {
                position1,
                position2,
                a_order,
                b_order,
            } => write!(
                f,
                "bond order differs on {}–{}: a has {}, b has {}",
                at(*position1),
                at(*position2),
                a_order,
                b_order
            ),
        }
    }
}

/// Compares two structures up to `tolerance` (Å) on atom positions.
///
/// Every atom of `a` is matched to the nearest unclaimed atom of `b` within
/// `tolerance`; the matching is therefore injective. The returned list is empty
/// exactly when the two are equal up to the tolerance. Each difference is
/// reported once.
pub fn compare_structures(
    a: &AtomicStructure,
    b: &AtomicStructure,
    tolerance: f64,
) -> Vec<Mismatch> {
    let mut mismatches = Vec::new();

    let mut a_to_b: FxHashMap<u32, u32> = FxHashMap::default();
    let mut b_to_a: FxHashMap<u32, u32> = FxHashMap::default();

    for a_atom in a.atoms_values() {
        let mut best: Option<(u32, f64)> = None;
        for candidate_id in b.get_atoms_in_radius(&a_atom.position, tolerance) {
            if b_to_a.contains_key(&candidate_id) {
                continue;
            }
            let Some(b_atom) = b.get_atom(candidate_id) else {
                continue;
            };
            let distance = a_atom.position.distance(b_atom.position);
            if best.is_none_or(|(_, best_distance)| distance < best_distance) {
                best = Some((candidate_id, distance));
            }
        }
        match best {
            Some((b_id, _)) => {
                a_to_b.insert(a_atom.id, b_id);
                b_to_a.insert(b_id, a_atom.id);
                let b_atom = b.get_atom(b_id).expect("just matched");
                if a_atom.atomic_number != b_atom.atomic_number {
                    mismatches.push(Mismatch::ElementDiffers {
                        position: a_atom.position,
                        a_element: a_atom.atomic_number,
                        b_element: b_atom.atomic_number,
                    });
                }
            }
            None => mismatches.push(Mismatch::UnmatchedInB {
                position: a_atom.position,
                element: a_atom.atomic_number,
            }),
        }
    }

    for b_atom in b.atoms_values() {
        if !b_to_a.contains_key(&b_atom.id) {
            mismatches.push(Mismatch::UnmatchedInA {
                position: b_atom.position,
                element: b_atom.atomic_number,
            });
        }
    }

    // Bonds are only comparable where both endpoints matched; a bond hanging off
    // an unmatched atom is already accounted for by that atom's report.
    for a_atom in a.atoms_values() {
        for bond in &a_atom.bonds {
            let other_id = bond.other_atom_id();
            if a_atom.id >= other_id {
                continue; // visit each bond once
            }
            let (Some(&b1), Some(&b2)) = (a_to_b.get(&a_atom.id), a_to_b.get(&other_id)) else {
                continue;
            };
            let position1 = a_atom.position;
            let position2 = a.get_atom(other_id).expect("bond endpoint").position;
            match bond_order_between(b, b1, b2) {
                None => mismatches.push(Mismatch::BondOnlyInA {
                    position1,
                    position2,
                }),
                Some(b_order) if b_order != bond.bond_order() => {
                    mismatches.push(Mismatch::BondOrderDiffers {
                        position1,
                        position2,
                        a_order: bond.bond_order(),
                        b_order,
                    })
                }
                Some(_) => {}
            }
        }
    }

    for b_atom in b.atoms_values() {
        for bond in &b_atom.bonds {
            let other_id = bond.other_atom_id();
            if b_atom.id >= other_id {
                continue;
            }
            let (Some(&a1), Some(&a2)) = (b_to_a.get(&b_atom.id), b_to_a.get(&other_id)) else {
                continue;
            };
            if bond_order_between(a, a1, a2).is_none() {
                mismatches.push(Mismatch::BondOnlyInB {
                    position1: b_atom.position,
                    position2: b.get_atom(other_id).expect("bond endpoint").position,
                });
            }
        }
    }

    mismatches
}

fn bond_order_between(structure: &AtomicStructure, id1: u32, id2: u32) -> Option<u8> {
    structure
        .get_atom(id1)?
        .bonds
        .iter()
        .find(|bond| bond.other_atom_id() == id2)
        .map(|bond| bond.bond_order())
}

/// A one-line-per-mismatch rendering, for a failure message or a generator's
/// report.
pub fn describe_mismatches(mismatches: &[Mismatch]) -> String {
    mismatches
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
