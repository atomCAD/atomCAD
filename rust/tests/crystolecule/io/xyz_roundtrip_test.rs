use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::io::xyz_loader::load_xyz;
use rust_lib_flutter_cad::crystolecule::io::xyz_saver::save_xyz;
use tempfile::tempdir;

fn create_simple_structure() -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Carbon
    structure.add_atom(1, DVec3::new(1.0, 0.0, 0.0)); // Hydrogen
    structure.add_atom(1, DVec3::new(-1.0, 0.0, 0.0)); // Hydrogen
    structure
}

fn create_water_molecule() -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(8, DVec3::new(0.0, 0.0, 0.0)); // Oxygen
    structure.add_atom(1, DVec3::new(0.96, 0.0, 0.0)); // Hydrogen
    structure.add_atom(1, DVec3::new(-0.24, 0.93, 0.0)); // Hydrogen
    structure
}

fn create_methane_molecule() -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Carbon
    structure.add_atom(1, DVec3::new(1.09, 0.0, 0.0)); // H1
    structure.add_atom(1, DVec3::new(-0.36, 1.03, 0.0)); // H2
    structure.add_atom(1, DVec3::new(-0.36, -0.51, 0.89)); // H3
    structure.add_atom(1, DVec3::new(-0.36, -0.51, -0.89)); // H4
    structure
}

fn assert_structures_equal(original: &AtomicStructure, loaded: &AtomicStructure) {
    assert_eq!(
        original.get_num_of_atoms(),
        loaded.get_num_of_atoms(),
        "Atom count mismatch"
    );

    let mut original_atoms: Vec<_> = original.iter_atoms().collect();
    let mut loaded_atoms: Vec<_> = loaded.iter_atoms().collect();

    original_atoms.sort_by(|a, b| {
        a.1.position
            .x
            .partial_cmp(&b.1.position.x)
            .unwrap()
            .then(a.1.position.y.partial_cmp(&b.1.position.y).unwrap())
            .then(a.1.position.z.partial_cmp(&b.1.position.z).unwrap())
    });
    loaded_atoms.sort_by(|a, b| {
        a.1.position
            .x
            .partial_cmp(&b.1.position.x)
            .unwrap()
            .then(a.1.position.y.partial_cmp(&b.1.position.y).unwrap())
            .then(a.1.position.z.partial_cmp(&b.1.position.z).unwrap())
    });

    for (i, ((_, orig_atom), (_, loaded_atom))) in
        original_atoms.iter().zip(loaded_atoms.iter()).enumerate()
    {
        assert_eq!(
            orig_atom.atomic_number, loaded_atom.atomic_number,
            "Atomic number mismatch at atom {}",
            i
        );

        let pos_diff = (orig_atom.position - loaded_atom.position).length();
        assert!(
            pos_diff < 1e-5,
            "Position mismatch at atom {}: original {:?}, loaded {:?}",
            i,
            orig_atom.position,
            loaded_atom.position
        );
    }
}

#[test]
fn test_xyz_roundtrip_simple() {
    let original = create_simple_structure();
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("simple.xyz");
    let file_path_str = file_path.to_str().unwrap();

    save_xyz(&original, file_path_str).expect("Failed to save XYZ");
    let loaded = load_xyz(file_path_str, false).expect("Failed to load XYZ");

    assert_structures_equal(&original, &loaded);
}

#[test]
fn test_xyz_roundtrip_water() {
    let original = create_water_molecule();
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("water.xyz");
    let file_path_str = file_path.to_str().unwrap();

    save_xyz(&original, file_path_str).expect("Failed to save XYZ");
    let loaded = load_xyz(file_path_str, false).expect("Failed to load XYZ");

    assert_structures_equal(&original, &loaded);
}

#[test]
fn test_xyz_roundtrip_methane() {
    let original = create_methane_molecule();
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("methane.xyz");
    let file_path_str = file_path.to_str().unwrap();

    save_xyz(&original, file_path_str).expect("Failed to save XYZ");
    let loaded = load_xyz(file_path_str, false).expect("Failed to load XYZ");

    assert_structures_equal(&original, &loaded);
}

#[test]
fn test_xyz_roundtrip_empty() {
    let original = AtomicStructure::new();
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("empty.xyz");
    let file_path_str = file_path.to_str().unwrap();

    save_xyz(&original, file_path_str).expect("Failed to save XYZ");
    let loaded = load_xyz(file_path_str, false).expect("Failed to load XYZ");

    assert_eq!(loaded.get_num_of_atoms(), 0);
}

#[test]
fn test_xyz_roundtrip_various_elements() {
    let mut original = AtomicStructure::new();
    original.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // Carbon
    original.add_atom(7, DVec3::new(1.5, 0.0, 0.0)); // Nitrogen
    original.add_atom(8, DVec3::new(3.0, 0.0, 0.0)); // Oxygen
    original.add_atom(16, DVec3::new(4.5, 0.0, 0.0)); // Sulfur
    original.add_atom(15, DVec3::new(6.0, 0.0, 0.0)); // Phosphorus
    original.add_atom(14, DVec3::new(7.5, 0.0, 0.0)); // Silicon

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("elements.xyz");
    let file_path_str = file_path.to_str().unwrap();

    save_xyz(&original, file_path_str).expect("Failed to save XYZ");
    let loaded = load_xyz(file_path_str, false).expect("Failed to load XYZ");

    assert_structures_equal(&original, &loaded);
}

#[test]
fn test_xyz_roundtrip_precision() {
    let mut original = AtomicStructure::new();
    original.add_atom(6, DVec3::new(1.234567, 2.345678, 3.456789));
    original.add_atom(6, DVec3::new(-0.000001, 0.000002, -0.000003));
    original.add_atom(6, DVec3::new(100.123456, -200.234567, 300.345678));

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("precision.xyz");
    let file_path_str = file_path.to_str().unwrap();

    save_xyz(&original, file_path_str).expect("Failed to save XYZ");
    let loaded = load_xyz(file_path_str, false).expect("Failed to load XYZ");

    assert_structures_equal(&original, &loaded);
}
