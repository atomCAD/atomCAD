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

    #[test]
    fn test_validate_len_int_array_literal() {
        let result = validate("len([1, 2, 3])", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_len_empty_array_literal() {
        let result = validate("len([]Int)", HashMap::new()).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_len_ivec3_array_var() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::IVec3)));
        let result = validate("len(a)", vars).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_len_of_indexed_nested_array() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Int)))),
        );
        vars.insert("i".into(), DataType::Int);
        let result = validate("len(a[i])", vars).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_len_non_array_rejected() {
        let result = validate("len(5)", HashMap::new());
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_len_wrong_arity_two_rejected() {
        let result = validate("len([1, 2], [3])", HashMap::new());
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_len_wrong_arity_zero_rejected() {
        // `len()` parses as zero-arg call.
        let parsed = parse("len()");
        if let Ok(expr) = parsed {
            let result = expr.validate(&HashMap::new(), get_function_signatures());
            assert!(result.is_err(), "got {:?}", result);
        }
        // If the parser rejects len() up front, that is also acceptable
        // (the goal is "not silently valid").
    }

    #[test]
    fn test_validate_len_of_array_of_array_var() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Float)))),
        );
        let result = validate("len(a)", vars).expect("should validate");
        assert_eq!(result, DataType::Int);
    }
}

mod evaluation_tests {
    use super::*;

    fn eval_with(expr_text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
        let expr = parse(expr_text).expect("parse");
        expr.evaluate(&vars, get_function_implementations())
    }

    #[test]
    fn test_eval_len_int_array_literal() {
        match eval_with("len([1, 2, 3])", HashMap::new()) {
            NetworkResult::Int(3) => {}
            other => panic!("expected Int(3), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_len_empty_array_literal() {
        match eval_with("len([]Int)", HashMap::new()) {
            NetworkResult::Int(0) => {}
            other => panic!("expected Int(0), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_len_nested_array_literal() {
        // len([[1,2], [3,4,5]]) == 2 (outer length)
        match eval_with("len([[1,2], [3,4,5]])", HashMap::new()) {
            NetworkResult::Int(2) => {}
            other => panic!("expected Int(2), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_len_ivec3_array_literal() {
        match eval_with("len([ivec3(1,2,3), ivec3(4,5,6)])", HashMap::new()) {
            NetworkResult::Int(2) => {}
            other => panic!("expected Int(2), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_len_var_array() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            NetworkResult::Array(vec![
                NetworkResult::Int(10),
                NetworkResult::Int(20),
                NetworkResult::Int(30),
                NetworkResult::Int(40),
            ]),
        );
        match eval_with("len(a)", vars) {
            NetworkResult::Int(4) => {}
            other => panic!("expected Int(4), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_len_composes_with_indexing() {
        // len(a[i]) where a is Array[Array[Int]] returns the length of a[i].
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            NetworkResult::Array(vec![
                NetworkResult::Array(vec![NetworkResult::Int(1), NetworkResult::Int(2)]),
                NetworkResult::Array(vec![
                    NetworkResult::Int(3),
                    NetworkResult::Int(4),
                    NetworkResult::Int(5),
                ]),
            ]),
        );
        vars.insert("i".into(), NetworkResult::Int(1));
        match eval_with("len(a[i])", vars) {
            NetworkResult::Int(3) => {}
            other => panic!("expected Int(3), got {}", other.to_display_string()),
        }
    }
}
