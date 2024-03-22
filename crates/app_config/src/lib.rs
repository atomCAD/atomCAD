use bevy::{
    prelude::*, 
    window::PrimaryWindow ,
    winit::WinitWindows,
};
use rusqlite::Result as SqliteResult;

#[derive(Resource, Debug)]
pub struct AppConfig {
    /// The primary key of the app_config table in the sqlite3 config database.
    #[allow(dead_code)]
    pub id: i32,
    /// The resolution of the primary window, as reported by windowing system.
    pub window_resolution: Vec2,
    /// The position of the top-left corner of the primary window, as reported
    /// by the windowing system.
    pub window_position: IVec2,
    /// Whether the primary window should be maximized.
    pub maximized: bool,
    /// Whether the primary window should be fullscreen.
    pub fullscreen: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            id: 1,
            window_resolution: (-1., -1.).into(),
            window_position: (-1, -1).into(),
            maximized: false,
            fullscreen: false,
        }
    }
}

impl AppConfig {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    pub fn load_from_sqlite<P>(path: P) -> Self
    where
        P: AsRef<std::path::Path>,
    {
        let defaults = Self::default();
        let result = || -> SqliteResult<AppConfig> {
            let conn = match rusqlite::Connection::open(&path) {
                Ok(conn) => conn,
                Err(err) => {
                    error!(
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
                    error!(
                        "Failed to prepare SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            let mut rows = match stmt.query(()) {
                Ok(rows) => rows,
                Err(err) => {
                    error!(
                        "Failed to execute SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            let Some(row) = rows.next()? else {
                warn!(
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
                maximized: row.get::<_, bool>(5).unwrap_or(defaults.maximized),
                fullscreen: row.get::<_, bool>(6).unwrap_or(defaults.fullscreen),
            })
        }();
        match result {
            Ok(config) => config,
            Err(err) => {
                error!("Failed to read AppConfig settings: {}", err);
                warn!("Using default AppConfig settings");
                defaults
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn save_to_sqlite<P>(&self, path: P)
    where
        P: AsRef<std::path::Path>,
    {
        let result = || -> rusqlite::Result<()> {
            let conn = rusqlite::Connection::open(path)?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS app_config (
                    id INTEGER PRIMARY KEY,
                    window_resolution_x REAL,
                    window_resolution_y REAL,
                    window_position_x INTEGER,
                    window_position_y INTEGER,
                    maximized BOOLEAN,
                    fullscreen BOOLEAN
                )",
                (),
            )?;
            const SQL: &str = "INSERT OR REPLACE INTO app_config (id, window_resolution_x, window_resolution_y, window_position_x, window_position_y, maximized, fullscreen) VALUES (?, ?, ?, ?, ?, ?, ?)";
            let mut stmt = match conn.prepare(SQL) {
                Ok(stmt) => stmt,
                Err(err) => {
                    error!(
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
                self.maximized as i32,
                self.fullscreen as i32
            ];

            match stmt.execute(params) {
                Ok(_) =>
                    // Log the compiled statement
                    if let Some(expanded_sql) = stmt.expanded_sql() {
                        debug!("Save Config SQL: {}", expanded_sql);
                    }
                ,
                Err(err) => {
                    error!(
                        "Failed to execute SQLite statement for app_config: \"{}\", {}",
                        SQL, err
                    );
                    return Err(err);
                }
            };
            Ok(())
        }();
        match result {
            Ok(_) => (),
            Err(err) => {
                error!("Failed to persist AppConfig settings: {}", err);
            }
        };
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn update_app_config(
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
            if window_position.x >= 0 && window_position.y >= 0 {
                app_config.window_position = (window_position.x, window_position.y).into();
            }
        };

        // Record maximized state of primary window.
        app_config.maximized = primary.is_maximized();

        // Record fullscreen state of primary window.
        app_config.fullscreen = primary.fullscreen().is_some();
    };
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn save_app_config(
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
            error!(
                "Failed to create config directory {}: {}",
                config_dir.display(),
                err
            );
            warn!("AppConfig will not be persisted.");
            return;
        }
    }

    // Save app config to sqlite3 database.
    app_config.save_to_sqlite(config_dir.join("settings.sqlite3"));
}