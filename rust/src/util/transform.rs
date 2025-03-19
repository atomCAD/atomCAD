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
}

impl Default for Transform {
  fn default() -> Self {
    Self { translation: DVec3::ZERO, rotation: DQuat::IDENTITY }
  }
}
