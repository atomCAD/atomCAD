use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::atom_export::{AtomExportFormat, AtomExportOptions};
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use glam::f64::DVec3;
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
        .save(&structure, path_str, &AtomExportOptions::default())
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
        .save(&structure, path_str, &AtomExportOptions::default())
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

// ---------------------------------------------------------------------------
// Frozen atoms in .xyz (`FREEZEXYZ` trailer line)
// ---------------------------------------------------------------------------

fn save_to_string(structure: &AtomicStructure, file: &str, write_frozen: bool) -> String {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join(file);
    let path_str = path.to_str().unwrap();
    AtomExportFormat::from_path(path_str)
        .expect("format recognized")
        .save(structure, path_str, &AtomExportOptions { write_frozen })
        .expect("save");
    fs::read_to_string(path_str).expect("read back")
}

#[test]
fn xyz_frozen_line_uses_file_positions_not_atom_ids() {
    let mut structure = AtomicStructure::new();
    let a = structure.add_atom(14, DVec3::new(0.0, 0.0, 0.0));
    let gone = structure.add_atom(14, DVec3::new(1.0, 0.0, 0.0));
    let c = structure.add_atom(1, DVec3::new(2.0, 0.0, 0.0));
    let d = structure.add_atom(1, DVec3::new(3.0, 0.0, 0.0));
    // Deleting atom 2 leaves an id gap: atom `c` (id 3) is line 2 of the file.
    structure.delete_atom(gone);
    structure.set_atom_frozen(a, true);
    structure.set_atom_frozen(c, true);
    let _ = d;

    let text = save_to_string(&structure, "frozen.xyz", true);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "3");
    assert_eq!(lines.len(), 2 + 3 + 1);
    assert_eq!(lines[5], "FREEZEXYZ 1 2");
}

#[test]
fn xyz_frozen_line_omitted_without_frozen_atoms() {
    let text = save_to_string(&create_water_molecule(), "water.xyz", true);
    assert!(!text.contains("FREEZEXYZ"));
    assert_eq!(text.lines().count(), 2 + 3);
}

#[test]
fn xyz_frozen_line_omitted_when_disabled() {
    let mut structure = create_water_molecule();
    structure.set_atom_frozen(1, true);
    let text = save_to_string(&structure, "water.xyz", false);
    assert!(!text.contains("FREEZEXYZ"));
}

#[test]
fn mol_ignores_write_frozen() {
    let mut structure = create_water_molecule();
    structure.set_atom_frozen(1, true);
    let text = save_to_string(&structure, "water.mol", true);
    assert!(!text.contains("FREEZEXYZ"));
}

#[test]
fn xyz_frozen_atoms_roundtrip() {
    let mut structure = create_water_molecule();
    structure.set_atom_frozen(1, true);
    structure.set_atom_frozen(3, true);
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("water.xyz");
    let path_str = path.to_str().unwrap();
    AtomExportFormat::Xyz
        .save(&structure, path_str, &AtomExportOptions::default())
        .expect("save");

    let loaded = load_xyz(path_str, true).expect("reload");
    let frozen: Vec<bool> = loaded.iter_atoms().map(|(_, a)| a.is_frozen()).collect();
    assert_eq!(frozen, vec![true, false, true]);
}

// ---------------------------------------------------------------------------
// Reading `FREEZEXYZ`
// ---------------------------------------------------------------------------

fn load_text(text: &str) -> Result<AtomicStructure, String> {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("in.xyz");
    fs::write(&path, text).expect("write");
    load_xyz(path.to_str().unwrap(), false).map_err(|e| e.to_string())
}

const FOUR_ATOMS: &str = "4\n\
    Lattice=\"3.84 0.0 0.0 0.0 3.84 0.0 0.0 0.0 30.0\" pbc=\"T T F\"\n\
    Si  0.000000  0.000000  -1.357500\n\
    Si  1.920000  1.920000   0.000000\n\
    H   0.000000  0.000000  -2.857500\n\
    H   1.920000  1.920000   1.500000\n";

#[test]
fn load_xyz_reads_freeze_line() {
    let structure = load_text(&format!("{FOUR_ATOMS}FREEZEXYZ 1 3\n")).expect("loads");
    let frozen: Vec<bool> = structure.iter_atoms().map(|(_, a)| a.is_frozen()).collect();
    assert_eq!(frozen, vec![true, false, true, false]);
}

#[test]
fn load_xyz_without_freeze_line_freezes_nothing() {
    let structure = load_text(FOUR_ATOMS).expect("loads");
    assert!(structure.iter_atoms().all(|(_, a)| !a.is_frozen()));
}

#[test]
fn load_xyz_tolerates_trailing_blank_lines() {
    let structure = load_text(&format!("{FOUR_ATOMS}\nFREEZEXYZ 2\n\n")).expect("loads");
    assert_eq!(
        structure
            .iter_atoms()
            .filter(|(_, a)| a.is_frozen())
            .count(),
        1
    );
}

#[test]
fn load_xyz_rejects_out_of_range_freeze_index() {
    for bad in ["0", "5", "x", "-1"] {
        let err = load_text(&format!("{FOUR_ATOMS}FREEZEXYZ 1 {bad}\n")).unwrap_err();
        assert!(err.contains("FREEZEXYZ"), "{bad}: {err}");
    }
}

#[test]
fn load_xyz_still_rejects_extra_atoms_and_junk() {
    let err = load_text(&format!("{FOUR_ATOMS}C 0 0 0\n")).unwrap_err();
    assert!(err.contains("Expected 4 atoms"), "{err}");
    let err = load_text(&format!("{FOUR_ATOMS}something else\n")).unwrap_err();
    assert!(err.contains("Unexpected content"), "{err}");
    let err = load_text("4\ntitle\nH 0 0 0\n").unwrap_err();
    assert!(err.contains("Expected 4 atoms, but found 1"), "{err}");
}
