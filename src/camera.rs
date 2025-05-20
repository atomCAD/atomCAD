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
    /// Camera movement sensitivity
    pub orbit_sensitivity: f32,
    pub pan_sensitivity: f32,
    pub zoom_sensitivity: f32,
    /// Constraints
    pub min_distance: f32,
    pub max_distance: f32,
}

impl Default for CadCamera {
    fn default() -> Self {
        Self {
            focus_point: Vec3::ZERO,
            distance: 10.0,
            orbit_sensitivity: 0.01,
            pan_sensitivity: 0.02,
            zoom_sensitivity: 0.1,
            min_distance: 1.0,
            max_distance: 100.0,
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
        for (mut transform, cam) in query.iter_mut() {
            // Get the current camera orientation
            let current_rotation = transform.rotation;

            // Create rotation quaternions for yaw and pitch
            let yaw = Quat::from_rotation_y(-delta_mouse.x * cam.orbit_sensitivity);
            let pitch = Quat::from_rotation_x(-delta_mouse.y * cam.orbit_sensitivity);

            // Apply rotations relative to current orientation
            transform.rotation = current_rotation * yaw * pitch;

            // Update camera position based on new orientation
            let forward = transform.rotation * -Vec3::Z;
            transform.translation = cam.focus_point - forward * cam.distance;
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

            // Update camera position based on new distance
            let forward = transform.rotation * -Vec3::Z;
            transform.translation = cam.focus_point - forward * cam.distance;
        }
    }
}

// End of File
