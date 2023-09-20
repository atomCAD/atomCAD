// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bevy::prelude::{Input, KeyCode, Res};

pub enum KeyCommand {
    Up,
    Down,
    Left,
    Right,
}

impl KeyCommand {
    pub fn pressed(&self, keyboard_input: &Res<Input<KeyCode>>) -> bool {
        match self {
            KeyCommand::Up => {
                keyboard_input.pressed(KeyCode::W) || keyboard_input.pressed(KeyCode::Up)
            }
            KeyCommand::Down => {
                keyboard_input.pressed(KeyCode::S) || keyboard_input.pressed(KeyCode::Down)
            }
            KeyCommand::Left => {
                keyboard_input.pressed(KeyCode::A) || keyboard_input.pressed(KeyCode::Left)
            }
            KeyCommand::Right => {
                keyboard_input.pressed(KeyCode::D) || keyboard_input.pressed(KeyCode::Right)
            }
        }
    }
}

pub fn get_movement(control: KeyCommand, input: &Res<Input<KeyCode>>) -> f32 {
    if control.pressed(input) {
        1.0
    } else {
        0.0
    }
}

// End of File
