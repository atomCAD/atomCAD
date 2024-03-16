use bevy::prelude::*;
use rusqlite::Result as SqliteResult;
use std::path::PathBuf;

pub trait AppConfigTrait {
    fn set_db_path(&mut self);
    fn load(&mut self) -> SqliteResult<()>;
    fn save(&self) -> SqliteResult<()>;
}

impl AppConfigTrait for AppConfig {
    fn set_db_path(&mut self) {
        let config_dir = directories::ProjectDirs::from("org", "atomcad", "atomCAD")
            .map(|dirs| dirs.config_dir().to_owned())
            .unwrap_or_else(|| PathBuf::from("."));
        self.db_path = Some(config_dir.join("settings.sqlite3"));

        // Create config directory if it doesn't exist.
        if !config_dir.exists() {
            if let Err(err) = std::fs::create_dir_all(&config_dir) {
                // reset the db_path to None if the directory creation fails
                self.db_path = None;
                error!(
                    "Failed to create config directory {}: {}",
                    config_dir.display(),
                    err
                );
                warn!("AppConfig will not be persisted as no storage can be created.");
                return;
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn load(&mut self) -> SqliteResult<()> {
        AppConfig::load_from_sqlite(self)
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn load(&mut self) -> SqliteResult<()> {
        *self = AppConfig::default();
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn save(&self) -> SqliteResult<()> {
        AppConfig::save_to_sqlite(self)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn save(&self) -> SqliteResult<()> {
        Ok(())
    }
}

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
    /// sqlite connection path
    pub db_path: Option<std::path::PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            id: 1,
            window_resolution: (-1., -1.).into(),
            window_position: (-1, -1).into(),
            maximized: false,
            fullscreen: false,
            db_path: None,
        }
    }
}

impl AppConfig {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn load_from_sqlite(&mut self) -> SqliteResult<()> {
        let conn = match self.db_path.as_ref() {
            Some(path) => rusqlite::Connection::open(path),
            None => {
                let err_msg = "Abort loading, no database path set!";
                error!("{}", err_msg);
                return Err(rusqlite::Error::InvalidPath(err_msg.into()));
            },
        }?;

        const SQL: &str = "SELECT * FROM app_config LIMIT 1";
        let mut stmt = conn.prepare(SQL)?;
        let mut rows = stmt.query(())?;

        if let Some(row) = rows.next()? {
            self.id = row.get(0)?;
            self.window_resolution = (row.get(1)?, row.get(2)?).into();
            self.window_position = (row.get(3)?, row.get(4)?).into();
            self.maximized = row.get(5)?;
            self.fullscreen = row.get(6)?;
        } else {
            warn!("No rows returned from SQLite query for app_config: \"{}\"", SQL);
            *self = Self::default();
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn save_to_sqlite(&self) -> rusqlite::Result<()> {
        let conn = match self.db_path.as_ref() {
            Some(path) => rusqlite::Connection::open(path),
            None => {
                let err_msg = "Abort saving, no database path set!";
                error!("{}", err_msg);
                return Err(rusqlite::Error::InvalidPath(err_msg.into()));
            },
        }?;

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
        let params = rusqlite::params![
            self.id,
            self.window_resolution.x,
            self.window_resolution.y,
            self.window_position.x,
            self.window_position.y,
            self.maximized as i32,
            self.fullscreen as i32
        ];

        conn.execute(SQL, params)?;

        Ok(())
    }
}
