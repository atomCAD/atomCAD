// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::AppState;
use bevy::prelude::*;

pub struct CadViewPlugin;

impl Plugin for CadViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::CadView), setup_cad_view)
            .add_systems(OnExit(AppState::CadView), cleanup_cad_view);
    }
}

// Tag component used to tag entities added on in CAD view.
#[derive(Component)]
struct OnCadView;

fn setup_cad_view(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Spawn a 3D camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        OnCadView,
    ));

    // Add a light
    commands.spawn((
        PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
        OnCadView,
    ));

    // Add a box centered at the origin
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        OnCadView,
    ));
}

fn cleanup_cad_view(mut commands: Commands, entities: Query<Entity, With<OnCadView>>) {
    for entity in entities.iter() {
        commands.entity(entity).despawn();
    }
}

// End of File
