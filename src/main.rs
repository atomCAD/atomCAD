// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// disable console on windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use atomcad::{menubar::setup_menu_bar, platform::bevy::PlatformTweaks, GamePlugin, APP_NAME};
use bevy::{
    prelude::*,
    window::{PresentMode, PrimaryWindow},
    winit::{WinitSettings, WinitWindows},
    DefaultPlugins,
};
use bevy_egui::EguiPlugin;
use std::io::Cursor;
use winit::window::Icon;

fn main() {
    App::new()
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: APP_NAME.into(),
                // FIXME: this should be read from a persistent settings file
                resolution: (800., 600.).into(),
                // Turn off vsync to maximize CPU/GPU usage and prevent the
                // application from blocking the main thread while waiting for
                // the screen refresh.  We may want to revisit this choice
                // later, but for now it simplifies development and testing.
                present_mode: PresentMode::AutoNoVsync,
                // Bind to canvas included in `index.html` on web
                canvas: Some("#bevy".to_owned()),
                // Tells wasm not to override default event handling,
                // like F5 and Ctrl+R
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PlatformTweaks)
        .add_plugins(EguiPlugin)
        .add_plugins(GamePlugin)
        .add_systems(Startup, set_window_icon)
        .add_systems(Startup, setup_menu_bar)
        .run();
}

// Sets the icon on windows and X11
fn set_window_icon(
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    let primary_entity = primary_window.single();
    let primary = windows.get_window(primary_entity).unwrap();
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
