//! The shared position-matching primitive
//! (`atomic_structure/matching.rs`): `nearest_unclaimed_atom`, `nearest_atom`
//! and `bond_order_between`.
//!
//! Four subsystems match atoms by position — `apply_diff`, mechanosynth's
//! `apply_step` and `compare_structures`, and the placement engine — and each
//! used to carry its own copy of the loop. These tests pin the behaviour the
//! copies have to agree on; the callers' own suites cover what they do with the
//! answer.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use glam::DVec3;
use rustc_hash::FxHashSet;

const H: i16 = 1;
const C: i16 = 6;
const N: i16 = 7;

/// Nothing is claimed.
fn free(_atom_id: u32) -> bool {
    false
}

#[test]
fn the_nearest_atom_within_tolerance_is_returned() {
    let mut s = AtomicStructure::new();
    let near = s.add_atom(C, DVec3::new(0.1, 0.0, 0.0));
    s.add_atom(C, DVec3::new(0.25, 0.0, 0.0));

    let found = s
        .nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, free)
        .expect("an atom is within tolerance");
    assert_eq!(found.atom_id, near);
    assert!((found.distance - 0.1).abs() < 1e-12);
}

#[test]
fn an_already_claimed_atom_is_skipped_in_favour_of_the_next_nearest() {
    let mut s = AtomicStructure::new();
    let near = s.add_atom(C, DVec3::new(0.1, 0.0, 0.0));
    let far = s.add_atom(C, DVec3::new(0.25, 0.0, 0.0));

    let mut claimed: FxHashSet<u32> = FxHashSet::default();
    claimed.insert(near);

    let found = s
        .nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, |id| claimed.contains(&id))
        .expect("the second atom is still free");
    assert_eq!(found.atom_id, far);
}

#[test]
fn the_element_filter_rejects_a_nearer_atom_of_the_wrong_element() {
    let mut s = AtomicStructure::new();
    s.add_atom(H, DVec3::new(0.05, 0.0, 0.0));
    let carbon = s.add_atom(C, DVec3::new(0.2, 0.0, 0.0));

    let found = s
        .nearest_unclaimed_atom(DVec3::ZERO, 0.3, Some(C), free)
        .expect("a carbon is within tolerance");
    assert_eq!(found.atom_id, carbon);

    // With no filter the hydrogen wins, which is what makes the filter's effect
    // visible rather than incidental.
    let unfiltered = s
        .nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, free)
        .expect("something is within tolerance");
    assert_ne!(unfiltered.atom_id, carbon);
}

#[test]
fn two_atoms_at_equal_distance_resolve_by_the_lower_id() {
    // Grid iteration order is an implementation detail, so a symmetric site
    // would otherwise make "which one matched" move with an unrelated edit.
    let mut s = AtomicStructure::new();
    let first = s.add_atom(C, DVec3::new(0.2, 0.0, 0.0));
    let second = s.add_atom(C, DVec3::new(-0.2, 0.0, 0.0));
    assert!(first < second);

    let found = s
        .nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, free)
        .expect("both are within tolerance");
    assert_eq!(found.atom_id, first);

    let mut claimed: FxHashSet<u32> = FxHashSet::default();
    claimed.insert(first);
    let next = s
        .nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, |id| claimed.contains(&id))
        .expect("the other one is still free");
    assert_eq!(next.atom_id, second);
}

#[test]
fn nothing_within_tolerance_is_none_and_the_nearest_is_still_describable() {
    let mut s = AtomicStructure::new();
    let lone = s.add_atom(N, DVec3::new(4.59, 0.0, 0.0));

    assert!(
        s.nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, free)
            .is_none()
    );

    // The diagnostic every "not found within tolerance" message ends with:
    // whatever *is* there, whether or not it passes the filters.
    let nearest = s.nearest_atom(DVec3::ZERO).expect("the structure has one");
    assert_eq!(nearest.atom_id, lone);
    assert!((nearest.distance - 4.59).abs() < 1e-12);

    assert!(AtomicStructure::new().nearest_atom(DVec3::ZERO).is_none());
}

#[test]
fn the_tolerance_boundary_is_inclusive() {
    let mut s = AtomicStructure::new();
    s.add_atom(C, DVec3::new(0.3, 0.0, 0.0));

    assert!(
        s.nearest_unclaimed_atom(DVec3::ZERO, 0.3, None, free)
            .is_some(),
        "an atom exactly at the tolerance matches"
    );
    assert!(
        s.nearest_unclaimed_atom(DVec3::ZERO, 0.29, None, free)
            .is_none()
    );
}

#[test]
fn bond_order_between_is_symmetric_and_none_for_unbonded_atoms() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(C, DVec3::ZERO);
    let b = s.add_atom(C, DVec3::new(1.4, 0.0, 0.0));
    let lone = s.add_atom(C, DVec3::new(10.0, 0.0, 0.0));
    s.add_bond(a, b, 2);

    assert_eq!(s.bond_order_between(a, b), Some(2));
    assert_eq!(s.bond_order_between(b, a), Some(2));
    assert_eq!(s.bond_order_between(a, lone), None);
    assert_eq!(s.bond_order_between(a, 9999), None);
}
