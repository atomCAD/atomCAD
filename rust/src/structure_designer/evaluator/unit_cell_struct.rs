use crate::structure_designer::common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use glam::f64::DVec3;
use glam::i32::IVec3;
use glam::f64::DVec2;
use glam::i32::IVec2;

#[derive(Debug, Clone)]
pub struct UnitCellStruct {
  pub a: DVec3,
  pub b: DVec3,
  pub c: DVec3,
}

/// Properties of a crystal plane defined by Miller indices
#[derive(Debug, Clone)]
pub struct CrystalPlaneProps {
  /// Normalized normal vector of the crystal plane in real space coordinates
  pub normal: DVec3,
  /// d-spacing (interplanar spacing) for this Miller index in real space units
  pub d_spacing: f64,
}

impl UnitCellStruct {
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
    (self.a - other.a).length() < EPSILON &&
    (self.b - other.b).length() < EPSILON &&
    (self.c - other.c).length() < EPSILON
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
    (lattice_pos.x * self.a + lattice_pos.y * self.b).truncate()
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
  /// * CrystalPlaneProps containing normalized normal vector and d-spacing
  /// 
  /// # Panics
  /// * Panics if the unit cell volume is zero (degenerate unit cell)
  pub fn dvec3_miller_index_to_plane_props(&self, miller_indices: &DVec3) -> CrystalPlaneProps {
    let h = miller_indices.x;
    let k = miller_indices.y;
    let l = miller_indices.z;
    
    // Calculate cross products for reciprocal lattice basis vectors
    let b_cross_c = self.b.cross(self.c);
    let c_cross_a = self.c.cross(self.a);
    let a_cross_b = self.a.cross(self.b);
    
    // Calculate unit cell volume
    let volume = self.a.dot(b_cross_c);
    
    // Check for degenerate unit cell
    if volume.abs() < 1e-12 {
      panic!("Unit cell has zero volume - cannot compute Miller index properties");
    }
    
    // Calculate the reciprocal lattice vector (unnormalized)
    let reciprocal_vector = (h * b_cross_c + k * c_cross_a + l * a_cross_b) / volume;
    
    // Calculate d-spacing as inverse of reciprocal vector magnitude
    let d_spacing = 1.0 / reciprocal_vector.length();
    
    // Calculate normalized normal vector
    let normal = reciprocal_vector.normalize();
    
    CrystalPlaneProps {
      normal,
      d_spacing,
    }
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
  /// * CrystalPlaneProps containing normalized normal vector and d-spacing
  /// 
  /// # Panics
  /// * Panics if the unit cell volume is zero (degenerate unit cell)
  pub fn ivec3_miller_index_to_plane_props(&self, miller_indices: &IVec3) -> CrystalPlaneProps {
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
  /// * Normalized normal vector of the crystal plane in real space coordinates
  /// 
  /// # Panics
  /// * Panics if the unit cell volume is zero (degenerate unit cell)
  pub fn dvec3_miller_index_to_normal(&self, miller_indices: &DVec3) -> DVec3 {
    self.dvec3_miller_index_to_plane_props(miller_indices).normal
  }

  /// Converts Miller indices to the normal vector of the corresponding crystal plane.
  /// 
  /// This is a convenience wrapper that extracts only the normal from the plane properties.
  /// 
  /// # Arguments
  /// * `miller_indices` - Miller indices (h, k, l) as IVec3
  /// 
  /// # Returns
  /// * Normalized normal vector of the crystal plane in real space coordinates
  /// 
  /// # Panics
  /// * Panics if the unit cell volume is zero (degenerate unit cell)
  pub fn ivec3_miller_index_to_normal(&self, miller_indices: &IVec3) -> DVec3 {
    self.ivec3_miller_index_to_plane_props(miller_indices).normal
  }
}