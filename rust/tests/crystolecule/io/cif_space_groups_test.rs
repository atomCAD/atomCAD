use rust_lib_flutter_cad::crystolecule::io::cif::space_groups::{
    lookup_symmetry_operations_by_hm, lookup_symmetry_operations_by_number,
};

// --- Lookup by number ---

#[test]
fn lookup_sg1_triclinic() {
    let ops = lookup_symmetry_operations_by_number(1).unwrap();
    assert_eq!(ops.len(), 1); // P 1: only identity
}

#[test]
fn lookup_sg2_inversion() {
    let ops = lookup_symmetry_operations_by_number(2).unwrap();
    assert_eq!(ops.len(), 2); // P -1: identity + inversion
}

#[test]
fn lookup_sg225_nacl_fm3m() {
    let ops = lookup_symmetry_operations_by_number(225).unwrap();
    assert_eq!(ops.len(), 192); // Fm-3m: 192 operations
}

#[test]
fn lookup_sg227_diamond_fd3m() {
    let ops = lookup_symmetry_operations_by_number(227).unwrap();
    assert_eq!(ops.len(), 192); // Fd-3m: 192 operations
}

#[test]
fn lookup_sg230_last() {
    let ops = lookup_symmetry_operations_by_number(230).unwrap();
    assert!(!ops.is_empty()); // Ia-3d should have operations
}

#[test]
fn lookup_sg0_out_of_range() {
    assert!(lookup_symmetry_operations_by_number(0).is_err());
}

#[test]
fn lookup_sg231_out_of_range() {
    assert!(lookup_symmetry_operations_by_number(231).is_err());
}

// --- Lookup by Hermann-Mauguin symbol ---

#[test]
fn lookup_hm_p1() {
    let ops = lookup_symmetry_operations_by_hm("P 1").unwrap();
    assert_eq!(ops.len(), 1);
}

#[test]
fn lookup_hm_fm3m() {
    let ops = lookup_symmetry_operations_by_hm("F m -3 m").unwrap();
    assert_eq!(ops.len(), 192);
}

#[test]
fn lookup_hm_fd3m() {
    let ops = lookup_symmetry_operations_by_hm("F d -3 m").unwrap();
    assert_eq!(ops.len(), 192);
}

#[test]
fn lookup_hm_case_insensitive() {
    let ops = lookup_symmetry_operations_by_hm("f d -3 m").unwrap();
    assert_eq!(ops.len(), 192);
}

#[test]
fn lookup_hm_no_spaces() {
    let ops = lookup_symmetry_operations_by_hm("Fd-3m").unwrap();
    assert_eq!(ops.len(), 192);
}

#[test]
fn lookup_hm_unknown() {
    assert!(lookup_symmetry_operations_by_hm("X Y Z").is_err());
}

// --- Verify symmetry operations produce correct results ---

#[test]
fn sg227_diamond_expansion() {
    use glam::DVec3;
    use rust_lib_flutter_cad::crystolecule::io::cif::symmetry::{
        CifAtomSite, expand_asymmetric_unit,
    };

    let ops = lookup_symmetry_operations_by_number(227).unwrap();

    // Diamond asymmetric unit: 2 atoms
    let asymmetric = vec![
        CifAtomSite {
            label: "C1".to_string(),
            element: "C".to_string(),
            fract: DVec3::new(0.0, 0.0, 0.0),
            occupancy: 1.0,
        },
        CifAtomSite {
            label: "C2".to_string(),
            element: "C".to_string(),
            fract: DVec3::new(0.25, 0.25, 0.25),
            occupancy: 1.0,
        },
    ];

    let expanded = expand_asymmetric_unit(&asymmetric, &ops, 0.01);
    assert_eq!(
        expanded.len(),
        8,
        "Diamond should have 8 atoms in the conventional cell"
    );
}

#[test]
fn sg225_nacl_expansion() {
    use glam::DVec3;
    use rust_lib_flutter_cad::crystolecule::io::cif::symmetry::{
        CifAtomSite, expand_asymmetric_unit,
    };

    let ops = lookup_symmetry_operations_by_number(225).unwrap();

    // NaCl asymmetric unit
    let asymmetric = vec![
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

    let expanded = expand_asymmetric_unit(&asymmetric, &ops, 0.01);
    assert_eq!(
        expanded.len(),
        8,
        "NaCl should have 8 atoms in the conventional cell"
    );
}

// --- Integration: CIF file with only space group number (no explicit ops) ---

#[test]
fn load_cif_with_space_group_number_only() {
    use rust_lib_flutter_cad::crystolecule::io::cif::load_cif_from_str;

    // Diamond CIF with space group number but NO explicit symmetry operations
    let cif = r#"data_diamond_sg_only
_cell_length_a 3.567
_cell_length_b 3.567
_cell_length_c 3.567
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
_symmetry_Int_Tables_number 227
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
C1 C 0.0 0.0 0.0
C2 C 0.25 0.25 0.25
"#;

    let result = load_cif_from_str(cif, None).unwrap();
    assert_eq!(
        result.atoms.len(),
        8,
        "Should expand to 8 atoms via SG 227 lookup"
    );
}

#[test]
fn load_cif_with_hm_name_only() {
    use rust_lib_flutter_cad::crystolecule::io::cif::load_cif_from_str;

    // NaCl CIF with HM name but NO explicit symmetry operations
    let cif = r#"data_nacl_hm_only
_cell_length_a 5.64
_cell_length_b 5.64
_cell_length_c 5.64
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
_symmetry_space_group_name_H-M 'F m -3 m'
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Na1 Na 0.0 0.0 0.0
Cl1 Cl 0.5 0.5 0.5
"#;

    let result = load_cif_from_str(cif, None).unwrap();
    assert_eq!(
        result.atoms.len(),
        8,
        "Should expand to 8 atoms via HM lookup"
    );
}

#[test]
fn load_cif_with_new_style_sg_number() {
    use rust_lib_flutter_cad::crystolecule::io::cif::load_cif_from_str;

    // Uses new-style _space_group_IT_number tag
    let cif = r#"data_test
_cell_length_a 3.567
_cell_length_b 3.567
_cell_length_c 3.567
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
_space_group_IT_number 227
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
C1 C 0.0 0.0 0.0
C2 C 0.25 0.25 0.25
"#;

    let result = load_cif_from_str(cif, None).unwrap();
    assert_eq!(result.atoms.len(), 8);
}

#[test]
fn load_cif_with_new_style_hm_alt() {
    use rust_lib_flutter_cad::crystolecule::io::cif::load_cif_from_str;

    // Uses new-style _space_group_name_H-M_alt tag
    let cif = r#"data_test
_cell_length_a 5.64
_cell_length_b 5.64
_cell_length_c 5.64
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
_space_group_name_H-M_alt 'F m -3 m'
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Na1 Na 0.0 0.0 0.0
Cl1 Cl 0.5 0.5 0.5
"#;

    let result = load_cif_from_str(cif, None).unwrap();
    assert_eq!(result.atoms.len(), 8);
}

#[test]
fn explicit_symops_still_preferred_over_lookup() {
    use rust_lib_flutter_cad::crystolecule::io::cif::load_cif_from_str;

    // CIF with both explicit symops (only identity) AND a space group number.
    // Explicit ops should take precedence — no expansion beyond the 2 asymmetric atoms.
    let cif = r#"data_test
_cell_length_a 3.567
_cell_length_b 3.567
_cell_length_c 3.567
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
_symmetry_Int_Tables_number 227
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
C2 C 0.25 0.25 0.25
"#;

    let result = load_cif_from_str(cif, None).unwrap();
    // Only identity operation → 2 atoms (no expansion)
    assert_eq!(result.atoms.len(), 2);
}
