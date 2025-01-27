use glam::f32::Vec3;
use glam::f32::Mat4;
use glam::f32::Quat;

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

  pub fn calc_headlight_direction(&self) -> Vec3 {
    let forward = (self.target - self.eye).normalize();
    let right = forward.cross(self.up).normalize();

    // Create a quaternion for a slight downward rotation (20 degrees)
    let angle_in_radians = 20.0_f32.to_radians();
    let rotation = Quat::from_axis_angle(right, -angle_in_radians);

    return rotation * forward;
  }
}
