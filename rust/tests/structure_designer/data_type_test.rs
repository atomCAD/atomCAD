use rust_lib_flutter_cad::structure_designer::data_type::DataType;

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
    let types = phase_grid_types();
    for src in &types {
        for dst in &types {
            let actual = DataType::can_be_converted_to(src, dst);
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
    let abstracts = [
        DataType::HasAtoms,
        DataType::HasStructure,
        DataType::HasFreeLinOps,
    ];
    let concretes = [DataType::Blueprint, DataType::Crystal, DataType::Molecule];
    for src in &abstracts {
        for dst in &concretes {
            assert!(
                !DataType::can_be_converted_to(src, dst),
                "abstract {:?} should not convert to concrete {:?}",
                src,
                dst
            );
        }
    }
}

#[test]
fn no_cross_abstract_conversions() {
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
                !DataType::can_be_converted_to(src, dst),
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
