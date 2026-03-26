// Tests for inline atom metadata serialization (Phase 4).
//
// 1. Backward-compatibility fixture test: loads a .cnnd file with the OLD format
//    (external maps) and verifies migration to inline diff atom flags.
// 2. Flags roundtrip: serialize atom_edit with flags, deserialize, verify flags survived.
// 3. Atom flags persist in diff structure: bare serializable atom flags round-trip.

use glam::f64::DVec3;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::{
    atom_edit_data_to_serializable, serializable_to_atom_edit_data,
};
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

/// Create an atom_edit with frozen + hybridization flags on diff atoms.
/// Serialize to SerializableAtomEditData, deserialize back. Verify flags survived.
#[test]
fn flags_roundtrip_serialization() {
    let mut data = AtomEditData::new();

    // Add diff atoms with various flag combinations
    let id1 = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Carbon
    let id2 = data.diff.add_atom(7, DVec3::new(1.0, 1.0, 1.0)); // Nitrogen
    let id3 = data.diff.add_atom(8, DVec3::new(2.0, 2.0, 2.0)); // Oxygen

    // id1: frozen + Sp2
    data.diff.set_atom_frozen(id1, true);
    data.diff.set_atom_hybridization_override(id1, 2); // Sp2

    // id2: frozen only
    data.diff.set_atom_frozen(id2, true);

    // id3: Sp3 only
    data.diff.set_atom_hybridization_override(id3, 1); // Sp3

    // Serialize
    let serializable =
        atom_edit_data_to_serializable(&data).expect("Failed to serialize atom_edit_data");

    // Deserialize
    let restored = serializable_to_atom_edit_data(&serializable)
        .expect("Failed to deserialize atom_edit_data");

    // Verify flags survived the roundtrip
    let atom1 = restored.diff.get_atom(id1).expect("Atom 1 missing");
    assert!(atom1.is_frozen(), "Atom 1 should be frozen after roundtrip");
    assert_eq!(
        atom1.hybridization_override(),
        2,
        "Atom 1 should have Sp2 after roundtrip"
    );

    let atom2 = restored.diff.get_atom(id2).expect("Atom 2 missing");
    assert!(atom2.is_frozen(), "Atom 2 should be frozen after roundtrip");
    assert_eq!(
        atom2.hybridization_override(),
        0,
        "Atom 2 should have Auto hybridization after roundtrip"
    );

    let atom3 = restored.diff.get_atom(id3).expect("Atom 3 missing");
    assert!(
        !atom3.is_frozen(),
        "Atom 3 should not be frozen after roundtrip"
    );
    assert_eq!(
        atom3.hybridization_override(),
        1,
        "Atom 3 should have Sp3 after roundtrip"
    );
}

/// Serialize/deserialize a bare AtomicStructure (via atom_edit_data wrapper)
/// with non-zero flags. Verify flags round-trip correctly.
/// Guards against Atom.flags being silently dropped by the serializer.
#[test]
fn atom_flags_persist_in_diff_structure() {
    let mut data = AtomEditData::new();

    // Add atoms with every flag combination
    let id1 = data.diff.add_atom(6, DVec3::ZERO);
    let id2 = data.diff.add_atom(14, DVec3::X);
    let id3 = data.diff.add_atom(32, DVec3::Y);
    let id4 = data.diff.add_atom(7, DVec3::Z);

    // id1: all flags (frozen + Sp1 + H passivation)
    data.diff.set_atom_frozen(id1, true);
    data.diff.set_atom_hybridization_override(id1, 3); // Sp1
    data.diff.set_atom_hydrogen_passivation(id1, true);

    // id2: no flags (zero)
    // id3: only H passivation
    data.diff.set_atom_hydrogen_passivation(id3, true);

    // id4: only hybridization Sp2
    data.diff.set_atom_hybridization_override(id4, 2); // Sp2

    // Roundtrip through serialization
    let serializable = atom_edit_data_to_serializable(&data).expect("Failed to serialize");
    let restored = serializable_to_atom_edit_data(&serializable).expect("Failed to deserialize");

    // Verify all flag combinations survived
    let a1 = restored.diff.get_atom(id1).unwrap();
    assert!(a1.is_frozen(), "Atom 1: frozen should persist");
    assert_eq!(a1.hybridization_override(), 3, "Atom 1: Sp1 should persist");
    assert!(
        a1.is_hydrogen_passivation(),
        "Atom 1: H passivation should persist"
    );

    let a2 = restored.diff.get_atom(id2).unwrap();
    assert!(!a2.is_frozen(), "Atom 2: should not be frozen");
    assert_eq!(a2.hybridization_override(), 0, "Atom 2: should be Auto");
    assert!(
        !a2.is_hydrogen_passivation(),
        "Atom 2: should not have H passivation"
    );

    let a3 = restored.diff.get_atom(id3).unwrap();
    assert!(!a3.is_frozen(), "Atom 3: should not be frozen");
    assert_eq!(a3.hybridization_override(), 0, "Atom 3: should be Auto");
    assert!(
        a3.is_hydrogen_passivation(),
        "Atom 3: H passivation should persist"
    );

    let a4 = restored.diff.get_atom(id4).unwrap();
    assert!(!a4.is_frozen(), "Atom 4: should not be frozen");
    assert_eq!(a4.hybridization_override(), 2, "Atom 4: Sp2 should persist");
    assert!(
        !a4.is_hydrogen_passivation(),
        "Atom 4: should not have H passivation"
    );
}
