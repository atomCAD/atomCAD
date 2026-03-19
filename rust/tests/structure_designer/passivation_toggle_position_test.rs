/// Tests for issue #205: toggling hydrogen passivation off should not change
/// non-hydrogen atom positions in the atom_edit node output.
///
/// These tests cover all identified code paths where toggling passivation could
/// cause position changes for non-H atoms.
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::atomic_structure_diff::apply_diff;
use rust_lib_flutter_cad::crystolecule::hydrogen_passivation::{AddHydrogensOptions, add_hydrogens};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    add_hydrogen_atom_edit, remove_hydrogen_atom_edit,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_atomic_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    structure: AtomicStructure,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_data = Box::new(ValueData {
        value: NetworkResult::Atomic(structure),
    });
    network.add_node("value", position, 0, value_data)
}

fn do_full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn get_selected_atomic_structure(designer: &StructureDesigner) -> &AtomicStructure {
    designer
        .get_atomic_structure_from_selected_node()
        .expect("No atomic structure from selected node")
}

/// Build a simple diamond-like structure: 3 carbon atoms in a chain with H passivation.
/// Returns (structure_with_H, carbon_positions).
fn build_passivated_chain() -> (AtomicStructure, Vec<DVec3>) {
    let mut s = AtomicStructure::new();

    // Three carbon atoms in a chain along X axis
    let c1_pos = DVec3::new(0.0, 0.0, 0.0);
    let c2_pos = DVec3::new(1.54, 0.0, 0.0);
    let c3_pos = DVec3::new(3.08, 0.0, 0.0);

    let c1 = s.add_atom(6, c1_pos);
    let c2 = s.add_atom(6, c2_pos);
    let c3 = s.add_atom(6, c3_pos);

    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, c3, BOND_SINGLE);

    // Add H passivation
    let options = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: false,
    };
    add_hydrogens(&mut s, &options);

    (s, vec![c1_pos, c2_pos, c3_pos])
}

/// Build the same chain without H passivation (simulating passivation toggled OFF).
fn build_unpassivated_chain() -> (AtomicStructure, Vec<DVec3>) {
    let mut s = AtomicStructure::new();

    let c1_pos = DVec3::new(0.0, 0.0, 0.0);
    let c2_pos = DVec3::new(1.54, 0.0, 0.0);
    let c3_pos = DVec3::new(3.08, 0.0, 0.0);

    let c1 = s.add_atom(6, c1_pos);
    let c2 = s.add_atom(6, c2_pos);
    let c3 = s.add_atom(6, c3_pos);

    s.add_bond(c1, c2, BOND_SINGLE);
    s.add_bond(c2, c3, BOND_SINGLE);

    (s, vec![c1_pos, c2_pos, c3_pos])
}

/// Collect positions of non-hydrogen atoms from a structure, sorted by position for comparison.
fn get_non_h_positions(structure: &AtomicStructure) -> Vec<DVec3> {
    let mut positions: Vec<DVec3> = structure
        .atom_ids()
        .filter_map(|&id| {
            let atom = structure.get_atom(id)?;
            if atom.atomic_number > 1 {
                Some(atom.position)
            } else {
                None
            }
        })
        .collect();
    positions.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap()
            .then(a.y.partial_cmp(&b.y).unwrap())
            .then(a.z.partial_cmp(&b.z).unwrap())
    });
    positions
}

/// Assert two sets of positions match within tolerance.
fn assert_positions_match(actual: &[DVec3], expected: &[DVec3], tolerance: f64, context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{}: different number of positions ({} vs {})",
        context,
        actual.len(),
        expected.len()
    );
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        let dist = a.distance(*e);
        assert!(
            dist < tolerance,
            "{}: position {} differs: actual {:?} vs expected {:?} (distance {})",
            context,
            i,
            a,
            e,
            dist
        );
    }
}

// ============================================================================
// Code Path 1: Upstream passivation toggle
// ============================================================================

/// Test: user adds atoms in atom_edit, then upstream atom_fill toggles passivation OFF.
/// The user-added atoms should keep their positions.
#[test]
fn test_upstream_passivation_toggle_preserves_added_atom_positions() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Base: passivated chain (simulating atom_fill with passivation ON)
    let (passivated_base, _c_positions) = build_passivated_chain();

    let value_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, passivated_base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add a new interior atom in the atom_edit diff
    let interior_pos = DVec3::new(1.54, 2.0, 0.0); // Above C2, away from H
    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        data.add_atom_to_diff(6, interior_pos);
    }
    do_full_refresh(&mut designer);

    // Verify the interior atom is in the result
    let result_before = get_selected_atomic_structure(&designer);
    let positions_before = get_non_h_positions(result_before);
    let has_interior = positions_before.iter().any(|p| p.distance(interior_pos) < 0.01);
    assert!(has_interior, "Interior atom should be in result before toggle");

    // Now simulate toggling passivation OFF: replace the base with unpassivated chain
    let (unpassivated_base, _c_positions) = build_unpassivated_chain();

    // Replace the value node's data
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(
            value_id,
            Box::new(ValueData {
                value: NetworkResult::Atomic(unpassivated_base),
            }),
        );
    }
    do_full_refresh(&mut designer);

    // The interior atom should still be at the same position
    let result_after = get_selected_atomic_structure(&designer);
    let positions_after = get_non_h_positions(result_after);
    let has_interior_after = positions_after.iter().any(|p| p.distance(interior_pos) < 0.01);
    assert!(
        has_interior_after,
        "Interior atom at {:?} should still be in result after passivation toggle. \
         Non-H positions: {:?}",
        interior_pos, positions_after
    );
}

/// Test: atom_edit has multiple user-added atoms. Upstream base changes from passivated
/// to unpassivated. All user-added atom positions must be preserved.
#[test]
fn test_upstream_passivation_toggle_preserves_multiple_added_atoms() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let (passivated_base, _) = build_passivated_chain();
    let value_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, passivated_base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add 3 interior atoms at various positions far from H atoms
    let added_positions = vec![
        DVec3::new(0.77, 3.0, 0.0),
        DVec3::new(1.54, 3.0, 1.0),
        DVec3::new(2.31, 3.0, -1.0),
    ];

    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        for pos in &added_positions {
            data.add_atom_to_diff(6, *pos);
        }
    }
    do_full_refresh(&mut designer);

    // Switch base to unpassivated
    let (unpassivated_base, _) = build_unpassivated_chain();
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(
            value_id,
            Box::new(ValueData {
                value: NetworkResult::Atomic(unpassivated_base),
            }),
        );
    }
    do_full_refresh(&mut designer);

    // All added atoms should be present at original positions
    let result = get_selected_atomic_structure(&designer);
    for expected_pos in &added_positions {
        let found = result.atom_ids().any(|&id| {
            let atom = result.get_atom(id).unwrap();
            atom.position.distance(*expected_pos) < 0.01
        });
        assert!(
            found,
            "Added atom at {:?} not found after passivation toggle",
            expected_pos
        );
    }
}

// ============================================================================
// Code Path 2: atom_edit remove_hydrogen then verify positions
// ============================================================================

/// Test: add H in atom_edit, then remove H. Non-H atom positions must not change.
#[test]
fn test_atom_edit_add_then_remove_hydrogen_preserves_positions() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    // Base: unpassivated chain
    let (base, c_positions) = build_unpassivated_chain();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add interior atom
    let interior_pos = DVec3::new(1.54, 2.5, 0.0);
    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        data.add_atom_to_diff(6, interior_pos);
    }
    do_full_refresh(&mut designer);

    // Record positions before H operations
    let positions_before = get_non_h_positions(get_selected_atomic_structure(&designer));

    // Add hydrogen
    add_hydrogen_atom_edit(&mut designer, false).expect("add H failed");
    do_full_refresh(&mut designer);

    // Remove hydrogen
    remove_hydrogen_atom_edit(&mut designer, false).expect("remove H failed");
    do_full_refresh(&mut designer);

    // Non-H positions should match
    let positions_after = get_non_h_positions(get_selected_atomic_structure(&designer));
    assert_positions_match(
        &positions_after,
        &positions_before,
        0.01,
        "Positions after add+remove H cycle",
    );
}

/// Test: add H, remove H, add H again. Positions should be stable through multiple cycles.
#[test]
fn test_multiple_hydrogen_toggle_cycles_preserve_positions() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let (base, _) = build_unpassivated_chain();
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add interior atoms
    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        data.add_atom_to_diff(6, DVec3::new(0.77, 3.0, 0.0));
        data.add_atom_to_diff(6, DVec3::new(2.31, 3.0, 0.0));
    }
    do_full_refresh(&mut designer);

    let positions_initial = get_non_h_positions(get_selected_atomic_structure(&designer));

    // Cycle 1: add H, remove H
    add_hydrogen_atom_edit(&mut designer, false).expect("add H 1");
    do_full_refresh(&mut designer);
    remove_hydrogen_atom_edit(&mut designer, false).expect("remove H 1");
    do_full_refresh(&mut designer);

    let positions_after_cycle1 = get_non_h_positions(get_selected_atomic_structure(&designer));
    assert_positions_match(
        &positions_after_cycle1,
        &positions_initial,
        0.01,
        "After cycle 1",
    );

    // Cycle 2: add H, remove H
    add_hydrogen_atom_edit(&mut designer, false).expect("add H 2");
    do_full_refresh(&mut designer);
    remove_hydrogen_atom_edit(&mut designer, false).expect("remove H 2");
    do_full_refresh(&mut designer);

    let positions_after_cycle2 = get_non_h_positions(get_selected_atomic_structure(&designer));
    assert_positions_match(
        &positions_after_cycle2,
        &positions_initial,
        0.01,
        "After cycle 2",
    );
}

// ============================================================================
// Code Path 3: hydrogen_passivation flag preservation in apply_diff
// ============================================================================

/// Test: apply_diff should preserve the hydrogen_passivation flag on result atoms.
#[test]
fn test_apply_diff_preserves_hydrogen_passivation_flag() {
    // Create base structure with passivation flags
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let h1 = base.add_atom(1, DVec3::new(1.09, 0.0, 0.0));
    base.add_bond(c1, h1, BOND_SINGLE);
    base.set_atom_hydrogen_passivation(h1, true);

    // Empty diff
    let diff = AtomicStructure::new_diff();

    let result = apply_diff(&base, &diff, 0.1);

    // Find the H atom in the result
    let h_result = result
        .result
        .atom_ids()
        .find(|&&id| {
            result
                .result
                .get_atom(id)
                .map_or(false, |a| a.atomic_number == 1)
        })
        .copied()
        .expect("H atom should be in result");

    // The hydrogen_passivation flag should be preserved
    let h_atom = result.result.get_atom(h_result).unwrap();
    assert!(
        h_atom.is_hydrogen_passivation(),
        "hydrogen_passivation flag should be preserved through apply_diff for base passthrough atoms"
    );
}

/// Test: apply_diff should preserve hydrogen_passivation flag for UNCHANGED markers.
#[test]
fn test_apply_diff_preserves_flag_for_unchanged_marker() {
    let mut base = AtomicStructure::new();
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let h1 = base.add_atom(1, DVec3::new(1.09, 0.0, 0.0));
    base.add_bond(c1, h1, BOND_SINGLE);
    base.set_atom_hydrogen_passivation(h1, true);

    // Diff with UNCHANGED marker at H position (tracking the H atom)
    let mut diff = AtomicStructure::new_diff();
    let unchanged_id = diff.add_atom(
        rust_lib_flutter_cad::crystolecule::atomic_structure::UNCHANGED_ATOMIC_NUMBER,
        DVec3::new(1.09, 0.0, 0.0),
    );
    diff.set_anchor_position(unchanged_id, DVec3::new(1.09, 0.0, 0.0));

    let result = apply_diff(&base, &diff, 0.1);

    // Find the H atom in the result
    let h_result = result
        .result
        .atom_ids()
        .find(|&&id| {
            result
                .result
                .get_atom(id)
                .map_or(false, |a| a.atomic_number == 1)
        })
        .copied()
        .expect("H atom should be in result");

    let h_atom = result.result.get_atom(h_result).unwrap();
    assert!(
        h_atom.is_hydrogen_passivation(),
        "hydrogen_passivation flag should be preserved for UNCHANGED-matched base atoms"
    );
}

// ============================================================================
// Code Path 4: Base change with diff atoms near base atom positions
// ============================================================================

/// Test: when base structure changes (H removed), diff atoms that were pure additions
/// should not accidentally match base atoms they weren't intended to match.
#[test]
fn test_base_change_does_not_cause_spurious_matching() {
    // Base v1: C chain with H passivation
    let (base_v1, _) = build_passivated_chain();

    // Diff: add an atom at a position that is NOT near any base atom
    let mut diff = AtomicStructure::new_diff();
    let added_pos = DVec3::new(1.54, 5.0, 0.0); // 5 A above C2
    diff.add_atom(6, added_pos);

    // Apply diff to base_v1 (with H)
    let result_v1 = apply_diff(&base_v1, &diff, 0.1);

    // Find the added atom in result
    let added_in_v1 = result_v1
        .result
        .atom_ids()
        .find(|&&id| {
            result_v1
                .result
                .get_atom(id)
                .map_or(false, |a| a.position.distance(added_pos) < 0.01)
        })
        .copied()
        .expect("Added atom should be in result v1");
    let v1_pos = result_v1.result.get_atom(added_in_v1).unwrap().position;

    // Base v2: same C chain without H passivation
    let (base_v2, _) = build_unpassivated_chain();

    // Apply same diff to base_v2 (without H)
    let result_v2 = apply_diff(&base_v2, &diff, 0.1);

    // The added atom should still be at the same position
    let added_in_v2 = result_v2
        .result
        .atom_ids()
        .find(|&&id| {
            result_v2
                .result
                .get_atom(id)
                .map_or(false, |a| a.position.distance(added_pos) < 0.01)
        })
        .copied()
        .expect("Added atom should be in result v2");
    let v2_pos = result_v2.result.get_atom(added_in_v2).unwrap().position;

    assert!(
        v1_pos.distance(v2_pos) < 0.001,
        "Added atom position should be unchanged: {:?} vs {:?}",
        v1_pos,
        v2_pos
    );
}

/// Test: base with H, diff has additions + H-related entries. Toggling passivation OFF
/// should not change the position of the pure additions.
#[test]
fn test_mixed_diff_with_base_passivation_toggle() {
    let (base_with_h, _c_positions) = build_passivated_chain();

    // Count H atoms in base
    let h_count = base_with_h
        .atom_ids()
        .filter(|&&id| base_with_h.get_atom(id).map_or(false, |a| a.atomic_number == 1))
        .count();
    assert!(h_count > 0, "Base should have H atoms");

    // Create diff with:
    // 1. Delete markers for some H atoms
    // 2. A pure addition atom
    let mut diff = AtomicStructure::new_diff();

    // Add a delete marker at first H atom position
    let first_h_pos = base_with_h
        .atom_ids()
        .find_map(|&id| {
            let a = base_with_h.get_atom(id)?;
            if a.atomic_number == 1 {
                Some(a.position)
            } else {
                None
            }
        })
        .unwrap();

    diff.add_atom(
        rust_lib_flutter_cad::crystolecule::atomic_structure::DELETED_SITE_ATOMIC_NUMBER,
        first_h_pos,
    );

    // Add pure addition atom far from any base atom
    let added_pos = DVec3::new(1.54, 10.0, 0.0);
    diff.add_atom(14, added_pos); // Si atom at y=10

    // Apply to base with H
    let result_with_h = apply_diff(&base_with_h, &diff, 0.1);
    let si_pos_with_h = result_with_h
        .result
        .atom_ids()
        .find_map(|&id| {
            let a = result_with_h.result.get_atom(id)?;
            if a.atomic_number == 14 {
                Some(a.position)
            } else {
                None
            }
        })
        .expect("Si atom should be in result with H");

    // Apply to base without H
    let (base_without_h, _) = build_unpassivated_chain();
    let result_without_h = apply_diff(&base_without_h, &diff, 0.1);
    let si_pos_without_h = result_without_h
        .result
        .atom_ids()
        .find_map(|&id| {
            let a = result_without_h.result.get_atom(id)?;
            if a.atomic_number == 14 {
                Some(a.position)
            } else {
                None
            }
        })
        .expect("Si atom should be in result without H");

    assert!(
        si_pos_with_h.distance(si_pos_without_h) < 0.001,
        "Si atom position should not change when base passivation is toggled: {:?} vs {:?}",
        si_pos_with_h,
        si_pos_without_h
    );
}

// ============================================================================
// Code Path: atom_edit with passivated base, add interior atoms, remove H
// ============================================================================

/// Test: atom_edit receives passivated base, user adds interior atoms,
/// then removes all H from atom_edit. Interior atom positions must not change.
#[test]
fn test_atom_edit_remove_h_from_passivated_base_preserves_interior_atoms() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let (passivated_base, _) = build_passivated_chain();
    let value_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, passivated_base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add interior atoms
    let interior_positions = vec![
        DVec3::new(0.77, 4.0, 0.0),
        DVec3::new(1.54, 4.0, 0.0),
        DVec3::new(2.31, 4.0, 0.0),
    ];
    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        for pos in &interior_positions {
            data.add_atom_to_diff(6, *pos);
        }
    }
    do_full_refresh(&mut designer);

    // Verify interior atoms exist before H removal
    let result_before = get_selected_atomic_structure(&designer);
    for expected_pos in &interior_positions {
        let found = result_before.atom_ids().any(|&id| {
            result_before
                .get_atom(id)
                .map_or(false, |a| a.position.distance(*expected_pos) < 0.01)
        });
        assert!(found, "Interior atom at {:?} should exist before H removal", expected_pos);
    }

    // Remove all hydrogen from atom_edit
    let msg = remove_hydrogen_atom_edit(&mut designer, false).expect("remove H failed");
    assert!(msg.contains("Removed"), "Should have removed some H: {}", msg);
    do_full_refresh(&mut designer);

    // All interior atoms should still be at original positions
    let result_after = get_selected_atomic_structure(&designer);
    for expected_pos in &interior_positions {
        let found = result_after.atom_ids().any(|&id| {
            result_after
                .get_atom(id)
                .map_or(false, |a| a.position.distance(*expected_pos) < 0.01)
        });
        assert!(
            found,
            "Interior atom at {:?} should still exist after H removal. \
             All non-H atoms: {:?}",
            expected_pos,
            get_non_h_positions(result_after)
        );
    }
}

// ============================================================================
// Code Path: base structure with ID gaps
// ============================================================================

/// Test: base structure has atom ID gaps (from lattice fill deleting atoms).
/// User adds interior atoms. Toggle passivation OFF. Positions preserved.
#[test]
fn test_gapped_base_passivation_toggle_preserves_positions() {
    let mut base = AtomicStructure::new();

    // C1 at origin
    let c1 = base.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id=1
    // Dummy to create gap
    let dummy = base.add_atom(6, DVec3::new(99.0, 99.0, 99.0)); // id=2
    // C2
    let c2 = base.add_atom(6, DVec3::new(1.54, 0.0, 0.0)); // id=3
    // C3
    let c3 = base.add_atom(6, DVec3::new(3.08, 0.0, 0.0)); // id=4

    base.add_bond(c1, c2, BOND_SINGLE);
    base.add_bond(c2, c3, BOND_SINGLE);
    base.delete_atom(dummy); // Create gap at id=2

    // Add H passivation
    let options = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: false,
    };
    add_hydrogens(&mut base, &options);

    // Set up network
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);
    let value_id = add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, base.clone());
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add interior atom
    let interior_pos = DVec3::new(1.54, 5.0, 0.0);
    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        data.add_atom_to_diff(6, interior_pos);
    }
    do_full_refresh(&mut designer);

    // Now create unpassivated version with same gaps
    let mut base_no_h = AtomicStructure::new();
    let c1 = base_no_h.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let dummy = base_no_h.add_atom(6, DVec3::new(99.0, 99.0, 99.0));
    let c2 = base_no_h.add_atom(6, DVec3::new(1.54, 0.0, 0.0));
    let c3 = base_no_h.add_atom(6, DVec3::new(3.08, 0.0, 0.0));
    base_no_h.add_bond(c1, c2, BOND_SINGLE);
    base_no_h.add_bond(c2, c3, BOND_SINGLE);
    base_no_h.delete_atom(dummy);

    // Switch to unpassivated base
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(
            value_id,
            Box::new(ValueData {
                value: NetworkResult::Atomic(base_no_h),
            }),
        );
    }
    do_full_refresh(&mut designer);

    // Interior atom should still be at original position
    let result = get_selected_atomic_structure(&designer);
    let found = result.atom_ids().any(|&id| {
        result
            .get_atom(id)
            .map_or(false, |a| a.position.distance(interior_pos) < 0.01)
    });
    assert!(
        found,
        "Interior atom at {:?} should still exist after passivation toggle with gapped base. \
         All non-H positions: {:?}",
        interior_pos,
        get_non_h_positions(result)
    );
}

// ============================================================================
// Regression: atom count stability through H toggle
// ============================================================================

/// Test: verify that non-H atom count does not change when H passivation is toggled.
#[test]
fn test_non_h_atom_count_stable_through_passivation_toggle() {
    let network_name = "test";
    let mut designer = setup_designer_with_network(network_name);

    let (passivated_base, _) = build_passivated_chain();
    let value_id =
        add_atomic_value_node(&mut designer, network_name, DVec2::ZERO, passivated_base);
    let atom_edit_id = designer.add_node("atom_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, atom_edit_id, 0);
    designer.select_node(atom_edit_id);
    do_full_refresh(&mut designer);

    // Add 2 interior atoms
    {
        use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::get_selected_atom_edit_data_mut;
        let data = get_selected_atom_edit_data_mut(&mut designer).unwrap();
        data.add_atom_to_diff(6, DVec3::new(0.5, 5.0, 0.0));
        data.add_atom_to_diff(6, DVec3::new(2.5, 5.0, 0.0));
    }
    do_full_refresh(&mut designer);

    let non_h_before = get_non_h_positions(get_selected_atomic_structure(&designer)).len();

    // Toggle passivation OFF
    let (unpassivated_base, _) = build_unpassivated_chain();
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.set_node_network_data(
            value_id,
            Box::new(ValueData {
                value: NetworkResult::Atomic(unpassivated_base),
            }),
        );
    }
    do_full_refresh(&mut designer);

    let non_h_after = get_non_h_positions(get_selected_atomic_structure(&designer)).len();

    assert_eq!(
        non_h_before, non_h_after,
        "Non-H atom count should be stable: {} before, {} after",
        non_h_before, non_h_after
    );
}
