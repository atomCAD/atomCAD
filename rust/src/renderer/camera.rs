use glam::f32::Vec3;
use glam::f32::Mat4;

pub struct Camera {
  pub eye: Vec3,
  pub target: Vec3,
  pub up: Vec3,
  pub aspect: f32,
  pub fovy: f32, // in radians
  pub znear: f32,
  pub zfar: f32,
}

impl Camera {
  pub fn build_view_projection_matrix(&self) -> Mat4 {
      let view = Mat4::look_at_rh(self.eye, self.target, self.up);
      let proj = Mat4::perspective_rh_gl(self.fovy, self.aspect, self.znear, self.zfar);
      return proj * view;
  }
}
