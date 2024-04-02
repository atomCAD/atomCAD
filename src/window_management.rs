use crate::setting_value::SettingValue;
use crate::window_settings::WindowSettings;
use crate::AppConfig;

use bevy::{
    app::AppExit,
    prelude::*,
    window::{PrimaryWindow, WindowCloseRequested, WindowMoved, WindowResized},
    winit::WinitWindows,
};

use std::io::Cursor;

use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::window::{Fullscreen, Icon};

// set window initial settings on startup
pub fn apply_initial_window_settings(
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    window_settings: ResMut<WindowSettings>,
) {
    debug!("Initial {:?}", window_settings);

    let window_resolution = match (
        &window_settings.window_resolution_x,
        &window_settings.window_resolution_y,
    ) {
        (SettingValue::Float(x), SettingValue::Float(y)) if *x > 0.0 && *y > 0.0 => {
            Some(LogicalSize::new(*x as f64, *y as f64))
        }
        _ => None,
    };

    let window_position = match (
        &window_settings.window_position_x,
        &window_settings.window_position_y,
    ) {
        (SettingValue::Int(x), SettingValue::Int(y)) => {
            Some(PhysicalPosition::new(*x as f64, *y as f64))
        }
        _ => None,
    };

    let window_fullscreen = match &window_settings.fullscreen {
        SettingValue::Bool(fullscreen) => {
            if *fullscreen {
                Some(Fullscreen::Borderless(None))
            } else {
                None
            }
        }
        _ => None,
    };

    let window_maximized: bool = match &window_settings.maximized {
        SettingValue::Bool(maximized) => *maximized,
        _ => false,
    };

    let primary_entity = primary_window.single();
    if let Some(primary) = windows.get_window(primary_entity) {
        if let Some(position) = window_position {
            primary.set_outer_position(position);
        }

        if window_maximized {
            primary.set_maximized(true);
        } else if let Some(resolution) = window_resolution {
            primary.set_max_inner_size(Some(resolution));
        }

        primary.set_fullscreen(window_fullscreen);
    }
}

// Sets the icon on Windows and X11.  The icon on macOS is sourced from the
// enclosing bundle, and is set in the Info.plist file.  That would be highly
// platform-specific code, and handled prior to bevy startup, not here.
pub fn set_window_icon(
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
pub fn save_window_settings_on_exit(
    app_config: Res<AppConfig>,
    app_exit_events: EventReader<AppExit>,
    close_events: EventReader<WindowCloseRequested>,
    window_settings: ResMut<WindowSettings>,
    // Add any other resources or components you need to save settings
) {
    // Only run when the app is exiting.
    if app_exit_events.is_empty() && close_events.is_empty() {
        return;
    }

    debug!("Saving settings before exit...");
    let _ = window_settings.save_to_storage(&app_config);
}

pub fn update_window_settings(
    resize_events: EventReader<WindowResized>,
    move_events: EventReader<WindowMoved>,
    mut window_settings: ResMut<WindowSettings>,
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    let primary_entity = primary_window.single();
    if let Some(primary) = windows.get_window(primary_entity) {
        // handle resizing dependent settings
        if !resize_events.is_empty() {
            // Handle window resize event
            let scale_factor = primary.scale_factor() as f32;
            let window_resolution = primary.inner_size();
            window_settings.window_resolution_x =
                SettingValue::Float(window_resolution.width as f32 / scale_factor);
            window_settings.window_resolution_y =
                SettingValue::Float(window_resolution.height as f32 / scale_factor);

            // Check fullscreen state
            let is_fullscreen = primary.fullscreen().is_some();
            let current_fullscreen = match window_settings.fullscreen {
                SettingValue::Bool(value) => value,
                _ => false, // Default to false or handle appropriately if not a bool
            };

            if current_fullscreen != is_fullscreen {
                window_settings.fullscreen = SettingValue::Bool(is_fullscreen);
            }

            // check maximized state
            let is_maximized = primary.is_maximized();
            let current_maximized = match window_settings.maximized {
                SettingValue::Bool(value) => value,
                _ => false, // Default to false or handle appropriately if not a bool
            };

            if current_maximized != is_maximized {
                window_settings.maximized = SettingValue::Bool(is_maximized);
            }

            debug!("Updated {:?}", window_settings);
        }

        // handle window moving dependent settings
        if !move_events.is_empty() {
            // Handle window move event
            if let Ok(window_position) = primary.outer_position() {
                window_settings.window_position_x = SettingValue::Int(window_position.x);
                window_settings.window_position_y = SettingValue::Int(window_position.y);

                debug!("Updated {:?}", window_settings);
            }
        }
    }
}
