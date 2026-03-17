use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::lattice_fill::{
    CrystallographicAddress, LatticeFillConfig, LatticeFillOptions, LatticeFillStatistics,
    PlacedAtomTracker, fill_lattice,
};
use rust_lib_flutter_cad::crystolecule::motif_parser::parse_motif;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::util::daabox::DAABox;
use std::collections::HashMap;

// =============================================================================
// PlacedAtomTracker Tests
// =============================================================================

#[test]
fn test_placed_atom_tracker_new() {
    let tracker = PlacedAtomTracker::new();
    assert!(tracker.get_atom_id(IVec3::ZERO, 0).is_none());
}

#[test]
fn test_placed_atom_tracker_record_and_get() {
    let mut tracker = PlacedAtomTracker::new();

    tracker.record_atom(IVec3::new(0, 0, 0), 0, 100);
    tracker.record_atom(IVec3::new(1, 0, 0), 0, 101);
    tracker.record_atom(IVec3::new(0, 0, 0), 1, 102);

    assert_eq!(tracker.get_atom_id(IVec3::new(0, 0, 0), 0), Some(100));
    assert_eq!(tracker.get_atom_id(IVec3::new(1, 0, 0), 0), Some(101));
    assert_eq!(tracker.get_atom_id(IVec3::new(0, 0, 0), 1), Some(102));

    // Non-existent entries
    assert_eq!(tracker.get_atom_id(IVec3::new(2, 0, 0), 0), None);
    assert_eq!(tracker.get_atom_id(IVec3::new(0, 0, 0), 5), None);
}

#[test]
fn test_placed_atom_tracker_get_by_address() {
    let mut tracker = PlacedAtomTracker::new();

    let address = CrystallographicAddress::new(IVec3::new(1, 2, 3), 4);
    tracker.record_atom(IVec3::new(1, 2, 3), 4, 42);

    assert_eq!(tracker.get_atom_id_by_address(&address), Some(42));
}

#[test]
fn test_placed_atom_tracker_iter_atoms() {
    let mut tracker = PlacedAtomTracker::new();

    tracker.record_atom(IVec3::new(0, 0, 0), 0, 100);
    tracker.record_atom(IVec3::new(1, 0, 0), 0, 101);
    tracker.record_atom(IVec3::new(0, 1, 0), 0, 102);

    let atoms: Vec<_> = tracker.iter_atoms().collect();
    assert_eq!(atoms.len(), 3);

    // Check that all expected atoms are present
    let atom_ids: Vec<u32> = atoms.iter().map(|(_, id)| *id).collect();
    assert!(atom_ids.contains(&100));
    assert!(atom_ids.contains(&101));
    assert!(atom_ids.contains(&102));
}

#[test]
fn test_placed_atom_tracker_overwrite() {
    let mut tracker = PlacedAtomTracker::new();

    tracker.record_atom(IVec3::ZERO, 0, 100);
    assert_eq!(tracker.get_atom_id(IVec3::ZERO, 0), Some(100));

    // Overwrite with new ID
    tracker.record_atom(IVec3::ZERO, 0, 200);
    assert_eq!(tracker.get_atom_id(IVec3::ZERO, 0), Some(200));
}

// =============================================================================
// CrystallographicAddress Tests
// =============================================================================

#[test]
fn test_crystallographic_address_new() {
    let address = CrystallographicAddress::new(IVec3::new(1, 2, 3), 5);

    assert_eq!(address.motif_space_pos, IVec3::new(1, 2, 3));
    assert_eq!(address.site_index, 5);
}

#[test]
fn test_crystallographic_address_equality() {
    let addr1 = CrystallographicAddress::new(IVec3::new(1, 2, 3), 5);
    let addr2 = CrystallographicAddress::new(IVec3::new(1, 2, 3), 5);
    let addr3 = CrystallographicAddress::new(IVec3::new(1, 2, 3), 6);
    let addr4 = CrystallographicAddress::new(IVec3::new(0, 2, 3), 5);

    assert_eq!(addr1, addr2);
    assert_ne!(addr1, addr3);
    assert_ne!(addr1, addr4);
}

#[test]
fn test_crystallographic_address_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();

    set.insert(CrystallographicAddress::new(IVec3::new(0, 0, 0), 0));
    set.insert(CrystallographicAddress::new(IVec3::new(1, 0, 0), 0));
    set.insert(CrystallographicAddress::new(IVec3::new(0, 0, 0), 1));

    assert_eq!(set.len(), 3);

    // Duplicate should not increase size
    set.insert(CrystallographicAddress::new(IVec3::new(0, 0, 0), 0));
    assert_eq!(set.len(), 3);
}

// =============================================================================
// LatticeFillStatistics Tests
// =============================================================================

#[test]
fn test_lattice_fill_statistics_new() {
    let stats = LatticeFillStatistics::new();

    assert_eq!(stats.fill_box_calls, 0);
    assert_eq!(stats.do_fill_box_calls, 0);
    assert_eq!(stats.atoms, 0);
    assert_eq!(stats.bonds, 0);
}

#[test]
fn test_lattice_fill_statistics_average_size() {
    let mut stats = LatticeFillStatistics::new();

    // No calls - should return zero
    assert_eq!(stats.get_average_do_fill_box_size(), DVec3::ZERO);

    // Add some data
    stats.do_fill_box_calls = 2;
    stats.do_fill_box_total_size = DVec3::new(10.0, 20.0, 30.0);

    let avg = stats.get_average_do_fill_box_size();
    assert!((avg.x - 5.0).abs() < 0.001);
    assert!((avg.y - 10.0).abs() < 0.001);
    assert!((avg.z - 15.0).abs() < 0.001);
}

#[test]
fn test_lattice_fill_statistics_average_depth() {
    let mut stats = LatticeFillStatistics::new();

    // No atoms - should return zero
    assert_eq!(stats.get_average_depth(), 0.0);

    // Add some data
    stats.atoms = 4;
    stats.total_depth = 12.0;

    assert!((stats.get_average_depth() - 3.0).abs() < 0.001);
}

// =============================================================================
// Integration Tests - fill_lattice
// =============================================================================

#[test]
fn test_fill_empty_region() {
    // Create a sphere far away from fill region
    let sphere = GeoNode::sphere(DVec3::new(100.0, 100.0, 100.0), 1.0);

    let unit_cell = UnitCellStruct::cubic_diamond();

    let motif_text = "
site 1 C 0.0 0.0 0.0
";
    let motif = parse_motif(motif_text).unwrap();

    let config = LatticeFillConfig {
        unit_cell,
        motif,
        parameter_element_values: HashMap::new(),
        geometry: sphere,
        motif_offset: DVec3::ZERO,
    };

    let options = LatticeFillOptions {
        hydrogen_passivation: false,
        remove_single_bond_atoms: false,
        reconstruct_surface: false,
        invert_phase: false,
    };

    // Fill region doesn't overlap with sphere
    let fill_region = DAABox::new(DVec3::new(-5.0, -5.0, -5.0), DVec3::new(5.0, 5.0, 5.0));

    let result = fill_lattice(&config, &options, &fill_region);

    // Should have no atoms
    assert_eq!(
        result.atomic_structure.get_num_of_atoms(),
        0,
        "Should have no atoms when geometry doesn't overlap"
    );
}

#[test]
fn test_fill_large_sphere_creates_atoms() {
    // Use a larger sphere to ensure atoms survive lone atom removal
    let sphere = GeoNode::sphere(DVec3::ZERO, 10.0);

    let unit_cell = UnitCellStruct::cubic_diamond();

    // Diamond motif with bonds
    let motif_text = "
site 1 C 0.0 0.0 0.0
site 2 C 0.25 0.25 0.25
bond 1 2
bond 1 -..2
bond 1 .-.2
bond 1 ..-2
";
    let motif = parse_motif(motif_text).unwrap();

    let config = LatticeFillConfig {
        unit_cell,
        motif,
        parameter_element_values: HashMap::new(),
        geometry: sphere,
        motif_offset: DVec3::ZERO,
    };

    let options = LatticeFillOptions {
        hydrogen_passivation: false,
        remove_single_bond_atoms: false,
        reconstruct_surface: false,
        invert_phase: false,
    };

    let fill_region = DAABox::new(
        DVec3::new(-15.0, -15.0, -15.0),
        DVec3::new(15.0, 15.0, 15.0),
    );

    let result = fill_lattice(&config, &options, &fill_region);

    // Should have atoms (larger sphere means more survive cleanup)
    assert!(
        result.atomic_structure.get_num_of_atoms() > 0,
        "Should have placed atoms"
    );

    // All atoms should be carbon (atomic number 6)
    for atom in result.atomic_structure.atoms_values() {
        assert_eq!(atom.atomic_number, 6, "All atoms should be carbon");
    }
}
