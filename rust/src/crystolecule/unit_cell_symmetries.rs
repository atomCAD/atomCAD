use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use glam::f64::DVec3;

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
            2 | 3 | 4 | 6 => {}
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
    /// Two equal axes, one unique. The usize indicates which axis (0=a, 1=b, 2=c) is unique
    Tetragonal(usize),
    /// a≠b≠c, α=β=γ=90°
    Orthorhombic,
    /// Two equal axes, one unique with 120° angle. The usize indicates which axis (0=a, 1=b, 2=c) is unique
    Hexagonal(usize),
    /// a=b=c, α=β=γ≠90° (and equal)
    Trigonal,
    /// One unique axis with non-90° angle. The usize indicates which axis (0=a, 1=b, 2=c) is unique
    Monoclinic(usize),
    /// a≠b≠c, α≠β≠γ≠90°
    Triclinic,
}

impl CrystalSystem {
    /// Returns a human-readable name for the crystal system
    pub fn name(&self) -> &'static str {
        match self {
            CrystalSystem::Cubic => "Cubic",
            CrystalSystem::Tetragonal(_) => "Tetragonal",
            CrystalSystem::Orthorhombic => "Orthorhombic",
            CrystalSystem::Hexagonal(_) => "Hexagonal",
            CrystalSystem::Trigonal => "Trigonal",
            CrystalSystem::Monoclinic(_) => "Monoclinic",
            CrystalSystem::Triclinic => "Triclinic",
        }
    }
}

/// Helper functions for crystal system classification
mod classification {
    use super::tolerance::{ANGLE_EPSILON, LENGTH_EPSILON};

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

use classification::{angle_equals, is_120_degrees, is_right_angle, lengths_equal};

/// All permutations of (0,1,2) for axis permutation testing
const ALL_PERMUTATIONS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

// --------------------------------
// Permutation-aware classifier for the seven crystal systems.
//
// Optimization: some properties are invariant under relabeling of axes
// (e.g. "how many angles are ≈90°" or "are all three angles ≈90°"). We check
// those properties once (no permutations needed). When a test requires the
// association of particular angles with particular edges (for example the
// conventional hexagonal pattern where α≈β≈90°, γ≈120° and a≈b) we test
// permutations.
pub fn classify_crystal_system(unit_cell: &UnitCellStruct) -> CrystalSystem {
    // lengths and angles
    let a = unit_cell.cell_length_a;
    let b = unit_cell.cell_length_b;
    let c = unit_cell.cell_length_c;
    let alpha = unit_cell.cell_angle_alpha;
    let beta = unit_cell.cell_angle_beta;
    let gamma = unit_cell.cell_angle_gamma;

    let lengths = [a, b, c];
    let angles = [alpha, beta, gamma];

    // --- INVARIANT QUICK CHECKS (no permutations needed) ---
    // Count how many angles are ≈90° (this is invariant under relabeling)
    let mut global_right_count = 0u8;
    if is_right_angle(alpha) {
        global_right_count += 1;
    }
    if is_right_angle(beta) {
        global_right_count += 1;
    }
    if is_right_angle(gamma) {
        global_right_count += 1;
    }

    // If all three angles are right we can classify cubic/tetragonal/orthorhombic
    // without permuting, because those tests depend only on length-equality
    // relationships that can be checked in an order-insensitive way.
    if global_right_count == 3 {
        // cubic: a ≈ b ≈ c
        if lengths_equal(a, b) && lengths_equal(b, c) {
            return CrystalSystem::Cubic;
        }

        // tetragonal: exactly two lengths equal, third different
        let ab = lengths_equal(a, b);
        let ac = lengths_equal(a, c);
        let bc = lengths_equal(b, c);
        if ab && !ac && !bc {
            return CrystalSystem::Tetragonal(2); // c is unique
        } else if ac && !ab && !bc {
            return CrystalSystem::Tetragonal(1); // b is unique
        } else if bc && !ab && !ac {
            return CrystalSystem::Tetragonal(0); // a is unique
        }

        // otherwise, all angles 90° but no equalities for higher symmetry -> orthorhombic
        return CrystalSystem::Orthorhombic;
    }

    // Trigonal (rhombohedral): a≈b≈c and α≈β≈γ≠90° (completely symmetric, no permutations needed)
    if lengths_equal(a, b)
        && lengths_equal(b, c)
        && angle_equals(alpha, beta)
        && angle_equals(beta, gamma)
        && !is_right_angle(alpha)
    {
        return CrystalSystem::Trigonal;
    }

    // --- PERMUTATION-NEEDED CHECKS ---
    // Hexagonal: exists permutation where alpha'≈beta'≈90°, gamma'≈120°, and a'≈b'
    // The unique axis is the one corresponding to the 120° angle
    for &p in &ALL_PERMUTATIONS {
        let la = lengths[p[0]];
        let lb = lengths[p[1]];
        let a_alpha = angles[p[0]];
        let a_beta = angles[p[1]];
        let a_gamma = angles[p[2]];

        if is_right_angle(a_alpha)
            && is_right_angle(a_beta)
            && is_120_degrees(a_gamma)
            && lengths_equal(la, lb)
        {
            return CrystalSystem::Hexagonal(p[2]); // The axis corresponding to the 120° angle is unique
        }
    }

    // Monoclinic: exactly two right angles, identify which axis is unique (non-90°)
    // We must ensure hexagonal (which also has two right angles) was already tested above
    if global_right_count == 2 {
        if !is_right_angle(alpha) {
            return CrystalSystem::Monoclinic(0); // a-axis is unique (α ≠ 90°)
        } else if !is_right_angle(beta) {
            return CrystalSystem::Monoclinic(1); // b-axis is unique (β ≠ 90°)
        } else if !is_right_angle(gamma) {
            return CrystalSystem::Monoclinic(2); // c-axis is unique (γ ≠ 90°)
        }
        // This shouldn't happen if global_right_count == 2, but fallback to conventional b-axis
        return CrystalSystem::Monoclinic(1);
    }

    // Otherwise triclinic (no special metric relations detected)
    CrystalSystem::Triclinic
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
        // 3-fold rotations along body diagonals
        // Body diagonals connect opposite corners of the unit cell
        let body_diagonal_1 = unit_cell.a + unit_cell.b + unit_cell.c; // [111]
        let body_diagonal_2 = -unit_cell.a + unit_cell.b + unit_cell.c; // [1̄11]
        let body_diagonal_3 = unit_cell.a - unit_cell.b + unit_cell.c; // [11̄1]
        let body_diagonal_4 = unit_cell.a + unit_cell.b - unit_cell.c; // [111̄]

        // 2-fold rotations along face diagonals
        // Face diagonals are in the faces of the unit cell
        let face_diagonal_ab_1 = unit_cell.a + unit_cell.b; // [110]
        let face_diagonal_ab_2 = unit_cell.a - unit_cell.b; // [11̄0]
        let face_diagonal_ac_1 = unit_cell.a + unit_cell.c; // [101]
        let face_diagonal_ac_2 = unit_cell.a - unit_cell.c; // [101̄]
        let face_diagonal_bc_1 = unit_cell.b + unit_cell.c; // [011]
        let face_diagonal_bc_2 = unit_cell.b - unit_cell.c; // [011̄]

        vec![
            // 4-fold rotations along crystallographic axes
            // These are the basis vectors themselves
            RotationalSymmetry::new(unit_cell.a, 4),
            RotationalSymmetry::new(unit_cell.b, 4),
            RotationalSymmetry::new(unit_cell.c, 4),
            // 3-fold rotations along body diagonals
            RotationalSymmetry::new(body_diagonal_1, 3),
            RotationalSymmetry::new(body_diagonal_2, 3),
            RotationalSymmetry::new(body_diagonal_3, 3),
            RotationalSymmetry::new(body_diagonal_4, 3),
            // 2-fold rotations along face diagonals
            RotationalSymmetry::new(face_diagonal_ab_1, 2),
            RotationalSymmetry::new(face_diagonal_ab_2, 2),
            RotationalSymmetry::new(face_diagonal_ac_1, 2),
            RotationalSymmetry::new(face_diagonal_ac_2, 2),
            RotationalSymmetry::new(face_diagonal_bc_1, 2),
            RotationalSymmetry::new(face_diagonal_bc_2, 2),
        ]
    }

    /// Helper function to build tetragonal symmetries given the unique and equal axes
    fn build_tetragonal_symmetries(
        unique_axis: DVec3,
        equal_axis1: DVec3,
        equal_axis2: DVec3,
    ) -> Vec<RotationalSymmetry> {
        vec![
            // 4-fold rotation along unique axis
            RotationalSymmetry::new(unique_axis, 4),
            // 2-fold rotations along equal axes
            RotationalSymmetry::new(equal_axis1, 2),
            RotationalSymmetry::new(equal_axis2, 2),
            // 2-fold rotations along face diagonals in the plane of equal axes
            RotationalSymmetry::new(equal_axis1 + equal_axis2, 2),
            RotationalSymmetry::new(equal_axis1 - equal_axis2, 2),
        ]
    }

    /// Analyzes rotational symmetries for tetragonal crystal system
    ///
    /// Tetragonal system has:
    /// - 4-fold rotation along unique axis (the one with different length)
    /// - 2-fold rotations along the two equal axes
    /// - 2-fold rotations along face diagonals in the plane of equal axes
    ///
    /// # Arguments
    /// * `unit_cell` - The unit cell structure
    /// * `unique_axis_index` - Index of the unique axis (0=a, 1=b, 2=c)
    pub fn analyze_tetragonal_symmetries(
        unit_cell: &UnitCellStruct,
        unique_axis_index: usize,
    ) -> Vec<RotationalSymmetry> {
        let axes = [unit_cell.a, unit_cell.b, unit_cell.c];

        // Get the unique axis and the two equal axes
        let unique_axis = axes[unique_axis_index];
        let equal_axes: Vec<DVec3> = axes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != unique_axis_index)
            .map(|(_, &axis)| axis)
            .collect();

        build_tetragonal_symmetries(unique_axis, equal_axes[0], equal_axes[1])
    }

    /// Analyzes rotational symmetries for orthorhombic crystal system
    ///
    /// Orthorhombic system has:
    /// - 2-fold rotations along a, b, c axes
    pub fn analyze_orthorhombic_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        // 2-fold rotations along all three crystallographic axes
        vec![
            RotationalSymmetry::new(unit_cell.a, 2),
            RotationalSymmetry::new(unit_cell.b, 2),
            RotationalSymmetry::new(unit_cell.c, 2),
        ]
    }

    /// Helper function to build hexagonal symmetries given the unique and equal axes
    fn build_hexagonal_symmetries(
        unique_axis: DVec3,
        equal_axis1: DVec3,
        equal_axis2: DVec3,
    ) -> Vec<RotationalSymmetry> {
        vec![
            // 6-fold rotation along unique axis (c-axis)
            RotationalSymmetry::new(unique_axis, 6),
            // 2-fold rotations perpendicular to unique axis
            // Along the two equal axes
            RotationalSymmetry::new(equal_axis1, 2),
            RotationalSymmetry::new(equal_axis2, 2),
            // Along face diagonals in the plane of equal axes
            RotationalSymmetry::new(equal_axis1 + equal_axis2, 2), // [110]
            RotationalSymmetry::new(equal_axis1 - equal_axis2, 2), // [11̄0]
            RotationalSymmetry::new(2.0 * equal_axis1 + equal_axis2, 2), // [210]
            RotationalSymmetry::new(equal_axis1 + 2.0 * equal_axis2, 2), // [120]
        ]
    }

    /// Analyzes rotational symmetries for hexagonal crystal system
    ///
    /// Hexagonal system has:
    /// - 6-fold rotation along unique axis (the one with 120° angle)
    /// - 2-fold rotations perpendicular to unique axis (along equal axes and face diagonals)
    ///
    /// # Arguments
    /// * `unit_cell` - The unit cell structure
    /// * `unique_axis_index` - Index of the unique axis (0=a, 1=b, 2=c)
    pub fn analyze_hexagonal_symmetries(
        unit_cell: &UnitCellStruct,
        unique_axis_index: usize,
    ) -> Vec<RotationalSymmetry> {
        let axes = [unit_cell.a, unit_cell.b, unit_cell.c];

        // Get the unique axis and the two equal axes
        let unique_axis = axes[unique_axis_index];
        let equal_axes: Vec<DVec3> = axes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != unique_axis_index)
            .map(|(_, &axis)| axis)
            .collect();

        build_hexagonal_symmetries(unique_axis, equal_axes[0], equal_axes[1])
    }

    /// Analyzes rotational symmetries for trigonal crystal system
    ///
    /// Trigonal (rhombohedral) system has:
    /// - ONE 3-fold rotation along the main body diagonal [111]
    ///
    /// While a=b=c and α=β=γ, the non-90° angles break the cubic symmetry.
    /// Only the main body diagonal [111] retains 3-fold rotational symmetry.
    /// The other body diagonals lose their 3-fold symmetry due to the rhombohedral distortion.
    pub fn analyze_trigonal_symmetries(unit_cell: &UnitCellStruct) -> Vec<RotationalSymmetry> {
        // Only ONE 3-fold rotation along the main body diagonal
        // In a rhombohedral system, the non-90° angles break the equivalence of all body diagonals
        // Only [111] remains a true 3-fold symmetry axis
        let main_body_diagonal = unit_cell.a + unit_cell.b + unit_cell.c; // [111]

        vec![RotationalSymmetry::new(main_body_diagonal, 3)]
    }

    /// Analyzes rotational symmetries for monoclinic crystal system
    ///
    /// Monoclinic system has:
    /// - 2-fold rotation along the unique axis (the one with non-90° angle)
    ///
    /// # Arguments
    /// * `unit_cell` - The unit cell structure
    /// * `unique_axis_index` - Index of the unique axis (0=a, 1=b, 2=c)
    pub fn analyze_monoclinic_symmetries(
        unit_cell: &UnitCellStruct,
        unique_axis_index: usize,
    ) -> Vec<RotationalSymmetry> {
        let axes = [unit_cell.a, unit_cell.b, unit_cell.c];
        let unique_axis = axes[unique_axis_index];

        // 2-fold rotation along the unique axis (the one with non-90° angle)
        vec![RotationalSymmetry::new(unique_axis, 2)]
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
/// use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
/// use rust_lib_flutter_cad::crystolecule::unit_cell_symmetries::analyze_unit_cell_symmetries;
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
        CrystalSystem::Tetragonal(unique_axis_index) => {
            analyze_tetragonal_symmetries(unit_cell, unique_axis_index)
        }
        CrystalSystem::Orthorhombic => analyze_orthorhombic_symmetries(unit_cell),
        CrystalSystem::Hexagonal(unique_axis_index) => {
            analyze_hexagonal_symmetries(unit_cell, unique_axis_index)
        }
        CrystalSystem::Trigonal => analyze_trigonal_symmetries(unit_cell),
        CrystalSystem::Monoclinic(unique_axis_index) => {
            analyze_monoclinic_symmetries(unit_cell, unique_axis_index)
        }
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
pub fn analyze_unit_cell_complete(
    unit_cell: &UnitCellStruct,
) -> (CrystalSystem, Vec<RotationalSymmetry>) {
    let crystal_system = classify_crystal_system(unit_cell);
    let symmetries = analyze_unit_cell_symmetries(unit_cell);
    (crystal_system, symmetries)
}
