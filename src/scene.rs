// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::camera::CameraPlugin;
use crate::AppState;
use bevy::prelude::*;
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin};
use bevy_mod_picking::DefaultPickingPlugins;
use molecule::{init_molecule, molecule_builder};

pub struct ScenePlugin;

/// This plugin handles scene related stuff
/// Scene logic is only active during the State `AppState::Active`
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((CameraPlugin, DefaultPickingPlugins, InfiniteGridPlugin));
        app.add_systems(OnEnter(AppState::Active), setup_molecular_view)
            .add_systems(OnEnter(AppState::Active), init_molecule)
            .add_systems(Update, molecule_builder.run_if(in_state(AppState::Active)));
    }
}

fn setup_molecular_view(mut commands: Commands) {
    // infinite grid
    commands.spawn(InfiniteGridBundle {
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..default()
    });

    // light source
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 10000.,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform {
            translation: Vec3::new(0., 1., 0.),
            rotation: Quat::from_rotation_x(-std::f32::consts::PI / 4.),
            ..default()
        },
        ..default()
    });
}

// End of File
