use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::hydrogen_passivation::{
    AddHydrogensOptions, add_hydrogens,
};

// ============================================================================
// Helpers
// ============================================================================

/// Create a structure with one atom at origin and N bonded neighbors.
fn make_structure(anchor_z: i16, neighbors: &[(i16, DVec3)]) -> (AtomicStructure, u32) {
    let mut s = AtomicStructure::new();
    let anchor = s.add_atom(anchor_z, DVec3::ZERO);
    for &(z, pos) in neighbors {
        let n = s.add_atom(z, pos);
        s.add_bond(anchor, n, 1);
    }
    (s, anchor)
}

/// Count atoms with atomic_number == 1 in the structure.
fn count_hydrogens(s: &AtomicStructure) -> usize {
    s.atoms_values().filter(|a| a.atomic_number == 1).count()
}

/// Count total bonds in the structure (each bond counted once).
fn count_bonds(s: &AtomicStructure) -> usize {
    s.get_num_of_bonds()
}

/// Count H atoms bonded to a specific atom.
fn count_h_bonded_to(s: &AtomicStructure, atom_id: u32) -> usize {
    let atom = s.get_atom(atom_id).unwrap();
    atom.bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter(|b| {
            s.get_atom(b.other_atom_id())
                .map_or(false, |a| a.atomic_number == 1)
        })
        .count()
}

/// Tetrahedral positions (normalized) scaled by distance.
fn tetrahedral_dirs(d: f64) -> [DVec3; 4] {
    [
        DVec3::new(1.0, 1.0, 1.0).normalize() * d,
        DVec3::new(-1.0, -1.0, 1.0).normalize() * d,
        DVec3::new(-1.0, 1.0, -1.0).normalize() * d,
        DVec3::new(1.0, -1.0, -1.0).normalize() * d,
    ]
}

fn default_options() -> AddHydrogensOptions {
    AddHydrogensOptions::default()
}

// ============================================================================
// Basic passivation: correct H count
// ============================================================================

#[test]
fn bare_carbon_gets_4_hydrogens() {
    let (mut s, _) = make_structure(6, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 4);
    assert_eq!(count_hydrogens(&s), 4);
    assert_eq!(count_bonds(&s), 4);
}

#[test]
fn carbon_with_1_bond_gets_3_hydrogens() {
    let (mut s, anchor) = make_structure(6, &[(6, DVec3::new(1.545, 0.0, 0.0))]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 3);
}

#[test]
fn carbon_with_2_bonds_gets_2_hydrogens() {
    let d = 1.545;
    let dirs = tetrahedral_dirs(d);
    let (mut s, anchor) = make_structure(6, &[(6, dirs[0]), (6, dirs[1])]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 2);
}

#[test]
fn carbon_with_3_bonds_gets_1_hydrogen() {
    let d = 1.545;
    let dirs = tetrahedral_dirs(d);
    let (mut s, anchor) = make_structure(6, &[(6, dirs[0]), (6, dirs[1]), (6, dirs[2])]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 1);
}

#[test]
fn nitrogen_sp3_with_0_bonds_gets_3_hydrogens() {
    let (mut s, _) = make_structure(7, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 3);
}

#[test]
fn nitrogen_with_2_bonds_gets_1_hydrogen() {
    let d = 1.47;
    let (mut s, anchor) = make_structure(
        7,
        &[
            (6, DVec3::new(d, 0.0, 0.0)),
            (6, DVec3::new(-d * 0.5, d * 0.866, 0.0)),
        ],
    );
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 1);
}

#[test]
fn oxygen_sp3_with_0_bonds_gets_2_hydrogens() {
    let (mut s, _) = make_structure(8, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 2);
}

#[test]
fn oxygen_with_1_bond_gets_1_hydrogen() {
    let (mut s, anchor) = make_structure(8, &[(6, DVec3::new(1.43, 0.0, 0.0))]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 1);
}

#[test]
fn fluorine_with_0_bonds_gets_1_hydrogen() {
    let (mut s, _) = make_structure(9, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 1);
}

#[test]
fn hydrogen_atom_not_passivated() {
    let (mut s, _) = make_structure(1, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}

#[test]
fn saturated_carbon_gets_no_hydrogens() {
    let d = 1.545;
    let dirs = tetrahedral_dirs(d);
    let (mut s, anchor) = make_structure(6, &[(6, dirs[0]), (6, dirs[1]), (6, dirs[2]), (6, dirs[3])]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 0);
}

#[test]
fn silicon_with_0_bonds_gets_4_hydrogens() {
    let (mut s, _) = make_structure(14, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 4);
}

// ============================================================================
// Geometry verification
// ============================================================================

#[test]
fn sp3_carbon_3_bonds_h_at_tetrahedral_angle() {
    let d = 1.545;
    let dirs = tetrahedral_dirs(d);
    let (mut s, anchor) = make_structure(6, &[(6, dirs[0]), (6, dirs[1]), (6, dirs[2])]);
    add_hydrogens(&mut s, &default_options());

    assert_eq!(count_h_bonded_to(&s, anchor), 1);

    // Find the H bonded to anchor
    let anchor_atom = s.get_atom(anchor).unwrap();
    let h_atom = anchor_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .find_map(|b| {
            let a = s.get_atom(b.other_atom_id())?;
            if a.atomic_number == 1 { Some(a) } else { None }
        })
        .unwrap();

    let h_dir = (h_atom.position - DVec3::ZERO).normalize();
    let anchor_pos = anchor_atom.position;

    // Check angle to each existing bond
    for &dir_pos in &dirs[..3] {
        let existing_dir = (dir_pos - anchor_pos).normalize();
        let angle = h_dir.angle_between(existing_dir).to_degrees();
        assert!(
            (angle - 109.47).abs() < 2.0,
            "Angle to existing bond = {:.2}°, expected ~109.47°",
            angle
        );
    }
}

#[test]
fn sp3_carbon_2_bonds_h_at_tetrahedral_angles() {
    let d = 1.545;
    let dirs = tetrahedral_dirs(d);
    let (mut s, anchor) = make_structure(6, &[(6, dirs[0]), (6, dirs[1])]);
    add_hydrogens(&mut s, &default_options());

    assert_eq!(count_h_bonded_to(&s, anchor), 2);

    // Find H atoms bonded to anchor
    let anchor_atom = s.get_atom(anchor).unwrap();
    let h_positions: Vec<DVec3> = anchor_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter_map(|b| {
            let a = s.get_atom(b.other_atom_id())?;
            if a.atomic_number == 1 { Some(a.position) } else { None }
        })
        .collect();

    // Each H should be ~109.47° from each existing bond
    for h_pos in &h_positions {
        let h_dir = h_pos.normalize();
        for &dir_pos in &dirs[..2] {
            let existing_dir = dir_pos.normalize();
            let angle = h_dir.angle_between(existing_dir).to_degrees();
            assert!(
                (angle - 109.47).abs() < 2.0,
                "H angle to existing bond = {:.2}°, expected ~109.47°",
                angle
            );
        }
    }
}

#[test]
fn sp2_carbon_2_bonds_h_at_120_degrees() {
    // Create an sp2 carbon (double bond triggers sp2 hybridization)
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.34, 0.0, 0.0));
    s.add_bond(c1, c2, 2); // double bond → sp2
    let c3 = s.add_atom(6, DVec3::new(-0.67, 1.16, 0.0));
    s.add_bond(c1, c3, 1);

    add_hydrogens(&mut s, &default_options());

    // c1 has 2 bonds (sp2) → 1 H needed (max 3 for sp2, has 2)
    // Find the H bonded to c1
    let c1_atom = s.get_atom(c1).unwrap();
    let c1_h_count = c1_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter(|b| {
            s.get_atom(b.other_atom_id())
                .map_or(false, |a| a.atomic_number == 1)
        })
        .count();
    assert_eq!(c1_h_count, 1, "sp2 C with 2 bonds should get 1 H");

    // The H should be roughly 120° from each existing bond
    let h_bonded_to_c1: Vec<_> = c1_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter_map(|b| {
            let a = s.get_atom(b.other_atom_id())?;
            if a.atomic_number == 1 {
                Some(a)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(h_bonded_to_c1.len(), 1);
    let h_dir = (h_bonded_to_c1[0].position - DVec3::ZERO).normalize();
    let dir_c2 = DVec3::new(1.34, 0.0, 0.0).normalize();
    let angle = h_dir.angle_between(dir_c2).to_degrees();
    assert!(
        (angle - 120.0).abs() < 5.0,
        "sp2 H angle = {:.2}°, expected ~120°",
        angle
    );
}

#[test]
fn sp1_carbon_1_bond_h_at_180_degrees() {
    // Create an sp1 carbon (triple bond)
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.2, 0.0, 0.0));
    s.add_bond(c1, c2, 3); // triple bond → sp1

    add_hydrogens(&mut s, &default_options());

    // c1 has 1 bond (sp1) → 1 H needed (max 2 for sp1, has 1)
    let c1_atom = s.get_atom(c1).unwrap();
    let c1_h: Vec<_> = c1_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter_map(|b| {
            let a = s.get_atom(b.other_atom_id())?;
            if a.atomic_number == 1 {
                Some(a)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(c1_h.len(), 1);

    let h_dir = (c1_h[0].position - DVec3::ZERO).normalize();
    let bond_dir = DVec3::new(1.2, 0.0, 0.0).normalize();
    let angle = h_dir.angle_between(bond_dir).to_degrees();
    assert!(
        (angle - 180.0).abs() < 1.0,
        "sp1 H angle = {:.2}°, expected 180°",
        angle
    );
}

// ============================================================================
// Bond lengths
// ============================================================================

#[test]
fn ch_bond_length_is_1_09() {
    let (mut s, anchor) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());

    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            let dist = (atom.position - s.get_atom(anchor).unwrap().position).length();
            assert!(
                (dist - 1.09).abs() < 0.001,
                "C-H bond length = {:.4}, expected 1.09",
                dist
            );
        }
    }
}

#[test]
fn nh_bond_length_is_1_01() {
    let (mut s, anchor) = make_structure(7, &[]);
    add_hydrogens(&mut s, &default_options());

    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            let dist = (atom.position - s.get_atom(anchor).unwrap().position).length();
            assert!(
                (dist - 1.01).abs() < 0.001,
                "N-H bond length = {:.4}, expected 1.01",
                dist
            );
        }
    }
}

#[test]
fn oh_bond_length_is_0_96() {
    let (mut s, anchor) = make_structure(8, &[]);
    add_hydrogens(&mut s, &default_options());

    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            let dist = (atom.position - s.get_atom(anchor).unwrap().position).length();
            assert!(
                (dist - 0.96).abs() < 0.001,
                "O-H bond length = {:.4}, expected 0.96",
                dist
            );
        }
    }
}

#[test]
fn sih_bond_length_is_1_48() {
    let (mut s, anchor) = make_structure(14, &[]);
    add_hydrogens(&mut s, &default_options());

    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            let dist = (atom.position - s.get_atom(anchor).unwrap().position).length();
            assert!(
                (dist - 1.48).abs() < 0.001,
                "Si-H bond length = {:.4}, expected 1.48",
                dist
            );
        }
    }
}

#[test]
fn unknown_element_uses_covalent_radii_sum() {
    // Titanium (Z=22) is not in the XH table, should fall back to covalent radii
    let (mut s, anchor) = make_structure(22, &[]);
    add_hydrogens(&mut s, &default_options());

    let h_count = count_hydrogens(&s);
    assert!(h_count > 0, "Ti should get some hydrogens");

    // Just verify it used some reasonable bond length (not crashing)
    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            let dist = (atom.position - s.get_atom(anchor).unwrap().position).length();
            assert!(dist > 0.5 && dist < 3.0, "Bond length {:.3} out of range", dist);
        }
    }
}

// ============================================================================
// Non-ideal geometry
// ============================================================================

#[test]
fn distorted_bonds_at_100_degrees_still_places_h() {
    // Two bonds at 100° instead of 109.47°
    let d = 1.545;
    let angle_rad = 100.0_f64.to_radians();
    let dir1 = DVec3::new(d, 0.0, 0.0);
    let dir2 = DVec3::new(d * angle_rad.cos(), d * angle_rad.sin(), 0.0);

    let (mut s, anchor) = make_structure(6, &[(6, dir1), (6, dir2)]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 2, "Should still place 2 H's on anchor");
}

#[test]
fn distorted_bonds_at_120_degrees_still_places_h() {
    // Two bonds at 120° instead of 109.47°
    let d = 1.545;
    let dir1 = DVec3::new(d, 0.0, 0.0);
    let dir2 = DVec3::new(-d * 0.5, d * 0.866, 0.0);

    let (mut s, anchor) = make_structure(6, &[(6, dir1), (6, dir2)]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 2, "Should still place 2 H's on anchor");
}

// ============================================================================
// Options
// ============================================================================

#[test]
fn selected_only_passivates_selected_atoms() {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(3.0, 0.0, 0.0));
    // Select only c1
    s.set_atom_selected(c1, true);

    let options = AddHydrogensOptions {
        selected_only: true,
        skip_already_passivated: true,
    };
    let result = add_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_added, 4, "Only c1 should get H's");

    // c2 should have no H's bonded
    let c2_atom = s.get_atom(c2).unwrap();
    let c2_bonds = c2_atom.bonds.iter().filter(|b| !b.is_delete_marker()).count();
    assert_eq!(c2_bonds, 0, "c2 should have no bonds (not selected)");
}

#[test]
fn skip_already_passivated_atoms() {
    let mut s = AtomicStructure::new();
    let c = s.add_atom(6, DVec3::ZERO);
    s.set_atom_hydrogen_passivation(c, true);

    let options = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: true,
    };
    let result = add_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_added, 0, "Flagged atom should be skipped");
}

#[test]
fn do_not_skip_passivated_when_flag_false() {
    let mut s = AtomicStructure::new();
    let c = s.add_atom(6, DVec3::ZERO);
    s.set_atom_hydrogen_passivation(c, true);

    let options = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: false,
    };
    let result = add_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_added, 4, "Should passivate despite flag");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_structure_adds_nothing() {
    let mut s = AtomicStructure::new();
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}

#[test]
fn structure_with_only_hydrogens_adds_nothing() {
    let mut s = AtomicStructure::new();
    s.add_atom(1, DVec3::ZERO);
    s.add_atom(1, DVec3::new(1.0, 0.0, 0.0));
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}

#[test]
fn delete_markers_are_skipped() {
    let mut s = AtomicStructure::new();
    s.add_atom(0, DVec3::ZERO); // delete marker
    s.add_atom(6, DVec3::new(3.0, 0.0, 0.0)); // real carbon
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 4, "Only the carbon gets H's");
}

#[test]
fn noble_gas_gets_no_hydrogens() {
    let (mut s, _) = make_structure(10, &[]); // Neon
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}

#[test]
fn helium_gets_no_hydrogens() {
    let (mut s, _) = make_structure(2, &[]); // Helium
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}

#[test]
fn argon_gets_no_hydrogens() {
    let (mut s, _) = make_structure(18, &[]); // Argon
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}

// ============================================================================
// Integration: methane construction
// ============================================================================

#[test]
fn bare_carbon_becomes_methane() {
    let (mut s, anchor) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());

    // 1 C + 4 H = 5 atoms
    let total_atoms: usize = s.atoms_values().count();
    assert_eq!(total_atoms, 5);
    assert_eq!(count_hydrogens(&s), 4);
    assert_eq!(count_bonds(&s), 4);

    // All C-H bonds are 1.09 Å
    let anchor_pos = s.get_atom(anchor).unwrap().position;
    let h_positions: Vec<DVec3> = s
        .atoms_values()
        .filter(|a| a.atomic_number == 1)
        .map(|a| a.position)
        .collect();

    for h_pos in &h_positions {
        let dist = (*h_pos - anchor_pos).length();
        assert!(
            (dist - 1.09).abs() < 0.001,
            "C-H distance = {:.4}, expected 1.09",
            dist
        );
    }

    // All H-C-H angles should be ~109.47°
    let h_dirs: Vec<DVec3> = h_positions.iter().map(|p| (*p - anchor_pos).normalize()).collect();
    for i in 0..h_dirs.len() {
        for j in (i + 1)..h_dirs.len() {
            let angle = h_dirs[i].angle_between(h_dirs[j]).to_degrees();
            assert!(
                (angle - 109.47).abs() < 2.0,
                "H-C-H angle = {:.2}°, expected ~109.47°",
                angle
            );
        }
    }
}

// ============================================================================
// Complex molecules
// ============================================================================

#[test]
fn water_saturated_gets_no_more_h() {
    // O with 2 H's already → saturated (O sp3 max = 2)
    let d = 0.96;
    let angle = 104.5_f64.to_radians() / 2.0;
    let h1 = DVec3::new(d * angle.sin(), d * angle.cos(), 0.0);
    let h2 = DVec3::new(-d * angle.sin(), d * angle.cos(), 0.0);

    let (mut s, _) = make_structure(8, &[(1, h1), (1, h2)]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0, "Water is saturated");
}

#[test]
fn ethylene_gets_2_more_h() {
    // C=C with 2 H's already placed
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.34, 0.0, 0.0));
    s.add_bond(c1, c2, 2); // double bond → sp2

    // Add one H on each C
    let h1 = s.add_atom(1, DVec3::new(-0.5, 0.87, 0.0));
    s.add_bond(c1, h1, 1);
    let h2 = s.add_atom(1, DVec3::new(1.84, -0.87, 0.0));
    s.add_bond(c2, h2, 1);

    let initial_h_count = count_hydrogens(&s);
    assert_eq!(initial_h_count, 2);

    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 2, "Each C needs 1 more H");
    assert_eq!(count_hydrogens(&s), 4);
}

// ============================================================================
// H atoms flagged correctly
// ============================================================================

#[test]
fn added_hydrogens_are_flagged_as_passivation() {
    let (mut s, _) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());

    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            assert!(
                atom.is_hydrogen_passivation(),
                "Added H should be flagged as hydrogen passivation"
            );
        }
    }
}

// ============================================================================
// Multi-atom structures
// ============================================================================

#[test]
fn ethane_like_two_carbons_bonded() {
    // C-C single bond, both need 3 H's each
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    s.add_bond(c1, c2, 1);

    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 6, "3 H per carbon");

    // Verify each carbon has 4 total bonds (1 C-C + 3 C-H)
    let c1_atom = s.get_atom(c1).unwrap();
    let c1_bonds = c1_atom.bonds.iter().filter(|b| !b.is_delete_marker()).count();
    assert_eq!(c1_bonds, 4);

    let c2_atom = s.get_atom(c2).unwrap();
    let c2_bonds = c2_atom.bonds.iter().filter(|b| !b.is_delete_marker()).count();
    assert_eq!(c2_bonds, 4);
}

#[test]
fn phosphorus_with_0_bonds_gets_3_hydrogens() {
    // P sp3 max = 3
    let (mut s, _) = make_structure(15, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 3);
}

#[test]
fn sulfur_with_0_bonds_gets_2_hydrogens() {
    // S sp3 max = 2
    let (mut s, _) = make_structure(16, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 2);
}

#[test]
fn chlorine_with_0_bonds_gets_1_hydrogen() {
    // Cl max = 1
    let (mut s, _) = make_structure(17, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 1);
}

#[test]
fn boron_with_0_bonds_gets_3_hydrogens() {
    // B sp2 max = 3
    let (mut s, _) = make_structure(5, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 3);
}

#[test]
fn germanium_h_bond_length() {
    let (mut s, anchor) = make_structure(32, &[]);
    add_hydrogens(&mut s, &default_options());

    let anchor_pos = s.get_atom(anchor).unwrap().position;
    for atom in s.atoms_values() {
        if atom.atomic_number == 1 {
            let dist = (atom.position - anchor_pos).length();
            assert!(
                (dist - 1.53).abs() < 0.001,
                "Ge-H bond length = {:.4}, expected 1.53",
                dist
            );
        }
    }
}

#[test]
fn negative_atomic_number_skipped() {
    // Parameter element with negative atomic number
    let mut s = AtomicStructure::new();
    s.add_atom(-1, DVec3::ZERO);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.hydrogens_added, 0);
}
