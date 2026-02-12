use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::text_format::{
    parse_diff_text, serialize_diff,
};
use std::collections::HashMap;

// =============================================================================
// serialize_diff tests
// =============================================================================

#[test]
fn test_serialize_empty_diff() {
    let diff = AtomicStructure::new_diff();
    let text = serialize_diff(&diff);
    assert!(text.is_empty());
}

#[test]
fn test_serialize_single_addition() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0)); // Carbon

    let text = serialize_diff(&diff);
    assert_eq!(text, "+C @ (1.0, 2.0, 3.0)");
}

#[test]
fn test_serialize_delete_marker() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(4.0, 5.0, 6.0));

    let text = serialize_diff(&diff);
    assert_eq!(text, "- @ (4.0, 5.0, 6.0)");
}

#[test]
fn test_serialize_move_with_anchor() {
    let mut diff = AtomicStructure::new_diff();
    let id = diff.add_atom(14, DVec3::new(7.0, 8.0, 9.0)); // Silicon
    diff.set_anchor_position(id, DVec3::new(7.0, 8.5, 9.0));

    let text = serialize_diff(&diff);
    assert_eq!(text, "~Si @ (7.0, 8.0, 9.0) [from (7.0, 8.5, 9.0)]");
}

#[test]
fn test_serialize_multiple_atoms() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(6, DVec3::new(1.0, 0.0, 0.0)); // C
    diff.add_atom(7, DVec3::new(2.0, 0.0, 0.0)); // N
    diff.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(3.0, 0.0, 0.0));

    let text = serialize_diff(&diff);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "+C @ (1.0, 0.0, 0.0)");
    assert_eq!(lines[1], "+N @ (2.0, 0.0, 0.0)");
    assert_eq!(lines[2], "- @ (3.0, 0.0, 0.0)");
}

#[test]
fn test_serialize_with_bonds() {
    let mut diff = AtomicStructure::new_diff();
    let a = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // C
    let b = diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0)); // C
    diff.add_bond(a, b, BOND_SINGLE);

    let text = serialize_diff(&diff);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "+C @ (0.0, 0.0, 0.0)");
    assert_eq!(lines[1], "+C @ (1.5, 0.0, 0.0)");
    assert_eq!(lines[2], "bond 1-2 single");
}

#[test]
fn test_serialize_with_bond_delete_marker() {
    let mut diff = AtomicStructure::new_diff();
    let a = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = diff.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    diff.add_bond(a, b, BOND_DELETED);

    let text = serialize_diff(&diff);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[2], "unbond 1-2");
}

#[test]
fn test_serialize_double_bond() {
    let mut diff = AtomicStructure::new_diff();
    let a = diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = diff.add_atom(8, DVec3::new(1.2, 0.0, 0.0)); // Oxygen
    diff.add_bond(a, b, BOND_DOUBLE);

    let text = serialize_diff(&diff);
    assert!(text.contains("bond 1-2 double"));
}

#[test]
fn test_serialize_triple_bond() {
    let mut diff = AtomicStructure::new_diff();
    let a = diff.add_atom(7, DVec3::new(0.0, 0.0, 0.0)); // N
    let b = diff.add_atom(7, DVec3::new(1.1, 0.0, 0.0)); // N
    diff.add_bond(a, b, BOND_TRIPLE);

    let text = serialize_diff(&diff);
    assert!(text.contains("bond 1-2 triple"));
}

// =============================================================================
// parse_diff_text tests
// =============================================================================

#[test]
fn test_parse_empty() {
    let diff = parse_diff_text("").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 0);
    assert!(diff.is_diff());
}

#[test]
fn test_parse_whitespace_only() {
    let diff = parse_diff_text("   \n  \n  ").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 0);
}

#[test]
fn test_parse_comments() {
    let diff = parse_diff_text("# This is a comment\n# Another comment").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 0);
}

#[test]
fn test_parse_single_addition() {
    let diff = parse_diff_text("+C @ (1.0, 2.0, 3.0)").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 1);

    let (_, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 6); // Carbon
    assert!((atom.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn test_parse_delete_marker() {
    let diff = parse_diff_text("- @ (4.0, 5.0, 6.0)").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 1);

    let (_, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!((atom.position - DVec3::new(4.0, 5.0, 6.0)).length() < 1e-10);
}

#[test]
fn test_parse_modification_with_anchor() {
    let diff = parse_diff_text("~Si @ (7.0, 8.0, 9.0) [from (7.0, 8.5, 9.0)]").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 1);

    let (&atom_id, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 14); // Silicon
    assert!((atom.position - DVec3::new(7.0, 8.0, 9.0)).length() < 1e-10);

    let anchor = diff.anchor_position(atom_id).unwrap();
    assert!((anchor - DVec3::new(7.0, 8.5, 9.0)).length() < 1e-10);
}

#[test]
fn test_parse_modification_without_anchor() {
    // ~El @ pos without [from ...] sets a self-anchor (anchor == position)
    let diff = parse_diff_text("~N @ (1.0, 2.0, 3.0)").unwrap();
    assert_eq!(diff.get_num_of_atoms(), 1);

    let (&atom_id, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 7); // Nitrogen
    assert!(diff.has_anchor_position(atom_id));
    let anchor = diff.anchor_position(atom_id).unwrap();
    // Self-anchor: anchor == position
    assert!((anchor - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn test_parse_bond() {
    let text = "+C @ (0.0, 0.0, 0.0)\n+C @ (1.5, 0.0, 0.0)\nbond 1-2 single";
    let diff = parse_diff_text(text).unwrap();
    assert_eq!(diff.get_num_of_atoms(), 2);

    // Check that a bond exists between the two atoms
    let atom_ids: Vec<u32> = diff.iter_atoms().map(|(id, _)| *id).collect();
    let atom1 = diff.get_atom(atom_ids[0]).unwrap();
    assert_eq!(atom1.bonds.len(), 1);
    assert_eq!(atom1.bonds[0].bond_order(), BOND_SINGLE);
}

#[test]
fn test_parse_unbond() {
    let text = "+C @ (0.0, 0.0, 0.0)\n+C @ (1.5, 0.0, 0.0)\nunbond 1-2";
    let diff = parse_diff_text(text).unwrap();

    let atom_ids: Vec<u32> = diff.iter_atoms().map(|(id, _)| *id).collect();
    let atom1 = diff.get_atom(atom_ids[0]).unwrap();
    assert_eq!(atom1.bonds.len(), 1);
    assert_eq!(atom1.bonds[0].bond_order(), BOND_DELETED);
}

#[test]
fn test_parse_double_bond() {
    let text = "+C @ (0.0, 0.0, 0.0)\n+O @ (1.2, 0.0, 0.0)\nbond 1-2 double";
    let diff = parse_diff_text(text).unwrap();

    let atom_ids: Vec<u32> = diff.iter_atoms().map(|(id, _)| *id).collect();
    let atom1 = diff.get_atom(atom_ids[0]).unwrap();
    assert_eq!(atom1.bonds[0].bond_order(), BOND_DOUBLE);
}

#[test]
fn test_parse_case_insensitive_elements() {
    // Lowercase
    let diff = parse_diff_text("+c @ (0.0, 0.0, 0.0)").unwrap();
    let (_, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 6);

    // Uppercase
    let diff = parse_diff_text("+SI @ (0.0, 0.0, 0.0)").unwrap();
    let (_, atom) = diff.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 14);
}

#[test]
fn test_parse_numeric_bond_order() {
    let text = "+C @ (0.0, 0.0, 0.0)\n+C @ (1.5, 0.0, 0.0)\nbond 1-2 2";
    let diff = parse_diff_text(text).unwrap();

    let atom_ids: Vec<u32> = diff.iter_atoms().map(|(id, _)| *id).collect();
    let atom1 = diff.get_atom(atom_ids[0]).unwrap();
    assert_eq!(atom1.bonds[0].bond_order(), BOND_DOUBLE);
}

// =============================================================================
// Round-trip tests
// =============================================================================

#[test]
fn test_roundtrip_empty() {
    let original = AtomicStructure::new_diff();
    let text = serialize_diff(&original);
    assert!(text.is_empty());
    // parse_diff_text("") should also return empty diff
    let restored = parse_diff_text("").unwrap();
    assert_eq!(restored.get_num_of_atoms(), 0);
    assert!(restored.is_diff());
}

#[test]
fn test_roundtrip_additions() {
    let mut original = AtomicStructure::new_diff();
    original.add_atom(6, DVec3::new(1.0, 2.0, 3.0)); // C
    original.add_atom(7, DVec3::new(4.0, 5.0, 6.0)); // N
    original.add_atom(8, DVec3::new(7.0, 8.0, 9.0)); // O

    let text = serialize_diff(&original);
    let restored = parse_diff_text(&text).unwrap();

    assert_eq!(restored.get_num_of_atoms(), 3);
    assert!(restored.is_diff());

    // Verify atoms match (order preserved)
    let orig_atoms: Vec<_> = original.iter_atoms().collect();
    let rest_atoms: Vec<_> = restored.iter_atoms().collect();

    for i in 0..3 {
        assert_eq!(orig_atoms[i].1.atomic_number, rest_atoms[i].1.atomic_number);
        assert!((orig_atoms[i].1.position - rest_atoms[i].1.position).length() < 1e-10);
    }
}

#[test]
fn test_roundtrip_deletion() {
    let mut original = AtomicStructure::new_diff();
    original.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(1.0, 2.0, 3.0));

    let text = serialize_diff(&original);
    let restored = parse_diff_text(&text).unwrap();

    assert_eq!(restored.get_num_of_atoms(), 1);
    let (_, atom) = restored.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, DELETED_SITE_ATOMIC_NUMBER);
    assert!((atom.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn test_roundtrip_move_with_anchor() {
    let mut original = AtomicStructure::new_diff();
    let id = original.add_atom(14, DVec3::new(7.0, 8.0, 9.0));
    original.set_anchor_position(id, DVec3::new(7.0, 8.5, 9.0));

    let text = serialize_diff(&original);
    let restored = parse_diff_text(&text).unwrap();

    assert_eq!(restored.get_num_of_atoms(), 1);
    let (&atom_id, atom) = restored.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 14);
    assert!((atom.position - DVec3::new(7.0, 8.0, 9.0)).length() < 1e-10);
    let anchor = restored.anchor_position(atom_id).unwrap();
    assert!((anchor - DVec3::new(7.0, 8.5, 9.0)).length() < 1e-10);
}

#[test]
fn test_roundtrip_with_bonds() {
    let mut original = AtomicStructure::new_diff();
    let a = original.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = original.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    let c = original.add_atom(8, DVec3::new(3.0, 0.0, 0.0));
    original.add_bond(a, b, BOND_SINGLE);
    original.add_bond(b, c, BOND_DOUBLE);

    let text = serialize_diff(&original);
    let restored = parse_diff_text(&text).unwrap();

    assert_eq!(restored.get_num_of_atoms(), 3);

    // Verify bonds
    let atom_ids: Vec<u32> = {
        let mut ids: Vec<u32> = restored.iter_atoms().map(|(id, _)| *id).collect();
        ids.sort();
        ids
    };

    let atom_a = restored.get_atom(atom_ids[0]).unwrap();
    let atom_b = restored.get_atom(atom_ids[1]).unwrap();

    // Atom A should have 1 bond (to B, single)
    assert_eq!(atom_a.bonds.len(), 1);
    assert_eq!(atom_a.bonds[0].bond_order(), BOND_SINGLE);

    // Atom B should have 2 bonds (to A single, to C double)
    assert_eq!(atom_b.bonds.len(), 2);
}

#[test]
fn test_roundtrip_with_unbond() {
    let mut original = AtomicStructure::new_diff();
    let a = original.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = original.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    original.add_bond(a, b, BOND_DELETED);

    let text = serialize_diff(&original);
    let restored = parse_diff_text(&text).unwrap();

    let atom_ids: Vec<u32> = {
        let mut ids: Vec<u32> = restored.iter_atoms().map(|(id, _)| *id).collect();
        ids.sort();
        ids
    };

    let atom_a = restored.get_atom(atom_ids[0]).unwrap();
    assert_eq!(atom_a.bonds.len(), 1);
    assert_eq!(atom_a.bonds[0].bond_order(), BOND_DELETED);
}

#[test]
fn test_roundtrip_complex_diff() {
    let mut original = AtomicStructure::new_diff();
    // Add a carbon (addition)
    let c = original.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    // Add a nitrogen (addition)
    let n = original.add_atom(7, DVec3::new(1.5, 0.0, 0.0));
    // Delete marker
    original.add_atom(DELETED_SITE_ATOMIC_NUMBER, DVec3::new(3.0, 0.0, 0.0));
    // Moved silicon with anchor
    let si = original.add_atom(14, DVec3::new(5.0, 1.0, 0.0));
    original.set_anchor_position(si, DVec3::new(5.0, 0.0, 0.0));
    // Bond between C and N
    original.add_bond(c, n, BOND_SINGLE);
    // Bond delete marker between C and Si
    original.add_bond(c, si, BOND_DELETED);

    let text = serialize_diff(&original);
    let restored = parse_diff_text(&text).unwrap();

    assert_eq!(restored.get_num_of_atoms(), 4);
    assert!(restored.is_diff());

    // Verify atom types
    let atoms: Vec<_> = {
        let mut a: Vec<_> = restored.iter_atoms().map(|(_, atom)| atom).collect();
        a.sort_by_key(|a| a.id);
        a
    };
    assert_eq!(atoms[0].atomic_number, 6); // C
    assert_eq!(atoms[1].atomic_number, 7); // N
    assert_eq!(atoms[2].atomic_number, DELETED_SITE_ATOMIC_NUMBER); // delete
    assert_eq!(atoms[3].atomic_number, 14); // Si

    // Verify anchor on Si
    assert!(restored.has_anchor_position(atoms[3].id));
}

// =============================================================================
// NodeData text properties tests
// =============================================================================

#[test]
fn test_get_text_properties_empty_diff() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let data = AtomEditData::new();
    let props = data.get_text_properties();

    // All properties always emitted, even with defaults
    let prop_map: HashMap<String, &TextValue> = props.iter().map(|(k, v)| (k.clone(), v)).collect();
    assert_eq!(*prop_map["diff"], TextValue::String("".to_string()));
    assert_eq!(*prop_map["output_diff"], TextValue::Bool(false));
    assert_eq!(*prop_map["show_anchor_arrows"], TextValue::Bool(false));
    assert_eq!(*prop_map["tolerance"], TextValue::Float(0.1));
}

#[test]
fn test_get_text_properties_with_diff() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = AtomEditData::new();
    data.diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0));

    let props = data.get_text_properties();
    let prop_map: HashMap<String, &TextValue> = props.iter().map(|(k, v)| (k.clone(), v)).collect();

    if let TextValue::String(text) = prop_map["diff"] {
        assert_eq!(text, "+C @ (1.0, 2.0, 3.0)");
    } else {
        panic!("Expected String TextValue for diff");
    }
}

#[test]
fn test_get_text_properties_with_config() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = AtomEditData::new();
    data.output_diff = true;
    data.show_anchor_arrows = true;
    data.tolerance = 0.5;

    let props = data.get_text_properties();

    let prop_map: HashMap<String, &TextValue> = props.iter().map(|(k, v)| (k.clone(), v)).collect();

    assert_eq!(*prop_map["output_diff"], TextValue::Bool(true));
    assert_eq!(*prop_map["show_anchor_arrows"], TextValue::Bool(true));
    assert_eq!(*prop_map["tolerance"], TextValue::Float(0.5));
}

#[test]
fn test_set_text_properties_diff() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = AtomEditData::new();

    let mut props = HashMap::new();
    props.insert(
        "diff".to_string(),
        TextValue::String("+C @ (1.0, 2.0, 3.0)\n+N @ (4.0, 5.0, 6.0)".to_string()),
    );

    data.set_text_properties(&props).unwrap();

    assert_eq!(data.diff.get_num_of_atoms(), 2);
    assert!(data.diff.is_diff());
}

#[test]
fn test_set_text_properties_config() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = AtomEditData::new();

    let mut props = HashMap::new();
    props.insert("output_diff".to_string(), TextValue::Bool(true));
    props.insert("show_anchor_arrows".to_string(), TextValue::Bool(true));
    props.insert("tolerance".to_string(), TextValue::Float(0.5));

    data.set_text_properties(&props).unwrap();

    assert!(data.output_diff);
    assert!(data.show_anchor_arrows);
    assert!((data.tolerance - 0.5).abs() < 1e-10);
}

#[test]
fn test_set_text_properties_empty_diff_string() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = AtomEditData::new();
    data.diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(data.diff.get_num_of_atoms(), 1);

    // Setting empty diff text should clear the diff
    let mut props = HashMap::new();
    props.insert("diff".to_string(), TextValue::String("".to_string()));
    data.set_text_properties(&props).unwrap();

    assert_eq!(data.diff.get_num_of_atoms(), 0);
    assert!(data.diff.is_diff());
}

#[test]
fn test_set_text_properties_clears_selection() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(1);
    data.selection.selected_diff_atoms.insert(2);

    let mut props = HashMap::new();
    props.insert(
        "diff".to_string(),
        TextValue::String("+C @ (1.0, 2.0, 3.0)".to_string()),
    );
    data.set_text_properties(&props).unwrap();

    assert!(data.selection.selected_base_atoms.is_empty());
    assert!(data.selection.selected_diff_atoms.is_empty());
}

#[test]
fn test_roundtrip_via_text_properties() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
    use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
    use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

    let mut original = AtomEditData::new();
    let c = original.diff.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let n = original.diff.add_atom(7, DVec3::new(1.5, 0.0, 0.0));
    original.diff.add_bond(c, n, BOND_SINGLE);
    original.output_diff = true;
    original.tolerance = 0.2;

    // Serialize via get_text_properties
    let props = original.get_text_properties();

    // Convert to HashMap
    let props_map: HashMap<String, TextValue> = props.into_iter().collect();

    // Deserialize via set_text_properties
    let mut restored = AtomEditData::new();
    restored.set_text_properties(&props_map).unwrap();

    assert_eq!(restored.diff.get_num_of_atoms(), 2);
    assert!(restored.output_diff);
    assert!((restored.tolerance - 0.2).abs() < 1e-10);
}

#[test]
fn test_serialize_replacement_self_anchor() {
    // A self-anchor (anchor == position) should serialize as ~El @ pos (no [from ...])
    let mut diff = AtomicStructure::new_diff();
    let id = diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0)); // Carbon
    diff.set_anchor_position(id, DVec3::new(1.0, 2.0, 3.0)); // Self-anchor

    let text = serialize_diff(&diff);
    assert_eq!(text, "~C @ (1.0, 2.0, 3.0)");
}

#[test]
fn test_roundtrip_replacement_preserves_tilde() {
    // ~C @ pos should round-trip as ~C @ pos (not +C @ pos)
    let text = "~C @ (1.0, 2.0, 3.0)";
    let diff = parse_diff_text(text).unwrap();
    let serialized = serialize_diff(&diff);
    assert_eq!(serialized, "~C @ (1.0, 2.0, 3.0)");
}

#[test]
fn test_roundtrip_addition_preserves_plus() {
    // +C @ pos should round-trip as +C @ pos
    let text = "+C @ (1.0, 2.0, 3.0)";
    let diff = parse_diff_text(text).unwrap();
    let serialized = serialize_diff(&diff);
    assert_eq!(serialized, "+C @ (1.0, 2.0, 3.0)");
}

#[test]
fn test_roundtrip_mixed_plus_and_tilde() {
    // Mixed + and ~ atoms should preserve their prefixes
    let text = "~C @ (0.89175, 2.67525, 2.67525)\n~C @ (1.7835, 1.7835, 3.567)\n+H @ (1.337625, 2.229375, 3.121125)\nbond 1-3 single\nbond 2-3 single";
    let diff = parse_diff_text(text).unwrap();
    let serialized = serialize_diff(&diff);
    let lines: Vec<&str> = serialized.lines().collect();
    assert!(lines[0].starts_with("~C @"));
    assert!(lines[1].starts_with("~C @"));
    assert!(lines[2].starts_with("+H @"));
}

// =============================================================================
// Error handling tests
// =============================================================================

#[test]
fn test_parse_unknown_element() {
    let result = parse_diff_text("+Xx @ (0.0, 0.0, 0.0)");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown element"));
}

#[test]
fn test_parse_invalid_position() {
    let result = parse_diff_text("+C @ (abc, 0.0, 0.0)");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid"));
}

#[test]
fn test_parse_missing_position() {
    let result = parse_diff_text("+C @");
    assert!(result.is_err());
}

#[test]
fn test_parse_unrecognized_line() {
    let result = parse_diff_text("foo bar");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unrecognized"));
}

#[test]
fn test_parse_bond_out_of_range() {
    let result = parse_diff_text("+C @ (0.0, 0.0, 0.0)\nbond 1-5 single");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("out of range"));
}

#[test]
fn test_parse_bond_zero_index() {
    let result = parse_diff_text("+C @ (0.0, 0.0, 0.0)\nbond 0-1 single");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("1-based"));
}

#[test]
fn test_parse_unknown_bond_order() {
    let result = parse_diff_text("+C @ (0.0, 0.0, 0.0)\n+C @ (1.0, 0.0, 0.0)\nbond 1-2 septuple");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown bond order"));
}

#[test]
fn test_parse_delete_missing_at() {
    let result = parse_diff_text("- (0.0, 0.0, 0.0)");
    assert!(result.is_err());
}

#[test]
fn test_parse_position_wrong_components() {
    let result = parse_diff_text("+C @ (1.0, 2.0)");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("3 components"));
}

// =============================================================================
// Edge case tests
// =============================================================================

#[test]
fn test_serialize_negative_coordinates() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(6, DVec3::new(-1.5, -2.5, -3.5));

    let text = serialize_diff(&diff);
    assert_eq!(text, "+C @ (-1.5, -2.5, -3.5)");

    // Round-trip
    let restored = parse_diff_text(&text).unwrap();
    let (_, atom) = restored.iter_atoms().next().unwrap();
    assert!((atom.position - DVec3::new(-1.5, -2.5, -3.5)).length() < 1e-10);
}

#[test]
fn test_serialize_integer_coordinates() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(6, DVec3::new(1.0, 2.0, 3.0));

    let text = serialize_diff(&diff);
    // Should have .0 suffix for integers to distinguish from int type
    assert!(text.contains("1.0"));
    assert!(text.contains("2.0"));
    assert!(text.contains("3.0"));
}

#[test]
fn test_parse_integer_positions() {
    // Parsing integer-like values should work
    let diff = parse_diff_text("+C @ (1, 2, 3)").unwrap();
    let (_, atom) = diff.iter_atoms().next().unwrap();
    assert!((atom.position - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-10);
}

#[test]
fn test_hydrogen_element() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(1, DVec3::new(0.0, 0.0, 0.0));

    let text = serialize_diff(&diff);
    assert_eq!(text, "+H @ (0.0, 0.0, 0.0)");

    let restored = parse_diff_text(&text).unwrap();
    let (_, atom) = restored.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 1);
}

#[test]
fn test_two_letter_element() {
    let mut diff = AtomicStructure::new_diff();
    diff.add_atom(26, DVec3::new(0.0, 0.0, 0.0)); // Iron

    let text = serialize_diff(&diff);
    assert!(text.starts_with("+Fe @"));

    let restored = parse_diff_text(&text).unwrap();
    let (_, atom) = restored.iter_atoms().next().unwrap();
    assert_eq!(atom.atomic_number, 26);
}

#[test]
fn test_mixed_operations_with_comments() {
    let text = r#"
# Add a carbon
+C @ (0.0, 0.0, 0.0)
# Add a nitrogen
+N @ (1.5, 0.0, 0.0)
# Delete atom at this position
- @ (3.0, 0.0, 0.0)

# Bond C-N
bond 1-2 single
"#;

    let diff = parse_diff_text(text).unwrap();
    assert_eq!(diff.get_num_of_atoms(), 3);

    let atom_ids: Vec<u32> = {
        let mut ids: Vec<u32> = diff.iter_atoms().map(|(id, _)| *id).collect();
        ids.sort();
        ids
    };
    let atom1 = diff.get_atom(atom_ids[0]).unwrap();
    assert_eq!(atom1.bonds.len(), 1);
}
