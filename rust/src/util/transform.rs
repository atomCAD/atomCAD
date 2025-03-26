use glam::f64::DVec3;
use glam::f64::DQuat;

#[derive(Clone)]
pub struct Transform {
  pub translation: DVec3,
  pub rotation: DQuat,
}

impl Transform {
  pub fn new(translation: DVec3, rotation: DQuat) -> Self {
    Self { translation, rotation }
  }

  /// Calculates the inverse of this transform
  pub fn inverse(&self) -> Transform {
    let inv_rotation = self.rotation.conjugate(); // For unit quaternions, conjugate is the same as inverse
    let inv_translation = -(inv_rotation.mul_vec3(self.translation));
    Transform {
      translation: inv_translation,
      rotation: inv_rotation,
    }
  }

  /// Calculates the delta transform from `from` to `self`.
  /// This represents the transformation needed to go from `from` to `self`.
  pub fn delta_from(&self, from: &Transform) -> Transform {
    // The delta transform can be calculated as: self * from^-1
    // First get the inverse of 'from'
    let from_inv = from.inverse();
    
    // Now calculate self * from_inv
    // For rotation: self.rotation * from_inv.rotation
    let delta_rotation = self.rotation * from_inv.rotation;
    
    // For translation: self.translation + self.rotation * from_inv.translation
    let delta_translation = self.translation + self.rotation.mul_vec3(from_inv.translation);
    
    Transform::new(delta_translation, delta_rotation)
  }
  
  /// Apply this transform to a position vector
  pub fn apply_to_position(&self, position: &DVec3) -> DVec3 {
    // Apply rotation to position and add translation
    self.rotation.mul_vec3(*position) + self.translation
  }
  
  /// Apply this transform to a position vector in-place
  pub fn apply_to_position_in_place(&self, position: &mut DVec3) {
    *position = self.rotation.mul_vec3(*position) + self.translation;
  }
}

impl Default for Transform {
  fn default() -> Self {
    Self { translation: DVec3::ZERO, rotation: DQuat::IDENTITY }
  }
}
