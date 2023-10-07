// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::camera::{CadViewBundle, CadViewControllerSettings, CameraPlugin};
use crate::AppState;
use bevy::{
    core_pipeline::{bloom::BloomSettings, tonemapping::Tonemapping},
    prelude::*,
};
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin};
use bevy_mod_picking::prelude::*;
use molecule::{init_molecule, molecule_builder};

pub struct ScenePlugin;

/// This plugin handles scene related stuff
/// Scene logic is only active during the State `AppState::Active`
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((CameraPlugin, DefaultPickingPlugins, InfiniteGridPlugin))
            .add_systems(OnEnter(AppState::Active), setup_molecular_view)
            .add_systems(OnEnter(AppState::Active), init_molecule)
            .add_systems(Update, molecule_builder.run_if(in_state(AppState::Active)));
    }
}

fn setup_molecular_view(mut commands: Commands) {
    // The initial position of the camera.
    let position = Vec3::new(0.0, 1.5, 6.0);
    // The direction the camera is looking.
    let target = Vec3::ZERO;
    // The "up" direction of the camera.
    let up = Vec3::Y;
    commands
        .spawn((
            Camera3dBundle {
                camera: Camera {
                    hdr: true, // HDR is required for bloom effects.
                    ..Default::default()
                },
                // Using a tonemapper that desaturates to white is recommneded
                // when using bloom effects.
                tonemapping: Tonemapping::TonyMcMapface,
                ..Default::default()
            },
            // Enable picking from this camera view.
            RaycastPickCamera::default(),
            // Enable bloom effects with default settings.
            BloomSettings::default(),
        ))
        // The `CadViewBundle` needs to be inserted rather than part of the
        // above spawn command because it contains a duplicate of the
        // `Transform` component.
        .insert(CadViewBundle::new(
            CadViewControllerSettings::default(),
            position,
            target,
            up,
        ));

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
