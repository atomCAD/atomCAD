// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{APP_NAME, AppPlugin, PlatformTweaks};
use bevy::prelude::*;

pub fn start() -> AppExit {
    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: APP_NAME.into(),
            canvas: Some("#bevy".to_owned()), // For web; no effect elewhere.
            ..default()
        }),
        ..default()
    };

    let default_plugins = DefaultPlugins.set(window_plugin);

    App::new()
        .add_plugins(default_plugins)
        .add_plugins(PlatformTweaks)
        .add_plugins(AppPlugin)
        .run()
}

// End of File
