// Tests for `+` string concatenation in the `expr` language.
// Strict: only `String + String`. No implicit conversion from numeric/bool —
// use a template literal `${x}` for mixed-type composition.

use rust_lib_flutter_cad::expr::expr::{BinOp, Expr};
use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

fn validate(text: &str) -> Result<DataType, String> {
    let expr = parse(text).expect("parse should succeed");
    let vars = HashMap::new();
    expr.validate(&vars, get_function_signatures())
}

fn validate_with(text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
    let expr = parse(text).expect("parse should succeed");
    expr.validate(&vars, get_function_signatures())
}

fn eval(text: &str) -> NetworkResult {
    let expr = parse(text).expect("parse should succeed");
    expr.evaluate(&HashMap::new(), get_function_implementations())
}

fn eval_with(text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
    let expr = parse(text).expect("parse should succeed");
    expr.evaluate(&vars, get_function_implementations())
}

fn assert_string(v: NetworkResult, expected: &str) {
    match v {
        NetworkResult::String(s) => assert_eq!(s, expected),
        other => panic!(
            "expected String({:?}), got {:?}",
            expected,
            other.infer_data_type()
        ),
    }
}

mod validation_tests {
    use super::*;

    #[test]
    fn test_string_plus_string_validates() {
        let ty = validate("`hello` + `world`").expect("should validate");
        assert_eq!(ty, DataType::String);
    }

    #[test]
    fn test_string_plus_string_with_variables() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), DataType::String);
        vars.insert("b".to_string(), DataType::String);
        // Wrap variables in a template to coerce them to String at the AST
        // level — `a` parses as a bare identifier (Var) which already has
        // type String here.
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string())),
        );
        let ty = expr.validate(&vars, get_function_signatures()).unwrap();
        assert_eq!(ty, DataType::String);
    }

    #[test]
    fn test_chained_string_concat_validates() {
        let ty = validate("`a` + `b` + `c`").expect("should validate");
        assert_eq!(ty, DataType::String);
    }

    #[test]
    fn test_string_minus_string_rejected() {
        let err = validate("`a` - `b`").expect_err("should reject");
        assert!(
            err.contains("not supported"),
            "unexpected error message: {}",
            err
        );
    }

    #[test]
    fn test_string_times_string_rejected() {
        let err = validate("`a` * `b`").expect_err("should reject");
        assert!(err.contains("not supported"), "unexpected error: {}", err);
    }

    #[test]
    fn test_string_div_string_rejected() {
        let err = validate("`a` / `b`").expect_err("should reject");
        assert!(err.contains("not supported"), "unexpected error: {}", err);
    }

    #[test]
    fn test_string_plus_int_rejected() {
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), DataType::Int);
        let err = validate_with("`a` + n", vars).expect_err("should reject");
        assert!(err.contains("not supported"), "unexpected error: {}", err);
    }

    #[test]
    fn test_string_plus_float_rejected() {
        let mut vars = HashMap::new();
        vars.insert("f".to_string(), DataType::Float);
        let err = validate_with("`a` + f", vars).expect_err("should reject");
        assert!(err.contains("not supported"), "unexpected error: {}", err);
    }

    #[test]
    fn test_string_plus_bool_rejected() {
        let mut vars = HashMap::new();
        vars.insert("b".to_string(), DataType::Bool);
        let err = validate_with("`a` + b", vars).expect_err("should reject");
        assert!(err.contains("not supported"), "unexpected error: {}", err);
    }

    #[test]
    fn test_int_plus_string_rejected() {
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), DataType::Int);
        let err = validate_with("n + `a`", vars).expect_err("should reject");
        assert!(err.contains("not supported"), "unexpected error: {}", err);
    }
}

mod evaluation_tests {
    use super::*;

    #[test]
    fn test_evaluate_basic_concat() {
        assert_string(eval("`hello` + ` ` + `world`"), "hello world");
    }

    #[test]
    fn test_evaluate_empty_concat() {
        assert_string(eval("`` + `x`"), "x");
        assert_string(eval("`x` + ``"), "x");
        assert_string(eval("`` + ``"), "");
    }

    #[test]
    fn test_evaluate_left_associative() {
        // `+` is left-associative, but for string concat that's only
        // observable as a parse-tree shape; the result is the same.
        assert_string(eval("`a` + `b` + `c`"), "abc");
    }

    #[test]
    fn test_evaluate_with_template_pieces() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), NetworkResult::String("ada".to_string()));
        vars.insert("n".to_string(), NetworkResult::Int(3));
        // Template auto-converts `n`; `+` glues the two String results.
        assert_string(
            eval_with("`hi ${name}` + `, count=${n}`", vars),
            "hi ada, count=3",
        );
    }

    #[test]
    fn test_evaluate_variable_concat() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), NetworkResult::String("foo".to_string()));
        vars.insert("b".to_string(), NetworkResult::String("bar".to_string()));
        assert_string(eval_with("a + b", vars), "foobar");
    }
}
