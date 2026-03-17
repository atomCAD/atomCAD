use crate::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use glam::f64::DVec2;
use glam::f64::DVec3;
use glam::i32::IVec2;
use glam::i32::IVec3;

#[derive(Debug, Clone)]
pub struct UnitCellStruct {
    pub a: DVec3,
    pub b: DVec3,
    pub c: DVec3,
    // Crystallographic parameters using the same naming as UnitCellData
    pub cell_length_a: f64,
    pub cell_length_b: f64,
    pub cell_length_c: f64,
    pub cell_angle_alpha: f64, // in degrees
    pub cell_angle_beta: f64,  // in degrees
    pub cell_angle_gamma: f64, // in degrees
}

/// Properties of a crystal plane defined by Miller indices
#[derive(Debug, Clone)]
pub struct CrystalPlaneProps {
    /// Normalized normal vector of the crystal plane in real space coordinates
    pub normal: DVec3,
    /// d-spacing (interplanar spacing) for this Miller index in real space units
    pub d_spacing: f64,
}

/// Properties of a 2D crystal line defined by Miller indices
#[derive(Debug, Clone)]
pub struct CrystalPlaneProps2D {
    /// Normalized normal vector of the crystal line in real 2D space coordinates
    pub normal: DVec2,
    /// d-spacing (inter-line spacing) for this Miller index in real space units
    pub d_spacing: f64,
}

impl UnitCellStruct {
    /// Creates a UnitCellStruct from three basis vectors.
    ///
    /// Calculates the crystallographic parameters (lengths and angles) from the basis vectors:
    /// - cell_length_a, cell_length_b, cell_length_c: lengths of the basis vectors
    /// - cell_angle_alpha: angle between b and c (in degrees)
    /// - cell_angle_beta: angle between a and c (in degrees)
    /// - cell_angle_gamma: angle between a and b (in degrees)
    ///
    /// # Arguments
    /// * `a` - First basis vector
    /// * `b` - Second basis vector
    /// * `c` - Third basis vector
    ///
    /// # Returns
    /// * A UnitCellStruct with both basis vectors and calculated crystallographic parameters
    pub fn new(a: DVec3, b: DVec3, c: DVec3) -> Self {
        // Calculate lengths
        let cell_length_a = a.length();
        let cell_length_b = b.length();
        let cell_length_c = c.length();

        // Calculate angles between vectors using dot product: cos(θ) = (u·v)/(|u||v|)
        // Alpha is angle between b and c
        let cos_alpha = b.dot(c) / (cell_length_b * cell_length_c);
        let cell_angle_alpha = cos_alpha.acos().to_degrees();

        // Beta is angle between a and c
        let cos_beta = a.dot(c) / (cell_length_a * cell_length_c);
        let cell_angle_beta = cos_beta.acos().to_degrees();

        // Gamma is angle between a and b
        let cos_gamma = a.dot(b) / (cell_length_a * cell_length_b);
        let cell_angle_gamma = cos_gamma.acos().to_degrees();

        UnitCellStruct {
            a,
            b,
            c,
            cell_length_a,
            cell_length_b,
            cell_length_c,
            cell_angle_alpha,
            cell_angle_beta,
            cell_angle_gamma,
        }
    }

    /// Creates a cubic diamond unit cell using the standard diamond lattice parameter
    ///
    /// Returns a UnitCellStruct with orthogonal basis vectors aligned with the coordinate axes,
    /// each with length equal to the diamond unit cell size (3.567 Ångströms).
    pub fn cubic_diamond() -> Self {
        let size = DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
        UnitCellStruct {
            a: DVec3::new(size, 0.0, 0.0),
            b: DVec3::new(0.0, size, 0.0),
            c: DVec3::new(0.0, 0.0, size),
            cell_length_a: size,
            cell_length_b: size,
            cell_length_c: size,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        }
    }

    /// Compares two unit cells with tolerance for small calculation errors.
    ///
    /// This method checks if the basis vectors of two unit cells are approximately equal
    /// within a small epsilon tolerance (1e-5). This is useful for CSG operations where
    /// geometries should have compatible unit cells but may have tiny numerical differences
    /// due to floating-point calculations.
    ///
    /// # Arguments
    /// * `other` - The other UnitCellStruct to compare with
    ///
    /// # Returns
    /// * `true` if the unit cells are approximately equal within tolerance
    /// * `false` if they differ significantly
    pub fn is_approximately_equal(&self, other: &UnitCellStruct) -> bool {
        const EPSILON: f64 = 1e-5;

        // Compare basis vectors by checking the length of their differences
        (self.a - other.a).length() < EPSILON
            && (self.b - other.b).length() < EPSILON
            && (self.c - other.c).length() < EPSILON
    }

    /// Determines whether the unit cell is approximately cubic within a small tolerance.
    ///
    /// A unit cell is considered approximately cubic if:
    /// 1. All three basis vectors have approximately equal lengths
    /// 2. All three basis vectors are approximately orthogonal to each other
    ///
    /// This method uses the same epsilon tolerance (1e-5) as `is_approximately_equal`
    /// for consistency in numerical comparisons.
    ///
    /// # Returns
    /// * `true` if the unit cell is approximately cubic within tolerance
    /// * `false` if it deviates significantly from cubic symmetry
    pub fn is_approximately_cubic(&self) -> bool {
        const EPSILON: f64 = 1e-5;

        // Get the lengths of the three basis vectors
        let len_a = self.a.length();
        let len_b = self.b.length();
        let len_c = self.c.length();

        // Check if all lengths are approximately equal
        let lengths_equal = (len_a - len_b).abs() < EPSILON
            && (len_b - len_c).abs() < EPSILON
            && (len_a - len_c).abs() < EPSILON;

        if !lengths_equal {
            return false;
        }

        // Check if all basis vectors are approximately orthogonal
        // For orthogonal vectors, their dot products should be approximately zero
        let dot_ab = self.a.dot(self.b).abs();
        let dot_bc = self.b.dot(self.c).abs();
        let dot_ac = self.a.dot(self.c).abs();

        // Use a scaled epsilon based on the square of the vector lengths
        // since dot products scale with the square of the magnitudes
        let scaled_epsilon = EPSILON * len_a * len_b;

        dot_ab < scaled_epsilon && dot_bc < scaled_epsilon && dot_ac < scaled_epsilon
    }

    /// Converts lattice coordinates to real space coordinates using the unit cell basis vectors.
    ///
    /// # Arguments
    /// * `lattice_pos` - Position in lattice coordinates as DVec3
    ///
    /// # Returns
    /// Position in real space coordinates as DVec3
    pub fn dvec3_lattice_to_real(&self, lattice_pos: &DVec3) -> DVec3 {
        lattice_pos.x * self.a + lattice_pos.y * self.b + lattice_pos.z * self.c
    }

    /// Converts lattice coordinates to real space coordinates using the unit cell basis vectors.
    ///
    /// # Arguments
    /// * `lattice_pos` - Position in lattice coordinates as IVec3
    ///
    /// # Returns
    /// Position in real space coordinates as DVec3
    pub fn ivec3_lattice_to_real(&self, lattice_pos: &IVec3) -> DVec3 {
        self.dvec3_lattice_to_real(&lattice_pos.as_dvec3())
    }

    pub fn dvec2_lattice_to_real(&self, lattice_pos: &DVec2) -> DVec2 {
        let real = lattice_pos.x * self.a + lattice_pos.y * self.b;
        DVec2::new(real.x, real.y)
    }

    pub fn ivec2_lattice_to_real(&self, lattice_pos: &IVec2) -> DVec2 {
        self.dvec2_lattice_to_real(&lattice_pos.as_dvec2())
    }

    pub fn float_lattice_to_real(&self, lattice_value: f64) -> f64 {
        lattice_value * self.a.length()
    }

    pub fn int_lattice_to_real(&self, lattice_value: i32) -> f64 {
        self.float_lattice_to_real(lattice_value as f64)
    }

    /// Converts a position from real space coordinates to lattice space coordinates.
    ///
    /// This method performs the inverse transformation of `dvec3_lattice_to_real`.
    /// Given a position in real space, it finds the corresponding lattice coordinates
    /// (u, v, w) such that: real_pos = u*a + v*b + w*c
    ///
    /// The conversion is performed by solving the linear system using the inverse
    /// of the unit cell matrix [a, b, c].
    ///
    /// # Arguments
    /// * `real_pos` - Position in real space coordinates as DVec3
    ///
    /// # Returns
    /// * Position in lattice space coordinates as DVec3
    ///
    /// # Panics
    /// * Panics if the unit cell matrix is singular (determinant is zero)
    pub fn real_to_dvec3_lattice(&self, real_pos: &DVec3) -> DVec3 {
        // Calculate the determinant of the unit cell matrix [a, b, c]
        let det = self.a.dot(self.b.cross(self.c));

        // Check for singular matrix (degenerate unit cell)
        if det.abs() < 1e-12 {
            panic!(
                "Unit cell matrix is singular - cannot convert from real to lattice coordinates"
            );
        }

        // Calculate the inverse matrix using Cramer's rule
        // For matrix [a, b, c], the inverse is (1/det) * [b×c, c×a, a×b]^T
        let inv_det = 1.0 / det;

        // Calculate the columns of the inverse matrix
        let inv_a = self.b.cross(self.c) * inv_det; // First row of inverse
        let inv_b = self.c.cross(self.a) * inv_det; // Second row of inverse  
        let inv_c = self.a.cross(self.b) * inv_det; // Third row of inverse

        // Apply the inverse transformation
        DVec3::new(
            inv_a.dot(*real_pos), // u coordinate
            inv_b.dot(*real_pos), // v coordinate
            inv_c.dot(*real_pos), // w coordinate
        )
    }

    /// Converts a position from real space coordinates to integer lattice space coordinates.
    ///
    /// This method is a convenience wrapper around `real_to_dvec3_lattice` that rounds
    /// the resulting floating-point lattice coordinates to the nearest integers.
    /// This is useful when you need lattice coordinates as discrete grid positions.
    ///
    /// # Arguments
    /// * `real_pos` - Position in real space coordinates as DVec3
    ///
    /// # Returns
    /// * Position in integer lattice space coordinates as IVec3
    ///
    /// # Panics
    /// * Panics if the unit cell matrix is singular (determinant is zero)
    pub fn real_to_ivec3_lattice(&self, real_pos: &DVec3) -> IVec3 {
        let lattice_pos = self.real_to_dvec3_lattice(real_pos);
        IVec3::new(
            lattice_pos.x.round() as i32,
            lattice_pos.y.round() as i32,
            lattice_pos.z.round() as i32,
        )
    }

    /// Converts Miller indices to crystal plane properties (normal and d-spacing).
    ///
    /// For a crystal plane with Miller indices (h, k, l), both the normal vector and
    /// d-spacing are calculated using the reciprocal lattice. The normal is given by:
    /// n = h*a* + k*b* + l*c*
    ///
    /// Where a*, b*, c* are the reciprocal lattice basis vectors:
    /// a* = (b × c) / (a · (b × c))
    /// b* = (c × a) / (a · (b × c))  
    /// c* = (a × b) / (a · (b × c))
    ///
    /// This can be computed directly as:
    /// n = (h(b × c) + k(c × a) + l(a × b)) / V
    /// where V = a · (b × c) is the unit cell volume.
    ///
    /// The d-spacing is calculated as d = 1 / |G_hkl| where G_hkl is the
    /// reciprocal lattice vector magnitude.
    ///
    /// # Arguments
    /// * `miller_indices` - Miller indices (h, k, l) as DVec3
    ///
    /// # Returns
    /// * Ok(CrystalPlaneProps) containing normalized normal vector and d-spacing
    /// * Err(String) with error message if the computation fails
    ///
    /// # Errors
    /// * Returns error if miller_indices is (0,0,0) - invalid Miller index
    /// * Returns error if the unit cell volume is zero (degenerate unit cell)
    pub fn dvec3_miller_index_to_plane_props(
        &self,
        miller_indices: &DVec3,
    ) -> Result<CrystalPlaneProps, String> {
        let h = miller_indices.x;
        let k = miller_indices.y;
        let l = miller_indices.z;

        // Check for invalid (0,0,0) Miller index
        if h.abs() < 1e-12 && k.abs() < 1e-12 && l.abs() < 1e-12 {
            return Err("Miller index (0,0,0) is invalid - cannot define a plane".to_string());
        }

        // Calculate cross products for reciprocal lattice basis vectors
        let b_cross_c = self.b.cross(self.c);
        let c_cross_a = self.c.cross(self.a);
        let a_cross_b = self.a.cross(self.b);

        // Calculate unit cell volume
        let volume = self.a.dot(b_cross_c);

        // Check for degenerate unit cell
        if volume.abs() < 1e-12 {
            return Err(
                "Unit cell has zero volume - cannot compute Miller index properties".to_string(),
            );
        }

        // Calculate the reciprocal lattice vector (unnormalized)
        let reciprocal_vector = (h * b_cross_c + k * c_cross_a + l * a_cross_b) / volume;

        // Calculate d-spacing as inverse of reciprocal vector magnitude
        let d_spacing = 1.0 / reciprocal_vector.length();

        // Calculate normalized normal vector
        let normal = reciprocal_vector.normalize();

        Ok(CrystalPlaneProps { normal, d_spacing })
    }

    /// Converts Miller indices to crystal plane properties (normal and d-spacing).
    ///
    /// This is a convenience wrapper around `dvec3_miller_index_to_plane_props` that accepts
    /// integer Miller indices and converts them to floating-point before computation.
    ///
    /// # Arguments
    /// * `miller_indices` - Miller indices (h, k, l) as IVec3
    ///
    /// # Returns
    /// * Ok(CrystalPlaneProps) containing normalized normal vector and d-spacing
    /// * Err(String) with error message if the computation fails
    ///
    /// # Errors
    /// * Returns error if miller_indices is (0,0,0) - invalid Miller index
    /// * Returns error if the unit cell volume is zero (degenerate unit cell)
    pub fn ivec3_miller_index_to_plane_props(
        &self,
        miller_indices: &IVec3,
    ) -> Result<CrystalPlaneProps, String> {
        self.dvec3_miller_index_to_plane_props(&miller_indices.as_dvec3())
    }

    /// Converts Miller indices to the normal vector of the corresponding crystal plane.
    ///
    /// This is a convenience wrapper that extracts only the normal from the plane properties.
    ///
    /// # Arguments
    /// * `miller_indices` - Miller indices (h, k, l) as DVec3
    ///
    /// # Returns
    /// * Ok(DVec3) - Normalized normal vector of the crystal plane in real space coordinates
    /// * Err(String) with error message if the computation fails
    ///
    /// # Errors
    /// * Returns error if miller_indices is (0,0,0) - invalid Miller index
    /// * Returns error if the unit cell volume is zero (degenerate unit cell)
    pub fn dvec3_miller_index_to_normal(&self, miller_indices: &DVec3) -> Result<DVec3, String> {
        Ok(self
            .dvec3_miller_index_to_plane_props(miller_indices)?
            .normal)
    }

    /// Converts Miller indices to the normal vector of the corresponding crystal plane.
    ///
    /// This is a convenience wrapper that extracts only the normal from the plane properties.
    ///
    /// # Arguments
    /// * `miller_indices` - Miller indices (h, k, l) as IVec3
    ///
    /// # Returns
    /// * Ok(DVec3) - Normalized normal vector of the crystal plane in real space coordinates
    /// * Err(String) with error message if the computation fails
    ///
    /// # Errors
    /// * Returns error if miller_indices is (0,0,0) - invalid Miller index
    /// * Returns error if the unit cell volume is zero (degenerate unit cell)
    pub fn ivec3_miller_index_to_normal(&self, miller_indices: &IVec3) -> Result<DVec3, String> {
        Ok(self
            .ivec3_miller_index_to_plane_props(miller_indices)?
            .normal)
    }

    /// Converts 2D Miller indices to crystal line properties (normal and d-spacing).
    ///
    /// In 2D, Miller indices (h, k) define a family of parallel lines. This method computes:
    /// - The normal vector perpendicular to these lines
    /// - The d-spacing (distance between parallel lines)
    ///
    /// # Arguments
    /// * `miller_indices` - Miller indices (h, k) as DVec2
    ///
    /// # Returns
    /// * Ok(CrystalPlaneProps2D) containing normalized normal vector and d-spacing
    /// * Err(String) with error message if the computation fails
    ///
    /// # Errors
    /// * Returns error if miller_indices is (0,0) - invalid Miller index
    /// * Returns error if the unit cell area is zero (degenerate unit cell)
    pub fn dvec2_miller_index_to_plane_props(
        &self,
        miller_indices: &DVec2,
    ) -> Result<CrystalPlaneProps2D, String> {
        let h = miller_indices.x;
        let k = miller_indices.y;

        // Check for invalid (0,0) Miller index
        if h.abs() < 1e-12 && k.abs() < 1e-12 {
            return Err("Miller index (0,0) is invalid - cannot define a plane".to_string());
        }

        // Extract 2D components of basis vectors
        let a2d = DVec2::new(self.a.x, self.a.y);
        let b2d = DVec2::new(self.b.x, self.b.y);

        // Calculate 2D area (cross product magnitude in 2D is the z-component)
        let area = a2d.x * b2d.y - a2d.y * b2d.x;

        // Check for degenerate unit cell
        if area.abs() < 1e-12 {
            return Err(
                "Unit cell has zero area - cannot compute Miller index properties".to_string(),
            );
        }

        // For 2D, the reciprocal lattice vectors are:
        // a* = (b.y, -b.x) / area
        // b* = (-a.y, a.x) / area
        let a_star = DVec2::new(b2d.y, -b2d.x) / area;
        let b_star = DVec2::new(-a2d.y, a2d.x) / area;

        // Calculate the reciprocal lattice vector (unnormalized)
        let reciprocal_vector = h * a_star + k * b_star;

        // Calculate d-spacing as inverse of reciprocal vector magnitude
        let d_spacing = 1.0 / reciprocal_vector.length();

        // Calculate normalized normal vector
        let normal = reciprocal_vector.normalize();

        Ok(CrystalPlaneProps2D { normal, d_spacing })
    }

    /// Converts 2D Miller indices to crystal line properties (normal and d-spacing).
    ///
    /// This is a convenience wrapper around `dvec2_miller_index_to_plane_props` that accepts
    /// integer Miller indices and converts them to floating-point before computation.
    ///
    /// # Arguments
    /// * `miller_indices` - Miller indices (h, k) as IVec2
    ///
    /// # Returns
    /// * Ok(CrystalPlaneProps2D) containing normalized normal vector and d-spacing
    /// * Err(String) with error message if the computation fails
    ///
    /// # Errors
    /// * Returns error if miller_indices is (0,0) - invalid Miller index
    /// * Returns error if the unit cell area is zero (degenerate unit cell)
    pub fn ivec2_miller_index_to_plane_props(
        &self,
        miller_indices: &IVec2,
    ) -> Result<CrystalPlaneProps2D, String> {
        self.dvec2_miller_index_to_plane_props(&miller_indices.as_dvec2())
    }

    /// Returns a basis vector by its index.
    ///
    /// # Arguments
    /// * `index` - Index of the basis vector (0 = a, 1 = b, 2 = c)
    ///
    /// # Returns
    /// * The corresponding basis vector as DVec3
    ///
    /// # Panics
    /// * Panics if index is not 0, 1, or 2
    pub fn get_basis_vector(&self, index: i32) -> DVec3 {
        match index {
            0 => self.a,
            1 => self.b,
            2 => self.c,
            _ => panic!("Basis vector index must be 0, 1, or 2, got {}", index),
        }
    }
}
