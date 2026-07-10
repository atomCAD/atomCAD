use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::io::atom_export::AtomExportFormat;
use rust_lib_flutter_cad::crystolecule::io::xyz_loader::load_xyz;
use std::fs;
use tempfile::tempdir;

fn create_water_molecule() -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    let o = structure.add_atom(8, DVec3::new(0.0, 0.0, 0.0)); // Oxygen
    let h1 = structure.add_atom(1, DVec3::new(0.757, 0.586, 0.0)); // Hydrogen
    let h2 = structure.add_atom(1, DVec3::new(-0.757, 0.586, 0.0)); // Hydrogen
    structure.add_bond_checked(o, h1, 1);
    structure.add_bond_checked(o, h2, 1);
    structure
}

#[test]
fn from_path_recognizes_extensions() {
    assert_eq!(
        AtomExportFormat::from_path("out.xyz"),
        Some(AtomExportFormat::Xyz)
    );
    assert_eq!(
        AtomExportFormat::from_path("part.mol"),
        Some(AtomExportFormat::Mol)
    );
}

#[test]
fn from_path_is_case_insensitive() {
    assert_eq!(
        AtomExportFormat::from_path("out.XYZ"),
        Some(AtomExportFormat::Xyz)
    );
    assert_eq!(
        AtomExportFormat::from_path("out.Xyz"),
        Some(AtomExportFormat::Xyz)
    );
    assert_eq!(
        AtomExportFormat::from_path("part.MOL"),
        Some(AtomExportFormat::Mol)
    );
    assert_eq!(
        AtomExportFormat::from_path("part.Mol"),
        Some(AtomExportFormat::Mol)
    );
}

#[test]
fn from_path_none_for_missing_extension() {
    assert_eq!(AtomExportFormat::from_path("structure"), None);
    assert_eq!(AtomExportFormat::from_path("some/dir/structure"), None);
}

#[test]
fn from_path_none_for_unknown_extension() {
    assert_eq!(AtomExportFormat::from_path("out.pdb"), None);
    assert_eq!(AtomExportFormat::from_path("out.txt"), None);
    assert_eq!(AtomExportFormat::from_path("out.cif"), None);
}

#[test]
fn from_path_ignores_dotted_directory_names() {
    // Only the final path component's extension counts: a dotted directory
    // name must not be mistaken for a file extension. Both separator styles
    // are exercised (backslash is a separator on Windows, the target platform).
    assert_eq!(AtomExportFormat::from_path("my.dir/file"), None);
    assert_eq!(
        AtomExportFormat::from_path("my.dir/file.xyz"),
        Some(AtomExportFormat::Xyz)
    );
    #[cfg(windows)]
    {
        assert_eq!(AtomExportFormat::from_path(r"C:\my.dir\file"), None);
        assert_eq!(
            AtomExportFormat::from_path(r"C:\my.dir\file.xyz"),
            Some(AtomExportFormat::Xyz)
        );
    }
}

#[test]
fn extension_and_metadata_round_out_all() {
    // ALL and the derived display list stay consistent.
    assert_eq!(AtomExportFormat::Xyz.extension(), "xyz");
    assert_eq!(AtomExportFormat::Mol.extension(), "mol");
    assert!(!AtomExportFormat::Xyz.label().is_empty());
    assert!(!AtomExportFormat::Mol.label().is_empty());
    assert!(!AtomExportFormat::Xyz.description().is_empty());
    assert!(!AtomExportFormat::Mol.description().is_empty());
    assert_eq!(
        AtomExportFormat::supported_extensions_display(),
        ".xyz, .mol"
    );
}

#[test]
fn save_xyz_roundtrips() {
    let structure = create_water_molecule();
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("water.xyz");
    let path_str = path.to_str().unwrap();

    AtomExportFormat::from_path(path_str)
        .expect("xyz recognized")
        .save(&structure, path_str)
        .expect("save xyz");

    let loaded = load_xyz(path_str, false).expect("reload xyz");
    assert_eq!(
        loaded.get_num_of_atoms(),
        structure.get_num_of_atoms(),
        "atom count should survive an xyz roundtrip"
    );
}

#[test]
fn save_mol_writes_v3000() {
    let structure = create_water_molecule();
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("water.mol");
    let path_str = path.to_str().unwrap();

    AtomExportFormat::from_path(path_str)
        .expect("mol recognized")
        .save(&structure, path_str)
        .expect("save mol");

    let content = fs::read_to_string(path_str).expect("read mol");
    assert!(content.contains("V3000"), "MOL output should be V3000");
    assert!(
        content.contains("M  V30 BEGIN CTAB"),
        "MOL output should contain a CTAB block"
    );
    assert!(
        content.contains("M  V30 COUNTS 3 2 0 0 0"),
        "MOL COUNTS should reflect 3 atoms / 2 bonds"
    );
}
