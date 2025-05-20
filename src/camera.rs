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
}

impl Default for CadCamera {
    fn default() -> Self {
        Self {
            focus_point: Vec3::ZERO,
            distance: 10.0,
            orbit_sensitivity: 0.01,
            pan_sensitivity: 0.02,
            zoom_sensitivity: 0.1,
        }
    }
}

impl CadCamera {
    /// Update camera position based on its current rotation and distance from focus point
    pub fn update_position(&self, transform: &mut Transform) {
        let forward = transform.rotation * -Vec3::Z;
        transform.translation = self.focus_point - forward * self.distance;
    }

    /// Handle orbit (rotation) movement
    pub fn orbit(&self, transform: &mut Transform, delta: Vec2) {
        // Get the current camera orientation
        let current_rotation = transform.rotation;

        // Create rotation quaternions for yaw and pitch
        let yaw = Quat::from_rotation_y(-delta.x * self.orbit_sensitivity);
        let pitch = Quat::from_rotation_x(-delta.y * self.orbit_sensitivity);

        // Apply rotations relative to current orientation
        transform.rotation = current_rotation * yaw * pitch;

        // Update the camera position
        self.update_position(transform);
    }

    /// Handle pan movement
    pub fn pan(&mut self, transform: &mut Transform, delta: Vec2) {
        // Calculate pan in screen space
        let right = transform.right();
        let up = transform.up();

        // Calculate the pan offset
        let pan_offset = -right * delta.x * self.pan_sensitivity * self.distance * 0.1
            + up * delta.y * self.pan_sensitivity * self.distance * 0.1;

        // Update the focus point
        self.focus_point += pan_offset;

        // Update the camera position
        self.update_position(transform);
    }

    /// Handle zoom movement
    pub fn zoom(&mut self, transform: &mut Transform, delta: f32) {
        // Zoom by adjusting distance
        let zoom_factor = 1.0 - delta * self.zoom_sensitivity;
        self.distance *= zoom_factor;

        // Update the camera position
        self.update_position(transform);
    }
}

/// Handle mouse rotation (orbit)
fn camera_orbit_system(
    mut mouse_motion: MessageReader<MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &CadCamera)>,
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
            cam.orbit(&mut transform, delta_mouse);
        }
    }
}

/// Handle mouse pan (middle mouse button)
fn camera_pan_system(
    mut mouse_motion: MessageReader<MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut CadCamera)>,
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
        for (mut transform, mut cam) in query.iter_mut() {
            cam.pan(&mut transform, delta_mouse);
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
            cam.zoom(&mut transform, scroll_delta);
        }
    }
}

// End of File
