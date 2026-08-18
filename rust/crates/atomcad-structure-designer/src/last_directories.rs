//! Last-used directory per file-dialog purpose (issue #420).
//!
//! Remembers which folder the user last picked in each kind of file dialog, so
//! the next dialog of that kind opens there instead of making them dig down
//! from home again.
//!
//! Stored separately from preferences in
//! `<config_dir>/atomCAD/last_directories.json`, beside `recent_files.json`, as
//! a `{ purpose_key: directory }` map.
//!
//! # Why the application has to do this itself
//!
//! It looks platform-specific, and it is — but in the opposite direction from
//! the obvious guess. On Windows `file_picker` calls comdlg32's
//! `GetOpenFileNameW` / `GetSaveFileNameW`, which fall back to the shell's
//! per-executable `LastVisitedPidlMRU` when the caller passes no initial
//! directory, so the folder *appears* to be remembered without a line of
//! atomCAD code. On Linux the same package talks to the XDG desktop portal,
//! which shows a folder only when the app names one explicitly. So the feature
//! was never implemented — Windows was faking it.
//!
//! Do not "simplify" this module away because it looks redundant on Windows.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_DIR_NAME: &str = "atomCAD";
const LAST_DIRECTORIES_FILE_NAME: &str = "last_directories.json";

/// Which kind of file dialog a remembered directory belongs to.
///
/// Each variant is an independent slot, because these folders are genuinely
/// different places in a real project: exporting a structure to `~/renders`
/// must not move where *Load Design* opens next time.
///
/// The strings returned by [`FileDialogPurpose::key`] are the persisted
/// compatibility contract — rename a variant freely, but never its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileDialogPurpose {
    /// `.cnnd` designs: *Load Design*, *Save Design As*, *Open Recent*.
    Design,
    /// `.cnnd` library files pulled in via *Import from .cnnd library*.
    CnndLibrary,
    /// Structure imports: `.xyz`, `.cif`.
    StructureImport,
    /// Structure exports: `.xyz`, `.mol`.
    StructureExport,
}

impl FileDialogPurpose {
    /// The key this purpose occupies in `last_directories.json`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::CnndLibrary => "library",
            Self::StructureImport => "import",
            Self::StructureExport => "export",
        }
    }
}

/// Returns the path to the last-directories JSON file.
fn get_last_directories_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(
        config_dir
            .join(CONFIG_DIR_NAME)
            .join(LAST_DIRECTORIES_FILE_NAME),
    )
}

/// Loads the whole purpose → directory map from `store`.
///
/// Returns an empty map if the file doesn't exist or can't be parsed: a
/// corrupt store costs the user the memory, never the dialog.
fn load_all_in(store: &Path) -> BTreeMap<String, String> {
    match fs::read_to_string(store) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    }
}

/// Saves the whole purpose → directory map to `store`.
///
/// A `BTreeMap` rather than a `HashMap` so the file's key order is stable
/// across runs and re-recording an unchanged folder produces no diff.
fn save_all_in(store: &Path, directories: &BTreeMap<String, String>) {
    if let Some(parent) = store.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }

    if let Ok(json) = serde_json::to_string_pretty(directories) {
        let _ = fs::write(store, json);
    }
}

/// [`get_last_directory`] against an explicit store file.
pub fn get_last_directory_in(store: &Path, purpose: FileDialogPurpose) -> Option<String> {
    let directory = load_all_in(store).remove(purpose.key())?;
    Path::new(&directory).is_dir().then_some(directory)
}

/// [`record_directory`] against an explicit store file.
pub fn record_directory_in(store: &Path, purpose: FileDialogPurpose, directory: &str) {
    if directory.is_empty() {
        return;
    }

    let mut all = load_all_in(store);
    all.insert(purpose.key().to_string(), directory.to_string());
    save_all_in(store, &all);
}

/// [`record_file`] against an explicit store file.
pub fn record_file_in(store: &Path, purpose: FileDialogPurpose, file_path: &str) {
    let Some(parent) = Path::new(file_path).parent() else {
        return;
    };

    let parent = parent.to_string_lossy();
    if parent.is_empty() {
        return;
    }

    record_directory_in(store, purpose, &parent);
}

/// Returns the directory last used for `purpose`, or `None` if none was
/// recorded or it no longer exists.
///
/// The existence check is what makes a removable drive or a deleted project
/// folder degrade gracefully: the caller falls through to its own default
/// instead of handing the platform a path it will silently ignore anyway.
pub fn get_last_directory(purpose: FileDialogPurpose) -> Option<String> {
    get_last_directory_in(&get_last_directories_path()?, purpose)
}

/// Records `directory` as the last one used for `purpose`.
///
/// Deliberately *not* validated against the filesystem: a directory on a
/// currently-unmounted volume is still the right thing to remember, and
/// [`get_last_directory`] already refuses to hand out one that has gone away.
pub fn record_directory(purpose: FileDialogPurpose, directory: &str) {
    let Some(store) = get_last_directories_path() else {
        return;
    };

    record_directory_in(&store, purpose, directory);
}

/// Records the directory *containing* `file_path` as the last one used for
/// `purpose`.
///
/// This is the form every call site wants: file dialogs return the chosen
/// file, and it is its folder we want back next time. A bare filename with no
/// parent component is ignored rather than recorded as the process's working
/// directory.
pub fn record_file(purpose: FileDialogPurpose, file_path: &str) {
    let Some(store) = get_last_directories_path() else {
        return;
    };

    record_file_in(&store, purpose, file_path);
}
