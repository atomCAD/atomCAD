// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bevy::math::Vec3Swizzles;
use bevy::prelude::*;

use crate::actions::game_control::{get_movement, GameControl};
use crate::scene::Torus;
use crate::AppState;

mod game_control;

pub const FOLLOW_EPSILON: f32 = 5.;

pub struct ActionsPlugin;

// This plugin listens for keyboard input and converts the input into Actions
// Actions can then be used as a resource in other systems to act on the user input.
impl Plugin for ActionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Actions>().add_systems(
            Update,
            set_movement_actions.run_if(in_state(AppState::Active)),
        );
    }
}

#[derive(Default, Resource)]
pub struct Actions {
    pub torus_movement: Option<Vec2>,
}

pub fn set_movement_actions(
    mut actions: ResMut<Actions>,
    keyboard_input: Res<Input<KeyCode>>,
    touch_input: Res<Touches>,
    torus: Query<&Transform, With<Torus>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) {
    let mut torus_movement = Vec2::new(
        get_movement(GameControl::Right, &keyboard_input)
            - get_movement(GameControl::Left, &keyboard_input),
        get_movement(GameControl::Up, &keyboard_input)
            - get_movement(GameControl::Down, &keyboard_input),
    );

    if let Some(touch_position) = touch_input.first_pressed_position() {
        let (camera, camera_transform) = camera.single();
        if let Some(touch_position) = camera.viewport_to_world_2d(camera_transform, touch_position)
        {
            let diff = touch_position - torus.single().translation.xy();
            if diff.length() > FOLLOW_EPSILON {
                torus_movement = diff.normalize();
            }
        }
    }

    if torus_movement != Vec2::ZERO {
        actions.torus_movement = Some(torus_movement.normalize());
    } else {
        actions.torus_movement = None;
    }
}

// End of File
