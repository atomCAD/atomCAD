use rust_lib_flutter_cad::crystolecule::io::cif::parser::parse_cif;

#[test]
fn parse_minimal_data_block() {
    let cif = "data_test\n_cell_length_a 5.0\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks.len(), 1);
    assert_eq!(doc.data_blocks[0].name, "test");
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_a"), Some("5.0"));
}

#[test]
fn tags_are_case_insensitive() {
    let cif = "data_x\n_Cell_Length_A 5.0\n";
    let doc = parse_cif(cif).unwrap();
    // Tags stored lowercase
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_a"), Some("5.0"));
    assert_eq!(doc.data_blocks[0].get_tag("_CELL_LENGTH_A"), Some("5.0"));
}

#[test]
fn strip_numeric_uncertainty() {
    let cif = "data_x\n_cell_length_a 5.4307(2)\n_cell_angle_beta 92.5140(10)\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_a"), Some("5.4307"));
    assert_eq!(
        doc.data_blocks[0].get_tag("_cell_angle_beta"),
        Some("92.5140")
    );
}

#[test]
fn uncertainty_not_stripped_from_non_numeric() {
    let cif = "data_x\n_tag abc(2)\n";
    let doc = parse_cif(cif).unwrap();
    // Not numeric before paren, so not stripped
    assert_eq!(doc.data_blocks[0].get_tag("_tag"), Some("abc(2)"));
}

#[test]
fn single_quoted_string() {
    let cif = "data_x\n_name 'Sodium chloride'\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_name"), Some("Sodium chloride"));
}

#[test]
fn double_quoted_string() {
    let cif = "data_x\n_name \"F d -3 m\"\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_name"), Some("F d -3 m"));
}

#[test]
fn semicolon_text_field() {
    let cif = "\
data_x
_publ_section_title
;
 Second edition. Interscience Publishers, New York, New York
;
_cell_length_a 3.567
";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(
        doc.data_blocks[0].get_tag("_publ_section_title"),
        Some("Second edition. Interscience Publishers, New York, New York")
    );
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_a"), Some("3.567"));
}

#[test]
fn semicolon_text_field_multiline() {
    let cif = "\
data_x
_description
;
Line one
Line two
Line three
;
";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(
        doc.data_blocks[0].get_tag("_description"),
        Some("Line one\nLine two\nLine three")
    );
}

#[test]
fn comments_are_stripped() {
    let cif = "\
# This is a comment
data_x
# Another comment
_cell_length_a 5.0 # inline comment
_cell_length_b 6.0
";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_a"), Some("5.0"));
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_b"), Some("6.0"));
}

#[test]
fn null_values_dot_and_question() {
    let cif = "data_x\n_tag1 .\n_tag2 ?\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_tag1"), Some("."));
    assert_eq!(doc.data_blocks[0].get_tag("_tag2"), Some("?"));
}

#[test]
fn simple_loop() {
    let cif = "\
data_x
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
C 0.00000 0.00000 0.00000
";
    let doc = parse_cif(cif).unwrap();
    let block = &doc.data_blocks[0];
    assert_eq!(block.loops.len(), 1);
    let loop_ = &block.loops[0];
    assert_eq!(loop_.columns.len(), 4);
    assert_eq!(loop_.rows.len(), 1);
    assert_eq!(loop_.rows[0], vec!["C", "0.00000", "0.00000", "0.00000"]);
}

#[test]
fn loop_multiple_rows() {
    let cif = "\
data_x
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Na1 Na 0.0 0.0 0.0
Cl1 Cl 0.5 0.5 0.5
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    assert_eq!(loop_.rows.len(), 2);
    assert_eq!(loop_.rows[0][0], "Na1");
    assert_eq!(loop_.rows[0][1], "Na");
    assert_eq!(loop_.rows[1][0], "Cl1");
    assert_eq!(loop_.rows[1][1], "Cl");
}

#[test]
fn loop_with_quoted_values() {
    let cif = "\
data_x
loop_
_publ_author_name
'Abrahams, S C'
'Bernstein, J L'
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    assert_eq!(loop_.rows.len(), 2);
    assert_eq!(loop_.rows[0][0], "Abrahams, S C");
    assert_eq!(loop_.rows[1][0], "Bernstein, J L");
}

#[test]
fn multiple_data_blocks() {
    let cif = "\
data_diamond
_chemical_name_common Diamond
_cell_length_a 3.567

data_nacl
_chemical_name_common 'Sodium chloride'
_cell_length_a 5.62
";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks.len(), 2);
    assert_eq!(doc.data_blocks[0].name, "diamond");
    assert_eq!(
        doc.data_blocks[0].get_tag("_chemical_name_common"),
        Some("Diamond")
    );
    assert_eq!(doc.data_blocks[1].name, "nacl");
    assert_eq!(
        doc.data_blocks[1].get_tag("_chemical_name_common"),
        Some("Sodium chloride")
    );
}

#[test]
fn find_loop_by_tag() {
    let cif = "\
data_x
loop_
_symmetry_equiv_pos_as_xyz
x,y,z
-x,-y,z
loop_
_atom_site_label
_atom_site_fract_x
C 0.0
";
    let doc = parse_cif(cif).unwrap();
    let block = &doc.data_blocks[0];

    let sym_loop = block.find_loop("_symmetry_equiv_pos_as_xyz").unwrap();
    assert_eq!(sym_loop.rows.len(), 2);
    assert_eq!(sym_loop.rows[0][0], "x,y,z");

    let atom_loop = block.find_loop("_atom_site_label").unwrap();
    assert_eq!(atom_loop.rows.len(), 1);
}

#[test]
fn column_values_helper() {
    let cif = "\
data_x
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
Na1 0.0 0.0
Cl1 0.5 0.5
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    let labels = loop_.column_values("_atom_site_label").unwrap();
    assert_eq!(labels, vec!["Na1", "Cl1"]);
    let xs = loop_.column_values("_atom_site_fract_x").unwrap();
    assert_eq!(xs, vec!["0.0", "0.5"]);
}

#[test]
fn multiple_loops_in_one_block() {
    let cif = "\
data_x
loop_
_symmetry_equiv_pos_as_xyz
x,y,z
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
C 0.0 0.0 0.0
loop_
_cod_related_entry_id
_cod_related_entry_database
1 AMCSD
";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].loops.len(), 3);
}

#[test]
fn mixed_tags_and_loops() {
    let cif = "\
data_x
_cell_length_a 3.567
_cell_length_b 3.567
loop_
_atom_site_label
C
_cell_angle_alpha 90
";
    let doc = parse_cif(cif).unwrap();
    let block = &doc.data_blocks[0];
    assert_eq!(block.get_tag("_cell_length_a"), Some("3.567"));
    assert_eq!(block.get_tag("_cell_angle_alpha"), Some("90"));
    assert_eq!(block.loops.len(), 1);
    assert_eq!(block.loops[0].rows[0][0], "C");
}

#[test]
fn uncertainty_stripping_in_loop_values() {
    let cif = "\
data_x
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
O1 1.02958(6) 0.38866(6) 0.24177(2)
";
    let doc = parse_cif(cif).unwrap();
    let row = &doc.data_blocks[0].loops[0].rows[0];
    assert_eq!(row[0], "O1"); // label not affected
    assert_eq!(row[1], "1.02958");
    assert_eq!(row[2], "0.38866");
    assert_eq!(row[3], "0.24177");
}

#[test]
fn data_block_name_preserved_case() {
    let cif = "data_9008564\n_cell_length_a 3.567\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].name, "9008564");
}

#[test]
fn empty_input() {
    let doc = parse_cif("").unwrap();
    assert!(doc.data_blocks.is_empty());
}

#[test]
fn comments_only() {
    let doc = parse_cif("# just a comment\n# another one\n").unwrap();
    assert!(doc.data_blocks.is_empty());
}

#[test]
fn loop_with_symmetry_operations() {
    let cif = "\
data_x
loop_
_space_group_symop_operation_xyz
x,y,z
-x,-y,z
x-y,x,1/2+z
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    assert_eq!(loop_.rows.len(), 3);
    assert_eq!(loop_.rows[0][0], "x,y,z");
    assert_eq!(loop_.rows[1][0], "-x,-y,z");
    assert_eq!(loop_.rows[2][0], "x-y,x,1/2+z");
}

#[test]
fn plus_prefix_in_symmetry_ops() {
    // Some CIF files use +X,+Y,+Z notation
    let cif = "\
data_x
loop_
_symmetry_equiv_pos_as_xyz
+X,+Y,+Z
-X,1/2+Y,1/2-Z
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    assert_eq!(loop_.rows[0][0], "+X,+Y,+Z");
    assert_eq!(loop_.rows[1][0], "-X,1/2+Y,1/2-Z");
}

#[test]
fn occupancy_values_with_dot() {
    // Values like "0." and "1." are valid CIF numbers
    let cif = "\
data_x
loop_
_atom_site_label
_atom_site_occupancy
Na1 1.
Cl1 0.
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    assert_eq!(loop_.rows[0][1], "1.");
    assert_eq!(loop_.rows[1][1], "0.");
}

#[test]
fn loop_with_many_columns() {
    // Test atom site loop with many columns (like NaCl fixture)
    let cif = "\
data_x
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_symmetry_multiplicity
_atom_site_Wyckoff_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
_atom_site_attached_hydrogens
_atom_site_calc_flag
Na1 Na1+ 4 a 0. 0. 0. 1. 0 d
Cl1 Cl1- 4 b 0.5 0.5 0.5 1. 0 d
";
    let doc = parse_cif(cif).unwrap();
    let loop_ = &doc.data_blocks[0].loops[0];
    assert_eq!(loop_.columns.len(), 10);
    assert_eq!(loop_.rows.len(), 2);
    assert_eq!(loop_.rows[0][0], "Na1");
    assert_eq!(loop_.rows[0][1], "Na1+");
    assert_eq!(loop_.rows[1][0], "Cl1");
    assert_eq!(loop_.rows[1][1], "Cl1-");
}

#[test]
fn parse_diamond_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/cif/diamond.cif").unwrap();
    let doc = parse_cif(&content).unwrap();
    assert_eq!(doc.data_blocks.len(), 1);
    let block = &doc.data_blocks[0];
    assert_eq!(block.name, "9008564");
    assert_eq!(block.get_tag("_cell_length_a"), Some("3.56679"));
    assert_eq!(block.get_tag("_cell_angle_alpha"), Some("90"));
    assert_eq!(block.get_tag("_space_group_it_number"), Some("227"));

    // Should have symmetry operations loop and atom site loop
    let sym_loop = block.find_loop("_space_group_symop_operation_xyz").unwrap();
    assert_eq!(sym_loop.rows.len(), 192);

    let atom_loop = block.find_loop("_atom_site_label").unwrap();
    assert_eq!(atom_loop.rows.len(), 1);
    assert_eq!(atom_loop.rows[0][0], "C");
}

#[test]
fn parse_nacl_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/cif/nacl.cif").unwrap();
    let doc = parse_cif(&content).unwrap();
    assert_eq!(doc.data_blocks.len(), 1);
    let block = &doc.data_blocks[0];
    assert_eq!(block.get_tag("_cell_length_a"), Some("5.62"));
    assert_eq!(
        block.get_tag("_symmetry_space_group_name_h-m"),
        Some("F m -3 m")
    );

    let sym_loop = block.find_loop("_symmetry_equiv_pos_as_xyz").unwrap();
    assert_eq!(sym_loop.rows.len(), 192);

    let atom_loop = block.find_loop("_atom_site_label").unwrap();
    assert_eq!(atom_loop.rows.len(), 2);
}

#[test]
fn parse_hexagonal_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/cif/hexagonal.cif").unwrap();
    let doc = parse_cif(&content).unwrap();
    let block = &doc.data_blocks[0];
    assert_eq!(block.get_tag("_cell_angle_gamma"), Some("120"));
    assert_eq!(block.get_tag("_cell_length_a"), Some("3.811"));

    let sym_loop = block.find_loop("_space_group_symop_operation_xyz").unwrap();
    assert_eq!(sym_loop.rows.len(), 12);

    let atom_loop = block.find_loop("_atom_site_label").unwrap();
    assert_eq!(atom_loop.rows.len(), 2);
    assert_eq!(atom_loop.rows[0][0], "Zn");
    assert_eq!(atom_loop.rows[1][0], "S");
}

#[test]
fn parse_multi_block_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/cif/multi_block.cif").unwrap();
    let doc = parse_cif(&content).unwrap();
    assert_eq!(doc.data_blocks.len(), 2);
    assert_eq!(doc.data_blocks[0].name, "diamond");
    assert_eq!(doc.data_blocks[1].name, "nacl");

    // Diamond block
    assert_eq!(
        doc.data_blocks[0].get_tag("_chemical_name_common"),
        Some("Diamond")
    );
    let diamond_atoms = doc.data_blocks[0].find_loop("_atom_site_label").unwrap();
    assert_eq!(diamond_atoms.rows.len(), 2); // C1 and C2

    // NaCl block
    assert_eq!(
        doc.data_blocks[1].get_tag("_chemical_name_common"),
        Some("Sodium chloride")
    );
    let nacl_atoms = doc.data_blocks[1].find_loop("_atom_site_label").unwrap();
    assert_eq!(nacl_atoms.rows.len(), 2); // Na1 and Cl1
}

#[test]
fn parse_with_bonds_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/cif/with_bonds.cif").unwrap();
    let doc = parse_cif(&content).unwrap();
    assert_eq!(doc.data_blocks.len(), 1);
    let block = &doc.data_blocks[0];

    // Should have uncertainty stripped from cell parameters
    assert_eq!(block.get_tag("_cell_length_a"), Some("8.3670"));
    assert_eq!(block.get_tag("_cell_angle_beta"), Some("92.5140"));

    // Should have atom sites
    let atom_loop = block.find_loop("_atom_site_label").unwrap();
    assert!(atom_loop.rows.len() > 20); // 21 heavy atoms + hydrogens + dummies

    // Should have bond data
    let bond_loop = block
        .find_loop("_geom_bond_atom_site_label_1")
        .unwrap();
    assert!(bond_loop.rows.len() > 30);

    // Verify uncertainty stripped from fractional coords in atom loop
    let fract_x_idx = atom_loop.column_index("_atom_site_fract_x").unwrap();
    let first_x = &atom_loop.rows[0][fract_x_idx];
    assert_eq!(first_x, "1.02958"); // was 1.02958(6)

    // Verify uncertainty stripped from bond distances
    let dist_idx = bond_loop
        .column_index("_geom_bond_distance")
        .unwrap();
    let first_dist = &bond_loop.rows[0][dist_idx];
    assert_eq!(first_dist, "1.3431"); // was 1.3431(8)
}

#[test]
fn quoted_string_with_embedded_quote() {
    // In CIF, a closing quote must be followed by whitespace or end of line.
    // So 'it''s' — the inner ' followed by non-whitespace is not a close.
    // The value is: it''s (the raw characters between outer quotes).
    let cif = "data_x\n_tag 'it''s'\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_tag"), Some("it''s"));
}

#[test]
fn tag_missing_from_block() {
    let cif = "data_x\n_cell_length_a 5.0\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(doc.data_blocks[0].get_tag("_nonexistent_tag"), None);
}

#[test]
fn find_loop_missing() {
    let cif = "data_x\n_cell_length_a 5.0\n";
    let doc = parse_cif(cif).unwrap();
    assert!(doc.data_blocks[0].find_loop("_nonexistent").is_none());
}

#[test]
fn backslash_in_value() {
    // CIF radiation type often has backslash: MoK\a
    let cif = "data_x\n_diffrn_radiation_type MoK\\a\n";
    let doc = parse_cif(cif).unwrap();
    assert_eq!(
        doc.data_blocks[0].get_tag("_diffrn_radiation_type"),
        Some("MoK\\a")
    );
}

#[test]
fn semicolon_depositor_comments() {
    // Test the long depositor comments from with_bonds.cif
    let cif = "\
data_x
_cod_depositor_comments
;
 Marking atoms as dummy.

 Author,
 2017-02-19
;
_cell_length_a 5.0
";
    let doc = parse_cif(cif).unwrap();
    let comment = doc.data_blocks[0]
        .get_tag("_cod_depositor_comments")
        .unwrap();
    assert!(comment.contains("Marking atoms as dummy."));
    assert!(comment.contains("2017-02-19"));
    assert_eq!(doc.data_blocks[0].get_tag("_cell_length_a"), Some("5.0"));
}
