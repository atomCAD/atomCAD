// Tests for `==` / `!=` equality comparison on String values in the `expr`
// language. Validation already permits comparing two compatible types
// (String/String is compatible), so these comparisons must also evaluate
// correctly at runtime — e.g. `if s == \`A\` then 1 else 2`.

use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

fn validate_with(text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
    let expr = parse(text).expect("parse should succeed");
    expr.validate(&vars, get_function_signatures())
}

fn eval_with(text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
    let expr = parse(text).expect("parse should succeed");
    expr.evaluate(&vars, get_function_implementations())
}

fn assert_bool(v: NetworkResult, expected: bool) {
    match v {
        NetworkResult::Bool(b) => assert_eq!(b, expected),
        other => panic!("expected Bool({}), got {:?}", expected, other.infer_data_type()),
    }
}

fn assert_int(v: NetworkResult, expected: i32) {
    match v {
        NetworkResult::Int(i) => assert_eq!(i, expected),
        other => panic!("expected Int({}), got {:?}", expected, other.infer_data_type()),
    }
}

mod validation_tests {
    use super::*;

    #[test]
    fn test_string_eq_string_validates() {
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), DataType::String);
        let ty = validate_with("s == `A`", vars).expect("should validate");
        assert_eq!(ty, DataType::Bool);
    }

    #[test]
    fn test_string_ne_string_validates() {
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), DataType::String);
        let ty = validate_with("s != `A`", vars).expect("should validate");
        assert_eq!(ty, DataType::Bool);
    }
}

mod evaluation_tests {
    use super::*;

    #[test]
    fn test_evaluate_string_eq_true() {
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), NetworkResult::String("A".to_string()));
        assert_bool(eval_with("s == `A`", vars), true);
    }

    #[test]
    fn test_evaluate_string_eq_false() {
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), NetworkResult::String("B".to_string()));
        assert_bool(eval_with("s == `A`", vars), false);
    }

    #[test]
    fn test_evaluate_string_ne_true() {
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), NetworkResult::String("B".to_string()));
        assert_bool(eval_with("s != `A`", vars), true);
    }

    #[test]
    fn test_evaluate_string_ne_false() {
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), NetworkResult::String("A".to_string()));
        assert_bool(eval_with("s != `A`", vars), false);
    }

    #[test]
    fn test_evaluate_string_literal_eq_literal() {
        assert_bool(eval_with("`A` == `A`", HashMap::new()), true);
        assert_bool(eval_with("`A` == `B`", HashMap::new()), false);
    }

    #[test]
    fn test_evaluate_string_eq_in_if() {
        // The original user-reported case: `if s == \`A\` then 1 else 2`.
        let mut vars = HashMap::new();
        vars.insert("s".to_string(), NetworkResult::String("A".to_string()));
        assert_int(eval_with("if s == `A` then 1 else 2", vars), 1);

        let mut vars = HashMap::new();
        vars.insert("s".to_string(), NetworkResult::String("Z".to_string()));
        assert_int(eval_with("if s == `A` then 1 else 2", vars), 2);
    }
}
