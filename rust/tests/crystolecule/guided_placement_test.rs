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
    assert_eq!(result.guide_dots().len(), 1);
    assert_eq!(result.guide_dots()[0].dot_type, GuideDotType::Primary);

    // The 4th direction should be opposite the centroid of the first 3
    // Expected: (1, -1, -1).normalize()
    let guide_dir = (result.guide_dots()[0].position - DVec3::ZERO).normalize();
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
    assert_eq!(result.guide_dots().len(), 2);
    assert!(result.guide_dots().iter().all(|d| d.dot_type == GuideDotType::Primary));

    // Verify all 4 mutual angles are tetrahedral
    let all_dirs: Vec<DVec3> = vec![
        h1.normalize(),
        h2.normalize(),
        (result.guide_dots()[0].position).normalize(),
        (result.guide_dots()[1].position).normalize(),
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
    assert_eq!(result.guide_dots().len(), 0);
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
    assert_eq!(result.guide_dots().len(), 0);
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
    assert_eq!(result.guide_dots().len(), 1);
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

    assert_eq!(result.guide_dots().len(), 1);
    let guide_dist = result.guide_dots()[0].position.length(); // anchor is at origin
    assert!(
        (guide_dist - result.bond_distance).abs() < 0.01,
        "Guide dot distance ({:.4}) should match bond_distance ({:.4})",
        guide_dist,
        result.bond_distance
    );
}

// ============================================================================
// Phase B: sp3 case 0 — bare atom → FreeSphere
// ============================================================================

#[test]
fn sp3_case0_bare_atom_returns_free_sphere() {
    // Bare carbon with no bonds → should return FreeSphere mode
    let mut structure = AtomicStructure::new();
    let anchor_id = structure.add_atom(6, DVec3::ZERO);

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1, // placing hydrogen
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 4); // sp3 carbon, 0 bonds
    assert!(result.mode.is_free_sphere());
    assert!(result.guide_dots().is_empty()); // FreeSphere has no fixed dots

    // Check sphere parameters
    if let GuidedPlacementMode::FreeSphere { center, radius, preview_position } = &result.mode {
        assert_eq!(*center, DVec3::ZERO);
        assert!((*radius - result.bond_distance).abs() < 1e-10);
        assert!(preview_position.is_none());
    } else {
        panic!("Expected FreeSphere mode");
    }
}

#[test]
fn sp3_case0_sphere_radius_matches_bond_distance() {
    // Silicon bare atom placing carbon → crystal bond length for Si-C
    let mut structure = AtomicStructure::new();
    let anchor_id = structure.add_atom(14, DVec3::new(1.0, 2.0, 3.0));

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        6, // placing carbon
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert!(result.mode.is_free_sphere());
    if let GuidedPlacementMode::FreeSphere { center, radius, .. } = &result.mode {
        assert_eq!(*center, DVec3::new(1.0, 2.0, 3.0));
        assert!((*radius - 1.889).abs() < 0.001, "Si-C crystal = {}", radius); // Si-C crystal
    }
}

// ============================================================================
// Phase B: Ray-sphere intersection
// ============================================================================

#[test]
fn ray_sphere_hit_front() {
    // Ray from (0, 0, -5) toward +Z, sphere at origin radius 1
    let hit = ray_sphere_nearest_point(
        &DVec3::new(0.0, 0.0, -5.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &DVec3::ZERO,
        1.0,
    );
    assert!(hit.is_some());
    let p = hit.unwrap();
    assert!((p.z - (-1.0)).abs() < 1e-6, "Front hit z = {}", p.z);
    assert!(p.x.abs() < 1e-6);
    assert!(p.y.abs() < 1e-6);
}

#[test]
fn ray_sphere_miss() {
    // Ray that misses the sphere entirely
    let hit = ray_sphere_nearest_point(
        &DVec3::new(5.0, 0.0, -5.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &DVec3::ZERO,
        1.0,
    );
    assert!(hit.is_none());
}

#[test]
fn ray_sphere_tangent() {
    // Ray tangent to sphere (edge case)
    let hit = ray_sphere_nearest_point(
        &DVec3::new(1.0, 0.0, -5.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &DVec3::ZERO,
        1.0,
    );
    // Tangent: should hit at exactly z=0
    assert!(hit.is_some());
    let p = hit.unwrap();
    assert!((p.x - 1.0).abs() < 1e-6);
    assert!(p.z.abs() < 1e-6);
}

#[test]
fn ray_sphere_behind_ray() {
    // Sphere is entirely behind the ray origin
    let hit = ray_sphere_nearest_point(
        &DVec3::new(0.0, 0.0, 5.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &DVec3::ZERO,
        1.0,
    );
    assert!(hit.is_none());
}

#[test]
fn ray_sphere_origin_inside() {
    // Ray origin inside the sphere → should hit the exit point (front hemisphere)
    let hit = ray_sphere_nearest_point(
        &DVec3::new(0.0, 0.0, 0.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &DVec3::ZERO,
        1.0,
    );
    assert!(hit.is_some());
    let p = hit.unwrap();
    // Should hit the far side of the sphere at z=1
    assert!((p.z - 1.0).abs() < 1e-6, "Exit hit z = {}", p.z);
}

#[test]
fn ray_sphere_hit_on_surface() {
    // Hit point should be on the sphere surface
    let center = DVec3::new(1.0, 2.0, 3.0);
    let radius = 2.5;
    let hit = ray_sphere_nearest_point(
        &DVec3::new(1.0, 2.0, -5.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &center,
        radius,
    );
    assert!(hit.is_some());
    let p = hit.unwrap();
    let dist_from_center = (p - center).length();
    assert!(
        (dist_from_center - radius).abs() < 1e-6,
        "Hit point should be on sphere surface, dist = {}",
        dist_from_center
    );
}

// ============================================================================
// Phase B: case 1 still returns empty (stub for Phase C)
// ============================================================================

#[test]
fn sp3_case1_still_stub() {
    // Carbon with 1 bond → case 1 is still a stub (empty dots)
    let d = 1.09;
    let (structure, anchor_id) = make_structure_with_neighbors(
        6,
        &[(1, DVec3::new(1.0, 0.0, 0.0) * d)],
    );

    let result = compute_guided_placement(
        &structure,
        anchor_id,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 3);
    // Case 1 is still a stub → FixedDots with empty guide_dots
    assert!(!result.mode.is_free_sphere());
    assert!(result.guide_dots().is_empty());
}
