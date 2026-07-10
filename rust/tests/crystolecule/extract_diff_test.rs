//! Tests for `extract_diff` — the inverse of `apply_diff`: deriving a diff from
//! a (before, after) pair of id-correlated structures.
//!
//! Every directed test asserts both the emitted diff's exact shape and the
//! roundtrip invariant `apply_diff(before, extract_diff(before, after)) ≡ after`.
//! Property-style tests hammer the same invariant on randomized id-stable
//! mutations. See `doc/design_diff_outputs_for_atom_ops.md` §3 Phase 1.

use crate::structure_equivalence::assert_structures_equivalent;
use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_DOUBLE, BOND_SINGLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, BondReference, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::{
    apply_diff, compose_two_diffs, extract_diff,
};

/// `apply_diff` positional matching tolerance.
const TOL: f64 = 0.1;
/// Structural-equivalence tolerance. Extraction + application is exact
/// (no arithmetic on positions), so this only guards float noise.
const EQ: f64 = 1e-6;

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// The core Phase 1 invariant, for exact extraction (ε = 0):
///   apply_diff(before, extract_diff(before, after, 0)) ≡ after
fn assert_roundtrip(before: &AtomicStructure, after: &AtomicStructure) {
    let diff = extract_diff(before, after, 0.0);
    assert!(diff.is_diff(), "extracted structure must be a diff");
    let applied = apply_diff(before, &diff, TOL).result;
    assert_structures_equivalent(&applied, after, EQ);
}

/// Returns the single atom of a one-atom diff (panics otherwise).
fn only_atom(
    diff: &AtomicStructure,
) -> &rust_lib_flutter_cad::crystolecule::atomic_structure::Atom {
    assert_eq!(diff.get_num_of_atoms(), 1, "expected exactly one diff atom");
    diff.atoms_values().next().unwrap()
}

// ============================================================================
// Directed unit tests (§3 Phase 1, tests 1–13)
// ============================================================================

#[test]
fn test1_identical_structures_empty_diff() {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, v(0.0, 0.0, 0.0));
    let c2 = s.add_atom(6, v(1.54, 0.0, 0.0));
    s.add_bond(c1, c2, BOND_SINGLE);

    let diff = extract_diff(&s, &s, 0.0);
    assert!(diff.is_diff());
    assert_eq!(diff.get_num_of_atoms(), 0);
    assert_eq!(diff.get_num_of_bonds(), 0);

    assert_roundtrip(&s, &s);
}

#[test]
fn test2_single_atom_moved() {
    let mut before = AtomicStructure::new();
    let c1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let c2 = before.add_atom(6, v(1.54, 0.0, 0.0));
    before.add_bond(c1, c2, BOND_SINGLE);

    let mut after = before.clone();
    after.set_atom_position(c1, v(0.5, 0.0, 0.0));

    let diff = extract_diff(&before, &after, 0.0);
    // Only the moved atom; the untouched neighbour is absent.
    let atom = only_atom(&diff);
    assert_eq!(atom.atomic_number, 6);
    assert_eq!(atom.position, v(0.5, 0.0, 0.0));
    assert_eq!(
        diff.anchor_position(atom.id).copied(),
        Some(v(0.0, 0.0, 0.0))
    );
    // Bond order unchanged, one endpoint modified → no bond entry.
    assert_eq!(diff.get_num_of_bonds(), 0);

    assert_roundtrip(&before, &after);
}

#[test]
fn test3_pruning_boundary() {
    let mut before = AtomicStructure::new();
    before.add_atom(6, v(0.0, 0.0, 0.0));

    let mut after = before.clone();
    after.set_atom_position(1, v(0.05, 0.0, 0.0)); // moved 0.05

    // ε = 0: the move is captured.
    let d0 = extract_diff(&before, &after, 0.0);
    assert_eq!(d0.get_num_of_atoms(), 1);
    assert_roundtrip(&before, &after);

    // ε = 0.1 (> 0.05): the atom is treated as untouched → empty diff.
    let d1 = extract_diff(&before, &after, 0.1);
    assert_eq!(d1.get_num_of_atoms(), 0);
    // Pruned: applying reproduces `before`, not `after`.
    let applied = apply_diff(&before, &d1, TOL).result;
    assert_structures_equivalent(&applied, &before, EQ);

    // Exactly-at-ε is "no more than" → still pruned.
    let mut after_exact = before.clone();
    after_exact.set_atom_position(1, v(0.1, 0.0, 0.0));
    let d2 = extract_diff(&before, &after_exact, 0.1);
    assert_eq!(d2.get_num_of_atoms(), 0);
}

#[test]
fn test4_element_changed_in_place() {
    let mut before = AtomicStructure::new();
    before.add_atom(6, v(1.0, 2.0, 3.0));

    let mut after = before.clone();
    after.set_atomic_number(1, 7); // C → N

    let diff = extract_diff(&before, &after, 0.0);
    let atom = only_atom(&diff);
    assert_eq!(atom.atomic_number, 7);
    assert_eq!(atom.position, v(1.0, 2.0, 3.0));
    assert_eq!(
        diff.anchor_position(atom.id).copied(),
        Some(v(1.0, 2.0, 3.0))
    );

    assert_roundtrip(&before, &after);
}

#[test]
fn test5_durable_vs_transient_flags() {
    // Durable flag (frozen) changed → anchored atom.
    let mut before = AtomicStructure::new();
    before.add_atom(6, v(0.0, 0.0, 0.0));

    let mut after = before.clone();
    after.set_atom_frozen(1, true);

    let diff = extract_diff(&before, &after, 0.0);
    let atom = only_atom(&diff);
    assert!(
        atom.is_frozen(),
        "durable frozen bit must be carried into the diff"
    );
    assert_eq!(atom.position, v(0.0, 0.0, 0.0));
    assert_eq!(
        diff.anchor_position(atom.id).copied(),
        Some(v(0.0, 0.0, 0.0))
    );
    assert_roundtrip(&before, &after);

    // Transient flags (selected, display-ghost) changed → empty diff.
    let mut after_transient = before.clone();
    after_transient.set_atom_selected(1, true);
    after_transient.set_atom_ghost(1, true);

    let empty = extract_diff(&before, &after_transient, 0.0);
    assert_eq!(empty.get_num_of_atoms(), 0);
    // Roundtrip holds under `≡` (durable-flag equivalence ignores transient bits).
    assert_roundtrip(&before, &after_transient);
}

#[test]
fn test6_atom_deleted() {
    let mut before = AtomicStructure::new();
    let c1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let c2 = before.add_atom(6, v(1.54, 0.0, 0.0));
    before.add_bond(c1, c2, BOND_SINGLE);

    let mut after = before.clone();
    after.delete_atom(c1);

    let diff = extract_diff(&before, &after, 0.0);
    let marker = only_atom(&diff);
    assert!(marker.is_delete_marker());
    assert_eq!(marker.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert_eq!(marker.position, v(0.0, 0.0, 0.0));
    assert_eq!(
        diff.anchor_position(marker.id).copied(),
        Some(v(0.0, 0.0, 0.0))
    );
    // Bond to the deleted atom emits nothing.
    assert_eq!(diff.get_num_of_bonds(), 0);

    assert_roundtrip(&before, &after);
}

#[test]
fn test7_atom_added_with_bond_to_untouched() {
    let mut before = AtomicStructure::new();
    let c1 = before.add_atom(6, v(0.0, 0.0, 0.0));

    let mut after = before.clone();
    let h = after.add_atom(1, v(0.0, -1.09, 0.0));
    after.add_bond(c1, h, BOND_SINGLE);

    let diff = extract_diff(&before, &after, 0.0);
    // One pure addition (H, no anchor) + one UNCHANGED marker endpoint.
    assert_eq!(diff.get_num_of_atoms(), 2);
    assert_eq!(diff.get_num_of_bonds(), 1);

    let h_atom = diff
        .atoms_values()
        .find(|a| a.atomic_number == 1)
        .expect("added H present");
    assert!(
        !diff.has_anchor_position(h_atom.id),
        "pure addition has no anchor"
    );

    let marker = diff
        .atoms_values()
        .find(|a| a.is_unchanged_marker())
        .expect("UNCHANGED marker endpoint present");
    assert_eq!(marker.position, v(0.0, 0.0, 0.0));
    assert!(diff.has_bond_between(h_atom.id, marker.id));

    assert_roundtrip(&before, &after);
}

#[test]
fn test8_bond_deleted_between_untouched_atoms() {
    let mut before = AtomicStructure::new();
    let c1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let c2 = before.add_atom(6, v(1.54, 0.0, 0.0));
    before.add_bond(c1, c2, BOND_SINGLE);

    let mut after = before.clone();
    after.delete_bond(&BondReference {
        atom_id1: c1,
        atom_id2: c2,
    });

    let diff = extract_diff(&before, &after, 0.0);
    // Two UNCHANGED markers connected by a BOND_DELETED entry.
    assert_eq!(diff.get_num_of_atoms(), 2);
    assert!(diff.atoms_values().all(|a| a.is_unchanged_marker()));
    assert_eq!(diff.get_num_of_bonds(), 1);

    let m: Vec<_> = diff.atoms_values().collect();
    let order = diff
        .get_atom(m[0].id)
        .unwrap()
        .bonds
        .iter()
        .find(|b| b.other_atom_id() == m[1].id)
        .unwrap()
        .bond_order();
    assert_eq!(order, BOND_DELETED);

    assert_roundtrip(&before, &after);
}

#[test]
fn test9_bond_order_changed() {
    let mut before = AtomicStructure::new();
    let c1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let c2 = before.add_atom(6, v(1.54, 0.0, 0.0));
    before.add_bond(c1, c2, BOND_SINGLE);

    let mut after = before.clone();
    after.add_bond_checked(c1, c2, BOND_DOUBLE); // updates existing order

    let diff = extract_diff(&before, &after, 0.0);
    assert_eq!(diff.get_num_of_atoms(), 2); // two UNCHANGED marker endpoints
    assert!(diff.atoms_values().all(|a| a.is_unchanged_marker()));
    assert_eq!(diff.get_num_of_bonds(), 1);

    let m: Vec<_> = diff.atoms_values().collect();
    let order = diff
        .get_atom(m[0].id)
        .unwrap()
        .bonds
        .iter()
        .find(|b| b.other_atom_id() == m[1].id)
        .unwrap()
        .bond_order();
    assert_eq!(order, BOND_DOUBLE);

    assert_roundtrip(&before, &after);
}

#[test]
fn test10_bond_between_moved_atoms_order_unchanged() {
    let mut before = AtomicStructure::new();
    let c1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let c2 = before.add_atom(6, v(1.54, 0.0, 0.0));
    before.add_bond(c1, c2, BOND_SINGLE);

    // Move both endpoints rigidly; the bond order is unchanged.
    let mut after = before.clone();
    after.set_atom_position(c1, v(0.0, 0.0, 1.0));
    after.set_atom_position(c2, v(1.54, 0.0, 1.0));

    let diff = extract_diff(&before, &after, 0.0);
    // Two modified atoms, no bond entry (apply_diff step 3a re-adds the base bond).
    assert_eq!(diff.get_num_of_atoms(), 2);
    assert_eq!(diff.get_num_of_bonds(), 0);

    // Yet the roundtrip preserves the bond.
    let applied = apply_diff(&before, &diff, TOL).result;
    assert_eq!(applied.get_num_of_bonds(), 1);
    assert_structures_equivalent(&applied, &after, EQ);
}

#[test]
fn test11_mixed_everything_at_once() {
    let mut before = AtomicStructure::new();
    let a1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let a2 = before.add_atom(6, v(3.0, 0.0, 0.0));
    let a3 = before.add_atom(7, v(6.0, 0.0, 0.0));
    let a4 = before.add_atom(8, v(9.0, 0.0, 0.0));
    before.add_bond(a1, a2, BOND_SINGLE);
    before.add_bond(a2, a3, BOND_SINGLE);
    before.add_bond(a3, a4, BOND_DOUBLE);

    let mut after = before.clone();
    after.set_atom_position(a1, v(0.5, 0.2, 0.0)); // move
    after.set_atomic_number(a2, 14); // replace C → Si
    after.delete_atom(a3); // delete (drops a2-a3 and a3-a4)
    let a5 = after.add_atom(1, v(0.5, -1.09, 0.0)); // add H
    after.add_bond(a1, a5, BOND_SINGLE); // new bond
    after.add_bond_checked(a1, a2, BOND_DOUBLE); // change a1-a2 order

    let diff = extract_diff(&before, &after, 0.0);
    // a1 modified, a2 modified, a3 delete marker, a5 addition; a4 untouched/absent.
    assert_eq!(diff.get_num_of_atoms(), 4);
    // (a1,a2) order override + (a1,a5) new bond.
    assert_eq!(diff.get_num_of_bonds(), 2);

    assert_roundtrip(&before, &after);
}

#[test]
fn test12_frozen_atoms_absent_among_moved() {
    let mut before = AtomicStructure::new();
    let f1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let m1 = before.add_atom(6, v(3.0, 0.0, 0.0));
    let f2 = before.add_atom(6, v(6.0, 0.0, 0.0));
    let m2 = before.add_atom(6, v(9.0, 0.0, 0.0));
    before.set_atom_frozen(f1, true);
    before.set_atom_frozen(f2, true);
    before.add_bond(f1, m1, BOND_SINGLE);
    before.add_bond(m1, f2, BOND_SINGLE);
    before.add_bond(f2, m2, BOND_SINGLE);

    // Frozen atoms held exactly fixed; only the two non-frozen atoms move.
    let mut after = before.clone();
    after.set_atom_position(m1, v(3.0, 0.5, 0.0));
    after.set_atom_position(m2, v(9.0, 0.5, 0.0));

    let diff = extract_diff(&before, &after, 0.0);
    // Exactly the two moved (non-frozen) atoms; no frozen atom, no marker.
    assert_eq!(diff.get_num_of_atoms(), 2);
    assert!(diff.atoms_values().all(|a| !a.is_unchanged_marker()));
    // Frozen positions must not appear in the diff.
    assert!(
        diff.atoms_values()
            .all(|a| a.position != v(0.0, 0.0, 0.0) && a.position != v(6.0, 0.0, 0.0))
    );

    assert_roundtrip(&before, &after);
}

#[test]
fn test13_determinism() {
    // A mixed before/after that exercises additions, deletions, and bond markers.
    let mut before = AtomicStructure::new();
    let a1 = before.add_atom(6, v(0.0, 0.0, 0.0));
    let a2 = before.add_atom(7, v(3.0, 0.0, 0.0));
    let a3 = before.add_atom(8, v(6.0, 0.0, 0.0));
    before.add_bond(a1, a2, BOND_SINGLE);
    before.add_bond(a2, a3, BOND_DOUBLE);

    let mut after = before.clone();
    after.set_atom_position(a1, v(0.3, 0.1, 0.0));
    after.delete_atom(a3);
    let a4 = after.add_atom(1, v(0.3, -1.09, 0.0));
    after.add_bond(a1, a4, BOND_SINGLE);
    after.add_bond_checked(a1, a2, BOND_DOUBLE);

    let d1 = extract_diff(&before, &after, 0.0);
    let d2 = extract_diff(&before, &after, 0.0);
    assert_eq!(serialize_diff(&d1), serialize_diff(&d2));
}

// ============================================================================
// Property-style randomized tests (fixed-seed, ~100 iterations)
// ============================================================================

#[test]
fn property_roundtrip_random_mutations() {
    let mut rng = Xorshift::new(0x9E37_79B9_7F4A_7C15);
    for _ in 0..100 {
        let base = gen_base(&mut rng);
        let after = mutate(&mut rng, &base);
        let diff = extract_diff(&base, &after, 0.0);
        let applied = apply_diff(&base, &diff, TOL).result;
        assert_structures_equivalent(&applied, &after, EQ);
    }
}

#[test]
fn property_composition_interplay() {
    let mut rng = Xorshift::new(0xD1B5_4A32_D192_ED03);
    for _ in 0..100 {
        let base = gen_base(&mut rng);
        let mid = mutate(&mut rng, &base);
        let final_s = mutate(&mut rng, &mid);

        let d1 = extract_diff(&base, &mid, 0.0);
        let d2 = extract_diff(&mid, &final_s, 0.0);
        let composed = compose_two_diffs(&d1, &d2, TOL).composed;
        let applied = apply_diff(&base, &composed, TOL).result;

        assert_structures_equivalent(&applied, &final_s, EQ);
    }
}

// ============================================================================
// Test-local helpers
// ============================================================================

/// Fully serializes a diff (all atoms + anchors + bonds, in id/canonical order)
/// for byte-exact determinism comparison. Unlike `to_detailed_string`, this
/// truncates nothing.
fn serialize_diff(diff: &AtomicStructure) -> String {
    let mut ids: Vec<u32> = diff.atom_ids().copied().collect();
    ids.sort_unstable();

    let mut lines = Vec::new();
    for id in &ids {
        let a = diff.get_atom(*id).unwrap();
        let anchor = diff.anchor_position(*id).copied();
        lines.push(format!(
            "A id={} Z={} pos={:?} flags={} anchor={:?}",
            id, a.atomic_number, a.position, a.flags, anchor
        ));
    }
    let mut bonds: Vec<(u32, u32, u8)> = Vec::new();
    for id in &ids {
        let a = diff.get_atom(*id).unwrap();
        for b in &a.bonds {
            let other = b.other_atom_id();
            if *id < other {
                bonds.push((*id, other, b.bond_order()));
            }
        }
    }
    bonds.sort_unstable();
    for (x, y, o) in bonds {
        lines.push(format!("B {}-{} order={}", x, y, o));
    }
    lines.join("\n")
}

/// Tiny deterministic xorshift64 PRNG (avoids a `rand` dependency).
struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        Xorshift(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform integer in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    /// True with probability `num/den`.
    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }
    /// Uniform f64 in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Uniform f64 in `[-mag, mag)`.
    fn delta(&mut self, mag: f64) -> f64 {
        (self.unit() * 2.0 - 1.0) * mag
    }
}

const ELEMENTS: [i16; 5] = [1, 6, 7, 8, 14];

/// Builds a random well-separated bonded structure (atoms 3.0 Å apart along x).
fn gen_base(rng: &mut Xorshift) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let n = 3 + rng.below(6) as usize; // 3..=8
    let mut ids = Vec::new();
    for i in 0..n {
        let z = ELEMENTS[rng.below(ELEMENTS.len() as u64) as usize];
        let id = s.add_atom(z, v(i as f64 * 3.0, 0.0, 0.0));
        if rng.chance(1, 4) {
            s.set_atom_frozen(id, true);
        }
        ids.push(id);
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.chance(1, 4) {
                let order = 1 + rng.below(3) as u8; // single/double/triple
                s.add_bond_checked(ids[i], ids[j], order);
            }
        }
    }
    s
}

/// Applies a random id-stable mutation script to a clone of `before`.
/// Deletions vacate slots without renumbering and additions append, so surviving
/// atoms keep their ids (the precondition `extract_diff` relies on).
fn mutate(rng: &mut Xorshift, before: &AtomicStructure) -> AtomicStructure {
    let mut after = before.clone();

    let mut orig_ids: Vec<u32> = before.atom_ids().copied().collect();
    orig_ids.sort_unstable();

    // Deletions.
    let mut alive: Vec<u32> = Vec::new();
    for &id in &orig_ids {
        if rng.chance(1, 5) {
            after.delete_atom(id);
        } else {
            alive.push(id);
        }
    }

    // Per-survivor edits: move, replace element, flip durable flags.
    for &id in &alive {
        if rng.chance(1, 3) {
            let p = before.get_atom(id).unwrap().position;
            after.set_atom_position(id, p + v(rng.delta(0.8), rng.delta(0.8), rng.delta(0.8)));
        }
        if rng.chance(1, 4) {
            let z = ELEMENTS[rng.below(ELEMENTS.len() as u64) as usize];
            after.set_atomic_number(id, z);
        }
        if rng.chance(1, 4) {
            let frozen = before.get_atom(id).unwrap().is_frozen();
            after.set_atom_frozen(id, !frozen);
        }
        if rng.chance(1, 5) {
            after.set_atom_hydrogen_passivation(id, true);
        }
    }

    // Bond edits among survivors.
    if alive.len() >= 2 {
        for _ in 0..rng.below(4) {
            let i = rng.below(alive.len() as u64) as usize;
            let mut j = rng.below(alive.len() as u64) as usize;
            if i == j {
                j = (j + 1) % alive.len();
            }
            let (a, b) = (alive[i], alive[j]);
            if after.has_bond_between(a, b) && rng.chance(1, 2) {
                after.delete_bond(&BondReference {
                    atom_id1: a,
                    atom_id2: b,
                });
            } else {
                let order = 1 + rng.below(3) as u8;
                after.add_bond_checked(a, b, order);
            }
        }
    }

    // Additions at fresh far positions. The offset grows with the (monotonically
    // increasing) slot count, so repeated `mutate` rounds never collide.
    let base_x = 1000.0 + after.get_num_of_atoms_including_deleted() as f64 * 3.0;
    for k in 0..rng.below(3) {
        let z = ELEMENTS[rng.below(4) as usize];
        let id = after.add_atom(z, v(base_x + k as f64 * 3.0, 0.0, 0.0));
        if !alive.is_empty() && rng.chance(1, 2) {
            let t = alive[rng.below(alive.len() as u64) as usize];
            let order = 1 + rng.below(3) as u8;
            after.add_bond_checked(id, t, order);
        }
    }

    after
}
