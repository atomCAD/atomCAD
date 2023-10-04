// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// disable console on windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use atomcad::{platform::bevy::PlatformTweaks, AppPlugin, APP_NAME};
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
        .insert_resource(WinitSettings::game())
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: APP_NAME.into(),
                // FIXME: this should be read from a persistent settings file
                resolution: (800., 600.).into(),
                resize_constraints: WindowResizeConstraints {
                    min_width: 640.,
                    min_height: 480.,
                    ..default()
                },
                // Turn on vsync to prevent maxing out the CPU/GPU, unless it
                // is actually needed to maintain an acceptable refresh rate.
                // This has the disadvantage of blocking the main thread while
                // waiting for the screen refresh, however, so we may want to
                // revisit this choice later.
                present_mode: PresentMode::AutoVsync,
                // Bind to canvas included in `index.html` on web (ignored otherwise)
                canvas: Some("#bevy".to_owned()),
                // Tells wasm not to override default event handling, like F5
                // and Ctrl+R.  Refreshing the page would potentially lose
                // work, and generally just waste system resources.  It's more
                // likey that the user would do this by accident, so let's
                // just block it off as a possibility.
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PlatformTweaks)
        .add_plugins(EguiPlugin)
        .add_plugins(AppPlugin)
        .add_systems(Startup, set_window_icon)
        .run();
}

// Sets the icon on Windows and X11.  The icon on macOS is sourced from the
// enclosing bundle, and is set in the Info.plist file.  That would be highly
// platform-specific code, and handled prior to bevy startup, not here.
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
