//! Recently opened files tracking.
//!
//! Stores a list of recently opened file paths in the user's config directory.
//! The list is capped at 10 entries, with the most recently opened file first.
//!
//! Stored separately from preferences in `<config_dir>/atomCAD/recent_files.json`.

use std::fs;
use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "atomCAD";
const RECENT_FILES_FILE_NAME: &str = "recent_files.json";
const MAX_RECENT_FILES: usize = 10;

/// Returns the path to the recent files JSON file.
fn get_recent_files_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(
        config_dir
            .join(CONFIG_DIR_NAME)
            .join(RECENT_FILES_FILE_NAME),
    )
}

/// Loads the list of recently opened file paths.
///
/// Returns an empty list if the file doesn't exist or can't be parsed.
pub fn load_recent_files() -> Vec<String> {
    let Some(path) = get_recent_files_path() else {
        return Vec::new();
    };

    if !path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Adds a file path to the recent files list.
///
/// The path is placed at the front of the list. If it already exists in the list,
/// it is moved to the front (deduplication). The list is capped at 10 entries.
/// Non-existent files are pruned during this operation.
pub fn add_recent_file(file_path: &str) {
    let mut files = load_recent_files();

    // Remove the path if it already exists (we'll re-add it at the front)
    files.retain(|f| f != file_path);

    // Insert at the front
    files.insert(0, file_path.to_string());

    // Prune non-existent files
    files.retain(|f| std::path::Path::new(f).exists());

    // Cap at max
    files.truncate(MAX_RECENT_FILES);

    save_recent_files(&files);
}

/// Saves the recent files list to disk.
fn save_recent_files(files: &[String]) {
    let Some(path) = get_recent_files_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    if let Ok(json) = serde_json::to_string_pretty(files) {
        let _ = fs::write(&path, json);
    }
}
