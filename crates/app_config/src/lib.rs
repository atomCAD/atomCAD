pub mod setting_value;
pub mod window_settings;

use bevy::{prelude::*, utils::HashMap};

use setting_value::{SettingRecord, SettingValue};
use std::{
    fmt,
    path::{Path, PathBuf},
};

pub trait AppConfigTrait {
    fn set_db_path(&mut self);
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

        // create default DB table if it doesn't exist
        if let Some(path) = &self.db_path {
            match rusqlite::Connection::open(path) {
                #[allow(unused_mut)]
                Ok(mut conn) => {
                    #[cfg(feature = "sqlite-tracing")]
                    conn.trace(Some(|stmt| {
                        debug!("SQL: {:?}", stmt);
                    }));

                    if let Err(err) = conn.execute(
                        "CREATE TABLE IF NOT EXISTS app_config_settings (
                            group_name TEXT,
                            name TEXT,
                            title TEXT,
                            description TEXT,
                            value TEXT,
                            value_type TEXT,
                            default_value TEXT,
                            visible BOOLEAN DEFAULT FALSE,
                            CONSTRAINT pk_settings PRIMARY KEY (group_name, name)
                        )",
                        (),
                    ) {
                        error!("Failed to create default database table: {}", err);
                    }
                }
                Err(err) => {
                    error!("Failed to open database connection: {}", err);
                }
            }
        }
    }
}

pub fn load_group(
    app_config: &AppConfig,
    group_name: &str,
) -> Result<HashMap<String, SettingValue>, String> {
    // Fetch the settings records from the database for the given group
    let settings_records =
        get_settings_records(app_config, group_name).map_err(|e| e.to_string())?; // Convert the error to a String

    let mut settings = HashMap::new();
    for record in settings_records {
        let value = match record.value_type.as_str() {
            "bool" => SettingValue::Bool(record.value.parse::<bool>().map_err(|e| e.to_string())?),
            "int" => SettingValue::Int(record.value.parse::<i32>().map_err(|e| e.to_string())?),
            "float" => SettingValue::Float(record.value.parse::<f32>().map_err(|e| e.to_string())?),
            "string" => SettingValue::String(record.value),
            _ => return Err(format!("Unknown type for setting '{}'", record.name)),
        };
        settings.insert(record.name, value);
    }

    Ok(settings)
}

fn get_settings_records(
    app_config: &AppConfig,
    group_name: &str,
) -> Result<Vec<SettingRecord>, rusqlite::Error> {
    let conn = match app_config.db_path.as_ref() {
        Some(path) => {
            #[allow(unused_mut)]
            let mut conn = rusqlite::Connection::open(path)?;
            #[cfg(feature = "sqlite-tracing")]
            conn.trace(Some(|stmt| {
                debug!("SQL: {:?}", stmt);
            }));
            conn
        }
        None => {
            let err_msg = "Abort loading, no database path set!";
            error!("{}", err_msg);
            return Err(rusqlite::Error::InvalidPath(err_msg.into()));
        }
    };

    let mut stmt = conn.prepare(
        "SELECT group_name, name, title, description, value, value_type, default_value, visible FROM app_config_settings WHERE group_name = ?"
    )?;

    let mut rows = stmt.query([&group_name])?;

    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        records.push(SettingRecord {
            group_name: row.get(0).unwrap_or("NONE".to_string()),
            name: row.get(1).unwrap_or(String::new()),
            title: row.get(2).unwrap_or(String::new()),
            description: row.get(3).unwrap_or(String::new()),
            value: row.get(4).unwrap(),
            value_type: row.get(5).unwrap(),
            default_value: row.get(6).unwrap_or(String::new()),
            visible: row.get(7).unwrap_or(false),
        });
    }

    Ok(records)
}

pub fn save_record_to_db(
    app_config: &AppConfig,
    group_name: &str,
    key: &str,
    value: &SettingValue,
) -> Result<(), rusqlite::Error> {
    let conn = match app_config.db_path.as_ref() {
        Some(path) => {
            #[allow(unused_mut)]
            let mut conn = rusqlite::Connection::open(path)?;
            #[cfg(feature = "sqlite-tracing")]
            conn.trace(Some(|stmt| {
                debug!("SQL: {:?}", stmt);
            }));
            conn
        }
        None => {
            let err_msg = "Abort loading, no database path set!";
            error!("{}", err_msg);
            return Err(rusqlite::Error::InvalidPath(err_msg.into()));
        }
    };

    let mut stmt = conn.prepare("INSERT OR REPLACE INTO app_config_settings (group_name, name, value, value_type) VALUES (?1, ?2, ?3, ?4)")?;
    stmt.execute(rusqlite::params![
        group_name,
        key,
        value.to_string(),
        value.type_as_string()
    ])?;

    Ok(())
}

#[derive(Resource, Clone, Default)]
pub struct AppConfig {
    pub db_path: Option<std::path::PathBuf>,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let db_path = self
            .db_path
            .as_ref()
            .map(|path| Path::new(path).to_string_lossy());

        match &db_path {
            Some(filename) => f
                .debug_struct("AppConfig")
                .field("db_path", filename)
                .finish(),
            None => f
                .debug_struct("AppConfig")
                .field("db_path", &"None")
                .finish(),
        }
    }
}
