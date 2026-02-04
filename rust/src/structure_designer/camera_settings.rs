use glam::f64::DVec3;

/// Camera settings stored per node network
#[derive(Clone, Debug, PartialEq)]
pub struct CameraSettings {
    pub eye: DVec3,
    pub target: DVec3,
    pub up: DVec3,
    pub orthographic: bool,
    pub ortho_half_height: f64,
    pub pivot_point: DVec3,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            eye: DVec3::new(0.0, -30.0, 10.0),
            target: DVec3::new(0.0, 0.0, 0.0),
            up: DVec3::new(0.0, 0.32, 0.95),
            orthographic: false,
            ortho_half_height: 10.0,
            pivot_point: DVec3::new(0.0, 0.0, 0.0),
        }
    }
}
