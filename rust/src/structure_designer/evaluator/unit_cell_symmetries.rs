use glam::f64::DVec3;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

/// Tolerance constants for crystal symmetry analysis
/// Using minimal set of constants for easy tuning
pub mod tolerance {
    /// Tolerance for comparing unit cell lengths (in Angstroms)
    /// Used for determining if lengths are equal (a=b, b=c, etc.)
    pub const LENGTH_EPSILON: f64 = 1e-6;
    
    /// Tolerance for comparing angles (in degrees)
    /// Used for determining if angles are equal or specific values (90°, 120°, etc.)
    pub const ANGLE_EPSILON: f64 = 1e-4;
}

/// Represents a rotational symmetry element in a crystal structure
#[derive(Debug, Clone, PartialEq)]
pub struct RotationalSymmetry {
    /// Normalized axis vector in real space coordinates
    pub axis: DVec3,
    /// N-fold rotation (2 = 2-fold, 3 = 3-fold, 4 = 4-fold, 6 = 6-fold)
    pub n_fold: u32,
}

impl RotationalSymmetry {
    /// Creates a new rotational symmetry element
    /// 
    /// # Arguments
    /// * `axis` - Rotation axis vector (will be normalized)
    /// * `n_fold` - N-fold rotation (must be 2, 3, 4, or 6)
    /// 
    /// # Panics
    /// * Panics if n_fold is not 2, 3, 4, or 6
    /// * Panics if axis is zero vector
    pub fn new(axis: DVec3, n_fold: u32) -> Self {
        if axis.length() < 1e-12 {
            panic!("Rotation axis cannot be zero vector");
        }
        
        match n_fold {
            2 | 3 | 4 | 6 => {},
            _ => panic!("Invalid n-fold rotation: {}. Must be 2, 3, 4, or 6", n_fold),
        }
        
        Self {
            axis: axis.normalize(),
            n_fold,
        }
    }
    
    /// Returns the smallest rotation angle in degrees for this symmetry element
    pub fn smallest_angle_degrees(&self) -> f64 {
        360.0 / (self.n_fold as f64)
    }
    
    /// Returns the smallest rotation angle in radians for this symmetry element
    pub fn smallest_angle_radians(&self) -> f64 {
        2.0 * std::f64::consts::PI / (self.n_fold as f64)
    }
}

/// The seven crystal systems based on unit cell parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrystalSystem {
    /// a=b=c, α=β=γ=90°
    Cubic,
    /// a=b≠c, α=β=γ=90°
    Tetragonal,
    /// a≠b≠c, α=β=γ=90°
    Orthorhombic,
    /// a=b≠c, α=β=90°, γ=120°
    Hexagonal,
    /// a=b=c, α=β=γ≠90° (and equal)
    Trigonal,
    /// a≠b≠c, α=γ=90°≠β
    Monoclinic,
    /// a≠b≠c, α≠β≠γ≠90°
    Triclinic,
}

impl CrystalSystem {
    /// Returns a human-readable name for the crystal system
    pub fn name(&self) -> &'static str {
        match self {
            CrystalSystem::Cubic => "Cubic",
            CrystalSystem::Tetragonal => "Tetragonal",
            CrystalSystem::Orthorhombic => "Orthorhombic",
            CrystalSystem::Hexagonal => "Hexagonal",
            CrystalSystem::Trigonal => "Trigonal",
            CrystalSystem::Monoclinic => "Monoclinic",
            CrystalSystem::Triclinic => "Triclinic",
        }
    }
}

/// Helper functions for crystal system classification
mod classification {
    use super::tolerance::{LENGTH_EPSILON, ANGLE_EPSILON};
    
    /// Checks if two lengths are approximately equal
    pub fn lengths_equal(a: f64, b: f64) -> bool {
        (a - b).abs() < LENGTH_EPSILON
    }
    
    /// Checks if an angle is approximately equal to a target value (in degrees)
    pub fn angle_equals(angle: f64, target: f64) -> bool {
        (angle - target).abs() < ANGLE_EPSILON
    }
    
    /// Checks if an angle is approximately 90 degrees
    pub fn is_right_angle(angle: f64) -> bool {
        angle_equals(angle, 90.0)
    }
    
    /// Checks if an angle is approximately 120 degrees
    pub fn is_120_degrees(angle: f64) -> bool {
        angle_equals(angle, 120.0)
    }
}

/// Classifies the crystal system based on unit cell parameters
/// 
/// # Arguments
/// * `unit_cell` - The unit cell structure containing both basis vectors and crystallographic parameters
/// 
/// # Returns
/// * The corresponding crystal system
pub fn classify_crystal_system(unit_cell: &UnitCellStruct) -> CrystalSystem {
    use classification::*;
    
    let a = unit_cell.cell_length_a;
    let b = unit_cell.cell_length_b;
    let c = unit_cell.cell_length_c;
    let alpha = unit_cell.cell_angle_alpha;
    let beta = unit_cell.cell_angle_beta;
    let gamma = unit_cell.cell_angle_gamma;
    
    // Check length relationships
    let a_eq_b = lengths_equal(a, b);
    let b_eq_c = lengths_equal(b, c);
    let a_eq_c = lengths_equal(a, c);
    let all_lengths_equal = a_eq_b && b_eq_c;
    
    // Check angle relationships
    let alpha_90 = is_right_angle(alpha);
    let beta_90 = is_right_angle(beta);
    let gamma_90 = is_right_angle(gamma);
    let all_angles_90 = alpha_90 && beta_90 && gamma_90;
    let gamma_120 = is_120_degrees(gamma);
    
    // Check if all angles are equal (for trigonal)
    let alpha_eq_beta = angle_equals(alpha, beta);
    let beta_eq_gamma = angle_equals(beta, gamma);
    let all_angles_equal = alpha_eq_beta && beta_eq_gamma;
    
    // Classification logic following crystallographic conventions
    if all_lengths_equal && all_angles_90 {
        // a=b=c, α=β=γ=90°
        CrystalSystem::Cubic
    } else if a_eq_b && !a_eq_c && all_angles_90 {
        // a=b≠c, α=β=γ=90°
        CrystalSystem::Tetragonal
    } else if !a_eq_b && !b_eq_c && !a_eq_c && all_angles_90 {
        // a≠b≠c, α=β=γ=90°
        CrystalSystem::Orthorhombic
    } else if a_eq_b && !a_eq_c && alpha_90 && beta_90 && gamma_120 {
        // a=b≠c, α=β=90°, γ=120°
        CrystalSystem::Hexagonal
    } else if all_lengths_equal && all_angles_equal && !all_angles_90 {
        // a=b=c, α=β=γ≠90° (and equal)
        CrystalSystem::Trigonal
    } else if !a_eq_b && !b_eq_c && !a_eq_c && alpha_90 && !beta_90 && gamma_90 {
        // a≠b≠c, α=γ=90°≠β
        CrystalSystem::Monoclinic
    } else {
        // Everything else: a≠b≠c, α≠β≠γ≠90°
        CrystalSystem::Triclinic
    }
}

/// Symmetry analysis functions for each crystal system
mod symmetry_analysis {
    use super::{RotationalSymmetry, UnitCellStruct};
    use glam::f64::DVec3;
    
    /// Analyzes rotational symmetries for cubic crystal system
    /// 
    /// Cubic system has:
    /// - 4-fold rotations along a, b, c axes (3 axes)
    /// - 3-fold rotations along body diagonals [111], [1̄11], [11̄1], [111̄] (4 axes)
    /// - 2-fold rotations along face diagonals [110], [101], [011], [1̄10], [101̄], [011̄] (6 axes)
    /// 
    /// Total: 13 rotation axes (excluding identity)
    pub fn analyze_cubic_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        let mut symmetries = Vec::new();
        
        // 4-fold rotations along crystallographic axes
        // These are the basis vectors themselves
        symmetries.push(RotationalSymmetry::new(unit_cell.a, 4));
        symmetries.push(RotationalSymmetry::new(unit_cell.b, 4));
        symmetries.push(RotationalSymmetry::new(unit_cell.c, 4));
        
        // 3-fold rotations along body diagonals
        // Body diagonals connect opposite corners of the unit cell
        let body_diagonal_1 = unit_cell.a + unit_cell.b + unit_cell.c;  // [111]
        let body_diagonal_2 = -unit_cell.a + unit_cell.b + unit_cell.c; // [1̄11]
        let body_diagonal_3 = unit_cell.a - unit_cell.b + unit_cell.c;  // [11̄1]
        let body_diagonal_4 = unit_cell.a + unit_cell.b - unit_cell.c;  // [111̄]
        
        symmetries.push(RotationalSymmetry::new(body_diagonal_1, 3));
        symmetries.push(RotationalSymmetry::new(body_diagonal_2, 3));
        symmetries.push(RotationalSymmetry::new(body_diagonal_3, 3));
        symmetries.push(RotationalSymmetry::new(body_diagonal_4, 3));
        
        // 2-fold rotations along face diagonals
        // Face diagonals are in the faces of the unit cell
        let face_diagonal_ab_1 = unit_cell.a + unit_cell.b;  // [110]
        let face_diagonal_ab_2 = unit_cell.a - unit_cell.b;  // [11̄0]
        let face_diagonal_ac_1 = unit_cell.a + unit_cell.c;  // [101]
        let face_diagonal_ac_2 = unit_cell.a - unit_cell.c;  // [101̄]
        let face_diagonal_bc_1 = unit_cell.b + unit_cell.c;  // [011]
        let face_diagonal_bc_2 = unit_cell.b - unit_cell.c;  // [011̄]
        
        symmetries.push(RotationalSymmetry::new(face_diagonal_ab_1, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_ab_2, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_ac_1, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_ac_2, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_bc_1, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_bc_2, 2));
        
        symmetries
    }
    
    /// Analyzes rotational symmetries for tetragonal crystal system
    /// 
    /// Tetragonal system has:
    /// - 4-fold rotation along c-axis
    /// - 2-fold rotations along a and b axes
    /// - 2-fold rotations along face diagonals in ab-plane
    pub fn analyze_tetragonal_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        let mut symmetries = Vec::new();
        
        // 4-fold rotation along c-axis (unique axis)
        symmetries.push(RotationalSymmetry::new(unit_cell.c, 4));
        
        // 2-fold rotations along a and b axes
        symmetries.push(RotationalSymmetry::new(unit_cell.a, 2));
        symmetries.push(RotationalSymmetry::new(unit_cell.b, 2));
        
        // 2-fold rotations along face diagonals in ab-plane
        let face_diagonal_1 = unit_cell.a + unit_cell.b;  // [110]
        let face_diagonal_2 = unit_cell.a - unit_cell.b;  // [11̄0]
        
        symmetries.push(RotationalSymmetry::new(face_diagonal_1, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_2, 2));
        
        symmetries
    }
    
    /// Analyzes rotational symmetries for orthorhombic crystal system
    /// 
    /// Orthorhombic system has:
    /// - 2-fold rotations along a, b, c axes
    pub fn analyze_orthorhombic_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        let mut symmetries = Vec::new();
        
        // 2-fold rotations along all three crystallographic axes
        symmetries.push(RotationalSymmetry::new(unit_cell.a, 2));
        symmetries.push(RotationalSymmetry::new(unit_cell.b, 2));
        symmetries.push(RotationalSymmetry::new(unit_cell.c, 2));
        
        symmetries
    }
    
    /// Analyzes rotational symmetries for hexagonal crystal system
    /// 
    /// Hexagonal system has:
    /// - 6-fold rotation along c-axis
    /// - 2-fold rotations perpendicular to c-axis (along a, b, and face diagonals)
    pub fn analyze_hexagonal_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        let mut symmetries = Vec::new();
        
        // 6-fold rotation along c-axis (unique axis)
        symmetries.push(RotationalSymmetry::new(unit_cell.c, 6));
        
        // 2-fold rotations perpendicular to c-axis
        // Along a and b axes
        symmetries.push(RotationalSymmetry::new(unit_cell.a, 2));
        symmetries.push(RotationalSymmetry::new(unit_cell.b, 2));
        
        // Along face diagonals in ab-plane
        let face_diagonal_1 = unit_cell.a + unit_cell.b;     // [110]
        let face_diagonal_2 = unit_cell.a - unit_cell.b;     // [11̄0]
        let face_diagonal_3 = 2.0 * unit_cell.a + unit_cell.b; // [210]
        let face_diagonal_4 = unit_cell.a + 2.0 * unit_cell.b; // [120]
        
        symmetries.push(RotationalSymmetry::new(face_diagonal_1, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_2, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_3, 2));
        symmetries.push(RotationalSymmetry::new(face_diagonal_4, 2));
        
        symmetries
    }
    
    /// Analyzes rotational symmetries for trigonal crystal system
    /// 
    /// Trigonal system has:
    /// - 3-fold rotation along the body diagonal (principal axis)
    pub fn analyze_trigonal_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        let mut symmetries = Vec::new();
        
        // 3-fold rotation along body diagonal [111]
        // This is the principal axis for trigonal/rhombohedral systems
        let body_diagonal = unit_cell.a + unit_cell.b + unit_cell.c;
        symmetries.push(RotationalSymmetry::new(body_diagonal, 3));
        
        symmetries
    }
    
    /// Analyzes rotational symmetries for monoclinic crystal system
    /// 
    /// Monoclinic system has:
    /// - 2-fold rotation along b-axis (conventional choice)
    pub fn analyze_monoclinic_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        let mut symmetries = Vec::new();
        
        // 2-fold rotation along b-axis (conventional unique axis for monoclinic)
        symmetries.push(RotationalSymmetry::new(unit_cell.b, 2));
        
        symmetries
    }
    
    /// Analyzes rotational symmetries for triclinic crystal system
    /// 
    /// Triclinic system has no rotational symmetries (only identity)
    pub fn analyze_triclinic_symmetries(_unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        // Triclinic has no rotational symmetries
        Vec::new()
    }
}

/// Main function to analyze all rotational symmetries of a unit cell
/// 
/// This function:
/// 1. Classifies the crystal system based on unit cell parameters
/// 2. Applies the appropriate symmetry analysis for that crystal system
/// 3. Returns all rotational symmetry elements
/// 
/// # Arguments
/// * `unit_cell` - The unit cell structure containing both basis vectors and crystallographic parameters
/// 
/// # Returns
/// * A vector of all rotational symmetry elements for the unit cell
/// * Empty vector for triclinic system (no rotational symmetries)
/// 
/// # Example
/// ```rust
/// use rust_lib_flutter_cad::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;
/// use rust_lib_flutter_cad::structure_designer::evaluator::unit_cell_symmetries::analyze_unit_cell_symmetries;
/// 
/// let cubic_unit_cell = UnitCellStruct::cubic_diamond();
/// let symmetries = analyze_unit_cell_symmetries(&cubic_unit_cell);
/// // Returns 13 rotational symmetry elements for cubic system
/// ```
pub fn analyze_unit_cell_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
    use symmetry_analysis::*;
    
    let crystal_system = classify_crystal_system(unit_cell);
    
    match crystal_system {
        CrystalSystem::Cubic => analyze_cubic_symmetries(unit_cell),
        CrystalSystem::Tetragonal => analyze_tetragonal_symmetries(unit_cell),
        CrystalSystem::Orthorhombic => analyze_orthorhombic_symmetries(unit_cell),
        CrystalSystem::Hexagonal => analyze_hexagonal_symmetries(unit_cell),
        CrystalSystem::Trigonal => analyze_trigonal_symmetries(unit_cell),
        CrystalSystem::Monoclinic => analyze_monoclinic_symmetries(unit_cell),
        CrystalSystem::Triclinic => analyze_triclinic_symmetries(unit_cell),
    }
}

/// Convenience function to get both crystal system and symmetries
/// 
/// # Arguments
/// * `unit_cell` - The unit cell structure
/// 
/// # Returns
/// * A tuple containing (crystal_system, symmetries)
pub fn analyze_unit_cell_complete(unit_cell: &UnitCellStruct) -> (CrystalSystem, Vec<RotationalSymmetry>) {
    let crystal_system = classify_crystal_system(unit_cell);
    let symmetries = analyze_unit_cell_symmetries(unit_cell);
    (crystal_system, symmetries)
}