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
    /// Navigation up-axis (turntable screen-vertical). Default `+Z`. See
    /// issue #349 / `doc/design_view_up_axis.md`.
    pub nav_up: DVec3,
    /// Cosmetic provenance label for `nav_up` (e.g. `"Z"`, `"(1 1 1)"`).
    pub nav_up_label: String,
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
            nav_up: DVec3::Z,
            nav_up_label: "Z".to_string(),
        }
    }
}
