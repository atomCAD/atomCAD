use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER, UNCHANGED_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::{
    apply_diff, compose_diffs, compose_two_diffs,
};

const TOL: f64 = 0.1;

// ============================================================================
// Test Helpers
// ============================================================================

/// Compares two AtomicStructure results for semantic equality using position-based matching.
fn assert_structures_equal(a: &AtomicStructure, b: &AtomicStructure, tolerance: f64) {
    assert_eq!(
        a.get_num_of_atoms(),
        b.get_num_of_atoms(),
        "atom count mismatch: a={}, b={}",
        a.get_num_of_atoms(),
        b.get_num_of_atoms()
    );

    let tol_sq = tolerance * tolerance;

    // Build position-based bijection from a → b
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
        // Check flags (excluding selection bit 0)
        assert_eq!(
            a_atom.flags & !0x1,
            b_atom.flags & !0x1,
            "flags mismatch at position {:?}",
            a_atom.position,
        );
        a_to_b.push((a_atom.id, b_atom.id));
    }

    // Build ID mapping
    let a_id_to_b_id: std::collections::HashMap<u32, u32> = a_to_b.iter().cloned().collect();

    // Compare bonds
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
            // Check bond order
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

/// Verifies the core correctness invariant:
///   apply_diff(apply_diff(base, diff1), diff2) == apply_diff(base, compose(diffs))
fn assert_compose_equivalence(base: &AtomicStructure, diffs: &[&AtomicStructure], tolerance: f64) {
    // Sequential application
    let mut sequential = base.clone();
    for diff in diffs {
        sequential = apply_diff(&sequential, diff, tolerance).result;
    }

    // Composed application
    let composed = compose_diffs(diffs, tolerance).unwrap();
    let composed_result = apply_diff(base, &composed.composed, tolerance).result;

    assert_structures_equal(&sequential, &composed_result, tolerance);
}

// ============================================================================
// 5.3 Identity and Trivial Cases
// ============================================================================

#[test]
fn compose_empty_diff_is_identity() {
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // pure addition C
    let diff2 = AtomicStructure::new_diff(); // empty

    let base = AtomicStructure::new();
    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_single_diff() {
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));

    let result = compose_diffs(&[&diff1], TOL).unwrap();
    assert_eq!(result.composed.get_num_of_atoms(), 1);
}

#[test]
fn compose_two_empty_diffs() {
    let diff1 = AtomicStructure::new_diff();
    let diff2 = AtomicStructure::new_diff();

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.composed.get_num_of_atoms(), 0);

    let base = AtomicStructure::new();
    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.4 Pure Addition Tests
// ============================================================================

#[test]
fn compose_two_additions_no_overlap() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0)); // N

    let mut diff2 = AtomicStructure::new_diff();
    diff2.add_atom(8, DVec3::new(0.0, 2.0, 0.0)); // O

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.composed.get_num_of_atoms(), 2);
}

#[test]
fn compose_addition_with_bond() {
    let base = AtomicStructure::new(); // empty

    // diff1: C at (0,0,0), C at (1.54,0,0), bond
    let mut diff1 = AtomicStructure::new_diff();
    let c1 = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = diff1.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    diff1.add_bond(c1, c2, BOND_SINGLE);

    // diff2: H bonded to C1 (via unchanged marker)
    let mut diff2 = AtomicStructure::new_diff();
    let unch = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let h = diff2.add_atom(1, DVec3::new(0.0, -1.09, 0.0));
    diff2.add_bond(unch, h, BOND_SINGLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.5 Pure Deletion Tests
// ============================================================================

#[test]
fn compose_two_deletions_different_atoms() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    base.add_atom(8, DVec3::new(0.0, 2.0, 0.0));

    // diff1: delete C at origin
    let mut diff1 = AtomicStructure::new_diff();
    let d1 = diff1.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(d1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: delete O at (0,2,0) — base passthrough
    let mut diff2 = AtomicStructure::new_diff();
    let d2 = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 2.0, 0.0));
    diff2.set_anchor_position(d2, DVec3::new(0.0, 2.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_delete_same_atom_twice() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    let d1 = diff1.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(d1, DVec3::new(0.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let d2 = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d2, DVec3::new(0.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.6 Cancellation Tests
// ============================================================================

#[test]
fn compose_add_then_delete_cancels() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0)); // pure addition N

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(2.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.stats.cancellations, 1);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_add_then_delete_with_bond_cleanup() {
    let base = AtomicStructure::new();

    let mut diff1 = AtomicStructure::new_diff();
    let c1 = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = diff1.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    diff1.add_bond(c1, c2, BOND_SINGLE);

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.composed.get_num_of_atoms(), 1); // only C2 left
    assert_eq!(result.composed.get_num_of_bonds(), 0); // bond dropped

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_add_then_delete_partial_cancellation() {
    let base = AtomicStructure::new();

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    diff1.add_atom(8, DVec3::new(4.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(2.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.composed.get_num_of_atoms(), 2); // C and O remain

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.7 Chained Modification Tests
// ============================================================================

#[test]
fn compose_move_then_move() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(7, DVec3::new(5.0, 0.0, 0.0));

    // diff1: move C from (0,0,0) to (1,0,0)
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: move C from (1,0,0) to (2,0,0)
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    // Composed should have: C at (2,0,0) anchor=(0,0,0)
    let composed_atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(composed_atom.position, DVec3::new(2.0, 0.0, 0.0));
    assert_eq!(
        result.composed.anchor_position(composed_atom.id).copied(),
        Some(DVec3::new(0.0, 0.0, 0.0))
    );

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_move_then_delete() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(1.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_replace_then_replace() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // diff1: C→N
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(7, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: N→O
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(8, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(0.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.atomic_number, 8); // should be O
}

#[test]
fn compose_move_then_replace() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(7, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_add_then_move() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0)); // pure addition

    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(7, DVec3::new(3.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(2.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.position, DVec3::new(3.0, 0.0, 0.0));
    // Should be pure addition (no anchor)
    assert!(!result.composed.has_anchor_position(atom.id));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_add_then_replace() {
    let base = AtomicStructure::new();

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(7, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.atomic_number, 7); // N
    assert!(!result.composed.has_anchor_position(atom.id)); // still pure addition

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_move_preserves_bonds_to_other_atoms() {
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    base.add_bond(c1, c2, BOND_SINGLE);

    // diff1: move C1 slightly
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(0.5, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: move C1 again
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(0.5, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.8 Unchanged Marker Tests
// ============================================================================

#[test]
fn compose_unchanged_then_modify() {
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    base.add_bond(c1, c2, BOND_SINGLE);

    // diff1: unchanged at origin, add N at (3,0,0), bond unchanged→N
    let mut diff1 = AtomicStructure::new_diff();
    let unch = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let n = diff1.add_atom(7, DVec3::new(3.0, 0.0, 0.0));
    diff1.add_bond(unch, n, BOND_SINGLE);

    // diff2: replace C at origin with Si
    let mut diff2 = AtomicStructure::new_diff();
    let si = diff2.add_atom(14, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(si, DVec3::new(0.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_unchanged_then_delete() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));

    // diff1: unchanged at origin, add H, bond unchanged→H
    let mut diff1 = AtomicStructure::new_diff();
    let unch = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let h = diff1.add_atom(1, DVec3::new(0.0, -1.09, 0.0));
    diff1.add_bond(unch, h, BOND_SINGLE);

    // diff2: delete at origin
    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_unchanged_then_unchanged_with_bonds() {
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    let c3 = base.add_atom(6, DVec3::new(3.08, 0.0, 0.0));
    base.add_bond(c1, c2, BOND_SINGLE);
    base.add_bond(c2, c3, BOND_SINGLE);

    // diff1: delete C1-C2 bond
    let mut diff1 = AtomicStructure::new_diff();
    let u1 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff1.add_bond(u1, u2, BOND_DELETED);

    // diff2: add bond from C1 to new N
    let mut diff2 = AtomicStructure::new_diff();
    let u3 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let n = diff2.add_atom(7, DVec3::new(0.0, 2.0, 0.0));
    diff2.add_bond(u3, n, BOND_SINGLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.9 Bond Composition Tests
// ============================================================================

#[test]
fn compose_add_bond_then_delete_bond() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    // no bond between them

    // diff1: add single bond
    let mut diff1 = AtomicStructure::new_diff();
    let u1 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff1.add_bond(u1, u2, BOND_SINGLE);

    // diff2: delete that bond
    let mut diff2 = AtomicStructure::new_diff();
    let u3 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u4 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff2.add_bond(u3, u4, BOND_DELETED);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_add_bond_then_change_order() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));

    // diff1: add single bond
    let mut diff1 = AtomicStructure::new_diff();
    let u1 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff1.add_bond(u1, u2, BOND_SINGLE);

    // diff2: change to double
    let mut diff2 = AtomicStructure::new_diff();
    let u3 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u4 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff2.add_bond(u3, u4, BOND_DOUBLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_delete_base_bond_passthrough() {
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    base.add_bond(c1, c2, BOND_SINGLE);

    // diff1: delete bond
    let mut diff1 = AtomicStructure::new_diff();
    let u1 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff1.add_bond(u1, u2, BOND_DELETED);

    // diff2: empty
    let diff2 = AtomicStructure::new_diff();

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_bond_between_added_atoms() {
    let base = AtomicStructure::new();

    // diff1: two C atoms, no bond
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff1.add_atom(6, DVec3::new(1.54, 0.0, 0.0));

    // diff2: bond them
    let mut diff2 = AtomicStructure::new_diff();
    let u1 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.54, 0.0, 0.0));
    diff2.add_bond(u1, u2, BOND_SINGLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_bond_with_endpoint_cancelled() {
    let base = AtomicStructure::new();

    let mut diff1 = AtomicStructure::new_diff();
    let c = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let n = diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    diff1.add_bond(c, n, BOND_SINGLE);

    // diff2: delete C
    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.composed.get_num_of_atoms(), 1); // N remains
    assert_eq!(result.composed.get_num_of_bonds(), 0); // bond dropped

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_bond_between_mixed_origins() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(5.0, 0.0, 0.0));

    // diff1: add N
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(0.0, 0.0, 0.0));

    // diff2: bond N to base C via unchanged markers
    let mut diff2 = AtomicStructure::new_diff();
    let u1 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(5.0, 0.0, 0.0));
    diff2.add_bond(u1, u2, BOND_SINGLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.10 Metadata (Flags) Tests
// ============================================================================

#[test]
fn compose_metadata_last_writer_wins() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // diff1: move + set frozen
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_atom_frozen(a1, true);

    // diff2: in-place, clear frozen
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_atom_frozen(a2, false);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_metadata_diff1_only() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // diff1: set frozen
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_atom_frozen(a1, true);

    // diff2: doesn't touch this atom
    let diff2 = AtomicStructure::new_diff();

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_metadata_on_pure_addition() {
    let base = AtomicStructure::new();

    // diff1: add with Sp2
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_atom_hybridization_override(a1, 2); // Sp2

    // diff2: change to Sp3
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_atom_hybridization_override(a2, 1); // Sp3

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

// ============================================================================
// 5.11 Multi-Diff (3+) Composition Tests
// ============================================================================

#[test]
fn compose_three_diffs_sequential() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(1.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    diff2.add_atom(8, DVec3::new(2.0, 0.0, 0.0));

    let mut diff3 = AtomicStructure::new_diff();
    let d = diff3.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(3.0, 0.0, 0.0));
    diff3.set_anchor_position(d, DVec3::new(3.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], TOL);
}

#[test]
fn compose_three_diffs_chained_moves() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));

    let mut diff3 = AtomicStructure::new_diff();
    let a3 = diff3.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    diff3.set_anchor_position(a3, DVec3::new(2.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], TOL);
}

#[test]
fn compose_three_diffs_add_move_delete() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(7, DVec3::new(1.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));

    let mut diff3 = AtomicStructure::new_diff();
    let d = diff3.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(2.0, 0.0, 0.0));
    diff3.set_anchor_position(d, DVec3::new(2.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], TOL);
}

#[test]
fn compose_three_diffs_interleaved_operations() {
    let mut base = AtomicStructure::new();
    let c = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let n = base.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    let o = base.add_atom(8, DVec3::new(4.0, 0.0, 0.0));
    let si = base.add_atom(14, DVec3::new(6.0, 0.0, 0.0));
    base.add_bond(c, n, BOND_SINGLE);
    base.add_bond(n, o, BOND_SINGLE);
    base.add_bond(o, si, BOND_SINGLE);

    // diff1: delete C, move N slightly
    let mut diff1 = AtomicStructure::new_diff();
    let d1 = diff1.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(d1, DVec3::new(0.0, 0.0, 0.0));
    let n1 = diff1.add_atom(7, DVec3::new(2.5, 0.0, 0.0));
    diff1.set_anchor_position(n1, DVec3::new(2.0, 0.0, 0.0));

    // diff2: set frozen on O, add P
    let mut diff2 = AtomicStructure::new_diff();
    let o2 = diff2.add_atom(8, DVec3::new(4.0, 0.0, 0.0));
    diff2.set_anchor_position(o2, DVec3::new(4.0, 0.0, 0.0));
    diff2.set_atom_frozen(o2, true);
    diff2.add_atom(15, DVec3::new(8.0, 0.0, 0.0)); // P

    // diff3: delete moved N, move Si, add H
    let mut diff3 = AtomicStructure::new_diff();
    let d3 = diff3.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(2.5, 0.0, 0.0));
    diff3.set_anchor_position(d3, DVec3::new(2.5, 0.0, 0.0));
    let si3 = diff3.add_atom(14, DVec3::new(6.0, 1.0, 0.0));
    diff3.set_anchor_position(si3, DVec3::new(6.0, 0.0, 0.0));
    diff3.add_atom(1, DVec3::new(8.0, 1.0, 0.0)); // H

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], TOL);
}

// ============================================================================
// 5.12 Edge Cases
// ============================================================================

#[test]
fn compose_diff2_targets_base_passthrough() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(7, DVec3::new(5.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(8, DVec3::new(10.0, 0.0, 0.0)); // pure addition O

    let mut diff2 = AtomicStructure::new_diff();
    let si = diff2.add_atom(14, DVec3::new(5.0, 0.0, 0.0));
    diff2.set_anchor_position(si, DVec3::new(5.0, 0.0, 0.0)); // replace N with Si

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_diff2_orphaned_delete_passthrough() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(7, DVec3::new(5.0, 0.0, 0.0));

    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(1, DVec3::new(2.0, 0.0, 0.0)); // pure addition H

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(5.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(5.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_near_tolerance_boundary() {
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff1.add_atom(7, DVec3::new(0.15, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.05, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.05, 0.0, 0.0));

    let base = AtomicStructure::new();
    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_all_diff1_atoms_cancelled() {
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let d1 = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d1, DVec3::new(0.0, 0.0, 0.0));
    let d2 = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(d2, DVec3::new(2.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.composed.get_num_of_atoms(), 0);
    assert_eq!(result.stats.cancellations, 2);

    let mut base = AtomicStructure::new();
    base.add_atom(14, DVec3::new(10.0, 0.0, 0.0));
    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn compose_diff1_delete_not_matchable() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // diff1: delete C at origin
    let mut diff1 = AtomicStructure::new_diff();
    let d = diff1.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));

    // diff2: add N at same position
    let mut diff2 = AtomicStructure::new_diff();
    diff2.add_atom(7, DVec3::new(0.0, 0.0, 0.0)); // pure addition

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    // Both should be in composed: delete marker + addition
    assert_eq!(result.composed.get_num_of_atoms(), 2);
}

// ============================================================================
// 5.13 Equivalence Property Tests (Complex Scenarios)
// ============================================================================

#[test]
fn equivalence_diamond_fragment_two_edits() {
    // 4 C atoms in tetrahedral arrangement
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    let c3 = base.add_atom(6, DVec3::new(0.77, 1.33, 0.0));
    let c4 = base.add_atom(6, DVec3::new(0.77, 0.44, 1.26));
    base.add_bond(c1, c2, BOND_SINGLE);
    base.add_bond(c1, c3, BOND_SINGLE);
    base.add_bond(c1, c4, BOND_SINGLE);

    // diff1: move c2 by 0.5, add H
    let mut diff1 = AtomicStructure::new_diff();
    let m = diff1.add_atom(6, DVec3::new(2.04, 0.0, 0.0));
    diff1.set_anchor_position(m, DVec3::new(1.54, 0.0, 0.0));
    let h = diff1.add_atom(1, DVec3::new(2.54, 0.5, 0.0));
    diff1.add_bond(m, h, BOND_SINGLE);

    // diff2: delete c3, change c1-c4 bond to double
    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.77, 1.33, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.77, 1.33, 0.0));
    let u1 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u4 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.77, 0.44, 1.26));
    diff2.add_bond(u1, u4, BOND_DOUBLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn equivalence_linear_chain_mixed_operations() {
    // 6 C atoms in a chain
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c3 = base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let c4 = base.add_atom(6, DVec3::new(4.5, 0.0, 0.0));
    let c5 = base.add_atom(6, DVec3::new(6.0, 0.0, 0.0));
    let c6 = base.add_atom(6, DVec3::new(7.5, 0.0, 0.0));
    base.add_bond(c1, c2, BOND_SINGLE);
    base.add_bond(c2, c3, BOND_SINGLE);
    base.add_bond(c3, c4, BOND_SINGLE);
    base.add_bond(c4, c5, BOND_SINGLE);
    base.add_bond(c5, c6, BOND_SINGLE);

    // diff1: delete C2, move C4, add N bonded to C1
    let mut diff1 = AtomicStructure::new_diff();
    let d = diff1.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    diff1.set_anchor_position(d, DVec3::new(1.5, 0.0, 0.0));
    let m = diff1.add_atom(6, DVec3::new(4.5, 1.0, 0.0)); // move C4
    diff1.set_anchor_position(m, DVec3::new(4.5, 0.0, 0.0));
    let u1 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let n = diff1.add_atom(7, DVec3::new(0.0, 1.5, 0.0)); // add N
    diff1.add_bond(u1, n, BOND_SINGLE);

    // diff2: replace C1 with Si, delete N (cancels diff1's addition), add double bond C5-C6
    let mut diff2 = AtomicStructure::new_diff();
    let si = diff2.add_atom(14, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(si, DVec3::new(0.0, 0.0, 0.0));
    let dn = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 1.5, 0.0));
    diff2.set_anchor_position(dn, DVec3::new(0.0, 1.5, 0.0));
    let u5 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(6.0, 0.0, 0.0));
    let u6 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(7.5, 0.0, 0.0));
    diff2.add_bond(u5, u6, BOND_DOUBLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn equivalence_bond_heavy() {
    // 4 atoms in a ring + cross bond
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c3 = base.add_atom(6, DVec3::new(1.5, 1.5, 0.0));
    let c4 = base.add_atom(6, DVec3::new(0.0, 1.5, 0.0));
    base.add_bond(c1, c2, BOND_SINGLE);
    base.add_bond(c2, c3, BOND_SINGLE);
    base.add_bond(c3, c4, BOND_SINGLE);
    base.add_bond(c4, c1, BOND_SINGLE);
    base.add_bond(c1, c3, BOND_SINGLE); // cross bond

    // diff1: delete C1-C3 bond, change C2-C3 to double, add N bonded to C4
    let mut diff1 = AtomicStructure::new_diff();
    let u1 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    let u3 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 1.5, 0.0));
    let u4 = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 1.5, 0.0));
    diff1.add_bond(u1, u3, BOND_DELETED);
    diff1.add_bond(u2, u3, BOND_DOUBLE);
    let n = diff1.add_atom(7, DVec3::new(0.0, 3.0, 0.0));
    diff1.add_bond(u4, n, BOND_SINGLE);

    // diff2: change C2-C3 back to single, re-add C1-C3, delete C4-N bond
    let mut diff2 = AtomicStructure::new_diff();
    let v1 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let v2 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    let v3 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 1.5, 0.0));
    let v4 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 1.5, 0.0));
    let vn = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 3.0, 0.0));
    diff2.add_bond(v2, v3, BOND_SINGLE);
    diff2.add_bond(v1, v3, BOND_SINGLE);
    diff2.add_bond(v4, vn, BOND_DELETED);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn equivalence_three_diffs_all_operation_types() {
    let mut base = AtomicStructure::new();
    let a1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let a2 = base.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    let a3 = base.add_atom(8, DVec3::new(4.0, 0.0, 0.0));
    let a4 = base.add_atom(14, DVec3::new(6.0, 0.0, 0.0));
    let a5 = base.add_atom(15, DVec3::new(8.0, 0.0, 0.0));
    let a6 = base.add_atom(16, DVec3::new(10.0, 0.0, 0.0));
    let a7 = base.add_atom(17, DVec3::new(12.0, 0.0, 0.0));
    let a8 = base.add_atom(9, DVec3::new(14.0, 0.0, 0.0));
    base.add_bond(a1, a2, BOND_SINGLE);
    base.add_bond(a3, a4, BOND_SINGLE);
    base.add_bond(a5, a6, BOND_SINGLE);
    base.add_bond(a7, a8, BOND_SINGLE);

    // diff1: add 2 atoms, delete a3, move a5
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(1, DVec3::new(0.0, 2.0, 0.0)); // add H
    diff1.add_atom(1, DVec3::new(2.0, 2.0, 0.0)); // add H
    let d = diff1.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(4.0, 0.0, 0.0));
    diff1.set_anchor_position(d, DVec3::new(4.0, 0.0, 0.0));
    let m = diff1.add_atom(15, DVec3::new(8.0, 1.0, 0.0));
    diff1.set_anchor_position(m, DVec3::new(8.0, 0.0, 0.0));

    // diff2: modify one of diff1's additions, delete a4, add bond
    let mut diff2 = AtomicStructure::new_diff();
    let e = diff2.add_atom(6, DVec3::new(0.0, 2.0, 0.0)); // change H→C at (0,2,0)
    diff2.set_anchor_position(e, DVec3::new(0.0, 2.0, 0.0));
    let d2 = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(6.0, 0.0, 0.0));
    diff2.set_anchor_position(d2, DVec3::new(6.0, 0.0, 0.0));
    let u6 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(10.0, 0.0, 0.0));
    let u7 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(12.0, 0.0, 0.0));
    diff2.add_bond(u6, u7, BOND_DOUBLE);

    // diff3: move the other H (diff1's at (2,2,0)), add bonds, set frozen
    let mut diff3 = AtomicStructure::new_diff();
    let m3 = diff3.add_atom(1, DVec3::new(2.0, 3.0, 0.0));
    diff3.set_anchor_position(m3, DVec3::new(2.0, 2.0, 0.0));
    diff3.set_atom_frozen(m3, true);
    let u1 = diff3.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let u2 = diff3.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(2.0, 0.0, 0.0));
    diff3.add_bond(u1, u2, BOND_DOUBLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], TOL);
}

#[test]
fn equivalence_pure_bond_diffs() {
    let mut base = AtomicStructure::new();
    let a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = base.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let d = base.add_atom(6, DVec3::new(4.5, 0.0, 0.0));
    let e = base.add_atom(6, DVec3::new(6.0, 0.0, 0.0));
    base.add_bond(a, b, BOND_SINGLE);
    base.add_bond(b, c, BOND_SINGLE);
    base.add_bond(c, d, BOND_SINGLE);
    base.add_bond(d, e, BOND_SINGLE);

    // diff1: delete A-B, add A-C single, change B-C to double
    let mut diff1 = AtomicStructure::new_diff();
    let ua = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let ub = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    let uc = diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(3.0, 0.0, 0.0));
    diff1.add_bond(ua, ub, BOND_DELETED);
    diff1.add_bond(ua, uc, BOND_SINGLE);
    diff1.add_bond(ub, uc, BOND_DOUBLE);

    // diff2: change A-C to triple, delete D-E, add A-E
    let mut diff2 = AtomicStructure::new_diff();
    let va = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let vc = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(3.0, 0.0, 0.0));
    let vd = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(4.5, 0.0, 0.0));
    let ve = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(6.0, 0.0, 0.0));
    diff2.add_bond(va, vc, BOND_TRIPLE);
    diff2.add_bond(vd, ve, BOND_DELETED);
    diff2.add_bond(va, ve, BOND_SINGLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

#[test]
fn equivalence_large_structure_sparse_diffs() {
    // Build a 50 atom grid
    let mut base = AtomicStructure::new();
    let mut ids = Vec::new();
    for i in 0..50 {
        let x = (i % 10) as f64 * 2.0;
        let y = (i / 10) as f64 * 2.0;
        let id = base.add_atom(6, DVec3::new(x, y, 0.0));
        ids.push(id);
    }
    // Add some bonds (neighboring in grid)
    for i in 0..50 {
        let col = i % 10;
        let row = i / 10;
        if col + 1 < 10 {
            base.add_bond(ids[i], ids[i + 1], BOND_SINGLE);
        }
        if row + 1 < 5 {
            base.add_bond(ids[i], ids[i + 10], BOND_SINGLE);
        }
    }

    // diff1: move 2 atoms, add 1
    let mut diff1 = AtomicStructure::new_diff();
    let m1 = diff1.add_atom(6, DVec3::new(0.0, 0.5, 0.0)); // move atom at (0,0)
    diff1.set_anchor_position(m1, DVec3::new(0.0, 0.0, 0.0));
    let m2 = diff1.add_atom(6, DVec3::new(4.0, 0.5, 0.0)); // move atom at (4,0)
    diff1.set_anchor_position(m2, DVec3::new(4.0, 0.0, 0.0));
    diff1.add_atom(7, DVec3::new(20.0, 0.0, 0.0)); // add N far away

    // diff2: delete 1 atom, modify 1 bond
    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(2.0, 0.0, 0.0));
    let u1 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(6.0, 0.0, 0.0));
    let u2 = diff2.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(8.0, 0.0, 0.0));
    diff2.add_bond(u1, u2, BOND_DOUBLE);

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);

    // Verify composed diff is small
    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert!(result.composed.get_num_of_atoms() < 10);
}

// ============================================================================
// 5.14 Composed Diff Structure Tests
// ============================================================================

#[test]
fn composed_diff_is_diff() {
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let diff2 = AtomicStructure::new_diff();

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert!(result.composed.is_diff());
}

#[test]
fn composed_diff_has_correct_anchors() {
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(1.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.position, DVec3::new(2.0, 0.0, 0.0));
    // Anchor should point to original base position (0,0,0), not intermediate (1,0,0)
    assert_eq!(
        result.composed.anchor_position(atom.id).copied(),
        Some(DVec3::new(0.0, 0.0, 0.0))
    );
}

#[test]
fn composed_diff_no_orphan_bonds() {
    let mut diff1 = AtomicStructure::new_diff();
    let c = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let n = diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    diff1.add_bond(c, n, BOND_SINGLE);

    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);

    // Verify no orphan bonds: all bond endpoints must be valid atom IDs
    for atom in result.composed.atoms_values() {
        for bond in &atom.bonds {
            assert!(
                result.composed.get_atom(bond.other_atom_id()).is_some(),
                "orphan bond: atom {} references non-existent atom {}",
                atom.id,
                bond.other_atom_id()
            );
        }
    }
}

#[test]
fn composed_diff_cancellation_is_clean() {
    let mut diff1 = AtomicStructure::new_diff();
    let c = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let n = diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0));
    diff1.add_bond(c, n, BOND_SINGLE);

    // Cancel C
    let mut diff2 = AtomicStructure::new_diff();
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    // C is cancelled: only N remains, no bonds
    assert_eq!(result.composed.get_num_of_atoms(), 1);
    assert_eq!(result.composed.get_num_of_bonds(), 0);

    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.atomic_number, 7); // N
    assert!(atom.bonds.is_empty());
}

#[test]
fn composed_stats_are_accurate() {
    // Setup: diff1 has 3 atoms (2 additions, 1 unchanged marker)
    // diff2 matches 1 addition (cancel it), leaves 1 unmatched, adds 1 new
    let mut diff1 = AtomicStructure::new_diff();
    diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // pure addition
    diff1.add_atom(7, DVec3::new(2.0, 0.0, 0.0)); // pure addition
    diff1.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(5.0, 0.0, 0.0)); // unchanged

    let mut diff2 = AtomicStructure::new_diff();
    // Cancel the C at origin
    let d = diff2.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    diff2.set_anchor_position(d, DVec3::new(0.0, 0.0, 0.0));
    // Add a new atom
    diff2.add_atom(8, DVec3::new(10.0, 0.0, 0.0)); // O

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    assert_eq!(result.stats.cancellations, 1); // C cancelled
    assert_eq!(result.stats.composed_pairs, 0); // only the cancelled pair was matched (and counted as cancellation)
    assert_eq!(result.stats.diff1_passthrough, 2); // N + unchanged marker
    assert_eq!(result.stats.diff2_passthrough, 1); // O
}

// ============================================================================
// Regression: replace-in-place + move composition
// ============================================================================

/// When diff1 replaces a base atom in-place (atom at position X with anchor X,
/// as created by the fixed `replace_in_diff`), and diff2 moves the atom from X
/// to Y, the composed diff must correctly consume the base atom at X.
///
/// Regression test: before the fix, replace_in_diff omitted the anchor, causing
/// the composition to treat the replacement as a "pure addition" and lose the
/// connection to the base atom.
#[test]
fn compose_replace_then_move() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Carbon at origin

    // diff1: replace C with N at same position (anchor = position, as replace_in_diff now creates)
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(7, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: move the atom (now N) from origin to (1,0,0)
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(7, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(0.0, 0.0, 0.0));

    let result = compose_two_diffs(&diff1, &diff2, TOL);
    // Composed: N at (1,0,0) with anchor (0,0,0) — traces back to the original base atom
    let atom = result.composed.atoms_values().next().unwrap();
    assert_eq!(atom.position, DVec3::new(1.0, 0.0, 0.0));
    assert_eq!(atom.atomic_number, 7);
    assert_eq!(
        result.composed.anchor_position(atom.id).copied(),
        Some(DVec3::new(0.0, 0.0, 0.0))
    );

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

/// Same as above but with an element that matches the base (position-only anchor).
#[test]
fn compose_same_element_replace_then_move() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // diff1: "replace" C with C (same element, anchor = position)
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: move from origin to (1,0,0)
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(0.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2], TOL);
}

/// Three-diff chain: replace, then move, then further modify.
#[test]
fn compose_replace_then_move_then_modify() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    base.add_atom(6, DVec3::new(5.0, 0.0, 0.0));

    // diff1: replace C at origin with N (anchor = position)
    let mut diff1 = AtomicStructure::new_diff();
    let a1 = diff1.add_atom(7, DVec3::new(0.0, 0.0, 0.0));
    diff1.set_anchor_position(a1, DVec3::new(0.0, 0.0, 0.0));

    // diff2: move N from origin to (1,0,0)
    let mut diff2 = AtomicStructure::new_diff();
    let a2 = diff2.add_atom(7, DVec3::new(1.0, 0.0, 0.0));
    diff2.set_anchor_position(a2, DVec3::new(0.0, 0.0, 0.0));

    // diff3: change element from N to O at (1,0,0)
    let mut diff3 = AtomicStructure::new_diff();
    let a3 = diff3.add_atom(8, DVec3::new(1.0, 0.0, 0.0));
    diff3.set_anchor_position(a3, DVec3::new(1.0, 0.0, 0.0));

    assert_compose_equivalence(&base, &[&diff1, &diff2, &diff3], TOL);
}

// ============================================================================
// compose_diffs (N-ary) edge cases
// ============================================================================

#[test]
fn compose_diffs_empty_slice_returns_none() {
    let result = compose_diffs(&[], TOL);
    assert!(result.is_none());
}
