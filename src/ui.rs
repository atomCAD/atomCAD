// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::loading::FontAssets;
use crate::AppState;
use bevy::prelude::*;

pub struct SplashScreenPlugin;

/// This plugin is responsible for the app "Get Started" splash screen (containing only one button...)
/// The splash screen is only drawn during the State `AppState::SplashScreen` and is removed when that state is exited
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
            normal: Color::rgb(0.15, 0.15, 0.15),
            hovered: Color::rgb(0.0, 0.655, 1.0),
        }
    }
}

// Tag component used to tag entities added on the splash screen
#[derive(Component)]
struct OnSplashScreen;

fn setup_splash_screen(
    mut commands: Commands,
    font_assets: Res<FontAssets>,
    button_colors: Res<ButtonColors>,
) {
    commands.spawn((Camera2dBundle::default(), OnSplashScreen));
    commands
        .spawn((
            NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ..default()
            },
            OnSplashScreen,
        ))
        .with_children(|parent| {
            parent.spawn(
                TextBundle::from_section(
                    "Shape tomorrow's world",
                    TextStyle {
                        font: font_assets.fira_sans.clone(),
                        font_size: 64.0,
                        color: Color::WHITE,
                    },
                )
                .with_style(Style {
                    margin: UiRect::all(Val::Auto),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                }),
            );
            parent
                .spawn(ButtonBundle {
                    style: Style {
                        width: Val::Px(250.0),
                        height: Val::Px(50.0),
                        margin: UiRect::all(Val::Auto),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    background_color: button_colors.normal.into(),
                    ..Default::default()
                })
                .with_children(|parent| {
                    parent.spawn(TextBundle::from_section(
                        "Get Started",
                        TextStyle {
                            font: font_assets.fira_sans.clone(),
                            font_size: 20.0,
                            color: Color::WHITE,
                        },
                    ));
                });
        });
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
                state.set(AppState::Active);
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

fn cleanup_splash_screen(mut commands: Commands, entities: Query<Entity, With<OnSplashScreen>>) {
    for entity in entities.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

// End of File
