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
}