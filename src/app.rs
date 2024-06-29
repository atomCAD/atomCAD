// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::{ecs::system::NonSendMarker, prelude::*, window::PrimaryWindow};

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, set_window_icon);
    }
}

/// Sets the icon on Windows and X11.  The icon on macOS is sourced from the enclosing bundle, and
/// is set in the Info.plist file.  That would be highly platform-specific code, and handled prior
/// to bevy startup, not here.
pub fn set_window_icon(
    _non_send_marker: NonSendMarker,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    use bevy::winit::WINIT_WINDOWS;
    // with_borrow_mut() will panic if WINIT_WINDOWS is borrowed elsewhere,
    // but all borrowings should be in the main thread,
    // and we must run in the main thread because of NonSendMarker.
    // So this should be safe.
    WINIT_WINDOWS.with_borrow_mut(|windows| {
        use std::io::Cursor;
        use winit::window::Icon;
        let primary_entity = match primary_window.single() {
            Ok(primary_entity) => primary_entity,
            Err(_) => return,
        };
        let primary = match windows.get_window(primary_entity) {
            Some(primary) => primary,
            None => return,
        };
        let icon_buf = Cursor::new(include_bytes!(env!("ATOMCAD_ICON_PATH")));
        if let Ok(image) = image::load(icon_buf, image::ImageFormat::Png) {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            let icon = Icon::from_rgba(rgba, width, height).unwrap();
            primary.set_window_icon(Some(icon));
        };
    })
}

// End of File
