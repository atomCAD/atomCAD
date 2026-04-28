use rust_lib_flutter_cad::expr::expr::Expr;
use rust_lib_flutter_cad::expr::lexer::{Token, tokenize};
use rust_lib_flutter_cad::expr::parser::{parse, parse_concrete_type_name};
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

mod lexer_tests {
    use super::*;

    #[test]
    fn test_tokenize_brackets() {
        let tokens = tokenize("[ ]");
        assert_eq!(tokens, vec![Token::LBracket, Token::RBracket, Token::Eof]);
    }

    #[test]
    fn test_tokenize_array_literal() {
        let tokens = tokenize("[1, 2, 3]");
        assert_eq!(
            tokens,
            vec![
                Token::LBracket,
                Token::Number(1.0),
                Token::Comma,
                Token::Number(2.0),
                Token::Comma,
                Token::Number(3.0),
                Token::RBracket,
                Token::Eof,
            ]
        );
    }
}

mod parser_tests {
    use super::*;

    fn assert_int_array(expr: &Expr, expected: &[i32]) {
        match expr {
            Expr::Array(elements) => {
                assert_eq!(elements.len(), expected.len(), "array length mismatch");
                for (e, n) in elements.iter().zip(expected.iter()) {
                    match e {
                        Expr::Int(v) => assert_eq!(v, n),
                        _ => panic!("expected Int element, got {:?}", e),
                    }
                }
            }
            _ => panic!("expected Expr::Array, got {:?}", expr),
        }
    }

    #[test]
    fn test_parse_simple_int_array() {
        let expr = parse("[1, 2, 3]").expect("parse should succeed");
        assert_int_array(&expr, &[1, 2, 3]);
    }

    #[test]
    fn test_parse_call_in_array() {
        let expr = parse("[ivec3(1,2,3), ivec3(4,5,6)]").expect("parse should succeed");
        match expr {
            Expr::Array(elements) => {
                assert_eq!(elements.len(), 2);
                for e in &elements {
                    match e {
                        Expr::Call(name, args) => {
                            assert_eq!(name, "ivec3");
                            assert_eq!(args.len(), 3);
                        }
                        _ => panic!("expected call"),
                    }
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_parse_empty_typed_array_primitive() {
        let expr = parse("[]IVec3").expect("parse should succeed");
        assert!(matches!(expr, Expr::EmptyArray(DataType::IVec3)));
    }

    #[test]
    fn test_parse_empty_typed_array_structure() {
        let expr = parse("[]Structure").expect("parse should succeed");
        assert!(matches!(expr, Expr::EmptyArray(DataType::Structure)));
    }

    #[test]
    fn test_parse_empty_typed_array_nested() {
        let expr = parse("[][IVec3]").expect("parse should succeed");
        match expr {
            Expr::EmptyArray(DataType::Array(inner)) => {
                assert_eq!(*inner, DataType::IVec3);
            }
            _ => panic!("expected EmptyArray(Array(IVec3))"),
        }
    }

    #[test]
    fn test_parse_empty_typed_array_double_nested() {
        let expr = parse("[][[Int]]").expect("parse should succeed");
        match expr {
            Expr::EmptyArray(DataType::Array(inner)) => match *inner {
                DataType::Array(innermost) => {
                    assert_eq!(*innermost, DataType::Int);
                }
                other => panic!("expected Array(Array(Int)), got {:?}", other),
            },
            other => panic!("expected EmptyArray(Array(Array(Int))), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_outer_with_inner_empty() {
        // [[]Int] -> 1-element outer, inner is EmptyArray(Int)
        let expr = parse("[[]Int]").expect("parse should succeed");
        match expr {
            Expr::Array(elements) => {
                assert_eq!(elements.len(), 1);
                match &elements[0] {
                    Expr::EmptyArray(DataType::Int) => {}
                    other => panic!("expected EmptyArray(Int), got {:?}", other),
                }
            }
            other => panic!("expected outer Array, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_no_type_after_empty_marker() {
        let res = parse("[]");
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_unknown_type_after_empty_marker() {
        let res = parse("[]Foo");
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_abstract_type_rejected() {
        let res = parse("[]HasAtoms");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_none_type_rejected() {
        let res = parse("[]None");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_trailing_comma_rejected() {
        let res = parse("[1, ]");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_bad_separator_rejected() {
        let res = parse("[1; 2]");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_unclosed_array_rejected() {
        let res = parse("[1, 2");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_int_in_element_position_is_var() {
        // [Int, x] -> element list, "Int" parses as Var("Int"); validation fails as
        // unknown variable, but parsing must succeed.
        let expr = parse("[Int, x]").expect("parse should succeed");
        match expr {
            Expr::Array(elements) => {
                assert_eq!(elements.len(), 2);
                match &elements[0] {
                    Expr::Var(name) => assert_eq!(name, "Int"),
                    other => panic!("expected Var(Int), got {:?}", other),
                }
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_single_element_array_var() {
        // [Structure] (no parameter) - parses as 1-element array containing Var("Structure")
        let expr = parse("[Structure]").expect("parse should succeed");
        match expr {
            Expr::Array(elements) => {
                assert_eq!(elements.len(), 1);
                match &elements[0] {
                    Expr::Var(name) => assert_eq!(name, "Structure"),
                    other => panic!("expected Var(Structure), got {:?}", other),
                }
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_non_empty_arrays() {
        let expr = parse("[[1, 2], [3, 4]]").expect("parse should succeed");
        match expr {
            Expr::Array(outer) => {
                assert_eq!(outer.len(), 2);
                for inner in outer.iter() {
                    match inner {
                        Expr::Array(elems) => assert_eq!(elems.len(), 2),
                        other => panic!("expected inner Array, got {:?}", other),
                    }
                }
            }
            other => panic!("expected outer Array, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_outer_with_two_empty_inners() {
        let expr = parse("[[]Int, []Int]").expect("parse should succeed");
        match expr {
            Expr::Array(outer) => {
                assert_eq!(outer.len(), 2);
                for inner in outer.iter() {
                    match inner {
                        Expr::EmptyArray(DataType::Int) => {}
                        other => panic!("expected EmptyArray(Int), got {:?}", other),
                    }
                }
            }
            other => panic!("expected outer Array, got {:?}", other),
        }
    }
}

mod validation_tests {
    use super::*;

    fn validate(expr_text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
        let expr = parse(expr_text)?;
        expr.validate(&vars, get_function_signatures())
    }

    #[test]
    fn test_validate_int_array() {
        let result = validate("[1, 2, 3]", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Int)));
    }

    #[test]
    fn test_validate_promoted_to_float() {
        // Note: existing expr lexer collapses `2.0` to Int(2) (pre-existing
        // behavior — see Token::Number handling in parser.rs). Use 2.5 to keep
        // the float-ness intact through parsing.
        let result = validate("[1, 2.5, 3]", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Float)));
    }

    #[test]
    fn test_validate_all_floats() {
        let result = validate("[1, 2.5, 3.5]", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Float)));
    }

    #[test]
    fn test_validate_bool_array() {
        let result = validate("[true, false]", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Bool)));
    }

    #[test]
    fn test_validate_ivec3_array() {
        let result =
            validate("[ivec3(1,2,3), ivec3(4,5,6)]", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::IVec3)));
    }

    #[test]
    fn test_validate_ivec3_promoted_to_vec3() {
        let result =
            validate("[ivec3(1,2,3), vec3(0.5,0.5,0.5)]", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Vec3)));
    }

    #[test]
    fn test_validate_with_int_params() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Int);
        vars.insert("b".into(), DataType::Int);
        let result = validate("[a, b]", vars).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Int)));
    }

    #[test]
    fn test_validate_with_int_float_params() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Int);
        vars.insert("b".into(), DataType::Float);
        let result = validate("[a, b]", vars).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Float)));
    }

    #[test]
    fn test_validate_with_ivec3_vec3_params() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::IVec3);
        vars.insert("b".into(), DataType::Vec3);
        let result = validate("[a, b]", vars).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Vec3)));
    }

    #[test]
    fn test_validate_structure_array() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Structure);
        let result = validate("[a, a, a]", vars).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Structure)));
    }

    #[test]
    fn test_validate_crystal_array() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Crystal);
        vars.insert("b".into(), DataType::Crystal);
        let result = validate("[a, b]", vars).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Crystal)));
    }

    #[test]
    fn test_validate_crystal_molecule_rejected() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Crystal);
        vars.insert("b".into(), DataType::Molecule);
        let result = validate("[a, b]", vars);
        assert!(
            result.is_err(),
            "expected unification failure: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_int_vec3_rejected() {
        let result = validate("[1, vec3(0,0,0)]", HashMap::new());
        assert!(result.is_err(), "expected mismatch: {:?}", result);
    }

    #[test]
    fn test_validate_singleton_array() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Int);
        let result = validate("[a]", vars).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Int)));
    }

    #[test]
    fn test_validate_empty_ivec3() {
        let result = validate("[]IVec3", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::IVec3)));
    }

    #[test]
    fn test_validate_empty_float() {
        let result = validate("[]Float", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Float)));
    }

    #[test]
    fn test_validate_empty_structure() {
        let result = validate("[]Structure", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Array(Box::new(DataType::Structure)));
    }

    #[test]
    fn test_validate_empty_nested() {
        let result = validate("[][IVec3]", HashMap::new()).expect("should validate");
        assert_eq!(
            result,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::IVec3))))
        );
    }

    #[test]
    fn test_validate_empty_double_nested() {
        let result = validate("[][[Int]]", HashMap::new()).expect("should validate");
        assert_eq!(
            result,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Array(
                Box::new(DataType::Int)
            )))))
        );
    }

    #[test]
    fn test_validate_recursive_unification_arrays() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::IVec3);
        vars.insert("b".into(), DataType::IVec3);
        let result = validate("[[a], [b]]", vars).expect("should validate");
        assert_eq!(
            result,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::IVec3))))
        );
    }

    #[test]
    fn test_validate_recursive_unification_int_float() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Int);
        vars.insert("b".into(), DataType::Float);
        let result = validate("[[a], [b]]", vars).expect("should validate");
        assert_eq!(
            result,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Float))))
        );
    }

    #[test]
    fn test_validate_outer_with_two_empty_inners() {
        let result = validate("[[]Int, []Int]", HashMap::new()).expect("should validate");
        assert_eq!(
            result,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Int))))
        );
    }

    #[test]
    fn test_validate_outer_with_one_empty_inner() {
        let result = validate("[[]Int]", HashMap::new()).expect("should validate");
        assert_eq!(
            result,
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Int))))
        );
    }
}

mod element_type_eligibility_tests {
    use super::*;

    /// Coverage test: enumerate every concrete `DataType` variant and assert that
    /// `parse_concrete_type_name` either accepts it or matches the documented
    /// rejection set. Forces an explicit accept-or-reject decision on any newly
    /// added `DataType` variant.
    #[test]
    fn every_concrete_datatype_is_array_eligible_or_explicitly_rejected() {
        // Build sample values for every DataType variant. If a new variant is
        // added to `DataType`, this match becomes non-exhaustive and the test
        // fails to compile, forcing the implementer to add an entry here AND a
        // policy decision in `parse_concrete_type_name`.
        let variants: Vec<DataType> = vec![
            DataType::None,
            DataType::Bool,
            DataType::String,
            DataType::Int,
            DataType::Float,
            DataType::Vec2,
            DataType::Vec3,
            DataType::IVec2,
            DataType::IVec3,
            DataType::IMat3,
            DataType::Mat3,
            DataType::LatticeVecs,
            DataType::DrawingPlane,
            DataType::Geometry2D,
            DataType::Blueprint,
            DataType::HasAtoms,
            DataType::Crystal,
            DataType::Molecule,
            DataType::HasStructure,
            DataType::HasFreeLinOps,
            DataType::Motif,
            DataType::Structure,
            DataType::Array(Box::new(DataType::Int)),
            DataType::Function(
                rust_lib_flutter_cad::structure_designer::data_type::FunctionType {
                    parameter_types: vec![DataType::Int],
                    output_type: Box::new(DataType::Int),
                },
            ),
        ];

        // Ensure every DataType variant is represented in `variants`. The
        // `match` ensures non-exhaustive variants cause a compile error.
        for v in &variants {
            match v {
                DataType::None
                | DataType::Bool
                | DataType::String
                | DataType::Int
                | DataType::Float
                | DataType::Vec2
                | DataType::Vec3
                | DataType::IVec2
                | DataType::IVec3
                | DataType::IMat3
                | DataType::Mat3
                | DataType::LatticeVecs
                | DataType::DrawingPlane
                | DataType::Geometry2D
                | DataType::Blueprint
                | DataType::HasAtoms
                | DataType::Crystal
                | DataType::Molecule
                | DataType::HasStructure
                | DataType::HasFreeLinOps
                | DataType::Motif
                | DataType::Structure
                | DataType::Array(_)
                | DataType::Function(_) => {}
            }
        }

        // Documented rejection set: None, the three abstract supertypes, and
        // any function type.
        let is_rejected = |dt: &DataType| -> bool {
            matches!(
                dt,
                DataType::None
                    | DataType::HasAtoms
                    | DataType::HasStructure
                    | DataType::HasFreeLinOps
                    | DataType::Function(_)
            )
        };

        // For each name-bearing variant, check that `parse_concrete_type_name`
        // matches the documented policy. The Array and Function variants are
        // not directly produced by the type-name path (`parse_concrete_type_name`
        // is only called on a single identifier), so we skip them here — the
        // recursive `[T]` and the function rejection are exercised by other
        // tests.
        for v in &variants {
            if matches!(v, DataType::Array(_) | DataType::Function(_)) {
                continue;
            }
            let name = format!("{}", v);
            let parsed = parse_concrete_type_name(&name);
            if is_rejected(v) {
                assert!(
                    parsed.is_none(),
                    "{} should be rejected as element type",
                    name
                );
            } else {
                assert_eq!(
                    parsed.as_ref(),
                    Some(v),
                    "{} should be accepted as element type",
                    name
                );
            }
        }
    }
}

mod evaluation_tests {
    use super::*;
    use glam::i32::IVec3;

    fn eval(expr_text: &str) -> NetworkResult {
        let expr = parse(expr_text).expect("parse");
        expr.evaluate(&HashMap::new(), get_function_implementations())
    }

    fn eval_with(expr_text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
        let expr = parse(expr_text).expect("parse");
        expr.evaluate(&vars, get_function_implementations())
    }

    #[test]
    fn test_eval_int_array() {
        match eval("[1, 2, 3]") {
            NetworkResult::Array(elements) => {
                assert_eq!(elements.len(), 3);
                match (&elements[0], &elements[1], &elements[2]) {
                    (NetworkResult::Int(1), NetworkResult::Int(2), NetworkResult::Int(3)) => {}
                    _ => panic!("unexpected element types"),
                }
            }
            other => panic!("expected Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_array_with_var_arithmetic() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), NetworkResult::Int(5));
        match eval_with("[a*2, a*3]", vars) {
            NetworkResult::Array(elements) => {
                assert_eq!(elements.len(), 2);
                match (&elements[0], &elements[1]) {
                    (NetworkResult::Int(10), NetworkResult::Int(15)) => {}
                    _ => panic!("unexpected element types"),
                }
            }
            other => panic!("expected Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_ivec3_array() {
        match eval("[ivec3(1,2,3), ivec3(4,5,6)]") {
            NetworkResult::Array(elements) => {
                assert_eq!(elements.len(), 2);
                match (&elements[0], &elements[1]) {
                    (NetworkResult::IVec3(a), NetworkResult::IVec3(b)) => {
                        assert_eq!(*a, IVec3::new(1, 2, 3));
                        assert_eq!(*b, IVec3::new(4, 5, 6));
                    }
                    _ => panic!("unexpected element types"),
                }
            }
            other => panic!("expected Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_nested_int_array() {
        match eval("[[1, 2], [3, 4]]") {
            NetworkResult::Array(outer) => {
                assert_eq!(outer.len(), 2);
                for (inner, expected) in outer.iter().zip([[1, 2], [3, 4]].iter()) {
                    match inner {
                        NetworkResult::Array(elems) => {
                            assert_eq!(elems.len(), 2);
                            match (&elems[0], &elems[1]) {
                                (NetworkResult::Int(a), NetworkResult::Int(b)) => {
                                    assert_eq!([*a, *b], *expected);
                                }
                                _ => panic!("expected Int elements"),
                            }
                        }
                        _ => panic!("expected inner Array"),
                    }
                }
            }
            other => panic!("expected outer Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_empty_int_array() {
        match eval("[]Int") {
            NetworkResult::Array(elements) => assert!(elements.is_empty()),
            other => panic!("expected empty Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_empty_ivec3_array() {
        match eval("[]IVec3") {
            NetworkResult::Array(elements) => assert!(elements.is_empty()),
            other => panic!("expected empty Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_empty_structure_array() {
        match eval("[]Structure") {
            NetworkResult::Array(elements) => assert!(elements.is_empty()),
            other => panic!("expected empty Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_empty_nested_array() {
        match eval("[][IVec3]") {
            NetworkResult::Array(elements) => assert!(elements.is_empty()),
            other => panic!("expected empty Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_outer_with_two_empty_inners() {
        match eval("[[]Int, []Int]") {
            NetworkResult::Array(outer) => {
                assert_eq!(outer.len(), 2);
                for inner in outer.iter() {
                    match inner {
                        NetworkResult::Array(elems) => assert!(elems.is_empty()),
                        _ => panic!("expected inner empty Array"),
                    }
                }
            }
            other => panic!("expected outer Array, got {}", other.to_display_string()),
        }
    }
}
