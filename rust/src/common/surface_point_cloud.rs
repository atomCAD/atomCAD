use glam::f32::Vec3;
use crate::util::transform::Transform;

pub struct SurfacePoint {
  pub position: Vec3,
  pub normal: Vec3, // points outwards
}

pub struct SurfacePointCloud {
  pub frame_transform: Transform,
  pub points: Vec<SurfacePoint>,
}

impl SurfacePointCloud {

  pub fn new() -> Self {
    Self {
      frame_transform: Transform::default(),
      points: Vec::new(),
    }
  }
}
