use rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif;
use rust_lib_flutter_cad::crystolecule::io::cif::structure::{
    extract_crystal_data, parse_symmetry_code, ParsedSymmetryCode,
};

fn fixture_path(name: &str) -> String {
    format!(
        "{}/tests/fixtures/cif/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    )
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).unwrap()
}

// --- Unit cell extraction ---

#[test]
fn diamond_unit_cell() {
    let doc = parse_cif(&load_fixture("diamond.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    let uc = &data.unit_cell;
    assert!((uc.cell_length_a - 3.56679).abs() < 1e-4);
    assert!((uc.cell_length_b - 3.56679).abs() < 1e-4);
    assert!((uc.cell_length_c - 3.56679).abs() < 1e-4);
    assert!((uc.cell_angle_alpha - 90.0).abs() < 1e-6);
    assert!((uc.cell_angle_beta - 90.0).abs() < 1e-6);
    assert!((uc.cell_angle_gamma - 90.0).abs() < 1e-6);
}

#[test]
fn nacl_unit_cell() {
    let doc = parse_cif(&load_fixture("nacl.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    let uc = &data.unit_cell;
    assert!((uc.cell_length_a - 5.62).abs() < 1e-4);
    assert!((uc.cell_length_b - 5.62).abs() < 1e-4);
    assert!((uc.cell_length_c - 5.62).abs() < 1e-4);
    assert!((uc.cell_angle_alpha - 90.0).abs() < 1e-6);
}

#[test]
fn hexagonal_unit_cell() {
    let doc = parse_cif(&load_fixture("hexagonal.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    let uc = &data.unit_cell;
    assert!((uc.cell_length_a - 3.811).abs() < 1e-4);
    assert!((uc.cell_length_b - 3.811).abs() < 1e-4);
    assert!((uc.cell_length_c - 6.234).abs() < 1e-4);
    assert!((uc.cell_angle_alpha - 90.0).abs() < 1e-6);
    assert!((uc.cell_angle_beta - 90.0).abs() < 1e-6);
    assert!((uc.cell_angle_gamma - 120.0).abs() < 1e-6);

    // Verify basis vectors for hexagonal cell:
    // a along x, b at 120° in xy-plane
    assert!((uc.a.x - 3.811).abs() < 1e-4);
    assert!(uc.a.y.abs() < 1e-10);
    assert!(uc.a.z.abs() < 1e-10);

    // b.x = b * cos(120°) = -b/2
    assert!((uc.b.x - (-3.811 / 2.0)).abs() < 1e-4);
    // b.y = b * sin(120°) = b * sqrt(3)/2
    assert!((uc.b.y - 3.811 * (3.0_f64).sqrt() / 2.0).abs() < 1e-4);

    // c along z
    assert!(uc.c.x.abs() < 1e-10);
    assert!(uc.c.y.abs() < 1e-10);
    assert!((uc.c.z - 6.234).abs() < 1e-4);
}

// --- Atom site extraction ---

#[test]
fn diamond_asymmetric_atoms() {
    let doc = parse_cif(&load_fixture("diamond.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // Diamond CIF has 1 asymmetric atom (C at origin)
    assert_eq!(data.asymmetric_atoms.len(), 1);
    assert_eq!(data.asymmetric_atoms[0].element, "C");
    assert!((data.asymmetric_atoms[0].fract.x).abs() < 1e-10);
    assert!((data.asymmetric_atoms[0].fract.y).abs() < 1e-10);
    assert!((data.asymmetric_atoms[0].fract.z).abs() < 1e-10);
}

#[test]
fn nacl_asymmetric_atoms() {
    let doc = parse_cif(&load_fixture("nacl.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // NaCl has 2 asymmetric atoms
    assert_eq!(data.asymmetric_atoms.len(), 2);

    // Na at origin
    assert_eq!(data.asymmetric_atoms[0].element, "Na");
    assert!((data.asymmetric_atoms[0].fract.x).abs() < 1e-10);

    // Cl at (0.5, 0.5, 0.5)
    assert_eq!(data.asymmetric_atoms[1].element, "Cl");
    assert!((data.asymmetric_atoms[1].fract.x - 0.5).abs() < 1e-10);
    assert!((data.asymmetric_atoms[1].fract.y - 0.5).abs() < 1e-10);
    assert!((data.asymmetric_atoms[1].fract.z - 0.5).abs() < 1e-10);
}

#[test]
fn nacl_element_parsed_from_type_symbol_with_charge() {
    // NaCl CIF has _atom_site_type_symbol values like "Na1+" and "Cl1-"
    let doc = parse_cif(&load_fixture("nacl.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    assert_eq!(data.asymmetric_atoms[0].element, "Na");
    assert_eq!(data.asymmetric_atoms[1].element, "Cl");
}

#[test]
fn hexagonal_asymmetric_atoms() {
    let doc = parse_cif(&load_fixture("hexagonal.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // Wurtzite has 2 asymmetric atoms: Zn and S
    assert_eq!(data.asymmetric_atoms.len(), 2);
    assert_eq!(data.asymmetric_atoms[0].element, "Zn");
    assert_eq!(data.asymmetric_atoms[1].element, "S");

    // Zn at (1/3, 2/3, 0)
    assert!((data.asymmetric_atoms[0].fract.x - 0.33333).abs() < 1e-4);
    assert!((data.asymmetric_atoms[0].fract.y - 0.66667).abs() < 1e-4);

    // S at (1/3, 2/3, 0.385)
    assert!((data.asymmetric_atoms[1].fract.z - 0.385).abs() < 1e-4);
}

#[test]
fn with_bonds_atom_sites_skip_dummy() {
    let doc = parse_cif(&load_fixture("with_bonds.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // The with_bonds.cif has real atoms (O, C, H) plus DUM atoms that should be skipped
    // Real atoms: 5 O + 21 C + 18 H = 44
    let element_counts: std::collections::HashMap<&str, usize> =
        data.asymmetric_atoms.iter().fold(
            std::collections::HashMap::new(),
            |mut acc, atom| {
                *acc.entry(atom.element.as_str()).or_insert(0) += 1;
                acc
            },
        );

    assert_eq!(element_counts.get("O"), Some(&5));
    assert_eq!(element_counts.get("C"), Some(&21));
    assert_eq!(element_counts.get("H"), Some(&18));

    // No dummy atoms should be present
    assert!(!data
        .asymmetric_atoms
        .iter()
        .any(|a| a.label.starts_with("DUM")));
}

// --- Symmetry operations ---

#[test]
fn diamond_symmetry_operations_count() {
    let doc = parse_cif(&load_fixture("diamond.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // Diamond Fd-3m has 192 symmetry operations
    assert_eq!(data.symmetry_operations.len(), 192);
}

#[test]
fn nacl_symmetry_operations_count() {
    let doc = parse_cif(&load_fixture("nacl.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // NaCl Fm-3m has 192 symmetry operations
    assert_eq!(data.symmetry_operations.len(), 192);
}

#[test]
fn hexagonal_symmetry_operations_count() {
    let doc = parse_cif(&load_fixture("hexagonal.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // Wurtzite P63mc has 12 symmetry operations
    assert_eq!(data.symmetry_operations.len(), 12);
}

#[test]
fn new_tag_name_used_for_diamond_symops() {
    // diamond.cif uses _space_group_symop_operation_xyz (new style)
    let doc = parse_cif(&load_fixture("diamond.cif")).unwrap();
    let block = &doc.data_blocks[0];

    // Verify the new-style tag is present
    assert!(block
        .find_loop("_space_group_symop_operation_xyz")
        .is_some());

    // extract_crystal_data should still work
    let data = extract_crystal_data(block).unwrap();
    assert!(!data.symmetry_operations.is_empty());
}

#[test]
fn old_tag_name_used_for_nacl_symops() {
    // nacl.cif uses _symmetry_equiv_pos_as_xyz (old style)
    let doc = parse_cif(&load_fixture("nacl.cif")).unwrap();
    let block = &doc.data_blocks[0];

    // Verify the old-style tag is present
    assert!(block
        .find_loop("_symmetry_equiv_pos_as_xyz")
        .is_some());

    // extract_crystal_data should still work
    let data = extract_crystal_data(block).unwrap();
    assert!(!data.symmetry_operations.is_empty());
}

// --- Bond extraction ---

#[test]
fn with_bonds_extracts_bonds() {
    let doc = parse_cif(&load_fixture("with_bonds.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // The CIF file has 46 bonds in the _geom_bond loop
    assert_eq!(data.bonds.len(), 46);
}

#[test]
fn with_bonds_first_bond_properties() {
    let doc = parse_cif(&load_fixture("with_bonds.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    let bond = &data.bonds[0];
    assert_eq!(bond.atom_label_1, "O(1)");
    assert_eq!(bond.atom_label_2, "C(7)");
    assert!((bond.distance - 1.3431).abs() < 1e-3);
    // Default bond order (no _ccdc_geom_bond_type in this file)
    assert_eq!(bond.bond_order, 1);
}

#[test]
fn with_bonds_symmetry_codes() {
    let doc = parse_cif(&load_fixture("with_bonds.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // All bonds in with_bonds.cif use 1_555 symmetry codes
    // which are equivalent to same-cell (no translation)
    for bond in &data.bonds {
        // 1_555 means "identity operation, no translation"
        if let Some(ref code) = bond.symmetry_code_1 {
            assert_eq!(code, "1_555");
        }
        if let Some(ref code) = bond.symmetry_code_2 {
            assert_eq!(code, "1_555");
        }
    }
}

#[test]
fn no_bonds_in_diamond() {
    let doc = parse_cif(&load_fixture("diamond.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // Diamond CIF from COD has no _geom_bond data
    assert!(data.bonds.is_empty());
}

// --- Symmetry code parsing ---

#[test]
fn parse_symmetry_code_identity() {
    let code = parse_symmetry_code("1_555").unwrap();
    assert_eq!(
        code,
        ParsedSymmetryCode {
            symop_index: 1,
            translation: glam::IVec3::new(0, 0, 0),
        }
    );
}

#[test]
fn parse_symmetry_code_with_translation() {
    let code = parse_symmetry_code("2_655").unwrap();
    assert_eq!(
        code,
        ParsedSymmetryCode {
            symop_index: 2,
            translation: glam::IVec3::new(1, 0, 0),
        }
    );
}

#[test]
fn parse_symmetry_code_negative_translation() {
    let code = parse_symmetry_code("3_454").unwrap();
    assert_eq!(
        code,
        ParsedSymmetryCode {
            symop_index: 3,
            translation: glam::IVec3::new(-1, 0, -1),
        }
    );
}

#[test]
fn parse_symmetry_code_invalid() {
    assert!(parse_symmetry_code(".").is_none());
    assert!(parse_symmetry_code("abc").is_none());
    assert!(parse_symmetry_code("1_55").is_none()); // too few digits
}

// --- Multi-block ---

#[test]
fn multi_block_first_block() {
    let doc = parse_cif(&load_fixture("multi_block.cif")).unwrap();
    assert_eq!(doc.data_blocks.len(), 2);

    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();
    // First block is diamond
    assert_eq!(data.asymmetric_atoms.len(), 2);
    assert_eq!(data.asymmetric_atoms[0].element, "C");
    assert_eq!(data.asymmetric_atoms[1].element, "C");
}

#[test]
fn multi_block_second_block() {
    let doc = parse_cif(&load_fixture("multi_block.cif")).unwrap();

    let data = extract_crystal_data(&doc.data_blocks[1]).unwrap();
    // Second block is NaCl
    assert_eq!(data.asymmetric_atoms.len(), 2);
    assert_eq!(data.asymmetric_atoms[0].element, "Na");
    assert_eq!(data.asymmetric_atoms[1].element, "Cl");
}

// --- Monoclinic unit cell (with_bonds.cif) ---

#[test]
fn monoclinic_unit_cell() {
    let doc = parse_cif(&load_fixture("with_bonds.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    let uc = &data.unit_cell;
    assert!((uc.cell_length_a - 8.3670).abs() < 1e-3);
    assert!((uc.cell_length_b - 8.4724).abs() < 1e-3);
    assert!((uc.cell_length_c - 23.6852).abs() < 1e-3);
    assert!((uc.cell_angle_alpha - 90.0).abs() < 1e-2);
    assert!((uc.cell_angle_beta - 92.514).abs() < 1e-2);
    assert!((uc.cell_angle_gamma - 90.0).abs() < 1e-2);
}

// --- Occupancy ---

#[test]
fn nacl_occupancy_values() {
    let doc = parse_cif(&load_fixture("nacl.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    for atom in &data.asymmetric_atoms {
        assert!((atom.occupancy - 1.0).abs() < 1e-10);
    }
}

// --- Element parsing edge cases ---

#[test]
fn with_bonds_old_style_symops() {
    // with_bonds.cif uses _symmetry_equiv_pos_as_xyz (old style)
    let doc = parse_cif(&load_fixture("with_bonds.cif")).unwrap();
    let data = extract_crystal_data(&doc.data_blocks[0]).unwrap();

    // P 1 21/c 1 has 4 symmetry operations
    assert_eq!(data.symmetry_operations.len(), 4);
}
