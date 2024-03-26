use bevy::{prelude::*, utils::HashMap};
use std::path::PathBuf;

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
                Ok(conn) => {
                    if let Err(err) = conn.execute(
                        "CREATE TABLE IF NOT EXISTS app_config_settings (
                            id INTEGER PRIMARY KEY,
                            group_name TEXT,
                            title TEXT,
                            name TEXT,
                            value TEXT,
                            value_type TEXT,
                            visible BOOLEAN,
                            description TEXT,
                            default_value TEXT
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

#[derive(Debug, Clone)]
pub struct SettingRecord {
    pub id: i32,
    pub group_name: String,
    pub title: String,
    pub name: String,
    pub value: String,
    pub value_type: String,
    pub visible: bool,
    pub description: String,
    pub default_value: String,
}

#[derive(Debug, Clone)]
pub enum SettingValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    // Add more types as needed
}

pub fn load_group(app_config: &AppConfig, group_name: &str) -> Result<HashMap<String, SettingValue>, String> {
    // Fetch the settings records from the database for the given group
    let settings_records = get_settings_records(app_config, group_name)
        .map_err(|e| e.to_string())?; // Convert the error to a String

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

fn get_settings_records(app_config: &AppConfig, group_name: &str) -> Result<Vec<SettingRecord>, rusqlite::Error> {
    let conn = match app_config.db_path.as_ref() {
        Some(path) => rusqlite::Connection::open(path)?,
        None => {
            let err_msg = "Abort loading, no database path set!";
            error!("{}", err_msg);
            return Err(rusqlite::Error::InvalidPath(err_msg.into()));
        },
    };

    let mut stmt = conn.prepare("SELECT * FROM app_config_settings WHERE group_name = ?")?;
    let mut rows = stmt.query(&[&group_name])?;

    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        records.push(SettingRecord {
            id: row.get(0).unwrap(),
            group_name: row.get(1).unwrap(),
            title: row.get(2).unwrap(),
            description: row.get(3).unwrap(),
            name: row.get(4).unwrap(),
            value: row.get(5).unwrap(),
            default_value: row.get(6).unwrap(),
            value_type: row.get(7).unwrap(),
            visible: row.get(8).unwrap(),
        });
    }

    Ok(records)
}

#[derive(Resource, Debug, Clone)]
pub struct AppConfig {
    pub db_path: Option<std::path::PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            db_path: None,
        }
    }
}

#[derive(Debug, Clone, Resource)]
pub struct WindowSettings {
    pub window_resolution_x: f32,
    pub window_resolution_y: f32,
    pub window_position_x: i32,
    pub window_position_y: i32,
    pub maximized: bool,
    pub fullscreen: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            window_resolution_x: -1.,
            window_resolution_y: -1.,
            window_position_x: -1,
            window_position_y: -1,
            maximized: false,
            fullscreen: false,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct WindowMaximized(pub bool);