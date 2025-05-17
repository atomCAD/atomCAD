// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{AppState, FontAssetHandles};
use bevy::{app::App, prelude::*};

pub struct SplashScreenPlugin;

impl Plugin for SplashScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ButtonColors>()
            .add_systems(OnEnter(AppState::SplashScreen), setup_splash_screen)
            .add_systems(
                Update,
                click_get_started.run_if(in_state(AppState::SplashScreen)),
            )
            .add_systems(OnExit(AppState::SplashScreen), cleanup_splash_screen);
    }
}

#[derive(Resource)]
struct ButtonColors {
    normal: Color,
    hovered: Color,
}

impl Default for ButtonColors {
    fn default() -> Self {
        ButtonColors {
            normal: Color::srgb(0.15, 0.15, 0.15),
            hovered: Color::srgb(0.0, 0.655, 1.0),
        }
    }
}

// Tag component used to tag entities added on in CAD view.
#[derive(Component)]
struct OnSplashScreen;

fn setup_splash_screen(
    mut commands: Commands,
    font_asset_handles: Res<FontAssetHandles>,
    button_colors: Res<ButtonColors>,
) {
    commands.spawn((
        Camera2d,
        Camera {
            // This is the same as the default clear color, which matches the dark gray color on
            // Bevy's website, but let's make it explicit in case Bevy ever changes its arbitrary
            // defaults.
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        OnSplashScreen,
    ));
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            OnSplashScreen,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Shape tomorrow's world"),
                TextFont {
                    font: font_asset_handles.fira_sans_bold.clone(),
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    margin: UiRect::all(Val::Auto),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(250.0),
                        height: Val::Px(50.0),
                        margin: UiRect::all(Val::Auto),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(button_colors.normal),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Get Started"),
                        TextFont {
                            font: font_asset_handles.fira_sans_bold.clone(),
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn cleanup_splash_screen(mut commands: Commands, entities: Query<Entity, With<OnSplashScreen>>) {
    for entity in entities.iter() {
        commands.entity(entity).despawn();
    }
}

fn click_get_started(
    button_colors: Res<ButtonColors>,
    mut state: ResMut<NextState<AppState>>,
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                state.set(AppState::CadView);
            }
            Interaction::Hovered => {
                *color = button_colors.hovered.into();
            }
            Interaction::None => {
                *color = button_colors.normal.into();
            }
        }
    }
}

// End of File
