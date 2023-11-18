// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::camera::{CadViewBundle, CadViewCamera, CadViewControllerSettings, CameraPlugin};
use crate::AppState;
use bevy::{
    core_pipeline::{bloom::BloomSettings, tonemapping::Tonemapping},
    prelude::*,
};
use bevy_mod_picking::prelude::*;
use molecule::{init_molecule, molecule_builder};

pub struct ScenePlugin;

/// This plugin handles scene related stuff
/// Scene logic is only active during the State `AppState::Active`
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((CameraPlugin, DefaultPickingPlugins))
            .add_systems(OnEnter(AppState::Active), setup_molecular_view)
            .add_systems(OnEnter(AppState::Active), init_molecule)
            .add_systems(Update, molecule_builder.run_if(in_state(AppState::Active)))
            .add_systems(
                Update,
                bind_light_to_camera.run_if(in_state(AppState::Active)),
            );
    }
}

/// Marks a light source as being behind the camera.  Such light(s) will have
/// their position and rotation transforms updated every frame to remain
/// behind the camera.
#[derive(Component, Default)]
struct BehindCameraLight;

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

    // light source
    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 10000.,
                shadows_enabled: false,
                ..default()
            },
            ..default()
        },
        // A directional light needs to have its transform specified.  In this
        // particular case the light be updated every frame to be behind the
        // camera (see `bind_light_to_camera`), so we needn't specify an
        // initial transform here.
        BehindCameraLight,
    ));
}

/// Bevy system which binds updates the directional light to always be behind
/// the camera.  This is a simple way to get a "sun" effect that is independent
/// of the camera's point of view.
fn bind_light_to_camera(
    // No light will ever have the `CadViewCamera` tag, but Bevy doesn't know
    // that.  Without this restriction it will complain that the queries are
    // potentially overlapping, resulting in an illegal double-borrowing.
    mut light: Query<&mut Transform, (With<BehindCameraLight>, Without<CadViewCamera>)>,
    camera: Query<&Transform, With<CadViewCamera>>,
) {
    // Point the light in the same direction as the camera. The position of a
    // directional light doesn't matter, so we only need to bother with that
    // if we use spot or point lights.  The scale never matters.  As a micro
    // optimization, we only copy the fields we need.
    let camera_transform = camera.single();
    let mut light_transform = light.single_mut();
    // Uncomment the following line if we start using point or spot lights.
    //light_transform.translation = camera_transform.translation;
    light_transform.rotation = camera_transform.rotation;
}

// End of File
