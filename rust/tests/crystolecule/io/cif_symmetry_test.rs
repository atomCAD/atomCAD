use glam::DVec3;
use rust_lib_flutter_cad::crystolecule::io::cif::symmetry::{
    CifAtomSite, expand_asymmetric_unit, parse_symmetry_operation,
};

// --- Parsing individual symmetry operation strings ---

#[test]
fn parse_identity() {
    let op = parse_symmetry_operation("x,y,z").unwrap();
    assert_row(&op.rows[0], 1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_negation_with_translation() {
    // -x+1/2,-y,z+1/2
    let op = parse_symmetry_operation("-x+1/2,-y,z+1/2").unwrap();
    assert_row(&op.rows[0], -1.0, 0.0, 0.0, 0.5);
    assert_row(&op.rows[1], 0.0, -1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.5);
}

#[test]
fn parse_mixed_variables() {
    // 1/4+y,1/4-x,3/4+z
    let op = parse_symmetry_operation("1/4+y,1/4-x,3/4+z").unwrap();
    assert_row(&op.rows[0], 0.0, 1.0, 0.0, 0.25);
    assert_row(&op.rows[1], -1.0, 0.0, 0.0, 0.25);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.75);
}

#[test]
fn parse_with_spaces() {
    let op = parse_symmetry_operation("-x + 1/2, y, -z + 1/2").unwrap();
    assert_row(&op.rows[0], -1.0, 0.0, 0.0, 0.5);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, -1.0, 0.5);
}

#[test]
fn parse_uppercase() {
    let op = parse_symmetry_operation("X,Y,Z").unwrap();
    assert_row(&op.rows[0], 1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_plus_prefix() {
    let op = parse_symmetry_operation("+X,+Y,+Z").unwrap();
    assert_row(&op.rows[0], 1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_decimal_translation() {
    let op = parse_symmetry_operation("-x+0.5,-y,z+0.5").unwrap();
    assert_row(&op.rows[0], -1.0, 0.0, 0.0, 0.5);
    assert_row(&op.rows[1], 0.0, -1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.5);
}

#[test]
fn parse_translation_before_variable() {
    // 1/2+x,y,1/2+z
    let op = parse_symmetry_operation("1/2+x,y,1/2+z").unwrap();
    assert_row(&op.rows[0], 1.0, 0.0, 0.0, 0.5);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.5);
}

#[test]
fn parse_abc_variables() {
    let op = parse_symmetry_operation("a,b,c").unwrap();
    assert_row(&op.rows[0], 1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_underscores_stripped() {
    let op = parse_symmetry_operation("x,_y,_z").unwrap();
    assert_row(&op.rows[0], 1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_explicit_coefficient_star() {
    // 2*x,y,z (rare but should work)
    let op = parse_symmetry_operation("2*x,y,z").unwrap();
    assert_row(&op.rows[0], 2.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_variable_divided_by_integer() {
    // x/3,y,z (rare)
    let op = parse_symmetry_operation("x/3,y,z").unwrap();
    assert_approx(op.rows[0][0], 1.0 / 3.0);
    assert_row(&op.rows[1], 0.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_complex_diamond_op() {
    // 3/4+z,3/4-x,1/4+y — from diamond.cif
    let op = parse_symmetry_operation("3/4+z,3/4-x,1/4+y").unwrap();
    assert_row(&op.rows[0], 0.0, 0.0, 1.0, 0.75);
    assert_row(&op.rows[1], -1.0, 0.0, 0.0, 0.75);
    assert_row(&op.rows[2], 0.0, 1.0, 0.0, 0.25);
}

#[test]
fn parse_negative_no_explicit_sign() {
    // -y,1/2+z,1/2-x
    let op = parse_symmetry_operation("-y,1/2+z,1/2-x").unwrap();
    assert_row(&op.rows[0], 0.0, -1.0, 0.0, 0.0);
    assert_row(&op.rows[1], 0.0, 0.0, 1.0, 0.5);
    assert_row(&op.rows[2], -1.0, 0.0, 0.0, 0.5);
}

#[test]
fn parse_hexagonal_op() {
    // x-y,x,1/2+z — from wurtzite
    let op = parse_symmetry_operation("x-y,x,1/2+z").unwrap();
    assert_row(&op.rows[0], 1.0, -1.0, 0.0, 0.0);
    assert_row(&op.rows[1], 1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.5);
}

#[test]
fn parse_negative_sum_of_variables() {
    // -x+y,-x,z — common hexagonal op
    let op = parse_symmetry_operation("-x+y,-x,z").unwrap();
    assert_row(&op.rows[0], -1.0, 1.0, 0.0, 0.0);
    assert_row(&op.rows[1], -1.0, 0.0, 0.0, 0.0);
    assert_row(&op.rows[2], 0.0, 0.0, 1.0, 0.0);
}

#[test]
fn parse_error_wrong_component_count() {
    let err = parse_symmetry_operation("x,y").unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("3 comma-separated"), "Got: {}", msg);
}

#[test]
fn parse_error_empty_string() {
    let err = parse_symmetry_operation("").unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("3 comma-separated") || msg.contains("Empty"),
        "Got: {}",
        msg
    );
}

// --- apply() tests ---

#[test]
fn apply_identity() {
    let op = parse_symmetry_operation("x,y,z").unwrap();
    let result = op.apply(DVec3::new(0.25, 0.5, 0.75));
    assert_vec3_approx(result, DVec3::new(0.25, 0.5, 0.75));
}

#[test]
fn apply_wraps_negative() {
    // -x,-y,-z applied to (0.25, 0.25, 0.25) → (-0.25,-0.25,-0.25) → (0.75, 0.75, 0.75)
    let op = parse_symmetry_operation("-x,-y,-z").unwrap();
    let result = op.apply(DVec3::new(0.25, 0.25, 0.25));
    assert_vec3_approx(result, DVec3::new(0.75, 0.75, 0.75));
}

#[test]
fn apply_wraps_over_one() {
    // x+1/2 applied to (0.75) → 1.25 → 0.25
    let op = parse_symmetry_operation("x+1/2,y,z").unwrap();
    let result = op.apply(DVec3::new(0.75, 0.0, 0.0));
    assert_vec3_approx(result, DVec3::new(0.25, 0.0, 0.0));
}

// --- Asymmetric unit expansion: Diamond ---

#[test]
fn expand_diamond_2_to_8_atoms() {
    let diamond_cif = std::fs::read_to_string("tests/fixtures/cif/diamond.cif").unwrap();
    let doc = rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif(&diamond_cif).unwrap();
    let block = &doc.data_blocks[0];

    // Parse symmetry operations
    let sym_loop = block.find_loop("_space_group_symop_operation_xyz").unwrap();
    let ops: Vec<_> = sym_loop
        .column_values("_space_group_symop_operation_xyz")
        .unwrap()
        .iter()
        .map(|s| parse_symmetry_operation(s).unwrap())
        .collect();
    assert_eq!(ops.len(), 192);

    // Asymmetric unit: 1 atom at (0,0,0)
    // Diamond Fd-3m origin choice 1: asymmetric unit is C at (0,0,0)
    // The second atom C at (0.25,0.25,0.25) is generated by symmetry
    let asym_atoms = vec![CifAtomSite {
        label: "C".to_string(),
        element: "C".to_string(),
        fract: DVec3::new(0.0, 0.0, 0.0),
        occupancy: 1.0,
    }];

    let expanded = expand_asymmetric_unit(&asym_atoms, &ops, 0.01);

    // Diamond conventional cell has 8 atoms
    assert_eq!(
        expanded.len(),
        8,
        "Expected 8 atoms in diamond unit cell, got {}",
        expanded.len()
    );

    // Verify all atoms are Carbon
    assert!(expanded.iter().all(|a| a.element == "C"));

    // Verify expected positions (sorted for comparison)
    let expected_positions = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(0.0, 0.5, 0.5),
        DVec3::new(0.25, 0.25, 0.25),
        DVec3::new(0.25, 0.75, 0.75),
        DVec3::new(0.5, 0.0, 0.5),
        DVec3::new(0.5, 0.5, 0.0),
        DVec3::new(0.75, 0.25, 0.75),
        DVec3::new(0.75, 0.75, 0.25),
    ];

    for expected in &expected_positions {
        let found = expanded
            .iter()
            .any(|a| fract_close(a.fract, *expected, 0.01));
        assert!(
            found,
            "Expected atom at ({:.4}, {:.4}, {:.4}) not found in expanded positions",
            expected.x, expected.y, expected.z
        );
    }
}

// --- Asymmetric unit expansion: NaCl ---

#[test]
fn expand_nacl_2_to_8_atoms() {
    let nacl_cif = std::fs::read_to_string("tests/fixtures/cif/nacl.cif").unwrap();
    let doc = rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif(&nacl_cif).unwrap();
    let block = &doc.data_blocks[0];

    let sym_loop = block.find_loop("_symmetry_equiv_pos_as_xyz").unwrap();
    let ops: Vec<_> = sym_loop
        .column_values("_symmetry_equiv_pos_as_xyz")
        .unwrap()
        .iter()
        .map(|s| parse_symmetry_operation(s).unwrap())
        .collect();
    assert_eq!(ops.len(), 192);

    // NaCl asymmetric unit: Na at (0,0,0), Cl at (0.5,0.5,0.5)
    let asym_atoms = vec![
        CifAtomSite {
            label: "Na1".to_string(),
            element: "Na".to_string(),
            fract: DVec3::new(0.0, 0.0, 0.0),
            occupancy: 1.0,
        },
        CifAtomSite {
            label: "Cl1".to_string(),
            element: "Cl".to_string(),
            fract: DVec3::new(0.5, 0.5, 0.5),
            occupancy: 1.0,
        },
    ];

    let expanded = expand_asymmetric_unit(&asym_atoms, &ops, 0.01);

    // NaCl Fm-3m: 4 Na + 4 Cl = 8 atoms in the conventional cell
    let na_count = expanded.iter().filter(|a| a.element == "Na").count();
    let cl_count = expanded.iter().filter(|a| a.element == "Cl").count();
    assert_eq!(na_count, 4, "Expected 4 Na atoms, got {}", na_count);
    assert_eq!(cl_count, 4, "Expected 4 Cl atoms, got {}", cl_count);
    assert_eq!(expanded.len(), 8);

    // Na positions: (0,0,0), (0.5,0.5,0), (0.5,0,0.5), (0,0.5,0.5)
    let expected_na = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(0.0, 0.5, 0.5),
        DVec3::new(0.5, 0.0, 0.5),
        DVec3::new(0.5, 0.5, 0.0),
    ];
    for pos in &expected_na {
        let found = expanded
            .iter()
            .any(|a| a.element == "Na" && fract_close(a.fract, *pos, 0.01));
        assert!(found, "Expected Na at {:?} not found", pos);
    }

    // Cl positions: (0.5,0.5,0.5), (0,0,0.5), (0,0.5,0), (0.5,0,0)
    let expected_cl = vec![
        DVec3::new(0.5, 0.5, 0.5),
        DVec3::new(0.0, 0.0, 0.5),
        DVec3::new(0.0, 0.5, 0.0),
        DVec3::new(0.5, 0.0, 0.0),
    ];
    for pos in &expected_cl {
        let found = expanded
            .iter()
            .any(|a| a.element == "Cl" && fract_close(a.fract, *pos, 0.01));
        assert!(found, "Expected Cl at {:?} not found", pos);
    }
}

// --- Deduplication of atoms on special positions ---

#[test]
fn deduplication_on_special_position() {
    // An atom at the origin is invariant under many operations.
    // With identity and inversion, (0,0,0) maps to itself.
    let ops = vec![
        parse_symmetry_operation("x,y,z").unwrap(),
        parse_symmetry_operation("-x,-y,-z").unwrap(),
    ];

    let atoms = vec![CifAtomSite {
        label: "Fe".to_string(),
        element: "Fe".to_string(),
        fract: DVec3::new(0.0, 0.0, 0.0),
        occupancy: 1.0,
    }];

    let expanded = expand_asymmetric_unit(&atoms, &ops, 0.01);
    assert_eq!(expanded.len(), 1, "Atom at origin should not be duplicated");
}

#[test]
fn deduplication_at_cell_boundary() {
    // An atom near (0,0,0) that maps to (1.0, 0, 0) via translation should be deduplicated.
    // (1.0 wraps to 0.0)
    let ops = vec![
        parse_symmetry_operation("x,y,z").unwrap(),
        parse_symmetry_operation("x+1/2,y+1/2,z").unwrap(),
    ];

    let atoms = vec![CifAtomSite {
        label: "A".to_string(),
        element: "A".to_string(),
        fract: DVec3::new(0.5, 0.5, 0.0),
        occupancy: 1.0,
    }];

    // (0.5,0.5,0) + (0.5,0.5,0) = (1.0,1.0,0) → (0,0,0)
    let expanded = expand_asymmetric_unit(&atoms, &ops, 0.01);
    assert_eq!(expanded.len(), 2); // (0.5,0.5,0) and (0.0,0.0,0) are distinct
}

// --- Fractional coordinate wrapping ---

#[test]
fn wrapping_into_0_1() {
    let op = parse_symmetry_operation("-x,-y,-z").unwrap();
    // (0.1, 0.2, 0.3) → (-0.1, -0.2, -0.3) → (0.9, 0.8, 0.7)
    let result = op.apply(DVec3::new(0.1, 0.2, 0.3));
    assert_vec3_approx(result, DVec3::new(0.9, 0.8, 0.7));
}

#[test]
fn wrapping_exact_one() {
    // x+1/2 applied to 0.5 → 1.0 → 0.0
    let op = parse_symmetry_operation("x+1/2,y,z").unwrap();
    let result = op.apply(DVec3::new(0.5, 0.0, 0.0));
    assert!(
        result.x < 0.001 || result.x > 0.999,
        "1.0 should wrap to ~0.0, got {}",
        result.x
    );
}

// --- Wurtzite expansion ---

#[test]
fn expand_wurtzite_zns() {
    let cif = std::fs::read_to_string("tests/fixtures/cif/hexagonal.cif").unwrap();
    let doc = rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif(&cif).unwrap();
    let block = &doc.data_blocks[0];

    let sym_loop = block.find_loop("_space_group_symop_operation_xyz").unwrap();
    let ops: Vec<_> = sym_loop
        .column_values("_space_group_symop_operation_xyz")
        .unwrap()
        .iter()
        .map(|s| parse_symmetry_operation(s).unwrap())
        .collect();
    assert_eq!(ops.len(), 12);

    let asym_atoms = vec![
        CifAtomSite {
            label: "Zn".to_string(),
            element: "Zn".to_string(),
            fract: DVec3::new(0.33333, 0.66667, 0.0),
            occupancy: 1.0,
        },
        CifAtomSite {
            label: "S".to_string(),
            element: "S".to_string(),
            fract: DVec3::new(0.33333, 0.66667, 0.385),
            occupancy: 1.0,
        },
    ];

    let expanded = expand_asymmetric_unit(&asym_atoms, &ops, 0.01);

    // Wurtzite P63mc, Z=2: 2 Zn + 2 S = 4 atoms
    let zn_count = expanded.iter().filter(|a| a.element == "Zn").count();
    let s_count = expanded.iter().filter(|a| a.element == "S").count();
    assert_eq!(zn_count, 2, "Expected 2 Zn atoms, got {}", zn_count);
    assert_eq!(s_count, 2, "Expected 2 S atoms, got {}", s_count);
    assert_eq!(expanded.len(), 4);
}

// --- Parse all 192 diamond symmetry operations ---

#[test]
fn parse_all_diamond_symmetry_operations() {
    let cif = std::fs::read_to_string("tests/fixtures/cif/diamond.cif").unwrap();
    let doc = rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif(&cif).unwrap();
    let block = &doc.data_blocks[0];

    let sym_loop = block.find_loop("_space_group_symop_operation_xyz").unwrap();
    let op_strings = sym_loop
        .column_values("_space_group_symop_operation_xyz")
        .unwrap();

    for (i, s) in op_strings.iter().enumerate() {
        let result = parse_symmetry_operation(s);
        assert!(
            result.is_ok(),
            "Failed to parse diamond op #{}: '{}' — {:?}",
            i + 1,
            s,
            result.err()
        );
    }
}

// --- Parse all 192 NaCl symmetry operations ---

#[test]
fn parse_all_nacl_symmetry_operations() {
    let cif = std::fs::read_to_string("tests/fixtures/cif/nacl.cif").unwrap();
    let doc = rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif(&cif).unwrap();
    let block = &doc.data_blocks[0];

    let sym_loop = block.find_loop("_symmetry_equiv_pos_as_xyz").unwrap();
    let op_strings = sym_loop
        .column_values("_symmetry_equiv_pos_as_xyz")
        .unwrap();

    for (i, s) in op_strings.iter().enumerate() {
        let result = parse_symmetry_operation(s);
        assert!(
            result.is_ok(),
            "Failed to parse NaCl op #{}: '{}' — {:?}",
            i + 1,
            s,
            result.err()
        );
    }
}

// --- Parse all wurtzite symmetry operations ---

#[test]
fn parse_all_wurtzite_symmetry_operations() {
    let cif = std::fs::read_to_string("tests/fixtures/cif/hexagonal.cif").unwrap();
    let doc = rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif(&cif).unwrap();
    let block = &doc.data_blocks[0];

    let sym_loop = block.find_loop("_space_group_symop_operation_xyz").unwrap();
    let op_strings = sym_loop
        .column_values("_space_group_symop_operation_xyz")
        .unwrap();

    for (i, s) in op_strings.iter().enumerate() {
        let result = parse_symmetry_operation(s);
        assert!(
            result.is_ok(),
            "Failed to parse wurtzite op #{}: '{}' — {:?}",
            i + 1,
            s,
            result.err()
        );
    }
}

// --- Helpers ---

fn assert_row(row: &[f64; 4], cx: f64, cy: f64, cz: f64, t: f64) {
    assert_approx(row[0], cx);
    assert_approx(row[1], cy);
    assert_approx(row[2], cz);
    assert_approx(row[3], t);
}

fn assert_approx(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10,
        "Expected {}, got {}",
        expected,
        actual
    );
}

fn assert_vec3_approx(actual: DVec3, expected: DVec3) {
    assert!(
        (actual.x - expected.x).abs() < 1e-6
            && (actual.y - expected.y).abs() < 1e-6
            && (actual.z - expected.z).abs() < 1e-6,
        "Expected ({:.6}, {:.6}, {:.6}), got ({:.6}, {:.6}, {:.6})",
        expected.x,
        expected.y,
        expected.z,
        actual.x,
        actual.y,
        actual.z
    );
}

fn fract_close(a: DVec3, b: DVec3, tol: f64) -> bool {
    let dx = min_fract_diff(a.x, b.x);
    let dy = min_fract_diff(a.y, b.y);
    let dz = min_fract_diff(a.z, b.z);
    (dx * dx + dy * dy + dz * dz).sqrt() < tol
}

fn min_fract_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    if d > 0.5 { 1.0 - d } else { d }
}
