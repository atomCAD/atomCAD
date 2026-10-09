//! The atoms a chemisorption search reads, and the key a change set is
//! deduplicated by: an atom's free valence, the reactive atoms of one side
//! (by tag), and [`change_key`].

use super::config::{ChemisorptionError, Side};
use super::transfer::Transfer;
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DELETED, BOND_DOUBLE, BOND_QUADRUPLE, BOND_SINGLE, BOND_TRIPLE,
};
use crate::guided_placement::{covalent_max_neighbors, detect_hybridization};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;

/// The normalized bond set a hypothesis is deduplicated and tie-broken by:
/// the formed pairs `(min, max)`, sorted, and the transfers as
/// `(donor, element, acceptor)`, sorted — the moving atom by its donor and
/// element rather than its id, so equivalent atoms on one donor count once.
pub type HypothesisKey = (Vec<(u32, u32)>, Vec<(u32, i16, u32)>);

/// The key of a bond-change set, shared by hypotheses and candidates.
pub fn change_key(formed: &[(u32, u32)], transfers: &[Transfer]) -> HypothesisKey {
    let pairs: BTreeSet<(u32, u32)> = formed.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
    let mut moves: Vec<(u32, i16, u32)> = transfers.iter().map(Transfer::key).collect();
    moves.sort_unstable();
    (pairs.into_iter().collect(), moves)
}

/// The atoms whose bonds a change set touches, sorted: both ends of every
/// formed bond, and every transfer's donor, moving atom and acceptor.
pub fn changed_atoms(formed: &[(u32, u32)], transfers: &[Transfer]) -> Vec<u32> {
    let set: BTreeSet<u32> = formed
        .iter()
        .flat_map(|&(a, b)| [a, b])
        .chain(
            transfers
                .iter()
                .flat_map(|t| [t.donor, t.moved, t.acceptor]),
        )
        .collect();
    set.into_iter().collect()
}

/// The largest total bond order an element carries, for the common main-group
/// elements; `None` elsewhere.
fn tabulated_valence(z: i16) -> Option<usize> {
    match z {
        1 | 9 | 17 | 35 | 53 => Some(1), // H, halogens
        8 | 16 => Some(2),               // O, S
        7 | 15 => Some(3),               // N, P
        6 | 14 | 32 => Some(4),          // C, Si, Ge
        _ => None,
    }
}

/// Bond order in half units, so an aromatic bond counts 1.5.
fn half_order(order: u8) -> usize {
    match order {
        BOND_DELETED => 0,
        BOND_SINGLE => 2,
        BOND_DOUBLE => 4,
        BOND_TRIPLE => 6,
        BOND_QUADRUPLE => 8,
        BOND_AROMATIC => 3,
        _ => 2, // dative, metallic: one bond
    }
}

/// How many more single bonds atom `id` can take: its valence minus the
/// total order of the bonds it has.
///
/// The valence is a fixed per-element table rather than one derived from the
/// atom's current hybridization, because a radical carbon with three single
/// bonds types as sp2 and would read as saturated. Outside the table it falls back to the geometric
/// neighbour limit the passivation path uses.
pub fn free_valence(structure: &AtomicStructure, id: u32) -> usize {
    let Some(atom) = structure.get_atom(id) else {
        return 0;
    };
    let z = structure.effective_atomic_number(atom);
    if z <= 0 {
        return 0;
    }
    let bonds: Vec<_> = atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .collect();
    match tabulated_valence(z) {
        Some(valence) => {
            let used: usize = bonds.iter().map(|b| half_order(b.bond_order())).sum();
            (2 * valence).saturating_sub(used) / 2
        }
        None => {
            let hyb = detect_hybridization(structure, id, None);
            covalent_max_neighbors(z, hyb).saturating_sub(bonds.len())
        }
    }
}

/// Reactive atoms of one side, as combined ids, sorted.
pub(crate) fn reactive_atoms(
    input: &AtomicStructure,
    ids: &FxHashMap<u32, u32>,
    tag: &Option<String>,
    side: Side,
) -> Result<Vec<u32>, ChemisorptionError> {
    let mut out: Vec<u32> = match tag.as_deref().map(str::trim) {
        None | Some("") => ids.values().copied().collect(),
        Some(t) => {
            let tagged = input.atoms_with_tag(t);
            if tagged.is_empty() {
                return Err(ChemisorptionError::UnknownTag {
                    side,
                    tag: t.to_string(),
                });
            }
            tagged.iter().map(|id| ids[id]).collect()
        }
    };
    out.sort_unstable();
    Ok(out)
}
