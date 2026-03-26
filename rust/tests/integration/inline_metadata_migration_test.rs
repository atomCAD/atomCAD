// Backward-compatibility fixture test for the inline atom metadata migration.
//
// This test loads a .cnnd file saved with the OLD serialization format that has
// per-atom metadata in external maps (frozen_base_atoms, frozen_diff_atoms,
// hybridization_override_base_atoms, hybridization_override_diff_atoms).
//
// Before the migration: verifies the old fields load into the current maps.
// After the migration:  verifies the loader applies old fields to diff atom flags.

use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;

const FIXTURE_DIR: &str = "tests/fixtures/inline_metadata_migration";

/// Load the old-format fixture and verify that the backward-compat migration
/// applies old map entries to inline diff atom flags.
///
/// Fixture contents:
///   - frozen_base_atoms: [5]
///   - frozen_diff_atoms: [2]
///   - hybridization_override_base_atoms: [{atom_id: 10, hybridization: 2 (Sp2)}]
///   - hybridization_override_diff_atoms: [{atom_id: 1, hybridization: 1 (Sp3)}]
///
/// Diff atoms:
///   - ID 1: C at (0.89175, 2.67525, 2.67525) — base-matched replacement
///   - ID 2: N at (5, 5, 5) — pure addition
///   - ID 3: O at (6, 6, 6) — pure addition
#[test]
fn load_old_format_metadata_maps() {
    let mut registry = NodeTypeRegistry::new();
    let _load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/old_format_metadata.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load old_format_metadata.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Find the atom_edit node
    let atom_edit_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "atom_edit")
        .expect("atom_edit node missing");

    let data = atom_edit_node
        .data
        .as_any_ref()
        .downcast_ref::<AtomEditData>()
        .expect("Failed to downcast to AtomEditData");

    // Verify the diff has 3 atoms
    assert_eq!(data.diff.get_num_of_atoms(), 3, "Expected 3 diff atoms");

    // Backward-compat migration applies old map entries to inline diff atom flags.
    // Base-atom overrides (frozen_base_atoms, hybridization_override_base_atoms) are
    // ignored during migration because promotion requires the base structure which is
    // unavailable at load time.

    // Verify frozen_diff_atoms: diff atom 2 (N addition) has frozen flag set inline
    assert!(
        data.diff.get_atom(2).unwrap().is_frozen(),
        "Diff atom 2 should be frozen (migrated from frozen_diff_atoms)"
    );
    // Diff atoms 1 and 3 should NOT be frozen
    assert!(
        !data.diff.get_atom(1).unwrap().is_frozen(),
        "Diff atom 1 should not be frozen"
    );
    assert!(
        !data.diff.get_atom(3).unwrap().is_frozen(),
        "Diff atom 3 should not be frozen"
    );

    // Verify hybridization_override_diff_atoms: diff atom 1 has Sp3 (value 1) inline
    assert_eq!(
        data.diff.get_atom(1).unwrap().hybridization_override(),
        1,
        "Diff atom 1 should have Sp3 hybridization override (migrated from hybridization_override_diff_atoms)"
    );
    // Diff atom 2 should have no hybridization override
    assert_eq!(
        data.diff.get_atom(2).unwrap().hybridization_override(),
        0,
        "Diff atom 2 should have no hybridization override"
    );
}
