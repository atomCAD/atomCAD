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

/// Load the old-format fixture and verify that all 4 external metadata maps
/// are populated correctly.
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

    // Verify frozen_base_atoms: base atom 5 is frozen
    assert!(
        data.frozen_base_atoms.contains(&5),
        "Base atom 5 should be frozen"
    );
    assert_eq!(data.frozen_base_atoms.len(), 1);

    // Verify frozen_diff_atoms: diff atom 2 (N addition) is frozen
    assert!(
        data.frozen_diff_atoms.contains(&2),
        "Diff atom 2 should be frozen"
    );
    assert_eq!(data.frozen_diff_atoms.len(), 1);

    // Verify hybridization_override_base_atoms: base atom 10 has Sp2 (value 2)
    assert_eq!(
        data.hybridization_override_base_atoms.get(&10),
        Some(&2),
        "Base atom 10 should have Sp2 hybridization override"
    );
    assert_eq!(data.hybridization_override_base_atoms.len(), 1);

    // Verify hybridization_override_diff_atoms: diff atom 1 has Sp3 (value 1)
    assert_eq!(
        data.hybridization_override_diff_atoms.get(&1),
        Some(&1),
        "Diff atom 1 should have Sp3 hybridization override"
    );
    assert_eq!(data.hybridization_override_diff_atoms.len(), 1);
}
