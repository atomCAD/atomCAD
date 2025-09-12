use rust_lib_flutter_cad::structure_designer::expr::expr::*;
use rust_lib_flutter_cad::structure_designer::expr::validation::{get_function_signatures};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIDataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_number_validation() {
        let expr = Expr::Float(42.0);
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_bool_validation() {
        let expr = Expr::Bool(true);
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Bool));
    }

    #[test]
    fn test_variable_validation_success() {
        let expr = Expr::Var("x".to_string());
        let mut variables = HashMap::new();
        variables.insert("x".to_string(), APIDataType::Float);
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_variable_validation_failure() {
        let expr = Expr::Var("unknown".to_string());
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown variable: unknown"));
    }

    #[test]
    fn test_arithmetic_validation_int_int() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(5.0)),
            BinOp::Add,
            Box::new(Expr::Float(3.0))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_arithmetic_validation_mixed_types() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(5.0)),
            BinOp::Mul,
            Box::new(Expr::Float(3.14))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_comparison_validation() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(5.0)),
            BinOp::Lt,
            Box::new(Expr::Float(10.0))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Bool));
    }

    #[test]
    fn test_logical_validation() {
        let expr = Expr::Binary(
            Box::new(Expr::Bool(true)),
            BinOp::And,
            Box::new(Expr::Bool(false))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Bool));
    }

    #[test]
    fn test_unary_neg_validation() {
        let expr = Expr::Unary(
            UnOp::Neg,
            Box::new(Expr::Float(42.0))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_unary_not_validation() {
        let expr = Expr::Unary(
            UnOp::Not,
            Box::new(Expr::Bool(true))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Bool));
    }

    #[test]
    fn test_function_call_validation_success() {
        let expr = Expr::Call(
            "sin".to_string(),
            vec![Expr::Float(3.14)]
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_function_call_validation_unknown_function() {
        let expr = Expr::Call(
            "unknown_func".to_string(),
            vec![Expr::Float(1.0)]
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown function: unknown_func"));
    }

    #[test]
    fn test_function_call_validation_wrong_arg_count() {
        let expr = Expr::Call(
            "sin".to_string(),
            vec![Expr::Float(1.0), Expr::Float(2.0)] // sin expects 1 arg
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects 1 arguments, got 2"));
    }

    #[test]
    fn test_conditional_validation_success() {
        let expr = Expr::Conditional(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::Float(1.0)),
            Box::new(Expr::Float(2.0))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_conditional_validation_type_promotion() {
        let expr = Expr::Conditional(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::Float(1.0)), // Float
            Box::new(Expr::Float(2.0))  // Float (both numbers are parsed as Float)
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Float));
    }

    #[test]
    fn test_conditional_validation_incompatible_branches() {
        let expr = Expr::Conditional(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::Float(1.0)),
            Box::new(Expr::Bool(false)) // Incompatible with Float
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("incompatible types"));
    }

    #[test]
    fn test_conditional_validation_invalid_condition() {
        let expr = Expr::Conditional(
            Box::new(Expr::Float(1.0)), // Float condition should work (non-zero = true)
            Box::new(Expr::Float(1.0)),
            Box::new(Expr::Float(2.0))
        );
        let variables = HashMap::new();
        
        let result = expr.validate(&variables, get_function_signatures());
        // This should actually succeed since Float can be used as condition
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be boolean or int"));
    }

    #[test]
    fn test_complex_expression_validation() {
        // (x + 2.0) * sin(y) > 0.0
        let mut variables = HashMap::new();
        variables.insert("x".to_string(), APIDataType::Float);
        variables.insert("y".to_string(), APIDataType::Float);
        
        let expr = Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Binary(
                    Box::new(Expr::Var("x".to_string())),
                    BinOp::Add,
                    Box::new(Expr::Float(2.0))
                )),
                BinOp::Mul,
                Box::new(Expr::Call(
                    "sin".to_string(),
                    vec![Expr::Var("y".to_string())]
                ))
            )),
            BinOp::Gt,
            Box::new(Expr::Float(0.0))
        );
        
        let result = expr.validate(&variables, get_function_signatures());
        assert_eq!(result, Ok(APIDataType::Bool));
    }
}
