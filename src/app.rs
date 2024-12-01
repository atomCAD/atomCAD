// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::{ecs::system::NonSendMarker, prelude::*};

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, set_window_icon);
    }
}

/// Sets the icon on Windows and X11.  The icon on macOS is sourced from the enclosing bundle, and
/// is set in the Info.plist file.  That would be highly platform-specific code, and handled prior
/// to bevy startup, not here.
#[cfg(not(target_os = "macos"))]
use bevy::window::PrimaryWindow;
#[cfg(not(target_os = "macos"))]
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
    });
}

#[cfg(target_os = "macos")]
pub fn set_window_icon(_non_send_marker: NonSendMarker) {
    static ICON_DATA: &[u8] = include_bytes!(env!("ATOMCAD_ICNS_PATH"));
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};
    use std::{os::raw::c_void, ptr::NonNull};
    let data = unsafe {
        NSData::dataWithBytesNoCopy_length(
            NonNull::new_unchecked(ICON_DATA.as_ptr() as *mut c_void),
            ICON_DATA.len(),
        )
    };
    let image = NSImage::alloc();
    let image = NSImage::initWithData(image, &data).expect("Failed to create NSImage from data.");
    let mtm = MainThreadMarker::new().expect("Must run on main thread.");
    let app = NSApplication::sharedApplication(mtm);
    unsafe {
        app.setApplicationIconImage(Some(&image));
    }
}

// End of File
