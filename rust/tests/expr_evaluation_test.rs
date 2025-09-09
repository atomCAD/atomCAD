use rust_lib_flutter_cad::structure_designer::expr::expr::*;
use rust_lib_flutter_cad::structure_designer::expr::validation::*;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIDataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;

#[cfg(test)]
mod evaluation_tests {
    use super::*;

    #[test]
    fn test_number_evaluation() {
        let expr = Expr::Float(42.5);
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 42.5),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_bool_evaluation() {
        let expr = Expr::Bool(true);
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_variable_evaluation_success() {
        let expr = Expr::Var("x".to_string());
        let mut context = EvaluationContext::new();
        context.add_variable("x".to_string(), NetworkResult::Float(3.14));
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 3.14),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_variable_evaluation_failure() {
        let expr = Expr::Var("unknown".to_string());
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("Unknown variable: unknown")),
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_arithmetic_addition() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(5.0)),
            BinOp::Add,
            Box::new(Expr::Float(3.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 8.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_arithmetic_subtraction() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(10.0)),
            BinOp::Sub,
            Box::new(Expr::Float(3.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 7.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_arithmetic_multiplication() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(4.0)),
            BinOp::Mul,
            Box::new(Expr::Float(2.5))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 10.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_arithmetic_division() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(15.0)),
            BinOp::Div,
            Box::new(Expr::Float(3.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 5.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_division_by_zero() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(10.0)),
            BinOp::Div,
            Box::new(Expr::Float(0.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("Division by zero")),
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_arithmetic_power() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(2.0)),
            BinOp::Pow,
            Box::new(Expr::Float(3.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 8.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_comparison_less_than() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(3.0)),
            BinOp::Lt,
            Box::new(Expr::Float(5.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_comparison_greater_than() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(7.0)),
            BinOp::Gt,
            Box::new(Expr::Float(5.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_comparison_equality() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(5.0)),
            BinOp::Eq,
            Box::new(Expr::Float(5.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_comparison_inequality() {
        let expr = Expr::Binary(
            Box::new(Expr::Float(3.0)),
            BinOp::Ne,
            Box::new(Expr::Float(5.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_logical_and_true() {
        let expr = Expr::Binary(
            Box::new(Expr::Bool(true)),
            BinOp::And,
            Box::new(Expr::Bool(true))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_logical_and_false() {
        let expr = Expr::Binary(
            Box::new(Expr::Bool(true)),
            BinOp::And,
            Box::new(Expr::Bool(false))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(!val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_logical_or_true() {
        let expr = Expr::Binary(
            Box::new(Expr::Bool(false)),
            BinOp::Or,
            Box::new(Expr::Bool(true))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_logical_or_false() {
        let expr = Expr::Binary(
            Box::new(Expr::Bool(false)),
            BinOp::Or,
            Box::new(Expr::Bool(false))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(!val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_unary_negation() {
        let expr = Expr::Unary(
            UnOp::Neg,
            Box::new(Expr::Float(42.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, -42.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_unary_positive() {
        let expr = Expr::Unary(
            UnOp::Pos,
            Box::new(Expr::Float(42.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 42.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_unary_not_true() {
        let expr = Expr::Unary(
            UnOp::Not,
            Box::new(Expr::Bool(true))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(!val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_unary_not_false() {
        let expr = Expr::Unary(
            UnOp::Not,
            Box::new(Expr::Bool(false))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Bool(val) => assert!(val),
            _ => panic!("Expected Bool result"),
        }
    }

    #[test]
    fn test_function_call_sin() {
        let expr = Expr::Call(
            "sin".to_string(),
            vec![Expr::Float(0.0)]
        );
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert!((val - 0.0).abs() < 1e-10),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_function_call_sqrt() {
        let expr = Expr::Call(
            "sqrt".to_string(),
            vec![Expr::Float(16.0)]
        );
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 4.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_function_call_sqrt_negative() {
        let expr = Expr::Call(
            "sqrt".to_string(),
            vec![Expr::Float(-1.0)]
        );
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("sqrt() of negative number")),
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_function_call_unknown() {
        let expr = Expr::Call(
            "unknown_func".to_string(),
            vec![Expr::Float(1.0)]
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("Unknown function: unknown_func")),
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_conditional_true_branch() {
        let expr = Expr::Conditional(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::Float(42.0)),
            Box::new(Expr::Float(24.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 42.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_conditional_false_branch() {
        let expr = Expr::Conditional(
            Box::new(Expr::Bool(false)),
            Box::new(Expr::Float(42.0)),
            Box::new(Expr::Float(24.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 24.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_complex_expression() {
        // (x + 2.0) * sin(y) where x = 3.0, y = π/2
        let mut context = EvaluationContext::with_standard_functions();
        context.add_variable("x".to_string(), NetworkResult::Float(3.0));
        context.add_variable("y".to_string(), NetworkResult::Float(std::f64::consts::PI / 2.0));
        
        let expr = Expr::Binary(
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
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => {
                // (3.0 + 2.0) * sin(π/2) = 5.0 * 1.0 = 5.0
                assert!((val - 5.0).abs() < 1e-10);
            },
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_nested_conditionals() {
        // if true then (if false then 1.0 else 2.0) else 3.0
        let expr = Expr::Conditional(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::Conditional(
                Box::new(Expr::Bool(false)),
                Box::new(Expr::Float(1.0)),
                Box::new(Expr::Float(2.0))
            )),
            Box::new(Expr::Float(3.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 2.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_error_propagation() {
        // Division by zero should propagate through complex expressions
        let expr = Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Float(10.0)),
                BinOp::Div,
                Box::new(Expr::Float(0.0)) // Division by zero
            )),
            BinOp::Add,
            Box::new(Expr::Float(5.0))
        );
        let context = EvaluationContext::new();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("Division by zero")),
            _ => panic!("Expected Error result"),
        }
    }
}
