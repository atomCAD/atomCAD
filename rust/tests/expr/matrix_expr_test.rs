use glam::f64::{DMat3, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::expr::expr::{BinOp, Expr};
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    NetworkResult, dmat3_to_rows, rows_to_dmat3,
};
use std::collections::HashMap;

#[cfg(test)]
mod matrix_tests {
    use super::*;

    fn identity_mat3() -> DMat3 {
        rows_to_dmat3(&[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
    }

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    fn mat3_approx_eq(a: &DMat3, b: &DMat3, tol: f64) -> bool {
        let ar = dmat3_to_rows(a);
        let br = dmat3_to_rows(b);
        for i in 0..3 {
            for j in 0..3 {
                if !approx_eq(ar[i][j], br[i][j], tol) {
                    return false;
                }
            }
        }
        true
    }

    // ========== Binary * : matrix × vector and matrix × matrix ==========

    #[test]
    fn test_mat3_identity_times_vec3() {
        // identity * (1, 2, 3) = (1, 2, 3)
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(identity_mat3()));
        variables.insert(
            "v".to_string(),
            NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)),
        );

        let expr = Expr::Binary(
            Box::new(Expr::Var("m".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("v".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Vec3(v) => {
                assert!(approx_eq(v.x, 1.0, 1e-9));
                assert!(approx_eq(v.y, 2.0, 1e-9));
                assert!(approx_eq(v.z, 3.0, 1e-9));
            }
            _ => panic!("Expected Vec3 result, got {:?}", result.to_display_string()),
        }
    }

    #[test]
    fn test_mat3_times_vec3_row_major_semantics() {
        // m = [[1,2,3],[4,5,6],[7,8,9]], v = (1,2,3)
        // m * v = (1+4+9, 4+10+18, 7+16+27) = (14, 32, 50)
        let m = rows_to_dmat3(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(m));
        variables.insert(
            "v".to_string(),
            NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)),
        );

        let expr = Expr::Binary(
            Box::new(Expr::Var("m".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("v".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Vec3(v) => {
                assert!(approx_eq(v.x, 14.0, 1e-9));
                assert!(approx_eq(v.y, 32.0, 1e-9));
                assert!(approx_eq(v.z, 50.0, 1e-9));
            }
            _ => panic!("Expected Vec3 result"),
        }
    }

    #[test]
    fn test_mat3_times_mat3_composed() {
        // a * identity = a
        let a = rows_to_dmat3(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Mat3(a));
        variables.insert("i".to_string(), NetworkResult::Mat3(identity_mat3()));

        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("i".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Mat3(m) => assert!(mat3_approx_eq(&m, &a, 1e-9)),
            _ => panic!("Expected Mat3 result"),
        }
    }

    #[test]
    fn test_mat3_times_mat3_row_major_composition() {
        // a = [[1,2,3],[4,5,6],[7,8,9]], b = diag(2,3,4)
        // a * b = a with column j scaled by b[j][j]
        //   [[2, 6, 12], [8, 15, 24], [14, 24, 36]]
        let a = rows_to_dmat3(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let b = rows_to_dmat3(&[[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]]);
        let expected = rows_to_dmat3(&[[2.0, 6.0, 12.0], [8.0, 15.0, 24.0], [14.0, 24.0, 36.0]]);

        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::Mat3(a));
        variables.insert("b".to_string(), NetworkResult::Mat3(b));

        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("b".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Mat3(m) => assert!(
                mat3_approx_eq(&m, &expected, 1e-9),
                "rows = {:?}",
                dmat3_to_rows(&m)
            ),
            _ => panic!("Expected Mat3 result"),
        }
    }

    #[test]
    fn test_imat3_times_imat3_integer_preserving() {
        // [[1,0,0],[0,2,0],[0,0,3]] * [[4,0,0],[0,5,0],[0,0,6]] = diag(4,10,18)
        let a: [[i32; 3]; 3] = [[1, 0, 0], [0, 2, 0], [0, 0, 3]];
        let b: [[i32; 3]; 3] = [[4, 0, 0], [0, 5, 0], [0, 0, 6]];

        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::IMat3(a));
        variables.insert("b".to_string(), NetworkResult::IMat3(b));

        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("b".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::IMat3(m) => {
                assert_eq!(m, [[4, 0, 0], [0, 10, 0], [0, 0, 18]]);
            }
            _ => panic!("Expected IMat3 result"),
        }
    }

    #[test]
    fn test_imat3_times_ivec3_integer_preserving() {
        // [[1,2,3],[4,5,6],[7,8,9]] * (1,2,3) = (14, 32, 50)
        let m: [[i32; 3]; 3] = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::IMat3(m));
        variables.insert("v".to_string(), NetworkResult::IVec3(IVec3::new(1, 2, 3)));

        let expr = Expr::Binary(
            Box::new(Expr::Var("m".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("v".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::IVec3(v) => {
                assert_eq!(v.x, 14);
                assert_eq!(v.y, 32);
                assert_eq!(v.z, 50);
            }
            _ => panic!("Expected IVec3 result"),
        }
    }

    #[test]
    fn test_imat3_plus_imat3_component_wise() {
        let a: [[i32; 3]; 3] = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
        let b: [[i32; 3]; 3] = [[9, 8, 7], [6, 5, 4], [3, 2, 1]];
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::IMat3(a));
        variables.insert("b".to_string(), NetworkResult::IMat3(b));

        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Add,
            Box::new(Expr::Var("b".to_string())),
        );

        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::IMat3(m) => {
                assert_eq!(m, [[10, 10, 10], [10, 10, 10], [10, 10, 10]]);
            }
            _ => panic!("Expected IMat3 result"),
        }
    }

    // ========== Member access: .mIJ ==========

    #[test]
    fn test_mat3_member_m11_returns_center() {
        let m = rows_to_dmat3(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(m));

        let expr = Expr::MemberAccess(Box::new(Expr::Var("m".to_string())), "m11".to_string());
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Float(val) => assert!(approx_eq(val, 5.0, 1e-9)),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_mat3_member_all_nine_entries() {
        // Row-major API: .mIJ returns row I, column J.
        let rows = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let m = rows_to_dmat3(&rows);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(m));

        for i in 0..3 {
            for j in 0..3 {
                let name = format!("m{}{}", i, j);
                let expr = Expr::MemberAccess(Box::new(Expr::Var("m".to_string())), name.clone());
                let result = expr.evaluate(&variables, get_function_implementations());
                match result {
                    NetworkResult::Float(val) => assert!(
                        approx_eq(val, rows[i][j], 1e-9),
                        ".{} expected {}, got {}",
                        name,
                        rows[i][j],
                        val
                    ),
                    _ => panic!("Expected Float result for .{}", name),
                }
            }
        }
    }

    #[test]
    fn test_imat3_member_access() {
        let m: [[i32; 3]; 3] = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::IMat3(m));

        for i in 0..3 {
            for j in 0..3 {
                let name = format!("m{}{}", i, j);
                let expr = Expr::MemberAccess(Box::new(Expr::Var("m".to_string())), name.clone());
                let result = expr.evaluate(&variables, get_function_implementations());
                match result {
                    NetworkResult::Int(val) => assert_eq!(val, m[i][j], ".{}", name),
                    _ => panic!("Expected Int result for .{}", name),
                }
            }
        }
    }

    #[test]
    fn test_member_access_validation_type() {
        let mut variables = HashMap::new();
        variables.insert("m".to_string(), DataType::Mat3);
        let expr = Expr::MemberAccess(Box::new(Expr::Var("m".to_string())), "m00".to_string());
        assert_eq!(
            expr.validate(&variables, get_function_signatures()),
            Ok(DataType::Float)
        );

        let mut variables = HashMap::new();
        variables.insert("m".to_string(), DataType::IMat3);
        let expr = Expr::MemberAccess(Box::new(Expr::Var("m".to_string())), "m22".to_string());
        assert_eq!(
            expr.validate(&variables, get_function_signatures()),
            Ok(DataType::Int)
        );
    }

    #[test]
    fn test_invalid_matrix_accessor_rejected_at_validation() {
        let mut variables = HashMap::new();
        variables.insert("m".to_string(), DataType::Mat3);
        let expr = Expr::MemberAccess(Box::new(Expr::Var("m".to_string())), "m33".to_string());
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Mat3") && err.contains("m33"), "got: {}", err);
    }

    // ========== Function calls ==========

    #[test]
    fn test_transpose3_of_imat3_rows_matches_imat3_cols() {
        // transpose3(imat3_rows(a,b,c)) ≡ imat3_cols(a,b,c)
        // Use integer-only inputs so we can compare IMat3s directly via itranspose3.
        let a = (1, 2, 3);
        let b = (4, 5, 6);
        let c = (7, 8, 9);

        let rows_expr = Expr::Call(
            "imat3_rows".to_string(),
            vec![
                Expr::Call(
                    "ivec3".to_string(),
                    vec![Expr::Int(a.0), Expr::Int(a.1), Expr::Int(a.2)],
                ),
                Expr::Call(
                    "ivec3".to_string(),
                    vec![Expr::Int(b.0), Expr::Int(b.1), Expr::Int(b.2)],
                ),
                Expr::Call(
                    "ivec3".to_string(),
                    vec![Expr::Int(c.0), Expr::Int(c.1), Expr::Int(c.2)],
                ),
            ],
        );
        let cols_expr = Expr::Call(
            "imat3_cols".to_string(),
            vec![
                Expr::Call(
                    "ivec3".to_string(),
                    vec![Expr::Int(a.0), Expr::Int(a.1), Expr::Int(a.2)],
                ),
                Expr::Call(
                    "ivec3".to_string(),
                    vec![Expr::Int(b.0), Expr::Int(b.1), Expr::Int(b.2)],
                ),
                Expr::Call(
                    "ivec3".to_string(),
                    vec![Expr::Int(c.0), Expr::Int(c.1), Expr::Int(c.2)],
                ),
            ],
        );
        let transposed = Expr::Call("itranspose3".to_string(), vec![rows_expr.clone()]);

        let variables = HashMap::new();
        let functions = get_function_implementations();
        let t_res = transposed.evaluate(&variables, functions);
        let c_res = cols_expr.evaluate(&variables, functions);

        match (t_res, c_res) {
            (NetworkResult::IMat3(t), NetworkResult::IMat3(c)) => assert_eq!(t, c),
            _ => panic!("Expected two IMat3 results"),
        }
    }

    #[test]
    fn test_det3_identity_is_one() {
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(identity_mat3()));

        let expr = Expr::Call("det3".to_string(), vec![Expr::Var("m".to_string())]);
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Float(val) => assert!(approx_eq(val, 1.0, 1e-9)),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_det3_singular_is_zero() {
        // Two identical rows → singular → det = 0.
        let m = rows_to_dmat3(&[[1.0, 2.0, 3.0], [1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(m));

        let expr = Expr::Call("det3".to_string(), vec![Expr::Var("m".to_string())]);
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Float(val) => assert!(approx_eq(val, 0.0, 1e-9)),
            _ => panic!("Expected Float result"),
        }
    }

    #[test]
    fn test_idet3_integer_matrix() {
        // det([[2,0,0],[0,3,0],[0,0,4]]) = 24
        let m: [[i32; 3]; 3] = [[2, 0, 0], [0, 3, 0], [0, 0, 4]];
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::IMat3(m));

        let expr = Expr::Call("idet3".to_string(), vec![Expr::Var("m".to_string())]);
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Int(val) => assert_eq!(val, 24),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_inv3_singular_returns_error() {
        let m = rows_to_dmat3(&[[1.0, 2.0, 3.0], [1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(m));

        let expr = Expr::Call("inv3".to_string(), vec![Expr::Var("m".to_string())]);
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Error(msg) => assert!(msg.contains("singular"), "got: {}", msg),
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_inv3_times_m_approx_identity() {
        // Non-singular m: m^-1 * m ≈ identity.
        let m = rows_to_dmat3(&[[2.0, 1.0, 0.0], [0.0, 3.0, 1.0], [1.0, 0.0, 2.0]]);
        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("m".to_string(), NetworkResult::Mat3(m));

        // inv3(m) * m
        let inv = Expr::Call("inv3".to_string(), vec![Expr::Var("m".to_string())]);
        let product = Expr::Binary(
            Box::new(inv),
            BinOp::Mul,
            Box::new(Expr::Var("m".to_string())),
        );
        let result = product.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Mat3(res) => {
                let id = identity_mat3();
                assert!(
                    mat3_approx_eq(&res, &id, 1e-9),
                    "got rows {:?}",
                    dmat3_to_rows(&res)
                );
            }
            _ => panic!("Expected Mat3 result"),
        }
    }

    #[test]
    fn test_to_imat3_truncates_mat3_diag() {
        // to_imat3(mat3_diag((1.7, 2.3, 3.5))) -> diag(1, 2, 3)
        let expr = Expr::Call(
            "to_imat3".to_string(),
            vec![Expr::Call(
                "mat3_diag".to_string(),
                vec![Expr::Call(
                    "vec3".to_string(),
                    vec![Expr::Float(1.7), Expr::Float(2.3), Expr::Float(3.5)],
                )],
            )],
        );
        let variables = HashMap::new();
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::IMat3(m) => {
                assert_eq!(m, [[1, 0, 0], [0, 2, 0], [0, 0, 3]]);
            }
            _ => panic!("Expected IMat3 result"),
        }
    }

    // ========== Validation error paths ==========

    #[test]
    fn test_vec3_times_mat3_rejected_at_validation() {
        let mut variables = HashMap::new();
        variables.insert("m".to_string(), DataType::Mat3);
        variables.insert("v".to_string(), DataType::Vec3);
        let expr = Expr::Binary(
            Box::new(Expr::Var("v".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("m".to_string())),
        );
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Mul") && err.contains("Vec3") && err.contains("Mat3"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_transpose3_rejects_non_mat3() {
        let mut variables = HashMap::new();
        variables.insert("v".to_string(), DataType::Vec3);
        let expr = Expr::Call("transpose3".to_string(), vec![Expr::Var("v".to_string())]);
        let result = expr.validate(&variables, get_function_signatures());
        assert!(result.is_err());
    }

    // ========== Implicit IMat3 → Mat3 promotion in binary ops ==========

    #[test]
    fn test_imat3_times_mat3_promotes_to_mat3() {
        let a: [[i32; 3]; 3] = [[1, 0, 0], [0, 2, 0], [0, 0, 3]];
        let b = rows_to_dmat3(&[[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]]);

        let mut variables: HashMap<String, NetworkResult> = HashMap::new();
        variables.insert("a".to_string(), NetworkResult::IMat3(a));
        variables.insert("b".to_string(), NetworkResult::Mat3(b));

        let expr = Expr::Binary(
            Box::new(Expr::Var("a".to_string())),
            BinOp::Mul,
            Box::new(Expr::Var("b".to_string())),
        );
        let result = expr.evaluate(&variables, get_function_implementations());
        match result {
            NetworkResult::Mat3(m) => {
                let expected = rows_to_dmat3(&[[2.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 6.0]]);
                assert!(mat3_approx_eq(&m, &expected, 1e-9));
            }
            _ => panic!("Expected Mat3 result"),
        }
    }
}
