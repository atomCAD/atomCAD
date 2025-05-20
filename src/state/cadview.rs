// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    AppState, CadCamera, CadCameraPlugin, FontAssetHandles, PdbAsset, PdbAssetHandles,
    PdbLoaderPlugin,
};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use molecule::MoleculeRenderPlugin;

pub struct CadViewPlugin;

impl Plugin for CadViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MoleculeRenderPlugin,
            PdbLoaderPlugin,
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

fn setup_cad_view(
    mut commands: Commands,
    font_asset_handles: Res<FontAssetHandles>,
    pdb_asset_handles: Res<PdbAssetHandles>,
    pdb_assets: Res<Assets<PdbAsset>>,
) {
    // Add an example molecule
    let neon_pump_imm = pdb_assets
        .get(&pdb_asset_handles.neon_pump_imm)
        .expect("Neon pump asset not loaded.");

    // Get the molecule's AABB
    let aabb = neon_pump_imm.aabb;
    let center: Vec3 = aabb.center.into();
    let half_extents: Vec3 = aabb.half_extents.into();

    // Spawn a 3D camera
    let camera_position = center + Vec3::new(0.0, half_extents.y * 2.0, half_extents.z * 4.0);
    let focus_point = center;
    let distance = camera_position.distance(focus_point);

    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(focus_point, Vec3::Y),
        CadCamera {
            focus_point,
            distance,
            ..default()
        },
        OnCadView,
    ));

    commands.spawn((
        neon_pump_imm.molecule.clone(),
        Transform::default(),
        Visibility::default(),
        aabb,
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
                    font: font_asset_handles.fira_sans_regular.clone(),
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
