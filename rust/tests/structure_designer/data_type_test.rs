use rust_lib_flutter_cad::api::structure_designer::structure_designer_api::{
    api_data_type_to_data_type, data_type_to_api_data_type,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIDataType, APIDataTypeBase,
};
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType, RecordType};
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

// --- Structural Function/Iter API round-trip tests ---
// See doc/design_structural_function_and_iter_types.md.

fn roundtrip(original: DataType) -> DataType {
    let api = data_type_to_api_data_type(&original);
    api_data_type_to_data_type(&api).expect("API → Rust conversion succeeded")
}

#[test]
fn iter_int_roundtrip() {
    let original = DataType::Iterator(Box::new(DataType::Int));
    let api = data_type_to_api_data_type(&original);
    assert!(api.data_type_base == APIDataTypeBase::Iter);
    assert!(!api.array);
    assert_eq!(api.children.len(), 1);
    assert!(api.children[0].data_type_base == APIDataTypeBase::Int);
    assert!(roundtrip(original.clone()) == original);
}

#[test]
fn function_arity1_roundtrip() {
    let original = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Float),
    });
    let api = data_type_to_api_data_type(&original);
    assert!(api.data_type_base == APIDataTypeBase::Function);
    assert_eq!(api.children.len(), 2);
    assert!(api.children[0].data_type_base == APIDataTypeBase::Int);
    assert!(api.children[1].data_type_base == APIDataTypeBase::Float);
    assert!(roundtrip(original.clone()) == original);
}

#[test]
fn function_arity0_roundtrip() {
    // A thunk: `() -> Float`. `children = [Float]` (just the return type).
    let original = DataType::Function(FunctionType {
        parameter_types: vec![],
        output_type: Box::new(DataType::Float),
    });
    let api = data_type_to_api_data_type(&original);
    assert!(api.data_type_base == APIDataTypeBase::Function);
    assert_eq!(api.children.len(), 1);
    assert!(api.children[0].data_type_base == APIDataTypeBase::Float);
    assert!(roundtrip(original.clone()) == original);
}

#[test]
fn function_arity3_roundtrip() {
    let original = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int, DataType::Bool, DataType::Vec3],
        output_type: Box::new(DataType::String),
    });
    let api = data_type_to_api_data_type(&original);
    assert!(api.data_type_base == APIDataTypeBase::Function);
    assert_eq!(api.children.len(), 4);
    assert!(api.children[0].data_type_base == APIDataTypeBase::Int);
    assert!(api.children[1].data_type_base == APIDataTypeBase::Bool);
    assert!(api.children[2].data_type_base == APIDataTypeBase::Vec3);
    assert!(api.children[3].data_type_base == APIDataTypeBase::String);
    assert!(roundtrip(original.clone()) == original);
}

#[test]
fn nested_iter_of_function_roundtrip() {
    // `Iter[(Int) -> Float]` — exercises children-of-children recursion.
    let inner_fn = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Float),
    });
    let original = DataType::Iterator(Box::new(inner_fn));
    let api = data_type_to_api_data_type(&original);
    assert!(api.data_type_base == APIDataTypeBase::Iter);
    assert_eq!(api.children.len(), 1);
    assert!(api.children[0].data_type_base == APIDataTypeBase::Function);
    assert_eq!(api.children[0].children.len(), 2);
    assert!(roundtrip(original.clone()) == original);
}

#[test]
fn array_of_iter_roundtrip() {
    // `Array[Iter[Int]]` — outer `array: true` on the API form combines
    // with the structural Iter base.
    let original = DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))));
    let api = data_type_to_api_data_type(&original);
    assert!(api.data_type_base == APIDataTypeBase::Iter);
    assert!(api.array);
    assert_eq!(api.children.len(), 1);
    assert!(api.children[0].data_type_base == APIDataTypeBase::Int);
    assert!(roundtrip(original.clone()) == original);
}

#[test]
fn custom_text_iter_promotes_on_back_conversion() {
    // Start from a `Custom`-base APIDataType carrying the text `"Iter[Int]"`
    // (the legacy escape hatch). After API → Rust → API, the structural
    // Iter variant should win, not `Custom`. This is the "next-paint
    // upgrade" path from §"Custom..." escape hatch interaction" in
    // doc/design_structural_function_and_iter_types.md.
    let starting = APIDataType {
        data_type_base: APIDataTypeBase::Custom,
        custom_data_type: Some("Iter[Int]".to_string()),
        array: false,
        children: vec![],
    };
    let rust = api_data_type_to_data_type(&starting).expect("parses Iter[Int]");
    let promoted = data_type_to_api_data_type(&rust);
    assert!(promoted.data_type_base == APIDataTypeBase::Iter);
    assert!(promoted.custom_data_type.is_none());
    assert_eq!(promoted.children.len(), 1);
    assert!(promoted.children[0].data_type_base == APIDataTypeBase::Int);
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

// --- Function-parameter bracketing in Display (issue #324) ---
// A function-typed *parameter* must be parenthesized because `->` is
// right-associative: without brackets `(Float -> Float) -> Float` would print
// as `Float -> Float -> Float`, which conventionally means the distinct type
// `Float -> (Float -> Float)` and parses back to `(Float, Float) -> Float`.

#[test]
fn display_brackets_function_typed_single_param() {
    // (Float -> Float) -> Float
    let inner = DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Float));
    let outer = DataType::Function(FunctionType::new(vec![inner], DataType::Float));
    assert_eq!(outer.to_string(), "(Float -> Float) -> Float");
}

#[test]
fn display_no_brackets_for_non_function_single_param() {
    // Non-function params are left bare: Float -> Float.
    let ty = DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Float));
    assert_eq!(ty.to_string(), "Float -> Float");
}

#[test]
fn display_brackets_function_typed_param_among_many() {
    // ((Float -> Float),Int) -> Bool — the function param is parenthesized
    // even though the surrounding comma already delimits it.
    let inner = DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Float));
    let outer = DataType::Function(FunctionType::new(
        vec![inner, DataType::Int],
        DataType::Bool,
    ));
    assert_eq!(outer.to_string(), "((Float -> Float),Int) -> Bool");
}

#[test]
fn function_typed_param_display_roundtrips() {
    // The pre-fix string `Float -> Float -> Float` parsed back to the WRONG
    // type `(Float, Float) -> Float`. The bracketed form must round-trip to
    // the original `(Float -> Float) -> Float`.
    let inner = DataType::Function(FunctionType::new(vec![DataType::Float], DataType::Float));
    let original = DataType::Function(FunctionType::new(vec![inner], DataType::Float));
    let reparsed = DataType::from_string(&original.to_string()).expect("parse");
    assert_eq!(reparsed, original);
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

// ---------------------------------------------------------------------------
// Nullary function coercion (`() -> T` → `T`).
// See `doc/design_nullary_function_coercion.md`.
// ---------------------------------------------------------------------------

/// `() -> T` as a `DataType`.
fn nullary(output: DataType) -> DataType {
    DataType::Function(FunctionType::new(vec![], output))
}

#[test]
fn nullary_forces_to_its_result_type() {
    let registry = NodeTypeRegistry::new();
    // `() -> Int` flows into an `Int` pin.
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &DataType::Int,
        &registry
    ));
}

#[test]
fn nullary_forces_through_value_widening() {
    let registry = NodeTypeRegistry::new();
    // The forced result still gets the ordinary leaf widenings: `() -> Int`
    // into a `Float` pin (Int -> Float), `() -> Crystal` into a `HasAtoms` pin
    // (concrete -> abstract phase upcast).
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &DataType::Float,
        &registry
    ));
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Crystal),
        &DataType::HasAtoms,
        &registry
    ));
}

#[test]
fn nullary_does_not_force_when_result_is_incompatible() {
    let registry = NodeTypeRegistry::new();
    // `() -> Int` into a `Bool` pin: Int is not convertible to Bool, so the
    // forced value isn't either.
    assert!(!DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &DataType::Bool,
        &registry
    ));
}

#[test]
fn nullary_coercion_is_one_directional() {
    let registry = NodeTypeRegistry::new();
    // The reverse `T -> () -> T` (auto-suspension) is intentionally NOT added.
    assert!(!DataType::can_be_converted_to(
        &DataType::Int,
        &nullary(DataType::Int),
        &registry
    ));
}

#[test]
fn nullary_to_function_pin_stays_a_function() {
    let registry = NodeTypeRegistry::new();
    // A function-shaped destination is handled by the function arm, not the
    // nullary arm: `() -> Int` into a `() -> Float` pin succeeds structurally
    // (same arity, Int -> Float return) and remains a function value — it is
    // NOT forced. We can only assert the type-level acceptance here.
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &nullary(DataType::Float),
        &registry
    ));
    // And into an `apply.f`-style `AnyFunction` slot.
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &DataType::AnyFunction {
            leading_params: vec![],
        },
        &registry
    ));
}

#[test]
fn nullary_composes_with_scalar_broadcast_into_collections() {
    let registry = NodeTypeRegistry::new();
    // Case A (allowed): a *scalar* `() -> T` source forces to `T`, then the
    // ordinary single-element broadcast wraps it into a collection pin.
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &DataType::Array(Box::new(DataType::Int)),
        &registry
    ));
    assert!(DataType::can_be_converted_to(
        &nullary(DataType::Int),
        &DataType::Iterator(Box::new(DataType::Int)),
        &registry
    ));
}

#[test]
fn nullary_does_not_recurse_through_collection_sources() {
    let registry = NodeTypeRegistry::new();
    // Case B (rejected, top-level-only rule / D1): an *array of* nullary
    // functions does NOT convert to an array of values. The runtime only
    // forces at the top-level pin, so accepting this would be a type-lie.
    assert!(!DataType::can_be_converted_to(
        &DataType::Array(Box::new(nullary(DataType::Int))),
        &DataType::Array(Box::new(DataType::Int)),
        &registry
    ));
}

#[test]
fn nullary_not_forced_in_strict_no_broadcast_variant() {
    let registry = NodeTypeRegistry::new();
    // The drag-aware strict variant deliberately does not apply nullary forcing.
    assert!(!DataType::can_be_converted_to_strict_no_broadcast(
        &nullary(DataType::Int),
        &DataType::Int,
        &registry
    ));
}

#[test]
fn higher_arity_functions_are_never_forced() {
    let registry = NodeTypeRegistry::new();
    // `(Int) -> Int` is not nullary, so it does not force into an `Int` pin.
    let unary = DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Int));
    assert!(!DataType::can_be_converted_to(
        &unary,
        &DataType::Int,
        &registry
    ));
}
