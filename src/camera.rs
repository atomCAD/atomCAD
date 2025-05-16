// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::{
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
};

/// Plugin for CAD-style orbit camera controls
pub struct CadCameraPlugin;

impl Plugin for CadCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (camera_orbit_system, camera_pan_system, camera_zoom_system).chain(),
        );
    }
}

/// Marks a camera as CAD-controllable
#[derive(Component)]
pub struct CadCamera {
    /// The point the camera orbits around
    pub focus_point: Vec3,
    /// Distance from the focus point
    pub distance: f32,
    /// Spherical coordinates: (theta, phi) in radians
    /// theta: rotation around Y axis (yaw)
    /// phi: rotation from XZ plane (pitch)
    pub spherical_coords: Vec2,
    /// Camera movement sensitivity
    pub orbit_sensitivity: f32,
    pub pan_sensitivity: f32,
    pub zoom_sensitivity: f32,
    /// Constraints
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_pitch: f32, // Prevent gimbal lock
    pub max_pitch: f32,
}

impl Default for CadCamera {
    fn default() -> Self {
        Self {
            focus_point: Vec3::ZERO,
            distance: 10.0,
            spherical_coords: Vec2::new(0.0, std::f32::consts::FRAC_PI_4),
            orbit_sensitivity: 0.01,
            pan_sensitivity: 0.02,
            zoom_sensitivity: 0.1,
            min_distance: 1.0,
            max_distance: 100.0,
            min_pitch: -std::f32::consts::FRAC_PI_2 + 0.001,
            max_pitch: std::f32::consts::FRAC_PI_2 - 0.001,
        }
    }
}

/// Handle mouse rotation (orbit)
fn camera_orbit_system(
    mut mouse_motion: MessageReader<MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut CadCamera)>,
) {
    // Rotate with left mouse (without modifier) OR with Cmd+mouse on Mac
    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let alt_held = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);

    let is_orbit = buttons.pressed(MouseButton::Left) && !shift_held && !alt_held;
    if !is_orbit {
        mouse_motion.clear();
        return;
    }

    let delta_mouse: Vec2 = mouse_motion
        .read()
        .map(|event| event.delta)
        .fold(Vec2::ZERO, |acc, delta| acc + delta);

    if delta_mouse.is_finite() && delta_mouse != Vec2::ZERO {
        for (mut transform, mut cam) in query.iter_mut() {
            // Update spherical coordinates
            cam.spherical_coords.x -= delta_mouse.x * cam.orbit_sensitivity;
            cam.spherical_coords.y += delta_mouse.y * cam.orbit_sensitivity;

            // Clamp pitch to prevent gimbal lock
            cam.spherical_coords.y = cam.spherical_coords.y.clamp(cam.min_pitch, cam.max_pitch);

            update_camera_transform(&mut transform, &cam);
        }
    }
}

/// Handle mouse pan (middle mouse button)
fn camera_pan_system(
    mut mouse_motion: MessageReader<MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Transform, &mut CadCamera)>,
) {
    // Pan with middle mouse OR Shift+mouse OR right mouse
    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    let is_pan = buttons.pressed(MouseButton::Middle)
        || buttons.pressed(MouseButton::Right)
        || (buttons.pressed(MouseButton::Left) && shift_held);
    if !is_pan {
        mouse_motion.clear();
        return;
    }

    let delta_mouse: Vec2 = mouse_motion
        .read()
        .map(|event| event.delta)
        .fold(Vec2::ZERO, |acc, delta| acc + delta);

    if delta_mouse.is_finite() && delta_mouse != Vec2::ZERO {
        for (transform, mut cam) in query.iter_mut() {
            // Calculate pan in screen space
            let right = transform.right();
            let up = transform.up();

            let pan_offset = -right * delta_mouse.x * cam.pan_sensitivity * cam.distance * 0.1
                + up * delta_mouse.y * cam.pan_sensitivity * cam.distance * 0.1;

            cam.focus_point += pan_offset;
        }
    }
}

/// Handle mouse wheel zoom
fn camera_zoom_system(
    mut scroll_events: MessageReader<MouseWheel>,
    mut query: Query<(&mut Transform, &mut CadCamera)>,
) {
    let scroll_delta: f32 = scroll_events
        .read()
        .map(|event| {
            // Handle both traditional scroll wheel and trackpad
            match event.unit {
                MouseScrollUnit::Line => event.y,
                MouseScrollUnit::Pixel => event.y * 0.01, // Scale down pixel values
            }
        })
        .fold(0.0, |acc, delta| acc + delta);

    if scroll_delta != 0.0 {
        for (mut transform, mut cam) in query.iter_mut() {
            // Zoom by adjusting distance
            let zoom_factor = 1.0 - scroll_delta * cam.zoom_sensitivity;
            cam.distance = (cam.distance * zoom_factor).clamp(cam.min_distance, cam.max_distance);

            update_camera_transform(&mut transform, &cam);
        }
    }
}

/// Update camera transform based on spherical coordinates
fn update_camera_transform(transform: &mut Transform, cam: &CadCamera) {
    // Convert spherical to Cartesian coordinates
    let theta = cam.spherical_coords.x;
    let phi = cam.spherical_coords.y;

    let x = cam.distance * phi.cos() * theta.sin();
    let y = cam.distance * phi.sin();
    let z = cam.distance * phi.cos() * theta.cos();

    let position = cam.focus_point + Vec3::new(x, y, z);

    // Update transform
    transform.translation = position;
    transform.look_at(cam.focus_point, Vec3::Y);
}

pub enum ViewDirection {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
}

/// Helper function to focus on a specific object
impl CadCamera {
    pub fn focus_on(&mut self, target: Vec3, distance: Option<f32>) {
        self.focus_point = target;
        if let Some(dist) = distance {
            self.distance = dist.clamp(self.min_distance, self.max_distance);
        }
    }

    /// Reset to default view
    pub fn reset_view(&mut self) {
        self.focus_point = Vec3::ZERO;
        self.distance = 10.0;
        self.spherical_coords = Vec2::new(0.0, std::f32::consts::FRAC_PI_4);
    }

    /// Set view direction (front, top, side, etc.)
    pub fn set_view_direction(&mut self, direction: ViewDirection) {
        match direction {
            ViewDirection::Front => self.spherical_coords = Vec2::new(0.0, 0.0),
            ViewDirection::Back => self.spherical_coords = Vec2::new(std::f32::consts::PI, 0.0),
            ViewDirection::Left => {
                self.spherical_coords = Vec2::new(-std::f32::consts::FRAC_PI_2, 0.0)
            }
            ViewDirection::Right => {
                self.spherical_coords = Vec2::new(std::f32::consts::FRAC_PI_2, 0.0)
            }
            ViewDirection::Top => {
                self.spherical_coords = Vec2::new(0.0, std::f32::consts::FRAC_PI_2)
            }
            ViewDirection::Bottom => {
                self.spherical_coords = Vec2::new(0.0, -std::f32::consts::FRAC_PI_2)
            }
            ViewDirection::Isometric => {
                self.spherical_coords =
                    Vec2::new(std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4)
            }
        }
    }
}

// End of File
