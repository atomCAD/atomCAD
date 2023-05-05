// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::actions::Actions;
use crate::loading::TextureAssets;
use crate::GameState;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub struct ScenePlugin;

#[derive(Component)]
pub struct Torus;

/// This plugin handles scene related stuff
/// Scene logic is only active during the State `GameState::Active`
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Active), spawn_scene)
            .add_systems(Update, ui_hello_world.run_if(in_state(GameState::Active)))
            .add_systems(Update, move_torus.run_if(in_state(GameState::Active)));
    }
}

fn ui_hello_world(mut egui_contexts: EguiContexts) {
    egui::Window::new("Hello").show(egui_contexts.ctx_mut(), |ui| {
        ui.label("Hello World!");
    });
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    _textures: Res<TextureAssets>,
) {
    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 1.5, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // torus
    commands
        .spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Torus {
                radius: 1.0,
                subdivisions_segments: 4,
                subdivisions_sides: 16,
                ..default()
            })),
            material: materials.add(Color::rgb(0.2, 0.8, 0.4).into()),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        })
        .insert(Torus);

    // light source
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 10000.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(-4.0, 8.0, 4.0),
        ..default()
    });
}

fn move_torus(
    time: Res<Time>,
    actions: Res<Actions>,
    mut torus_query: Query<&mut Transform, With<Torus>>,
) {
    if actions.torus_movement.is_none() {
        return;
    }
    let speed = 1.;
    let movement = Vec3::new(
        actions.torus_movement.unwrap().x * speed * time.delta_seconds(),
        0.,
        -actions.torus_movement.unwrap().y * speed * time.delta_seconds(),
    );
    for mut torus_transform in &mut torus_query {
        torus_transform.translation += movement;
    }
}

// End of File
