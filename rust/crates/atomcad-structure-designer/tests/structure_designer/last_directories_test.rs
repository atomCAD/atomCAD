//! Tests for the per-purpose last-used-directory store (issue #420).
//!
//! These drive the `*_in` variants against a `TempDir` rather than the public
//! wrappers, which would read and write the developer's real
//! `<config_dir>/atomCAD/last_directories.json`.

use atomcad_structure_designer::last_directories::{
    FileDialogPurpose, get_last_directory_in, record_directory_in, record_file_in,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ALL_PURPOSES: [FileDialogPurpose; 4] = [
    FileDialogPurpose::Design,
    FileDialogPurpose::CnndLibrary,
    FileDialogPurpose::StructureImport,
    FileDialogPurpose::StructureExport,
];

/// A temp dir plus the store path inside it. The store's parent is created
/// lazily by `save_all_in`, exactly as it is in the real config dir.
fn store_in(temp: &TempDir) -> PathBuf {
    temp.path().join("atomCAD").join("last_directories.json")
}

/// `path.to_string_lossy()` normalised the way the module stores it, so
/// assertions compare like with like on Windows and Unix alike.
fn as_stored(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn test_missing_store_yields_no_directory() {
    let temp = TempDir::new().unwrap();

    for purpose in ALL_PURPOSES {
        assert_eq!(get_last_directory_in(&store_in(&temp), purpose), None);
    }
}

#[test]
fn test_record_then_get_roundtrip() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    let designs = temp.path().join("designs");
    std::fs::create_dir_all(&designs).unwrap();

    record_directory_in(&store, FileDialogPurpose::Design, &as_stored(&designs));

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        Some(as_stored(&designs))
    );
}

/// The whole point of keying by purpose: exporting somewhere must not move
/// where *Load Design* opens.
#[test]
fn test_purposes_are_independent_slots() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    let designs = temp.path().join("designs");
    let renders = temp.path().join("renders");
    std::fs::create_dir_all(&designs).unwrap();
    std::fs::create_dir_all(&renders).unwrap();

    record_directory_in(&store, FileDialogPurpose::Design, &as_stored(&designs));
    record_directory_in(
        &store,
        FileDialogPurpose::StructureExport,
        &as_stored(&renders),
    );

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        Some(as_stored(&designs))
    );
    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::StructureExport),
        Some(as_stored(&renders))
    );
    // Untouched slots stay empty rather than inheriting a neighbour's folder.
    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::StructureImport),
        None
    );
    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::CnndLibrary),
        None
    );
}

#[test]
fn test_record_file_stores_its_parent_directory() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    let designs = temp.path().join("designs");
    std::fs::create_dir_all(&designs).unwrap();
    let file = designs.join("nanobeam.cnnd");

    record_file_in(&store, FileDialogPurpose::Design, &as_stored(&file));

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        Some(as_stored(&designs))
    );
}

/// A bare filename has an empty parent. Recording it would pin the dialog to
/// whatever the process's working directory happened to be.
#[test]
fn test_record_file_ignores_a_bare_filename() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);

    record_file_in(&store, FileDialogPurpose::Design, "design.cnnd");

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        None
    );
}

#[test]
fn test_record_directory_ignores_an_empty_path() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);

    record_directory_in(&store, FileDialogPurpose::StructureImport, "");

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::StructureImport),
        None
    );
}

/// A folder that has since been deleted (or lives on an unmounted volume) must
/// not be handed back — the caller falls through to its own default instead.
#[test]
fn test_vanished_directory_is_not_returned() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    let scratch = temp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();

    record_directory_in(
        &store,
        FileDialogPurpose::StructureImport,
        &as_stored(&scratch),
    );
    assert!(get_last_directory_in(&store, FileDialogPurpose::StructureImport).is_some());

    std::fs::remove_dir_all(&scratch).unwrap();

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::StructureImport),
        None
    );
}

/// A file rather than a directory is likewise refused: `is_dir` is the check,
/// not `exists`.
#[test]
fn test_a_file_path_is_not_accepted_as_a_directory() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    let file = temp.path().join("not_a_directory.txt");
    std::fs::write(&file, "x").unwrap();

    record_directory_in(
        &store,
        FileDialogPurpose::StructureExport,
        &as_stored(&file),
    );

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::StructureExport),
        None
    );
}

#[test]
fn test_rerecording_replaces_the_previous_directory() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();

    record_directory_in(&store, FileDialogPurpose::Design, &as_stored(&first));
    record_directory_in(&store, FileDialogPurpose::Design, &as_stored(&second));

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        Some(as_stored(&second))
    );
}

/// A corrupt store costs the user the memory, never the dialog.
#[test]
fn test_corrupt_store_reads_as_empty_and_can_be_rewritten() {
    let temp = TempDir::new().unwrap();
    let store = store_in(&temp);
    std::fs::create_dir_all(store.parent().unwrap()).unwrap();
    std::fs::write(&store, "{ this is not json").unwrap();

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        None
    );

    let designs = temp.path().join("designs");
    std::fs::create_dir_all(&designs).unwrap();
    record_directory_in(&store, FileDialogPurpose::Design, &as_stored(&designs));

    assert_eq!(
        get_last_directory_in(&store, FileDialogPurpose::Design),
        Some(as_stored(&designs))
    );
}

/// The keys are the persisted compatibility contract. If one of these changes,
/// every user silently loses the folder that slot remembered.
#[test]
fn test_purpose_keys_are_stable_and_distinct() {
    assert_eq!(FileDialogPurpose::Design.key(), "design");
    assert_eq!(FileDialogPurpose::CnndLibrary.key(), "library");
    assert_eq!(FileDialogPurpose::StructureImport.key(), "import");
    assert_eq!(FileDialogPurpose::StructureExport.key(), "export");

    let keys: std::collections::HashSet<&str> = ALL_PURPOSES.iter().map(|p| p.key()).collect();
    assert_eq!(
        keys.len(),
        ALL_PURPOSES.len(),
        "purpose keys must be unique"
    );
}
