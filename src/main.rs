// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// disable console on windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_config::*;
use atomcad::{platform::bevy::PlatformTweaks, AppPlugin, APP_NAME};
use bevy::asset::AssetMetaCheck;
use bevy::{
    app::AppExit,
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
    let mut app_config = AppConfig::default();

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    app_config.set_db_path();
    let window_settings_group = match load_group(&app_config, "primary_window") {
        Ok(settings) => settings,
        Err(e) => {
            let window_defaults = WindowSettings::default();
            error!("Failed to load window settings: {}", e);
            vec![
                (String::from("resolution_x"), SettingValue::Float(window_defaults.window_resolution_x)),
                (String::from("resolution_y"), SettingValue::Float(window_defaults.window_resolution_y)),
                (String::from("position_x"), SettingValue::Int(window_defaults.window_position_x)),
                (String::from("position_y"), SettingValue::Int(window_defaults.window_position_y)),
                (String::from("maximized"), SettingValue::Bool(window_defaults.maximized)),
                (String::from("fullscreen"), SettingValue::Bool(window_defaults.fullscreen)),
            ]
            .into_iter()
            .collect()
        }
    };

    #[cfg(debug_assertions)]
    let default_plugins = DefaultPlugins.set(LogPlugin {
        filter: format!("warn,{}=trace,app_config=trace", env!("CARGO_PKG_NAME")).into(),
        ..Default::default() 
    });

    let window_resolution = match (
        window_settings_group.get("resolution_x"),
        window_settings_group.get("resolution_y"),
    ) {
        (Some(SettingValue::Float(x)), Some(SettingValue::Float(y))) if *x >= 0.0 && *y >= 0.0 => {
            WindowResolution::new(*x, *y)
        }
        _ => WindowResolution::default(),
    };

    let window_position = match (
        window_settings_group.get("position_x"),
        window_settings_group.get("position_y"),
    ) {
        (Some(SettingValue::Int(x)), Some(SettingValue::Int(y))) => WindowPosition::At((*x, *y).into()),
        _ => WindowPosition::Automatic,
    };

    let window_fullscreen: bool = match window_settings_group.get("fullscreen") {
        Some(SettingValue::Bool(fullscreen)) => *fullscreen,
        _ => false,
    };

    let window_maximized: bool = match window_settings_group.get("maximized") {
        Some(SettingValue::Bool(maximized)) => *maximized,
        _ => false,
    };

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: APP_NAME.into(),
            resolution: window_resolution,
            position: window_position,
            mode: if window_fullscreen {
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
        .insert_resource(WindowMaximized(window_maximized))
        .add_plugins(default_plugins)
        .add_plugins(PlatformTweaks)
        .add_plugins(EguiPlugin)
        .add_plugins(AppPlugin)
        .add_systems(Startup, (set_window_icon, set_window_maximized))
        .add_event::<AppExit>();

    debug!("Loaded {:?}", &app_config);
            
    // Application settings are only persisted on desktop platforms.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let app = app
        .insert_resource(app_config)
        .add_systems(Last, save_settings_on_exit);
        // .add_systems(Update, update_app_config)
        // .add_systems(Last, save_app_config);
        
    app.run();
}


// set window to maximized based on the config
fn set_window_maximized(
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    window_maximized: Res<WindowMaximized>,
) {
    let primary_entity = primary_window.single();
    if let Some(primary) = windows.get_window(primary_entity) {
        if window_maximized.0 {
            primary.set_maximized(true);
        }
    }
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn save_settings_on_exit(
    mut app_exit_events: EventReader<AppExit>,
    // Add any other resources or components you need to save settings
) {
    if app_exit_events.read().next().is_some() {
        debug!("Saving settings before exit...");
    }
}


// #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
// fn update_app_config(
//     mut app_config: ResMut<AppConfig>,
//     windows: NonSend<WinitWindows>,
//     primary_window: Query<Entity, With<PrimaryWindow>>,
// ) {
//     let primary_entity = primary_window.single();
//     if let Some(primary) = windows.get_window(primary_entity) {
//         // Record resolution of primary window.
//         let scale_factor = primary.scale_factor() as f32;
//         let window_resolution = primary.inner_size();
//         if window_resolution.width > 0 && window_resolution.height > 0 {
//             app_config.window_resolution = (
//                 (window_resolution.width as f32) / scale_factor,
//                 (window_resolution.height as f32) / scale_factor,
//             )
//                 .into();
//         };

//         // Record position of primary window.
//         if let Ok(window_position) = primary.outer_position() {
//             if window_position.x >= 0 && window_position.y >= 0 {
//                 app_config.window_position = (window_position.x, window_position.y).into();
//             }
//         };

//         // Record maximized state of primary window.
//         app_config.maximized = primary.is_maximized();

//         // Record fullscreen state of primary window.
//         app_config.fullscreen = primary.fullscreen().is_some();
//     };
// }

// #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
// fn save_app_config(
//     app_config: ResMut<AppConfig>,
//     app_exit_events: EventReader<bevy::app::AppExit>,
// ) {
//     // Only run when the app is exiting.
//     if app_exit_events.is_empty() {
//         return;
//     }

//     if let Err(e) = app_config.save() {
//         error!("Failed to save app configuration: {}", e);
//         return;
//     } else {
//         debug!("Saved {:?}", app_config);
//     }
// }

// // End of File
