use rust_lib_flutter_cad::structure_designer::expr::expr::*;
use rust_lib_flutter_cad::structure_designer::expr::validation::{get_function_signatures, get_function_implementations};
use std::collections::HashMap;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;

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
        let variables = HashMap::new();
        let functions = get_function_signatures();
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::IVec2));
    }

    #[test]
    fn test_vec2_constructor_evaluation() {
        let expr = Expr::Call("vec2".to_string(), vec![
            Expr::Float(3.0),
            Expr::Float(4.0)
        ]);
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
        let result = expr.evaluate(&variables, functions);
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
        let variables = HashMap::new();
        let functions = get_function_signatures();
        
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::Vec3));
    }

    #[test]
    fn test_vec3_constructor_evaluation() {
        let expr = Expr::Call(
            "vec3".to_string(),
            vec![Expr::Float(1.0), Expr::Float(2.0), Expr::Float(3.0)]
        );
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
        let result = expr.evaluate(&variables, functions);
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
        let variables = HashMap::new();
        let functions = get_function_signatures();
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::IVec2));
    }

    #[test]
    fn test_ivec2_constructor_evaluation() {
        let expr = Expr::Call(
            "ivec2".to_string(),
            vec![Expr::Float(5.7), Expr::Float(6.3)] // Should round to 6, 6
        );
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
        let result = expr.evaluate(&variables, functions);
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
        let variables = HashMap::new();
        let functions = get_function_signatures();
        
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::IVec3));
    }

    #[test]
    fn test_ivec3_constructor_evaluation() {
        let expr = Expr::Call(
            "ivec3".to_string(),
            vec![Expr::Float(7.0), Expr::Float(8.0), Expr::Float(9.0)]
        );
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec2);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        let functions = get_function_signatures();
        assert_eq!(expr_x.validate(&variables, functions), Ok(DataType::Float));
        assert_eq!(expr_y.validate(&variables, functions), Ok(DataType::Float));
    }

    #[test]
    fn test_vec2_member_access_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec2(DVec2::new(3.14, 2.71)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        let functions = get_function_implementations();
        match expr_x.evaluate(&variables, functions) {
            NetworkResult::Float(val) => assert_eq!(val, 3.14),
            _ => panic!("Expected Float result"),
        }
        
        match expr_y.evaluate(&variables, functions) {
            NetworkResult::Float(val) => assert_eq!(val, 2.71),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_vec3_member_access_validation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec3);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        let functions = get_function_signatures();
        assert_eq!(expr_x.validate(&variables, functions), Ok(DataType::Float));
        assert_eq!(expr_y.validate(&variables, functions), Ok(DataType::Float));
        assert_eq!(expr_z.validate(&variables, functions), Ok(DataType::Float));
    }

    #[test]
    fn test_vec3_member_access_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        let functions = get_function_implementations();
        match expr_x.evaluate(&variables, functions) {
            NetworkResult::Float(val) => assert_eq!(val, 1.0),
            _ => panic!("Expected Float result"),
        }
        
        match expr_y.evaluate(&variables, functions) {
            NetworkResult::Float(val) => assert_eq!(val, 2.0),
            _ => panic!("Expected Float result"),
        }
        
        match expr_z.evaluate(&variables, functions) {
            NetworkResult::Float(val) => assert_eq!(val, 3.0),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_ivec2_member_access_validation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::IVec2);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        let functions = get_function_signatures();
        assert_eq!(expr_x.validate(&variables, functions), Ok(DataType::Int));
        assert_eq!(expr_y.validate(&variables, functions), Ok(DataType::Int));
    }

    #[test]
    fn test_ivec2_member_access_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::IVec2(IVec2::new(10, 20)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        
        let functions = get_function_implementations();
        match expr_x.evaluate(&variables, functions) {
            NetworkResult::Int(val) => assert_eq!(val, 10),
            _ => panic!("Expected Int result"),
        }
        
        match expr_y.evaluate(&variables, functions) {
            NetworkResult::Int(val) => assert_eq!(val, 20),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_ivec3_member_access_validation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::IVec3);
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        let functions = get_function_signatures();
        assert_eq!(expr_x.validate(&variables, functions), Ok(DataType::Int));
        assert_eq!(expr_y.validate(&variables, functions), Ok(DataType::Int));
        assert_eq!(expr_z.validate(&variables, functions), Ok(DataType::Int));
    }

    #[test]
    fn test_ivec3_member_access_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::IVec3(IVec3::new(100, 200, 300)));
        
        let expr_x = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "x".to_string());
        let expr_y = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "y".to_string());
        let expr_z = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        
        let functions = get_function_implementations();
        match expr_x.evaluate(&variables, functions) {
            NetworkResult::Int(val) => assert_eq!(val, 100),
            _ => panic!("Expected Int result"),
        }
        
        match expr_y.evaluate(&variables, functions) {
            NetworkResult::Int(val) => assert_eq!(val, 200),
            _ => panic!("Expected Int result"),
        }
        
        match expr_z.evaluate(&variables, functions) {
            NetworkResult::Int(val) => assert_eq!(val, 300),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_invalid_member_access() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec2);
        
        // Vec2 doesn't have 'z' component
        let expr = Expr::MemberAccess(Box::new(Expr::Var("v".to_string())), "z".to_string());
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have member 'z'"));
    }

    // ========== Vector Arithmetic Tests ==========

    #[test]
    fn test_vec2_addition_validation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), DataType::Vec2);
        variables.insert("b".to_string(), DataType::Vec2);
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::Vec2));
    }

    #[test]
    fn test_vec2_addition_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec2(DVec2::new(1.0, 2.0)));
        variables.insert("b".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec3(DVec3::new(10.0, 20.0, 30.0)));
        variables.insert("b".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Sub,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::IVec2(IVec2::new(2, 3)));
        variables.insert("b".to_string(), NetworkResult::IVec2(IVec2::new(4, 5)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec2);
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Float(2.0))
        );
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::Vec2));
    }

    #[test]
    fn test_vec2_scalar_multiplication_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Float(2.0))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Float(3.0)),
            BinOp::Mul,
            Box::new(Expr::Var("v".to_string()))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec2(DVec2::new(10.0, 20.0)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Div,
            Box::new(Expr::Float(2.0))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::IVec2(IVec2::new(5, 7)));
        
        // Test with integer literal - should stay IVec2
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Int(3)) // Int literal
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::IVec2(IVec2::new(5, 7)));
        
        // Test with float literal - should promote to Vec2
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Float(3.0)) // Float literal
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), DataType::IVec2);
        variables.insert("b".to_string(), DataType::Vec2);
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::Vec2)); // Should promote to Vec2
    }

    #[test]
    fn test_ivec2_vec2_addition_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::IVec2(IVec2::new(1, 2)));
        variables.insert("b".to_string(), NetworkResult::Vec2(DVec2::new(3.5, 4.5)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec3(DVec3::new(10.5, 20.5, 30.5)));
        variables.insert("b".to_string(), NetworkResult::IVec3(IVec3::new(1, 2, 3)));
        
        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Sub,
            Box::new(Expr::Var("b".to_string()))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
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
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
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
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
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
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 5.0), // 3.0 + 2.0
            _ => panic!("Expected Float result"),
        }
    }

    // ========== Error Cases ==========

    #[test]
    fn test_vector_scalar_addition_error() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec2);
        
        // Vec2 + Float should fail (only Mul/Div allowed for vector-scalar)
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Add,
            Box::new(Expr::Float(2.0))
        );
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported for types"));
    }

    // ========== Vector Math Function Tests ==========

    #[test]
    fn test_length2_validation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec2);
        
        let expr = Expr::Call("length2".to_string(), vec![Expr::Var("v".to_string())]);
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert_eq!(result, Ok(DataType::Float));
    }

    #[test]
    fn test_length2_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Call("length2".to_string(), vec![Expr::Var("v".to_string())]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 5.0), // sqrt(3² + 4²) = 5
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_length3_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 2.0)));
        
        let expr = Expr::Call("length3".to_string(), vec![Expr::Var("v".to_string())]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 3.0), // sqrt(1² + 2² + 2²) = 3
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_normalize2_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Call("normalize2".to_string(), vec![Expr::Var("v".to_string())]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Vec2(vec) => {
                assert!((vec.x - 0.6).abs() < 1e-10); // 3/5 = 0.6
                assert!((vec.y - 0.8).abs() < 1e-10); // 4/5 = 0.8
                assert!((vec.length() - 1.0).abs() < 1e-10); // Should be unit length
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_normalize3_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 2.0)));
        
        let expr = Expr::Call("normalize3".to_string(), vec![Expr::Var("v".to_string())]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Vec3(vec) => {
                assert!((vec.x - 1.0/3.0).abs() < 1e-10); // 1/3
                assert!((vec.y - 2.0/3.0).abs() < 1e-10); // 2/3
                assert!((vec.z - 2.0/3.0).abs() < 1e-10); // 2/3
                assert!((vec.length() - 1.0).abs() < 1e-10); // Should be unit length
            },
            _ => panic!("Expected Vec3 result"),
        }
    }

    #[test]
    fn test_normalize_zero_vector_error() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), NetworkResult::Vec2(DVec2::new(0.0, 0.0)));
        
        let expr = Expr::Call("normalize2".to_string(), vec![Expr::Var("v".to_string())]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("Cannot normalize zero-length vector")),
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_dot2_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec2(DVec2::new(2.0, 3.0)));
        variables.insert("b".to_string(), NetworkResult::Vec2(DVec2::new(4.0, 5.0)));
        
        let expr = Expr::Call("dot2".to_string(), vec![
            Expr::Var("a".to_string()),
            Expr::Var("b".to_string())
        ]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 23.0), // 2*4 + 3*5 = 8 + 15 = 23
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_dot3_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)));
        variables.insert("b".to_string(), NetworkResult::Vec3(DVec3::new(4.0, 5.0, 6.0)));
        
        let expr = Expr::Call("dot3".to_string(), vec![
            Expr::Var("a".to_string()),
            Expr::Var("b".to_string())
        ]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 32.0), // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_cross_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 0.0, 0.0)));
        variables.insert("b".to_string(), NetworkResult::Vec3(DVec3::new(0.0, 1.0, 0.0)));
        
        let expr = Expr::Call("cross".to_string(), vec![
            Expr::Var("a".to_string()),
            Expr::Var("b".to_string())
        ]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Vec3(vec) => {
                assert_eq!(vec.x, 0.0);
                assert_eq!(vec.y, 0.0);
                assert_eq!(vec.z, 1.0); // (1,0,0) × (0,1,0) = (0,0,1)
            },
            _ => panic!("Expected Vec3 result"),
        }
    }

    #[test]
    fn test_distance2_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec2(DVec2::new(0.0, 0.0)));
        variables.insert("b".to_string(), NetworkResult::Vec2(DVec2::new(3.0, 4.0)));
        
        let expr = Expr::Call("distance2".to_string(), vec![
            Expr::Var("a".to_string()),
            Expr::Var("b".to_string())
        ]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 5.0), // distance from origin to (3,4) = 5
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_distance3_evaluation() {
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Vec3(DVec3::new(0.0, 0.0, 0.0)));
        variables.insert("b".to_string(), NetworkResult::Vec3(DVec3::new(1.0, 2.0, 2.0)));
        
        let expr = Expr::Call("distance3".to_string(), vec![
            Expr::Var("a".to_string()),
            Expr::Var("b".to_string())
        ]);
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Float(val) => assert_eq!(val, 3.0), // distance from origin to (1,2,2) = 3
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_complex_vector_math_expression() {
        // Test: normalize2(vec2(3.0, 4.0)) * length2(vec2(6.0, 8.0))
        let variables = HashMap::new();
        let functions = get_function_implementations();
        
        let expr = Expr::Binary(
            Box::new(Expr::Call("normalize2".to_string(), vec![
                Expr::Call("vec2".to_string(), vec![Expr::Float(3.0), Expr::Float(4.0)])
            ])),
            BinOp::Mul,
            Box::new(Expr::Call("length2".to_string(), vec![
                Expr::Call("vec2".to_string(), vec![Expr::Float(6.0), Expr::Float(8.0)])
            ]))
        );
        
        let functions = get_function_implementations();
        let result = expr.evaluate(&variables, functions);
        match result {
            NetworkResult::Vec2(vec) => {
                // normalize2(3,4) = (0.6, 0.8), length2(6,8) = 10
                // (0.6, 0.8) * 10 = (6.0, 8.0)
                assert_eq!(vec.x, 6.0);
                assert_eq!(vec.y, 8.0);
            },
            _ => panic!("Expected Vec2 result"),
        }
    }

    #[test]
    fn test_mismatched_vector_dimensions() {
        let mut variables = HashMap::new();
        variables.insert("v2".to_string(), DataType::Vec2);
        variables.insert("v3".to_string(), DataType::Vec3);
        
        // Vec2 + Vec3 should fail
        let expr = Expr::Binary(
            Box::new(Expr::Var("v2".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("v3".to_string()))
        );
        
        let functions = get_function_signatures();
        let result = expr.validate(&variables, functions);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported for types"));
    }
}
