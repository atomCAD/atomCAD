// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::AppState;
use bevy::{
    core_pipeline::{bloom::BloomSettings, tonemapping::Tonemapping},
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use bevy_egui::EguiContexts;

const CONST_SCROLL_SPEED_PIXELS: f32 = 0.05;
const CONST_SCROLL_SPEED_LINES: f32 = 1.6;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Active), spawn_camera)
            .add_systems(Update, pan_orbit_camera.run_if(in_state(AppState::Active)));
    }
}

fn spawn_camera(mut commands: Commands) {
    let position = Vec3::new(0.0, 1.5, 6.0);
    let target = Vec3::ZERO;
    commands.spawn((
        Camera3dBundle {
            camera: Camera {
                hdr: true, // HDR is required for bloom effects.
                ..default()
            },
            // Using a tonemapper that desaturates to white is recommneded
            // when using bloom effects.
            tonemapping: Tonemapping::TonyMcMapface,
            transform: Transform::from_translation(position).looking_at(target, Vec3::Y),
            ..default()
        },
        // Enable bloom effects with default settings.
        BloomSettings::default(),
        PanOrbitCamera {
            radius: (position - target).length(),
            ..default()
        },
    ));
}

// Copied from the Unofficial Bevy Cheat Book
// https://bevy-cheatbook.github.io/cookbook/pan-orbit-camera.html
// with minimal tweaks (so far).
#[derive(Component)]
pub struct PanOrbitCamera {
    pub focus: Vec3,
    pub radius: f32,
    pub inverted: bool,
}

impl Default for PanOrbitCamera {
    fn default() -> Self {
        PanOrbitCamera {
            focus: Vec3::ZERO,
            radius: 10.0,
            inverted: false,
        }
    }
}

pub fn pan_orbit_camera(
    window: Query<&Window, With<PrimaryWindow>>,
    mut egui_contexts: EguiContexts,
    mut ev_motion: EventReader<MouseMotion>,
    mut ev_scroll: EventReader<MouseWheel>,
    input_mouse: Res<Input<MouseButton>>,
    mut query: Query<(&mut PanOrbitCamera, &mut Transform, &Projection)>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };
    let window_size = Vec2::new(window.width(), window.height());

    // Don't move the camera if the cursor is outside the window.
    match window.cursor_position() {
        None => return,
        Some(pos) => {
            if pos.x < 0.0 || window_size.x < pos.x || pos.y < 0.0 || window_size.y < pos.y {
                return;
            }
        }
    };

    if egui_contexts.ctx_mut().is_pointer_over_area() {
        // don't move the camera if the mouse is over an egui element
        return;
    }

    // TODO: Fetch these from user settings.
    let orbit_button = MouseButton::Left;
    let pan_button = MouseButton::Right;

    let mut pan = Vec2::ZERO;
    let mut rotation_move = Vec2::ZERO;
    let mut scroll = 0.0;
    let mut orbit_button_changed = false;

    if input_mouse.pressed(orbit_button) {
        for ev in ev_motion.iter() {
            rotation_move += ev.delta;
        }
    } else if input_mouse.pressed(pan_button) {
        // Pan only if we're not rotating at the moment
        for ev in ev_motion.iter() {
            pan += ev.delta;
        }
    }
    for ev in ev_scroll.iter() {
        scroll += match ev.unit {
            MouseScrollUnit::Pixel => ev.y * CONST_SCROLL_SPEED_PIXELS,
            MouseScrollUnit::Line => ev.y * CONST_SCROLL_SPEED_LINES,
        };
    }
    if input_mouse.just_released(orbit_button) || input_mouse.just_pressed(orbit_button) {
        orbit_button_changed = true;
    }

    for (mut pan_orbit, mut transform, projection) in query.iter_mut() {
        if orbit_button_changed {
            let up = transform.rotation * Vec3::Y;
            pan_orbit.inverted = up.y <= 0.0;
        }

        if rotation_move.length_squared() > 0.0 {
            let delta_x = {
                let delta = rotation_move.x / window_size.x * 2.0 * std::f32::consts::PI;
                if pan_orbit.inverted {
                    -delta
                } else {
                    delta
                }
            };
            let delta_y = rotation_move.y / window_size.y * std::f32::consts::PI;
            let yaw = Quat::from_rotation_y(-delta_x);
            let pitch = Quat::from_rotation_x(-delta_y);
            transform.rotation = yaw * transform.rotation; // rotate around global y axis
            transform.rotation *= pitch; // rotate around local x axis
        } else if pan.length_squared() > 0.0 {
            // make panning distance independent of resolution and FOV
            if let Projection::Perspective(projection) = projection {
                pan *= Vec2::new(projection.fov * projection.aspect_ratio, projection.fov)
                    / window_size;
            }
            // translate by local axes
            let right = transform.rotation * Vec3::X * -pan.x;
            let up = transform.rotation * Vec3::Y * pan.y;
            // make panning proportional to distance away from the focus point
            let translation = (right + up) * pan_orbit.radius;
            pan_orbit.focus += translation;
        } else if scroll.abs() > 0.0 {
            pan_orbit.radius -= scroll * pan_orbit.radius * 0.2;
            // don't allow zooming in too close, or the camera will get stuck
            pan_orbit.radius = f32::max(pan_orbit.radius, 0.05);
        } else {
            continue;
        }

        // emulating parent/child to make the yaw/y-axis rotation behave like a turntable
        // parent = x and y rotation
        // child = z-offset
        let rot_matrix = Mat3::from_quat(transform.rotation);
        transform.translation =
            pan_orbit.focus + rot_matrix.mul_vec3(Vec3::new(0.0, 0.0, pan_orbit.radius));
    }
}

// End of File
