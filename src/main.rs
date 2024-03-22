// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// disable console on windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_config::*;
use atomcad::{platform::bevy::PlatformTweaks, AppPlugin, APP_NAME};
use bevy::asset::AssetMetaCheck;
use bevy::{
    prelude::*,
    window::{PresentMode, PrimaryWindow, WindowMode, WindowResolution},
    winit::{WinitSettings, WinitWindows},
    DefaultPlugins,
    log::LogPlugin,
};
use bevy_egui::EguiPlugin;
use std::io::Cursor;
use winit::window::Icon;

fn main() {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let config_dir =
        if let Some(project_dirs) = directories::ProjectDirs::from("org", "atomcad", "atomCAD") {
            project_dirs.config_dir().to_owned()
        } else {
            std::path::PathBuf::from(".")
        };

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let settings_db = config_dir.join("settings.sqlite3");

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let app_config = AppConfig::load_from_sqlite(settings_db);

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let app_config = AppConfig::default();

    #[cfg(debug_assertions)]
    let default_plugins = DefaultPlugins.set(LogPlugin {
        filter: format!("warn,{}=trace", env!("CARGO_PKG_NAME")).into(),
        ..Default::default() 
    });

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: APP_NAME.into(),
            resolution: if app_config.window_resolution.x < 0. || app_config.window_resolution.y < 0. {
                WindowResolution::default()
            } else {
                app_config.window_resolution.into()
            },
            position: if app_config.window_position.x < 0 && app_config.window_position.y < 0 {
                WindowPosition::Automatic
            } else {
                WindowPosition::At(app_config.window_position)
            },
            mode: if app_config.fullscreen {
                WindowMode::BorderlessFullscreen
            } else {
                WindowMode::Windowed
            },
            resize_constraints: WindowResizeConstraints {
                min_width: 640.,
                min_height: 480.,
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

    let mut app = App::new();

    app.insert_resource(WinitSettings::desktop_app())
        .insert_resource(Msaa::Off)
        .insert_resource(AssetMetaCheck::Never)
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins(default_plugins)
        .add_plugins(PlatformTweaks)
        .add_plugins(EguiPlugin)
        .add_plugins(AppPlugin)
        .add_systems(Startup, set_window_icon)
        .add_systems(Startup, set_window_maximized);

    debug!("Config PATH {:?}", config_dir);
    debug!("Loaded {:?}", app_config);
            
    // Application settings are only persisted on desktop platforms.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let app = app
        .insert_resource(app_config)
        .add_systems(Update, update_app_config)
        .add_systems(Last, save_app_config);
        
    app.run();
}

// set window to maximized based on the config
fn set_window_maximized(
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    app_config: ResMut<AppConfig>,
) {
    let primary_entity = primary_window.single();
    if let Some(primary) = windows.get_window(primary_entity) {
        if app_config.maximized {
            primary.set_maximized(true);
        }
    };
}

// Sets the icon on Windows and X11.  The icon on macOS is sourced from the
// enclosing bundle, and is set in the Info.plist file.  That would be highly
// platform-specific code, and handled prior to bevy startup, not here.
fn set_window_icon(
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    let primary_entity = primary_window.single();
    let Some(primary) = windows.get_window(primary_entity) else {
        return;
    };
    let icon_buf = Cursor::new(include_bytes!(
        "../build/macos/AppIcon.iconset/icon_256x256.png"
    ));
    if let Ok(image) = image::load(icon_buf, image::ImageFormat::Png) {
        let image = image.into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        let icon = Icon::from_rgba(rgba, width, height).unwrap();
        primary.set_window_icon(Some(icon));
    };
}

// End of File
