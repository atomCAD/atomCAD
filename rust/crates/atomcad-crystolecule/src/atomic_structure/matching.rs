//! Position matching against a structure: "which atom is *here*".
//!
//! Four subsystems ask the same question — `apply_diff`'s
//! [`match_diff_atoms`](crate::atomic_structure_diff), mechanosynth's
//! `apply_step` and `compare_structures`, and the placement engine — and each
//! had grown its own copy of the loop: walk the spatial grid around a target,
//! skip the atoms an earlier match already claimed, optionally require an
//! element, keep the nearest. The copies drifted in the two places that matter,
//! tie-breaking and what is reported when nothing is found, so the loop lives
//! here once.
//!
//! Two rules the copies did not all have:
//!
//! - **Ties resolve by the lower atom id.** Two atoms at the same distance are
//!   a real possibility on a symmetric site, and grid iteration order is an
//!   implementation detail; without a tie-break, "which one matched" could move
//!   with an unrelated edit.
//! - **A failure is describable.** [`AtomicStructure::nearest_atom`] is the
//!   companion whole-structure scan every "not found within tolerance"
//!   diagnostic ends with. It ignores both the claim set and the element filter
//!   on purpose: the message is about what *is* there.

use super::AtomicStructure;
use glam::f64::DVec3;

/// One atom found by a position match, with the distance that found it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtomMatch {
    pub atom_id: u32,
    /// Distance from the queried position, Ångström.
    pub distance: f64,
}

impl AtomicStructure {
    /// The nearest atom to `target` within `tolerance` that `is_claimed` does
    /// not reject and, when `element` is `Some`, carries that atomic number.
    ///
    /// `is_claimed` is a predicate rather than a set so that callers holding a
    /// map, a set or nothing at all can share one implementation; pass
    /// `|_| false` when no atom is spoken for.
    ///
    /// Ties break by the lower atom id, so the answer does not depend on grid
    /// iteration order.
    pub fn nearest_unclaimed_atom<F>(
        &self,
        target: DVec3,
        tolerance: f64,
        element: Option<i16>,
        is_claimed: F,
    ) -> Option<AtomMatch>
    where
        F: Fn(u32) -> bool,
    {
        let mut best: Option<AtomMatch> = None;
        for atom_id in self.get_atoms_in_radius(&target, tolerance) {
            if is_claimed(atom_id) {
                continue;
            }
            let Some(atom) = self.get_atom(atom_id) else {
                continue;
            };
            if let Some(z) = element
                && atom.atomic_number != z
            {
                continue;
            }
            let candidate = AtomMatch {
                atom_id,
                distance: atom.position.distance(target),
            };
            let better = match best {
                None => true,
                Some(current) => match candidate.distance.total_cmp(&current.distance) {
                    std::cmp::Ordering::Less => true,
                    std::cmp::Ordering::Equal => candidate.atom_id < current.atom_id,
                    std::cmp::Ordering::Greater => false,
                },
            };
            if better {
                best = Some(candidate);
            }
        }
        best
    }

    /// The nearest atom to `target` in the whole structure, whatever its
    /// element and whoever has claimed it — the diagnostic that accompanies a
    /// failed match.
    ///
    /// A full scan rather than a widening grid search: it runs once, on a path
    /// that is already failing, and a grid walk that has to widen until it
    /// finds something is more code for no benefit there.
    pub fn nearest_atom(&self, target: DVec3) -> Option<AtomMatch> {
        self.atoms_values()
            .map(|atom| AtomMatch {
                atom_id: atom.id,
                distance: atom.position.distance(target),
            })
            .min_by(|a, b| {
                a.distance
                    .total_cmp(&b.distance)
                    .then(a.atom_id.cmp(&b.atom_id))
            })
    }

    /// The order of the bond between two atoms, or `None` when they are not
    /// bonded (or either id is absent). Order-insensitive in its arguments.
    pub fn bond_order_between(&self, atom_id1: u32, atom_id2: u32) -> Option<u8> {
        self.get_atom(atom_id1)?
            .bonds
            .iter()
            .find(|bond| bond.other_atom_id() == atom_id2)
            .map(|bond| bond.bond_order())
    }
}
