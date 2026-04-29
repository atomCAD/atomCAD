use rust_lib_flutter_cad::expr::expr::{BinOp, Expr};
use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_simple_index() {
        let expr = parse("a[0]").expect("parse should succeed");
        match expr {
            Expr::Index(arr, idx) => {
                match *arr {
                    Expr::Var(name) => assert_eq!(name, "a"),
                    other => panic!("expected Var(a), got {:?}", other),
                }
                match *idx {
                    Expr::Int(0) => {}
                    other => panic!("expected Int(0), got {:?}", other),
                }
            }
            other => panic!("expected Index, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_index_with_expression() {
        let expr = parse("a[i + 1]").expect("parse should succeed");
        match expr {
            Expr::Index(arr, idx) => {
                match *arr {
                    Expr::Var(name) => assert_eq!(name, "a"),
                    other => panic!("expected Var(a), got {:?}", other),
                }
                match *idx {
                    Expr::Binary(left, BinOp::Add, right) => {
                        assert!(matches!(*left, Expr::Var(ref n) if n == "i"));
                        assert!(matches!(*right, Expr::Int(1)));
                    }
                    other => panic!("expected Binary(Add), got {:?}", other),
                }
            }
            other => panic!("expected Index, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_chained_index() {
        let expr = parse("a[i][j]").expect("parse should succeed");
        // Index(Index(Var(a), Var(i)), Var(j))
        match expr {
            Expr::Index(outer, j) => {
                assert!(matches!(*j, Expr::Var(ref n) if n == "j"));
                match *outer {
                    Expr::Index(arr, i) => {
                        assert!(matches!(*arr, Expr::Var(ref n) if n == "a"));
                        assert!(matches!(*i, Expr::Var(ref n) if n == "i"));
                    }
                    other => panic!("expected nested Index, got {:?}", other),
                }
            }
            other => panic!("expected outer Index, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_index_then_member_access() {
        let expr = parse("a[i].x").expect("parse should succeed");
        // MemberAccess(Index(Var(a), Var(i)), "x")
        match expr {
            Expr::MemberAccess(inner, name) => {
                assert_eq!(name, "x");
                match *inner {
                    Expr::Index(arr, i) => {
                        assert!(matches!(*arr, Expr::Var(ref n) if n == "a"));
                        assert!(matches!(*i, Expr::Var(ref n) if n == "i"));
                    }
                    other => panic!("expected Index, got {:?}", other),
                }
            }
            other => panic!("expected MemberAccess, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_array_literal_then_index() {
        let expr = parse("[1, 2, 3][0]").expect("parse should succeed");
        match expr {
            Expr::Index(arr, idx) => {
                match *arr {
                    Expr::Array(elements) => {
                        assert_eq!(elements.len(), 3);
                    }
                    other => panic!("expected Array, got {:?}", other),
                }
                assert!(matches!(*idx, Expr::Int(0)));
            }
            other => panic!("expected Index, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_empty_index_rejected() {
        let res = parse("a[]");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_comma_in_index_rejected() {
        let res = parse("a[1, 2]");
        assert!(res.is_err(), "got {:?}", res);
    }

    #[test]
    fn test_parse_unclosed_index_rejected() {
        let res = parse("a[1");
        assert!(res.is_err(), "got {:?}", res);
    }
}

mod validation_tests {
    use super::*;

    fn validate(expr_text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
        let expr = parse(expr_text)?;
        expr.validate(&vars, get_function_signatures())
    }

    #[test]
    fn test_validate_int_array_index() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::Int)));
        let result = validate("a[0]", vars).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_float_array_index_with_int_var() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::Float)));
        vars.insert("i".into(), DataType::Int);
        let result = validate("a[i]", vars).expect("should validate");
        assert_eq!(result, DataType::Float);
    }

    #[test]
    fn test_validate_ivec3_array_index() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::IVec3)));
        vars.insert("i".into(), DataType::Int);
        let result = validate("a[i]", vars).expect("should validate");
        assert_eq!(result, DataType::IVec3);
    }

    #[test]
    fn test_validate_nested_array_index() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            DataType::Array(Box::new(DataType::Array(Box::new(DataType::Int)))),
        );
        vars.insert("i".into(), DataType::Int);
        vars.insert("j".into(), DataType::Int);
        let result = validate("a[i][j]", vars).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_index_then_member() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::IVec3)));
        vars.insert("i".into(), DataType::Int);
        let result = validate("a[i].x", vars).expect("should validate");
        assert_eq!(result, DataType::Int);
    }

    #[test]
    fn test_validate_index_into_non_array_rejected() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Int);
        let result = validate("a[0]", vars);
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_bool_index_rejected() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::Int)));
        let result = validate("a[true]", vars);
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_float_index_rejected() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::Int)));
        let result = validate("a[1.5]", vars);
        assert!(result.is_err(), "got {:?}", result);
    }

    #[test]
    fn test_validate_structure_array_index() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), DataType::Array(Box::new(DataType::Structure)));
        vars.insert("i".into(), DataType::Int);
        let result = validate("a[i]", vars).expect("should validate");
        assert_eq!(result, DataType::Structure);
    }
}

mod evaluation_tests {
    use super::*;

    fn eval_with(expr_text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
        let expr = parse(expr_text).expect("parse");
        expr.evaluate(&vars, get_function_implementations())
    }

    fn int_array(values: &[i32]) -> NetworkResult {
        NetworkResult::Array(values.iter().map(|n| NetworkResult::Int(*n)).collect())
    }

    #[test]
    fn test_eval_simple_index() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), int_array(&[10, 20, 30]));
        match eval_with("a[0]", vars) {
            NetworkResult::Int(10) => {}
            other => panic!("expected Int(10), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_last_element() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), int_array(&[10, 20, 30]));
        match eval_with("a[2]", vars) {
            NetworkResult::Int(30) => {}
            other => panic!("expected Int(30), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_index_with_arithmetic() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), int_array(&[10, 20, 30]));
        vars.insert("i".into(), NetworkResult::Int(1));
        match eval_with("a[i + 1]", vars) {
            NetworkResult::Int(30) => {}
            other => panic!("expected Int(30), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_nested_index() {
        let mut vars = HashMap::new();
        vars.insert(
            "a".into(),
            NetworkResult::Array(vec![int_array(&[1, 2]), int_array(&[3, 4])]),
        );
        vars.insert("i".into(), NetworkResult::Int(1));
        vars.insert("j".into(), NetworkResult::Int(0));
        match eval_with("a[i][j]", vars) {
            NetworkResult::Int(3) => {}
            other => panic!("expected Int(3), got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_negative_index_error() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), int_array(&[10]));
        match eval_with("a[-1]", vars) {
            NetworkResult::Error(msg) => {
                assert!(
                    msg.contains("out of bounds") && msg.contains("-1") && msg.contains("length 1"),
                    "unexpected error message: {}",
                    msg
                );
            }
            other => panic!("expected Error, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_past_end_error() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), int_array(&[10, 20, 30]));
        match eval_with("a[3]", vars) {
            NetworkResult::Error(msg) => {
                assert!(
                    msg.contains("out of bounds") && msg.contains("3") && msg.contains("length 3"),
                    "unexpected error message: {}",
                    msg
                );
            }
            other => panic!("expected Error, got {}", other.to_display_string()),
        }
    }

    #[test]
    fn test_eval_empty_array_index_error() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), NetworkResult::Array(vec![]));
        match eval_with("a[0]", vars) {
            NetworkResult::Error(msg) => {
                assert!(
                    msg.contains("out of bounds") && msg.contains("length 0"),
                    "unexpected error message: {}",
                    msg
                );
            }
            other => panic!("expected Error, got {}", other.to_display_string()),
        }
    }
}
