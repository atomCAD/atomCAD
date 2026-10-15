use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::hydrogen_passivation::{
    AddHydrogensOptions, RemoveHydrogensOptions, add_hydrogens, open_valence_slots,
    remove_hydrogens,
};
use glam::f64::DVec3;

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
                .is_some_and(|a| a.atomic_number == 1)
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
    assert_eq!(result.atoms_added, 4);
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
    assert_eq!(result.atoms_added, 3);
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
    assert_eq!(result.atoms_added, 2);
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
    assert_eq!(result.atoms_added, 1);
}

#[test]
fn hydrogen_atom_not_passivated() {
    let (mut s, _) = make_structure(1, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 0);
}

#[test]
fn saturated_carbon_gets_no_hydrogens() {
    let d = 1.545;
    let dirs = tetrahedral_dirs(d);
    let (mut s, anchor) =
        make_structure(6, &[(6, dirs[0]), (6, dirs[1]), (6, dirs[2]), (6, dirs[3])]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_h_bonded_to(&s, anchor), 0);
}

#[test]
fn silicon_with_0_bonds_gets_4_hydrogens() {
    let (mut s, _) = make_structure(14, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 4);
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
            if a.atomic_number == 1 {
                Some(a.position)
            } else {
                None
            }
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
                .is_some_and(|a| a.atomic_number == 1)
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
            if a.atomic_number == 1 { Some(a) } else { None }
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
            if a.atomic_number == 1 { Some(a) } else { None }
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
            assert!(
                dist > 0.5 && dist < 3.0,
                "Bond length {:.3} out of range",
                dist
            );
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
    assert_eq!(
        count_h_bonded_to(&s, anchor),
        2,
        "Should still place 2 H's on anchor"
    );
}

#[test]
fn distorted_bonds_at_120_degrees_still_places_h() {
    // Two bonds at 120° instead of 109.47°
    let d = 1.545;
    let dir1 = DVec3::new(d, 0.0, 0.0);
    let dir2 = DVec3::new(-d * 0.5, d * 0.866, 0.0);

    let (mut s, anchor) = make_structure(6, &[(6, dir1), (6, dir2)]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(
        count_h_bonded_to(&s, anchor),
        2,
        "Should still place 2 H's on anchor"
    );
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
        passivant_element: 1,
        clash: None,
    };
    let result = add_hydrogens(&mut s, &options);
    assert_eq!(result.atoms_added, 4, "Only c1 should get H's");

    // c2 should have no H's bonded
    let c2_atom = s.get_atom(c2).unwrap();
    let c2_bonds = c2_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .count();
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
        passivant_element: 1,
        clash: None,
    };
    let result = add_hydrogens(&mut s, &options);
    assert_eq!(result.atoms_added, 0, "Flagged atom should be skipped");
}

#[test]
fn do_not_skip_passivated_when_flag_false() {
    let mut s = AtomicStructure::new();
    let c = s.add_atom(6, DVec3::ZERO);
    s.set_atom_hydrogen_passivation(c, true);

    let options = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: false,
        passivant_element: 1,
        clash: None,
    };
    let result = add_hydrogens(&mut s, &options);
    assert_eq!(result.atoms_added, 4, "Should passivate despite flag");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn empty_structure_adds_nothing() {
    let mut s = AtomicStructure::new();
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 0);
}

#[test]
fn structure_with_only_hydrogens_adds_nothing() {
    let mut s = AtomicStructure::new();
    s.add_atom(1, DVec3::ZERO);
    s.add_atom(1, DVec3::new(1.0, 0.0, 0.0));
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 0);
}

#[test]
fn delete_markers_are_skipped() {
    let mut s = AtomicStructure::new();
    s.add_atom(0, DVec3::ZERO); // delete marker
    s.add_atom(6, DVec3::new(3.0, 0.0, 0.0)); // real carbon
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 4, "Only the carbon gets H's");
}

#[test]
fn noble_gas_gets_no_hydrogens() {
    let (mut s, _) = make_structure(10, &[]); // Neon
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 0);
}

#[test]
fn helium_gets_no_hydrogens() {
    let (mut s, _) = make_structure(2, &[]); // Helium
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 0);
}

#[test]
fn argon_gets_no_hydrogens() {
    let (mut s, _) = make_structure(18, &[]); // Argon
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 0);
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
    let h_dirs: Vec<DVec3> = h_positions
        .iter()
        .map(|p| (*p - anchor_pos).normalize())
        .collect();
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
    assert_eq!(result.atoms_added, 0, "Water is saturated");
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
    assert_eq!(result.atoms_added, 2, "Each C needs 1 more H");
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
    assert_eq!(result.atoms_added, 6, "3 H per carbon");

    // Verify each carbon has 4 total bonds (1 C-C + 3 C-H)
    let c1_atom = s.get_atom(c1).unwrap();
    let c1_bonds = c1_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .count();
    assert_eq!(c1_bonds, 4);

    let c2_atom = s.get_atom(c2).unwrap();
    let c2_bonds = c2_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .count();
    assert_eq!(c2_bonds, 4);
}

#[test]
fn phosphorus_with_0_bonds_gets_3_hydrogens() {
    // P sp3 max = 3
    let (mut s, _) = make_structure(15, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 3);
}

#[test]
fn sulfur_with_0_bonds_gets_2_hydrogens() {
    // S sp3 max = 2
    let (mut s, _) = make_structure(16, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 2);
}

#[test]
fn chlorine_with_0_bonds_gets_1_hydrogen() {
    // Cl max = 1
    let (mut s, _) = make_structure(17, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 1);
}

#[test]
fn boron_with_0_bonds_gets_3_hydrogens() {
    // B sp2 max = 3
    let (mut s, _) = make_structure(5, &[]);
    let result = add_hydrogens(&mut s, &default_options());
    assert_eq!(result.atoms_added, 3);
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
    assert_eq!(result.atoms_added, 0);
}

// ============================================================================
// Hydrogen depassivation: basic removal
// ============================================================================

fn remove_all_options() -> RemoveHydrogensOptions {
    RemoveHydrogensOptions {
        selected_only: false,
    }
}

#[test]
fn remove_h_methane_leaves_bare_carbon() {
    // Bare C → passivate → 1C+4H → remove all H → 1C, 0 bonds
    let (mut s, anchor) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_hydrogens(&s), 4);

    let result = remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(result.hydrogens_removed, 4);
    assert_eq!(count_hydrogens(&s), 0);

    let total: usize = s.atoms_values().count();
    assert_eq!(total, 1, "Only the carbon remains");
    assert_eq!(count_bonds(&s), 0, "No bonds left");
    assert_eq!(
        count_h_bonded_to(&s, anchor),
        0,
        "Carbon has no H neighbors"
    );
}

#[test]
fn remove_h_ethane_leaves_two_carbons_with_bond() {
    // C-C + 6H → remove all H → 2C, 1 C-C bond
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    s.add_bond(c1, c2, 1);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_hydrogens(&s), 6);

    let result = remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(result.hydrogens_removed, 6);
    assert_eq!(count_hydrogens(&s), 0);

    let total: usize = s.atoms_values().count();
    assert_eq!(total, 2, "Only the two carbons remain");
    assert_eq!(count_bonds(&s), 1, "C-C bond remains");
}

#[test]
fn remove_h_only_hydrogens_leaves_empty() {
    let mut s = AtomicStructure::new();
    s.add_atom(1, DVec3::ZERO);
    s.add_atom(1, DVec3::new(1.0, 0.0, 0.0));

    let result = remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(result.hydrogens_removed, 2);

    let total: usize = s.atoms_values().count();
    assert_eq!(total, 0, "All atoms removed");
}

#[test]
fn remove_h_no_hydrogens_removes_nothing() {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    s.add_bond(c1, c2, 1);

    let result = remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(result.hydrogens_removed, 0);

    let total: usize = s.atoms_values().count();
    assert_eq!(total, 2);
    assert_eq!(count_bonds(&s), 1);
}

#[test]
fn remove_h_empty_structure() {
    let mut s = AtomicStructure::new();
    let result = remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(result.hydrogens_removed, 0);
}

// ============================================================================
// Hydrogen depassivation: selection filter
// ============================================================================

#[test]
fn remove_h_selected_only_by_neighbor() {
    // Ethane: select c1 → only c1's 3 H's removed, c2's 3 H's remain
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    s.add_bond(c1, c2, 1);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_hydrogens(&s), 6);

    s.set_atom_selected(c1, true);

    let options = RemoveHydrogensOptions {
        selected_only: true,
    };
    let result = remove_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_removed, 3);
    assert_eq!(count_hydrogens(&s), 3);
    assert_eq!(count_h_bonded_to(&s, c2), 3, "c2's H's untouched");
}

#[test]
fn remove_h_selected_only_h_itself_selected() {
    // Methane: select one H directly → only that H removed
    let (mut s, anchor) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());

    // Find one H and select it
    let h_id = s.atoms_values().find(|a| a.atomic_number == 1).unwrap().id;
    s.set_atom_selected(h_id, true);

    let options = RemoveHydrogensOptions {
        selected_only: true,
    };
    let result = remove_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_removed, 1);
    assert_eq!(count_hydrogens(&s), 3);
    assert_eq!(count_h_bonded_to(&s, anchor), 3);
}

#[test]
fn remove_h_selected_only_nothing_selected() {
    let (mut s, _) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_hydrogens(&s), 4);

    let options = RemoveHydrogensOptions {
        selected_only: true,
    };
    let result = remove_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_removed, 0);
    assert_eq!(count_hydrogens(&s), 4);
}

#[test]
fn remove_h_selected_only_all_selected_same_as_all() {
    let (mut s, _) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());

    // Select all atoms
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    for id in ids {
        s.set_atom_selected(id, true);
    }

    let options = RemoveHydrogensOptions {
        selected_only: true,
    };
    let result = remove_hydrogens(&mut s, &options);
    assert_eq!(result.hydrogens_removed, 4);
    assert_eq!(count_hydrogens(&s), 0);
}

// ============================================================================
// Hydrogen depassivation: bond cleanup
// ============================================================================

#[test]
fn remove_h_carbon_has_zero_bonds_after() {
    let (mut s, anchor) = make_structure(6, &[]);
    add_hydrogens(&mut s, &default_options());
    remove_hydrogens(&mut s, &remove_all_options());

    let c = s.get_atom(anchor).unwrap();
    let active_bonds = c.bonds.iter().filter(|b| !b.is_delete_marker()).count();
    assert_eq!(
        active_bonds, 0,
        "Carbon should have 0 bonds after H removal"
    );
}

#[test]
fn remove_h_ethane_cc_bond_intact() {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    s.add_bond(c1, c2, 1);
    add_hydrogens(&mut s, &default_options());
    remove_hydrogens(&mut s, &remove_all_options());

    // C-C bond should remain
    let c1_atom = s.get_atom(c1).unwrap();
    let c1_neighbors: Vec<u32> = c1_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .map(|b| b.other_atom_id())
        .collect();
    assert_eq!(c1_neighbors, vec![c2]);
}

// ============================================================================
// Hydrogen depassivation: mixed structures
// ============================================================================

#[test]
fn remove_h_mixed_elements_only_h_removed() {
    // C, N, O each with H's → remove H → all heavy atoms remain
    let mut s = AtomicStructure::new();
    let c = s.add_atom(6, DVec3::ZERO);
    let n = s.add_atom(7, DVec3::new(3.0, 0.0, 0.0));
    let o = s.add_atom(8, DVec3::new(6.0, 0.0, 0.0));
    add_hydrogens(&mut s, &default_options());

    let h_before = count_hydrogens(&s);
    assert!(h_before > 0);

    remove_hydrogens(&mut s, &remove_all_options());

    assert_eq!(count_hydrogens(&s), 0);
    assert!(s.get_atom(c).is_some(), "Carbon still exists");
    assert!(s.get_atom(n).is_some(), "Nitrogen still exists");
    assert!(s.get_atom(o).is_some(), "Oxygen still exists");
}

#[test]
fn remove_h_delete_markers_not_removed() {
    let mut s = AtomicStructure::new();
    let dm = s.add_atom(0, DVec3::ZERO); // delete marker
    s.add_atom(1, DVec3::new(1.0, 0.0, 0.0)); // hydrogen

    let result = remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(result.hydrogens_removed, 1);
    assert!(s.get_atom(dm).is_some(), "Delete marker still exists");
}

// ============================================================================
// Hydrogen depassivation: round-trip with passivation
// ============================================================================

#[test]
fn round_trip_bare_carbon_passivate_then_strip() {
    let (mut s, anchor) = make_structure(6, &[]);

    // Passivate → 4 H
    add_hydrogens(&mut s, &default_options());
    assert_eq!(count_hydrogens(&s), 4);

    // Strip → back to 1 C, 0 bonds
    remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(count_hydrogens(&s), 0);

    let total: usize = s.atoms_values().count();
    assert_eq!(total, 1);
    assert_eq!(count_bonds(&s), 0);
    assert!(s.get_atom(anchor).is_some());
}

#[test]
fn round_trip_ethane_strip_then_repassivate() {
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    s.add_bond(c1, c2, 1);
    add_hydrogens(&mut s, &default_options());

    let h_before = count_hydrogens(&s);
    assert_eq!(h_before, 6);

    // Strip H
    remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(count_hydrogens(&s), 0);
    assert_eq!(count_bonds(&s), 1); // Only C-C

    // Re-passivate
    let opts = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: false,
        passivant_element: 1,
        clash: None,
    };
    add_hydrogens(&mut s, &opts);
    assert_eq!(count_hydrogens(&s), 6);
    assert_eq!(count_h_bonded_to(&s, c1), 3);
    assert_eq!(count_h_bonded_to(&s, c2), 3);
}

#[test]
fn round_trip_ethylene_strip_and_repassivate() {
    // C=C double bond, sp2: each C gets 2 H → strip → 2C with double bond → re-passivate
    let mut s = AtomicStructure::new();
    let c1 = s.add_atom(6, DVec3::ZERO);
    let c2 = s.add_atom(6, DVec3::new(1.34, 0.0, 0.0));
    s.add_bond(c1, c2, 2);

    let opts_no_skip = AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: false,
        passivant_element: 1,
        clash: None,
    };
    add_hydrogens(&mut s, &opts_no_skip);
    let h_count_original = count_hydrogens(&s);
    assert_eq!(h_count_original, 4, "Each sp2 C gets 2 H");

    // Strip
    remove_hydrogens(&mut s, &remove_all_options());
    assert_eq!(count_hydrogens(&s), 0);

    let total: usize = s.atoms_values().count();
    assert_eq!(total, 2, "Only the two carbons remain");
    assert_eq!(count_bonds(&s), 1, "Only C=C bond remains");

    // Re-passivate
    add_hydrogens(&mut s, &opts_no_skip);
    assert_eq!(count_hydrogens(&s), 4, "Should get 4 H back");
}

// ============================================================================
// Halogen passivation (doc/design_halogen_passivation.md Phase 1, general path)
// ============================================================================

fn passivant_options(element: i16) -> AddHydrogensOptions {
    AddHydrogensOptions {
        selected_only: false,
        skip_already_passivated: true,
        passivant_element: element,
        clash: None,
    }
}

/// Sorted positions for order-independent comparison.
fn sorted_positions(s: &AtomicStructure) -> Vec<DVec3> {
    let mut v: Vec<DVec3> = s.atoms_values().map(|a| a.position).collect();
    v.sort_by(|p, q| {
        p.x.total_cmp(&q.x)
            .then(p.y.total_cmp(&q.y))
            .then(p.z.total_cmp(&q.z))
    });
    v
}

/// A bare carbon fluorinates to CF₄-like geometry: 4 F at the C–F bond length,
/// and the general path flags each terminator (D5).
#[test]
fn fluorine_passivation_bond_length_and_flag() {
    let (mut s, c) = make_structure(6, &[]);
    let result = add_hydrogens(&mut s, &passivant_options(9));
    assert_eq!(result.atoms_added, 4, "bare carbon gets 4 F");
    assert_eq!(count_hydrogens(&s), 0, "no hydrogens when passivant is F");

    let c_pos = s.get_atom(c).unwrap().position;
    let f_atoms: Vec<_> = s.atoms_values().filter(|a| a.atomic_number == 9).collect();
    assert_eq!(f_atoms.len(), 4);
    for f in &f_atoms {
        let d = (f.position - c_pos).length();
        assert!(
            (d - 1.35).abs() < 1e-6,
            "C–F bond length must be 1.35, got {d}"
        );
        assert!(
            f.is_hydrogen_passivation(),
            "the general path flags its terminators (D5)"
        );
    }
}

/// Chlorine passivation puts terminators at the C–Cl length, and two runs
/// produce byte-identical positions (determinism — the whole point of the issue).
#[test]
fn chlorine_passivation_bond_length_and_determinism() {
    let build = || {
        let (mut s, c) = make_structure(6, &[(6, DVec3::new(1.5, 0.0, 0.0))]);
        add_hydrogens(&mut s, &passivant_options(17));
        (s, c)
    };

    let (s1, c) = build();
    // The neighbor carbon is also passivated, so count only the Cl bonded to `c`.
    let c_atom = s1.get_atom(c).unwrap();
    let cl_on_c: Vec<u32> = c_atom
        .bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .map(|b| b.other_atom_id())
        .filter(|&id| s1.get_atom(id).is_some_and(|a| a.atomic_number == 17))
        .collect();
    assert_eq!(cl_on_c.len(), 3, "a C with one C–C bond needs 3 more Cl");
    let c_pos = c_atom.position;
    for &id in &cl_on_c {
        let d = (s1.get_atom(id).unwrap().position - c_pos).length();
        assert!(
            (d - 1.77).abs() < 1e-6,
            "C–Cl bond length must be 1.77, got {d}"
        );
    }

    let (s2, _) = build();
    assert_eq!(
        sorted_positions(&s1),
        sorted_positions(&s2),
        "passivation must be deterministic"
    );
}

/// Host-skip rule: `host == 1 || host == passivant` (design decision log #6).
#[test]
fn host_skip_rule_lone_atoms() {
    // A lone F is not capped with another F (no F₂ "passivation").
    {
        let mut s = AtomicStructure::new();
        s.add_atom(9, DVec3::ZERO);
        let r = add_hydrogens(&mut s, &passivant_options(9));
        assert_eq!(r.atoms_added, 0, "a lone F must not be self-capped with F");
        assert_eq!(s.atoms_values().count(), 1);
    }
    // A lone F under H passivation is still H-capped — today's behavior.
    {
        let mut s = AtomicStructure::new();
        s.add_atom(9, DVec3::ZERO);
        let r = add_hydrogens(&mut s, &default_options());
        assert_eq!(r.atoms_added, 1, "a lone F is still capped with one H");
        assert_eq!(count_hydrogens(&s), 1);
    }
    // A lone H is never passivated, for any passivant (H is a terminator, never a host).
    {
        let mut s = AtomicStructure::new();
        s.add_atom(1, DVec3::ZERO);
        let r = add_hydrogens(&mut s, &passivant_options(9));
        assert_eq!(r.atoms_added, 0, "a lone H must never be passivated");
    }
}

/// D2 per-site H guard on **silicon** (the element where the three sites
/// differ): the general path uses the molecular `XH_BOND_LENGTHS` value 1.48,
/// distinct from lattice-fill (1.42) and surf_recon (1.50).
#[test]
fn per_site_h_pinning_silicon_general_path() {
    let (mut s, si) = make_structure(14, &[]);
    add_hydrogens(&mut s, &default_options());
    let si_pos = s.get_atom(si).unwrap().position;
    let hs: Vec<_> = s.atoms_values().filter(|a| a.atomic_number == 1).collect();
    assert!(!hs.is_empty(), "silicon must be passivated with H");
    for h in &hs {
        let d = (h.position - si_pos).length();
        assert!(
            (d - 1.48).abs() < 1e-6,
            "general-path Si–H bond length must be 1.48, got {d}"
        );
    }
}

// ============================================================================
// open_valence_slots — the shared analysis half
// ============================================================================

/// The slot count and the placement path must agree atom for atom: the count
/// is what a proxy report shows as `open_valences` and what a
/// quantum-chemistry multiplicity is read from (`doc/design_proxy_node.md`
/// §7.5).
#[test]
fn open_valence_slots_agrees_with_what_add_hydrogens_places() {
    let dirs = tetrahedral_dirs(1.54);
    let (mut s, anchor) = make_structure(6, &[(6, dirs[0]), (6, dirs[1])]);

    // Two of four tetrahedral slots are taken on the anchor; each neighbour
    // has one bond and three slots of its own.
    assert_eq!(open_valence_slots(&s, anchor, 1), 2);
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    let total: usize = ids.iter().map(|&id| open_valence_slots(&s, id, 1)).sum();
    assert_eq!(total, 8);

    let added = add_hydrogens(&mut s, &default_options()).atoms_added;
    assert_eq!(added, total, "the two halves count the same slots");

    // And the structure has no slot left once they are placed.
    let ids: Vec<u32> = s.atom_ids().copied().collect();
    let left: usize = ids.iter().map(|&id| open_valence_slots(&s, id, 1)).sum();
    assert_eq!(left, 0);
}

#[test]
fn open_valence_slots_is_zero_where_no_terminator_can_go() {
    // A saturated carbon.
    let dirs = tetrahedral_dirs(1.09);
    let (s, methane_c) =
        make_structure(6, &[(1, dirs[0]), (1, dirs[1]), (1, dirs[2]), (1, dirs[3])]);
    assert_eq!(open_valence_slots(&s, methane_c, 1), 0);

    // A hydrogen is a terminator, never a host — whatever the passivant.
    let mut s = AtomicStructure::new();
    let h = s.add_atom(1, DVec3::ZERO);
    assert_eq!(open_valence_slots(&s, h, 1), 0);
    assert_eq!(open_valence_slots(&s, h, 9), 0);

    // The host-skip rule: no F₂, though H still caps a lone F.
    let f = s.add_atom(9, DVec3::new(5.0, 0.0, 0.0));
    assert_eq!(open_valence_slots(&s, f, 9), 0);
    assert_eq!(open_valence_slots(&s, f, 1), 1);

    // Markers and unresolved parameters carry no element, and a missing atom
    // is not an atom.
    let marker = s.add_atom(0, DVec3::new(10.0, 0.0, 0.0));
    assert_eq!(open_valence_slots(&s, marker, 1), 0);
    assert_eq!(open_valence_slots(&s, 9999, 1), 0);
}

// ============================================================================
// The clash rule (doc/design_chemisorb_mirrors_and_passivation.md §5.1)
// ============================================================================

use atomcad_crystolecule::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::hydrogen_passivation::{
    AddHydrogensResult, DEFAULT_PASSIVATION_CLASH_FACTOR, add_hydrogens_filtered,
};
use atomcad_crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use std::collections::HashSet;

const A_SI: f64 = 5.431;
const A_C: f64 = 3.567;

fn clash_options(factor: Option<f64>) -> AddHydrogensOptions {
    AddHydrogensOptions {
        clash: factor,
        ..AddHydrogensOptions::default()
    }
}

fn vdw(z: i16) -> f64 {
    ATOM_INFO
        .get(&(z as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .van_der_waals_radius
}

/// A bare diamond-lattice slab of `element` (lattice constant `a`): a cube of
/// `cells` cubic cells, cut by `top` (an outward plane normal through the
/// cube's centre) when given. Returns the slab and the hosts to passivate:
/// the open-valence atoms of the top layer within `radius` of the facet's
/// centre, away from the cube's bare edges.
fn surface(
    element: i16,
    a: f64,
    cells: f64,
    top: Option<DVec3>,
    reconstruct: bool,
    radius: f64,
) -> (AtomicStructure, HashSet<u32>) {
    let mut motif = DEFAULT_ZINCBLENDE_MOTIF.clone();
    for p in &mut motif.parameters {
        p.default_atomic_number = element;
    }
    let size = cells * a;
    let max = DVec3::splat(size);
    let mut planes = vec![
        GeoNode::half_space(DVec3::NEG_X, DVec3::ZERO),
        GeoNode::half_space(DVec3::X, max),
        GeoNode::half_space(DVec3::NEG_Y, DVec3::ZERO),
        GeoNode::half_space(DVec3::Y, max),
        GeoNode::half_space(DVec3::NEG_Z, DVec3::ZERO),
    ];
    // The (100) slab is 1.5 cells deep; a tilted facet cuts the full cube.
    let normal = match top {
        Some(n) => {
            planes.push(GeoNode::half_space(DVec3::Z, max));
            let n = n.normalize();
            planes.push(GeoNode::half_space(n, max / 2.0));
            n
        }
        None => {
            planes.push(GeoNode::half_space(DVec3::Z, DVec3::new(0.0, 0.0, 1.5 * a)));
            DVec3::Z
        }
    };
    let config = LatticeFillConfig {
        unit_cell: UnitCellStruct::new(
            DVec3::new(a, 0.0, 0.0),
            DVec3::new(0.0, a, 0.0),
            DVec3::new(0.0, 0.0, a),
        ),
        motif,
        parameter_element_values: std::collections::HashMap::new(),
        geometry: GeoNode::intersection_3d(planes),
        motif_offset: DVec3::ZERO,
        regions: Vec::new(),
    };
    let options = LatticeFillOptions {
        hydrogen_passivation: false,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms: true,
        reconstruct_surface: reconstruct,
        invert_phase: false,
        rebond_concave_clashes: true,
        passivation_element: 1,
    };
    let region = DAABox::new(DVec3::splat(-5.0), max + DVec3::splat(5.0));
    let s = fill_lattice(&config, &options, &region).atomic_structure;

    let height = |p: DVec3| p.dot(normal);
    let top_height = s
        .atoms_values()
        .map(|atom| height(atom.position))
        .fold(f64::MIN, f64::max);
    let in_top_layer = |p: DVec3| height(p) > top_height - 0.6;
    let layer: Vec<DVec3> = s
        .atoms_values()
        .filter(|atom| in_top_layer(atom.position))
        .map(|atom| atom.position)
        .collect();
    let centre = layer.iter().copied().sum::<DVec3>() / layer.len() as f64;
    let hosts: HashSet<u32> = s
        .atoms_values()
        .filter(|atom| {
            let d = atom.position - centre;
            in_top_layer(atom.position)
                && (d - normal * d.dot(normal)).length() < radius
                && open_valence_slots(&s, atom.id, 1) > 0
        })
        .map(|atom| atom.id)
        .collect();
    assert!(hosts.len() >= 6, "the facet has a patch of hosts");
    (s, hosts)
}

fn si100_dimers() -> (AtomicStructure, HashSet<u32>) {
    surface(14, A_SI, 6.0, None, true, 8.0)
}

fn si100_dihydride() -> (AtomicStructure, HashSet<u32>) {
    surface(14, A_SI, 6.0, None, false, 8.0)
}

fn c100_dihydride() -> (AtomicStructure, HashSet<u32>) {
    surface(6, A_C, 8.0, None, false, 6.0)
}

fn si111() -> (AtomicStructure, HashSet<u32>) {
    surface(14, A_SI, 6.0, Some(DVec3::ONE), false, 8.0)
}

/// Passivates `hosts` of `input` and returns the output with the result.
fn passivate_hosts(
    input: &AtomicStructure,
    hosts: &HashSet<u32>,
    factor: Option<f64>,
) -> (AtomicStructure, AddHydrogensResult) {
    let mut out = input.clone();
    let result =
        add_hydrogens_filtered(&mut out, &clash_options(factor), &|id| hosts.contains(&id));
    (out, result)
}

/// The terminators `output` gained over `input`: `(id, host)`.
fn placed(input: &AtomicStructure, output: &AtomicStructure) -> Vec<(u32, u32)> {
    output
        .atoms_values()
        .filter(|atom| input.get_atom(atom.id).is_none())
        .map(|atom| {
            let host = atom.bonds[0].other_atom_id();
            (atom.id, host)
        })
        .collect()
}

/// Whether `other` is outside the rule's exclusion for a terminator on
/// `host`, read off `output` (where the host's own terminators are bonded
/// neighbours too).
fn tested_against(output: &AtomicStructure, host: u32, other: u32) -> bool {
    other != host
        && !output
            .get_atom(host)
            .unwrap()
            .bonds
            .iter()
            .any(|b| !b.is_delete_marker() && b.other_atom_id() == other)
}

/// The brute-force O(n²) oracle, independent of the implementation: the
/// first atom of `output` a terminator at `p` on `host` would clash with.
fn first_clash(output: &AtomicStructure, host: u32, p: DVec3, factor: f64) -> Option<u32> {
    let r = vdw(1);
    output
        .atoms_values()
        .filter(|other| other.atomic_number > 0 && p.distance(other.position) > 1e-9)
        .filter(|other| tested_against(output, host, other.id))
        .find(|other| p.distance(other.position) < factor * (r + vdw(other.atomic_number)))
        .map(|other| other.id)
}

/// Both directions of §6 Phase 1's oracle check, for a passivation of
/// `hosts` at `factor`. Returns the number skipped.
fn assert_rule_holds(input: &AtomicStructure, hosts: &HashSet<u32>, factor: f64) -> usize {
    let (output, result) = passivate_hosts(input, hosts, Some(factor));
    let terminators = placed(input, &output);
    assert_eq!(terminators.len(), result.atoms_added);

    // Sound: no placed terminator is inside the rule with anything.
    for &(id, host) in &terminators {
        let p = output.get_atom(id).unwrap().position;
        assert_eq!(
            first_clash(&output, host, p, factor),
            None,
            "terminator {id} on {host} clashes"
        );
    }

    // Not over-eager: every slot the rule skipped would clash, added alone.
    let (all, _) = passivate_hosts(input, hosts, None);
    let kept: Vec<(DVec3, u32)> = terminators
        .iter()
        .map(|&(id, host)| (output.get_atom(id).unwrap().position, host))
        .collect();
    let mut skipped = 0;
    for (id, host) in placed(input, &all) {
        let p = all.get_atom(id).unwrap().position;
        if kept.iter().any(|&(q, h)| h == host && q.distance(p) < 1e-9) {
            continue;
        }
        skipped += 1;
        assert!(
            first_clash(&output, host, p, factor).is_some(),
            "slot at {p:?} on {host} was skipped without a clash"
        );
    }
    assert_eq!(skipped, result.skipped);
    assert_eq!(
        result.atoms_added + result.skipped,
        placed(input, &all).len()
    );
    skipped
}

/// The smallest `distance / (r_vdW(H) + r_vdW(other))` over the pairs the
/// rule tests, with every terminator placed (no rule).
fn nearest_contact(input: &AtomicStructure, hosts: &HashSet<u32>) -> f64 {
    let (output, _) = passivate_hosts(input, hosts, None);
    let mut best = f64::MAX;
    for (id, host) in placed(input, &output) {
        let p = output.get_atom(id).unwrap().position;
        for other in output.atoms_values() {
            if other.id == id
                || other.atomic_number <= 0
                || !tested_against(&output, host, other.id)
            {
                continue;
            }
            let ratio = p.distance(other.position) / (vdw(1) + vdw(other.atomic_number));
            best = best.min(ratio);
        }
    }
    best
}

#[test]
fn an_inert_clash_rule_changes_nothing() {
    // `clash: None` is the code path every existing test above exercises; a
    // rule that can never reject must produce the same atoms, positions and
    // ids on every fixture.
    let mut fixtures: Vec<AtomicStructure> = vec![
        make_structure(6, &[]).0,
        make_structure(7, &[]).0,
        make_structure(8, &[(6, DVec3::new(1.43, 0.0, 0.0))]).0,
        make_structure(6, &[(6, DVec3::new(1.545, 0.0, 0.0))]).0,
    ];
    for (s, hosts) in [si100_dimers(), si100_dihydride(), c100_dihydride(), si111()] {
        let mut s = s;
        let ids: Vec<u32> = s.atom_ids().copied().collect();
        for id in ids {
            if !hosts.contains(&id) {
                s.set_atom_hydrogen_passivation(id, true); // skipped by the default options
            }
        }
        fixtures.push(s);
    }
    for input in fixtures {
        let mut none = input.clone();
        let mut inert = input.clone();
        let a = add_hydrogens(&mut none, &clash_options(None));
        let b = add_hydrogens(&mut inert, &clash_options(Some(1e-9)));
        assert_eq!(a.skipped, 0);
        assert_eq!(b.skipped, 0);
        assert_eq!(a.atoms_added, b.atoms_added);
        let atoms = |s: &AtomicStructure| -> Vec<(u32, i16, [u64; 3])> {
            s.atoms_values()
                .map(|atom| {
                    let p = atom.position;
                    (
                        atom.id,
                        atom.atomic_number,
                        [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()],
                    )
                })
                .collect()
        };
        assert_eq!(atoms(&none), atoms(&inert));
        assert_eq!(none.get_num_of_bonds(), inert.get_num_of_bonds());
    }
}

#[test]
fn surface_fixtures_nearest_contacts() {
    // The measured numbers of §8.1: the reconstructed and the (111) surface
    // must clear the default factor, the dihydrides must fall inside it.
    let report = [
        ("Si(100) 2x1 monohydride", si100_dimers()),
        ("Si(111) 1x1", si111()),
        ("Si(100) 1x1 dihydride", si100_dihydride()),
        ("C(100) 1x1 dihydride", c100_dihydride()),
    ]
    .map(|(name, (s, hosts))| {
        let ratio = nearest_contact(&s, &hosts);
        println!("{name}: nearest contact {ratio:.3} of the vdW sum");
        ratio
    });
    assert!(report[0] > DEFAULT_PASSIVATION_CLASH_FACTOR);
    assert!(report[1] > DEFAULT_PASSIVATION_CLASH_FACTOR);
    assert!(report[2] < DEFAULT_PASSIVATION_CLASH_FACTOR);
    assert!(report[3] < DEFAULT_PASSIVATION_CLASH_FACTOR);
}

#[test]
fn monohydride_and_111_surfaces_skip_nothing() {
    for (s, hosts) in [si100_dimers(), si111()] {
        let (_, result) = passivate_hosts(&s, &hosts, Some(DEFAULT_PASSIVATION_CLASH_FACTOR));
        assert_eq!(result.skipped, 0);
        assert_eq!(result.atoms_added, hosts.len(), "one H per host");
    }
}

#[test]
fn dihydride_surfaces_skip_clashing_terminators() {
    let factor = DEFAULT_PASSIVATION_CLASH_FACTOR;
    for (s, hosts) in [si100_dihydride(), c100_dihydride()] {
        assert!(assert_rule_holds(&s, &hosts, factor) > 0);
    }

    // Si(100): the clashes are H–H (1.42 Å), so the rule is greedy rather
    // than all-or-nothing: every host keeps at least one H.
    let (s, hosts) = si100_dihydride();
    let (out, _) = passivate_hosts(&s, &hosts, Some(factor));
    for &host in &hosts {
        assert!(count_h_bonded_to(&out, host) >= 1);
    }

    // C(100): every H also sits 1.75 Å from the neighbouring surface carbon
    // (0.59 of the H–C sum), a structure atom, so at the default factor no
    // slot survives and the surface stays bare (§8.1).
    let (s, hosts) = c100_dihydride();
    let (_, result) = passivate_hosts(&s, &hosts, Some(factor));
    assert_eq!((result.atoms_added, result.skipped), (0, 2 * hosts.len()));
    // Below that contact only the overlapping H–H pairs (0.74 Å) clash, and
    // one H of each survives.
    let (out, result) = passivate_hosts(&s, &hosts, Some(0.5));
    assert!(result.skipped > 0);
    for &host in &hosts {
        assert!(count_h_bonded_to(&out, host) >= 1);
    }
}

#[test]
fn the_oracle_holds_across_factors() {
    for (s, hosts) in [si100_dimers(), si100_dihydride(), c100_dihydride(), si111()] {
        for factor in [0.5, 0.7, 0.9, 1.1] {
            assert_rule_holds(&s, &hosts, factor);
        }
    }
}

/// Two Si hosts, each with three Si neighbours behind it, whose one open
/// slot each points at the other: the two H would sit 1.24 Å apart, each
/// 2.72 Å from the other host (outside 0.7 of the H–Si sum). `first` is the
/// host built first, and so the one with the lower id.
fn facing_hosts(first_at_origin: bool) -> (AtomicStructure, u32, u32) {
    let d = 4.2;
    let mut s = AtomicStructure::new();
    let add_host = |s: &mut AtomicStructure, at: DVec3, open: DVec3| {
        let host = s.add_atom(14, at);
        // Three neighbours tetrahedral to `open`.
        let u = open.any_orthonormal_vector();
        let v = open.cross(u);
        let cone = (180.0f64 - 109.47).to_radians();
        for k in 0..3 {
            let phi = (k as f64) * 120f64.to_radians();
            let dir = -open * cone.cos() + (u * phi.cos() + v * phi.sin()) * cone.sin();
            let n = s.add_atom(14, at + dir.normalize() * 2.35);
            s.add_bond(host, n, 1);
        }
        host
    };
    let (a, b) = if first_at_origin {
        let a = add_host(&mut s, DVec3::ZERO, DVec3::X);
        let b = add_host(&mut s, DVec3::new(d, 0.0, 0.0), DVec3::NEG_X);
        (a, b)
    } else {
        let b = add_host(&mut s, DVec3::new(d, 0.0, 0.0), DVec3::NEG_X);
        let a = add_host(&mut s, DVec3::ZERO, DVec3::X);
        (a, b)
    };
    (s, a, b)
}

#[test]
fn of_a_clashing_pair_the_lower_id_host_keeps_its_terminator() {
    for first_at_origin in [true, false] {
        let (s, a, b) = facing_hosts(first_at_origin);
        let hosts: HashSet<u32> = [a, b].into();
        let (out, result) = passivate_hosts(&s, &hosts, Some(DEFAULT_PASSIVATION_CLASH_FACTOR));
        assert_eq!((result.atoms_added, result.skipped), (1, 1));
        let survivor = a.min(b);
        assert_eq!(count_h_bonded_to(&out, survivor), 1);
        assert_eq!(count_h_bonded_to(&out, a.max(b)), 0);
        // Renumbering swaps which host survives.
        let expected = if first_at_origin { a } else { b };
        assert_eq!(survivor, expected);
        // Without the rule both are placed.
        let (_, none) = passivate_hosts(&s, &hosts, None);
        assert_eq!((none.atoms_added, none.skipped), (2, 0));
    }
}

/// A zigzag chain of `n` bare carbons on diamond-lattice positions.
fn carbon_chain(n: usize) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let mut prev = None;
    for i in 0..n {
        let k = i as f64;
        let p = DVec3::new(k, k, (i % 2) as f64) * (A_C / 4.0);
        let id = s.add_atom(6, p);
        if let Some(prev) = prev {
            s.add_bond(prev, id, 1);
        }
        prev = Some(id);
    }
    s
}

#[test]
fn methyl_terminated_chains_passivate_fully() {
    // The 1–3 exclusion: every H sits ~2.15 Å from the neighbouring C, inside
    // 0.7 of the H–C sum, and must not be skipped for it.
    for n in 2..=6 {
        let mut s = carbon_chain(n);
        let result = add_hydrogens(
            &mut s,
            &clash_options(Some(DEFAULT_PASSIVATION_CLASH_FACTOR)),
        );
        assert_eq!(result.skipped, 0, "C{n}");
        assert_eq!(result.atoms_added, 2 * n + 2, "C{n}H{}", 2 * n + 2);
    }
    // Neopentane: four methyls on one carbon.
    let dirs = tetrahedral_dirs(1.545);
    let (mut s, _) = make_structure(6, &[(6, dirs[0]), (6, dirs[1]), (6, dirs[2]), (6, dirs[3])]);
    let result = add_hydrogens(
        &mut s,
        &clash_options(Some(DEFAULT_PASSIVATION_CLASH_FACTOR)),
    );
    assert_eq!((result.atoms_added, result.skipped), (12, 0));
}

#[test]
fn geminal_terminators_do_not_clash_with_each_other() {
    // The two H of an ideal OH2 are 1.57 Å apart, the three of NH3 1.65 Å,
    // both inside 0.7 of the H–H sum (1.68 Å). They share a host, so they are
    // 1–3 partners and the rule does not test them against each other.
    for (z, expected) in [(8, 2), (7, 3), (6, 4), (14, 4)] {
        let (mut s, _) = make_structure(z, &[]);
        let result = add_hydrogens(
            &mut s,
            &clash_options(Some(DEFAULT_PASSIVATION_CLASH_FACTOR)),
        );
        assert_eq!((result.atoms_added, result.skipped), (expected, 0), "Z={z}");
    }
}

#[test]
fn a_terminator_inside_an_unrelated_molecule_is_skipped() {
    // An Si host whose one open slot points up; methane above it.
    let build = |methane_height: f64| {
        let (mut s, host) = make_structure(14, &[]);
        // Three legs of a tetrahedron around -z, so the free one is +z.
        let cone = (180.0f64 - 109.47).to_radians();
        for k in 0..3 {
            let phi = (k as f64) * 120f64.to_radians();
            let dir = DVec3::new(phi.cos() * cone.sin(), phi.sin() * cone.sin(), -cone.cos());
            let n = s.add_atom(14, dir * 2.35);
            s.add_bond(host, n, 1);
        }
        let c = s.add_atom(6, DVec3::new(0.0, 0.0, methane_height));
        for d in tetrahedral_dirs(1.09) {
            let h = s.add_atom(1, DVec3::new(0.0, 0.0, methane_height) + d);
            s.add_bond(c, h, 1);
        }
        (s, host)
    };
    let factor = Some(DEFAULT_PASSIVATION_CLASH_FACTOR);
    // The H would sit at z = 1.48, 1.0 Å below the methane carbon.
    let (s, host) = build(2.48);
    let hosts: HashSet<u32> = [host].into();
    let (out, result) = passivate_hosts(&s, &hosts, factor);
    assert_eq!((result.atoms_added, result.skipped), (0, 1));
    assert_eq!(count_h_bonded_to(&out, host), 0);
    // Moved away, the terminator is placed again.
    let (s, host) = build(10.0);
    let hosts: HashSet<u32> = [host].into();
    let (out, result) = passivate_hosts(&s, &hosts, factor);
    assert_eq!((result.atoms_added, result.skipped), (1, 0));
    assert_eq!(count_h_bonded_to(&out, host), 1);
}
