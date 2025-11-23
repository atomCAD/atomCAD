use rust_lib_flutter_cad::crystolecule::unit_cell_struct::{UnitCellStruct, CrystalPlaneProps};
use glam::f64::DVec3;
use glam::i32::IVec3;

#[cfg(test)]
mod unit_cell_tests {
    use super::*;

/// Test the round-trip conversion: lattice -> real -> lattice
#[test]
fn test_lattice_real_round_trip_cubic() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    // Test various lattice positions
    let test_positions = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(-1.0, 2.0, -0.5),
        DVec3::new(0.5, -0.25, 1.75),
    ];
    
    for original_lattice in test_positions {
        // Convert lattice -> real -> lattice
        let real_pos = unit_cell.dvec3_lattice_to_real(&original_lattice);
        let recovered_lattice = unit_cell.real_to_dvec3_lattice(&real_pos);
        
        // Check that we get back the original position (within numerical precision)
        let diff = (recovered_lattice - original_lattice).length();
        assert!(diff < 1e-10, 
            "Round-trip failed for {:?}: got {:?}, diff = {}", 
            original_lattice, recovered_lattice, diff);
    }
}

/// Test the round-trip conversion with a non-orthogonal unit cell
#[test]
fn test_lattice_real_round_trip_hexagonal() {
    // Create a hexagonal unit cell (120° angle between a and b)
    let a = 4.0;
    let c = 6.0;
    let unit_cell = UnitCellStruct {
        a: DVec3::new(a, 0.0, 0.0),
        b: DVec3::new(-a * 0.5, a * (3.0_f64.sqrt() / 2.0), 0.0), // 120° rotation
        c: DVec3::new(0.0, 0.0, c),
        cell_length_a: a,
        cell_length_b: a,
        cell_length_c: c,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 90.0,
        cell_angle_gamma: 120.0,
    };
    
    let test_positions = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(1.0, 1.0, 0.0),
        DVec3::new(0.5, 0.5, 0.5),
        DVec3::new(-0.25, 1.5, -0.75),
    ];
    
    for original_lattice in test_positions {
        let real_pos = unit_cell.dvec3_lattice_to_real(&original_lattice);
        let recovered_lattice = unit_cell.real_to_dvec3_lattice(&real_pos);
        
        let diff = (recovered_lattice - original_lattice).length();
        assert!(diff < 1e-10, 
            "Hexagonal round-trip failed for {:?}: got {:?}, diff = {}", 
            original_lattice, recovered_lattice, diff);
    }
}

/// Test the round-trip conversion with a triclinic unit cell (most general case)
#[test]
fn test_lattice_real_round_trip_triclinic() {
    // Create a general triclinic unit cell with arbitrary angles
    let unit_cell = UnitCellStruct {
        a: DVec3::new(3.0, 0.0, 0.0),
        b: DVec3::new(1.0, 4.0, 0.0),
        c: DVec3::new(0.5, 1.5, 5.0),
        cell_length_a: 3.0,
        cell_length_b: 4.123, // sqrt(1^2 + 4^2) = sqrt(17)
        cell_length_c: 5.220, // sqrt(0.5^2 + 1.5^2 + 5^2) = sqrt(27.5)
        cell_angle_alpha: 75.0,
        cell_angle_beta: 85.0,
        cell_angle_gamma: 95.0,
    };
    
    let test_positions = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(-2.0, 3.0, -1.0),
        DVec3::new(0.33, -0.67, 2.25),
    ];
    
    for original_lattice in test_positions {
        let real_pos = unit_cell.dvec3_lattice_to_real(&original_lattice);
        let recovered_lattice = unit_cell.real_to_dvec3_lattice(&real_pos);
        
        let diff = (recovered_lattice - original_lattice).length();
        assert!(diff < 1e-10, 
            "Triclinic round-trip failed for {:?}: got {:?}, diff = {}", 
            original_lattice, recovered_lattice, diff);
    }
}

/// Test that the inverse transformation is mathematically correct
#[test]
fn test_inverse_transformation_identity() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    // Test that converting the unit cell basis vectors gives the identity
    let lattice_a = DVec3::new(1.0, 0.0, 0.0);
    let lattice_b = DVec3::new(0.0, 1.0, 0.0);
    let lattice_c = DVec3::new(0.0, 0.0, 1.0);
    
    let real_a = unit_cell.dvec3_lattice_to_real(&lattice_a);
    let real_b = unit_cell.dvec3_lattice_to_real(&lattice_b);
    let real_c = unit_cell.dvec3_lattice_to_real(&lattice_c);
    
    // These should be the unit cell basis vectors
    assert!((real_a - unit_cell.a).length() < 1e-10);
    assert!((real_b - unit_cell.b).length() < 1e-10);
    assert!((real_c - unit_cell.c).length() < 1e-10);
    
    // Converting them back should give the original lattice coordinates
    let recovered_a = unit_cell.real_to_dvec3_lattice(&real_a);
    let recovered_b = unit_cell.real_to_dvec3_lattice(&real_b);
    let recovered_c = unit_cell.real_to_dvec3_lattice(&real_c);
    
    assert!((recovered_a - lattice_a).length() < 1e-10);
    assert!((recovered_b - lattice_b).length() < 1e-10);
    assert!((recovered_c - lattice_c).length() < 1e-10);
}

/// Test conversion with integer lattice coordinates
#[test]
fn test_ivec3_lattice_conversion() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    let test_positions = vec![
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 2, -3),
        IVec3::new(5, -2, 1),
    ];
    
    for original_lattice in test_positions {
        // Convert integer lattice -> real -> lattice (float)
        let real_pos = unit_cell.ivec3_lattice_to_real(&original_lattice);
        let recovered_lattice = unit_cell.real_to_dvec3_lattice(&real_pos);
        
        // Should match the original integer coordinates (as floats)
        let expected = original_lattice.as_dvec3();
        let diff = (recovered_lattice - expected).length();
        assert!(diff < 1e-10, 
            "IVec3 round-trip failed for {:?}: got {:?}, diff = {}", 
            original_lattice, recovered_lattice, diff);
    }
}

/// Test that zero vector converts correctly
#[test]
fn test_zero_vector_conversion() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    let zero_lattice = DVec3::ZERO;
    let zero_real = unit_cell.dvec3_lattice_to_real(&zero_lattice);
    let recovered_lattice = unit_cell.real_to_dvec3_lattice(&zero_real);
    
    assert!(zero_real.length() < 1e-10, "Zero lattice should map to zero real");
    assert!(recovered_lattice.length() < 1e-10, "Zero real should map to zero lattice");
}

/// Test Miller index to plane properties conversion
#[test]
fn test_miller_index_plane_properties() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    // Test some common Miller indices for cubic systems
    let test_indices = vec![
        IVec3::new(1, 0, 0),  // (100) plane
        IVec3::new(0, 1, 0),  // (010) plane
        IVec3::new(0, 0, 1),  // (001) plane
        IVec3::new(1, 1, 0),  // (110) plane
        IVec3::new(1, 1, 1),  // (111) plane
    ];
    
    for miller_index in test_indices {
        let plane_props = unit_cell.ivec3_miller_index_to_plane_props(&miller_index);
        
        // Normal should be normalized
        let normal_length = plane_props.normal.length();
        assert!((normal_length - 1.0).abs() < 1e-10, 
            "Normal should be normalized for {:?}, got length {}", 
            miller_index, normal_length);
        
        // d-spacing should be positive
        assert!(plane_props.d_spacing > 0.0, 
            "d-spacing should be positive for {:?}, got {}", 
            miller_index, plane_props.d_spacing);
        
        // For cubic systems, verify some known relationships
        if miller_index == IVec3::new(1, 0, 0) {
            // (100) plane normal should be along x-axis
            let expected_normal = DVec3::new(1.0, 0.0, 0.0);
            let diff = (plane_props.normal - expected_normal).length();
            assert!(diff < 1e-10, "(100) normal should be (1,0,0), got {:?}", plane_props.normal);
        }
    }
}

/// Test consistency between old and new Miller index methods
#[test]
fn test_miller_index_method_consistency() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    let test_indices = vec![
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(1, 1, 0),
        IVec3::new(1, 1, 1),
        IVec3::new(2, 1, 0),
    ];
    
    for miller_index in test_indices {
        // Compare old method (normal only) with new method (plane properties)
        let old_normal = unit_cell.ivec3_miller_index_to_normal(&miller_index);
        let plane_props = unit_cell.ivec3_miller_index_to_plane_props(&miller_index);
        
        // Normals should be identical
        let diff = (old_normal - plane_props.normal).length();
        assert!(diff < 1e-10, 
            "Normal methods should be consistent for {:?}: old={:?}, new={:?}", 
            miller_index, old_normal, plane_props.normal);
    }
}

/// Test error handling for degenerate unit cells
#[test]
#[should_panic(expected = "Unit cell matrix is singular")]
fn test_singular_matrix_panic() {
    // Create a degenerate unit cell (all vectors in the same plane)
    let degenerate_unit_cell = UnitCellStruct {
        a: DVec3::new(1.0, 0.0, 0.0),
        b: DVec3::new(2.0, 0.0, 0.0),  // Parallel to a
        c: DVec3::new(3.0, 0.0, 0.0),  // Also parallel to a
        cell_length_a: 1.0,
        cell_length_b: 2.0,
        cell_length_c: 3.0,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 90.0,
        cell_angle_gamma: 90.0,
    };
    
    let real_pos = DVec3::new(1.0, 1.0, 1.0);
    
    // This should panic because the matrix is singular
    degenerate_unit_cell.real_to_dvec3_lattice(&real_pos);
}

/// Test numerical precision with very small and very large values
#[test]
fn test_numerical_precision() {
    let unit_cell = UnitCellStruct::cubic_diamond();
    
    // Test with very small values
    let small_lattice = DVec3::new(1e-8, -1e-8, 1e-8);
    let small_real = unit_cell.dvec3_lattice_to_real(&small_lattice);
    let recovered_small = unit_cell.real_to_dvec3_lattice(&small_real);
    
    let small_diff = (recovered_small - small_lattice).length();
    assert!(small_diff < 1e-15, "Small value precision test failed: diff = {}", small_diff);
    
    // Test with large values
    let large_lattice = DVec3::new(1e6, -1e6, 1e6);
    let large_real = unit_cell.dvec3_lattice_to_real(&large_lattice);
    let recovered_large = unit_cell.real_to_dvec3_lattice(&large_real);
    
    let large_diff = (recovered_large - large_lattice).length();
    assert!(large_diff < 1e-6, "Large value precision test failed: diff = {}", large_diff);
}

} // End of unit_cell_tests module




