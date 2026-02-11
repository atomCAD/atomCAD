use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_DELETED;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::{
    AtomSource, DiffStats, apply_diff,
};

const DEFAULT_TOLERANCE: f64 = 0.1;

// ============================================================================
// Helpers
// ============================================================================

/// Creates a simple test molecule: C-C with two H atoms.
/// C1(0,0,0) - C2(1.5,0,0), H1(-0.5,0.5,0) bonded to C1, H2(2.0,0.5,0) bonded to C2
fn create_ethane_like() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id=1
    let c2 = s.add_atom(6, DVec3::new(1.5, 0.0, 0.0)); // id=2
    let h1 = s.add_atom(1, DVec3::new(-0.5, 0.5, 0.0)); // id=3
    let h2 = s.add_atom(1, DVec3::new(2.0, 0.5, 0.0)); // id=4
    s.add_bond(c1, c2, 1);
    s.add_bond(c1, h1, 1);
    s.add_bond(c2, h2, 1);
    s
}

/// Helper: count atoms with a given atomic number in a structure.
fn count_element(s: &AtomicStructure, atomic_number: i16) -> usize {
    s.atoms_values()
        .filter(|a| a.atomic_number == atomic_number)
        .count()
}

/// Helper: check if a bond exists between two atoms in a structure.
fn has_bond(s: &AtomicStructure, id1: u32, id2: u32) -> bool {
    s.has_bond_between(id1, id2)
}

/// Helper: get bond order between two atoms (by result IDs).
fn bond_order_between(s: &AtomicStructure, id1: u32, id2: u32) -> Option<u8> {
    if let Some(atom) = s.get_atom(id1) {
        for bond in &atom.bonds {
            if bond.other_atom_id() == id2 {
                return Some(bond.bond_order());
            }
        }
    }
    None
}

// ============================================================================
// Group 1: Atom Addition
// ============================================================================

#[test]
fn test_add_atom_to_structure() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Add a nitrogen at a new position (no match in base)
    diff.add_atom(7, DVec3::new(5.0, 5.0, 5.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 5); // 4 base + 1 added
    assert_eq!(count_element(&result.result, 7), 1); // 1 nitrogen
    assert_eq!(result.stats.atoms_added, 1);
    assert_eq!(result.stats.atoms_deleted, 0);
    assert_eq!(result.stats.atoms_modified, 0);
}

#[test]
fn test_add_multiple_atoms() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(7, DVec3::new(5.0, 5.0, 5.0));
    diff.add_atom(8, DVec3::new(6.0, 6.0, 6.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 6);
    assert_eq!(result.stats.atoms_added, 2);
}

// ============================================================================
// Group 2: Atom Deletion
// ============================================================================

#[test]
fn test_delete_atom_by_position_match() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Delete marker at C1 position (0,0,0)
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 3); // 4 - 1 deleted
    assert_eq!(count_element(&result.result, 6), 1); // Only C2 remains
    assert_eq!(result.stats.atoms_deleted, 1);
    // Bonds to deleted atom should also be gone
    assert_eq!(result.result.get_num_of_bonds(), 1); // C2-H2 survives, C1-C2 and C1-H1 gone
}

#[test]
fn test_no_match_delete_marker_ignored() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Delete marker at a position with no base atom
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(99.0, 99.0, 99.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 4); // Nothing deleted
    assert_eq!(result.stats.atoms_deleted, 0);
}

// ============================================================================
// Group 3: Atom Replacement
// ============================================================================

#[test]
fn test_replace_element_at_matched_position() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Replace C1 (at 0,0,0) with Silicon (14)
    diff.add_atom(14, DVec3::new(0.0, 0.0, 0.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 4);
    assert_eq!(count_element(&result.result, 14), 1); // 1 silicon
    assert_eq!(count_element(&result.result, 6), 1); // C2 remains
    assert_eq!(result.stats.atoms_modified, 1);
    assert_eq!(result.stats.atoms_added, 0);
    assert_eq!(result.stats.atoms_deleted, 0);
}

// ============================================================================
// Group 4: Atom Movement (Anchors)
// ============================================================================

#[test]
fn test_move_atom_via_anchor() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Move C1 from (0,0,0) to (0.5,0.5,0.5)
    let diff_id = diff.add_atom(6, DVec3::new(0.5, 0.5, 0.5));
    diff.set_anchor_position(diff_id, DVec3::new(0.0, 0.0, 0.0)); // Anchor at old position

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 4);
    assert_eq!(result.stats.atoms_modified, 1);

    // Verify the atom is at the new position
    let moved_atom = result
        .result
        .atoms_values()
        .find(|a| (a.position - DVec3::new(0.5, 0.5, 0.5)).length() < 1e-10)
        .expect("Moved atom should be at new position");
    assert_eq!(moved_atom.atomic_number, 6);
}

#[test]
fn test_move_preserves_bonds_to_non_diff_neighbors() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Move C1 from (0,0,0) to (0.3,0,0). C1 is bonded to C2 and H1 in base.
    let diff_id = diff.add_atom(6, DVec3::new(0.3, 0.0, 0.0));
    diff.set_anchor_position(diff_id, DVec3::new(0.0, 0.0, 0.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // All 3 bonds should survive: C1-C2, C1-H1, C2-H2
    assert_eq!(result.result.get_num_of_bonds(), 3);
}

#[test]
fn test_move_and_element_change_combined() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Move C1 from (0,0,0) to (0.3,0.3,0.3) AND change to Silicon
    let diff_id = diff.add_atom(14, DVec3::new(0.3, 0.3, 0.3));
    diff.set_anchor_position(diff_id, DVec3::new(0.0, 0.0, 0.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 4);
    assert_eq!(count_element(&result.result, 14), 1);
    assert_eq!(count_element(&result.result, 6), 1); // C2 remains
    assert_eq!(result.stats.atoms_modified, 1);
}

// ============================================================================
// Group 5: Bond Resolution
// ============================================================================

#[test]
fn test_bond_both_atoms_in_diff_override() {
    // Both C1 and C2 are in the diff (as replacements), with a double bond between them
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let d1 = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // matches C1
    let d2 = diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0)); // matches C2
    diff.add_bond(d1, d2, 2); // Override: double bond

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // Find the result atoms that correspond to C1 and C2
    let result_c1 = *result.provenance.base_to_result.get(&1).unwrap();
    let result_c2 = *result.provenance.base_to_result.get(&2).unwrap();

    assert_eq!(
        bond_order_between(&result.result, result_c1, result_c2),
        Some(2)
    );
}

#[test]
fn test_bond_both_atoms_in_diff_delete_marker() {
    // Both C1 and C2 in diff, with a bond delete marker between them
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let d1 = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let d2 = diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    diff.add_bond_checked(d1, d2, BOND_DELETED); // Delete marker

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    let result_c1 = *result.provenance.base_to_result.get(&1).unwrap();
    let result_c2 = *result.provenance.base_to_result.get(&2).unwrap();

    // The C1-C2 bond should be deleted
    assert!(!has_bond(&result.result, result_c1, result_c2));
    assert_eq!(result.stats.bonds_deleted, 1);
}

#[test]
fn test_bond_both_atoms_in_diff_no_diff_bond_survives() {
    // Both C1 and C2 in diff (as identity entries), no diff bond → base bond survives
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // identity for C1
    diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0)); // identity for C2
    // No bond between them in diff

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    let result_c1 = *result.provenance.base_to_result.get(&1).unwrap();
    let result_c2 = *result.provenance.base_to_result.get(&2).unwrap();

    // Base bond survives
    assert!(has_bond(&result.result, result_c1, result_c2));
    assert_eq!(
        bond_order_between(&result.result, result_c1, result_c2),
        Some(1)
    );
}

#[test]
fn test_bond_one_atom_in_diff_survives() {
    // Only C1 is in the diff (replaced with Si), C2 is not → C1-C2 bond survives
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(14, DVec3::new(0.0, 0.0, 0.0)); // Replace C1 with Si

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // Bond between replaced atom and C2 should survive
    let result_c1 = *result.provenance.base_to_result.get(&1).unwrap();
    let result_c2 = *result.provenance.base_to_result.get(&2).unwrap();
    assert!(has_bond(&result.result, result_c1, result_c2));
}

#[test]
fn test_bond_neither_atom_in_diff_untouched() {
    // Diff only adds a new atom far away — base bonds untouched
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(7, DVec3::new(10.0, 10.0, 10.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_bonds(), 3); // All 3 base bonds survive
}

#[test]
fn test_replace_bonded_atoms_preserves_bond() {
    // Replace both C1 and C2 with Si — their bond should survive without explicit bond in diff
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(14, DVec3::new(0.0, 0.0, 0.0)); // C1 → Si
    diff.add_atom(14, DVec3::new(1.5, 0.0, 0.0)); // C2 → Si
    // No bond in diff → base bond should survive by default

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    let result_c1 = *result.provenance.base_to_result.get(&1).unwrap();
    let result_c2 = *result.provenance.base_to_result.get(&2).unwrap();
    assert!(has_bond(&result.result, result_c1, result_c2));
    assert_eq!(
        bond_order_between(&result.result, result_c1, result_c2),
        Some(1)
    );
}

#[test]
fn test_identity_entry_matches_base_atom() {
    // Add an identity entry (same pos, same element) — should match base atom
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Identity for C1

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 4); // No new atoms
    assert_eq!(result.stats.atoms_modified, 1); // Counted as modified (identity edit)
    assert_eq!(result.stats.atoms_added, 0);
}

#[test]
fn test_new_bond_between_diff_added_atoms() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let n1 = diff.add_atom(7, DVec3::new(5.0, 0.0, 0.0));
    let n2 = diff.add_atom(7, DVec3::new(6.0, 0.0, 0.0));
    diff.add_bond(n1, n2, 1);

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 6);
    assert_eq!(result.result.get_num_of_bonds(), 4); // 3 base + 1 new
    assert_eq!(result.stats.bonds_added, 1);
}

#[test]
fn test_new_bond_between_diff_matched_and_added() {
    // Bond between a matched diff atom (identity for C1) and a new added atom
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let d1 = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Identity for C1
    let d2 = diff.add_atom(7, DVec3::new(-1.0, 0.0, 0.0)); // New N atom
    diff.add_bond(d1, d2, 1);

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 5);
    // 3 base bonds + 1 new = 4
    assert_eq!(result.result.get_num_of_bonds(), 4);
    assert_eq!(result.stats.bonds_added, 1);
}

// ============================================================================
// Group 6: Tolerance Edge Cases
// ============================================================================

#[test]
fn test_tolerance_just_inside() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff = AtomicStructure::new_diff();
    // Place diff atom at distance 0.09 < tolerance 0.1
    diff.add_atom(14, DVec3::new(0.09, 0.0, 0.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 1); // Matched and replaced
    assert_eq!(count_element(&result.result, 14), 1);
    assert_eq!(result.stats.atoms_modified, 1);
}

#[test]
fn test_tolerance_just_outside() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff = AtomicStructure::new_diff();
    // Place diff atom at distance 0.11 > tolerance 0.1
    diff.add_atom(14, DVec3::new(0.11, 0.0, 0.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 2); // No match → added
    assert_eq!(count_element(&result.result, 14), 1); // Added silicon
    assert_eq!(count_element(&result.result, 6), 1); // Original carbon
    assert_eq!(result.stats.atoms_added, 1);
    assert_eq!(result.stats.atoms_modified, 0);
}

// ============================================================================
// Group 7: Multiple Close Atoms (Greedy Assignment)
// ============================================================================

#[test]
fn test_greedy_assignment_closest_first() {
    // Two base atoms close together. Two diff atoms should match one each.
    let mut base = AtomicStructure::new();
    let _b1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let _b2 = base.add_atom(6, DVec3::new(0.05, 0.0, 0.0)); // Very close to b1

    let mut diff = AtomicStructure::new_diff();
    // d1 is closer to b1 (distance 0.01), d2 is closer to b2 (distance 0.01)
    diff.add_atom(14, DVec3::new(0.01, 0.0, 0.0)); // Close to b1
    diff.add_atom(15, DVec3::new(0.06, 0.0, 0.0)); // Close to b2

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 2);
    assert_eq!(result.stats.atoms_modified, 2);
    assert_eq!(result.stats.atoms_added, 0);
    // Both should be replaced
    assert_eq!(count_element(&result.result, 14), 1);
    assert_eq!(count_element(&result.result, 15), 1);
}

// ============================================================================
// Group 8: Edge Cases
// ============================================================================

#[test]
fn test_empty_diff_is_identity() {
    let base = create_ethane_like();
    let diff = AtomicStructure::new_diff();

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 4);
    assert_eq!(result.result.get_num_of_bonds(), 3);
    assert_eq!(
        result.stats,
        DiffStats {
            atoms_added: 0,
            atoms_deleted: 0,
            atoms_modified: 0,
            bonds_added: 0,
            bonds_deleted: 0,
        }
    );
}

#[test]
fn test_empty_base_all_added() {
    let base = AtomicStructure::new();
    let mut diff = AtomicStructure::new_diff();
    let d1 = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let d2 = diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    diff.add_bond(d1, d2, 1);

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 2);
    assert_eq!(result.result.get_num_of_bonds(), 1);
    assert_eq!(result.stats.atoms_added, 2);
    assert_eq!(result.stats.bonds_added, 1);
}

#[test]
fn test_result_has_is_diff_false() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(7, DVec3::new(5.0, 5.0, 5.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);
    assert!(!result.result.is_diff());
}

// ============================================================================
// Group 9: Provenance
// ============================================================================

#[test]
fn test_provenance_added_atom() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let diff_n = diff.add_atom(7, DVec3::new(5.0, 5.0, 5.0));

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    let result_n = *result.provenance.diff_to_result.get(&diff_n).unwrap();
    assert_eq!(
        result.provenance.sources.get(&result_n),
        Some(&AtomSource::DiffAdded(diff_n))
    );
}

#[test]
fn test_provenance_passthrough_base_atom() {
    let base = create_ethane_like();
    let diff = AtomicStructure::new_diff();

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // All 4 base atoms should be pass-throughs
    for base_id in 1..=4u32 {
        let result_id = *result
            .provenance
            .base_to_result
            .get(&base_id)
            .expect("Base atom should be in result");
        assert_eq!(
            result.provenance.sources.get(&result_id),
            Some(&AtomSource::BasePassthrough(base_id))
        );
    }
}

#[test]
fn test_provenance_replaced_atom() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let diff_si = diff.add_atom(14, DVec3::new(0.0, 0.0, 0.0)); // Replace C1

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    let result_id = *result
        .provenance
        .base_to_result
        .get(&1)
        .expect("Base C1 should map to result");
    assert_eq!(
        result.provenance.sources.get(&result_id),
        Some(&AtomSource::DiffMatchedBase {
            diff_id: diff_si,
            base_id: 1,
        })
    );
    assert_eq!(
        result.provenance.diff_to_result.get(&diff_si),
        Some(&result_id)
    );
}

#[test]
fn test_provenance_deleted_atom_absent() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0)); // Delete C1

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // Base atom 1 (C1) should NOT be in base_to_result (it was deleted)
    assert!(!result.provenance.base_to_result.contains_key(&1));
    // The delete marker diff atom should NOT be in diff_to_result
    // (delete markers don't produce result atoms)
    assert!(result.provenance.diff_to_result.is_empty());
}

#[test]
fn test_provenance_reverse_maps_correct() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let diff_si = diff.add_atom(14, DVec3::new(0.0, 0.0, 0.0)); // Replace C1
    let diff_n = diff.add_atom(7, DVec3::new(5.0, 5.0, 5.0)); // Add N

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // base_to_result should contain base atoms 1,2,3,4 (C1 is matched by diff, others pass through)
    assert!(result.provenance.base_to_result.contains_key(&1));
    assert!(result.provenance.base_to_result.contains_key(&2));
    assert!(result.provenance.base_to_result.contains_key(&3));
    assert!(result.provenance.base_to_result.contains_key(&4));

    // diff_to_result should contain both diff atoms
    assert!(result.provenance.diff_to_result.contains_key(&diff_si));
    assert!(result.provenance.diff_to_result.contains_key(&diff_n));

    // The replaced atom should be in both maps, pointing to the same result ID
    let result_from_base = result.provenance.base_to_result.get(&1).unwrap();
    let result_from_diff = result.provenance.diff_to_result.get(&diff_si).unwrap();
    assert_eq!(result_from_base, result_from_diff);
}

// ============================================================================
// Group 10: Stats Verification
// ============================================================================

#[test]
fn test_stats_comprehensive() {
    let base = create_ethane_like(); // 4 atoms, 3 bonds

    let mut diff = AtomicStructure::new_diff();
    // Delete C1 (at 0,0,0)
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    // Replace C2 (at 1.5,0,0) with Si
    diff.add_atom(14, DVec3::new(1.5, 0.0, 0.0));
    // Add new N atom
    let n_id = diff.add_atom(7, DVec3::new(5.0, 5.0, 5.0));
    // Add new O atom bonded to N
    let o_id = diff.add_atom(8, DVec3::new(6.0, 5.0, 5.0));
    diff.add_bond(n_id, o_id, 2);

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.stats.atoms_deleted, 1); // C1
    assert_eq!(result.stats.atoms_modified, 1); // C2 → Si
    assert_eq!(result.stats.atoms_added, 2); // N, O
    assert_eq!(result.stats.bonds_added, 1); // N-O
    // C1-C2 and C1-H1 bonds are deleted (C1 was deleted)
    assert_eq!(result.stats.bonds_deleted, 2);
}

// ============================================================================
// Group 11: Bond Delete Marker for Non-Existent Bond
// ============================================================================

#[test]
fn test_bond_delete_marker_no_base_bond_noop() {
    // Place a bond delete marker between two diff-added atoms that have no base bond
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    let d1 = diff.add_atom(7, DVec3::new(5.0, 0.0, 0.0));
    let d2 = diff.add_atom(8, DVec3::new(6.0, 0.0, 0.0));
    diff.add_bond_checked(d1, d2, BOND_DELETED);

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // The delete marker bond should be a no-op — no crash, no bond added
    assert_eq!(result.result.get_num_of_bonds(), 3); // Only base bonds
    assert_eq!(result.stats.bonds_deleted, 0);
    assert_eq!(result.stats.bonds_added, 0);
}

// ============================================================================
// Group 12: Moving Two Bonded Atoms Simultaneously
// ============================================================================

#[test]
fn test_move_two_bonded_atoms_preserves_bond() {
    let base = create_ethane_like();
    let mut diff = AtomicStructure::new_diff();
    // Move C1 from (0,0,0) to (0.3,0.3,0)
    let d1 = diff.add_atom(6, DVec3::new(0.3, 0.3, 0.0));
    diff.set_anchor_position(d1, DVec3::new(0.0, 0.0, 0.0));
    // Move C2 from (1.5,0,0) to (1.8,0.3,0)
    let d2 = diff.add_atom(6, DVec3::new(1.8, 0.3, 0.0));
    diff.set_anchor_position(d2, DVec3::new(1.5, 0.0, 0.0));
    // No bond between them in diff → base bond should survive

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    let result_c1 = *result.provenance.base_to_result.get(&1).unwrap();
    let result_c2 = *result.provenance.base_to_result.get(&2).unwrap();

    // Bond between the two moved atoms should survive
    assert!(has_bond(&result.result, result_c1, result_c2));
    // All 3 bonds should survive
    assert_eq!(result.result.get_num_of_bonds(), 3);
}

// ============================================================================
// Group 13: Larger Tolerance
// ============================================================================

#[test]
fn test_custom_tolerance() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut diff = AtomicStructure::new_diff();
    // At distance 0.5 — outside default 0.1 but inside 1.0
    diff.add_atom(14, DVec3::new(0.5, 0.0, 0.0));

    // With default tolerance: should NOT match
    let result_strict = apply_diff(&base, &diff, 0.1);
    assert_eq!(result_strict.result.get_num_of_atoms(), 2);

    // With larger tolerance: should match
    let result_loose = apply_diff(&base, &diff, 1.0);
    assert_eq!(result_loose.result.get_num_of_atoms(), 1);
    assert_eq!(count_element(&result_loose.result, 14), 1);
}

// ============================================================================
// Group 14: Complex Scenario
// ============================================================================

#[test]
fn test_complex_mixed_operations() {
    // Base: diamond-like fragment: 4 carbons in tetrahedral arrangement
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let c2 = base.add_atom(6, DVec3::new(1.0, 1.0, 0.0));
    let c3 = base.add_atom(6, DVec3::new(1.0, 0.0, 1.0));
    let c4 = base.add_atom(6, DVec3::new(0.0, 1.0, 1.0));
    base.add_bond(c1, c2, 1);
    base.add_bond(c1, c3, 1);
    base.add_bond(c1, c4, 1);
    base.add_bond(c2, c3, 1);

    // Diff: delete C4, replace C1 with Si, add N, bond N to C2
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 1.0, 1.0)); // Delete C4
    diff.add_atom(14, DVec3::new(0.0, 0.0, 0.0)); // Replace C1 with Si
    let d_c2 = diff.add_atom(6, DVec3::new(1.0, 1.0, 0.0)); // Identity for C2 (needed for new bond)
    let d_n = diff.add_atom(7, DVec3::new(2.0, 2.0, 0.0)); // Add N
    diff.add_bond(d_c2, d_n, 1); // Bond C2 to N

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    // Atoms: 4 base - 1 deleted + 1 added = 4
    assert_eq!(result.result.get_num_of_atoms(), 4);
    assert_eq!(count_element(&result.result, 14), 1); // Si (was C1)
    assert_eq!(count_element(&result.result, 6), 2); // C2, C3
    assert_eq!(count_element(&result.result, 7), 1); // N (added)

    // Bonds: base had C1-C2, C1-C3, C1-C4, C2-C3
    // C1-C4 deleted (C4 deleted) → 1 bond deleted
    // C2-N added → 1 bond added
    // C1-C2, C1-C3, C2-C3 survive (C1 replaced by Si, bonds survive)
    assert_eq!(result.result.get_num_of_bonds(), 4); // 3 surviving + 1 new

    assert_eq!(result.stats.atoms_deleted, 1);
    assert_eq!(result.stats.atoms_modified, 2); // Si replacement + C2 identity
    assert_eq!(result.stats.atoms_added, 1);
    assert_eq!(result.stats.bonds_added, 1);
    assert_eq!(result.stats.bonds_deleted, 1); // C1-C4
}

// ============================================================================
// Group 15: Bond ordering edge case
// ============================================================================

#[test]
fn test_atom_deletion_removes_all_connected_bonds() {
    // Delete an atom that has multiple bonds — all connected bonds should be gone
    let mut base = AtomicStructure::new();
    let center = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let a = base.add_atom(1, DVec3::new(1.0, 0.0, 0.0));
    let b = base.add_atom(1, DVec3::new(0.0, 1.0, 0.0));
    let c = base.add_atom(1, DVec3::new(0.0, 0.0, 1.0));
    base.add_bond(center, a, 1);
    base.add_bond(center, b, 1);
    base.add_bond(center, c, 1);

    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0)); // Delete center

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 3); // 3 H atoms remain
    assert_eq!(result.result.get_num_of_bonds(), 0); // All bonds gone
    assert_eq!(result.stats.bonds_deleted, 3);
}

// ============================================================================
// Group 16: Empty structures
// ============================================================================

#[test]
fn test_both_empty() {
    let base = AtomicStructure::new();
    let diff = AtomicStructure::new_diff();

    let result = apply_diff(&base, &diff, DEFAULT_TOLERANCE);

    assert_eq!(result.result.get_num_of_atoms(), 0);
    assert_eq!(result.result.get_num_of_bonds(), 0);
    assert_eq!(result.stats, DiffStats::default());
}
