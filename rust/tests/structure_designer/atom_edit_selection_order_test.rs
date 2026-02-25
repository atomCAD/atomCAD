use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditSelection, SelectionProvenance,
};

// =============================================================================
// Basic selection order tracking
// =============================================================================

#[test]
fn test_selection_order_empty_by_default() {
    let selection = AtomEditSelection::new();
    assert!(selection.selection_order.is_empty());
    assert!(selection.last_selected_atoms(5).is_empty());
}

#[test]
fn test_track_selected_appends_in_order() {
    let mut selection = AtomEditSelection::new();

    selection.selected_base_atoms.insert(10);
    selection.track_selected(SelectionProvenance::Base, 10);

    selection.selected_diff_atoms.insert(20);
    selection.track_selected(SelectionProvenance::Diff, 20);

    selection.selected_base_atoms.insert(30);
    selection.track_selected(SelectionProvenance::Base, 30);

    assert_eq!(selection.selection_order.len(), 3);
    assert_eq!(selection.selection_order[0], (SelectionProvenance::Base, 10));
    assert_eq!(selection.selection_order[1], (SelectionProvenance::Diff, 20));
    assert_eq!(selection.selection_order[2], (SelectionProvenance::Base, 30));
}

#[test]
fn test_track_selected_no_duplicate() {
    let mut selection = AtomEditSelection::new();

    selection.selected_base_atoms.insert(10);
    selection.track_selected(SelectionProvenance::Base, 10);
    selection.track_selected(SelectionProvenance::Base, 10);
    selection.track_selected(SelectionProvenance::Base, 10);

    assert_eq!(selection.selection_order.len(), 1);
}

#[test]
fn test_track_same_id_different_provenance() {
    let mut selection = AtomEditSelection::new();

    selection.selected_base_atoms.insert(5);
    selection.track_selected(SelectionProvenance::Base, 5);

    selection.selected_diff_atoms.insert(5);
    selection.track_selected(SelectionProvenance::Diff, 5);

    assert_eq!(selection.selection_order.len(), 2);
    assert_eq!(selection.selection_order[0], (SelectionProvenance::Base, 5));
    assert_eq!(selection.selection_order[1], (SelectionProvenance::Diff, 5));
}

// =============================================================================
// Untrack (toggle-off / deletion)
// =============================================================================

#[test]
fn test_untrack_selected_removes_entry() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 10);
    selection.track_selected(SelectionProvenance::Diff, 20);
    selection.track_selected(SelectionProvenance::Base, 30);

    selection.untrack_selected(SelectionProvenance::Diff, 20);

    assert_eq!(selection.selection_order.len(), 2);
    assert_eq!(selection.selection_order[0], (SelectionProvenance::Base, 10));
    assert_eq!(selection.selection_order[1], (SelectionProvenance::Base, 30));
}

#[test]
fn test_untrack_nonexistent_is_noop() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 10);
    selection.untrack_selected(SelectionProvenance::Diff, 10); // Different provenance
    selection.untrack_selected(SelectionProvenance::Base, 999); // Different ID

    assert_eq!(selection.selection_order.len(), 1);
}

// =============================================================================
// Clear
// =============================================================================

#[test]
fn test_clear_empties_selection_order() {
    let mut selection = AtomEditSelection::new();

    selection.selected_base_atoms.insert(10);
    selection.track_selected(SelectionProvenance::Base, 10);
    selection.selected_diff_atoms.insert(20);
    selection.track_selected(SelectionProvenance::Diff, 20);

    selection.clear();

    assert!(selection.selection_order.is_empty());
    assert!(selection.selected_base_atoms.is_empty());
    assert!(selection.selected_diff_atoms.is_empty());
}

// =============================================================================
// Update provenance (base → diff promotion)
// =============================================================================

#[test]
fn test_update_order_provenance() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 10);
    selection.track_selected(SelectionProvenance::Diff, 20);
    selection.track_selected(SelectionProvenance::Base, 30);

    // Simulate promoting base atom 10 to diff atom 100
    selection.update_order_provenance(
        SelectionProvenance::Base,
        10,
        SelectionProvenance::Diff,
        100,
    );

    assert_eq!(selection.selection_order.len(), 3);
    assert_eq!(
        selection.selection_order[0],
        (SelectionProvenance::Diff, 100)
    );
    assert_eq!(
        selection.selection_order[1],
        (SelectionProvenance::Diff, 20)
    );
    assert_eq!(
        selection.selection_order[2],
        (SelectionProvenance::Base, 30)
    );
}

#[test]
fn test_update_order_provenance_nonexistent_is_noop() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 10);

    selection.update_order_provenance(
        SelectionProvenance::Diff,
        10, // Wrong provenance
        SelectionProvenance::Diff,
        100,
    );

    // Order unchanged
    assert_eq!(selection.selection_order.len(), 1);
    assert_eq!(
        selection.selection_order[0],
        (SelectionProvenance::Base, 10)
    );
}

// =============================================================================
// last_selected_atoms
// =============================================================================

#[test]
fn test_last_selected_atoms_returns_tail() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 1);
    selection.track_selected(SelectionProvenance::Base, 2);
    selection.track_selected(SelectionProvenance::Diff, 3);
    selection.track_selected(SelectionProvenance::Base, 4);

    let last2 = selection.last_selected_atoms(2);
    assert_eq!(last2.len(), 2);
    assert_eq!(last2[0], (SelectionProvenance::Diff, 3));
    assert_eq!(last2[1], (SelectionProvenance::Base, 4));
}

#[test]
fn test_last_selected_atoms_count_exceeds_length() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 1);
    selection.track_selected(SelectionProvenance::Diff, 2);

    let last10 = selection.last_selected_atoms(10);
    assert_eq!(last10.len(), 2);
    assert_eq!(last10[0], (SelectionProvenance::Base, 1));
    assert_eq!(last10[1], (SelectionProvenance::Diff, 2));
}

#[test]
fn test_last_selected_atoms_zero_count() {
    let mut selection = AtomEditSelection::new();

    selection.track_selected(SelectionProvenance::Base, 1);

    let last0 = selection.last_selected_atoms(0);
    assert!(last0.is_empty());
}

// =============================================================================
// Integration: simulate click → toggle → replace → clear sequences
// =============================================================================

#[test]
fn test_click_toggle_off_removes_from_order() {
    let mut selection = AtomEditSelection::new();

    // Click atom 10 (base), atom 20 (diff), atom 30 (base)
    selection.selected_base_atoms.insert(10);
    selection.track_selected(SelectionProvenance::Base, 10);

    selection.selected_diff_atoms.insert(20);
    selection.track_selected(SelectionProvenance::Diff, 20);

    selection.selected_base_atoms.insert(30);
    selection.track_selected(SelectionProvenance::Base, 30);

    // Toggle off atom 20
    selection.selected_diff_atoms.remove(&20);
    selection.untrack_selected(SelectionProvenance::Diff, 20);

    assert_eq!(selection.selection_order.len(), 2);
    assert_eq!(
        selection.selection_order[0],
        (SelectionProvenance::Base, 10)
    );
    assert_eq!(
        selection.selection_order[1],
        (SelectionProvenance::Base, 30)
    );

    // Last selected is atom 30
    let last1 = selection.last_selected_atoms(1);
    assert_eq!(last1[0], (SelectionProvenance::Base, 30));
}

#[test]
fn test_replace_clears_then_adds() {
    let mut selection = AtomEditSelection::new();

    // Select atoms 10, 20
    selection.selected_base_atoms.insert(10);
    selection.track_selected(SelectionProvenance::Base, 10);
    selection.selected_diff_atoms.insert(20);
    selection.track_selected(SelectionProvenance::Diff, 20);

    // Replace with atom 30
    selection.clear();
    selection.selected_base_atoms.insert(30);
    selection.track_selected(SelectionProvenance::Base, 30);

    assert_eq!(selection.selection_order.len(), 1);
    assert_eq!(
        selection.selection_order[0],
        (SelectionProvenance::Base, 30)
    );
}

#[test]
fn test_marquee_sorted_order() {
    let mut selection = AtomEditSelection::new();

    // Simulate marquee selecting atoms 30, 10, 20 (but we expect them sorted by ID)
    let marquee_atoms: Vec<(SelectionProvenance, u32)> = vec![
        (SelectionProvenance::Base, 10),
        (SelectionProvenance::Diff, 20),
        (SelectionProvenance::Base, 30),
    ];

    // Marquee appends in sorted order
    for (prov, id) in &marquee_atoms {
        match prov {
            SelectionProvenance::Base => {
                selection.selected_base_atoms.insert(*id);
            }
            SelectionProvenance::Diff => {
                selection.selected_diff_atoms.insert(*id);
            }
        }
        selection.track_selected(*prov, *id);
    }

    assert_eq!(selection.selection_order.len(), 3);
    assert_eq!(
        selection.selection_order[0],
        (SelectionProvenance::Base, 10)
    );
    assert_eq!(
        selection.selection_order[1],
        (SelectionProvenance::Diff, 20)
    );
    assert_eq!(
        selection.selection_order[2],
        (SelectionProvenance::Base, 30)
    );
}

#[test]
fn test_promotion_preserves_order_position() {
    let mut selection = AtomEditSelection::new();

    // Select atoms: base 10, diff 20, base 30
    selection.track_selected(SelectionProvenance::Base, 10);
    selection.track_selected(SelectionProvenance::Diff, 20);
    selection.track_selected(SelectionProvenance::Base, 30);

    // Promote base 10 → diff 100 (e.g., drag promoted it)
    selection.update_order_provenance(
        SelectionProvenance::Base,
        10,
        SelectionProvenance::Diff,
        100,
    );

    // The promoted entry keeps its position (index 0)
    assert_eq!(
        selection.selection_order[0],
        (SelectionProvenance::Diff, 100)
    );
    assert_eq!(
        selection.selection_order[1],
        (SelectionProvenance::Diff, 20)
    );
    assert_eq!(
        selection.selection_order[2],
        (SelectionProvenance::Base, 30)
    );

    // Last selected is still base 30
    let last1 = selection.last_selected_atoms(1);
    assert_eq!(last1[0], (SelectionProvenance::Base, 30));
}
