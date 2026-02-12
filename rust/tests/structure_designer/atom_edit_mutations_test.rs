use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_SINGLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, BondReference, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, BondDeletionInfo, DiffAtomKind, classify_diff_atom,
};
use rust_lib_flutter_cad::util::transform::Transform;

// =============================================================================
// classify_diff_atom tests
// =============================================================================

#[test]
fn test_classify_delete_marker() {
    let mut diff = AtomicStructure::new_diff();
    let id = diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(classify_diff_atom(&diff, id), DiffAtomKind::DeleteMarker);
}

#[test]
fn test_classify_matched_base() {
    let mut diff = AtomicStructure::new_diff();
    let id = diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0));
    diff.set_anchor_position(id, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(classify_diff_atom(&diff, id), DiffAtomKind::MatchedBase);
}

#[test]
fn test_classify_pure_addition() {
    let mut diff = AtomicStructure::new_diff();
    let id = diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(classify_diff_atom(&diff, id), DiffAtomKind::PureAddition);
}

#[test]
fn test_classify_nonexistent_atom() {
    let diff = AtomicStructure::new_diff();
    assert_eq!(classify_diff_atom(&diff, 999), DiffAtomKind::PureAddition);
}

// =============================================================================
// convert_to_delete_marker tests
// =============================================================================

#[test]
fn test_convert_normal_atom_to_delete_marker() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));

    data.convert_to_delete_marker(id);

    // Old atom removed, new delete marker exists
    assert!(data.diff.get_atom(id).is_none());
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    // The new atom should be a delete marker at (1, 0, 0)
    let (_, atom) = data.diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!((atom.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
}

#[test]
fn test_convert_moved_atom_to_delete_marker() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    data.diff.set_anchor_position(id, DVec3::new(1.0, 0.0, 0.0));

    data.convert_to_delete_marker(id);

    // Delete marker should be at the anchor position (1, 0, 0), not (2, 0, 0)
    assert!(data.diff.get_atom(id).is_none());
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (_, atom) = data.diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!((atom.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
}

#[test]
fn test_convert_nonexistent_atom_is_noop() {
    let mut data = AtomEditData::new();
    data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));

    data.convert_to_delete_marker(999);

    // Nothing changed
    assert_eq!(data.diff.get_num_of_atoms(), 1);
}

// =============================================================================
// apply_delete_diff_view tests
// =============================================================================

#[test]
fn test_delete_diff_view_removes_delete_marker() {
    let mut data = AtomEditData::new();
    let id = data
        .diff
        .add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);

    data.apply_delete_diff_view(&[(id, DiffAtomKind::DeleteMarker)], &[]);

    assert_eq!(data.diff.get_num_of_atoms(), 0);
    assert!(data.selection.selected_diff_atoms.is_empty());
}

#[test]
fn test_delete_diff_view_converts_matched_to_delete_marker() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    data.diff.set_anchor_position(id, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);

    data.apply_delete_diff_view(&[(id, DiffAtomKind::MatchedBase)], &[]);

    // Original atom removed, replaced by delete marker at anchor position
    assert!(data.diff.get_atom(id).is_none());
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (_, atom) = data.diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!((atom.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
    assert!(data.selection.selected_diff_atoms.is_empty());
}

#[test]
fn test_delete_diff_view_removes_pure_addition() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);

    data.apply_delete_diff_view(&[(id, DiffAtomKind::PureAddition)], &[]);

    assert_eq!(data.diff.get_num_of_atoms(), 0);
    assert!(data.selection.selected_diff_atoms.is_empty());
}

#[test]
fn test_delete_diff_view_removes_bond_delete_marker() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    data.diff.add_bond(id_a, id_b, BOND_DELETED);

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };
    data.selection.selected_bonds.insert(bond_ref.clone());

    data.apply_delete_diff_view(&[], &[bond_ref]);

    // Bond removed, atoms still present
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    let atom_a = data.diff.get_atom(id_a).unwrap();
    assert!(atom_a.bonds.is_empty());
    assert!(data.selection.selected_bonds.is_empty());
}

#[test]
fn test_delete_diff_view_removes_normal_bond() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    data.diff.add_bond(id_a, id_b, BOND_SINGLE);

    let bond_ref = BondReference {
        atom_id1: id_a,
        atom_id2: id_b,
    };

    data.apply_delete_diff_view(&[], &[bond_ref]);

    // Bond removed, atoms still present
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    let atom_a = data.diff.get_atom(id_a).unwrap();
    assert!(atom_a.bonds.is_empty());
}

#[test]
fn test_delete_diff_view_mixed() {
    let mut data = AtomEditData::new();

    // A delete marker
    let del_id = data
        .diff
        .add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(del_id);

    // A pure addition
    let add_id = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(add_id);

    // Two atoms with a bond delete marker
    let bond_a = data.diff.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    let bond_b = data.diff.add_atom(6, DVec3::new(4.5, 0.0, 0.0));
    data.diff.add_bond(bond_a, bond_b, BOND_DELETED);
    let bond_ref = BondReference {
        atom_id1: bond_a,
        atom_id2: bond_b,
    };
    data.selection.selected_bonds.insert(bond_ref.clone());

    data.apply_delete_diff_view(
        &[
            (del_id, DiffAtomKind::DeleteMarker),
            (add_id, DiffAtomKind::PureAddition),
        ],
        &[bond_ref],
    );

    // Delete marker removed, addition removed, bond removed
    assert!(data.diff.get_atom(del_id).is_none());
    assert!(data.diff.get_atom(add_id).is_none());
    // Bond atoms still present but bond removed
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    let atom_a = data.diff.get_atom(bond_a).unwrap();
    assert!(atom_a.bonds.is_empty());
    assert!(data.selection.is_empty());
}

// =============================================================================
// apply_delete_result_view tests
// =============================================================================

#[test]
fn test_delete_result_view_base_atom_creates_delete_marker() {
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(42);

    let pos = DVec3::new(1.0, 2.0, 3.0);
    data.apply_delete_result_view(&[(42, pos)], &[], &[]);

    // Diff now contains a delete marker
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (_, atom) = data.diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!((atom.position - pos).length() < 1e-10);
    assert!(!data.selection.selected_base_atoms.contains(&42));
}

#[test]
fn test_delete_result_view_pure_addition_removes_from_diff() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);

    data.apply_delete_result_view(&[], &[(id, true)], &[]);

    assert_eq!(data.diff.get_num_of_atoms(), 0);
    assert!(!data.selection.selected_diff_atoms.contains(&id));
}

#[test]
fn test_delete_result_view_matched_atom_becomes_delete_marker() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(14, DVec3::new(1.0, 0.0, 0.0));
    data.diff.set_anchor_position(id, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);

    data.apply_delete_result_view(&[], &[(id, false)], &[]);

    // Original replaced by delete marker at anchor position
    assert!(data.diff.get_atom(id).is_none());
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (_, atom) = data.diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!(!data.selection.selected_diff_atoms.contains(&id));
}

#[test]
fn test_delete_result_view_bond_adds_delete_marker() {
    let mut data = AtomEditData::new();
    let id_a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let id_b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    // No bond yet between them

    let bond_info = BondDeletionInfo {
        diff_id_a: Some(id_a),
        diff_id_b: Some(id_b),
        identity_a: None,
        identity_b: None,
    };

    data.apply_delete_result_view(&[], &[], &[bond_info]);

    // Bond delete marker should be added
    let atom_a = data.diff.get_atom(id_a).unwrap();
    assert_eq!(atom_a.bonds.len(), 1);
    assert_eq!(atom_a.bonds[0].bond_order(), BOND_DELETED);
}

#[test]
fn test_delete_result_view_bond_creates_identity_entries() {
    let mut data = AtomEditData::new();
    // No atoms in diff yet — bond endpoints need identity entries

    let bond_info = BondDeletionInfo {
        diff_id_a: None,
        diff_id_b: None,
        identity_a: Some((6, DVec3::new(0.0, 0.0, 0.0))),
        identity_b: Some((6, DVec3::new(1.5, 0.0, 0.0))),
    };

    data.apply_delete_result_view(&[], &[], &[bond_info]);

    // Two identity atoms created plus bond delete marker between them
    assert_eq!(data.diff.get_num_of_atoms(), 2);
    let mut found_bond = false;
    for (_, atom) in data.diff.iter_atoms() {
        for bond in &atom.bonds {
            if bond.bond_order() == BOND_DELETED {
                found_bond = true;
            }
        }
    }
    assert!(found_bond);
}

// =============================================================================
// apply_replace tests
// =============================================================================

#[test]
fn test_replace_diff_atoms() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0)); // Carbon
    data.selection.selected_diff_atoms.insert(id);

    data.apply_replace(14, &[]); // Replace with Silicon

    let atom = data.diff.get_atom(id).unwrap();
    assert_eq!(atom.atomic_number, 14);
    // Selection unchanged
    assert!(data.selection.selected_diff_atoms.contains(&id));
}

#[test]
fn test_replace_base_atoms() {
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(42);

    let pos = DVec3::new(1.0, 2.0, 3.0);
    data.apply_replace(14, &[(42, pos)]);

    // Base atom removed from selection, new diff atom created
    assert!(!data.selection.selected_base_atoms.contains(&42));
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (new_id, atom) = data.diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 14);
    assert!((atom.position - pos).length() < 1e-10);
    assert!(data.selection.selected_diff_atoms.contains(new_id));
}

#[test]
fn test_replace_delete_marker_in_diff_view() {
    let mut data = AtomEditData::new();
    let id = data
        .diff
        .add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);

    data.apply_replace(14, &[]); // Replace delete marker with Silicon

    let atom = data.diff.get_atom(id).unwrap();
    assert_eq!(atom.atomic_number, 14); // Revived as Silicon
}

// =============================================================================
// apply_transform tests
// =============================================================================

#[test]
fn test_transform_diff_atoms() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);
    data.selection.selection_transform = Some(Transform::new(
        DVec3::new(1.0, 0.0, 0.0),
        glam::f64::DQuat::IDENTITY,
    ));

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(&relative, &[]);

    let atom = data.diff.get_atom(id).unwrap();
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
}

#[test]
fn test_transform_base_atoms_creates_anchors() {
    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(42);
    data.selection.selection_transform = Some(Transform::default());

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(&relative, &[(42, 6, DVec3::new(1.0, 0.0, 0.0))]);

    // Base atom removed from selection
    assert!(!data.selection.selected_base_atoms.contains(&42));
    // New diff atom created at transformed position with anchor at old position
    assert_eq!(data.diff.get_num_of_atoms(), 1);
    let (new_id, atom) = data.diff.iter_atoms().next().unwrap();
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
    let anchor = data.diff.anchor_position(*new_id).unwrap();
    assert!((anchor - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
    assert!(data.selection.selected_diff_atoms.contains(new_id));
}

#[test]
fn test_transform_preserves_existing_anchor() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    data.diff.set_anchor_position(id, DVec3::new(0.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);
    data.selection.selection_transform = Some(Transform::new(
        DVec3::new(2.0, 0.0, 0.0),
        glam::f64::DQuat::IDENTITY,
    ));

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(&relative, &[]);

    let atom = data.diff.get_atom(id).unwrap();
    assert!((atom.position - DVec3::new(3.0, 0.0, 0.0)).length() < 1e-10);
    // Anchor stays at original position
    let anchor = data.diff.anchor_position(id).unwrap();
    assert!((anchor - DVec3::new(0.0, 0.0, 0.0)).length() < 1e-10);
}

#[test]
fn test_transform_updates_selection_transform() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);
    let initial_transform = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.selection.selection_transform = Some(initial_transform.clone());

    let relative = Transform::new(DVec3::new(2.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(&relative, &[]);

    let updated = data.selection.selection_transform.as_ref().unwrap();
    // Selection transform should be updated algebraically
    let expected = initial_transform.apply_to_new(&relative);
    assert!((updated.translation - expected.translation).length() < 1e-10);
}

#[test]
fn test_transform_clears_bond_selection() {
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0));
    data.selection.selected_diff_atoms.insert(id);
    data.selection.selection_transform = Some(Transform::default());
    data.selection.selected_bonds.insert(BondReference {
        atom_id1: 0,
        atom_id2: 1,
    });

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(&relative, &[]);

    assert!(data.selection.selected_bonds.is_empty());
}
