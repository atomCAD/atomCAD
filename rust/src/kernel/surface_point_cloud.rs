use glam::f32::Vec3;

pub struct SurfacePoint {
  pub position: Vec3,
  pub normal: Vec3, // points outwards
}

pub struct SurfacePointCloud {
  pub points: Vec<SurfacePoint>,
}

impl SurfacePointCloud {

  pub fn new() -> Self {
    Self {
      points: Vec::new(),
    }
  }
}
