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
    fn test_validate_append_int_to_int_array() {
        let result = validate("append([1, 2], 3)", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Int));
    }

    #[test]
    fn test_validate_append_float_promotes_to_float_array() {
        // Note: existing expr lexer collapses `3.0` to Int. Use 3.5 to keep
        // the float-ness intact through parsing (matches the workaround in
        // array_literal_test::test_validate_promoted_to_float).
        let result = validate("append([1, 2], 3.5)", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Float));
    }

    #[test]
    fn test_validate_append_to_empty_array() {
        let result = validate("append([]Int, 5)", HashMap::new()).expect("should validate");
        assert_eq!(result, array(DataType::Int));
    }

    #[test]
    fn test_validate_append_vec3_to_ivec3_array_promotes() {
        let result = validate(
            "append([ivec3(1,2,3)], vec3(1.0, 2.0, 3.0))",
            HashMap::new(),
        )
        .expect("should validate");
        assert_eq!(result, array(DataType::Vec3));
    }

    #[test]
    fn test_validate_append_nested_array() {
        let result = validate("append([[1,2]], [3,4])", HashMap::new()).expect("should validate");
        assert_eq!(result, array(array(DataType::Int)));
    }

    #[test]
    fn test_validate_append_with_array_var() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), array(DataType::IVec3));
        vars.insert("x".into(), DataType::IVec3);
        let result = validate("append(a, x)", vars).expect("should validate");
        assert_eq!(result, array(DataType::IVec3));
    }

    #[test]
    fn test_validate_append_incompatible_element_type_rejected() {
        let result = validate("append([1, 2], ivec3(1,2,3))", HashMap::new());
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_append_non_array_first_arg_rejected() {
        let result = validate("append(5, 3)", HashMap::new());
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_append_arity_one_rejected() {
        let parsed = parse("append([1, 2])");
        if let Ok(expr) = parsed {
            let result = expr.validate(&HashMap::new(), get_function_signatures());
            assert!(result.is_err(), "got {:?}", result);
        }
        // Parser-level rejection is also acceptable.
    }

    #[test]
    fn test_validate_append_arity_three_rejected() {
        let parsed = parse("append([1], 2, 3)");
        if let Ok(expr) = parsed {
            let result = expr.validate(&HashMap::new(), get_function_signatures());
            assert!(result.is_err(), "got {:?}", result);
        }
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
    fn test_eval_append_int_to_int_array() {
        let result = eval_with("append([1, 2], 3)", HashMap::new());
        assert_int_array(result, &[1, 2, 3]);
    }

    #[test]
    fn test_eval_append_to_empty_array() {
        let result = eval_with("append([]Int, 5)", HashMap::new());
        assert_int_array(result, &[5]);
    }

    #[test]
    fn test_eval_append_chained() {
        let result = eval_with("append(append([1], 2), 3)", HashMap::new());
        assert_int_array(result, &[1, 2, 3]);
    }

    #[test]
    fn test_eval_len_of_append() {
        match eval_with("len(append([1, 2], 3))", HashMap::new()) {
            NetworkResult::Int(3) => {}
            other => panic!("expected Int(3), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_append_then_index() {
        match eval_with("append([1, 2], 3)[2]", HashMap::new()) {
            NetworkResult::Int(3) => {}
            other => panic!("expected Int(3), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_append_composes_with_concat() {
        let result = eval_with("concat(append([1], 2), [3, 4])", HashMap::new());
        assert_int_array(result, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_eval_append_with_array_var() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            NetworkResult::Array(vec![NetworkResult::Int(10), NetworkResult::Int(20)]),
        );
        let result = eval_with("append(a, 30)", vars);
        assert_int_array(result, &[10, 20, 30]);
    }

    #[test]
    fn test_eval_append_preserves_element_order() {
        let result = eval_with("append([3, 1, 4], 1)", HashMap::new());
        assert_int_array(result, &[3, 1, 4, 1]);
    }
}
