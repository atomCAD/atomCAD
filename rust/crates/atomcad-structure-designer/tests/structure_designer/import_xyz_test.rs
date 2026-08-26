//! `import_xyz` node tests.
//!
//! Currently just the payload-preservation rule, which `import_cube` shares and
//! documents at length: the property setter behind the path text field runs on
//! every **focus loss**, not only on a real edit, so rebuilding the node data
//! with `atomic_structure: None` on every write silently un-imported an
//! already-loaded file. `ImportXYZData::with_file_name` is what keeps a no-op
//! write harmless; `set_import_xyz_data` in the api layer is its only caller.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use glam::DVec3;

fn loaded_data(file_name: &str) -> ImportXYZData {
    let mut atoms = AtomicStructure::new();
    atoms.add_atom(6, DVec3::ZERO);
    ImportXYZData {
        file_name: Some(file_name.to_string()),
        atomic_structure: Some(atoms),
    }
}

#[test]
fn an_unchanged_file_name_keeps_the_imported_structure() {
    let data = loaded_data("molecule.xyz");
    assert!(
        data.with_file_name(Some("molecule.xyz".to_string()))
            .atomic_structure
            .is_some(),
        "re-writing the same file name must not un-import the file"
    );
}

#[test]
fn a_changed_file_name_drops_the_imported_structure() {
    let data = loaded_data("molecule.xyz");
    assert!(
        data.with_file_name(Some("other.xyz".to_string()))
            .atomic_structure
            .is_none(),
        "a different file name must invalidate the payload"
    );
    assert!(
        data.with_file_name(None).atomic_structure.is_none(),
        "clearing the file name must invalidate the payload too"
    );
}
