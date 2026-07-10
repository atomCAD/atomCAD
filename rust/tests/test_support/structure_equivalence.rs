//! Shared test support: structural equivalence (`≡`) of two `AtomicStructure`s.
//!
//! The `crystolecule` and `structure_designer` integration-test crates are
//! separate binaries and cannot import from each other, so this helper is
//! `#[path]`-included as a module from both crate roots
//! (`tests/crystolecule.rs` and `tests/structure_designer.rs`).
//!
//! `≡` is the executable spec behind the diff-extraction roundtrip invariant in
//! `doc/design_diff_outputs_for_atom_ops.md`: two structures are equivalent when
//! they hold equal multisets of (position, element, durable flags) atoms and
//! equal bond multisets — matched *positionally*, because `apply_diff` re-assigns
//! ids so id equality is meaningless.
#![allow(dead_code)]

use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::DURABLE_FLAGS_MASK;

/// Asserts that two structures are structurally equivalent (`≡`).
///
/// Atoms are matched greedily by nearest position within `tolerance`; each match
/// must agree on element and durable flags (transient bits — selection and
/// display-ghost — are ignored). Bonds are then compared as a multiset mapped
/// through the position matching, including bond order.
pub fn assert_structures_equivalent(a: &AtomicStructure, b: &AtomicStructure, tolerance: f64) {
    assert_eq!(
        a.get_num_of_atoms(),
        b.get_num_of_atoms(),
        "atom count mismatch: a={}, b={}",
        a.get_num_of_atoms(),
        b.get_num_of_atoms()
    );

    let tol_sq = tolerance * tolerance;

    // Build position-based bijection from a → b.
    let mut b_claimed: Vec<bool> = vec![false; b.get_num_of_atoms()];
    let b_atoms: Vec<_> = b.atoms_values().collect();
    let mut a_to_b: Vec<(u32, u32)> = Vec::new(); // (a_id, b_id)

    for a_atom in a.atoms_values() {
        let mut best_idx = None;
        let mut best_dist = f64::MAX;
        for (i, b_atom) in b_atoms.iter().enumerate() {
            if b_claimed[i] {
                continue;
            }
            let dist = a_atom.position.distance_squared(b_atom.position);
            if dist < best_dist {
                best_dist = dist;
                best_idx = Some(i);
            }
        }
        let idx = best_idx.expect("no matching b atom found");
        assert!(
            best_dist <= tol_sq,
            "atom at {:?} (Z={}) has no match within tolerance in b (nearest dist={})",
            a_atom.position,
            a_atom.atomic_number,
            best_dist.sqrt()
        );
        b_claimed[idx] = true;
        let b_atom = b_atoms[idx];
        assert_eq!(
            a_atom.atomic_number, b_atom.atomic_number,
            "element mismatch at position {:?}: a={}, b={}",
            a_atom.position, a_atom.atomic_number, b_atom.atomic_number
        );
        // Compare durable flags only (ignore transient selection / display-ghost bits).
        assert_eq!(
            a_atom.flags & DURABLE_FLAGS_MASK,
            b_atom.flags & DURABLE_FLAGS_MASK,
            "durable flags mismatch at position {:?}: a={:#b}, b={:#b}",
            a_atom.position,
            a_atom.flags & DURABLE_FLAGS_MASK,
            b_atom.flags & DURABLE_FLAGS_MASK,
        );
        a_to_b.push((a_atom.id, b_atom.id));
    }

    // Build ID mapping.
    let a_id_to_b_id: std::collections::HashMap<u32, u32> = a_to_b.iter().cloned().collect();

    // Compare bonds.
    assert_eq!(
        a.get_num_of_bonds(),
        b.get_num_of_bonds(),
        "bond count mismatch: a={}, b={}",
        a.get_num_of_bonds(),
        b.get_num_of_bonds()
    );

    for a_atom in a.atoms_values() {
        for bond in &a_atom.bonds {
            let other_a_id = bond.other_atom_id();
            if a_atom.id >= other_a_id {
                continue; // only check each bond once
            }
            let b_id1 = a_id_to_b_id[&a_atom.id];
            let b_id2 = a_id_to_b_id[&other_a_id];
            assert!(
                b.has_bond_between(b_id1, b_id2),
                "bond between positions {:?} and {:?} exists in a but not in b",
                a_atom.position,
                a.get_atom(other_a_id).unwrap().position
            );
            let b_order = b
                .get_atom(b_id1)
                .unwrap()
                .bonds
                .iter()
                .find(|bb| bb.other_atom_id() == b_id2)
                .unwrap()
                .bond_order();
            assert_eq!(
                bond.bond_order(),
                b_order,
                "bond order mismatch between {:?} and {:?}",
                a_atom.position,
                a.get_atom(other_a_id).unwrap().position
            );
        }
    }
}
