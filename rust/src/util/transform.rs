use glam::f64::DVec3;
use glam::f64::DQuat;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec3_serializer;
use crate::common::serialization_utils::dquat_serializer;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
  #[serde(with = "dvec3_serializer")]
  pub translation: DVec3,
  #[serde(with = "dquat_serializer")]
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
  
  /// Apply a relative transform to this transform
  /// 
  /// This applies the given relative transform to the current transform (self).
  /// After this operation, self will represent the combined transformation.
  /// 
  /// # Arguments
  /// * `rel_transform` - The relative transform to apply to this transform
  pub fn apply(&mut self, rel_transform: &Transform) {
    // For rotation: self.rotation = rel_transform.rotation * self.rotation
    self.rotation = rel_transform.rotation * self.rotation;
    
    // For translation: self.translation = rel_transform.translation + rel_transform.rotation * self.translation
    self.translation = rel_transform.translation + rel_transform.rotation.mul_vec3(self.translation);
  }
  
  /// Apply a relative transform to this transform and return a new Transform
  /// 
  /// This creates a new Transform that is the result of applying the given relative transform 
  /// to this transform. The original transform is not modified.
  /// 
  /// # Arguments
  /// * `rel_transform` - The relative transform to apply to this transform
  /// 
  /// # Returns
  /// A new Transform that is the result of applying the relative transform to this transform
  pub fn apply_to_new(&self, rel_transform: &Transform) -> Transform {
    let mut result = self.clone();
    result.apply(rel_transform);
    result
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

