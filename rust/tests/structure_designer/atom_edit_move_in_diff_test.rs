/// Tests for the anchor invariant in atom_edit's diff system.
///
/// The critical invariant: anchors are set ONLY at promotion time (when a base
/// atom is first added to the diff). `move_in_diff` must never set anchors.
/// Pure addition atoms (created by AddAtom, no base counterpart) must never
/// have anchors, because `apply_diff` treats anchored-but-unmatched atoms as
/// "orphaned tracked atoms" and drops them from the result.
///
/// See "Anchor Invariant" in atom_edit/AGENTS.md.
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// =============================================================================
// Helpers
// =============================================================================

fn setup_atom_edit_result_view() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);
    // output_diff defaults to false (result view), no need to set it
    designer
}

fn refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn get_data_mut(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

/// Count the number of atoms in a structure.
fn atom_count(structure: &AtomicStructure) -> usize {
    structure.iter_atoms().count()
}

/// Count the number of unique bonds in a structure (each bond counted once).
fn bond_count(structure: &AtomicStructure) -> usize {
    let mut count = 0;
    for (_, atom) in structure.iter_atoms() {
        for bond in &atom.bonds {
            // Count each bond only once (lower ID → higher ID)
            if atom.id < bond.other_atom_id() {
                count += 1;
            }
        }
    }
    count
}

// =============================================================================
// Tests: pure addition atoms survive move_in_diff + apply_diff
// =============================================================================

/// A pure addition atom (no anchor) moved with move_in_diff must appear
/// in the apply_diff result at its new position.
#[test]
fn test_pure_addition_survives_move_in_diff() {
    let base = AtomicStructure::new();
    let mut data = AtomEditData::new();

    // Add a pure addition atom (no anchor, no base counterpart)
    let diff_id = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // Move it — this must NOT set an anchor
    data.move_in_diff(diff_id, DVec3::new(2.0, 0.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    // The atom must appear in the result (not orphaned)
    assert_eq!(
        result.stats.orphaned_tracked_atoms, 0,
        "Pure addition must not become an orphaned tracked atom after move_in_diff"
    );
    assert_eq!(result.stats.atoms_added, 1);

    // Verify it's at the moved position
    let result_id = *result
        .provenance
        .diff_to_result
        .get(&diff_id)
        .expect("Moved pure addition must appear in result");
    let atom = result.result.get_atom(result_id).unwrap();
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
}

/// Multiple moves on a pure addition should all work without creating anchors.
#[test]
fn test_pure_addition_survives_multiple_moves() {
    let base = AtomicStructure::new();
    let mut data = AtomEditData::new();

    let diff_id = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    // Simulate multiple drag frames
    data.move_in_diff(diff_id, DVec3::new(1.0, 0.0, 0.0));
    data.move_in_diff(diff_id, DVec3::new(2.0, 0.0, 0.0));
    data.move_in_diff(diff_id, DVec3::new(3.0, 1.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    assert_eq!(result.stats.orphaned_tracked_atoms, 0);
    let result_id = *result.provenance.diff_to_result.get(&diff_id).unwrap();
    let atom = result.result.get_atom(result_id).unwrap();
    assert!((atom.position - DVec3::new(3.0, 1.0, 0.0)).length() < 1e-10);
}

/// A pure addition bonded to another pure addition: moving one must not
/// orphan either atom or the bond.
#[test]
fn test_bonded_pure_additions_survive_move() {
    let base = AtomicStructure::new();
    let mut data = AtomEditData::new();

    let a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    data.add_bond_in_diff(a, b, BOND_SINGLE);

    // Move atom b
    data.move_in_diff(b, DVec3::new(2.0, 0.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    assert_eq!(result.stats.orphaned_tracked_atoms, 0);
    assert_eq!(result.stats.orphaned_bonds, 0);
    assert_eq!(result.stats.atoms_added, 2);
    assert_eq!(result.stats.bonds_added, 1);
}

// =============================================================================
// Tests: promoted base atoms survive move_in_diff + apply_diff
// =============================================================================

/// A promoted base atom (with anchor) moved with move_in_diff must still
/// match its base atom and appear in the result at the new position.
#[test]
fn test_promoted_base_atom_survives_move_in_diff() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut data = AtomEditData::new();

    // Promote: add identity entry with anchor at base position
    let diff_id = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    data.diff.set_anchor_position(diff_id, DVec3::new(0.0, 0.0, 0.0));

    // Move it away from the base position
    data.move_in_diff(diff_id, DVec3::new(2.0, 0.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    // The atom must match the base atom (modified, not orphaned)
    assert_eq!(result.stats.orphaned_tracked_atoms, 0);
    assert_eq!(result.stats.atoms_modified, 1);

    let result_id = *result.provenance.diff_to_result.get(&diff_id).unwrap();
    let atom = result.result.get_atom(result_id).unwrap();
    assert!((atom.position - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-10);
}

/// A promoted base atom bonded to a pure addition: moving the pure addition
/// must not orphan either atom.
#[test]
fn test_promoted_base_with_pure_addition_bond_survives_move() {
    let mut base = AtomicStructure::new();
    base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let mut data = AtomEditData::new();

    // Promote the base atom
    let promoted = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    data.diff
        .set_anchor_position(promoted, DVec3::new(0.0, 0.0, 0.0));

    // Add a pure addition bonded to the promoted atom
    let added = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    data.add_bond_in_diff(promoted, added, BOND_SINGLE);

    // Move the pure addition
    data.move_in_diff(added, DVec3::new(2.0, 1.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    assert_eq!(
        result.stats.orphaned_tracked_atoms, 0,
        "Moving the pure addition must not orphan it"
    );
    assert_eq!(result.stats.orphaned_bonds, 0);
    // 1 base atom modified (promoted), 1 atom added (pure addition), 1 bond added
    assert_eq!(result.stats.atoms_modified, 1);
    assert_eq!(result.stats.atoms_added, 1);
    assert_eq!(result.stats.bonds_added, 1);

    // Verify positions
    let added_result = *result.provenance.diff_to_result.get(&added).unwrap();
    let atom = result.result.get_atom(added_result).unwrap();
    assert!((atom.position - DVec3::new(2.0, 1.0, 0.0)).length() < 1e-10);
}

// =============================================================================
// Tests: full evaluation in result view (no wired input = empty base)
// =============================================================================

/// Add an atom to diff, move it, evaluate in result view (output_diff=false).
/// The atom must appear in the evaluated output.
#[test]
fn test_move_in_diff_survives_result_view_evaluation() {
    let mut designer = setup_atom_edit_result_view();

    // Add atom and move it
    {
        let data = get_data_mut(&mut designer);
        let id = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        data.move_in_diff(id, DVec3::new(3.0, 0.0, 0.0));
    }

    // Evaluate — triggers apply_diff internally
    refresh(&mut designer);

    // Check the output structure has the atom
    let output = designer
        .get_atomic_structure_from_selected_node()
        .expect("Evaluation should produce an atomic structure");
    assert_eq!(
        atom_count(&output),
        1,
        "The moved pure addition must survive evaluation in result view"
    );
}

/// Add two atoms with a bond, move one, evaluate in result view.
/// Both atoms and the bond must survive.
#[test]
fn test_bonded_atoms_survive_move_in_result_view_evaluation() {
    let mut designer = setup_atom_edit_result_view();

    {
        let data = get_data_mut(&mut designer);
        let a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
        let b = data.diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
        data.add_bond_in_diff(a, b, BOND_SINGLE);
        // Move atom b
        data.move_in_diff(b, DVec3::new(2.5, 0.0, 0.0));
    }

    refresh(&mut designer);

    let output = designer
        .get_atomic_structure_from_selected_node()
        .expect("Evaluation should produce an atomic structure");
    assert_eq!(atom_count(&output), 2);
    assert_eq!(bond_count(&output), 1);
}

// =============================================================================
// Regression: verify anchored pure additions WOULD be orphaned
// =============================================================================

/// Directly verify that an anchored atom with no matching base is orphaned.
/// This is the bug that move_in_diff used to cause.
#[test]
fn test_anchored_pure_addition_is_orphaned_in_apply_diff() {
    let base = AtomicStructure::new(); // empty base

    // Use AtomEditData::new() which creates a diff with is_diff=true
    let mut data = AtomEditData::new();
    let id = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    // Manually set an anchor — this simulates the old buggy move_in_diff
    data.diff.set_anchor_position(id, DVec3::new(0.0, 0.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    // The atom should be orphaned because it has an anchor but no base match
    assert_eq!(
        result.stats.orphaned_tracked_atoms, 1,
        "An anchored atom with no matching base atom must be orphaned"
    );
    assert_eq!(
        atom_count(&result.result),
        0,
        "Orphaned atom must not appear in the result"
    );
}

/// Same scenario but with a bond — the bond becomes orphaned too.
#[test]
fn test_anchored_pure_addition_with_bond_orphans_bond() {
    let base = AtomicStructure::new();

    let mut data = AtomEditData::new();
    let a = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = data.diff.add_atom(6, DVec3::new(2.0, 0.0, 0.0));
    data.add_bond_in_diff(a, b, BOND_SINGLE);

    // Anchor only atom b (simulating the old bug where move_in_diff set anchors)
    data.diff.set_anchor_position(b, DVec3::new(1.5, 0.0, 0.0));

    let result = apply_diff(&base, &data.diff, data.tolerance);

    assert_eq!(result.stats.orphaned_tracked_atoms, 1);
    assert_eq!(result.stats.orphaned_bonds, 1);
    // Only atom a (the non-anchored one) survives
    assert_eq!(atom_count(&result.result), 1);
}
