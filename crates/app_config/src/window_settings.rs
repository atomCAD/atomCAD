use crate::{load_group, save_record_to_db, setting_value::SettingValue, AppConfig};

use serde::{Serialize, Deserialize};
use std::fmt;
use bevy::{
    prelude::*, 
    utils::HashMap,
};

#[derive(Clone, Serialize, Deserialize, Resource)]
pub struct WindowSettings {
    pub window_resolution_x: SettingValue,
    pub window_resolution_y: SettingValue,
    pub window_position_x: SettingValue,
    pub window_position_y: SettingValue,
    pub maximized: SettingValue,
    pub fullscreen: SettingValue,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            window_resolution_x: SettingValue::Float(-1.),
            window_resolution_y: SettingValue::Float(-1.),
            window_position_x: SettingValue::Int(0),
            window_position_y: SettingValue::Int(0),
            maximized: SettingValue::Bool(false),
            fullscreen: SettingValue::Bool(false),
        }
    }
}

impl fmt::Debug for WindowSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let window_resolution = match (&self.window_resolution_x, &self.window_resolution_y) {
            (SettingValue::Float(x), SettingValue::Float(y)) => format!("({}, {})", x, y),
            _ => "Invalid".to_string(),
        };

        let window_position = match (&self.window_position_x, &self.window_position_y) {
            (SettingValue::Int(x), SettingValue::Int(y)) => format!("({}, {})", x, y),
            _ => "Invalid".to_string(),
        };

        let maximized = match &self.maximized {
            SettingValue::Bool(value) => value.to_string(),
            _ => "Invalid".to_string(),
        };

        let fullscreen = match &self.fullscreen {
            SettingValue::Bool(value) => value.to_string(),
            _ => "Invalid".to_string(),
        };

        f.debug_struct("WindowSettings")
            .field("window_resolution", &window_resolution)
            .field("window_position", &window_position)
            .field("maximized", &maximized)
            .field("fullscreen", &fullscreen)
            .finish()
    }
}


impl WindowSettings {
    pub fn load_from_storage(app_config: &AppConfig) -> Self {
        let window_settings_group = load_group(app_config, "primary_window").unwrap_or_default();
        let default_settings = WindowSettings::default();

        Self {
            window_resolution_x: window_settings_group.get("resolution_x").cloned().unwrap_or(default_settings.window_resolution_x),
            window_resolution_y: window_settings_group.get("resolution_y").cloned().unwrap_or(default_settings.window_resolution_y),
            window_position_x: window_settings_group.get("position_x").cloned().unwrap_or(default_settings.window_position_x),
            window_position_y: window_settings_group.get("position_y").cloned().unwrap_or(default_settings.window_position_y),
            maximized: window_settings_group.get("maximized").cloned().unwrap_or(default_settings.maximized),
            fullscreen: window_settings_group.get("fullscreen").cloned().unwrap_or(default_settings.fullscreen),
        }
    }

    pub fn save_to_storage(&self, app_config: &AppConfig) -> Result<(), String> {
        let mut settings = HashMap::new();
        settings.insert("resolution_x", self.window_resolution_x.clone());
        settings.insert("resolution_y", self.window_resolution_y.clone());
        settings.insert("position_x", self.window_position_x.clone());
        settings.insert("position_y", self.window_position_y.clone());
        settings.insert("maximized", self.maximized.clone());
        settings.insert("fullscreen", self.fullscreen.clone());

        for (key, value) in settings {
            save_record_to_db(app_config, "primary_window", key, &value)
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }
}


#[derive(Resource, Debug, Clone)]
pub struct WindowMaximized(pub bool);