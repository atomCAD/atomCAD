use glam::f64::DVec3;
use glam::f64::DMat4;
use glam::f64::DQuat;

pub struct Camera {
  pub eye: DVec3,
  pub target: DVec3,
  pub up: DVec3,
  pub aspect: f64,
  pub fovy: f64, // in radians
  pub znear: f64,
  pub zfar: f64,
  pub orthographic: bool,
  pub ortho_half_height: f64,
}

impl Camera {
  pub fn build_view_projection_matrix(&self) -> DMat4 {
      let view = DMat4::look_at_rh(self.eye, self.target, self.up);
      let proj = if self.orthographic {
          // Calculate the orthographic projection matrix
          let right = self.ortho_half_height * self.aspect;
          DMat4::orthographic_rh_gl(
              -right, right,
              -self.ortho_half_height, self.ortho_half_height,
              self.znear, self.zfar
          )
      } else {
          // Use the existing perspective projection
          DMat4::perspective_rh_gl(self.fovy, self.aspect, self.znear, self.zfar)
      };
      return proj * view;
  }

  pub fn calc_headlight_direction(&self) -> DVec3 {
    let forward = (self.target - self.eye).normalize();
    let right = forward.cross(self.up).normalize();

    // Create a quaternion for a slight downward rotation (20 degrees)
    let angle_in_radians = 20.0_f64.to_radians();
    let rotation = DQuat::from_axis_angle(right, -angle_in_radians);

    return rotation * forward;
  }
}
