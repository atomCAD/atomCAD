use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::crystolecule::unit_cell_symmetries::{
    classify_crystal_system, analyze_unit_cell_symmetries, analyze_unit_cell_complete,
    CrystalSystem, RotationalSymmetry
};
use glam::f64::{DVec3, DMat3};
use glam::i32::IVec3;

/// Helper function to create a test unit cell with given parameters
fn create_test_unit_cell(a: f64, b: f64, c: f64, alpha: f64, beta: f64, gamma: f64) -> UnitCellStruct {
    // Convert angles from degrees to radians for calculation
    let alpha_rad = alpha.to_radians();
    let beta_rad = beta.to_radians();
    let gamma_rad = gamma.to_radians();
    
    // Calculate basis vectors using standard crystallographic convention
    let basis_a = DVec3::new(a, 0.0, 0.0);
    let basis_b = DVec3::new(b * gamma_rad.cos(), b * gamma_rad.sin(), 0.0);
    
    let cos_alpha = alpha_rad.cos();
    let cos_beta = beta_rad.cos();
    let cos_gamma = gamma_rad.cos();
    let sin_gamma = gamma_rad.sin();
    
    let c_x = c * cos_beta;
    let c_y = c * (cos_alpha - cos_beta * cos_gamma) / sin_gamma;
    let c_z_squared = c * c - c_x * c_x - c_y * c_y;
    let c_z = if c_z_squared > 0.0 { c_z_squared.sqrt() } else { 0.0 };
    let basis_c = DVec3::new(c_x, c_y, c_z);
    
    UnitCellStruct {
        a: basis_a,
        b: basis_b,
        c: basis_c,
        cell_length_a: a,
        cell_length_b: b,
        cell_length_c: c,
        cell_angle_alpha: alpha,
        cell_angle_beta: beta,
        cell_angle_gamma: gamma,
    }
}

#[test]
fn test_cubic_classification() {
    let cubic_cell = create_test_unit_cell(5.0, 5.0, 5.0, 90.0, 90.0, 90.0);
    let system = classify_crystal_system(&cubic_cell);
    assert_eq!(system, CrystalSystem::Cubic);
}

#[test]
fn test_tetragonal_classification() {
    let tetragonal_cell = create_test_unit_cell(4.0, 4.0, 6.0, 90.0, 90.0, 90.0);
    let system = classify_crystal_system(&tetragonal_cell);
    assert_eq!(system, CrystalSystem::Tetragonal(2)); // c-axis is unique (a=b≠c)
}

#[test]
fn test_orthorhombic_classification() {
    let orthorhombic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 90.0, 90.0, 90.0);
    let system = classify_crystal_system(&orthorhombic_cell);
    assert_eq!(system, CrystalSystem::Orthorhombic);
}

#[test]
fn test_hexagonal_classification() {
    let hexagonal_cell = create_test_unit_cell(4.0, 4.0, 6.0, 90.0, 90.0, 120.0);
    let system = classify_crystal_system(&hexagonal_cell);
    assert_eq!(system, CrystalSystem::Hexagonal(2)); // c-axis is unique (γ=120°)
}

#[test]
fn test_trigonal_classification() {
    let trigonal_cell = create_test_unit_cell(5.0, 5.0, 5.0, 75.0, 75.0, 75.0);
    let system = classify_crystal_system(&trigonal_cell);
    assert_eq!(system, CrystalSystem::Trigonal);
}

#[test]
fn test_monoclinic_classification() {
    let monoclinic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 90.0, 110.0, 90.0);
    let system = classify_crystal_system(&monoclinic_cell);
    assert_eq!(system, CrystalSystem::Monoclinic(1)); // b-axis is unique (β=110°≠90°)
}

#[test]
fn test_triclinic_classification() {
    let triclinic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 85.0, 95.0, 105.0);
    let system = classify_crystal_system(&triclinic_cell);
    assert_eq!(system, CrystalSystem::Triclinic);
}

#[test]
fn test_cubic_symmetries_count() {
    let cubic_cell = UnitCellStruct::cubic_diamond();
    let symmetries = analyze_unit_cell_symmetries(&cubic_cell);
    
    // Cubic should have 13 rotational symmetries:
    // 3 four-fold + 4 three-fold + 6 two-fold = 13
    assert_eq!(symmetries.len(), 13);
    
    // Count by n-fold
    let four_fold_count = symmetries.iter().filter(|s| s.n_fold == 4).count();
    let three_fold_count = symmetries.iter().filter(|s| s.n_fold == 3).count();
    let two_fold_count = symmetries.iter().filter(|s| s.n_fold == 2).count();
    
    assert_eq!(four_fold_count, 3);  // Along a, b, c axes
    assert_eq!(three_fold_count, 4); // Along body diagonals
    assert_eq!(two_fold_count, 6);   // Along face diagonals
}

#[test]
fn test_tetragonal_symmetries_count() {
    let tetragonal_cell = create_test_unit_cell(4.0, 4.0, 6.0, 90.0, 90.0, 90.0);
    let symmetries = analyze_unit_cell_symmetries(&tetragonal_cell);
    
    // Tetragonal should have 5 rotational symmetries:
    // 1 four-fold + 4 two-fold = 5
    assert_eq!(symmetries.len(), 5);
    
    let four_fold_count = symmetries.iter().filter(|s| s.n_fold == 4).count();
    let two_fold_count = symmetries.iter().filter(|s| s.n_fold == 2).count();
    
    assert_eq!(four_fold_count, 1); // Along c-axis
    assert_eq!(two_fold_count, 4);  // Along a, b axes and face diagonals
}

#[test]
fn test_orthorhombic_symmetries_count() {
    let orthorhombic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 90.0, 90.0, 90.0);
    let symmetries = analyze_unit_cell_symmetries(&orthorhombic_cell);
    
    // Orthorhombic should have 3 rotational symmetries:
    // 3 two-fold = 3
    assert_eq!(symmetries.len(), 3);
    
    let two_fold_count = symmetries.iter().filter(|s| s.n_fold == 2).count();
    assert_eq!(two_fold_count, 3); // Along a, b, c axes
}

#[test]
fn test_trigonal_symmetries_count() {
    let trigonal_cell = create_test_unit_cell(4.5, 4.5, 4.5, 75.0, 75.0, 75.0);
    let symmetries = analyze_unit_cell_symmetries(&trigonal_cell);
    
    // Trigonal should have 1 rotational symmetry:
    // 1 three-fold along main body diagonal [111]
    assert_eq!(symmetries.len(), 1);
    
    let three_fold_count = symmetries.iter().filter(|s| s.n_fold == 3).count();
    assert_eq!(three_fold_count, 1); // Along [111] body diagonal
}

#[test]
fn test_triclinic_no_symmetries() {
    let triclinic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 85.0, 95.0, 105.0);
    let symmetries = analyze_unit_cell_symmetries(&triclinic_cell);
    
    // Triclinic should have no rotational symmetries
    assert_eq!(symmetries.len(), 0);
}

#[test]
fn test_rotational_symmetry_creation() {
    let axis = DVec3::new(1.0, 0.0, 0.0);
    let symmetry = RotationalSymmetry::new(axis, 4);
    
    assert_eq!(symmetry.n_fold, 4);
    assert_eq!(symmetry.smallest_angle_degrees(), 90.0);
    assert!((symmetry.smallest_angle_radians() - std::f64::consts::PI / 2.0).abs() < 1e-10);
    
    // Axis should be normalized
    assert!((symmetry.axis.length() - 1.0).abs() < 1e-10);
}

#[test]
fn test_analyze_unit_cell_complete() {
    let cubic_cell = UnitCellStruct::cubic_diamond();
    let (system, symmetries) = analyze_unit_cell_complete(&cubic_cell);
    
    assert_eq!(system, CrystalSystem::Cubic);
    assert_eq!(symmetries.len(), 13);
}

// ================================
// PERMUTATION TESTS
// Test different axis permutations for systems where it matters
// ================================

#[test]
fn test_tetragonal_a_unique() {
    // a≠b=c case: a-axis is unique
    let tetragonal_cell = create_test_unit_cell(6.0, 4.0, 4.0, 90.0, 90.0, 90.0);
    let system = classify_crystal_system(&tetragonal_cell);
    assert_eq!(system, CrystalSystem::Tetragonal(0)); // a-axis is unique
    
    // Test symmetries still work correctly
    let symmetries = analyze_unit_cell_symmetries(&tetragonal_cell);
    assert_eq!(symmetries.len(), 5);
    let four_fold_count = symmetries.iter().filter(|s| s.n_fold == 4).count();
    assert_eq!(four_fold_count, 1); // Along a-axis (unique)
}

#[test]
fn test_tetragonal_b_unique() {
    // a=c≠b case: b-axis is unique
    let tetragonal_cell = create_test_unit_cell(4.0, 6.0, 4.0, 90.0, 90.0, 90.0);
    let system = classify_crystal_system(&tetragonal_cell);
    assert_eq!(system, CrystalSystem::Tetragonal(1)); // b-axis is unique
    
    // Test symmetries still work correctly
    let symmetries = analyze_unit_cell_symmetries(&tetragonal_cell);
    assert_eq!(symmetries.len(), 5);
    let four_fold_count = symmetries.iter().filter(|s| s.n_fold == 4).count();
    assert_eq!(four_fold_count, 1); // Along b-axis (unique)
}

#[test]
fn test_hexagonal_a_unique() {
    // b=c≠a, β=γ=90°, α=120° case: a-axis is unique
    let hexagonal_cell = create_test_unit_cell(6.0, 4.0, 4.0, 120.0, 90.0, 90.0);
    let system = classify_crystal_system(&hexagonal_cell);
    assert_eq!(system, CrystalSystem::Hexagonal(0)); // a-axis is unique (α=120°)
    
    // Test symmetries still work correctly
    let symmetries = analyze_unit_cell_symmetries(&hexagonal_cell);
    assert_eq!(symmetries.len(), 7); // 1 six-fold + 6 two-fold
    let six_fold_count = symmetries.iter().filter(|s| s.n_fold == 6).count();
    assert_eq!(six_fold_count, 1); // Along a-axis (unique)
}

#[test]
fn test_hexagonal_b_unique() {
    // a=c≠b, α=γ=90°, β=120° case: b-axis is unique
    let hexagonal_cell = create_test_unit_cell(4.0, 6.0, 4.0, 90.0, 120.0, 90.0);
    let system = classify_crystal_system(&hexagonal_cell);
    assert_eq!(system, CrystalSystem::Hexagonal(1)); // b-axis is unique (β=120°)
    
    // Test symmetries still work correctly
    let symmetries = analyze_unit_cell_symmetries(&hexagonal_cell);
    assert_eq!(symmetries.len(), 7); // 1 six-fold + 6 two-fold
    let six_fold_count = symmetries.iter().filter(|s| s.n_fold == 6).count();
    assert_eq!(six_fold_count, 1); // Along b-axis (unique)
}

#[test]
fn test_monoclinic_a_unique() {
    // β=γ=90°≠α case: a-axis is unique
    let monoclinic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 110.0, 90.0, 90.0);
    let system = classify_crystal_system(&monoclinic_cell);
    assert_eq!(system, CrystalSystem::Monoclinic(0)); // a-axis is unique (α≠90°)
    
    // Test symmetries still work correctly
    let symmetries = analyze_unit_cell_symmetries(&monoclinic_cell);
    assert_eq!(symmetries.len(), 1); // 1 two-fold along unique axis
    let two_fold_count = symmetries.iter().filter(|s| s.n_fold == 2).count();
    assert_eq!(two_fold_count, 1); // Along a-axis (unique)
}

#[test]
fn test_monoclinic_c_unique() {
    // α=β=90°≠γ case: c-axis is unique
    let monoclinic_cell = create_test_unit_cell(3.0, 4.0, 5.0, 90.0, 90.0, 110.0);
    let system = classify_crystal_system(&monoclinic_cell);
    assert_eq!(system, CrystalSystem::Monoclinic(2)); // c-axis is unique (γ≠90°)
    
    // Test symmetries still work correctly
    let symmetries = analyze_unit_cell_symmetries(&monoclinic_cell);
    assert_eq!(symmetries.len(), 1); // 1 two-fold along unique axis
    let two_fold_count = symmetries.iter().filter(|s| s.n_fold == 2).count();
    assert_eq!(two_fold_count, 1); // Along c-axis (unique)
}

// ================================
// COMPREHENSIVE SYMMETRY VALIDATION TESTS
// Tests that verify rotational symmetries actually transform lattice points to lattice points
// ================================

/// Generates pseudorandom lattice points for testing
/// Returns 15 IVec3 points distributed around the origin
fn generate_test_lattice_points() -> Vec<IVec3> {
    vec![
        IVec3::new(0, 0, 0),    // Origin
        IVec3::new(1, 0, 0),    // Unit vectors
        IVec3::new(0, 1, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(1, 1, 0),    // Face diagonals
        IVec3::new(1, 0, 1),
        IVec3::new(0, 1, 1),
        IVec3::new(1, 1, 1),    // Body diagonal
        IVec3::new(-1, 2, 1),   // Some arbitrary points
        IVec3::new(2, -1, 3),
        // Additional larger coordinate points for more robust testing
        IVec3::new(5, -3, 7),   // Larger coordinates
        IVec3::new(-4, 8, 2),
        IVec3::new(9, 1, -6),
        IVec3::new(-7, -5, 10),
        IVec3::new(6, 9, -8),
    ]
}

/// Creates a rotation matrix from axis and angle
/// 
/// # Arguments
/// * `axis` - Normalized rotation axis
/// * `angle_radians` - Rotation angle in radians
/// 
/// # Returns
/// * 3x3 rotation matrix
fn create_rotation_matrix(axis: DVec3, angle_radians: f64) -> DMat3 {
    let cos_theta = angle_radians.cos();
    let sin_theta = angle_radians.sin();
    let one_minus_cos = 1.0 - cos_theta;
    
    let x = axis.x;
    let y = axis.y;
    let z = axis.z;
    
    // Rodrigues' rotation formula
    DMat3::from_cols(
        DVec3::new(
            cos_theta + x * x * one_minus_cos,
            y * x * one_minus_cos + z * sin_theta,
            z * x * one_minus_cos - y * sin_theta,
        ),
        DVec3::new(
            x * y * one_minus_cos - z * sin_theta,
            cos_theta + y * y * one_minus_cos,
            z * y * one_minus_cos + x * sin_theta,
        ),
        DVec3::new(
            x * z * one_minus_cos + y * sin_theta,
            y * z * one_minus_cos - x * sin_theta,
            cos_theta + z * z * one_minus_cos,
        ),
    )
}

/// Tests if a DVec3 point is close to integer coordinates
/// 
/// # Arguments
/// * `point` - The point to test
/// * `tolerance` - Maximum allowed deviation from integer coordinates
/// 
/// # Returns
/// * true if all coordinates are within tolerance of integers
fn is_close_to_integer_coordinates(point: DVec3, tolerance: f64) -> bool {
    let x_close = (point.x - point.x.round()).abs() < tolerance;
    let y_close = (point.y - point.y.round()).abs() < tolerance;
    let z_close = (point.z - point.z.round()).abs() < tolerance;
    
    x_close && y_close && z_close
}

/// Validates that all rotational symmetries of a unit cell actually transform lattice points to lattice points
/// 
/// This is the core test function that:
/// 1. Generates test lattice points
/// 2. For each symmetry, creates rotation matrices for all equivalent rotations
/// 3. For each lattice point, converts to real space, rotates, converts back to lattice space
/// 4. Verifies the result is close to integer coordinates
/// 
/// # Arguments
/// * `unit_cell` - The unit cell to test
/// * `test_name` - Name for error messages
fn validate_symmetries_preserve_lattice(unit_cell: &UnitCellStruct, test_name: &str) {
    let symmetries = analyze_unit_cell_symmetries(unit_cell);
    let test_points = generate_test_lattice_points();
    let tolerance = 1e-10; // Very strict tolerance for lattice point preservation
    
    println!("Testing {} with {} symmetries and {} lattice points", 
             test_name, symmetries.len(), test_points.len());
    
    for (sym_idx, symmetry) in symmetries.iter().enumerate() {
        // Test all equivalent rotations for this symmetry element
        for rotation_step in 1..symmetry.n_fold {
            let angle = (rotation_step as f64) * symmetry.smallest_angle_radians();
            let rotation_matrix = create_rotation_matrix(symmetry.axis, angle);
            
            for (point_idx, &lattice_point) in test_points.iter().enumerate() {
                // Convert lattice point to real space
                let real_point = unit_cell.ivec3_lattice_to_real(&lattice_point);
                
                // Apply rotation in real space
                let rotated_real_point = rotation_matrix * real_point;
                
                // Convert back to lattice space
                let rotated_lattice_point = unit_cell.real_to_dvec3_lattice(&rotated_real_point);
                
                // Check if result is close to integer coordinates
                if !is_close_to_integer_coordinates(rotated_lattice_point, tolerance) {
                    panic!(
                        "{}: Symmetry {} (axis: {:.6}, {:.6}, {:.6}, {}-fold, rotation {}/{}) failed for lattice point {} ({}, {}, {})\n\
                         Real point: ({:.6}, {:.6}, {:.6})\n\
                         Rotated real: ({:.6}, {:.6}, {:.6})\n\
                         Rotated lattice: ({:.6}, {:.6}, {:.6})\n\
                         Expected integer coordinates but got deviations: ({:.2e}, {:.2e}, {:.2e})",
                        test_name, sym_idx, 
                        symmetry.axis.x, symmetry.axis.y, symmetry.axis.z, 
                        symmetry.n_fold, rotation_step, symmetry.n_fold - 1,
                        point_idx, lattice_point.x, lattice_point.y, lattice_point.z,
                        real_point.x, real_point.y, real_point.z,
                        rotated_real_point.x, rotated_real_point.y, rotated_real_point.z,
                        rotated_lattice_point.x, rotated_lattice_point.y, rotated_lattice_point.z,
                        rotated_lattice_point.x - rotated_lattice_point.x.round(),
                        rotated_lattice_point.y - rotated_lattice_point.y.round(),
                        rotated_lattice_point.z - rotated_lattice_point.z.round()
                    );
                }
            }
        }
    }
    
    println!("✓ {} passed all symmetry validation tests", test_name);
}

#[test]
fn test_cubic_symmetry_validation() {
    let cubic_cell = create_test_unit_cell(5.000, 5.000, 5.000, 90.0, 90.0, 90.0);
    validate_symmetries_preserve_lattice(&cubic_cell, "Cubic");
}

#[test]
fn test_tetragonal_symmetry_validation() {
    let tetragonal_cell = create_test_unit_cell(4.000, 4.000, 6.500, 90.0, 90.0, 90.0);
    validate_symmetries_preserve_lattice(&tetragonal_cell, "Tetragonal");
}

#[test]
fn test_orthorhombic_symmetry_validation() {
    let orthorhombic_cell = create_test_unit_cell(3.100, 4.250, 5.600, 90.0, 90.0, 90.0);
    validate_symmetries_preserve_lattice(&orthorhombic_cell, "Orthorhombic");
}

#[test]
fn test_monoclinic_symmetry_validation() {
    let monoclinic_cell = create_test_unit_cell(5.000, 6.000, 4.200, 90.0, 110.0, 90.0);
    validate_symmetries_preserve_lattice(&monoclinic_cell, "Monoclinic");
}

#[test]
fn test_triclinic_symmetry_validation() {
    let triclinic_cell = create_test_unit_cell(4.120, 5.370, 6.890, 83.3, 97.5, 110.2);
    validate_symmetries_preserve_lattice(&triclinic_cell, "Triclinic");
}

#[test]
fn test_hexagonal_symmetry_validation() {
    let hexagonal_cell = create_test_unit_cell(2.460, 2.460, 6.700, 90.0, 90.0, 120.0);
    validate_symmetries_preserve_lattice(&hexagonal_cell, "Hexagonal");
}

#[test]
fn test_trigonal_symmetry_validation() {
    let trigonal_cell = create_test_unit_cell(4.500, 4.500, 4.500, 75.0, 75.0, 75.0);
    validate_symmetries_preserve_lattice(&trigonal_cell, "Trigonal (Rhombohedral)");
}







