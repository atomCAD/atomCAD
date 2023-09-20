// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::type_complexity)]

mod actions;
mod audio;
mod camera;
mod loading;
mod menu;
pub mod menubar;
pub mod platform;
pub(crate) mod platform_impl;
mod scene;

use crate::actions::ActionsPlugin;
use crate::audio::InternalAudioPlugin;
use crate::loading::LoadingPlugin;
use crate::menu::MenuPlugin;
use crate::scene::ScenePlugin;

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
    // Here the menu is drawn and waiting for user interaction
    Menu,
}

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<AppState>().add_plugins((
            LoadingPlugin,
            MenuPlugin,
            ActionsPlugin,
            InternalAudioPlugin,
            ScenePlugin,
        ));
    }
}

// End of File
