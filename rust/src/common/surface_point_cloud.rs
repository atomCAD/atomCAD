use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::util::transform::Transform;
use crate::util::transform::Transform2D;

pub struct SurfacePoint {
  pub position: DVec3,
  pub normal: DVec3, // points outwards
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

pub struct SurfacePoint2D {
  pub position: DVec2,
  pub normal: DVec2, // points outwards
}

pub struct SurfacePointCloud2D {
  pub frame_transform: Transform2D,
  pub points: Vec<SurfacePoint2D>,
}

impl SurfacePointCloud2D {

  pub fn new() -> Self {
    Self {
      frame_transform: Transform2D::default(),
      points: Vec::new(),
    }
  }
}
