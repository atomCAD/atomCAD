// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{APP_NAME, AppPlugin, PlatformTweaks};
use bevy::{prelude::*, window::PresentMode, winit::WinitSettings};
use event_loop_waker::{EventLoopWakerPlugin, setup_ctrlc_handler};

pub fn start() -> AppExit {
    setup_ctrlc_handler();

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: APP_NAME.into(),
            present_mode: PresentMode::AutoNoVsync,
            canvas: Some("#bevy".to_owned()), // For web; no effect elewhere.
            prevent_default_event_handling: true, // Capture browser hotkeys.
            ..default()
        }),
        ..default()
    };

    let default_plugins = DefaultPlugins.set(window_plugin);

    App::new()
        .insert_resource(WinitSettings::desktop_app())
        .add_plugins(default_plugins)
        .add_plugins(PlatformTweaks)
        .add_plugins(EventLoopWakerPlugin)
        .add_plugins(AppPlugin)
        .run()
}

// End of File
