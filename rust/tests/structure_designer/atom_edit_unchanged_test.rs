use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_DOUBLE, BOND_SINGLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER, UNCHANGED_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, BaseAtomPromotionInfo, BondDeletionInfo,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::text_format::{
    parse_diff_text, serialize_diff,
};
use rust_lib_flutter_cad::util::transform::Transform;

// =============================================================================
// Identity entry creation tests (verify UNCHANGED is used)
// =============================================================================

#[test]
fn test_bond_deletion_uses_unchanged_markers() {
    // When deleting a bond between two base atoms, the identity entries
    // should use UNCHANGED_ATOMIC_NUMBER, not the real element.
    let mut data = AtomEditData::new();

    let info = BondDeletionInfo {
        diff_id_a: None,
        diff_id_b: None,
        identity_a: Some((6, DVec3::new(0.0, 0.0, 0.0))),
        identity_b: Some((7, DVec3::new(1.5, 0.0, 0.0))),
    };

    data.apply_delete_result_view(&[], &[], &[info]);

    // Both identity entries should be UNCHANGED, not C(6) or N(7)
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    for (_id, atom) in data.diff.iter_atoms() {
        assert_eq!(
            atom.atomic_number, UNCHANGED_ATOMIC_NUMBER,
            "Identity entry should use UNCHANGED_ATOMIC_NUMBER"
        );
    }
}

// =============================================================================
// Promotion tests — apply_replace
// =============================================================================

#[test]
fn test_replace_promotes_unchanged_marker() {
    // Create an UNCHANGED marker (as if from a bond tool), then replace it.
    let mut data = AtomEditData::new();

    // Simulate: bond tool created an UNCHANGED atom at (1,0,0)
    let unchanged_id = data.diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));

    // Now replace base atom 42 (which has existing diff entry = unchanged_id)
    data.selection.selected_base_atoms.insert(42);
    data.apply_replace(
        14, // Silicon
        &[BaseAtomPromotionInfo {
            base_id: 42,
            atomic_number: 6,
            position: DVec3::new(1.0, 0.0, 0.0),
            existing_diff_id: Some(unchanged_id),
        }],
    );

    // The existing diff entry should be reused (not a new atom created)
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let atom = data.diff.get_atom(unchanged_id).unwrap();
    assert_eq!(atom.atomic_number, 14); // Promoted to Silicon
    assert!(data.selection.selected_diff_atoms.contains(&unchanged_id));
    assert!(!data.selection.selected_base_atoms.contains(&42));
    // Should have an anchor (replacement marker)
    assert!(data.diff.anchor_position(unchanged_id).is_some());
}

// =============================================================================
// Promotion tests — apply_transform
// =============================================================================

#[test]
fn test_transform_promotes_unchanged_marker() {
    let mut data = AtomEditData::new();

    // Simulate: bond tool created an UNCHANGED atom with a bond
    let unch_a = data.diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let unch_b = data.diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    data.diff.add_bond(unch_a, unch_b, BOND_SINGLE);

    // Select base atom 42 (has existing diff entry = unch_a)
    data.selection.selected_base_atoms.insert(42);
    data.selection.selection_transform = Some(Transform::default());

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(
        &relative,
        &[BaseAtomPromotionInfo {
            base_id: 42,
            atomic_number: 6, // Carbon
            position: DVec3::new(0.0, 0.0, 0.0),
            existing_diff_id: Some(unch_a),
        }],
    );

    // unch_a should be promoted (reused), not a new atom
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    let atom = data.diff.get_atom(unch_a).unwrap();
    assert_eq!(atom.atomic_number, 6); // Promoted to Carbon
    assert!((atom.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10); // Moved
    assert!(data.diff.anchor_position(unch_a).is_some()); // Anchor set

    // Bond should still exist
    let bonds: Vec<_> = atom.bonds.iter().collect();
    assert_eq!(bonds.len(), 1);
    assert_eq!(bonds[0].other_atom_id(), unch_b);
}

#[test]
fn test_transform_no_existing_entry_still_works() {
    // When there's no existing diff entry, behavior should be the same as before.
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(42);
    data.selection.selection_transform = Some(Transform::default());

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(
        &relative,
        &[BaseAtomPromotionInfo {
            base_id: 42,
            atomic_number: 6,
            position: DVec3::new(1.0, 0.0, 0.0),
            existing_diff_id: None,
        }],
    );

    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (new_id, atom) = data.diff.iter_atoms().next().unwrap();
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
    assert!(data.diff.anchor_position(*new_id).is_some());
    assert!(data.selection.selected_diff_atoms.contains(new_id));
}

// =============================================================================
// End-to-end: UNCHANGED + apply_diff
// =============================================================================

#[test]
fn test_unchanged_bond_addition_via_apply_diff() {
    // Base: atoms A(C) at (0,0,0) and B(N) at (1.5,0,0), no bond.
    let mut base = AtomicStructure::new();
    let a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = base.add_atom(7, DVec3::new(1.5, 0.0, 0.0));

    // Diff: two UNCHANGED atoms at same positions with a bond.
    let mut diff = AtomicStructure::new_diff();
    let da = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let db = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    diff.add_bond(da, db, BOND_SINGLE);

    let result = apply_diff(&base, &diff, 0.1);

    // Result: A(C) and B(N) with a bond. Atoms unchanged (not -1).
    assert_eq!(result.result.get_num_of_atoms(), 2);
    let ra = *result.provenance.base_to_result.get(&a).unwrap();
    let rb = *result.provenance.base_to_result.get(&b).unwrap();
    let atom_a = result.result.get_atom(ra).unwrap();
    let atom_b = result.result.get_atom(rb).unwrap();
    assert_eq!(atom_a.atomic_number, 6); // Still Carbon
    assert_eq!(atom_b.atomic_number, 7); // Still Nitrogen
    assert!(result.result.has_bond_between(ra, rb));

    // Stats: no atoms modified
    assert_eq!(result.stats.atoms_modified, 0);
    assert_eq!(result.stats.unchanged_references, 2);
    assert_eq!(result.stats.bonds_added, 1);
}

#[test]
fn test_unchanged_bond_deletion_via_apply_diff() {
    // Base: A-B with single bond
    let mut base = AtomicStructure::new();
    let a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = base.add_atom(7, DVec3::new(1.5, 0.0, 0.0));
    base.add_bond(a, b, BOND_SINGLE);

    // Diff: two UNCHANGED atoms with BOND_DELETED
    let mut diff = AtomicStructure::new_diff();
    let da = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let db = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    diff.add_bond(da, db, BOND_DELETED);

    let result = apply_diff(&base, &diff, 0.1);

    assert_eq!(result.result.get_num_of_atoms(), 2);
    let ra = *result.provenance.base_to_result.get(&a).unwrap();
    let rb = *result.provenance.base_to_result.get(&b).unwrap();
    assert!(!result.result.has_bond_between(ra, rb));
    assert_eq!(result.stats.atoms_modified, 0);
    assert_eq!(result.stats.unchanged_references, 2);
}

#[test]
fn test_unchanged_bond_order_change_via_apply_diff() {
    // Base: A-B single bond
    let mut base = AtomicStructure::new();
    let a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = base.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    base.add_bond(a, b, BOND_SINGLE);

    // Diff: two UNCHANGED atoms with double bond
    let mut diff = AtomicStructure::new_diff();
    let da = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let db = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    diff.add_bond(da, db, BOND_DOUBLE);

    let result = apply_diff(&base, &diff, 0.1);

    let ra = *result.provenance.base_to_result.get(&a).unwrap();
    let rb = *result.provenance.base_to_result.get(&b).unwrap();
    // Bond should now be double
    assert!(result.result.has_bond_between(ra, rb));
    let atom_a = result.result.get_atom(ra).unwrap();
    let bond = atom_a
        .bonds
        .iter()
        .find(|b| b.other_atom_id() == rb)
        .unwrap();
    assert_eq!(bond.bond_order(), BOND_DOUBLE);
    assert_eq!(result.stats.atoms_modified, 0);
}

#[test]
fn test_unchanged_alongside_real_edits() {
    // Base: A, B, C, D
    let mut base = AtomicStructure::new();
    let _a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = base.add_atom(7, DVec3::new(1.5, 0.0, 0.0));
    let c = base.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let d = base.add_atom(6, DVec3::new(4.5, 0.0, 0.0));

    // Diff: delete A, move B, UNCHANGED C and D with a bond, add E
    let mut diff = AtomicStructure::new_diff();
    // Delete marker for A
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    // Move B: anchor at old pos, new position
    let db = diff.add_atom(7, DVec3::new(1.5, 1.0, 0.0));
    diff.set_anchor_position(db, DVec3::new(1.5, 0.0, 0.0));
    // UNCHANGED C and D with bond
    let dc = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(3.0, 0.0, 0.0));
    let dd = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(4.5, 0.0, 0.0));
    diff.add_bond(dc, dd, BOND_SINGLE);
    // Add E
    diff.add_atom(8, DVec3::new(6.0, 0.0, 0.0)); // Oxygen

    let result = apply_diff(&base, &diff, 0.1);

    // A deleted, B moved, C and D unchanged with bond, E added
    assert_eq!(result.result.get_num_of_atoms(), 4); // B, C, D, E
    assert_eq!(result.stats.atoms_deleted, 1);
    assert_eq!(result.stats.atoms_modified, 1); // B moved
    assert_eq!(result.stats.atoms_added, 1); // E
    assert_eq!(result.stats.unchanged_references, 2); // C and D
    assert_eq!(result.stats.bonds_added, 1);

    // Verify C and D are bonded
    let rc = *result.provenance.base_to_result.get(&c).unwrap();
    let rd = *result.provenance.base_to_result.get(&d).unwrap();
    assert!(result.result.has_bond_between(rc, rd));
    // Verify B is at new position
    let rb = *result.provenance.base_to_result.get(&b).unwrap();
    let atom_b = result.result.get_atom(rb).unwrap();
    assert!((atom_b.position - DVec3::new(1.5, 1.0, 0.0)).length() < 1e-10);
}

#[test]
fn test_unchanged_bond_to_added_atom() {
    // Base: A(C) at (0,0,0)
    let mut base = AtomicStructure::new();
    let a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // Diff: UNCHANGED at A + added E + bond
    let mut diff = AtomicStructure::new_diff();
    let da = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let de = diff.add_atom(8, DVec3::new(1.5, 0.0, 0.0)); // Oxygen
    diff.add_bond(da, de, BOND_SINGLE);

    let result = apply_diff(&base, &diff, 0.1);

    assert_eq!(result.result.get_num_of_atoms(), 2);
    let ra = *result.provenance.base_to_result.get(&a).unwrap();
    let re = *result.provenance.diff_to_result.get(&de).unwrap();
    assert!(result.result.has_bond_between(ra, re));
    assert_eq!(result.result.get_atom(ra).unwrap().atomic_number, 6); // Still Carbon
}

// =============================================================================
// Text format: unchanged serialization/parsing
// =============================================================================

#[test]
fn test_serialize_unchanged_marker() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.0, 2.0, 3.0));

    let text = serialize_diff(&diff);
    assert_eq!(text, "unchanged @ (1.0, 2.0, 3.0)");
}

#[test]
fn test_parse_unchanged_marker() {
    let text = "unchanged @ (1.0, 2.0, 3.0)";
    let diff = parse_diff_text(text).unwrap();

    assert_eq!(diff.get_num_of_atoms(), 1);
    let (_id, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, UNCHANGED_ATOMIC_NUMBER);
    assert!((atom.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn test_unchanged_text_format_roundtrip() {
    let mut diff = AtomicStructure::new_diff();
    let a = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let b = diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    diff.add_bond(a, b, BOND_SINGLE);

    let text1 = serialize_diff(&diff);
    let parsed = parse_diff_text(&text1).unwrap();
    let text2 = serialize_diff(&parsed);

    assert_eq!(text1, text2);
    assert!(text1.contains("unchanged @"));
    assert!(text1.contains("bond 1-2 single"));
}

#[test]
fn test_unchanged_mixed_text_format() {
    let text = "+C @ (0.0, 0.0, 0.0)\nunchanged @ (1.5, 0.0, 0.0)\nbond 1-2 single";
    let diff = parse_diff_text(text).unwrap();

    assert_eq!(diff.get_num_of_atoms(), 2);
    let mut atoms: Vec<_> = diff.iter_atoms().collect();
    atoms.sort_by_key(|(id, _)| *id);
    assert_eq!(atoms[0].1.atomic_number, 6); // Carbon addition
    assert_eq!(atoms[1].1.atomic_number, UNCHANGED_ATOMIC_NUMBER);
}

#[test]
fn test_parse_unchanged_missing_at_fails() {
    let result = parse_diff_text("unchanged (1.0, 2.0, 3.0)");
    assert!(result.is_err());
}

#[test]
fn test_parse_unchanged_missing_position_fails() {
    let result = parse_diff_text("unchanged @");
    assert!(result.is_err());
}

// =============================================================================
// Promotion: bonds survive promotion
// =============================================================================

#[test]
fn test_replace_preserves_bonds_on_unchanged_promotion() {
    let mut data = AtomEditData::new();

    // Simulate: bond tool created two UNCHANGED atoms with a bond
    let unch_a = data.diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(0.0, 0.0, 0.0));
    let unch_b = data.diff.add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.5, 0.0, 0.0));
    data.diff.add_bond(unch_a, unch_b, BOND_SINGLE);

    // Replace base atom A (which has existing UNCHANGED entry)
    data.selection.selected_base_atoms.insert(42);
    data.apply_replace(
        14, // Silicon
        &[BaseAtomPromotionInfo {
            base_id: 42,
            atomic_number: 6,
            position: DVec3::new(0.0, 0.0, 0.0),
            existing_diff_id: Some(unch_a),
        }],
    );

    // Bond should still exist between unch_a (now Si) and unch_b
    let atom_a = data.diff.get_atom(unch_a).unwrap();
    assert_eq!(atom_a.atomic_number, 14);
    let bonds: Vec<_> = atom_a.bonds.iter().collect();
    assert_eq!(bonds.len(), 1);
    assert_eq!(bonds[0].other_atom_id(), unch_b);
}

// =============================================================================
// Backward compatibility: old identity entries still work
// =============================================================================

#[test]
fn test_old_identity_entries_still_work() {
    // A diff using real atomic numbers (not UNCHANGED) should still work
    // via the replacement path in apply_diff.
    let mut base = AtomicStructure::new();
    let a = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = base.add_atom(7, DVec3::new(1.5, 0.0, 0.0));

    let mut diff = AtomicStructure::new_diff();
    let da = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Real C (old-style identity)
    let db = diff.add_atom(7, DVec3::new(1.5, 0.0, 0.0)); // Real N (old-style identity)
    diff.add_bond(da, db, BOND_SINGLE);

    let result = apply_diff(&base, &diff, 0.1);

    let ra = *result.provenance.base_to_result.get(&a).unwrap();
    let rb = *result.provenance.base_to_result.get(&b).unwrap();
    assert!(result.result.has_bond_between(ra, rb));
    assert_eq!(result.result.get_atom(ra).unwrap().atomic_number, 6);
    assert_eq!(result.result.get_atom(rb).unwrap().atomic_number, 7);
}
