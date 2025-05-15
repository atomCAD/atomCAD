// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    AppState, AtomCluster, AtomClusterPlugin, AtomInstance, CadCamera, CadCameraPlugin, FontAssets,
};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::{camera::primitives::Aabb, prelude::*};

pub struct CadViewPlugin;

impl Plugin for CadViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AtomClusterPlugin,
            CadCameraPlugin,
            FrameTimeDiagnosticsPlugin {
                max_history_length: 9,
                smoothing_factor: 0.2,
            },
        ))
        .add_systems(OnEnter(AppState::CadView), setup_cad_view)
        .add_systems(OnExit(AppState::CadView), cleanup_cad_view)
        .add_systems(
            Update,
            update_fps_display.run_if(in_state(AppState::CadView)),
        );
    }
}

// Tag component used to tag entities added on in CAD view.
#[derive(Component)]
struct OnCadView;

// Component to mark the FPS text entity
#[derive(Component)]
struct FpsText;

fn setup_cad_view(mut commands: Commands, font_assets: Res<FontAssets>) {
    // Spawn a 3D camera
    let camera_position = Vec3::new(0.0, 1.5, 5.0);
    let focus_point = Vec3::ZERO;
    let distance = camera_position.distance(focus_point);
    let delta = camera_position - focus_point;
    let theta = delta.x.atan2(delta.z);
    let phi = (delta.y / distance).asin();
    let spherical_coords = Vec2::new(theta, phi);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(focus_point, Vec3::Y),
        CadCamera {
            focus_point,
            distance,
            spherical_coords,
            ..default()
        },
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

    // Add a sphere cloud
    commands.spawn((
        AtomCluster {
            atoms: vec![
                // Water molecule

                // Oxygen (center)
                AtomInstance {
                    position: Vec3::new(0.0, 0.0, 0.0),
                    kind: 8,
                },
                // Hydrogen 1
                AtomInstance {
                    position: Vec3::new(-0.757, 0.586, 0.0),
                    kind: 1,
                },
                // Hydrogen 2
                AtomInstance {
                    position: Vec3::new(0.757, 0.586, 0.0),
                    kind: 1,
                },
            ],
        },
        Transform::default(),
        Visibility::default(),
        Aabb {
            center: Vec3A::ZERO,
            half_extents: Vec3A::new(1.0, 1.0, 1.0),
        },
        OnCadView,
    ));

    // Add FPS counter in the top-left corner
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                padding: UiRect::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            OnCadView,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("FPS: --"),
                TextFont {
                    font: font_assets.fira_sans_regular.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                FpsText,
            ));
        });
}

fn update_fps_display(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    if let Ok(mut text) = query.single_mut()
        && let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS)
        && let Some(value) = fps.smoothed()
    {
        text.0 = format!("FPS: {value:>4.1}");
    }
}

fn cleanup_cad_view(mut commands: Commands, entities: Query<Entity, With<OnCadView>>) {
    for entity in entities.iter() {
        commands.entity(entity).despawn();
    }
}

// End of File
