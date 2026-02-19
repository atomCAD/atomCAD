use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::guided_placement::*;

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

    let (structure, anchor_id) = make_structure_with_neighbors(6, &[(1, h1), (1, h2), (1, h3)]);

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
    let all_dirs: Vec<DVec3> = vec![h1.normalize(), h2.normalize(), h3.normalize(), guide_dir];
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
    assert!(
        result
            .guide_dots()
            .iter()
            .all(|d| d.dot_type == GuideDotType::Primary)
    );

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

    let (structure, anchor_id) = make_structure_with_neighbors(6, &[(1, h1), (1, h2), (1, h3)]);

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
    if let GuidedPlacementMode::FreeSphere {
        center,
        radius,
        preview_position,
    } = &result.mode
    {
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
// Phase C: sp3 case 1 — dihedral-aware placement
// ============================================================================

#[test]
fn sp3_case1_with_dihedral_reference_gives_6_dots() {
    // Ethane-like: C-C where the second carbon has one hydrogen.
    // Anchor = first carbon (1 bond to second carbon).
    // Second carbon's H provides dihedral reference.
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO); // anchor
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0)); // neighbor
    structure.add_bond(c1, c2, 1);
    // Add a hydrogen to C2 to provide dihedral reference
    let h = structure.add_atom(1, DVec3::new(1.545 + 0.7, 0.7, 0.0));
    structure.add_bond(c2, h, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1, // placing hydrogen
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 3);
    // Should get FixedDots with 6 dots (3 trans + 3 cis)
    assert!(!result.mode.is_free_sphere());
    assert!(!result.mode.is_free_ring());
    assert_eq!(result.guide_dots().len(), 6);

    // First 3 should be Primary (trans), last 3 should be Secondary (cis)
    for dot in &result.guide_dots()[..3] {
        assert_eq!(dot.dot_type, GuideDotType::Primary);
    }
    for dot in &result.guide_dots()[3..] {
        assert_eq!(dot.dot_type, GuideDotType::Secondary);
    }
}

#[test]
fn sp3_case1_dihedral_dots_at_tetrahedral_angle() {
    // C-C with H on second carbon
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);
    let h = structure.add_atom(1, DVec3::new(1.545 + 0.7, 0.7, 0.0));
    structure.add_bond(c2, h, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    let bond_dir = DVec3::new(1.0, 0.0, 0.0); // C1→C2 direction
    let tet_angle = 109.47_f64;

    // All 6 guide dots should be at ~109.47° from the existing bond direction
    for (i, dot) in result.guide_dots().iter().enumerate() {
        let guide_dir = dot.position.normalize(); // anchor at origin
        let angle_deg = guide_dir.angle_between(bond_dir).to_degrees();
        assert!(
            (angle_deg - tet_angle).abs() < 1.0,
            "Guide dot {} angle = {:.2}° (expected ~{:.2}°)",
            i,
            angle_deg,
            tet_angle,
        );
    }
}

#[test]
fn sp3_case1_trans_dots_staggered_120_apart() {
    // Trans dots should be at 120° angular spacing around the bond axis
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);
    let h = structure.add_atom(1, DVec3::new(1.545 + 0.7, 0.7, 0.0));
    structure.add_bond(c2, h, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    let bond_axis = DVec3::new(1.0, 0.0, 0.0);
    let trans_dots = &result.guide_dots()[..3];

    // Project trans dots onto plane perpendicular to bond axis
    let projected: Vec<DVec3> = trans_dots
        .iter()
        .map(|d| {
            let p = d.position - bond_axis * d.position.dot(bond_axis);
            p.normalize()
        })
        .collect();

    // Each pair of projected directions should be at ~120° apart
    for i in 0..3 {
        for j in (i + 1)..3 {
            let angle = projected[i].angle_between(projected[j]).to_degrees();
            assert!(
                (angle - 120.0).abs() < 1.0,
                "Trans dots {} and {} projected angle = {:.2}° (expected ~120°)",
                i,
                j,
                angle,
            );
        }
    }
}

#[test]
fn sp3_case1_guide_dots_at_correct_distance() {
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);
    let h = structure.add_atom(1, DVec3::new(1.545 + 0.7, 0.7, 0.0));
    structure.add_bond(c2, h, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1, // placing H
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );

    for (i, dot) in result.guide_dots().iter().enumerate() {
        let dist = dot.position.length(); // anchor at origin
        assert!(
            (dist - result.bond_distance).abs() < 0.01,
            "Guide dot {} distance = {:.4} (expected {:.4})",
            i,
            dist,
            result.bond_distance,
        );
    }
}

// ============================================================================
// Phase C: sp3 case 1 — no dihedral reference → FreeRing
// ============================================================================

#[test]
fn sp3_case1_no_dihedral_returns_free_ring() {
    // C-C where second C has no other bonds → no dihedral reference
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 3);
    assert!(result.mode.is_free_ring());
    assert!(result.guide_dots().is_empty()); // FreeRing has no fixed dots
}

#[test]
fn sp3_case1_ring_geometry_correct() {
    // Ring center, normal, and radius should match the tetrahedral cone geometry
    let bond_dist = 1.545; // C-C crystal
    let tet_angle = 109.47_f64.to_radians();
    let cone_half_angle = std::f64::consts::PI - tet_angle;

    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(bond_dist, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        6, // C-C so crystal bond length is used
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    if let GuidedPlacementMode::FreeRing {
        ring_center,
        ring_normal,
        ring_radius,
        bond_distance: bd,
        anchor_pos,
        ..
    } = &result.mode
    {
        // Bond distance should be C-C crystal
        assert!((*bd - bond_dist).abs() < 0.001, "Bond distance = {}", bd);

        // Ring normal should point away from the existing bond (i.e., -X direction)
        let expected_normal = DVec3::new(-1.0, 0.0, 0.0);
        assert!(
            ring_normal.dot(expected_normal) > 0.99,
            "Ring normal = {:?}",
            ring_normal
        );

        // Ring center should be along -bond_dir at cos(cone_half_angle) * bond_dist
        let expected_center_x = -bond_dist * cone_half_angle.cos();
        assert!(
            (ring_center.x - expected_center_x).abs() < 0.01,
            "Ring center x = {} (expected {})",
            ring_center.x,
            expected_center_x
        );

        // Ring radius should be sin(cone_half_angle) * bond_dist
        let expected_radius = bond_dist * cone_half_angle.sin();
        assert!(
            (*ring_radius - expected_radius).abs() < 0.01,
            "Ring radius = {} (expected {})",
            ring_radius,
            expected_radius
        );

        // Anchor pos should be at origin
        assert_eq!(*anchor_pos, DVec3::ZERO);
    } else {
        panic!("Expected FreeRing mode, got {:?}", result.mode);
    }
}

// ============================================================================
// Phase C: dihedral reference finding
// ============================================================================

#[test]
fn find_dihedral_reference_ethane_like() {
    // C1-C2 where C2 has an H → dihedral reference exists
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);
    let h = structure.add_atom(1, DVec3::new(1.545, 1.0, 0.0));
    structure.add_bond(c2, h, 1);

    let ref_perp = find_dihedral_reference(&structure, c1, c2);
    assert!(ref_perp.is_some());

    let ref_dir = ref_perp.unwrap();
    // Reference should be perpendicular to C1-C2 axis (X axis)
    let bond_axis = DVec3::new(1.0, 0.0, 0.0);
    assert!(
        ref_dir.dot(bond_axis).abs() < 1e-6,
        "Dihedral ref should be perpendicular to bond axis, dot = {}",
        ref_dir.dot(bond_axis)
    );
}

#[test]
fn find_dihedral_reference_bare_neighbor_returns_none() {
    // C1-C2 where C2 has no other bonds → no dihedral reference
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);

    let ref_perp = find_dihedral_reference(&structure, c1, c2);
    assert!(ref_perp.is_none());
}

// ============================================================================
// Phase C: ray-ring intersection
// ============================================================================

#[test]
fn ray_ring_hit() {
    // Ring in the XY plane at z=1, radius=1, centered at origin
    let center = DVec3::new(0.0, 0.0, 1.0);
    let normal = DVec3::new(0.0, 0.0, 1.0);
    let radius = 1.0;

    // Ray from (2, 0, 0) toward +Z should hit the ring plane at (2, 0, 1),
    // then project to the closest point on the circle = (1, 0, 1)
    let hit = ray_ring_nearest_point(
        &DVec3::new(2.0, 0.0, 0.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &center,
        &normal,
        radius,
    );
    assert!(hit.is_some());
    let p = hit.unwrap();
    assert!((p.x - 1.0).abs() < 1e-6, "Hit x = {}", p.x);
    assert!(p.y.abs() < 1e-6, "Hit y = {}", p.y);
    assert!((p.z - 1.0).abs() < 1e-6, "Hit z = {}", p.z);
}

#[test]
fn ray_ring_parallel_returns_none() {
    // Ray parallel to ring plane
    let hit = ray_ring_nearest_point(
        &DVec3::new(0.0, 0.0, 0.0),
        &DVec3::new(1.0, 0.0, 0.0),
        &DVec3::new(0.0, 0.0, 1.0),
        &DVec3::new(0.0, 0.0, 1.0),
        1.0,
    );
    assert!(hit.is_none());
}

#[test]
fn ray_ring_hit_on_circle() {
    // Hit point should be on the circle (distance from center = radius)
    let center = DVec3::new(1.0, 2.0, 3.0);
    let normal = DVec3::new(0.0, 1.0, 0.0);
    let radius = 2.0;

    let hit = ray_ring_nearest_point(
        &DVec3::new(5.0, 0.0, 3.0),
        &DVec3::new(0.0, 1.0, 0.0),
        &center,
        &normal,
        radius,
    );
    assert!(hit.is_some());
    let p = hit.unwrap();
    let dist = (p - center).length();
    assert!(
        (dist - radius).abs() < 1e-6,
        "Hit should be on circle, dist from center = {}",
        dist
    );
}

// ============================================================================
// Phase C: ring preview positions
// ============================================================================

#[test]
fn ring_preview_positions_120_apart() {
    use rust_lib_flutter_cad::crystolecule::guided_placement::compute_ring_preview_positions;

    let ring_center = DVec3::new(0.0, 0.0, -0.5);
    let ring_normal = DVec3::new(0.0, 0.0, -1.0);
    let ring_radius = 0.9;
    let anchor_pos = DVec3::ZERO;
    let bond_distance = 1.545;
    let point_on_ring = DVec3::new(ring_radius, 0.0, -0.5);

    use rust_lib_flutter_cad::crystolecule::guided_placement::cone_half_angle_for_ring;
    let num_dots = 3;
    let positions = compute_ring_preview_positions(
        ring_center,
        ring_normal,
        ring_radius,
        anchor_pos,
        bond_distance,
        point_on_ring,
        num_dots,
        cone_half_angle_for_ring(num_dots),
    );

    // All 3 positions should be at bond_distance from anchor
    for (i, pos) in positions.iter().enumerate() {
        let dist = pos.length(); // anchor at origin
        assert!(
            (dist - bond_distance).abs() < 0.01,
            "Position {} distance = {} (expected {})",
            i,
            dist,
            bond_distance,
        );
    }

    // Mutual angles between positions should all be ~109.47° (tetrahedral)
    // Wait — these are NOT at tetrahedral angles to each other. They're at
    // ~120° projected spacing around the cone. Let's check the projected angle.
    let bond_axis = DVec3::new(0.0, 0.0, 1.0); // C1→C2 = +Z (ring_normal is -Z)
    let projected: Vec<DVec3> = positions
        .iter()
        .map(|p| {
            let proj = *p - bond_axis * p.dot(bond_axis);
            proj.normalize()
        })
        .collect();

    for i in 0..3 {
        for j in (i + 1)..3 {
            let angle = projected[i].angle_between(projected[j]).to_degrees();
            assert!(
                (angle - 120.0).abs() < 1.0,
                "Projected angle between {} and {} = {:.2}° (expected ~120°)",
                i,
                j,
                angle,
            );
        }
    }
}

// ============================================================================
// Phase C: transition from ring to fixed dots
// ============================================================================

#[test]
fn case1_with_dihedral_then_case2_after_placing() {
    // After placing an atom from case 1, the anchor now has 2 bonds → case 2
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO);
    let c2 = structure.add_atom(6, DVec3::new(1.545, 0.0, 0.0));
    structure.add_bond(c1, c2, 1);
    // Add a second bond to C1 (simulating placement)
    let h = structure.add_atom(1, DVec3::new(-0.5, 0.8, 0.0));
    structure.add_bond(c1, h, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 2);
    // Should be FixedDots with 2 guide dots (sp3 case 2)
    assert!(!result.mode.is_free_sphere());
    assert!(!result.mode.is_free_ring());
    assert_eq!(result.guide_dots().len(), 2);
}

// ============================================================================
// Phase D: sp2 geometry (120° trigonal planar)
// ============================================================================

#[test]
fn sp2_case2_two_bonds_one_guide() {
    // Formaldehyde-like: C=O + 1H, but we model as 2 single bonds for simplicity.
    // Carbon at origin with 2 bonds in the XY plane at ~120°.
    let d = 1.5;
    let b1 = DVec3::new(d, 0.0, 0.0);
    let b2 = DVec3::new(-d * 0.5, d * 0.866, 0.0); // 120° from b1
    let (structure, anchor) = make_structure_with_neighbors(6, &[(1, b1), (8, b2)]);

    let result = compute_guided_placement(
        &structure,
        anchor,
        1, // H
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.hybridization, Hybridization::Sp2);
    assert_eq!(result.remaining_slots, 1);
    assert_eq!(result.guide_dots().len(), 1);

    // The guide dot should be at ~120° from both existing bonds (opposite their midpoint)
    let anchor_pos = DVec3::ZERO;
    let dot_dir = (result.guide_dots()[0].position - anchor_pos).normalize();
    let b1_dir = b1.normalize();
    let b2_dir = b2.normalize();

    let angle1 = dot_dir.angle_between(b1_dir).to_degrees();
    let angle2 = dot_dir.angle_between(b2_dir).to_degrees();

    assert!(
        (angle1 - 120.0).abs() < 2.0,
        "Angle to b1 = {:.2}° (expected ~120°)",
        angle1
    );
    assert!(
        (angle2 - 120.0).abs() < 2.0,
        "Angle to b2 = {:.2}° (expected ~120°)",
        angle2
    );
}

#[test]
fn sp2_case1_with_reference_two_guides() {
    // Carbon with 1 bond along +X, neighbor has another bond (planar reference).
    let mut structure = AtomicStructure::new();
    let c1 = structure.add_atom(6, DVec3::ZERO); // anchor
    let c2 = structure.add_atom(6, DVec3::new(1.5, 0.0, 0.0)); // neighbor
    structure.add_bond(c1, c2, 1);
    // Give neighbor a second bond to provide planar reference
    let h = structure.add_atom(1, DVec3::new(1.5, 1.0, 0.0));
    structure.add_bond(c2, h, 1);

    let result = compute_guided_placement(
        &structure,
        c1,
        1, // H
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.hybridization, Hybridization::Sp2);
    assert_eq!(result.remaining_slots, 2);
    assert_eq!(result.guide_dots().len(), 2);

    // Both guide dots should be at 120° from the existing bond
    let bond_dir = DVec3::new(1.0, 0.0, 0.0);
    for (i, dot) in result.guide_dots().iter().enumerate() {
        let dot_dir = dot.position.normalize();
        let angle = dot_dir.angle_between(bond_dir).to_degrees();
        assert!(
            (angle - 120.0).abs() < 2.0,
            "Guide dot {} angle to bond = {:.2}° (expected ~120°)",
            i,
            angle
        );
    }

    // The two guides should be 120° apart from each other
    let d1 = result.guide_dots()[0].position.normalize();
    let d2 = result.guide_dots()[1].position.normalize();
    let mutual_angle = d1.angle_between(d2).to_degrees();
    assert!(
        (mutual_angle - 120.0).abs() < 2.0,
        "Mutual angle = {:.2}° (expected ~120°)",
        mutual_angle
    );
}

#[test]
fn sp2_case1_no_reference_returns_ring() {
    // Carbon with 1 bond, neighbor has no other bonds → ring fallback
    let (structure, anchor) = make_structure_with_neighbors(6, &[(6, DVec3::new(1.5, 0.0, 0.0))]);

    let result = compute_guided_placement(
        &structure,
        anchor,
        1, // H
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.hybridization, Hybridization::Sp2);
    assert!(
        result.mode.is_free_ring(),
        "Expected FreeRing mode for sp2 case 1 without reference"
    );
    if let GuidedPlacementMode::FreeRing { num_ring_dots, .. } = &result.mode {
        assert_eq!(*num_ring_dots, 2, "sp2 ring should have 2 dots");
    }
}

#[test]
fn sp2_case0_returns_free_sphere() {
    // Bare atom with sp2 override → free sphere
    let (structure, anchor) = make_structure_with_neighbors(6, &[]);

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert!(result.mode.is_free_sphere());
}

#[test]
fn sp2_saturated_no_guides() {
    // Carbon with 3 bonds and sp2 → saturated
    let d = 1.5;
    let (structure, anchor) = make_structure_with_neighbors(
        6,
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-d * 0.5, d * 0.866, 0.0)),
            (1, DVec3::new(-d * 0.5, -d * 0.866, 0.0)),
        ],
    );

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 0);
    assert_eq!(result.guide_dots().len(), 0);
}

// ============================================================================
// Phase D: sp1 geometry (180° linear)
// ============================================================================

#[test]
fn sp1_case1_opposite_direction() {
    // Carbon with 1 bond along +X → guide dot at -X
    let (structure, anchor) = make_structure_with_neighbors(6, &[(6, DVec3::new(1.5, 0.0, 0.0))]);

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp1),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.hybridization, Hybridization::Sp1);
    assert_eq!(result.remaining_slots, 1);
    assert_eq!(result.guide_dots().len(), 1);

    let dot_dir = result.guide_dots()[0].position.normalize();
    let bond_dir = DVec3::new(1.0, 0.0, 0.0);
    let angle = dot_dir.angle_between(bond_dir).to_degrees();
    assert!(
        (angle - 180.0).abs() < 1.0,
        "sp1 guide should be at 180° from bond, got {:.2}°",
        angle
    );
}

#[test]
fn sp1_saturated_no_guides() {
    // Carbon with 2 bonds and sp1 → saturated
    let (structure, anchor) = make_structure_with_neighbors(
        6,
        &[
            (6, DVec3::new(1.5, 0.0, 0.0)),
            (6, DVec3::new(-1.5, 0.0, 0.0)),
        ],
    );

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp1),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.remaining_slots, 0);
    assert_eq!(result.guide_dots().len(), 0);
}

#[test]
fn sp1_case0_returns_free_sphere() {
    let (structure, anchor) = make_structure_with_neighbors(6, &[]);

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp1),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert!(result.mode.is_free_sphere());
}

// ============================================================================
// Phase D: hybridization override dispatch
// ============================================================================

#[test]
fn hybridization_override_sp2_changes_geometry() {
    // Carbon with 1 single bond: auto=sp3, override=sp2 should use 120° geometry
    let (structure, anchor) = make_structure_with_neighbors(6, &[(6, DVec3::new(1.5, 0.0, 0.0))]);

    // Auto: should be sp3
    let result_auto = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );
    assert_eq!(result_auto.hybridization, Hybridization::Sp3);

    // Override sp2: should give sp2 geometry
    let result_sp2 = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );
    assert_eq!(result_sp2.hybridization, Hybridization::Sp2);
}

#[test]
fn hybridization_override_sp1_changes_geometry() {
    // Carbon with 1 bond: override=sp1 should give 180° opposite dot
    let (structure, anchor) = make_structure_with_neighbors(6, &[(6, DVec3::new(1.5, 0.0, 0.0))]);

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp1),
        BondMode::Covalent,
        BondLengthMode::Crystal,
    );

    assert_eq!(result.hybridization, Hybridization::Sp1);
    assert_eq!(result.guide_dots().len(), 1);
}

// ============================================================================
// Phase D: bond mode (dative) tests
// ============================================================================

#[test]
fn dative_nitrogen_sp3_unlocks_lone_pair() {
    // NH3: nitrogen with 3 bonds. Covalent → saturated. Dative → 1 dot.
    let d = 1.0;
    let (structure, anchor) = make_structure_with_neighbors(
        7, // N
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-0.333 * d, 0.942 * d, 0.0)),
            (1, DVec3::new(-0.333 * d, -0.471 * d, 0.816 * d)),
        ],
    );

    // Covalent mode: saturated
    let result_cov = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );
    assert_eq!(result_cov.remaining_slots, 0);
    assert!(result_cov.has_additional_geometric_capacity);

    // Dative mode: 1 remaining slot (lone pair position)
    let result_dat = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Dative,
        BondLengthMode::Uff,
    );
    assert_eq!(result_dat.remaining_slots, 1);
    assert_eq!(result_dat.guide_dots().len(), 1);
}

#[test]
fn dative_oxygen_sp3_unlocks_two_lone_pairs() {
    // H2O: oxygen with 2 bonds. Covalent → saturated. Dative → 2 dots.
    let d = 0.96;
    let (structure, anchor) = make_structure_with_neighbors(
        8, // O
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-d * 0.5, d * 0.866, 0.0)),
        ],
    );

    // Covalent: saturated
    let result_cov = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );
    assert_eq!(result_cov.remaining_slots, 0);
    assert!(result_cov.has_additional_geometric_capacity);

    // Dative: 2 remaining (lone pair positions)
    let result_dat = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Dative,
        BondLengthMode::Uff,
    );
    assert_eq!(result_dat.remaining_slots, 2);
    assert_eq!(result_dat.guide_dots().len(), 2);
}

#[test]
fn dative_no_effect_on_carbon_sp3() {
    // CH3: carbon with 3 bonds. Both modes give 1 remaining slot.
    let d = 1.09;
    let tet = std::f64::consts::FRAC_PI_4; // placeholder for tetrahedral angle
    let _ = tet;
    let (structure, anchor) = make_structure_with_neighbors(
        6, // C
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-d * 0.333, d * 0.943, 0.0)),
            (1, DVec3::new(-d * 0.333, -d * 0.471, d * 0.816)),
        ],
    );

    let result_cov = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );
    let result_dat = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Dative,
        BondLengthMode::Uff,
    );

    assert_eq!(result_cov.remaining_slots, 1);
    assert_eq!(result_dat.remaining_slots, 1);
    assert!(!result_cov.has_additional_geometric_capacity);
}

#[test]
fn saturation_feedback_nitrogen_has_additional_capacity() {
    // NH3 in covalent mode: saturated but has additional capacity for dative
    let d = 1.0;
    let (structure, anchor) = make_structure_with_neighbors(
        7,
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-0.333 * d, 0.942 * d, 0.0)),
            (1, DVec3::new(-0.333 * d, -0.471 * d, 0.816 * d)),
        ],
    );

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );

    assert_eq!(result.remaining_slots, 0);
    assert!(
        result.has_additional_geometric_capacity,
        "NH3 should report additional geometric capacity (lone pair)"
    );
}

#[test]
fn carbon_sp3_no_additional_capacity() {
    // CH3 in covalent mode: not saturated, but also no additional capacity
    let d = 1.09;
    let (structure, anchor) = make_structure_with_neighbors(
        6,
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-d * 0.333, d * 0.943, 0.0)),
            (1, DVec3::new(-d * 0.333, -d * 0.471, d * 0.816)),
        ],
    );

    let result = compute_guided_placement(
        &structure,
        anchor,
        1,
        None,
        BondMode::Covalent,
        BondLengthMode::Uff,
    );

    assert!(
        !result.has_additional_geometric_capacity,
        "Carbon sp3 should NOT report additional capacity (covalent max = geometric max = 4)"
    );
}

// ============================================================================
// Phase D: sp2 ring preview positions
// ============================================================================

#[test]
fn sp2_ring_preview_positions_180_apart() {
    use rust_lib_flutter_cad::crystolecule::guided_placement::compute_ring_preview_positions;

    let ring_center = DVec3::new(0.0, 0.0, -0.5);
    let ring_normal = DVec3::new(0.0, 0.0, -1.0);
    let ring_radius = 0.9;
    let anchor_pos = DVec3::ZERO;
    let bond_distance = 1.545;
    let point_on_ring = DVec3::new(ring_radius, 0.0, -0.5);

    let num_dots = 2;
    let cone_half_angle = cone_half_angle_for_ring(num_dots);
    let positions = compute_ring_preview_positions(
        ring_center,
        ring_normal,
        ring_radius,
        anchor_pos,
        bond_distance,
        point_on_ring,
        num_dots,
        cone_half_angle,
    );

    assert_eq!(positions.len(), 2);

    // Both positions should be at bond_distance from anchor
    for (i, pos) in positions.iter().enumerate() {
        let dist = pos.length();
        assert!(
            (dist - bond_distance).abs() < 0.01,
            "Position {} distance = {} (expected {})",
            i,
            dist,
            bond_distance,
        );
    }

    // The two positions should be 180° apart projected onto the ring plane
    let bond_axis = DVec3::new(0.0, 0.0, 1.0);
    let p1_proj = (positions[0] - bond_axis * positions[0].dot(bond_axis)).normalize();
    let p2_proj = (positions[1] - bond_axis * positions[1].dot(bond_axis)).normalize();
    let proj_angle = p1_proj.angle_between(p2_proj).to_degrees();
    assert!(
        (proj_angle - 180.0).abs() < 1.0,
        "Projected angle between sp2 ring dots = {:.2}° (expected ~180°)",
        proj_angle,
    );
}

// ============================================================================
// Phase D: combined hybridization + bond mode
// ============================================================================

#[test]
fn boron_sp2_covalent_saturated_sp3_dative_unlocks() {
    // BH3: boron with 3 bonds (sp2 auto-detected).
    // Covalent sp2 → saturated (3 max).
    // Override to sp3 + Dative → 1 remaining slot.
    let d = 1.2;
    let (structure, anchor) = make_structure_with_neighbors(
        5, // B
        &[
            (1, DVec3::new(d, 0.0, 0.0)),
            (1, DVec3::new(-d * 0.5, d * 0.866, 0.0)),
            (1, DVec3::new(-d * 0.5, -d * 0.866, 0.0)),
        ],
    );

    // Covalent sp2: saturated
    let result_cov = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp2),
        BondMode::Covalent,
        BondLengthMode::Uff,
    );
    assert_eq!(result_cov.remaining_slots, 0);

    // Override sp3 + Dative: 1 remaining
    let result_sp3_dat = compute_guided_placement(
        &structure,
        anchor,
        1,
        Some(Hybridization::Sp3),
        BondMode::Dative,
        BondLengthMode::Uff,
    );
    assert_eq!(result_sp3_dat.remaining_slots, 1);
    assert_eq!(result_sp3_dat.guide_dots().len(), 1);
}
