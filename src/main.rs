// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// disable console on windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use atomcad::{platform::bevy::PlatformTweaks, AppPlugin, APP_NAME};
use bevy::{
    prelude::*,
    window::{PresentMode, PrimaryWindow, WindowMode, WindowResolution},
    winit::{WinitSettings, WinitWindows},
    DefaultPlugins,
};
use bevy_egui::EguiPlugin;
use std::io::Cursor;
use winit::window::Icon;

#[derive(Resource)]
struct AppConfig {
    /// The primary key of the app_config table in the sqlite3 config database.
    #[allow(dead_code)]
    id: i32,
    /// The resolution of the primary window, as reported by windowing system.
    window_resolution: Vec2,
    /// The position of the top-left corner of the primary window, as reported
    /// by the windowing system.
    window_position: IVec2,
    /// Whether the primary window should be fullscreen.
    fullscreen: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            id: 1,
            window_resolution: (-1., -1.).into(),
            window_position: (-1, -1).into(),
            fullscreen: false,
        }
    }
}

impl AppConfig {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn load_from_sqlite<P>(path: P) -> Self
    where
        P: AsRef<std::path::Path>,
    {
        let defaults = Self::default();
        match || -> rusqlite::Result<AppConfig> {
            let conn = match rusqlite::Connection::open(&path) {
                Ok(conn) => conn,
                Err(err) => {
                    info!(
                        "Failed to open SQLite database {}: {}",
                        path.as_ref().display(),
                        err
                    );
                    return Err(err);
                }
            };
            const SQL: &str = "SELECT * FROM app_config LIMIT 1";
            let mut stmt = match conn.prepare(SQL) {
                Ok(stmt) => stmt,
                Err(err) => {
                    info!(
                        "Failed to prepare SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            let mut rows = match stmt.query(()) {
                Ok(rows) => rows,
                Err(err) => {
                    info!(
                        "Failed to execute SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            let Some(row) = rows.next()? else {
                info!(
                    "No rows returned from SQLite query for app_config: \"{}\"",
                    SQL
                );
                return Err(rusqlite::Error::QueryReturnedNoRows);
            };
            Ok(Self {
                id: row.get::<_, i32>(0).unwrap_or(defaults.id),
                window_resolution: (
                    row.get::<_, f32>(1).unwrap_or(defaults.window_resolution.x),
                    row.get::<_, f32>(2).unwrap_or(defaults.window_resolution.y),
                )
                    .into(),
                window_position: (
                    row.get::<_, i32>(3).unwrap_or(defaults.window_position.x),
                    row.get::<_, i32>(4).unwrap_or(defaults.window_position.y),
                )
                    .into(),
                fullscreen: row.get::<_, bool>(5).unwrap_or(defaults.fullscreen),
            })
        }() {
            Ok(config) => config,
            Err(err) => {
                info!("Failed to read AppConfig settings: {}", err);
                info!("Using default AppConfig settings");
                defaults
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn save_to_sqlite<P>(&self, path: P)
    where
        P: AsRef<std::path::Path>,
    {
        match || -> rusqlite::Result<()> {
            let conn = rusqlite::Connection::open(path)?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS app_config (
                    id INTEGER PRIMARY KEY,
                    window_resolution_x REAL,
                    window_resolution_y REAL,
                    window_position_x INTEGER,
                    window_position_y INTEGER,
                    fullscreen INTEGER
                )",
                (),
            )?;
            const SQL: &str = "INSERT OR REPLACE INTO app_config (id, window_resolution_x, window_resolution_y, window_position_x, window_position_y, fullscreen) VALUES (?, ?, ?, ?, ?, ?)";
            let mut stmt = match conn.prepare(SQL) {
                Ok(stmt) => stmt,
                Err(err) => {
                    info!(
                        "Failed to prepare SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            let params = rusqlite::params![
                self.id,
                self.window_resolution.x,
                self.window_resolution.y,
                self.window_position.x,
                self.window_position.y,
                self.fullscreen
            ];
            match stmt.execute(params) {
                Ok(_) => (),
                Err(err) => {
                    info!(
                        "Failed to execute SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            Ok(())
        }() {
            Ok(_) => (),
            Err(err) => {
                info!("Failed to persist AppConfig settings: {}", err);
            }
        };
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn update_app_config(
    mut app_config: ResMut<AppConfig>,
    windows: NonSend<WinitWindows>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    let primary_entity = primary_window.single();
    if let Some(primary) = windows.get_window(primary_entity) {
        // Record resolution of primary window.
        let scale_factor = primary.scale_factor() as f32;
        let window_resolution = primary.inner_size();
        if window_resolution.width > 0 && window_resolution.height > 0 {
            app_config.window_resolution = (
                (window_resolution.width as f32) / scale_factor,
                (window_resolution.height as f32) / scale_factor,
            )
                .into();
        };

        // Record position of primary window.
        if let Ok(window_position) = primary.outer_position() {
            if window_position.x > 0 || window_position.y > 0 {
                app_config.window_position = (window_position.x, window_position.y).into();
            }
        };

        // Record fullscreen state of primary window.
        app_config.fullscreen = primary.fullscreen().is_some();
    };
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn save_app_config(
    app_config: ResMut<AppConfig>,
    app_exit_events: EventReader<bevy::app::AppExit>,
) {
    // Only run when the app is exiting.
    if app_exit_events.is_empty() {
        return;
    }

    // Create config directory if it doesn't exist.
    let config_dir =
        if let Some(project_dirs) = directories::ProjectDirs::from("org", "atomcad", "atomCAD") {
            project_dirs.config_dir().to_owned()
        } else {
            std::path::PathBuf::from(".")
        };
    if !config_dir.exists() {
        if let Err(err) = std::fs::create_dir_all(&config_dir) {
            info!(
                "Failed to create config directory {}: {}",
                config_dir.display(),
                err
            );
            info!("AppConfig will not be persisted.");
            return;
        }
    }

    // Save app config to sqlite3 database.
    app_config.save_to_sqlite(config_dir.join("settings.sqlite3"));
}

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

    let mut app = App::new();

    // Platform-independent setup code.
    app.insert_resource(WinitSettings::game())
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: APP_NAME.into(),
                resolution: if app_config.window_resolution.x < 0.
                    || app_config.window_resolution.y < 0.
                {
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
        .add_systems(Startup, set_window_icon);

    // Application settings are only persisted on desktop platforms.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    let app = app
        .insert_resource(app_config)
        .add_systems(Update, update_app_config)
        .add_systems(Last, save_app_config);

    app.run();
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
