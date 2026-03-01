// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::pdb::parse_pdb_content;
use crate::{
    AppState, CadCamera, CadCameraPlugin, FontAssetHandles, PdbAsset, PdbAssetHandles,
    PdbLoaderPlugin,
};
use bevy::camera::primitives::Aabb;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use molecule::{Molecule, MoleculeRenderPlugin};

/// Message to load a molecule from PDB file content,
/// replacing the currently displayed molecule.
#[derive(Message)]
pub struct LoadMolecule {
    pub name: String,
    pub content: String,
}

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
        .add_message::<LoadMolecule>()
        .add_systems(OnEnter(AppState::CadView), setup_cad_view)
        .add_systems(OnExit(AppState::CadView), cleanup_cad_view)
        .add_systems(
            Update,
            update_fps_display.run_if(in_state(AppState::CadView)),
        )
        .add_systems(
            Update,
            (start_loading_molecule, finish_loading_molecule).run_if(in_state(AppState::CadView)),
        );
    }
}

// Tag component used to tag entities added on in CAD view.
#[derive(Component)]
struct OnCadView;

// Component to mark the FPS text entity
#[derive(Component)]
struct FpsText;

/// Resource that holds an in-flight background task for loading a PDB file.
#[derive(Resource)]
struct PendingMolecule(Task<Result<PdbAsset, String>>);

fn spawn_molecule(commands: &mut Commands, molecule: Molecule, aabb: Aabb) -> Entity {
    commands
        .spawn((
            molecule,
            Transform::default(),
            Visibility::default(),
            aabb,
            OnCadView,
        ))
        .id()
}

fn spawn_camera(commands: &mut Commands, aabb: &Aabb) -> Entity {
    let center: Vec3 = aabb.center.into();
    let half_extents: Vec3 = aabb.half_extents.into();

    let camera_position = center + Vec3::new(0.0, half_extents.y * 2.0, half_extents.z * 4.0);
    let focus_point = center;
    let distance = camera_position.distance(focus_point);

    commands
        .spawn((
            Camera3d::default(),
            Transform::from_translation(camera_position).looking_at(focus_point, Vec3::Y),
            CadCamera {
                focus_point,
                distance,
                ..default()
            },
            OnCadView,
        ))
        .id()
}

fn setup_cad_view(
    mut commands: Commands,
    font_asset_handles: Res<FontAssetHandles>,
    pdb_asset_handles: Res<PdbAssetHandles>,
    pdb_assets: Res<Assets<PdbAsset>>,
) {
    let neon_pump_imm = pdb_assets
        .get(&pdb_asset_handles.neon_pump_imm)
        .expect("Neon pump asset not loaded.");

    spawn_molecule(
        &mut commands,
        neon_pump_imm.molecule.clone(),
        neon_pump_imm.aabb,
    );
    spawn_camera(&mut commands, &neon_pump_imm.aabb);

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

fn start_loading_molecule(mut commands: Commands, mut events: MessageReader<LoadMolecule>) {
    if let Some(event) = events.read().last() {
        let name = event.name.clone();
        let content = event.content.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            parse_pdb_content(&content).map_err(|e| format!("Failed to parse {}: {e}", name))
        });
        commands.insert_resource(PendingMolecule(task));
    }
}

fn finish_loading_molecule(
    mut commands: Commands,
    pending: Option<ResMut<PendingMolecule>>,
    molecule_query: Query<Entity, (With<Molecule>, With<OnCadView>)>,
    camera_query: Query<Entity, (With<CadCamera>, With<OnCadView>)>,
) {
    let Some(mut pending) = pending else { return };

    let result = match block_on(poll_once(&mut pending.0)) {
        Some(result) => result,
        None => return, // Still loading
    };

    commands.remove_resource::<PendingMolecule>();

    let pdb_asset = match result {
        Ok(asset) => asset,
        Err(e) => {
            error!("{e}");
            return;
        }
    };

    // Despawn existing molecule and camera
    for entity in molecule_query.iter() {
        commands.entity(entity).despawn();
    }
    for entity in camera_query.iter() {
        commands.entity(entity).despawn();
    }

    spawn_molecule(&mut commands, pdb_asset.molecule, pdb_asset.aabb);
    spawn_camera(&mut commands, &pdb_asset.aabb);
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
    // Cancel any in-flight background load so it doesn't complete after re-entering CadView.
    commands.remove_resource::<PendingMolecule>();
}

// End of File
