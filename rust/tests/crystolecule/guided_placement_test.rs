use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::guided_placement::*;
use glam::f64::DVec3;

/// Helper: create a structure with one anchor atom at origin and N neighbors.
fn make_structure_with_neighbors(
    anchor_z: i16,
    neighbor_positions: &[(i16, DVec3)],
) -> (AtomicStructure, u32) {
    let mut structure = AtomicStructure::new();
    let anchor_id = structure.add_atom(anchor_z, DVec3::ZERO);
    for &(z, pos) in neighbor_positions {
        let neighbor_id = structure.add_atom(z, pos);
        structure.add_bond(anchor_id, neighbor_id, 1); // single bond
    }
    (structure, anchor_id)
}

/// Helper: verify all mutual angles between directions are close to the tetrahedral angle.
fn assert_tetrahedral_angles(dirs: &[DVec3], tolerance_deg: f64) {
    let tet_angle_deg = 109.47;
    for i in 0..dirs.len() {
        for j in (i + 1)..dirs.len() {
            let angle_rad = dirs[i].normalize().angle_between(dirs[j].normalize());
            let angle_deg = angle_rad.to_degrees();
            assert!(
                (angle_deg - tet_angle_deg).abs() < tolerance_deg,
                "Angle between dir {} and dir {} = {:.2}° (expected ~{:.2}°)",
                i,
                j,
                angle_deg,
                tet_angle_deg,
            );
        }
    }
}

// ============================================================================
// sp3 case 3: 3 existing bonds → 1 guide dot
// ============================================================================

#[test]
fn sp3_case3_methyl_fourth_direction() {
    // CH3-like: carbon at origin with 3 hydrogens in tetrahedral positions
    let d = 1.09; // C-H distance
    // Three tetrahedral directions (pointing toward 3 of the 4 vertices of a tetrahedron)
    let h1 = DVec3::new(1.0, 1.0, 1.0).normalize() * d;
    let h2 = DVec3::new(-1.0, -1.0, 1.0).normalize() * d;
    let h3 = DVec3::new(-1.0, 1.0, -1.0).normalize() * d;

    let (structure, anchor_id) =
        make_structure_with_neighbors(6, &[(1, h1), (1, h2), (1, h3)]);

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1, // placing hydrogen
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 1);
    assert_eq!(result.guide_dots.len(), 1);
    assert_eq!(result.guide_dots[0].dot_type, GuideDotType::Primary);

    // The 4th direction should be opposite the centroid of the first 3
    // Expected: (1, -1, -1).normalize()
    let guide_dir = (result.guide_dots[0].position - DVec3::ZERO).normalize();
    let expected_dir = DVec3::new(1.0, -1.0, -1.0).normalize();
    let dot = guide_dir.dot(expected_dir);
    assert!(
        dot > 0.99,
        "Guide dot direction should be opposite centroid, got dot={:.4}",
        dot
    );

    // Verify all 4 mutual angles are tetrahedral
    let all_dirs: Vec<DVec3> = vec![
        h1.normalize(),
        h2.normalize(),
        h3.normalize(),
        guide_dir,
    ];
    assert_tetrahedral_angles(&all_dirs, 1.0);
}

// ============================================================================
// sp3 case 2: 2 existing bonds → 2 guide dots
// ============================================================================

#[test]
fn sp3_case2_two_bonds_two_guides() {
    let d = 1.09;
    let h1 = DVec3::new(1.0, 1.0, 1.0).normalize() * d;
    let h2 = DVec3::new(-1.0, -1.0, 1.0).normalize() * d;

    let (structure, anchor_id) = make_structure_with_neighbors(6, &[(1, h1), (1, h2)]);

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 2);
    assert_eq!(result.guide_dots.len(), 2);
    assert!(result.guide_dots.iter().all(|d| d.dot_type == GuideDotType::Primary));

    // Verify all 4 mutual angles are tetrahedral
    let all_dirs: Vec<DVec3> = vec![
        h1.normalize(),
        h2.normalize(),
        (result.guide_dots[0].position).normalize(),
        (result.guide_dots[1].position).normalize(),
    ];
    assert_tetrahedral_angles(&all_dirs, 1.5);
}

// ============================================================================
// sp3 case 4: saturated → 0 guide dots
// ============================================================================

#[test]
fn sp3_saturated_no_guides() {
    let d = 1.09;
    let h1 = DVec3::new(1.0, 1.0, 1.0).normalize() * d;
    let h2 = DVec3::new(-1.0, -1.0, 1.0).normalize() * d;
    let h3 = DVec3::new(-1.0, 1.0, -1.0).normalize() * d;
    let h4 = DVec3::new(1.0, -1.0, -1.0).normalize() * d;

    let (structure, anchor_id) =
        make_structure_with_neighbors(6, &[(1, h1), (1, h2), (1, h3), (1, h4)]);

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 0);
    assert_eq!(result.guide_dots.len(), 0);
}

// ============================================================================
// Bond distance: crystal mode
// ============================================================================

#[test]
fn bond_distance_crystal_c_c() {
    let d = bond_distance(6, 6, "C_3", BondLengthMode::Crystal);
    assert!((d - 1.545).abs() < 0.001, "C-C crystal = {}", d);
}

#[test]
fn bond_distance_crystal_si_si() {
    let d = bond_distance(14, 14, "Si3", BondLengthMode::Crystal);
    assert!((d - 2.352).abs() < 0.001, "Si-Si crystal = {}", d);
}

#[test]
fn bond_distance_crystal_si_c() {
    let d = bond_distance(14, 6, "Si3", BondLengthMode::Crystal);
    assert!((d - 1.889).abs() < 0.001, "Si-C crystal = {}", d);
}

#[test]
fn bond_distance_crystal_ga_as() {
    let d = bond_distance(31, 33, "Ga3+3", BondLengthMode::Crystal);
    assert!((d - 2.448).abs() < 0.001, "GaAs crystal = {}", d);
}

#[test]
fn bond_distance_crystal_b_n() {
    let d = bond_distance(5, 7, "B_3", BondLengthMode::Crystal);
    assert!((d - 1.567).abs() < 0.001, "BN crystal = {}", d);
}

// ============================================================================
// Bond distance: UFF mode
// ============================================================================

#[test]
fn bond_distance_uff_c_c() {
    let d = bond_distance(6, 6, "C_3", BondLengthMode::Uff);
    // UFF C_3-C_3 rest length ~ 1.514
    assert!(d > 1.4 && d < 1.6, "C-C UFF = {}", d);
}

#[test]
fn bond_distance_uff_c_h() {
    let d = bond_distance(6, 1, "C_3", BondLengthMode::Uff);
    // C-H UFF ~ 1.08-1.10
    assert!(d > 1.0 && d < 1.2, "C-H UFF = {}", d);
}

// ============================================================================
// Bond distance: crystal fallback to UFF
// ============================================================================

#[test]
fn bond_distance_crystal_fallback_c_h() {
    // C-H not in crystal table → falls back to UFF
    let d = bond_distance(6, 1, "C_3", BondLengthMode::Crystal);
    assert!(d > 1.0 && d < 1.2, "C-H crystal fallback = {}", d);
}

// ============================================================================
// Bond distance: UFF mode ignores crystal table
// ============================================================================

#[test]
fn bond_distance_uff_mode_ignores_crystal() {
    let d_uff = bond_distance(6, 6, "C_3", BondLengthMode::Uff);
    let d_crystal = bond_distance(6, 6, "C_3", BondLengthMode::Crystal);
    // Crystal gives 1.545, UFF gives ~1.514 — should differ
    assert!(
        (d_uff - d_crystal).abs() > 0.01,
        "UFF ({}) and crystal ({}) should differ for C-C",
        d_uff,
        d_crystal
    );
}

// ============================================================================
// Hybridization auto-detection
// ============================================================================

#[test]
fn hybridization_auto_carbon_sp3() {
    // Carbon with 4 single bonds → sp3
    let d = 1.09;
    let (structure, anchor_id) = make_structure_with_neighbors(
        6,
        &[
            (1, DVec3::new(1.0, 1.0, 1.0).normalize() * d),
            (1, DVec3::new(-1.0, -1.0, 1.0).normalize() * d),
            (1, DVec3::new(-1.0, 1.0, -1.0).normalize() * d),
            (1, DVec3::new(1.0, -1.0, -1.0).normalize() * d),
        ],
    );
    let h = detect_hybridization(&structure, anchor_id, None);
    assert_eq!(h, Hybridization::Sp3);
}

#[test]
fn hybridization_auto_bare_carbon_defaults_sp3() {
    let mut structure = AtomicStructure::new();
    let anchor_id = structure.add_atom(6, DVec3::ZERO);
    let h = detect_hybridization(&structure, anchor_id, None);
    assert_eq!(h, Hybridization::Sp3);
}

#[test]
fn hybridization_override_works() {
    let mut structure = AtomicStructure::new();
    let anchor_id = structure.add_atom(6, DVec3::ZERO);
    let h = detect_hybridization(&structure, anchor_id, Some(Hybridization::Sp2));
    assert_eq!(h, Hybridization::Sp2);
}

// ============================================================================
// Saturation limits
// ============================================================================

#[test]
fn saturation_nitrogen_sp3_covalent() {
    assert_eq!(
        effective_max_neighbors(7, Hybridization::Sp3, BondMode::Covalent),
        3
    );
}

#[test]
fn saturation_nitrogen_sp3_dative() {
    assert_eq!(
        effective_max_neighbors(7, Hybridization::Sp3, BondMode::Dative),
        4
    );
}

#[test]
fn saturation_oxygen_sp3_covalent() {
    assert_eq!(
        effective_max_neighbors(8, Hybridization::Sp3, BondMode::Covalent),
        2
    );
}

#[test]
fn saturation_oxygen_sp3_dative() {
    assert_eq!(
        effective_max_neighbors(8, Hybridization::Sp3, BondMode::Dative),
        4
    );
}

#[test]
fn saturation_fluorine() {
    assert_eq!(
        effective_max_neighbors(9, Hybridization::Sp3, BondMode::Covalent),
        1
    );
}

#[test]
fn saturation_carbon_sp3() {
    assert_eq!(
        effective_max_neighbors(6, Hybridization::Sp3, BondMode::Covalent),
        4
    );
}

#[test]
fn saturation_hydrogen() {
    assert_eq!(
        effective_max_neighbors(1, Hybridization::Sp3, BondMode::Covalent),
        1
    );
}

// ============================================================================
// Additional capacity detection
// ============================================================================

#[test]
fn nitrogen_has_additional_capacity() {
    // NH3: nitrogen sp3 with 3 bonds
    let d = 1.01; // N-H
    let (structure, anchor_id) = make_structure_with_neighbors(
        7,
        &[
            (1, DVec3::new(1.0, 1.0, 1.0).normalize() * d),
            (1, DVec3::new(-1.0, -1.0, 1.0).normalize() * d),
            (1, DVec3::new(-1.0, 1.0, -1.0).normalize() * d),
        ],
    );

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );

    assert_eq!(result.remaining_slots, 0);
    assert!(result.has_additional_geometric_capacity);
    assert_eq!(result.guide_dots.len(), 0);
}

#[test]
fn carbon_no_additional_capacity() {
    // CH3: carbon sp3 with 3 bonds — covalent max = geometric max = 4
    let d = 1.09;
    let (structure, anchor_id) = make_structure_with_neighbors(
        6,
        &[
            (1, DVec3::new(1.0, 1.0, 1.0).normalize() * d),
            (1, DVec3::new(-1.0, -1.0, 1.0).normalize() * d),
            (1, DVec3::new(-1.0, 1.0, -1.0).normalize() * d),
        ],
    );

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );

    assert_eq!(result.remaining_slots, 1);
    assert!(!result.has_additional_geometric_capacity);
    assert_eq!(result.guide_dots.len(), 1);
}

// ============================================================================
// Full guided placement: guide dot positions have correct bond distance
// ============================================================================

#[test]
fn guide_dot_distance_matches_bond_distance() {
    let d = 1.09;
    let h1 = DVec3::new(1.0, 1.0, 1.0).normalize() * d;
    let h2 = DVec3::new(-1.0, -1.0, 1.0).normalize() * d;
    let h3 = DVec3::new(-1.0, 1.0, -1.0).normalize() * d;

    let (structure, anchor_id) =
        make_structure_with_neighbors(6, &[(1, h1), (1, h2), (1, h3)]);

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );

    assert_eq!(result.guide_dots.len(), 1);
    let guide_dist = result.guide_dots[0].position.length(); // anchor is at origin
    assert!(
        (guide_dist - result.bond_distance).abs() < 0.01,
        "Guide dot distance ({:.4}) should match bond_distance ({:.4})",
        guide_dist,
        result.bond_distance
    );
}
