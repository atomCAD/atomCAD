// TODO: these will not be constant, will be set by the user
use glam::f32::Vec3;
use glam::f64::DVec3;

pub const REAL_IMPLICIT_VOLUME_MIN: DVec3 = DVec3::new(-800.0, -800.0, -800.0);
pub const REAL_IMPLICIT_VOLUME_MAX: DVec3 = DVec3::new(800.0, 800.0, 800.0);

pub const MAX_EVAL_CACHE_SIZE: i32 = 1000000;

// Gadget constants
pub const HANDLE_TRIANGLE_SIDE_LENGTH: f64 = 1.2;
pub const HANDLE_RADIUS: f64 = 0.4;
pub const HANDLE_RADIUS_HIT_TEST_FACTOR: f64 = 1.2;
pub const HANDLE_DIVISIONS: u32 = 16;
pub const HANDLE_HEIGHT: f64 = 0.6;
pub const HANDLE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.8); // Dark blue for handles
pub const SELECTED_HANDLE_COLOR: Vec3 = Vec3::new(1.0, 0.6, 0.0); // Orange for selected handle
pub const LINE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.6);
pub const LINE_DIVISIONS: u32 = 16;
pub const LINE_RADIUS: f64 = 0.18;
pub const LINE_RADIUS_HIT_TEST_FACTOR: f64 = 1.3;

pub const CONNECTED_PIN_SYMBOL: &str = "⎆";
















