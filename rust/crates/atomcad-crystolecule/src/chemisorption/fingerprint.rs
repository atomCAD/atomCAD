//! `input_fingerprint`: a hash of everything one search depends on, so a
//! stored result can be checked against the inputs it is about to be shown
//! for.
//!
//! A search takes seconds to minutes, so a caller (the `chemisorb` node) runs
//! it once, on request, and keeps the report. The fingerprint is what makes
//! keeping it safe: a report is shown only when the current inputs hash to the
//! value it was computed from, never for a pose the user has since moved.
//!
//! What is hashed, per input: every atom's id, element, position bits, bonds
//! (partner and order), frozen flag, hybridization override, and whether it
//! carries that side's reactive tag. What is **not** hashed: selection, the
//! display-only flags, and tags other than the reactive one — none of them
//! changes a search, and selecting an atom must not make a result stale. Of
//! the settings, every field of [`ChemisorptionSearch`] (`max_transfers` only
//! while a transfer rule is enabled, the one case it is read).
//!
//! The value is compared within one process and never persisted, so the
//! standard library's hasher is enough.

use super::config::ChemisorptionSearch;
use crate::atomic_structure::AtomicStructure;
use crate::simulation::uff::VdwMode;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_structure(s: &AtomicStructure, tag: &Option<String>, h: &mut DefaultHasher) {
    let tag = tag.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let mut count = 0usize;
    for atom in s.atoms_values() {
        count += 1;
        atom.id.hash(h);
        atom.atomic_number.hash(h);
        for c in atom.position.to_array() {
            c.to_bits().hash(h);
        }
        atom.is_frozen().hash(h);
        atom.hybridization_override().hash(h);
        tag.is_some_and(|t| s.atom_has_tag(atom.id, t)).hash(h);
        atom.bonds.len().hash(h);
        for bond in &atom.bonds {
            bond.other_atom_id().hash(h);
            bond.bond_order().hash(h);
        }
    }
    count.hash(h);
}

/// The fingerprint of one search: both inputs and every setting.
pub fn input_fingerprint(
    adsorbate: &AtomicStructure,
    substrate: &AtomicStructure,
    config: &ChemisorptionSearch,
) -> u64 {
    let mut h = DefaultHasher::new();
    hash_structure(adsorbate, &config.adsorbate_tag, &mut h);
    hash_structure(substrate, &config.substrate_tag, &mut h);

    let ChemisorptionSearch {
        adsorbate_tag,
        substrate_tag,
        reach,
        pair_tolerance,
        max_formed_bonds,
        transfers,
        max_transfers,
        budget,
        max_iterations,
        gradient_rms_tolerance,
        vdw_mode,
    } = config;
    adsorbate_tag.hash(&mut h);
    substrate_tag.hash(&mut h);
    reach.to_bits().hash(&mut h);
    pair_tolerance.to_bits().hash(&mut h);
    max_formed_bonds.hash(&mut h);
    // `max_transfers` is read only when a transfer rule is enabled, so without
    // one it must not make a result stale.
    transfers.hash(&mut h);
    if !transfers.is_empty() {
        max_transfers.hash(&mut h);
    }
    budget.hash(&mut h);
    max_iterations.hash(&mut h);
    gradient_rms_tolerance.to_bits().hash(&mut h);
    match vdw_mode {
        VdwMode::AllPairs => 0u8.hash(&mut h),
        VdwMode::Cutoff(r) => {
            1u8.hash(&mut h);
            r.to_bits().hash(&mut h);
        }
    }
    h.finish()
}
