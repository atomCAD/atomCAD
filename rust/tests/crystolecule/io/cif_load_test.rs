use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::io::cif::{CifLoadError, load_cif, load_cif_from_str};

fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/cif/{}", env!("CARGO_MANIFEST_DIR"), name)
}

/// Helper: check that a fractional coordinate matches expected within tolerance.
fn assert_fract_near(actual: DVec3, expected: DVec3, tol: f64, label: &str) {
    assert!(
        (actual.x - expected.x).abs() < tol
            && (actual.y - expected.y).abs() < tol
            && (actual.z - expected.z).abs() < tol,
        "{}: expected ({:.4}, {:.4}, {:.4}), got ({:.4}, {:.4}, {:.4})",
        label,
        expected.x,
        expected.y,
        expected.z,
        actual.x,
        actual.y,
        actual.z,
    );
}

/// Helper: count atoms of a given atomic number.
fn count_element(
    result: &rust_lib_flutter_cad::crystolecule::io::cif::CifLoadResult,
    atomic_number: i16,
) -> usize {
    result
        .atoms
        .iter()
        .filter(|a| a.atomic_number == atomic_number)
        .count()
}

/// Helper: check that a site with the given element exists near the expected position.
fn has_site_near(
    result: &rust_lib_flutter_cad::crystolecule::io::cif::CifLoadResult,
    atomic_number: i16,
    expected: DVec3,
    tol: f64,
) -> bool {
    result.atoms.iter().any(|a| {
        a.atomic_number == atomic_number
            && (a.fract.x - expected.x).abs() < tol
            && (a.fract.y - expected.y).abs() < tol
            && (a.fract.z - expected.z).abs() < tol
    })
}

// --- Diamond tests ---

#[test]
fn load_diamond_cif_atom_count() {
    let result = load_cif(&fixture_path("diamond.cif"), None).unwrap();
    // Diamond Fd-3m: 1 asymmetric atom → 8 in conventional cell
    assert_eq!(
        result.atoms.len(),
        8,
        "Diamond should have 8 atoms in the conventional cell"
    );
}

#[test]
fn load_diamond_cif_all_carbon() {
    let result = load_cif(&fixture_path("diamond.cif"), None).unwrap();
    // All atoms should be carbon (Z=6)
    assert_eq!(count_element(&result, 6), 8);
}

#[test]
fn load_diamond_cif_unit_cell() {
    let result = load_cif(&fixture_path("diamond.cif"), None).unwrap();
    let uc = &result.unit_cell;
    let a = uc.a.length();
    let b = uc.b.length();
    let c = uc.c.length();
    assert!((a - 3.56679).abs() < 0.001, "a = {}", a);
    assert!((b - 3.56679).abs() < 0.001, "b = {}", b);
    assert!((c - 3.56679).abs() < 0.001, "c = {}", c);
}

#[test]
fn load_diamond_cif_expected_positions() {
    let result = load_cif(&fixture_path("diamond.cif"), None).unwrap();
    let tol = 0.02;

    // FCC corner/face positions
    assert!(
        has_site_near(&result, 6, DVec3::new(0.0, 0.0, 0.0), tol),
        "Missing (0,0,0)"
    );
    assert!(
        has_site_near(&result, 6, DVec3::new(0.5, 0.5, 0.0), tol),
        "Missing (0.5,0.5,0)"
    );
    assert!(
        has_site_near(&result, 6, DVec3::new(0.5, 0.0, 0.5), tol),
        "Missing (0.5,0,0.5)"
    );
    assert!(
        has_site_near(&result, 6, DVec3::new(0.0, 0.5, 0.5), tol),
        "Missing (0,0.5,0.5)"
    );

    // Interior (tetrahedral) positions
    assert!(
        has_site_near(&result, 6, DVec3::new(0.25, 0.25, 0.25), tol),
        "Missing (0.25,0.25,0.25)"
    );
    assert!(
        has_site_near(&result, 6, DVec3::new(0.25, 0.75, 0.75), tol),
        "Missing (0.25,0.75,0.75)"
    );
    assert!(
        has_site_near(&result, 6, DVec3::new(0.75, 0.25, 0.75), tol),
        "Missing (0.75,0.25,0.75)"
    );
    assert!(
        has_site_near(&result, 6, DVec3::new(0.75, 0.75, 0.25), tol),
        "Missing (0.75,0.75,0.25)"
    );
}

// --- NaCl tests ---

#[test]
fn load_nacl_cif_atom_count() {
    let result = load_cif(&fixture_path("nacl.cif"), None).unwrap();
    // NaCl Fm-3m: 2 asymmetric atoms → 8 in conventional cell (4 Na + 4 Cl)
    assert_eq!(
        result.atoms.len(),
        8,
        "NaCl should have 8 atoms; got {}",
        result.atoms.len()
    );
}

#[test]
fn load_nacl_cif_element_counts() {
    let result = load_cif(&fixture_path("nacl.cif"), None).unwrap();
    // Na = 11, Cl = 17
    assert_eq!(count_element(&result, 11), 4, "Should have 4 Na atoms");
    assert_eq!(count_element(&result, 17), 4, "Should have 4 Cl atoms");
}

#[test]
fn load_nacl_cif_unit_cell() {
    let result = load_cif(&fixture_path("nacl.cif"), None).unwrap();
    let a = result.unit_cell.a.length();
    assert!((a - 5.62).abs() < 0.01, "NaCl a = {} (expected 5.62)", a);
}

#[test]
fn load_nacl_cif_expected_positions() {
    let result = load_cif(&fixture_path("nacl.cif"), None).unwrap();
    let tol = 0.02;

    // Na positions (FCC of Na sublattice)
    assert!(has_site_near(&result, 11, DVec3::new(0.0, 0.0, 0.0), tol));
    assert!(has_site_near(&result, 11, DVec3::new(0.0, 0.5, 0.5), tol));
    assert!(has_site_near(&result, 11, DVec3::new(0.5, 0.0, 0.5), tol));
    assert!(has_site_near(&result, 11, DVec3::new(0.5, 0.5, 0.0), tol));

    // Cl positions (FCC of Cl sublattice, offset by 0.5)
    assert!(has_site_near(&result, 17, DVec3::new(0.5, 0.5, 0.5), tol));
    assert!(has_site_near(&result, 17, DVec3::new(0.5, 0.0, 0.0), tol));
    assert!(has_site_near(&result, 17, DVec3::new(0.0, 0.5, 0.0), tol));
    assert!(has_site_near(&result, 17, DVec3::new(0.0, 0.0, 0.5), tol));
}

// --- Hexagonal (Wurtzite) tests ---

#[test]
fn load_hexagonal_cif_atom_count() {
    let result = load_cif(&fixture_path("hexagonal.cif"), None).unwrap();
    // Wurtzite P63mc: 2 asymmetric atoms → 4 in unit cell (2 Zn + 2 S)
    assert_eq!(
        result.atoms.len(),
        4,
        "Wurtzite should have 4 atoms; got {}",
        result.atoms.len()
    );
}

#[test]
fn load_hexagonal_cif_element_counts() {
    let result = load_cif(&fixture_path("hexagonal.cif"), None).unwrap();
    // Zn = 30, S = 16
    assert_eq!(count_element(&result, 30), 2, "Should have 2 Zn atoms");
    assert_eq!(count_element(&result, 16), 2, "Should have 2 S atoms");
}

#[test]
fn load_hexagonal_cif_unit_cell_non_orthogonal() {
    let result = load_cif(&fixture_path("hexagonal.cif"), None).unwrap();
    let uc = &result.unit_cell;
    let a = uc.a.length();
    let c = uc.c.length();
    assert!((a - 3.811).abs() < 0.01, "a = {} (expected 3.811)", a);
    assert!((c - 6.234).abs() < 0.01, "c = {} (expected 6.234)", c);

    // gamma = 120°: basis_a · basis_b should give cos(120°) = -0.5
    let dot = uc.a.normalize().dot(uc.b.normalize());
    assert!(
        (dot - (-0.5)).abs() < 0.01,
        "cos(gamma) = {} (expected -0.5 for 120°)",
        dot
    );
}

#[test]
fn load_hexagonal_cif_expected_positions() {
    let result = load_cif(&fixture_path("hexagonal.cif"), None).unwrap();
    let tol = 0.02;

    // Zn positions in wurtzite
    assert!(has_site_near(
        &result,
        30,
        DVec3::new(1.0 / 3.0, 2.0 / 3.0, 0.0),
        tol
    ));
    assert!(has_site_near(
        &result,
        30,
        DVec3::new(2.0 / 3.0, 1.0 / 3.0, 0.5),
        tol
    ));

    // S positions in wurtzite
    assert!(has_site_near(
        &result,
        16,
        DVec3::new(1.0 / 3.0, 2.0 / 3.0, 0.385),
        tol
    ));
    assert!(has_site_near(
        &result,
        16,
        DVec3::new(2.0 / 3.0, 1.0 / 3.0, 0.885),
        tol
    ));
}

// --- Block selection tests ---

#[test]
fn load_multi_block_first_block_by_default() {
    let result = load_cif(&fixture_path("multi_block.cif"), None).unwrap();
    // First block is diamond — should have 8 carbon atoms
    assert_eq!(result.atoms.len(), 8);
    assert_eq!(count_element(&result, 6), 8);
}

#[test]
fn load_multi_block_select_by_name() {
    let result = load_cif(&fixture_path("multi_block.cif"), Some("nacl")).unwrap();
    // NaCl block — 4 Na + 4 Cl
    assert_eq!(result.atoms.len(), 8);
    assert_eq!(count_element(&result, 11), 4);
    assert_eq!(count_element(&result, 17), 4);
}

#[test]
fn load_multi_block_unknown_name_error() {
    let err = load_cif(&fixture_path("multi_block.cif"), Some("nonexistent")).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent"),
        "Error should mention the requested name: {}",
        msg
    );
    assert!(
        msg.contains("diamond") || msg.contains("nacl"),
        "Error should list available blocks: {}",
        msg
    );
}

// --- Error handling tests ---

#[test]
fn load_cif_nonexistent_file() {
    let err = load_cif("/nonexistent/path/to/file.cif", None).unwrap_err();
    assert!(matches!(err, CifLoadError::Io(_)));
}

#[test]
fn load_cif_empty_content() {
    let err = load_cif_from_str("", None).unwrap_err();
    assert!(matches!(err, CifLoadError::NoDataBlocks));
}

#[test]
fn load_cif_from_str_diamond_minimal() {
    // Minimal inline CIF with identity symmetry only (1 atom stays 1 atom)
    let cif = r#"
data_test
_cell_length_a 3.567
_cell_length_b 3.567
_cell_length_c 3.567
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_symmetry_equiv_pos_as_xyz
x,y,z
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
C1 C 0.0 0.0 0.0
"#;
    let result = load_cif_from_str(cif, None).unwrap();
    assert_eq!(result.atoms.len(), 1);
    assert_eq!(result.atoms[0].atomic_number, 6);
    assert_fract_near(
        result.atoms[0].fract,
        DVec3::new(0.0, 0.0, 0.0),
        0.001,
        "C1",
    );
}
