use glam::f64::DVec3;
use glam::f64::DMat4;
use glam::f64::DQuat;
use crate::api::common_api_types::APICameraCanonicalView;

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
  pub pivot_point: DVec3,
}

impl Camera {
  pub fn build_view_matrix(&self) -> DMat4 {
      DMat4::look_at_rh(self.eye, self.target, self.up)
  }

  pub fn build_projection_matrix(&self) -> DMat4 {
      if self.orthographic {
          // Calculate the orthographic projection matrix
          let right = self.ortho_half_height * self.aspect;
          DMat4::orthographic_rh(
              -right, right,
              -self.ortho_half_height, self.ortho_half_height,
              self.znear, self.zfar
          )
      } else {
          DMat4::perspective_rh_gl(self.fovy, self.aspect, self.znear, self.zfar)
      }
  }

  pub fn build_view_projection_matrix(&self) -> DMat4 {
      let view = self.build_view_matrix();
      let proj = self.build_projection_matrix();
      // println!("Projection matrix: {:?}", proj);
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

  pub fn get_canonical_view(&self) -> APICameraCanonicalView {
    // Calculate view direction (from eye to target)
    let view_dir = (self.target - self.eye).normalize();
    
    // Check for alignment with cardinal axes
    // We use a small epsilon for floating point comparison
    const EPSILON: f64 = 0.001;
    
    // Check if the view direction is aligned with positive or negative X, Y, or Z axis
    // These direction checks must match the directions set in set_canonical_view
    // Z-up coordinate system: X=right, Y=forward, Z=up
    if (view_dir - DVec3::new(-1.0, 0.0, 0.0)).length_squared() < EPSILON {
      return APICameraCanonicalView::Right;
    } else if (view_dir - DVec3::new(1.0, 0.0, 0.0)).length_squared() < EPSILON {
      return APICameraCanonicalView::Left;
    } else if (view_dir - DVec3::new(0.0, 0.0, -1.0)).length_squared() < EPSILON {
      return APICameraCanonicalView::Top;
    } else if (view_dir - DVec3::new(0.0, 0.0, 1.0)).length_squared() < EPSILON {
      return APICameraCanonicalView::Bottom;
    } else if (view_dir - DVec3::new(0.0, -1.0, 0.0)).length_squared() < EPSILON {
      return APICameraCanonicalView::Back;
    } else if (view_dir - DVec3::new(0.0, 1.0, 0.0)).length_squared() < EPSILON {
      return APICameraCanonicalView::Front;
    }
    
    // If not aligned with any cardinal direction, return Custom
    APICameraCanonicalView::Custom
  }
  
  pub fn set_canonical_view(&mut self, view: APICameraCanonicalView) {
    // If view is Custom, do nothing
    if matches!(view, APICameraCanonicalView::Custom) {
      return;
    }
    
    // Define a constant distance for canonical views
    const CANONICAL_DISTANCE: f64 = 40.0;
    
    // Set target to origin
    self.target = DVec3::new(0.0, 0.0, 0.0);
    
    // Define the viewing direction and up vectors for each canonical view
    // Z-up coordinate system: X=right, Y=forward, Z=up
    let (view_dir, up) = match view {
      APICameraCanonicalView::Top => (
        DVec3::new(0.0, 0.0, -1.0),    // Looking down from +Z
        DVec3::new(0.0, -1.0, 0.0)     // Up is -Y (screen up when looking down)
      ),
      APICameraCanonicalView::Bottom => (
        DVec3::new(0.0, 0.0, 1.0),     // Looking up from -Z
        DVec3::new(0.0, 1.0, 0.0)      // Up is +Y (screen up when looking up)
      ),
      APICameraCanonicalView::Front => (
        DVec3::new(0.0, 1.0, 0.0),     // Looking from -Y (towards +Y)
        DVec3::new(0.0, 0.0, 1.0)      // Up is +Z
      ),
      APICameraCanonicalView::Back => (
        DVec3::new(0.0, -1.0, 0.0),    // Looking from +Y (towards -Y)
        DVec3::new(0.0, 0.0, 1.0)      // Up is +Z
      ),
      APICameraCanonicalView::Left => (
        DVec3::new(1.0, 0.0, 0.0),     // Looking from -X (towards +X)
        DVec3::new(0.0, 0.0, 1.0)      // Up is +Z
      ),
      APICameraCanonicalView::Right => (
        DVec3::new(-1.0, 0.0, 0.0),    // Looking from +X (towards -X)
        DVec3::new(0.0, 0.0, 1.0)      // Up is +Z
      ),
      APICameraCanonicalView::Custom => {
        // This shouldn't happen because of the check at the beginning
        // But we provide a default value for completeness
        (DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, 0.0, 1.0))
      }
    };
    
    // Set eye position at CANONICAL_DISTANCE away from the origin in the view direction
    // We subtract the view_dir because we want to look toward the target from that direction
    self.eye = self.target - view_dir * CANONICAL_DISTANCE;
    
    // Set the up direction
    self.up = up;
    
    // If in orthographic mode, adjust ortho_half_height based on fovy
    if self.orthographic {
      // Calculate ortho_half_height that would give the same view frustum at the target distance
      // tan(fovy/2) * distance gives the half-height of the view frustum at that distance
      self.ortho_half_height = (self.fovy / 2.0).tan() * CANONICAL_DISTANCE;
    }
  }
}
