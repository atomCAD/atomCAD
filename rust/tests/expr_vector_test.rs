use rust_lib_flutter_cad::structure_designer::expr::expr::*;
use rust_lib_flutter_cad::structure_designer::expr::validation::*;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIDataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};

#[cfg(test)]
mod vector_tests {
    use super::*;

    // ========== Vector Constructor Tests ==========

    #[test]
    fn test_vec2_constructor_validation() {
        let expr = Expr::Call("ivec2".to_string(), vec![
            Expr::Float(5.0),
            Expr::Float(7.0)
        ]);
        let context = ValidationContext::with_standard_functions();
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::IVec2));
    }

    #[test]
    fn test_vec2_constructor_evaluation() {
        let expr = Expr::Call("vec2".to_string(), vec![
            Expr::Float(3.0),
            Expr::Float(4.0)
        ]);
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 3.0);
                assert_eq!(vec.y, 4.0);
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_vec3_constructor_validation() {
        let expr = Expr::Call(
            "vec3".to_string(),
            vec![Expr::Float(1.0), Expr::Float(2.0), Expr::Float(3.0)]
        );
        let context = ValidationContext::with_standard_functions();
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::Vec3));
    }

    #[test]
    fn test_vec3_constructor_evaluation() {
        let expr = Expr::Call(
            "vec3".to_string(),
            vec![Expr::Float(1.0), Expr::Float(2.0), Expr::Float(3.0)]
        );
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec3(vec) => {
                assert_eq!(vec.x, 1.0);
                assert_eq!(vec.y, 2.0);
                assert_eq!(vec.z, 3.0);
            },
            _ => panic!("Expected Vec3 result"),
        }
    }

    #[test]
    fn test_ivec2_constructor_validation() {
        let expr = Expr::Call(
            "ivec2".to_string(),
            vec![Expr::Float(5.0), Expr::Float(6.0)] // Will be converted to int
        );
        let context = ValidationContext::with_standard_functions();
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::IVec2));
    }

    #[test]
    fn test_ivec2_constructor_evaluation() {
        let expr = Expr::Call(
            "ivec2".to_string(),
            vec![Expr::Float(5.7), Expr::Float(6.3)] // Should round to 6, 6
        );
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::IVec2(vec) => {
                assert_eq!(vec.x, 6); // 5.7 rounds to 6
                assert_eq!(vec.y, 6); // 6.3 rounds to 6
            },
            _ => panic!("Expected IVec2 result"),
        }
    }

    #[test]
    fn test_ivec3_constructor_validation() {
        let expr = Expr::Call("ivec3".to_string(), vec![
            Expr::Float(10.0),
            Expr::Float(20.0),
            Expr::Float(30.0)
        ]);
        let context = ValidationContext::with_standard_functions();
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::IVec3));
    }

    #[test]
    fn test_ivec3_constructor_evaluation() {
        let expr = Expr::Call(
            "ivec3".to_string(),
            vec![Expr::Float(7.0), Expr::Float(8.0), Expr::Float(9.0)]
        );
        let context = EvaluationContext::with_standard_functions();
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::IVec3(vec) => {
                assert_eq!(vec.x, 7);
                assert_eq!(vec.y, 8);
                assert_eq!(vec.z, 9);
            },
            _ => panic!("Expected IVec3 result"),
        }
    }

    // ========== Member Access Tests ==========

    #[test]
    fn test_vec2_member_access_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::Vec2);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        assert_eq!(expr_x.validate(&context), Ok(APIDataType::Float));
        assert_eq!(expr_y.validate(&context), Ok(APIDataType::Float));
    }

    #[test]
    fn test_vec2_member_access_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::Vec2(DVec2::new(3.14, 2.71)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        match expr_x.evaluate(&context) {
            NetworkResult::Float(val) => assert_eq!(val, 3.14),
            _ => panic!("Expected Float result"),
        }
        
        match expr_y.evaluate(&context) {
            NetworkResult::Float(val) => assert_eq!(val, 2.71),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_vec3_member_access_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::Vec3);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        assert_eq!(expr_x.validate(&context), Ok(APIDataType::Float));
        assert_eq!(expr_y.validate(&context), Ok(APIDataType::Float));
        assert_eq!(expr_z.validate(&context), Ok(APIDataType::Float));
    }

    #[test]
    fn test_vec3_member_access_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        match expr_x.evaluate(&context) {
            NetworkResult::Float(val) => assert_eq!(val, 1.0),
            _ => panic!("Expected Float result"),
        }
        
        match expr_y.evaluate(&context) {
            NetworkResult::Float(val) => assert_eq!(val, 2.0),
            _ => panic!("Expected Float result"),
        }
        
        match expr_z.evaluate(&context) {
            NetworkResult::Float(val) => assert_eq!(val, 3.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_ivec2_member_access_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::IVec2);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        assert_eq!(expr_x.validate(&context), Ok(APIDataType::Int));
        assert_eq!(expr_y.validate(&context), Ok(APIDataType::Int));
    }

    #[test]
    fn test_ivec2_member_access_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::IVec2(IVec2::new(10, 20)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        match expr_x.evaluate(&context) {
            NetworkResult::Int(val) => assert_eq!(val, 10),
            _ => panic!("Expected Int result"),
        }
        
        match expr_y.evaluate(&context) {
            NetworkResult::Int(val) => assert_eq!(val, 20),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_ivec3_member_access_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::IVec3);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        assert_eq!(expr_x.validate(&context), Ok(APIDataType::Int));
        assert_eq!(expr_y.validate(&context), Ok(APIDataType::Int));
        assert_eq!(expr_z.validate(&context), Ok(APIDataType::Int));
    }

    #[test]
    fn test_ivec3_member_access_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::IVec3(IVec3::new(100, 200, 300)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        match expr_x.evaluate(&context) {
            NetworkResult::Int(val) => assert_eq!(val, 100),
            _ => panic!("Expected Int result"),
        }
        
        match expr_y.evaluate(&context) {
            NetworkResult::Int(val) => assert_eq!(val, 200),
            _ => panic!("Expected Int result"),
        }
        
        match expr_z.evaluate(&context) {
            NetworkResult::Int(val) => assert_eq!(val, 300),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_invalid_member_access() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::Vec2);
        
        // Vec2 doesn't have 'z' component
        let expr = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        let result = expr.validate(&context);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have member 'z'"));
    }

    // ========== Vector Arithmetic Tests ==========

    #[test]
    fn test_vec2_addition_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("a".to_string(), APIDataType::Vec2);
        context.add_variable("b".to_string(), APIDataType::Vec2);
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::Vec2));
    }

    #[test]
    fn test_vec2_addition_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("a".to_string(), NetworkResult::Vec2(DVec2::new(1.0, 2.0)));
        context.add_variable("b".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 4.0); // 1.0 + 3.0
                assert_eq!(vec.y, 6.0); // 2.0 + 4.0
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_vec3_subtraction_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("a".to_string(), NetworkResult::Vec3(DVec3::new(10.0, 20.0, 30.0)));
        context.add_variable("b".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Sub,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec3(vec) => {
                assert_eq!(vec.x, 9.0);  // 10.0 - 1.0
                assert_eq!(vec.y, 18.0); // 20.0 - 2.0
                assert_eq!(vec.z, 27.0); // 30.0 - 3.0
            },
            _ => panic!("Expected Vec3 result"),
        }
    }

    #[test]
    fn test_ivec2_multiplication_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("a".to_string(), NetworkResult::IVec2(IVec2::new(2, 3)));
        context.add_variable("b".to_string(), NetworkResult::IVec2(IVec2::new(4, 5)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::IVec2(vec) => {
                assert_eq!(vec.x, 8);  // 2 * 4
                assert_eq!(vec.y, 15); // 3 * 5
            },
            _ => panic!("Expected IVec2 result"),
        }
    }

    // ========== Vector-Scalar Operations Tests ==========

    #[test]
    fn test_vec2_scalar_multiplication_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::Vec2);
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Float(2.0))
        );
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::Vec2));
    }

    #[test]
    fn test_vec2_scalar_multiplication_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Float(2.0))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 6.0); // 3.0 * 2.0
                assert_eq!(vec.y, 8.0); // 4.0 * 2.0
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_scalar_vec3_multiplication_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Float(3.0)),
            BinOp::Mul,
            Box::new(Expr::Var("v".to_string()))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec3(vec) => {
                assert_eq!(vec.x, 3.0); // 3.0 * 1.0
                assert_eq!(vec.y, 6.0); // 3.0 * 2.0
                assert_eq!(vec.z, 9.0); // 3.0 * 3.0
            },
            _ => panic!("Expected Vec3 result"),
        }
    }

    #[test]
    fn test_vec2_scalar_division_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::Vec2(DVec2::new(10.0, 20.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Div,
            Box::new(Expr::Float(2.0))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 5.0);  // 10.0 / 2.0
                assert_eq!(vec.y, 10.0); // 20.0 / 2.0
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_ivec2_scalar_multiplication_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::IVec2(IVec2::new(5, 7)));
        
        // Test with integer literal - should stay IVec2
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Int(3)) // Int literal
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::IVec2(vec) => {
                assert_eq!(vec.x, 15); // 5 * 3
                assert_eq!(vec.y, 21); // 7 * 3
            },
            _ => panic!("Expected IVec2 result"),
        }
    }

    #[test]
    fn test_ivec2_float_multiplication_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("v".to_string(), NetworkResult::IVec2(IVec2::new(5, 7)));
        
        // Test with float literal - should promote to Vec2
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Float(3.0)) // Float literal
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 15.0); // 5 * 3.0
                assert_eq!(vec.y, 21.0); // 7 * 3.0
            },
            _ => panic!("Expected Vec2 result (IVec2 * Float promotes to Vec2)"),
        }
    }

    // ========== Type Promotion Tests ==========

    #[test]
    fn test_ivec2_vec2_addition_validation() {
        let mut context = ValidationContext::new();
        context.add_variable("a".to_string(), APIDataType::IVec2);
        context.add_variable("b".to_string(), APIDataType::Vec2);
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.validate(&context);
        assert_eq!(result, Ok(APIDataType::Vec2)); // Should promote to Vec2
    }

    #[test]
    fn test_ivec2_vec2_addition_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("a".to_string(), NetworkResult::IVec2(IVec2::new(1, 2)));
        context.add_variable("b".to_string(), NetworkResult::Vec2(DVec2::new(3.5, 4.5)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 4.5); // 1.0 + 3.5
                assert_eq!(vec.y, 6.5); // 2.0 + 4.5
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_vec3_ivec3_subtraction_evaluation() {
        let mut context = EvaluationContext::new();
        context.add_variable("a".to_string(), NetworkResult::Vec3(DVec3::new(10.5, 20.5, 30.5)));
        context.add_variable("b".to_string(), NetworkResult::IVec3(IVec3::new(1, 2, 3)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Sub,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec3(vec) => {
                assert_eq!(vec.x, 9.5);  // 10.5 - 1.0
                assert_eq!(vec.y, 18.5); // 20.5 - 2.0
                assert_eq!(vec.z, 27.5); // 30.5 - 3.0
            },
            _ => panic!("Expected Vec3 result"),
        }
    }

    // ========== Complex Expression Tests ==========

    #[test]
    fn test_complex_vector_expression() {
        // Test: vec2(1.0, 2.0) * 3.0 + vec2(4.0, 5.0)
        let context = EvaluationContext::with_standard_functions();
        
        let expr = Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Call(
                    "vec2".to_string(),
                    vec![Expr::Float(1.0), Expr::Float(2.0)]
                )),
                BinOp::Mul,
                Box::new(Expr::Float(3.0))
            )),
            BinOp::Add,
            Box::new(Expr::Call(
                "vec2".to_string(),
                vec![Expr::Float(4.0), Expr::Float(5.0)]
            ))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Vec2(vec) => {
                assert_eq!(vec.x, 7.0); // (1.0 * 3.0) + 4.0 = 7.0
                assert_eq!(vec.y, 11.0); // (2.0 * 3.0) + 5.0 = 11.0
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_vector_member_access_in_expression() {
        // Test: vec2(3.0, 4.0).x + vec2(1.0, 2.0).y
        let context = EvaluationContext::with_standard_functions();
        
        let expr = Expr::Binary(
            Box::new(Expr::MemberAccess(
                Box::new(Expr::Call(
                    "vec2".to_string(),
                    vec![Expr::Float(3.0), Expr::Float(4.0)]
                )),
                "x".to_string()
            )),
            BinOp::Add,
            Box::new(Expr::MemberAccess(
                Box::new(Expr::Call(
                    "vec2".to_string(),
                    vec![Expr::Float(1.0), Expr::Float(2.0)]
                )),
                "y".to_string()
            ))
        );
        
        let result = expr.evaluate(&context);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 5.0), // 3.0 + 2.0
            _ => panic!("Expected Float result"),
        }
    }

    // ========== Error Cases ==========

    #[test]
    fn test_vector_scalar_addition_error() {
        let mut context = ValidationContext::new();
        context.add_variable("v".to_string(), APIDataType::Vec2);
        
        // Vec2 + Float should fail (only Mul/Div allowed for vector-scalar)
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Add,
            Box::new(Expr::Float(2.0))
        );
        
        let result = expr.validate(&context);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported for types"));
    }

    #[test]
    fn test_mismatched_vector_dimensions() {
        let mut context = ValidationContext::new();
        context.add_variable("v2".to_string(), APIDataType::Vec2);
        context.add_variable("v3".to_string(), APIDataType::Vec3);
        
        // Vec2 + Vec3 should fail
        let expr = Expr::Binary(
            Box::new(Expr::Var("v2".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("v3".to_string()))
        );
        
        let result = expr.validate(&context);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported for types"));
    }
}
