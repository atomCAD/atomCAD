use rust_lib_flutter_cad::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::structure_designer::evaluator::unit_cell_symmetries::{
    classify_crystal_system, analyze_unit_cell_symmetries, analyze_unit_cell_complete,
    CrystalSystem, RotationalSymmetry
};
use glam::f64::DVec3;

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
