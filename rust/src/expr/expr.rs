use crate::expr::validation::{EvaluationFunction, FunctionSignature};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
    // Comparison operators
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical operators
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i32),
    Float(f64),
    Bool(bool),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    Call(String, Vec<Expr>),
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>), // if condition then expr1 else expr2
    MemberAccess(Box<Expr>, String),              // expr.member (e.g., vec.x, vec.y, vec.z)
    Array(Vec<Expr>),                             // non-empty array literal: [e1, e2, ...]
    EmptyArray(DataType),                         // typed empty array: []Type
    Index(Box<Expr>, Box<Expr>),                  // arr[index]
}

impl Expr {
    /// Validates the expression and returns its inferred type
    pub fn validate(
        &self,
        variables: &HashMap<String, DataType>,
        functions: &HashMap<String, FunctionSignature>,
    ) -> Result<DataType, String> {
        match self {
            Expr::Int(_) => Ok(DataType::Int),
            Expr::Float(_) => Ok(DataType::Float),
            Expr::Bool(_) => Ok(DataType::Bool),
            Expr::Var(name) => variables
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Unknown variable: {}", name)),
            Expr::Unary(op, expr) => {
                let expr_type = expr.validate(variables, functions)?;
                match op {
                    UnOp::Neg | UnOp::Pos => match expr_type {
                        DataType::Int | DataType::Float => Ok(expr_type),
                        _ => Err(format!(
                            "Unary {:?} operator requires numeric type, got {:?}",
                            op, expr_type
                        )),
                    },
                    UnOp::Not => {
                        match expr_type {
                            DataType::Bool => Ok(DataType::Bool),
                            DataType::Int => Ok(DataType::Bool), // Allow int as bool
                            _ => Err(format!(
                                "Logical NOT requires boolean or int type, got {:?}",
                                expr_type
                            )),
                        }
                    }
                }
            }
            Expr::Binary(left, op, right) => {
                let left_type = left.validate(variables, functions)?.clone();
                let right_type = right.validate(variables, functions)?.clone();

                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
                        // Arithmetic operations
                        match (&left_type, &right_type) {
                            // Scalar arithmetic
                            (DataType::Int, DataType::Int) => Ok(DataType::Int),
                            (DataType::Float, DataType::Float) => Ok(DataType::Float),
                            (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => {
                                Ok(DataType::Float)
                            }

                            // Vector-vector arithmetic (component-wise)
                            (DataType::Vec2, DataType::Vec2) => Ok(DataType::Vec2),
                            (DataType::Vec3, DataType::Vec3) => Ok(DataType::Vec3),
                            (DataType::IVec2, DataType::IVec2) => Ok(DataType::IVec2),
                            (DataType::IVec3, DataType::IVec3) => Ok(DataType::IVec3),

                            // Vector type promotion (ivec + vec → vec)
                            (DataType::IVec2, DataType::Vec2)
                            | (DataType::Vec2, DataType::IVec2) => Ok(DataType::Vec2),
                            (DataType::IVec3, DataType::Vec3)
                            | (DataType::Vec3, DataType::IVec3) => Ok(DataType::Vec3),

                            // Matrix +, -: component-wise on matching matrix types.
                            // Matrix *: matrix product (Mat3 × Mat3) or matrix-vector (Mat3 × Vec3).
                            // See doc/design_matrix_types.md D7.
                            (DataType::Mat3, DataType::Mat3)
                                if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) =>
                            {
                                Ok(DataType::Mat3)
                            }
                            (DataType::IMat3, DataType::IMat3)
                                if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) =>
                            {
                                Ok(DataType::IMat3)
                            }
                            // Matrix type promotion (IMat3 + Mat3 → Mat3) for all three ops.
                            (DataType::IMat3, DataType::Mat3)
                            | (DataType::Mat3, DataType::IMat3)
                                if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) =>
                            {
                                Ok(DataType::Mat3)
                            }
                            // Matrix × Vector (left-multiply only; Vec3 × Mat3 is rejected).
                            (DataType::Mat3, DataType::Vec3) if matches!(op, BinOp::Mul) => {
                                Ok(DataType::Vec3)
                            }
                            (DataType::IMat3, DataType::IVec3) if matches!(op, BinOp::Mul) => {
                                Ok(DataType::IVec3)
                            }
                            // Mixed matrix-vector with promotion.
                            (DataType::Mat3, DataType::IVec3) if matches!(op, BinOp::Mul) => {
                                Ok(DataType::Vec3)
                            }
                            (DataType::IMat3, DataType::Vec3) if matches!(op, BinOp::Mul) => {
                                Ok(DataType::Vec3)
                            }

                            // Vector-scalar operations (only for Mul and Div)
                            (DataType::Vec2, DataType::Float)
                            | (DataType::Float, DataType::Vec2)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::Vec2)
                            }
                            (DataType::Vec3, DataType::Float)
                            | (DataType::Float, DataType::Vec3)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::Vec3)
                            }
                            (DataType::IVec2, DataType::Int) | (DataType::Int, DataType::IVec2)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::IVec2)
                            }
                            (DataType::IVec3, DataType::Int) | (DataType::Int, DataType::IVec3)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::IVec3)
                            }

                            // Mixed vector-scalar with promotion
                            (DataType::Vec2, DataType::Int) | (DataType::Int, DataType::Vec2)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::Vec2)
                            }
                            (DataType::Vec3, DataType::Int) | (DataType::Int, DataType::Vec3)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::Vec3)
                            }
                            (DataType::IVec2, DataType::Float)
                            | (DataType::Float, DataType::IVec2)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::Vec2)
                            }
                            (DataType::IVec3, DataType::Float)
                            | (DataType::Float, DataType::IVec3)
                                if matches!(op, BinOp::Mul | BinOp::Div) =>
                            {
                                Ok(DataType::Vec3)
                            }

                            _ => Err(format!(
                                "Arithmetic operation {:?} not supported for types {:?} and {:?}",
                                op, left_type, right_type
                            )),
                        }
                    }
                    BinOp::Mod => {
                        // Modulo operation - only works with integers
                        match (&left_type, &right_type) {
                            (DataType::Int, DataType::Int) => Ok(DataType::Int),
                            _ => Err(format!(
                                "Modulo operation not supported for types {:?} and {:?}",
                                left_type, right_type
                            )),
                        }
                    }
                    BinOp::Eq | BinOp::Ne => {
                        // Equality comparison - can compare any compatible types
                        if Self::types_compatible(&left_type, &right_type) {
                            Ok(DataType::Bool)
                        } else {
                            Err(format!(
                                "Cannot compare incompatible types {:?} and {:?}",
                                left_type, right_type
                            ))
                        }
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        // Ordering comparison - requires numeric types
                        match (&left_type, &right_type) {
                            (DataType::Int, DataType::Int)
                            | (DataType::Float, DataType::Float)
                            | (DataType::Int, DataType::Float)
                            | (DataType::Float, DataType::Int) => Ok(DataType::Bool),
                            _ => Err(format!(
                                "Comparison operation {:?} requires numeric types, got {:?} and {:?}",
                                op, left_type, right_type
                            )),
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        // Logical operations
                        match (&left_type, &right_type) {
                            (DataType::Bool, DataType::Bool) => Ok(DataType::Bool),
                            (DataType::Int, DataType::Int) => Ok(DataType::Bool), // Allow int as bool
                            (DataType::Bool, DataType::Int) | (DataType::Int, DataType::Bool) => {
                                Ok(DataType::Bool)
                            }
                            _ => Err(format!(
                                "Logical operation {:?} requires boolean or int types, got {:?} and {:?}",
                                op, left_type, right_type
                            )),
                        }
                    }
                }
            }
            Expr::Call(name, args) => {
                // Special-case `len`: polymorphic over any Array[T], cannot be expressed
                // by FunctionSignature's fixed parameter_types model.
                if name == "len" {
                    if args.len() != 1 {
                        return Err(format!(
                            "Function len expects 1 argument, got {}",
                            args.len()
                        ));
                    }
                    let arg_type = args[0].validate(variables, functions)?;
                    match arg_type {
                        DataType::Array(_) => return Ok(DataType::Int),
                        other => {
                            return Err(format!(
                                "Function len argument 1 expects an array type, got {}",
                                other
                            ));
                        }
                    }
                }

                // Special-case `concat`: polymorphic over Array[T1] × Array[T2] → Array[unify(T1,T2)].
                // Same rationale as `len` — the polymorphic shape doesn't fit FunctionSignature.
                if name == "concat" {
                    if args.len() != 2 {
                        return Err(format!(
                            "Function concat expects 2 arguments, got {}",
                            args.len()
                        ));
                    }
                    let a_ty = args[0].validate(variables, functions)?;
                    let b_ty = args[1].validate(variables, functions)?;
                    let elem_a = match a_ty {
                        DataType::Array(t) => *t,
                        other => {
                            return Err(format!(
                                "Function concat argument 1 expects an array type, got {}",
                                other
                            ));
                        }
                    };
                    let elem_b = match b_ty {
                        DataType::Array(t) => *t,
                        other => {
                            return Err(format!(
                                "Function concat argument 2 expects an array type, got {}",
                                other
                            ));
                        }
                    };
                    let elem = unify_array_element_types(&elem_a, &elem_b).map_err(|_| {
                        format!(
                            "concat arguments have incompatible element types: {} and {}",
                            elem_a, elem_b
                        )
                    })?;
                    return Ok(DataType::Array(Box::new(elem)));
                }

                // Special-case `append`: polymorphic over Array[T] × U → Array[unify(T, U)].
                // Same rationale as `len` and `concat` — the polymorphic shape doesn't fit
                // FunctionSignature.
                if name == "append" {
                    if args.len() != 2 {
                        return Err(format!(
                            "Function append expects 2 arguments, got {}",
                            args.len()
                        ));
                    }
                    let arr_ty = args[0].validate(variables, functions)?;
                    let elem_ty = args[1].validate(variables, functions)?;
                    let arr_elem = match arr_ty {
                        DataType::Array(t) => *t,
                        other => {
                            return Err(format!(
                                "Function append argument 1 expects an array type, got {}",
                                other
                            ));
                        }
                    };
                    let unified = unify_array_element_types(&arr_elem, &elem_ty).map_err(|_| {
                        format!(
                            "append element type {} is incompatible with array element type {}",
                            elem_ty, arr_elem
                        )
                    })?;
                    return Ok(DataType::Array(Box::new(unified)));
                }

                // Validate function exists
                let signature = functions
                    .get(name)
                    .ok_or_else(|| format!("Unknown function: {}", name))?;

                // Check argument count
                if args.len() != signature.parameter_types.len() {
                    return Err(format!(
                        "Function {} expects {} arguments, got {}",
                        name,
                        signature.parameter_types.len(),
                        args.len()
                    ));
                }

                // Validate each argument type
                for (i, (arg, expected_type)) in
                    args.iter().zip(&signature.parameter_types).enumerate()
                {
                    let arg_type = arg.validate(variables, functions)?;
                    if !Self::types_compatible(&arg_type, expected_type) {
                        return Err(format!(
                            "Function {} argument {} expects type {}, got {}",
                            name,
                            i + 1,
                            expected_type,
                            arg_type
                        ));
                    }
                }

                Ok(signature.return_type.clone())
            }
            Expr::Conditional(condition, then_expr, else_expr) => {
                let condition_type = condition.validate(variables, functions)?;
                let then_type = then_expr.validate(variables, functions)?;
                let else_type = else_expr.validate(variables, functions)?;

                // Condition must be boolean or int
                match condition_type {
                    DataType::Bool | DataType::Int => {}
                    _ => {
                        return Err(format!(
                            "Conditional condition must be boolean or int, got {}",
                            condition_type
                        ));
                    }
                }

                // Then and else branches must have compatible types
                if Self::types_compatible(&then_type, &else_type) {
                    // Return the more general type
                    match (&then_type, &else_type) {
                        (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => {
                            Ok(DataType::Float)
                        }
                        _ => Ok(then_type.clone()), // Same types or other compatible combinations
                    }
                } else {
                    Err(format!(
                        "Conditional branches have incompatible types: {} and {}",
                        then_type, else_type
                    ))
                }
            }
            Expr::MemberAccess(expr, member) => {
                let expr_type = expr.validate(variables, functions)?.clone();
                match (expr_type.clone(), member.as_str()) {
                    // Vec2 components
                    (DataType::Vec2, "x" | "y") => Ok(DataType::Float),
                    // Vec3 components
                    (DataType::Vec3, "x" | "y" | "z") => Ok(DataType::Float),
                    // IVec2 components
                    (DataType::IVec2, "x" | "y") => Ok(DataType::Int),
                    // IVec3 components
                    (DataType::IVec3, "x" | "y" | "z") => Ok(DataType::Int),
                    // Mat3 / IMat3 `.mIJ` accessors (I, J in 0..=2; row i, column j).
                    // See doc/design_matrix_types.md D7.
                    (DataType::Mat3, m) if is_matrix_accessor(m) => Ok(DataType::Float),
                    (DataType::IMat3, m) if is_matrix_accessor(m) => Ok(DataType::Int),
                    _ => Err(format!(
                        "Type {} does not have member '{}'",
                        expr_type, member
                    )),
                }
            }
            Expr::EmptyArray(t) => Ok(DataType::Array(Box::new(t.clone()))),
            Expr::Index(arr, idx) => {
                let arr_ty = arr.validate(variables, functions)?;
                let idx_ty = idx.validate(variables, functions)?;
                let elem_ty = match arr_ty {
                    DataType::Array(inner) => *inner,
                    other => {
                        return Err(format!("cannot index into non-array type {}", other));
                    }
                };
                if !matches!(idx_ty, DataType::Int) {
                    return Err(format!("array index must be Int, got {}", idx_ty));
                }
                Ok(elem_ty)
            }
            Expr::Array(elements) => {
                // elements is non-empty by construction (parser produces this only
                // when at least one element was parsed).
                let mut unified = elements[0].validate(variables, functions)?;
                for (i, e) in elements.iter().enumerate().skip(1) {
                    let ti = e.validate(variables, functions)?;
                    unified = unify_array_element_types(&unified, &ti).map_err(|_| {
                        format!(
                            "array element {} has type {}, incompatible with prior element type {}",
                            i, ti, unified
                        )
                    })?;
                }
                Ok(DataType::Array(Box::new(unified)))
            }
        }
    }

    /// Evaluates the expression and returns the result
    pub fn evaluate(
        &self,
        variables: &HashMap<String, NetworkResult>,
        functions: &HashMap<String, EvaluationFunction>,
    ) -> NetworkResult {
        match self {
            Expr::Int(n) => NetworkResult::Int(*n),
            Expr::Float(n) => NetworkResult::Float(*n),
            Expr::Bool(b) => NetworkResult::Bool(*b),
            Expr::Var(name) => variables
                .get(name)
                .cloned()
                .unwrap_or_else(|| NetworkResult::Error(format!("Unknown variable: {}", name))),
            Expr::Unary(op, expr) => {
                let value = expr.evaluate(variables, functions);
                if let NetworkResult::Error(_) = value {
                    return value;
                }

                match op {
                    UnOp::Neg => match value {
                        NetworkResult::Int(n) => NetworkResult::Int(-n),
                        NetworkResult::Float(n) => NetworkResult::Float(-n),
                        _ => NetworkResult::Error("Negation requires numeric type".to_string()),
                    },
                    UnOp::Pos => match value {
                        NetworkResult::Int(_) | NetworkResult::Float(_) => value,
                        _ => NetworkResult::Error(
                            "Positive operator requires numeric type".to_string(),
                        ),
                    },
                    UnOp::Not => match value {
                        NetworkResult::Bool(b) => NetworkResult::Bool(!b),
                        NetworkResult::Int(n) => NetworkResult::Bool(n == 0),
                        _ => NetworkResult::Error(
                            "Logical NOT requires boolean or int type".to_string(),
                        ),
                    },
                }
            }
            Expr::Binary(left, op, right) => {
                let left_val = left.evaluate(variables, functions);
                if let NetworkResult::Error(_) = left_val {
                    return left_val;
                }

                let right_val = right.evaluate(variables, functions);
                if let NetworkResult::Error(_) = right_val {
                    return right_val;
                }

                Self::evaluate_binary_op(left_val, *op, right_val)
            }
            Expr::Call(name, args) => {
                // Evaluate all arguments first
                let mut arg_values = Vec::new();
                for arg in args {
                    let val = arg.evaluate(variables, functions);
                    if let NetworkResult::Error(_) = val {
                        return val;
                    }
                    arg_values.push(val);
                }

                // Call the function
                if let Some(func) = functions.get(name) {
                    func(&arg_values)
                } else {
                    NetworkResult::Error(format!("Unknown function: {}", name))
                }
            }
            Expr::Conditional(condition, then_expr, else_expr) => {
                let condition_val = condition.evaluate(variables, functions);
                if let NetworkResult::Error(_) = condition_val {
                    return condition_val;
                }

                let is_true = match condition_val {
                    NetworkResult::Bool(b) => b,
                    NetworkResult::Int(n) => n != 0,
                    _ => {
                        return NetworkResult::Error(
                            "Conditional condition must be boolean or int".to_string(),
                        );
                    }
                };

                if is_true {
                    then_expr.evaluate(variables, functions)
                } else {
                    else_expr.evaluate(variables, functions)
                }
            }
            Expr::MemberAccess(expr, member) => {
                let value = expr.evaluate(variables, functions);
                if let NetworkResult::Error(_) = value {
                    return value;
                }

                match (value, member.as_str()) {
                    // Vec2 components
                    (NetworkResult::Vec2(vec), "x") => NetworkResult::Float(vec.x),
                    (NetworkResult::Vec2(vec), "y") => NetworkResult::Float(vec.y),
                    // Vec3 components
                    (NetworkResult::Vec3(vec), "x") => NetworkResult::Float(vec.x),
                    (NetworkResult::Vec3(vec), "y") => NetworkResult::Float(vec.y),
                    (NetworkResult::Vec3(vec), "z") => NetworkResult::Float(vec.z),
                    // IVec2 components
                    (NetworkResult::IVec2(vec), "x") => NetworkResult::Int(vec.x),
                    (NetworkResult::IVec2(vec), "y") => NetworkResult::Int(vec.y),
                    // IVec3 components
                    (NetworkResult::IVec3(vec), "x") => NetworkResult::Int(vec.x),
                    (NetworkResult::IVec3(vec), "y") => NetworkResult::Int(vec.y),
                    (NetworkResult::IVec3(vec), "z") => NetworkResult::Int(vec.z),
                    // Mat3 `.mIJ`: public API is row-major, storage is column-major glam DMat3.
                    // `.mIJ` returns `m.col(J)[I]`. See doc/design_matrix_types.md §"Row-major
                    // API over column-major storage".
                    (NetworkResult::Mat3(m), name) => match parse_matrix_accessor(name) {
                        Some((i, j)) => NetworkResult::Float(m.col(j)[i]),
                        None => NetworkResult::Error(format!(
                            "Cannot access member '{}' on value",
                            member
                        )),
                    },
                    // IMat3 is stored row-major directly: `.mIJ` = `m[I][J]`.
                    (NetworkResult::IMat3(m), name) => match parse_matrix_accessor(name) {
                        Some((i, j)) => NetworkResult::Int(m[i][j]),
                        None => NetworkResult::Error(format!(
                            "Cannot access member '{}' on value",
                            member
                        )),
                    },
                    _ => {
                        NetworkResult::Error(format!("Cannot access member '{}' on value", member))
                    }
                }
            }
            Expr::EmptyArray(_) => NetworkResult::Array(vec![]),
            Expr::Index(arr, idx) => {
                let arr_v = arr.evaluate(variables, functions);
                if let NetworkResult::Error(_) = arr_v {
                    return arr_v;
                }
                let idx_v = idx.evaluate(variables, functions);
                if let NetworkResult::Error(_) = idx_v {
                    return idx_v;
                }

                let elements = match arr_v {
                    NetworkResult::Array(v) => v,
                    _ => return NetworkResult::Error("indexing non-array value".into()),
                };
                let i = match idx_v {
                    NetworkResult::Int(n) => n,
                    _ => return NetworkResult::Error("array index must be Int".into()),
                };
                if i < 0 || (i as usize) >= elements.len() {
                    return NetworkResult::Error(format!(
                        "array index {} out of bounds for array of length {}",
                        i,
                        elements.len()
                    ));
                }
                elements.into_iter().nth(i as usize).unwrap()
            }
            Expr::Array(elements) => {
                let mut out = Vec::with_capacity(elements.len());
                for e in elements {
                    let v = e.evaluate(variables, functions);
                    if let NetworkResult::Error(_) = v {
                        return v;
                    }
                    out.push(v);
                }
                NetworkResult::Array(out)
            }
        }
    }

    /// Helper function to check if two types are compatible for operations
    fn types_compatible(type1: &DataType, type2: &DataType) -> bool {
        match (type1, type2) {
            // Same types are always compatible
            (a, b) if a == b => true,
            // Numeric types are compatible with each other
            (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => true,
            // Bool and Int are compatible for logical operations
            (DataType::Bool, DataType::Int) | (DataType::Int, DataType::Bool) => true,
            // Vector type compatibility (for comparisons)
            (DataType::IVec2, DataType::Vec2) | (DataType::Vec2, DataType::IVec2) => true,
            (DataType::IVec3, DataType::Vec3) | (DataType::Vec3, DataType::IVec3) => true,
            // Matrix type compatibility (for comparisons / function argument promotion)
            (DataType::IMat3, DataType::Mat3) | (DataType::Mat3, DataType::IMat3) => true,
            _ => false,
        }
    }

    /// Helper function to evaluate binary operations
    fn evaluate_binary_op(left: NetworkResult, op: BinOp, right: NetworkResult) -> NetworkResult {
        // Matrix-involving arithmetic (+, -, *) uses bespoke logic: `*` is matrix
        // product / matrix-vector, not component-wise like the scalar/vector case
        // handled by `arithmetic_op`.
        if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul)
            && (matches!(left, NetworkResult::Mat3(_) | NetworkResult::IMat3(_))
                || matches!(right, NetworkResult::Mat3(_) | NetworkResult::IMat3(_)))
        {
            return Self::matrix_arith_op(left, right, op);
        }

        match op {
            BinOp::Add => Self::arithmetic_op(left, right, |a, b| a + b, |a, b| a + b),
            BinOp::Sub => Self::arithmetic_op(left, right, |a, b| a - b, |a, b| a - b),
            BinOp::Mul => Self::arithmetic_op(left, right, |a, b| a * b, |a, b| a * b),
            BinOp::Div => {
                // Check for division by zero first
                match &right {
                    NetworkResult::Int(0) => {
                        return NetworkResult::Error("Division by zero".to_string());
                    }
                    NetworkResult::Float(f) if *f == 0.0 => {
                        return NetworkResult::Error("Division by zero".to_string());
                    }
                    _ => {}
                }
                Self::arithmetic_op(left, right, |a, b| a / b, |a, b| a / b)
            }
            BinOp::Mod => {
                // Check for modulo by zero first
                if let NetworkResult::Int(0) = &right {
                    return NetworkResult::Error("Modulo by zero".to_string());
                }
                // Modulo only works with integers
                match (left, right) {
                    (NetworkResult::Int(a), NetworkResult::Int(b)) => NetworkResult::Int(a % b),
                    _ => NetworkResult::Error(
                        "Modulo operation requires integer operands".to_string(),
                    ),
                }
            }
            BinOp::Pow => {
                Self::arithmetic_op(left, right, |a, b| a.pow(b as u32), |a, b| a.powf(b))
            }
            BinOp::Eq => Self::comparison_op(
                left,
                right,
                |a, b| a == b,
                |a, b| (a - b).abs() < f64::EPSILON,
            ),
            BinOp::Ne => Self::comparison_op(
                left,
                right,
                |a, b| a != b,
                |a, b| (a - b).abs() >= f64::EPSILON,
            ),
            BinOp::Lt => Self::comparison_op(left, right, |a, b| a < b, |a, b| a < b),
            BinOp::Le => Self::comparison_op(left, right, |a, b| a <= b, |a, b| a <= b),
            BinOp::Gt => Self::comparison_op(left, right, |a, b| a > b, |a, b| a > b),
            BinOp::Ge => Self::comparison_op(left, right, |a, b| a >= b, |a, b| a >= b),
            BinOp::And => Self::logical_op(left, right, |a, b| a && b),
            BinOp::Or => Self::logical_op(left, right, |a, b| a || b),
        }
    }

    /// Helper for arithmetic operations
    fn arithmetic_op<F1, F2>(
        left: NetworkResult,
        right: NetworkResult,
        int_op: F1,
        float_op: F2,
    ) -> NetworkResult
    where
        F1: Fn(i32, i32) -> i32,
        F2: Fn(f64, f64) -> f64,
    {
        use glam::f64::{DVec2, DVec3};
        use glam::i32::{IVec2, IVec3};

        match (left, right) {
            // Scalar operations
            (NetworkResult::Int(a), NetworkResult::Int(b)) => NetworkResult::Int(int_op(a, b)),
            (NetworkResult::Float(a), NetworkResult::Float(b)) => {
                NetworkResult::Float(float_op(a, b))
            }
            (NetworkResult::Int(a), NetworkResult::Float(b)) => {
                NetworkResult::Float(float_op(a as f64, b))
            }
            (NetworkResult::Float(a), NetworkResult::Int(b)) => {
                NetworkResult::Float(float_op(a, b as f64))
            }

            // Vector-vector operations (component-wise)
            (NetworkResult::Vec2(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b.x), float_op(a.y, b.y)))
            }
            (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a.x, b.x),
                float_op(a.y, b.y),
                float_op(a.z, b.z),
            )),
            (NetworkResult::IVec2(a), NetworkResult::IVec2(b)) => {
                NetworkResult::IVec2(IVec2::new(int_op(a.x, b.x), int_op(a.y, b.y)))
            }
            (NetworkResult::IVec3(a), NetworkResult::IVec3(b)) => NetworkResult::IVec3(IVec3::new(
                int_op(a.x, b.x),
                int_op(a.y, b.y),
                int_op(a.z, b.z),
            )),

            // Vector type promotion (ivec + vec → vec)
            (NetworkResult::IVec2(a), NetworkResult::Vec2(b)) => NetworkResult::Vec2(DVec2::new(
                float_op(a.x as f64, b.x),
                float_op(a.y as f64, b.y),
            )),
            (NetworkResult::Vec2(a), NetworkResult::IVec2(b)) => NetworkResult::Vec2(DVec2::new(
                float_op(a.x, b.x as f64),
                float_op(a.y, b.y as f64),
            )),
            (NetworkResult::IVec3(a), NetworkResult::Vec3(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a.x as f64, b.x),
                float_op(a.y as f64, b.y),
                float_op(a.z as f64, b.z),
            )),
            (NetworkResult::Vec3(a), NetworkResult::IVec3(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a.x, b.x as f64),
                float_op(a.y, b.y as f64),
                float_op(a.z, b.z as f64),
            )),

            // Vector-scalar operations (only for multiplication and division)
            (NetworkResult::Vec2(a), NetworkResult::Float(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b), float_op(a.y, b)))
            }
            (NetworkResult::Float(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a, b.x), float_op(a, b.y)))
            }
            (NetworkResult::Vec3(a), NetworkResult::Float(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a.x, b),
                float_op(a.y, b),
                float_op(a.z, b),
            )),
            (NetworkResult::Float(a), NetworkResult::Vec3(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a, b.x),
                float_op(a, b.y),
                float_op(a, b.z),
            )),
            (NetworkResult::IVec2(a), NetworkResult::Int(b)) => {
                NetworkResult::IVec2(IVec2::new(int_op(a.x, b), int_op(a.y, b)))
            }
            (NetworkResult::Int(a), NetworkResult::IVec2(b)) => {
                NetworkResult::IVec2(IVec2::new(int_op(a, b.x), int_op(a, b.y)))
            }
            (NetworkResult::IVec3(a), NetworkResult::Int(b)) => {
                NetworkResult::IVec3(IVec3::new(int_op(a.x, b), int_op(a.y, b), int_op(a.z, b)))
            }
            (NetworkResult::Int(a), NetworkResult::IVec3(b)) => {
                NetworkResult::IVec3(IVec3::new(int_op(a, b.x), int_op(a, b.y), int_op(a, b.z)))
            }

            // Mixed vector-scalar with promotion
            (NetworkResult::Vec2(a), NetworkResult::Int(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b as f64), float_op(a.y, b as f64)))
            }
            (NetworkResult::Int(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a as f64, b.x), float_op(a as f64, b.y)))
            }
            (NetworkResult::Vec3(a), NetworkResult::Int(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a.x, b as f64),
                float_op(a.y, b as f64),
                float_op(a.z, b as f64),
            )),
            (NetworkResult::Int(a), NetworkResult::Vec3(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a as f64, b.x),
                float_op(a as f64, b.y),
                float_op(a as f64, b.z),
            )),
            (NetworkResult::IVec2(a), NetworkResult::Float(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x as f64, b), float_op(a.y as f64, b)))
            }
            (NetworkResult::Float(a), NetworkResult::IVec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a, b.x as f64), float_op(a, b.y as f64)))
            }
            (NetworkResult::IVec3(a), NetworkResult::Float(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a.x as f64, b),
                float_op(a.y as f64, b),
                float_op(a.z as f64, b),
            )),
            (NetworkResult::Float(a), NetworkResult::IVec3(b)) => NetworkResult::Vec3(DVec3::new(
                float_op(a, b.x as f64),
                float_op(a, b.y as f64),
                float_op(a, b.z as f64),
            )),

            _ => NetworkResult::Error(
                "Arithmetic operation not supported for these types".to_string(),
            ),
        }
    }

    /// Helper for comparison operations
    fn comparison_op<F1, F2>(
        left: NetworkResult,
        right: NetworkResult,
        int_op: F1,
        float_op: F2,
    ) -> NetworkResult
    where
        F1: FnOnce(i32, i32) -> bool,
        F2: FnOnce(f64, f64) -> bool,
    {
        match (left, right) {
            (NetworkResult::Int(a), NetworkResult::Int(b)) => NetworkResult::Bool(int_op(a, b)),
            (NetworkResult::Float(a), NetworkResult::Float(b)) => {
                NetworkResult::Bool(float_op(a, b))
            }
            (NetworkResult::Int(a), NetworkResult::Float(b)) => {
                NetworkResult::Bool(float_op(a as f64, b))
            }
            (NetworkResult::Float(a), NetworkResult::Int(b)) => {
                NetworkResult::Bool(float_op(a, b as f64))
            }
            (NetworkResult::Bool(a), NetworkResult::Bool(b)) => {
                NetworkResult::Bool(int_op(a as i32, b as i32))
            }
            _ => NetworkResult::Error("Comparison operation requires compatible types".to_string()),
        }
    }

    /// Helper for logical operations
    fn logical_op<F>(left: NetworkResult, right: NetworkResult, op: F) -> NetworkResult
    where
        F: FnOnce(bool, bool) -> bool,
    {
        let left_bool = match left {
            NetworkResult::Bool(b) => b,
            NetworkResult::Int(n) => n != 0,
            _ => {
                return NetworkResult::Error(
                    "Logical operation requires boolean or int types".to_string(),
                );
            }
        };

        let right_bool = match right {
            NetworkResult::Bool(b) => b,
            NetworkResult::Int(n) => n != 0,
            _ => {
                return NetworkResult::Error(
                    "Logical operation requires boolean or int types".to_string(),
                );
            }
        };

        NetworkResult::Bool(op(left_bool, right_bool))
    }

    /// Convert the expression to prefix notation string representation
    pub fn to_prefix_string(&self) -> String {
        match self {
            Expr::Int(n) => n.to_string(),
            Expr::Float(n) => n.to_string(),
            Expr::Bool(b) => b.to_string(),
            Expr::Var(name) => name.clone(),
            Expr::Unary(op, expr) => {
                let op_str = match op {
                    UnOp::Neg => "neg",
                    UnOp::Pos => "pos",
                    UnOp::Not => "not",
                };
                format!("({} {})", op_str, expr.to_prefix_string())
            }
            Expr::Binary(left, op, right) => {
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Pow => "^",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                format!(
                    "({} {} {})",
                    op_str,
                    left.to_prefix_string(),
                    right.to_prefix_string()
                )
            }
            Expr::Call(name, args) => {
                let args_str = args
                    .iter()
                    .map(|arg| arg.to_prefix_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                if args.is_empty() {
                    format!("(call {})", name)
                } else {
                    format!("(call {} {})", name, args_str)
                }
            }
            Expr::Conditional(condition, then_expr, else_expr) => {
                format!(
                    "(if {} then {} else {})",
                    condition.to_prefix_string(),
                    then_expr.to_prefix_string(),
                    else_expr.to_prefix_string()
                )
            }
            Expr::MemberAccess(expr, member) => {
                format!("(. {} {})", expr.to_prefix_string(), member)
            }
            Expr::Array(elements) => {
                let elems_str = elements
                    .iter()
                    .map(|e| e.to_prefix_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("(array {})", elems_str)
            }
            Expr::EmptyArray(t) => format!("(empty-array {})", t),
            Expr::Index(arr, idx) => format!(
                "(index {} {})",
                arr.to_prefix_string(),
                idx.to_prefix_string()
            ),
        }
    }

    /// Evaluates a binary arithmetic op where at least one operand is a matrix.
    ///
    /// Handles `+`, `-` (component-wise), `*` (matrix product or matrix×vector).
    /// Integer/float promotion mirrors the vector rules — any Mat3 involvement
    /// upcasts IMat3 inputs to Mat3; IVec3 upcasts to Vec3 when multiplied by Mat3.
    /// See doc/design_matrix_types.md D7.
    fn matrix_arith_op(left: NetworkResult, right: NetworkResult, op: BinOp) -> NetworkResult {
        use crate::structure_designer::evaluator::network_result::imat3_rows_to_dmat3;
        use glam::f64::{DMat3, DVec3};

        // Component-wise add/sub for matrices.
        let imat3_component = |a: [[i32; 3]; 3], b: [[i32; 3]; 3], f: fn(i32, i32) -> i32| {
            let mut r = [[0i32; 3]; 3];
            for i in 0..3 {
                for j in 0..3 {
                    r[i][j] = f(a[i][j], b[i][j]);
                }
            }
            r
        };

        match (left, right, op) {
            // IMat3 op IMat3 — integer-preserving for +, -, *.
            (NetworkResult::IMat3(a), NetworkResult::IMat3(b), BinOp::Add) => {
                NetworkResult::IMat3(imat3_component(a, b, |x, y| x + y))
            }
            (NetworkResult::IMat3(a), NetworkResult::IMat3(b), BinOp::Sub) => {
                NetworkResult::IMat3(imat3_component(a, b, |x, y| x - y))
            }
            (NetworkResult::IMat3(a), NetworkResult::IMat3(b), BinOp::Mul) => {
                let mut r = [[0i32; 3]; 3];
                for i in 0..3 {
                    for k in 0..3 {
                        r[i][k] = (0..3).map(|j| a[i][j] * b[j][k]).sum();
                    }
                }
                NetworkResult::IMat3(r)
            }

            // IMat3 × IVec3 (integer-preserving matrix-vector).
            (NetworkResult::IMat3(m), NetworkResult::IVec3(v), BinOp::Mul) => {
                let mut r = [0i32; 3];
                for i in 0..3 {
                    r[i] = m[i][0] * v.x + m[i][1] * v.y + m[i][2] * v.z;
                }
                NetworkResult::IVec3(glam::i32::IVec3::new(r[0], r[1], r[2]))
            }

            // Mat3 op Mat3.
            (NetworkResult::Mat3(a), NetworkResult::Mat3(b), BinOp::Add) => {
                NetworkResult::Mat3(a + b)
            }
            (NetworkResult::Mat3(a), NetworkResult::Mat3(b), BinOp::Sub) => {
                NetworkResult::Mat3(a - b)
            }
            // glam's DMat3 * DMat3 matches our row-major semantics — see
            // doc/design_matrix_types.md §"Row-major API over column-major storage".
            (NetworkResult::Mat3(a), NetworkResult::Mat3(b), BinOp::Mul) => {
                NetworkResult::Mat3(a.mul_mat3(&b))
            }

            // Mat3 × Vec3.
            (NetworkResult::Mat3(m), NetworkResult::Vec3(v), BinOp::Mul) => {
                NetworkResult::Vec3(m.mul_vec3(v))
            }

            // Mixed Mat3/IMat3: promote IMat3 → Mat3 and recurse.
            (NetworkResult::IMat3(a), NetworkResult::Mat3(b), op) => Self::matrix_arith_op(
                NetworkResult::Mat3(imat3_rows_to_dmat3(&a)),
                NetworkResult::Mat3(b),
                op,
            ),
            (NetworkResult::Mat3(a), NetworkResult::IMat3(b), op) => Self::matrix_arith_op(
                NetworkResult::Mat3(a),
                NetworkResult::Mat3(imat3_rows_to_dmat3(&b)),
                op,
            ),

            // Mat3 × IVec3: promote IVec3 → Vec3.
            (NetworkResult::Mat3(m), NetworkResult::IVec3(v), BinOp::Mul) => {
                NetworkResult::Vec3(m.mul_vec3(DVec3::new(v.x as f64, v.y as f64, v.z as f64)))
            }
            // IMat3 × Vec3: promote IMat3 → Mat3.
            (NetworkResult::IMat3(m), NetworkResult::Vec3(v), BinOp::Mul) => {
                let mat: DMat3 = imat3_rows_to_dmat3(&m);
                NetworkResult::Vec3(mat.mul_vec3(v))
            }

            _ => NetworkResult::Error(
                "Matrix arithmetic operation not supported for these types".to_string(),
            ),
        }
    }
}

/// Unifies two element types for an array literal under the same promotion rules
/// used by conditional-branch unification and arithmetic. Returns the unified
/// type or `Err(())` if no common type exists.
///
/// Rules:
/// - Identical types unify to themselves.
/// - Int + Float -> Float; IVec2 + Vec2 -> Vec2; IVec3 + Vec3 -> Vec3;
///   IMat3 + Mat3 -> Mat3; Bool + Int -> Int.
/// - Array[T1] + Array[T2] -> Array[unify(T1, T2)] (recursive).
/// - Anything else: error.
pub(crate) fn unify_array_element_types(a: &DataType, b: &DataType) -> Result<DataType, ()> {
    if a == b {
        return Ok(a.clone());
    }
    match (a, b) {
        (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => Ok(DataType::Float),
        (DataType::IVec2, DataType::Vec2) | (DataType::Vec2, DataType::IVec2) => Ok(DataType::Vec2),
        (DataType::IVec3, DataType::Vec3) | (DataType::Vec3, DataType::IVec3) => Ok(DataType::Vec3),
        (DataType::IMat3, DataType::Mat3) | (DataType::Mat3, DataType::IMat3) => Ok(DataType::Mat3),
        (DataType::Bool, DataType::Int) | (DataType::Int, DataType::Bool) => Ok(DataType::Int),
        (DataType::Array(inner_a), DataType::Array(inner_b)) => {
            let inner = unify_array_element_types(inner_a, inner_b)?;
            Ok(DataType::Array(Box::new(inner)))
        }
        _ => Err(()),
    }
}

/// Returns true if `member` is a valid matrix accessor name of the form `m<i><j>`
/// where `i` and `j` are single digits 0..=2 (e.g., `m00`, `m12`).
fn is_matrix_accessor(member: &str) -> bool {
    parse_matrix_accessor(member).is_some()
}

/// Parses a matrix accessor name `m<i><j>` into `(i, j)` with `i, j` in 0..=2.
/// Returns `None` for anything else (e.g., `m3`, `m33`, `mxx`, `row`).
fn parse_matrix_accessor(member: &str) -> Option<(usize, usize)> {
    let bytes = member.as_bytes();
    if bytes.len() != 3 || bytes[0] != b'm' {
        return None;
    }
    let i = (bytes[1] as char).to_digit(10)? as usize;
    let j = (bytes[2] as char).to_digit(10)? as usize;
    if i <= 2 && j <= 2 { Some((i, j)) } else { None }
}
