use rust_lib_flutter_cad::crystolecule::io::poscar_loader::parse_poscar;

// Silicon diamond cubic: 2 atoms per unit cell, a = 5.431 Angstrom
const SILICON_POSCAR: &str = "\
Si diamond cubic
1.0
5.431 0.000 0.000
0.000 5.431 0.000
0.000 0.000 5.431
Si
2
Direct
0.000 0.000 0.000
0.250 0.250 0.250
";

// NaCl rock salt: 8 atoms per unit cell (4 Na + 4 Cl), a = 5.640 Angstrom
const NACL_POSCAR: &str = "\
NaCl rock salt
1.0
5.640 0.000 0.000
0.000 5.640 0.000
0.000 0.000 5.640
Na Cl
4 4
Direct
0.000 0.000 0.000
0.500 0.500 0.000
0.500 0.000 0.500
0.000 0.500 0.500
0.500 0.500 0.500
0.000 0.000 0.500
0.000 0.500 0.000
0.500 0.000 0.000
";

// BCC Iron: 2 atoms per unit cell, a = 2.866 Angstrom
const BCC_FE_POSCAR: &str = "\
BCC Iron
1.0
2.866 0.000 0.000
0.000 2.866 0.000
0.000 0.000 2.866
Fe
2
Direct
0.000 0.000 0.000
0.500 0.500 0.500
";

// Silicon with Cartesian coordinates
const SILICON_CARTESIAN_POSCAR: &str = "\
Si diamond cubic (cartesian)
1.0
5.431 0.000 0.000
0.000 5.431 0.000
0.000 0.000 5.431
Si
2
Cartesian
0.000 0.000 0.000
1.35775 1.35775 1.35775
";

// Silicon with scaling factor
const SILICON_SCALED_POSCAR: &str = "\
Si diamond cubic (scaled)
2.0
2.7155 0.000 0.000
0.000 2.7155 0.000
0.000 0.000 2.7155
Si
2
Direct
0.000 0.000 0.000
0.250 0.250 0.250
";

#[test]
fn test_parse_silicon_diamond_cubic() {
    let (unit_cell, motif) = parse_poscar(SILICON_POSCAR).unwrap();

    // Check unit cell dimensions
    assert!((unit_cell.cell_length_a - 5.431).abs() < 1e-6);
    assert!((unit_cell.cell_length_b - 5.431).abs() < 1e-6);
    assert!((unit_cell.cell_length_c - 5.431).abs() < 1e-6);

    // Check angles (cubic = 90 degrees)
    assert!((unit_cell.cell_angle_alpha - 90.0).abs() < 1e-6);
    assert!((unit_cell.cell_angle_beta - 90.0).abs() < 1e-6);
    assert!((unit_cell.cell_angle_gamma - 90.0).abs() < 1e-6);

    // Check motif: 2 Si atoms
    assert_eq!(motif.sites.len(), 2);
    assert_eq!(motif.sites[0].atomic_number, 14); // Si
    assert_eq!(motif.sites[1].atomic_number, 14); // Si

    // Check fractional coordinates
    assert!((motif.sites[0].position.x - 0.0).abs() < 1e-6);
    assert!((motif.sites[0].position.y - 0.0).abs() < 1e-6);
    assert!((motif.sites[0].position.z - 0.0).abs() < 1e-6);

    assert!((motif.sites[1].position.x - 0.25).abs() < 1e-6);
    assert!((motif.sites[1].position.y - 0.25).abs() < 1e-6);
    assert!((motif.sites[1].position.z - 0.25).abs() < 1e-6);

    // No bonds or parameters from POSCAR
    assert!(motif.bonds.is_empty());
    assert!(motif.parameters.is_empty());
}

#[test]
fn test_parse_nacl_rock_salt() {
    let (unit_cell, motif) = parse_poscar(NACL_POSCAR).unwrap();

    // Check unit cell
    assert!((unit_cell.cell_length_a - 5.640).abs() < 1e-6);

    // Check motif: 8 atoms total (4 Na + 4 Cl)
    assert_eq!(motif.sites.len(), 8);

    // First 4 atoms are Na (atomic number 11)
    for i in 0..4 {
        assert_eq!(motif.sites[i].atomic_number, 11, "Site {} should be Na", i);
    }

    // Last 4 atoms are Cl (atomic number 17)
    for i in 4..8 {
        assert_eq!(motif.sites[i].atomic_number, 17, "Site {} should be Cl", i);
    }
}

#[test]
fn test_parse_bcc_iron() {
    let (unit_cell, motif) = parse_poscar(BCC_FE_POSCAR).unwrap();

    // Check unit cell
    assert!((unit_cell.cell_length_a - 2.866).abs() < 1e-6);

    // Check motif: 2 Fe atoms (atomic number 26)
    assert_eq!(motif.sites.len(), 2);
    assert_eq!(motif.sites[0].atomic_number, 26);
    assert_eq!(motif.sites[1].atomic_number, 26);

    // BCC: corner at (0,0,0) and body center at (0.5, 0.5, 0.5)
    assert!((motif.sites[0].position.x - 0.0).abs() < 1e-6);
    assert!((motif.sites[1].position.x - 0.5).abs() < 1e-6);
    assert!((motif.sites[1].position.y - 0.5).abs() < 1e-6);
    assert!((motif.sites[1].position.z - 0.5).abs() < 1e-6);
}

#[test]
fn test_parse_cartesian_coordinates() {
    let (unit_cell, motif) = parse_poscar(SILICON_CARTESIAN_POSCAR).unwrap();

    // Same unit cell as direct case
    assert!((unit_cell.cell_length_a - 5.431).abs() < 1e-6);

    // 2 Si atoms
    assert_eq!(motif.sites.len(), 2);

    // First atom at origin in fractional coordinates
    assert!((motif.sites[0].position.x - 0.0).abs() < 1e-6);
    assert!((motif.sites[0].position.y - 0.0).abs() < 1e-6);
    assert!((motif.sites[0].position.z - 0.0).abs() < 1e-6);

    // Second atom: 1.35775/5.431 ≈ 0.25 in fractional coordinates
    assert!((motif.sites[1].position.x - 0.25).abs() < 1e-3);
    assert!((motif.sites[1].position.y - 0.25).abs() < 1e-3);
    assert!((motif.sites[1].position.z - 0.25).abs() < 1e-3);
}

#[test]
fn test_parse_scaling_factor() {
    let (unit_cell, motif) = parse_poscar(SILICON_SCALED_POSCAR).unwrap();

    // After applying scaling factor of 2.0: 2.7155 * 2.0 = 5.431
    assert!((unit_cell.cell_length_a - 5.431).abs() < 1e-6);
    assert!((unit_cell.cell_length_b - 5.431).abs() < 1e-6);
    assert!((unit_cell.cell_length_c - 5.431).abs() < 1e-6);

    // 2 Si atoms
    assert_eq!(motif.sites.len(), 2);
}

#[test]
fn test_parse_empty_content() {
    let result = parse_poscar("");
    assert!(result.is_err());
}

#[test]
fn test_parse_too_few_lines() {
    let content = "Comment\n1.0\n5.0 0.0 0.0\n";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_invalid_scaling_factor() {
    let content = "\
Comment
abc
5.0 0.0 0.0
0.0 5.0 0.0
0.0 0.0 5.0
Si
1
Direct
0.0 0.0 0.0
";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_negative_scaling_factor() {
    let content = "\
Comment
-1.0
5.0 0.0 0.0
0.0 5.0 0.0
0.0 0.0 5.0
Si
1
Direct
0.0 0.0 0.0
";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_unknown_element() {
    let content = "\
Comment
1.0
5.0 0.0 0.0
0.0 5.0 0.0
0.0 0.0 5.0
Xx
1
Direct
0.0 0.0 0.0
";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_mismatched_species_counts() {
    let content = "\
Comment
1.0
5.0 0.0 0.0
0.0 5.0 0.0
0.0 0.0 5.0
Si Na
1
Direct
0.0 0.0 0.0
";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_invalid_coordinate_type() {
    let content = "\
Comment
1.0
5.0 0.0 0.0
0.0 5.0 0.0
0.0 0.0 5.0
Si
1
Selective
0.0 0.0 0.0
";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_insufficient_atom_lines() {
    let content = "\
Comment
1.0
5.0 0.0 0.0
0.0 5.0 0.0
0.0 0.0 5.0
Si
3
Direct
0.0 0.0 0.0
0.5 0.5 0.5
";
    let result = parse_poscar(content);
    assert!(result.is_err());
}

#[test]
fn test_parse_non_cubic_unit_cell() {
    // Hexagonal unit cell
    let content = "\
Hexagonal cell
1.0
2.5 0.0 0.0
-1.25 2.165 0.0
0.0 0.0 4.0
Si
1
Direct
0.0 0.0 0.0
";
    let (unit_cell, motif) = parse_poscar(content).unwrap();

    // Check basis vectors
    assert!((unit_cell.a.x - 2.5).abs() < 1e-6);
    assert!((unit_cell.b.x - (-1.25)).abs() < 1e-6);
    assert!((unit_cell.b.y - 2.165).abs() < 1e-6);
    assert!((unit_cell.c.z - 4.0).abs() < 1e-6);

    // Gamma angle should be ~120 degrees for hexagonal
    assert!((unit_cell.cell_angle_gamma - 120.0).abs() < 0.1);

    assert_eq!(motif.sites.len(), 1);
}

#[test]
fn test_bonds_by_site_index_vectors() {
    let (_, motif) = parse_poscar(SILICON_POSCAR).unwrap();

    // bonds_by_site*_index should have the same length as sites
    assert_eq!(motif.bonds_by_site1_index.len(), motif.sites.len());
    assert_eq!(motif.bonds_by_site2_index.len(), motif.sites.len());

    // All should be empty since POSCAR has no bond information
    for v in &motif.bonds_by_site1_index {
        assert!(v.is_empty());
    }
    for v in &motif.bonds_by_site2_index {
        assert!(v.is_empty());
    }
}

#[test]
fn test_file_load_nonexistent() {
    use rust_lib_flutter_cad::crystolecule::io::poscar_loader::load_poscar;
    let result = load_poscar("/nonexistent/path/to/file.poscar");
    assert!(result.is_err());
}
