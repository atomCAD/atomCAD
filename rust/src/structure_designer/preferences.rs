//! Persistence functions for user preferences.
//!
//! This module handles loading and saving preferences to the user's config directory.
//! Preferences are stored in `<config_dir>/atomCAD/preferences.json`.
//!
//! # Platform-Specific Locations
//!
//! - **Windows:** `%APPDATA%\atomCAD\preferences.json`
//! - **macOS:** `~/Library/Application Support/atomCAD/preferences.json`
//! - **Linux:** `~/.config/atomCAD/preferences.json`

use std::fs;
use std::path::PathBuf;
use crate::api::structure_designer::structure_designer_preferences::StructureDesignerPreferences;

const CONFIG_DIR_NAME: &str = "atomCAD";
const PREFERENCES_FILE_NAME: &str = "preferences.json";

/// Returns the path to the preferences file, or None if the config directory cannot be determined.
pub fn get_preferences_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(config_dir.join(CONFIG_DIR_NAME).join(PREFERENCES_FILE_NAME))
}

/// Loads preferences from the user's config directory.
///
/// Returns the loaded preferences, or defaults if:
/// - The config directory cannot be determined
/// - The preferences file doesn't exist (first run)
/// - The file is corrupted or invalid JSON
///
/// This function never fails - it always returns usable preferences.
pub fn load_preferences() -> StructureDesignerPreferences {
    let Some(path) = get_preferences_path() else {
        eprintln!("[preferences] Could not determine config directory, using defaults");
        return StructureDesignerPreferences::default();
    };

    if !path.exists() {
        // First run or file was deleted - silently use defaults
        return StructureDesignerPreferences::default();
    }

    match fs::read_to_string(&path) {
        Ok(contents) => {
            match serde_json::from_str(&contents) {
                Ok(prefs) => prefs,
                Err(e) => {
                    eprintln!("[preferences] Failed to parse {}: {}, using defaults", path.display(), e);
                    StructureDesignerPreferences::default()
                }
            }
        }
        Err(e) => {
            eprintln!("[preferences] Failed to read {}: {}, using defaults", path.display(), e);
            StructureDesignerPreferences::default()
        }
    }
}

/// Saves preferences to the user's config directory.
///
/// Creates the config directory if it doesn't exist.
/// Logs warnings on failure but doesn't propagate errors (preferences not saving is non-critical).
pub fn save_preferences(prefs: &StructureDesignerPreferences) {
    let Some(path) = get_preferences_path() else {
        eprintln!("[preferences] Could not determine config directory, preferences not saved");
        return;
    };

    // Create the config directory if it doesn't exist
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("[preferences] Failed to create config directory {}: {}", parent.display(), e);
            return;
        }
    }

    // Serialize with pretty printing for human readability
    let json = match serde_json::to_string_pretty(prefs) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("[preferences] Failed to serialize preferences: {}", e);
            return;
        }
    };

    if let Err(e) = fs::write(&path, json) {
        eprintln!("[preferences] Failed to write {}: {}", path.display(), e);
    }
}
