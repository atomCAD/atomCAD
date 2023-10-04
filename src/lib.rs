// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Bevy uses some very complex types for specifying system inputs.
// There's just no getting around this, so silence clippy's complaints.
#![allow(clippy::type_complexity)]

mod actions;
mod camera;
mod loading;
pub mod menubar;
pub mod platform;
pub(crate) mod platform_impl;
mod scene;
mod ui;

use crate::actions::ActionsPlugin;
use crate::loading::LoadingPlugin;
use crate::menubar::MenuBarPlugin;
use crate::scene::ScenePlugin;
use crate::ui::SplashScreenPlugin;

use bevy::app::App;
use bevy::prelude::*;

pub const APP_NAME: &str = "atomCAD";

// We use States to separate logic
// See https://bevy-cheatbook.github.io/programming/states.html
// Or https://github.com/bevyengine/bevy/blob/main/examples/ecs/state.rs
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
enum AppState {
    // During the loading State the LoadingPlugin will load our assets
    #[default]
    Loading,
    // During this State the scene graph is rendered and the user can interact
    // with the camera.
    Active,
    // Here the "Get Started" prompt is drawn and waiting for user interaction
    SplashScreen,
}

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<AppState>().add_plugins((
            ActionsPlugin,
            LoadingPlugin,
            MenuBarPlugin,
            ScenePlugin,
            SplashScreenPlugin,
        ));
    }
}

// End of File
