// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// disable console on windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_config::*;
use atomcad::{platform::bevy::PlatformTweaks, AppPlugin, APP_NAME};
use bevy::asset::AssetMetaCheck;
use bevy::{
    app::AppExit, log::LogPlugin, prelude::*, window::PresentMode, winit::WinitSettings,
    DefaultPlugins,
};
use bevy_egui::EguiPlugin;

mod window_management;
use window_management::{apply_initial_window_settings, set_window_icon, update_window_settings};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use window_management::save_window_settings_on_exit;

use window_settings::WindowSettings;

fn main() {
    #[allow(unused_mut)]
    let mut app_config = AppConfig::default();
    let mut app = App::new();

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    app_config.set_db_path();

    let default_plugins = DefaultPlugins;

    let default_plugins = default_plugins.set(AssetPlugin {
        meta_check: AssetMetaCheck::Never,
        ..default()
    });

    #[cfg(debug_assertions)]
    let default_plugins = default_plugins.set(LogPlugin {
        filter: format!("warn,{}=trace,app_config=trace", env!("CARGO_PKG_NAME")),
        ..Default::default()
    });

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: APP_NAME.into(),
            resize_constraints: WindowResizeConstraints {
                min_width: WindowSettings::MIN_WIDTH,
                min_height: WindowSettings::MIN_HEIGHT,
                ..default()
            },
            present_mode: PresentMode::AutoNoVsync,
            canvas: Some("#bevy".to_owned()),
            prevent_default_event_handling: false,
            ..default()
        }),
        ..default()
    };

    let default_plugins = default_plugins.set(window_plugin);
    app.add_plugins(default_plugins);

    debug!("Loaded {:?}", &app_config);

    // Application settings are only persisted on desktop platforms.
    let window_settings = WindowSettings::load(&app_config);

    app.insert_resource(WinitSettings::desktop_app())
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(app_config)
        .insert_resource(window_settings)
        .add_plugins(PlatformTweaks)
        .add_plugins(EguiPlugin)
        .add_plugins(AppPlugin)
        .add_systems(Startup, set_window_icon)
        .add_systems(Startup, apply_initial_window_settings)
        .add_systems(Update, update_window_settings)
        .add_event::<AppExit>();

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let app = app.add_systems(Last, save_window_settings_on_exit);

    app.run();
}
