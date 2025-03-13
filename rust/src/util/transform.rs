use glam::f32::Vec3;
use glam::f32::Quat;

#[derive(Clone)]
pub struct Transform {
  pub translation: Vec3,
  pub rotation: Quat,
}

impl Transform {
  pub fn new(translation: Vec3, rotation: Quat) -> Self {
    Self { translation, rotation }
  }
}

impl Default for Transform {
  fn default() -> Self {
    Self { translation: Vec3::ZERO, rotation: Quat::IDENTITY }
  }
}
