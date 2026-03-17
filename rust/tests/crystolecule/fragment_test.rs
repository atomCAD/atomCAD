use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::fragment::compute_moving_fragment;
use std::collections::HashSet;

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a linear chain: A(1) — B(2) — C(3) — D(4)
fn create_linear_chain() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let d = s.add_atom(6, DVec3::new(4.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    s.add_bond(b, c, 1);
    s.add_bond(c, d, 1);
    s
}

/// Creates a 6-membered ring: 1—2—3—4—5—6—1
fn create_six_membered_ring() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let ids: Vec<u32> = (0..6)
        .map(|i| {
            let angle = std::f64::consts::TAU * (i as f64) / 6.0;
            s.add_atom(6, DVec3::new(angle.cos(), angle.sin(), 0.0))
        })
        .collect();
    for i in 0..6 {
        s.add_bond(ids[i], ids[(i + 1) % 6], 1);
    }
    s
}

/// Creates a branched tree:
///
/// ```text
///         H(3)
///         |
/// H(5)—C(1)—C(2)—H(4)
///         |
///         H(6)
/// ```
fn create_branched_structure() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let h3 = s.add_atom(1, DVec3::new(0.0, 1.0, 0.0));
    let h4 = s.add_atom(1, DVec3::new(2.5, 0.0, 0.0));
    let h5 = s.add_atom(1, DVec3::new(-1.0, 0.0, 0.0));
    let h6 = s.add_atom(1, DVec3::new(0.0, -1.0, 0.0));
    s.add_bond(c1, c2, 1);
    s.add_bond(c1, h3, 1);
    s.add_bond(c2, h4, 1);
    s.add_bond(c1, h5, 1);
    s.add_bond(c1, h6, 1);
    s
}

fn set_of(ids: &[u32]) -> HashSet<u32> {
    ids.iter().copied().collect()
}

// ============================================================================
// Linear Chain Tests
// ============================================================================

#[test]
fn linear_chain_move_first_ref_second() {
    // A(1)—B(2)—C(3)—D(4), moving A ref B → fragment {A}
    let s = create_linear_chain();
    let fragment = compute_moving_fragment(&s, 1, 2);
    assert_eq!(fragment, set_of(&[1]));
}

#[test]
fn linear_chain_move_last_ref_third() {
    // Moving D(4) ref C(3) → fragment {D}
    let s = create_linear_chain();
    let fragment = compute_moving_fragment(&s, 4, 3);
    assert_eq!(fragment, set_of(&[4]));
}

#[test]
fn linear_chain_move_second_ref_third() {
    // Moving B(2) ref C(3) → fragment {B, A} (A is closer to B than to C)
    let s = create_linear_chain();
    let fragment = compute_moving_fragment(&s, 2, 3);
    assert_eq!(fragment, set_of(&[1, 2]));
}

#[test]
fn linear_chain_move_third_ref_second() {
    // Moving C(3) ref B(2) → fragment {C, D}
    let s = create_linear_chain();
    let fragment = compute_moving_fragment(&s, 3, 2);
    assert_eq!(fragment, set_of(&[3, 4]));
}

// ============================================================================
// Ring Tests
// ============================================================================

#[test]
fn ring_opposite_atoms() {
    // 6-membered ring: 1—2—3—4—5—6—1
    // Moving 1 ref 4 → fragment {1, 2, 6} (nearest 3 on 1's side)
    // Distances from 1: 1→0, 2→1, 3→2, 4→3, 5→2, 6→1
    // Distances from 4: 4→0, 3→1, 5→1, 2→2, 6→2, 1→3
    // Atom 1: d_m=0 < d_f=3 → moves
    // Atom 2: d_m=1 < d_f=2 → moves
    // Atom 3: d_m=2 > d_f=1 → stays (tie-break: d_m=2 vs d_f=1, not a tie)
    // Atom 4: d_m=3 > d_f=0 → stays
    // Atom 5: d_m=2 > d_f=1 → stays
    // Atom 6: d_m=1 < d_f=2 → moves
    let s = create_six_membered_ring();
    let fragment = compute_moving_fragment(&s, 1, 4);
    assert_eq!(fragment, set_of(&[1, 2, 6]));
}

#[test]
fn ring_adjacent_atoms() {
    // Moving 1 ref 2:
    // Distances from 1: 1→0, 2→1, 3→2, 4→3, 5→2, 6→1
    // Distances from 2: 2→0, 1→1, 3→1, 4→2, 5→3, 6→2
    // Atom 1: 0 < 1 → moves
    // Atom 3: 2 > 1 → stays
    // Atom 4: 3 > 2 → stays
    // Atom 5: 2 < 3 → moves
    // Atom 6: 1 < 2 → moves
    let s = create_six_membered_ring();
    let fragment = compute_moving_fragment(&s, 1, 2);
    assert_eq!(fragment, set_of(&[1, 5, 6]));
}

// ============================================================================
// Branched Structure Tests
// ============================================================================

#[test]
fn branched_move_c2_ref_c1() {
    // Moving C2(2) ref C1(1) → fragment {C2, H4}
    let s = create_branched_structure();
    let fragment = compute_moving_fragment(&s, 2, 1);
    assert_eq!(fragment, set_of(&[2, 4]));
}

#[test]
fn branched_move_h3_ref_c1() {
    // Moving H3(3) ref C1(1) → fragment {H3} (leaf atom)
    let s = create_branched_structure();
    let fragment = compute_moving_fragment(&s, 3, 1);
    assert_eq!(fragment, set_of(&[3]));
}

#[test]
fn branched_move_c1_ref_c2() {
    // Moving C1(1) ref C2(2) → fragment {C1, H3, H5, H6}
    let s = create_branched_structure();
    let fragment = compute_moving_fragment(&s, 1, 2);
    assert_eq!(fragment, set_of(&[1, 3, 5, 6]));
}

// ============================================================================
// Disconnected Component Tests
// ============================================================================

#[test]
fn disconnected_atoms_stay_fixed() {
    // Two disconnected pairs: (1—2) and (3—4)
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = s.add_atom(6, DVec3::new(10.0, 0.0, 0.0));
    let d = s.add_atom(6, DVec3::new(11.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    s.add_bond(c, d, 1);

    // Moving 1 ref 2: only atom 1 moves. Atoms 3 and 4 are disconnected → stay fixed.
    let fragment = compute_moving_fragment(&s, 1, 2);
    assert_eq!(fragment, set_of(&[1]));
}

#[test]
fn disconnected_fragment_with_multiple_atoms() {
    // Chain (1—2—3) and disconnected pair (4—5)
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let d = s.add_atom(6, DVec3::new(10.0, 0.0, 0.0));
    let e = s.add_atom(6, DVec3::new(11.5, 0.0, 0.0));
    s.add_bond(a, b, 1);
    s.add_bond(b, c, 1);
    s.add_bond(d, e, 1);

    // Moving 1 ref 3 in the chain: {1} is closer to 1, {3} closer to 3, {2} is tied → stays.
    // Disconnected {4,5} unreachable from both → stay fixed.
    let fragment = compute_moving_fragment(&s, 1, 3);
    assert_eq!(fragment, set_of(&[1]));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn single_bond_two_atoms() {
    // Just two atoms bonded: 1—2
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    s.add_bond(a, b, 1);

    let fragment = compute_moving_fragment(&s, 1, 2);
    assert_eq!(fragment, set_of(&[1]));

    let fragment = compute_moving_fragment(&s, 2, 1);
    assert_eq!(fragment, set_of(&[2]));
}

#[test]
fn same_atom_returns_empty() {
    let s = create_linear_chain();
    let fragment = compute_moving_fragment(&s, 1, 1);
    assert!(fragment.is_empty());
}

#[test]
fn unbonded_atoms_moving_ref() {
    // Two unbonded atoms: moving 1 ref 2. Neither can reach the other via bonds.
    // dist_m from 1: {1→0}. dist_f from 2: {2→0}.
    // Atom 1: dm=0, df=MAX → moves. Atom 2: dm=MAX, df=0 → stays.
    let mut s = AtomicStructure::new();
    s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(5.0, 0.0, 0.0));

    let fragment = compute_moving_fragment(&s, 1, 2);
    assert_eq!(fragment, set_of(&[1]));
}

// ============================================================================
// Symmetry / Consistency Tests
// ============================================================================

#[test]
fn fragments_partition_connected_component() {
    // In a linear chain A—B—C—D, moving A ref D and moving D ref A
    // should give complementary fragments (no overlap, union = all).
    let s = create_linear_chain();
    let frag_a = compute_moving_fragment(&s, 1, 4);
    let frag_d = compute_moving_fragment(&s, 4, 1);

    // No overlap
    assert!(frag_a.is_disjoint(&frag_d));

    // Together with tied atoms, they cover all atoms.
    // Tied atoms: B(2) is equidistant (dist from 1 = 1, dist from 4 = 2 → not tied)
    // Actually: distances from 1: {1:0, 2:1, 3:2, 4:3}
    //           distances from 4: {4:0, 3:1, 2:2, 1:3}
    // Atom 2: dm=1 < df=2 → in frag_a
    // Atom 3: dm=2 > df=1 → in frag_d
    // So frag_a = {1, 2}, frag_d = {3, 4} → union = {1,2,3,4}
    let union: HashSet<u32> = frag_a.union(&frag_d).copied().collect();
    assert_eq!(union, set_of(&[1, 2, 3, 4]));
}

#[test]
fn ring_with_pendant() {
    // Ring 1—2—3—4—5—6—1 with a pendant: 3—7
    // Moving 7 ref 3 → fragment {7}
    // Moving 3 ref 7 → fragment includes 3 and ring atoms on 3's side away from 7
    let mut s = create_six_membered_ring();
    let pendant = s.add_atom(1, DVec3::new(0.0, 0.0, 1.0));
    s.add_bond(3, pendant, 1); // pendant = 7

    let fragment = compute_moving_fragment(&s, 7, 3);
    assert_eq!(fragment, set_of(&[7]));

    // Moving 3 ref 7: everything closer to 3 than to 7.
    // dist from 3: {3:0, 2:1, 4:1, 1:2, 5:2, 6:3, 7:1}
    // dist from 7: {7:0, 3:1, 2:2, 4:2, 1:3, 5:3, 6:4}
    // Atom 3: 0 < 1 → moves
    // Atom 2: 1 < 2 → moves
    // Atom 4: 1 < 2 → moves
    // Atom 1: 2 < 3 → moves
    // Atom 5: 2 < 3 → moves
    // Atom 6: 3 < 4 → moves
    let fragment = compute_moving_fragment(&s, 3, 7);
    assert_eq!(fragment, set_of(&[1, 2, 3, 4, 5, 6]));
}

#[test]
fn moving_atom_always_in_fragment() {
    let s = create_linear_chain();
    for moving in 1..=4 {
        for reference in 1..=4 {
            if moving == reference {
                continue;
            }
            let fragment = compute_moving_fragment(&s, moving, reference);
            assert!(
                fragment.contains(&moving),
                "Fragment for moving={moving} ref={reference} should contain the moving atom"
            );
        }
    }
}

#[test]
fn reference_atom_never_in_fragment() {
    let s = create_linear_chain();
    for moving in 1..=4 {
        for reference in 1..=4 {
            if moving == reference {
                continue;
            }
            let fragment = compute_moving_fragment(&s, moving, reference);
            assert!(
                !fragment.contains(&reference),
                "Fragment for moving={moving} ref={reference} should not contain the reference atom"
            );
        }
    }
}
