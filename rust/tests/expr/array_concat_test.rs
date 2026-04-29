use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

mod validation_tests {
    use super::*;

    fn validate(expr_text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
        let expr = parse(expr_text)?;
        expr.validate(&vars, get_function_signatures())
    }

    fn array(t: DataType) -> DataType {
        DataType::Array(Box::new(t))
    }

    #[test]
    fn test_validate_concat_int_arrays() {
        let result = validate("concat([1, 2], [3, 4])", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Int));
    }

    #[test]
    fn test_validate_concat_int_float_promotes_to_float() {
        // Note: existing expr lexer collapses `3.0`/`4.0` to Int. Use non-integer
        // floats to keep the float-ness intact through parsing (matches the
        // workaround in array_literal_test::test_validate_promoted_to_float).
        let result =
            validate("concat([1, 2], [3.5, 4.5])", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Float));
    }

    #[test]
    fn test_validate_concat_empty_with_non_empty() {
        let result = validate("concat([]Int, [1, 2, 3])", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Int));
    }

    #[test]
    fn test_validate_concat_two_empties() {
        let result = validate("concat([]Int, []Int)", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Int));
    }

    #[test]
    fn test_validate_concat_ivec3_vec3_promotes() {
        let result = validate(
            "concat([ivec3(1,2,3)], [vec3(1.0, 2.0, 3.0)])",
            HashMap::new(),
        )
        .expect("should validate");
        assert_eq!(result, array(DataType::Vec3));
    }

    #[test]
    fn test_validate_concat_incompatible_element_types_rejected() {
        let result = validate("concat([1, 2], [vec3(1,2,3)])", HashMap::new());
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_concat_non_array_arg_rejected() {
        let result = validate("concat([1, 2], 3)", HashMap::new());
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_concat_arity_one_rejected() {
        // `concat([1, 2])` parses as one-arg call.
        let parsed = parse("concat([1, 2])");
        if let Ok(expr) = parsed {
            let result = expr.validate(&HashMap::new(), get_function_signatures());
            assert!(result.is_err(), "got {:?}", result);
        }
        // Parser-level rejection is also acceptable.
    }

    #[test]
    fn test_validate_concat_arity_three_rejected() {
        let parsed = parse("concat([1, 2], [3], [4])");
        if let Ok(expr) = parsed {
            let result = expr.validate(&HashMap::new(), get_function_signatures());
            assert!(result.is_err(), "got {:?}", result);
        }
    }

    #[test]
    fn test_validate_concat_nested_arrays() {
        let result = validate("concat([[1,2]], [[3,4]])", HashMap::new()).expect("should validate");
        assert_eq!(result, array(array(DataType::Int)));
    }

    #[test]
    fn test_validate_concat_with_array_vars() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), array(DataType::IVec3));
        vars.insert("b".into(), array(DataType::IVec3));
        let result = validate("concat(a, b)", vars).expect("should validate");
        assert_eq!(result, array(DataType::IVec3));
    }
}

mod evaluation_tests {
    use super::*;

    fn eval_with(expr_text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
        let expr = parse(expr_text).expect("parse");
        expr.evaluate(&vars, get_function_implementations())
    }

    fn assert_int_array(result: NetworkResult, expected: &[i32]) {
        match result {
            NetworkResult::Array(items) => {
                assert_eq!(items.len(), expected.len(), "length mismatch");
                for (i, (got, want)) in items.iter().zip(expected.iter()).enumerate() {
                    match got {
                        NetworkResult::Int(n) => assert_eq!(n, want, "element {}", i),
                        other => panic!("expected Int at {}, got {}", i, other.to_display_string()),
                    }
                }
            }
            other => panic!("expected Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_concat_two_int_arrays() {
        let result = eval_with("concat([1, 2], [3, 4])", HashMap::new());
        assert_int_array(result, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_eval_concat_empty_with_non_empty() {
        let result = eval_with("concat([]Int, [1, 2])", HashMap::new());
        assert_int_array(result, &[1, 2]);
    }

    #[test]
    fn test_eval_concat_two_empties() {
        match eval_with("concat([]Int, []Int)", HashMap::new()) {
            NetworkResult::Array(items) => {
                assert!(
                    items.is_empty(),
                    "expected empty array, got {} items",
                    items.len()
                );
            }
            other => panic!("expected Array, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_concat_nested_calls() {
        // concat([1], concat([2], [3])) == [1, 2, 3]
        let result = eval_with("concat([1], concat([2], [3]))", HashMap::new());
        assert_int_array(result, &[1, 2, 3]);
    }

    #[test]
    fn test_eval_len_of_concat() {
        // len(concat([1, 2], [3, 4])) == 4
        match eval_with("len(concat([1, 2], [3, 4]))", HashMap::new()) {
            NetworkResult::Int(4) => {}
            other => panic!("expected Int(4), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_concat_then_index_promoted_element() {
        // concat([1, 2], [3.5])[2] returns the actual third element. Validation
        // unifies the array type to Array[Float], but at runtime each element
        // keeps its concrete representation — element 2 is the original Float(3.5).
        // Note: `3.0` would be collapsed to Int(3) by the lexer, so we use 3.5.
        match eval_with("concat([1, 2], [3.5])[2]", HashMap::new()) {
            NetworkResult::Float(f) => {
                assert!((f - 3.5).abs() < f64::EPSILON, "got {}", f);
            }
            other => panic!("expected Float(3.5), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_concat_with_array_vars() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            NetworkResult::Array(vec![NetworkResult::Int(10), NetworkResult::Int(20)]),
        );
        vars.insert(
            "b".into(),
            NetworkResult::Array(vec![
                NetworkResult::Int(30),
                NetworkResult::Int(40),
                NetworkResult::Int(50),
            ]),
        );
        let result = eval_with("concat(a, b)", vars);
        assert_int_array(result, &[10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_eval_concat_preserves_element_order() {
        let result = eval_with("concat([3, 1, 4], [1, 5, 9, 2])", HashMap::new());
        assert_int_array(result, &[3, 1, 4, 1, 5, 9, 2]);
    }
}
