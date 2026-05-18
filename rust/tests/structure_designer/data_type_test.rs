use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

/// All phase types plus a non-phase control (Float). Used to construct the
/// conversion matrix for the phase split.
fn phase_grid_types() -> Vec<DataType> {
    vec![
        DataType::Blueprint,
        DataType::Crystal,
        DataType::Molecule,
        DataType::HasAtoms,
        DataType::HasStructure,
        DataType::HasFreeLinOps,
        DataType::Float,
    ]
}

/// Returns true if `(src, dst)` should be a permitted conversion per the
/// crystal/molecule split design (doc/design_crystal_molecule_split.md §6.1).
fn expected_conversion(src: &DataType, dst: &DataType) -> bool {
    if src == dst {
        // Identity holds for concrete and non-phase types only — abstract types
        // never appear as sources in wire validation, so we still report identity
        // here (the language-level identity rule fires before the abstract check).
        return true;
    }
    matches!(
        (src, dst),
        (DataType::Crystal, DataType::HasAtoms)
            | (DataType::Crystal, DataType::HasStructure)
            | (DataType::Molecule, DataType::HasAtoms)
            | (DataType::Molecule, DataType::HasFreeLinOps)
            | (DataType::Blueprint, DataType::HasStructure)
            | (DataType::Blueprint, DataType::HasFreeLinOps)
    )
}

#[test]
fn phase_conversion_matrix_matches_design_doc() {
    let registry = NodeTypeRegistry::new();
    let types = phase_grid_types();
    for src in &types {
        for dst in &types {
            let actual = DataType::can_be_converted_to(src, dst, &registry);
            let expected = expected_conversion(src, dst);
            assert_eq!(
                actual, expected,
                "can_be_converted_to({:?}, {:?}) = {} but expected {}",
                src, dst, actual, expected
            );
        }
    }
}

#[test]
fn no_abstract_to_concrete_conversions() {
    let registry = NodeTypeRegistry::new();
    let abstracts = [
        DataType::HasAtoms,
        DataType::HasStructure,
        DataType::HasFreeLinOps,
    ];
    let concretes = [DataType::Blueprint, DataType::Crystal, DataType::Molecule];
    for src in &abstracts {
        for dst in &concretes {
            assert!(
                !DataType::can_be_converted_to(src, dst, &registry),
                "abstract {:?} should not convert to concrete {:?}",
                src,
                dst
            );
        }
    }
}

#[test]
fn no_cross_abstract_conversions() {
    let registry = NodeTypeRegistry::new();
    let abstracts = [
        DataType::HasAtoms,
        DataType::HasStructure,
        DataType::HasFreeLinOps,
    ];
    for src in &abstracts {
        for dst in &abstracts {
            if src == dst {
                continue;
            }
            assert!(
                !DataType::can_be_converted_to(src, dst, &registry),
                "cross-abstract conversion {:?} -> {:?} must be rejected",
                src,
                dst
            );
        }
    }
}

#[test]
fn is_abstract_truth_table() {
    assert!(DataType::HasAtoms.is_abstract());
    assert!(DataType::HasStructure.is_abstract());
    assert!(DataType::HasFreeLinOps.is_abstract());

    assert!(!DataType::Blueprint.is_abstract());
    assert!(!DataType::Crystal.is_abstract());
    assert!(!DataType::Molecule.is_abstract());
    assert!(!DataType::Float.is_abstract());
    assert!(!DataType::Int.is_abstract());
    assert!(!DataType::None.is_abstract());
    assert!(!DataType::Structure.is_abstract());
    assert!(!DataType::Motif.is_abstract());
    assert!(!DataType::Array(Box::new(DataType::HasAtoms)).is_abstract());
}

#[test]
fn array_elementwise_conversion_follows_element_rule() {
    let registry = NodeTypeRegistry::new();
    // Each permitted concrete -> abstract upcast must also be permitted
    // when both sides are wrapped in a single Array layer.
    let pairs = [
        (DataType::Crystal, DataType::HasAtoms),
        (DataType::Crystal, DataType::HasStructure),
        (DataType::Molecule, DataType::HasAtoms),
        (DataType::Molecule, DataType::HasFreeLinOps),
        (DataType::Blueprint, DataType::HasStructure),
        (DataType::Blueprint, DataType::HasFreeLinOps),
        // Primitive numeric promotion should also lift through arrays.
        (DataType::Int, DataType::Float),
    ];
    for (src, dst) in &pairs {
        let arr_src = DataType::Array(Box::new(src.clone()));
        let arr_dst = DataType::Array(Box::new(dst.clone()));
        assert!(
            DataType::can_be_converted_to(&arr_src, &arr_dst, &registry),
            "[{:?}] -> [{:?}] should be permitted because {:?} -> {:?} is",
            src,
            dst,
            src,
            dst
        );
    }

    // Nested arrays: [[Molecule]] -> [[HasAtoms]] should also work.
    let nested_src = DataType::Array(Box::new(DataType::Array(Box::new(DataType::Molecule))));
    let nested_dst = DataType::Array(Box::new(DataType::Array(Box::new(DataType::HasAtoms))));
    assert!(DataType::can_be_converted_to(
        &nested_src,
        &nested_dst,
        &registry
    ));
}

#[test]
fn array_elementwise_conversion_rejects_forbidden_element_pairs() {
    let registry = NodeTypeRegistry::new();
    // Abstract -> concrete must not be rescued by array wrapping.
    let arr_abstract = DataType::Array(Box::new(DataType::HasAtoms));
    let arr_concrete = DataType::Array(Box::new(DataType::Crystal));
    assert!(!DataType::can_be_converted_to(
        &arr_abstract,
        &arr_concrete,
        &registry
    ));

    // Cross-abstract upcasts are forbidden, and array wrapping must not
    // rescue them either.
    let arr_has_atoms = DataType::Array(Box::new(DataType::HasAtoms));
    let arr_has_structure = DataType::Array(Box::new(DataType::HasStructure));
    assert!(!DataType::can_be_converted_to(
        &arr_has_atoms,
        &arr_has_structure,
        &registry
    ));

    // An array cannot be narrowed to a single element.
    let arr_molecule = DataType::Array(Box::new(DataType::Molecule));
    assert!(!DataType::can_be_converted_to(
        &arr_molecule,
        &DataType::HasAtoms,
        &registry
    ));
}

#[test]
fn new_type_names_roundtrip_through_string() {
    for name in [
        "Crystal",
        "Molecule",
        "HasStructure",
        "HasFreeLinOps",
        "HasAtoms",
    ] {
        let parsed = DataType::from_string(name).expect("parse");
        assert_eq!(parsed.to_string(), name);
    }
}

// ---------------------------------------------------------------------------
// Record names that aren't bare identifiers: backtick-quoting on Display and
// legacy unquoted paren-blob acceptance on read. Covers .cnnd files saved by
// older builds that wrote record def names like `surface(100)_gemcut_named`
// verbatim into a parameter's `data_type` string.
// ---------------------------------------------------------------------------

#[test]
fn record_name_with_parens_and_digits_displays_with_backticks() {
    let ty = DataType::Record(RecordType::Named("surface(100)_gemcut_named".to_string()));
    assert_eq!(ty.to_string(), "Record(`surface(100)_gemcut_named`)");
}

#[test]
fn record_name_with_leading_digit_displays_with_backticks() {
    let ty = DataType::Record(RecordType::Named("1leading_digit".to_string()));
    assert_eq!(ty.to_string(), "Record(`1leading_digit`)");
}

#[test]
fn simple_record_name_displays_without_backticks() {
    let ty = DataType::Record(RecordType::Named("Foo_2".to_string()));
    assert_eq!(ty.to_string(), "Record(Foo_2)");
}

#[test]
fn backtick_quoted_record_name_roundtrips() {
    let ty = DataType::Record(RecordType::Named("surface(100)_gemcut_named".to_string()));
    let s = ty.to_string();
    let parsed = DataType::from_string(&s).expect("parse");
    assert_eq!(parsed, ty);
}

#[test]
fn legacy_unquoted_record_name_parses() {
    // .cnnd files written by older builds contain this exact byte sequence.
    let parsed = DataType::from_string("Record(surface(100)_gemcut_named)").expect("parse");
    assert_eq!(
        parsed,
        DataType::Record(RecordType::Named("surface(100)_gemcut_named".to_string()))
    );
}

#[test]
fn legacy_unquoted_record_name_inside_iter_parses() {
    let parsed = DataType::from_string("Iter[Record(surface(100)_gemcut_named)]").expect("parse");
    assert_eq!(
        parsed,
        DataType::Iterator(Box::new(DataType::Record(RecordType::Named(
            "surface(100)_gemcut_named".to_string(),
        ))))
    );
}

#[test]
fn legacy_unquoted_record_name_inside_array_parses() {
    let parsed = DataType::from_string("[Record(weird.name)]").expect("parse");
    assert_eq!(
        parsed,
        DataType::Array(Box::new(DataType::Record(RecordType::Named(
            "weird.name".to_string(),
        ))))
    );
}

#[test]
fn record_with_leading_digit_legacy_form_parses() {
    let parsed = DataType::from_string("Record(1foo)").expect("parse");
    assert_eq!(
        parsed,
        DataType::Record(RecordType::Named("1foo".to_string()))
    );
}

#[test]
fn record_legacy_form_round_trips_to_backticked_canonical() {
    // Legacy unquoted form on input, canonical backticked form on output.
    let parsed = DataType::from_string("Record(surface(100)_gemcut_named)").expect("parse");
    assert_eq!(parsed.to_string(), "Record(`surface(100)_gemcut_named`)");
    // And the canonical form parses back to the same value.
    let reparsed = DataType::from_string(&parsed.to_string()).expect("parse");
    assert_eq!(reparsed, parsed);
}

#[test]
fn simple_input_skips_normalization_allocation() {
    // Smoke test: a clean input still parses. (We don't observe Cow allocation
    // directly, but a regression that mangled non-Record inputs would surface
    // as a parse error here.)
    for s in [
        "Int",
        "[Float]",
        "Record(Point)",
        "Iter[Record(Point)]",
        "(Int, Float) => Bool",
    ] {
        DataType::from_string(s).unwrap_or_else(|e| panic!("failed on {s:?}: {e}"));
    }
}
